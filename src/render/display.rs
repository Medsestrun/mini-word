//! Display list: render-ready representation

use crate::document::{BlockKind, Document, ListMarker, ParagraphId};
use crate::editing::{Cursor, Selection};
use crate::layout::{LayoutState, INDENT_WIDTH};
use crate::{Point, Rect};

/// Unique identifier for a display item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayItemId {
    pub para_id: ParagraphId,
    pub line_index: u32,
    pub run_index: u32,
}

impl DisplayItemId {
    pub fn new(para_id: ParagraphId, line_index: usize, run_index: usize) -> Self {
        Self {
            para_id,
            line_index: line_index as u32,
            run_index: run_index as u32,
        }
    }
}

/// Display representation of a list marker
#[derive(Debug, Clone, PartialEq)]
pub enum ListMarkerDisplay {
    Bullet,
    Number(String),
}

impl From<&ListMarker> for ListMarkerDisplay {
    fn from(marker: &ListMarker) -> Self {
        match marker {
            ListMarker::Bullet => ListMarkerDisplay::Bullet,
            ListMarker::Numbered { ordinal } => {
                ListMarkerDisplay::Number(format!("{}.", ordinal))
            }
        }
    }
}

/// A display item to render
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    /// Text run
    TextRun {
        id: DisplayItemId,
        position: Point,
        text: String,
        block_kind: BlockKind,
        /// UTF-16 code unit range (start, end) relative to line text
        selection_range: Option<(usize, usize)>,
        /// Style spans (start, len, font_id) relative to line text (in bytes)
        styles: Vec<(usize, usize, u32)>,
    },
    /// List marker (bullet or number)
    ListMarker {
        id: DisplayItemId,
        position: Point,
        marker: ListMarkerDisplay,
    },
    /// Cursor caret
    Caret {
        position: Point,
        height: f32,
        /// UTF-16 code unit offset within the line (for correct JS text measurement)
        utf16_offset_in_line: usize,
    },
    /// Page break indicator
    PageBreak {
        y: f32,
        page_number: usize,
    },
}

impl DisplayItem {
    /// Get the ID of this item, if it has one
    pub fn id(&self) -> Option<DisplayItemId> {
        match self {
            DisplayItem::TextRun { id, .. } => Some(*id),
            DisplayItem::ListMarker { id, .. } => Some(*id),
            _ => None,
        }
    }
}

/// Display list for a single page
#[derive(Debug, Clone, PartialEq)]
pub struct DisplayPage {
    pub page_index: usize,
    pub bounds: Rect,
    pub items: Vec<DisplayItem>,
}

/// Complete display list for rendering
#[derive(Debug, Clone)]
pub struct DisplayList {
    pub version: u64,
    pub pages: Vec<DisplayPage>,
}

