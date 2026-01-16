//! Document model with rope-based storage

mod block;
mod paragraph;
mod rope;

pub use block::{BlockKind, BlockMeta, ListId, ListMarker};
pub use paragraph::{ParagraphId, ParagraphIndex};
pub use rope::Rope;

use crate::editing::{AbsoluteOffset, DocPosition, EditOp, EditResult};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

/// The main document structure
#[derive(Debug)]
pub struct Document {
    /// Rope storing the full text content
    content: Rope,
    /// Block-level metadata indexed by paragraph ID
    blocks: FxHashMap<ParagraphId, BlockMeta>,
    /// Paragraph index for fast lookups
    paragraph_index: ParagraphIndex,
    /// Monotonic version counter
    version: u64,
    /// Next paragraph ID to assign
    next_para_id: u64,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// Create a new empty document
    pub fn new() -> Self {
        let first_para = ParagraphId(0);
        let mut blocks = FxHashMap::default();
        blocks.insert(
            first_para,
            BlockMeta {
                kind: BlockKind::Paragraph,
                start_offset: 0,
                byte_len: 0,
            },
        );

        let mut paragraph_index = ParagraphIndex::new();
        paragraph_index.insert(first_para, 0, 0);

        Self {
            content: Rope::new(),
            blocks,
            paragraph_index,
            version: 0,
            next_para_id: 1,
        }
    }

    /// Create a document from initial text
    pub fn from_text(text: &str) -> Self {
        let mut doc = Self {
            content: Rope::new(),
            blocks: FxHashMap::default(),
            paragraph_index: ParagraphIndex::new(),
            version: 0,
            next_para_id: 0,
        };

        // Parse paragraphs (split by double newline or single newline for simplicity)
        let mut offset = 0;
        for para_text in text.split('\n') {
            let para_id = ParagraphId(doc.next_para_id);
            doc.next_para_id += 1;

            let para_len = para_text.len();
            doc.blocks.insert(
                para_id,
                BlockMeta {
                    kind: BlockKind::Paragraph,
                    start_offset: offset,
                    byte_len: para_len,
                },
            );
            doc.paragraph_index.insert(para_id, offset, para_len);
            offset += para_len + 1; // +1 for the newline
        }

        doc.content = Rope::from_str(text);

        // Ensure at least one paragraph exists
        if doc.blocks.is_empty() {
            let para_id = ParagraphId(doc.next_para_id);
            doc.next_para_id += 1;
            doc.blocks.insert(
                para_id,
                BlockMeta {
                    kind: BlockKind::Paragraph,
                    start_offset: 0,
                    byte_len: 0,
                },
            );
            doc.paragraph_index.insert(para_id, 0, 0);
        }

        doc
    }

    /// Get the document version
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Get total text length
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Check if document is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Get the full document text
    pub fn text(&self) -> String {
        self.content.to_string()
    }

    /// Get text for a specific paragraph
    pub fn paragraph_text(&self, para_id: ParagraphId) -> String {
        if let Some(meta) = self.blocks.get(&para_id) {
            self.content
                .slice(meta.start_offset, meta.start_offset + meta.byte_len)
        } else {
            String::new()
        }
    }

    /// Get block metadata for a paragraph
    pub fn block_meta(&self, para_id: ParagraphId) -> Option<&BlockMeta> {
        self.blocks.get(&para_id)
    }

    /// Get the first paragraph ID
    pub fn first_paragraph(&self) -> ParagraphId {
        self.paragraph_index.first()
    }

