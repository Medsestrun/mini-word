//! Mini-Word: A high-performance WYSIWYG text editor core
//!
//! This crate provides the core editing engine with:
//! - Incremental layout (only affected paragraphs are relaid out)
//! - Diff-based rendering (only changed items are emitted)
//! - Rope-based document model for O(log n) edits
//! - Full undo/redo support

pub mod document;
pub mod editing;
pub mod layout;
pub mod render;
pub mod undo;
pub mod wasm;

// Re-export WASM types for direct use
pub use wasm::WasmEditor;

// Re-export primary types
pub use document::{BlockKind, BlockMeta, Document, ListMarker, ParagraphId};
pub use editing::{Affinity, Cursor, DocPosition, EditOp, EditResult, Selection};
pub use layout::{LayoutConstraints, LayoutState, LineLayout, ParagraphLayout};
pub use render::{DisplayItem, DisplayItemId, DisplayList, DisplayPage, RenderDiff, RenderPatch};
pub use undo::UndoManager;

/// Editor coordinates
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Editor rectangle
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }

    pub fn contains_point(&self, point: Point) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }
}

/// The main editor state combining all components
pub struct Editor {
    pub document: Document,
    pub cursor: Cursor,
    pub selection: Option<Selection>,
    pub layout: LayoutState,
    pub undo_manager: UndoManager,
    layout_dirty: bool,
}

impl Editor {
    /// Create a new editor with the given constraints
    pub fn new(constraints: LayoutConstraints) -> Self {
        Self {
            document: Document::new(),
            cursor: Cursor::default(),
            selection: None,
            layout: LayoutState::new(constraints),
            undo_manager: UndoManager::new(100),
            layout_dirty: true,
        }
    }

    /// Create an editor with initial text content
    pub fn with_text(text: &str, constraints: LayoutConstraints) -> Self {
        let mut editor = Self::new(constraints);
        editor.document = Document::from_text(text);
        editor.layout_dirty = true;
        editor
    }

    /// Insert text at the current cursor position
    pub fn insert_text(&mut self, text: &str) -> EditResult {
        self.undo_manager
            .begin_transaction("insert", &self.cursor, self.selection.as_ref());

        let position = self.document.position_to_offset(&self.cursor.position);
        let op = EditOp::Insert {
            position,
            text: text.to_string(),
        };

        let reverse = self.document.compute_reverse(&op);
        let result = self.document.apply_edit(op.clone());

        self.undo_manager.record_edit(op, reverse);
        self.undo_manager.commit();

        // Update cursor
        self.cursor.position = result.new_cursor.clone();
        self.selection = None;

        // Mark layout dirty
        self.layout.invalidate(&result);
        self.layout_dirty = true;

        result
    }

    /// Delete text in the given range or at cursor
    pub fn delete(&mut self, backward: bool) -> Option<EditResult> {
        self.undo_manager
            .begin_transaction("delete", &self.cursor, self.selection.as_ref());

        let (start, end) = if let Some(ref sel) = self.selection {
            let (s, e) = sel.ordered();
            (
                self.document.position_to_offset(&s),
                self.document.position_to_offset(&e),
            )
        } else {
            let pos = self.document.position_to_offset(&self.cursor.position);
            if backward {
                if pos.0 == 0 {
                    return None;
                }
                let prev = self.document.prev_grapheme_offset(pos);
                (prev, pos)
            } else {
                let next = self.document.next_grapheme_offset(pos);
                if next == pos {
                    return None;
                }
                (pos, next)
            }
        };

        let op = EditOp::Delete { start, end };
        let reverse = self.document.compute_reverse(&op);
        let result = self.document.apply_edit(op.clone());

        self.undo_manager.record_edit(op, reverse);
        self.undo_manager.commit();

        // Update cursor
        self.cursor.position = result.new_cursor.clone();
        self.selection = None;

        // Mark layout dirty
        self.layout.invalidate(&result);
        self.layout_dirty = true;

        Some(result)
    }

    /// Perform layout if needed and return render diff
    pub fn update_layout(&mut self) -> Option<RenderDiff> {
        if !self.layout_dirty {
            return None;
        }

        let diff = self.layout.relayout(&self.document);
        self.layout_dirty = false;

        Some(diff)
    }

    /// Build display list for the given viewport
    pub fn build_display_list(&self, viewport: Rect) -> DisplayList {
        self.layout.build_display_list(
            &self.document,
            viewport,
            &self.cursor,
            self.selection.as_ref(),
        )
    }

    /// Undo the last operation
    pub fn undo(&mut self) -> bool {
        if let Some(result) = self.undo_manager.undo(&mut self.document) {
            self.cursor = result.cursor;
            self.selection = result.selection;
            self.layout_dirty = true;
            // Full layout invalidation for undo
            self.layout.invalidate_all();
            true
        } else {
            false
        }
    }

    /// Redo the last undone operation
    pub fn redo(&mut self) -> bool {
        if let Some(result) = self.undo_manager.redo(&mut self.document) {
            self.cursor = result.cursor;
            self.selection = result.selection;
            self.layout_dirty = true;
            self.layout.invalidate_all();
            true
        } else {
            false
        }
    }

    /// Move cursor by the given delta
    pub fn move_cursor(&mut self, horizontal: i32, vertical: i32, extend_selection: bool) {
        if extend_selection && self.selection.is_none() {
            self.selection = Some(Selection {
                anchor: self.cursor.position.clone(),
                active: self.cursor.position.clone(),
            });
        }

        // Horizontal movement
        if horizontal != 0 {
            let offset = self.document.position_to_offset(&self.cursor.position);
            let new_offset = if horizontal > 0 {
                self.document.next_grapheme_offset(offset)
            } else {
                self.document.prev_grapheme_offset(offset)
            };
            self.cursor.position = self.document.offset_to_position(new_offset);
            self.cursor.preferred_x = None;
        }

        // Vertical movement
        if vertical != 0 {
            if let Some(new_pos) = self.layout.move_cursor_vertical(
                &self.document,
                &self.cursor.position,
                vertical,
                self.cursor.preferred_x,
            ) {
                // Remember X position for vertical movement
                if self.cursor.preferred_x.is_none() {
                    self.cursor.preferred_x = self
                        .layout
                        .position_to_x(&self.document, &self.cursor.position);
                }
                self.cursor.position = new_pos;
            }
        }

        if extend_selection {
            if let Some(ref mut sel) = self.selection {
                sel.active = self.cursor.position.clone();
            }
        } else {
            self.selection = None;
        }
    }

    /// Get document text
    pub fn text(&self) -> String {
        self.document.text()
    }

    /// Get total page count
    pub fn page_count(&self) -> usize {
        self.layout.page_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_constraints() -> LayoutConstraints {
        LayoutConstraints {
            page_width: 612.0,  // US Letter
            page_height: 792.0,
            margin_top: 72.0,
            margin_bottom: 72.0,
            margin_left: 72.0,
            margin_right: 72.0,
        }
    }

    #[test]
    fn test_create_editor() {
        let editor = Editor::new(default_constraints());
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn test_insert_text() {
        let mut editor = Editor::new(default_constraints());
        editor.insert_text("Hello, World!");
        assert_eq!(editor.text(), "Hello, World!");
    }

    #[test]
    fn test_undo_redo() {
        let mut editor = Editor::new(default_constraints());
        editor.insert_text("Hello");
        assert_eq!(editor.text(), "Hello");

        editor.undo();
        assert_eq!(editor.text(), "");

        editor.redo();
        assert_eq!(editor.text(), "Hello");
    }
}
