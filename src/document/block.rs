//! Block-level element metadata

/// Unique identifier for a list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListId(pub u64);

/// Type of list marker
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMarker {
    Bullet,
    Numbered { ordinal: u32 },
}

impl ListMarker {
    /// Get the display string for this marker
    pub fn display(&self) -> String {
        match self {
            ListMarker::Bullet => "•".to_string(),
            ListMarker::Numbered { ordinal } => format!("{}.", ordinal),
        }
    }
}

/// The kind of block element
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    /// Regular paragraph
    Paragraph,
    /// Heading with level (1-6)
    Heading { level: u8 },
    /// List item
    ListItem {
        list_id: ListId,
        indent_level: u8,
        marker: ListMarker,
    },
}

impl Default for BlockKind {
    fn default() -> Self {
        BlockKind::Paragraph
    }
}

impl BlockKind {
    /// Get the line height multiplier for this block kind
    pub fn line_height_multiplier(&self) -> f32 {
        match self {
            BlockKind::Paragraph => 1.0,
            BlockKind::Heading { level } => match level {
                1 => 1.5,
                2 => 1.4,
                3 => 1.3,
                _ => 1.2,
            },
            BlockKind::ListItem { .. } => 1.0,
        }
    }

    /// Get the spacing after this block (in line heights)
    pub fn spacing_after(&self) -> f32 {
        match self {
            BlockKind::Paragraph => 1.0,
            BlockKind::Heading { .. } => 0.5,
            BlockKind::ListItem { .. } => 0.25,
        }
    }

    /// Check if this is a heading
    pub fn is_heading(&self) -> bool {
        matches!(self, BlockKind::Heading { .. })
    }

    /// Check if this is a list item
    pub fn is_list_item(&self) -> bool {
        matches!(self, BlockKind::ListItem { .. })
    }
}

/// Style information for a span of text
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleSpan {
    /// Start offset relative to block start
    pub start: usize,
    /// End offset relative to block start
    pub end: usize,
    /// Font ID to use
    pub font_id: crate::layout::font::FontId,
}

/// Metadata for a block-level element
#[derive(Debug, Clone, PartialEq)]
pub struct BlockMeta {
    /// The kind of block
    pub kind: BlockKind,
    /// Byte offset where this block starts in the document
    pub start_offset: usize,
    /// Length of this block in bytes
    pub byte_len: usize,
    /// Style spans for this block (sorted by start)
    pub styles: Vec<StyleSpan>,
}

impl BlockMeta {
    /// Create a new paragraph block
    pub fn paragraph(start_offset: usize, byte_len: usize) -> Self {
        Self {
            kind: BlockKind::Paragraph,
            start_offset,
            byte_len,
            styles: Vec::new(),
        }
    }

    /// Create a new heading block
    pub fn heading(level: u8, start_offset: usize, byte_len: usize) -> Self {
        Self {
            kind: BlockKind::Heading { level: level.min(6).max(1) },
            start_offset,
            byte_len,
            styles: Vec::new(),
        }
    }

    /// Create a new list item block
    pub fn list_item(
        list_id: ListId,
        indent_level: u8,
        marker: ListMarker,
        start_offset: usize,
        byte_len: usize,
    ) -> Self {
        Self {
            kind: BlockKind::ListItem {
                list_id,
                indent_level,
                marker,
            },
            start_offset,
            byte_len,
            styles: Vec::new(),
        }
    }

    /// Get the end offset of this block
    pub fn end_offset(&self) -> usize {
        self.start_offset + self.byte_len
    }

    /// Handle text insertion at the given relative offset
    pub fn on_insert(&mut self, offset: usize, len: usize) {
        if self.styles.is_empty() {
             // If no styles, everything stays default. 
             // Ideally we should have a default style? 
             // For now, empty list means "default font".
             return;
        }

        let mut extended = false;
        for style in &mut self.styles {
            // Valid insertion point: inside span or at the end of span
            // If inserting at 0, only extend if it's the very first span? 
            // Standard behavior: inherit from previous char. 
            // If offset > style.start && offset <= style.end: extend
            
            if offset > style.start && offset <= style.end {
                style.end += len;
                extended = true;
            } else if offset <= style.start {
                // Determine if we should push this span forward
                style.start += len;
                style.end += len;
            }
        }
    }