    /// Get paragraph order for iteration
    pub fn paragraph_order(&self) -> impl Iterator<Item = ParagraphId> + '_ {
        self.paragraph_index.iter()
    }

    /// Get paragraph count
    pub fn paragraph_count(&self) -> usize {
        self.paragraph_index.len()
    }

    /// Convert DocPosition to AbsoluteOffset
    pub fn position_to_offset(&self, pos: &DocPosition) -> AbsoluteOffset {
        if let Some(meta) = self.blocks.get(&pos.para_id) {
            AbsoluteOffset(meta.start_offset + pos.offset)
        } else {
            AbsoluteOffset(0)
        }
    }

    /// Convert AbsoluteOffset to DocPosition
    pub fn offset_to_position(&self, offset: AbsoluteOffset) -> DocPosition {
        let (para_id, para_start) = self.paragraph_index.para_at_offset(offset.0);
        DocPosition {
            para_id,
            offset: offset.0.saturating_sub(para_start),
        }
    }

    /// Get the paragraph containing an offset
    pub fn para_at_offset(&self, offset: usize) -> ParagraphId {
        self.paragraph_index.para_at_offset(offset).0
    }

    /// Get text range
    pub fn text_range(&self, range: std::ops::Range<usize>) -> String {
        self.content.slice(range.start, range.end)
    }

    /// Get next grapheme cluster offset
    pub fn next_grapheme_offset(&self, offset: AbsoluteOffset) -> AbsoluteOffset {
        use unicode_segmentation::UnicodeSegmentation;

        let text = self.content.to_string();
        let mut graphemes = text.grapheme_indices(true);

        // Find current grapheme
        for (idx, grapheme) in graphemes.by_ref() {
            if idx >= offset.0 {
                return AbsoluteOffset(idx + grapheme.len());
            }
        }

        AbsoluteOffset(text.len())
    }

    /// Get previous grapheme cluster offset
    pub fn prev_grapheme_offset(&self, offset: AbsoluteOffset) -> AbsoluteOffset {
        use unicode_segmentation::UnicodeSegmentation;

        if offset.0 == 0 {
            return AbsoluteOffset(0);
        }

        let text = self.content.to_string();
        let graphemes: Vec<_> = text.grapheme_indices(true).collect();

        // Find previous grapheme
        for (idx, _) in graphemes.iter().rev() {
            if *idx < offset.0 {
                return AbsoluteOffset(*idx);
            }
        }

        AbsoluteOffset(0)
    }

    /// Apply an edit operation
    pub fn apply_edit(&mut self, op: EditOp) -> EditResult {
        self.version += 1;

        match op {
            EditOp::Insert { position, text } => self.apply_insert(position, &text),
            EditOp::Delete { start, end } => self.apply_delete(start, end),
            EditOp::Transaction { ops } => {
                let mut result = EditResult {
                    version: self.version,
                    affected_paragraphs: SmallVec::new(),
                    created_paragraphs: SmallVec::new(),
                    deleted_paragraphs: SmallVec::new(),
                    new_cursor: DocPosition::default(),
                };

                for op in ops {
                    let sub_result = self.apply_edit(op);
                    result.affected_paragraphs.extend(sub_result.affected_paragraphs);
                    result.created_paragraphs.extend(sub_result.created_paragraphs);
                    result.deleted_paragraphs.extend(sub_result.deleted_paragraphs);
                    result.new_cursor = sub_result.new_cursor;
                }

                result
            }
        }
    }

    /// Apply an insert operation
    fn apply_insert(&mut self, position: AbsoluteOffset, text: &str) -> EditResult {
        let mut affected = SmallVec::new();
        let mut created = SmallVec::new();

        // Find affected paragraph
        let (para_id, para_start) = self.paragraph_index.para_at_offset(position.0);
        affected.push(para_id);

        // Insert into rope
        self.content.insert(position.0, text);

        // Check for new paragraph boundaries
        let newline_positions: Vec<_> = text
            .char_indices()
            .filter(|(_, c)| *c == '\n')
            .map(|(i, _)| i)
            .collect();

        if newline_positions.is_empty() {
            // No new paragraphs, just update the current one
            if let Some(meta) = self.blocks.get_mut(&para_id) {
                meta.byte_len += text.len();
            }
            self.paragraph_index.update_lengths_after(position.0, text.len() as isize);
        } else {
            // Split paragraph at newlines
            let original_meta = self.blocks.get(&para_id).cloned();

            if let Some(meta) = original_meta {
                let offset_in_para = position.0.saturating_sub(para_start);
                let original_byte_len = meta.byte_len;
                let mut current_start = meta.start_offset;

                // First segment stays in original paragraph
                let first_len = offset_in_para + newline_positions[0];
                if let Some(m) = self.blocks.get_mut(&para_id) {
                    m.byte_len = first_len;
                }
                self.paragraph_index.update_length(para_id, first_len);
                current_start += first_len + 1; // +1 for newline

                // Create new paragraphs for each segment
                for (i, &nl_pos) in newline_positions.iter().enumerate() {
                    let next_end = if i + 1 < newline_positions.len() {
                        newline_positions[i + 1]
                    } else {
                        // Last segment: remaining text after last newline
                        text.len() + original_byte_len.saturating_sub(offset_in_para)
                    };

                    let segment_len = next_end.saturating_sub(nl_pos).saturating_sub(1);
                    let new_para = ParagraphId(self.next_para_id);
                    self.next_para_id += 1;

                    self.blocks.insert(
                        new_para,
                        BlockMeta {
                            kind: BlockKind::Paragraph,
                            start_offset: current_start,
                            byte_len: segment_len,
                        },
                    );
                    self.paragraph_index.insert_after(para_id, new_para, current_start, segment_len);
                    created.push(new_para);

                    current_start += segment_len + 1;
                }
            }
        }

        // Shift offsets for paragraphs after insertion point
        self.shift_block_offsets_after(position.0, text.len() as isize);

        let new_offset = AbsoluteOffset(position.0 + text.len());
        let new_cursor = self.offset_to_position(new_offset);

        EditResult {
            version: self.version,
            affected_paragraphs: affected,
            created_paragraphs: created,
            deleted_paragraphs: SmallVec::new(),
            new_cursor,
        }
    }

    /// Apply a delete operation
    fn apply_delete(&mut self, start: AbsoluteOffset, end: AbsoluteOffset) -> EditResult {
        let mut affected = SmallVec::new();
        let mut deleted = SmallVec::new();

        if start.0 >= end.0 {
            return EditResult {
                version: self.version,
                affected_paragraphs: affected,
                created_paragraphs: SmallVec::new(),
                deleted_paragraphs: deleted,
                new_cursor: self.offset_to_position(start),
            };
        }

        // Find affected paragraphs
        let (start_para, start_para_offset) = self.paragraph_index.para_at_offset(start.0);
        let (end_para, _) = self.paragraph_index.para_at_offset(end.0.saturating_sub(1));

        affected.push(start_para);

        // Delete from rope
        self.content.delete(start.0, end.0);

        let delete_len = end.0 - start.0;

        if start_para == end_para {
            // Single paragraph affected
            if let Some(meta) = self.blocks.get_mut(&start_para) {
                meta.byte_len = meta.byte_len.saturating_sub(delete_len);
            }
            self.paragraph_index.update_lengths_after(start.0, -(delete_len as isize));
        } else {
            // Multiple paragraphs: merge first and last, delete middle ones
            let mut paras_to_check: Vec<_> = self.paragraph_index.iter().collect();
            let mut found_start = false;
            let mut in_range = false;

            for para_id in paras_to_check {
                if para_id == start_para {
                    found_start = true;
                    in_range = true;
                    continue;
                }

                if in_range {
                    if para_id == end_para {
                        // Merge end para content into start para
                        if let Some(end_meta) = self.blocks.get(&end_para) {
                            let offset_in_end = end.0.saturating_sub(end_meta.start_offset);
                            let remaining_in_end = end_meta.byte_len.saturating_sub(offset_in_end);
                            if let Some(start_meta) = self.blocks.get_mut(&start_para) {
                                let kept_in_start = start.0.saturating_sub(start_para_offset);
                                start_meta.byte_len = kept_in_start + remaining_in_end;
                            }
                        }
                        self.blocks.remove(&end_para);
                        self.paragraph_index.remove(end_para);
                        deleted.push(end_para);
                        in_range = false;
                    } else {
                        // Delete middle paragraph entirely
                        self.blocks.remove(&para_id);
                        self.paragraph_index.remove(para_id);
                        deleted.push(para_id);
                    }
                }
            }
        }

        // Shift offsets for paragraphs after deletion
        self.shift_block_offsets_after(start.0, -(delete_len as isize));

        EditResult {
            version: self.version,
            affected_paragraphs: affected,
            created_paragraphs: SmallVec::new(),
            deleted_paragraphs: deleted,
            new_cursor: self.offset_to_position(start),
        }
    }

    /// Shift block offsets after a position
    fn shift_block_offsets_after(&mut self, after_offset: usize, delta: isize) {
        for (_, meta) in self.blocks.iter_mut() {
            if meta.start_offset > after_offset {
                meta.start_offset = (meta.start_offset as isize + delta) as usize;
            }
        }
    }

    /// Compute the reverse operation for undo
    pub fn compute_reverse(&self, op: &EditOp) -> EditOp {
        match op {
            EditOp::Insert { position, text } => EditOp::Delete {
                start: *position,
                end: AbsoluteOffset(position.0 + text.len()),
            },
            EditOp::Delete { start, end } => {
                let deleted_text = self.text_range(start.0..end.0);
                EditOp::Insert {
                    position: *start,
                    text: deleted_text,
                }
            }
            EditOp::Transaction { ops } => EditOp::Transaction {
                ops: ops.iter().rev().map(|op| self.compute_reverse(op)).collect(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_document() {
        let doc = Document::new();
        assert_eq!(doc.len(), 0);
        assert!(doc.is_empty());
    }

    #[test]
    fn test_from_text() {
        let doc = Document::from_text("Hello\nWorld");
        assert_eq!(doc.text(), "Hello\nWorld");
        assert_eq!(doc.paragraph_count(), 2);
    }

    #[test]
    fn test_insert() {
        let mut doc = Document::new();
        let result = doc.apply_edit(EditOp::Insert {
            position: AbsoluteOffset(0),
            text: "Hello".to_string(),
        });
        assert_eq!(doc.text(), "Hello");
        assert_eq!(result.affected_paragraphs.len(), 1);
    }

    #[test]
    fn test_delete() {
        let mut doc = Document::from_text("Hello World");
        doc.apply_edit(EditOp::Delete {
            start: AbsoluteOffset(5),
            end: AbsoluteOffset(11),
        });
        assert_eq!(doc.text(), "Hello");
    }
}
