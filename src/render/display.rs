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
        /// Character offset within the line (for frontend text measurement)
        line_char_offset: usize,
    },
    /// Selection highlight
    SelectionRect {
        rect: Rect,
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

                        // Text run
                        items.push(DisplayItem::TextRun {
                            id: DisplayItemId::new(para_id, line_idx, 0),
                            position: Point {
                                x: constraints.margin_left + indent,
                                y,
                            },
                            text: line_text,
                            block_kind: block_kind.clone(),
                        });

                        // Selection rectangles
                        if let Some(sel) = selection {
                            if !sel.is_collapsed() {
                                if let Some(rect) = Self::selection_rect_for_line(
                                    document,
                                    layout,
                                    para_id,
                                    line_idx,
                                    line,
                                    sel,
                                    constraints,
                                    y,
                                    indent,
                                ) {
                                    items.push(DisplayItem::SelectionRect { rect });
                                }
                            }
                        }

                        y += line.height;
                    }
                }

                if para_id == page_layout.end_para {
                    break;
                }
            }

            // Cursor
            if let Some((caret_pos, line_char_offset)) = Self::cursor_position(
                document,
                layout,
                cursor,
                page_layout,
                constraints,
            ) {
                items.push(DisplayItem::Caret {
                    position: caret_pos,
                    height: crate::layout::LINE_HEIGHT,
                    line_char_offset,
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
    /// Returns (Point, line_char_offset) where line_char_offset is the character position within the line
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

        // Calculate character offset within line
        let line_char_offset = cursor.position.offset.saturating_sub(line.byte_range.start);

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
                        let block_meta = document.block_meta(para_id);
                        let indent = layout.indent_for(
                            block_meta.unwrap_or(&crate::document::BlockMeta {
                                kind: BlockKind::Paragraph,
                                start_offset: 0,
                                byte_len: 0,
                            })
                        );

                        let x = constraints.margin_left + indent + 
                            line.x_for_offset(cursor.position.offset);
                        
                        return Some((Point { x, y }, line_char_offset));
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

    /// Calculate selection rectangle for a line
    fn selection_rect_for_line(
        document: &Document,
        layout: &LayoutState,
        para_id: ParagraphId,
        line_idx: usize,
        line: &crate::layout::LineLayout,
        selection: &Selection,
        constraints: &crate::layout::LayoutConstraints,
        y: f32,
        indent: f32,
    ) -> Option<Rect> {
        let (sel_start, sel_end) = selection.ordered();
        
        // Convert selection to absolute offsets
        let sel_start_abs = document.position_to_offset(&sel_start);
        let sel_end_abs = document.position_to_offset(&sel_end);

        // Get paragraph start offset
        let para_start = document.block_meta(para_id)?.start_offset;
        
        // Convert line range to absolute
        let line_start_abs = para_start + line.byte_range.start;
        let line_end_abs = para_start + line.byte_range.end;

        // Check if line intersects selection
        if line_end_abs <= sel_start_abs.0 || line_start_abs >= sel_end_abs.0 {
            return None;
        }

        // Calculate X coordinates
        let start_x = if sel_start_abs.0 <= line_start_abs {
            0.0
        } else {
            let offset_in_line = sel_start_abs.0 - para_start - line.byte_range.start;
            line.x_for_offset(line.byte_range.start + offset_in_line)
        };

        let end_x = if sel_end_abs.0 >= line_end_abs {
            line.width
        } else {
            let offset_in_line = sel_end_abs.0 - para_start - line.byte_range.start;
            line.x_for_offset(line.byte_range.start + offset_in_line)
        };

        Some(Rect {
            x: constraints.margin_left + indent + start_x,
            y,
            width: end_x - start_x,
            height: line.height,
        })
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