impl DisplayList {
    /// Build display list from layout state
    pub fn build(
        document: &Document,
        layout: &LayoutState,
        viewport: Rect,
        cursor: &Cursor,
        selection: Option<&Selection>,
    ) -> Self {
        let constraints = layout.constraints();
        let mut pages = Vec::new();

        // Calculate which pages are visible
        let page_height = constraints.page_height;
        let first_visible_page = (viewport.y / page_height).floor() as usize;
        let last_visible_page = ((viewport.y + viewport.height) / page_height).ceil() as usize;

        for (page_idx, page_layout) in layout.pages().iter().enumerate() {
            // Skip pages outside viewport
            if page_idx < first_visible_page || page_idx > last_visible_page {
                continue;
            }

            let page_y_offset = page_idx as f32 * page_height;
            let mut items = Vec::new();
            let mut y = constraints.margin_top;

            // Iterate through paragraphs on this page
            let mut in_page = false;
            for para_id in document.paragraph_order() {
                if para_id == page_layout.start_para {
                    in_page = true;
                }

                if !in_page {
                    continue;
                }

                if let Some(para_layout) = layout.paragraph_layout(para_id) {
                    let block_meta = document.block_meta(para_id);
                    let block_kind = block_meta
                        .map(|m| m.kind.clone())
                        .unwrap_or(BlockKind::Paragraph);

                    let para_text = document.paragraph_text(para_id);
                    let indent = layout.indent_for(
                        block_meta.unwrap_or(&crate::document::BlockMeta {
                            kind: BlockKind::Paragraph,
                            start_offset: 0,
                            byte_len: 0,
                            styles: Vec::new(),
                        })
                    );

                    // Determine line range for this page
                    let start_line = if para_id == page_layout.start_para {
                        page_layout.start_line
                    } else {
                        0
                    };

                    let end_line = if para_id == page_layout.end_para {
                        page_layout.end_line + 1
                    } else {
                        para_layout.lines.len()
                    };

                    for line_idx in start_line..end_line.min(para_layout.lines.len()) {
                        let line = &para_layout.lines[line_idx];

                        // Emit list marker on first line
                        if line_idx == 0 {
                            if let BlockKind::ListItem { marker, indent_level, .. } = &block_kind {
                                let marker_x = constraints.margin_left 
                                    + (*indent_level as f32 * INDENT_WIDTH)
                                    - 16.0; // Marker width
                                
                                items.push(DisplayItem::ListMarker {
                                    id: DisplayItemId::new(para_id, 0, 0),
                                    position: Point { x: marker_x, y },
                                    marker: marker.into(),
                                });
                            }
                        }

                        // Extract line text
                        let line_text = if line.byte_range.end <= para_text.len() {
                            para_text[line.byte_range.clone()].to_string()
                        } else {
                            String::new()
                        };

                        // Selection range for this line
                        let selection_range = selection.and_then(|sel| {
                            if !sel.is_collapsed() {
                                Self::selection_range_for_line(
                                    document,
                                    para_id,
                                    line,
                                    sel,
                                    &line_text,
                                )
                            } else {
                                None
                            }
                        });

                        // Calculate styles for this line
                        // Line byte range is relative to paragraph start
                        // Styles in block_meta are relative to paragraph start
                        // We need to output styles relative to line start
                        let line_styles = if let Some(meta) = block_meta {
                            meta.styles.iter()
                                .filter_map(|s| {
                                    // Intersect [s.start, s.end) with [line.byte_range.start, line.byte_range.end)
                                    let start = s.start.max(line.byte_range.start);
                                    let end = s.end.min(line.byte_range.end);
                                    
                                    if start < end {
                                        Some((
                                            start - line.byte_range.start,
                                            end - start,
                                            s.font_id.0
                                        ))
                                    } else {
                                        None
                                    }
                                })
                                .collect()
                        } else {
                            Vec::new()
                        };

                        // Text run
                        items.push(DisplayItem::TextRun {
                            id: DisplayItemId::new(para_id, line_idx, 0),
                            position: Point {
                                x: constraints.margin_left + indent,
                                y,
                            },
                            text: line_text,
                            block_kind: block_kind.clone(),
                            selection_range,
                            styles: line_styles,
                        });

                        y += line.height;
                    }
                }

                if para_id == page_layout.end_para {
                    break;
                }
            }

            // Cursor
            if let Some((caret_pos, utf16_offset)) = Self::cursor_position(
                document,
                layout,
                cursor,
                page_layout,
                constraints,
            ) {
                items.push(DisplayItem::Caret {
                    position: caret_pos,
                    height: layout.font_library.get(crate::layout::font::FontId(0)).map(|m| m.line_height).unwrap_or(16.0),
                    utf16_offset_in_line: utf16_offset,
                });
            }

            pages.push(DisplayPage {
                page_index: page_idx,
                bounds: Rect {
                    x: 0.0,
                    y: page_y_offset,
                    width: constraints.page_width,
                    height: constraints.page_height,
                },
                items,
            });
        }

        DisplayList {
            version: document.version(),
            pages,
        }
    }