    /// Handle text deletion
    pub fn on_delete(&mut self, start: usize, end: usize) {
        if self.styles.is_empty() {
            return;
        }
        
        let delete_len = end - start;
        // Filter map to remove/adjust spans
        self.styles = self.styles.iter().filter_map(|s| {
            let mut style = s.clone();
            
            if style.end <= start {
                // Before deletion, unchanged
                Some(style)
            } else if style.start >= end {
                // After deletion, shift back
                style.start -= delete_len;
                style.end -= delete_len;
                Some(style)
            } else {
                // Overlaps with deletion
                // Clip start
                if style.start < start {
                    // Force end to be at start of deletion (it effectively truncates)
                    // The part after deletion is handled by "After deletion" logic?
                    // No. If a span covers the deletion, it shrinks.
                    // Span: [0, 10), Delete: [2, 5). Result: [0, 7).
                    // style.start (0) < start (2). style.end (10) > end (5).
                    style.end -= delete_len;
                    Some(style)
                } else {
                    // Span starts inside deletion.
                    // Span: [3, 8), Delete: [2, 5). Result: [2, 5) (shifted 2). 
                    // New start = start of deletion = 2? No.
                    // The text from [3, 5) is gone. Text from [5, 8) shifts to [2, 5).
                    // So style.start becomes start (2).
                    // style.end becomes 8 - 3 = 5.
                    style.start = start;
                    style.end -= delete_len;
                    
                    if style.start >= style.end {
                        None
                    } else {
                        Some(style)
                    }
                }
            }
        }).collect();
    }

    /// Split styles at a relative offset, returning styles for the new second block
    pub fn split_styles_at(&mut self, split_offset: usize) -> Vec<StyleSpan> {
        let mut second_half_styles = Vec::new();
        
        // Truncate current styles and collect second half
        self.styles = self.styles.iter().filter_map(|s| {
            if s.end <= split_offset {
                // Fully in first half
                Some(s.clone())
            } else if s.start >= split_offset {
                // Fully in second half
                let mut new_s = s.clone();
                new_s.start -= split_offset;
                new_s.end -= split_offset;
                second_half_styles.push(new_s);
                None
            } else {
                // Split across
                // First half
                let mut first = s.clone();
                first.end = split_offset;
                
                // Second half
                let mut second = s.clone();
                second.start = 0;
                second.end = s.end - split_offset;
                second.font_id = s.font_id;
                second_half_styles.push(second);
                
                Some(first)
            }
        }).collect();
        
        second_half_styles
    }

    /// Apply formatting to a range
    pub fn format_range(&mut self, start: usize, end: usize, font_id: crate::layout::font::FontId) {
        if start >= end { return; }

        // Remove existing styles in range
        let mut new_styles = Vec::new();
        let mut text_covered_start = 0;

        // If styles empty, assume whole block was default. 
        // If we format [5, 10), and length is 20.
        // We have [0, 5) default (implicit), [5, 10) new, [10, 20) default.
        // But we don't track default. 
        // We need to be careful: "empty styles" means "all default".
        // If we add one style key, does the rest remain default (implicit)?
        // Yes, Layout engine should handle "gaps" as default font.
        
        // Naive implementation: just add the span and handle overlaps by "punching holes"?
        // Better: flatten styles. 
        // Since we don't enforce coverage, we can just remove overlapping parts of existing styles
        // and add the new one.
        
        let input_styles = std::mem::take(&mut self.styles);
        
        for s in input_styles {
            if s.end <= start || s.start >= end {
                // Disjoint
                new_styles.push(s);
            } else {
                // Overlap
                if s.start < start {
                    // Keep prefix
                    new_styles.push(StyleSpan {
                        start: s.start,
                        end: start,
                        font_id: s.font_id,
                    });
                }
                
                if s.end > end {
                    // Keep suffix
                    new_styles.push(StyleSpan {
                        start: end,
                        end: s.end,
                        font_id: s.font_id,
                    });
                }
            }
        }
        
        // Insert new style and sort
        new_styles.push(StyleSpan {
            start,
            end,
            font_id,
        });
        
        new_styles.sort_by_key(|s| s.start);
        
        // Merge adjacent identical styles
        let mut merged: Vec<StyleSpan> = Vec::new();
        for s in new_styles {
            if let Some(last) = merged.last_mut() {
                if last.end == s.start && last.font_id == s.font_id {
                    last.end = s.end;
                    continue;
                }
            }
            merged.push(s);
        }
        
        self.styles = merged;
    }

    /// Append styles from another block (used when merging paragraphs)
    pub fn append_styles(&mut self, mut other_styles: Vec<StyleSpan>, offset_shift: usize) {
        for style in &mut other_styles {
            style.start += offset_shift;
            style.end += offset_shift;
        }
        self.styles.extend(other_styles);
        
        // Optimize: merge adjacent spans if possible
        // (Optional, but good for cleanliness)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_kind() {
        let para = BlockKind::Paragraph;
        assert!(!para.is_heading());
        assert!(!para.is_list_item());

        let heading = BlockKind::Heading { level: 1 };
        assert!(heading.is_heading());

        let list = BlockKind::ListItem {
            list_id: ListId(0),
            indent_level: 0,
            marker: ListMarker::Bullet,
        };
        assert!(list.is_list_item());
    }

    #[test]
    fn test_list_marker_display() {
        assert_eq!(ListMarker::Bullet.display(), "•");
        assert_eq!(ListMarker::Numbered { ordinal: 1 }.display(), "1.");
        assert_eq!(ListMarker::Numbered { ordinal: 10 }.display(), "10.");
    }
}