    /// Calculate cursor position on page
    /// Returns (Point, utf16_offset) where utf16_offset is the UTF-16 code unit offset within the line
    /// (UTF-16 offsets are used for correct text measurement in JavaScript)
    fn cursor_position(
        document: &Document,
        layout: &LayoutState,
        cursor: &Cursor,
        page: &crate::layout::PageLayout,
        constraints: &crate::layout::LayoutConstraints,
    ) -> Option<(Point, usize)> {
        // Check if cursor is on this page
        if cursor.position.para_id < page.start_para 
            || cursor.position.para_id > page.end_para 
        {
            return None;
        }

        let para_layout = layout.paragraph_layout(cursor.position.para_id)?;
        let (line_idx, line) = para_layout.line_at_offset(cursor.position.offset)?;

        // Check line is on this page
        if cursor.position.para_id == page.start_para && line_idx < page.start_line {
            return None;
        }
        if cursor.position.para_id == page.end_para && line_idx > page.end_line {
            return None;
        }

        // Calculate UTF-16 code unit offset within line
        // Get paragraph text and extract the portion from line start to cursor
        let para_text = document.paragraph_text(cursor.position.para_id);
        let line_start_byte = line.byte_range.start;
        let cursor_byte = cursor.position.offset;
        
        // Get text from line start to cursor position (both are byte offsets within paragraph)
        let text_before_cursor = para_text
            .get(line_start_byte..cursor_byte)
            .unwrap_or("");
        
        // Convert to UTF-16 code units for JS
        let utf16_offset = text_before_cursor.chars().map(|c| c.len_utf16()).sum::<usize>();

        // Calculate Y position
        let mut y = constraints.margin_top;
        
        // Add height of previous paragraphs on this page
        for para_id in document.paragraph_order() {
            if para_id == page.start_para {
                break;
            }
        }

        // Add height of lines on this page before cursor line
        let start_para = page.start_para;
        let start_line = page.start_line;

        for para_id in document.paragraph_order() {
            if para_id < start_para {
                continue;
            }

            if let Some(pl) = layout.paragraph_layout(para_id) {
                let first_line = if para_id == start_para { start_line } else { 0 };
                
                for (idx, ln) in pl.lines.iter().enumerate() {
                    if idx < first_line {
                        continue;
                    }

                    if para_id == cursor.position.para_id && idx == line_idx {
                        // Found cursor line
                        // Note: We don't calculate precise X here because Web client
                        // calculates it using DOM measurement for perfect alignment.
                        // We still provide Y and utf16_offset which are essential.
                        let x = 0.0;
                        
                        return Some((Point { x, y }, utf16_offset));
                    }

                    y += ln.height;
                }
            }

            if para_id == page.end_para {
                break;
            }
        }

        None
    }

    /// Calculate selection range (UTF-16) for a line
    fn selection_range_for_line(
        _document: &Document,
        para_id: ParagraphId,
        line: &crate::layout::LineLayout,
        selection: &Selection,
        line_text: &str, // slice of text for this line
    ) -> Option<(usize, usize)> {
        let (sel_start, sel_end) = selection.ordered();
        
        // Convert selection to absolute byte offsets
        // Note: selection positions (DocPosition) are relative to paragraph start
        // But for comparison, we need to handle paragraph boundaries.
        // Actually, we can just compare DocPosition directly if we are careful.
        // But line ranges are byte offsets within paragraph.
        
        // Check if this paragraph intersects selection
        if para_id < sel_start.para_id || para_id > sel_end.para_id {
            return None;
        }

        // Line byte range in paragraph
        let line_start_byte = line.byte_range.start;
        let line_end_byte = line.byte_range.end;

        // Calculate intersection in paragraph-relative byte offsets
        let intersect_start_byte = if para_id == sel_start.para_id {
            sel_start.offset.max(line_start_byte)
        } else {
            line_start_byte
        };

        let intersect_end_byte = if para_id == sel_end.para_id {
            sel_end.offset.min(line_end_byte)
        } else {
            line_end_byte
        };

        if intersect_start_byte >= intersect_end_byte {
            return None;
        }

        // Now we have the byte range *within the paragraph* that is selected: [intersect_start_byte, intersect_end_byte)
        // We need to convert this to UTF-16 offsets *relative to the line start*.
        
        // Offset relative to line start (bytes)
        let rel_start_byte = intersect_start_byte.saturating_sub(line_start_byte);
        let rel_end_byte = intersect_end_byte.saturating_sub(line_start_byte);
        
        // Safety check for slicing
        if rel_start_byte > line_text.len() || rel_end_byte > line_text.len() {
            return None; 
        }

        // Convert byte offsets to UTF-16 offsets
        let text_before_start = &line_text[..rel_start_byte];
        let text_segment = &line_text[rel_start_byte..rel_end_byte];
        
        let utf16_start = text_before_start.chars().map(|c| c.len_utf16()).sum::<usize>();
        let utf16_len = text_segment.chars().map(|c| c.len_utf16()).sum::<usize>();
        let utf16_end = utf16_start + utf16_len;

        Some((utf16_start, utf16_end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_item_id() {
        let id = DisplayItemId::new(ParagraphId(5), 3, 0);
        assert_eq!(id.para_id, ParagraphId(5));
        assert_eq!(id.line_index, 3);
        assert_eq!(id.run_index, 0);
    }

    #[test]
    fn test_list_marker_display() {
        let bullet: ListMarkerDisplay = (&ListMarker::Bullet).into();
        assert_eq!(bullet, ListMarkerDisplay::Bullet);

        let number: ListMarkerDisplay = (&ListMarker::Numbered { ordinal: 5 }).into();
        assert_eq!(number, ListMarkerDisplay::Number("5.".to_string()));
    }
}
