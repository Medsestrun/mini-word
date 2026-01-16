//! WASM bindings for the editor

use wasm_bindgen::prelude::*;
use serde::{Deserialize, Serialize};
use crate::{Editor, LayoutConstraints, Rect};

/// Initialize panic hook for better error messages
#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

/// WASM-exposed editor wrapper
#[wasm_bindgen]
pub struct WasmEditor {
    editor: Editor,
}

#[wasm_bindgen]
impl WasmEditor {
    /// Create a new editor with default page size (US Letter)
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        let constraints = LayoutConstraints {
            page_width: 816.0,   // 8.5" at 96 DPI
            page_height: 1056.0, // 11" at 96 DPI
            margin_top: 96.0,    // 1" margins
            margin_bottom: 96.0,
            margin_left: 96.0,
            margin_right: 96.0,
        };

        let mut editor = Editor::new(constraints);
        // Initialize layout for the empty document
        editor.update_layout();
        
        Self { editor }
    }

    /// Create editor with custom page dimensions
    #[wasm_bindgen(js_name = withDimensions)]
    pub fn with_dimensions(
        page_width: f32,
        page_height: f32,
        margin_top: f32,
        margin_bottom: f32,
        margin_left: f32,
        margin_right: f32,
    ) -> Self {
        let constraints = LayoutConstraints {
            page_width,
            page_height,
            margin_top,
            margin_bottom,
            margin_left,
            margin_right,
        };

        let mut editor = Editor::new(constraints);
        // Initialize layout for the empty document
        editor.update_layout();
        
        Self { editor }
    }

    /// Insert text at current cursor position
    #[wasm_bindgen(js_name = insertText)]
    pub fn insert_text(&mut self, text: &str) {
        self.editor.insert_text(text);
        self.editor.update_layout();
    }

    /// Delete backward (backspace)
    #[wasm_bindgen(js_name = deleteBackward)]
    pub fn delete_backward(&mut self) -> bool {
        let result = self.editor.delete(true).is_some();
        if result {
            self.editor.update_layout();
        }
        result
    }

    /// Delete forward (delete key)
    #[wasm_bindgen(js_name = deleteForward)]
    pub fn delete_forward(&mut self) -> bool {
        let result = self.editor.delete(false).is_some();
        if result {
            self.editor.update_layout();
        }
        result
    }

    /// Move cursor
    #[wasm_bindgen(js_name = moveCursor)]
    pub fn move_cursor(&mut self, horizontal: i32, vertical: i32, extend_selection: bool) {
        self.editor.move_cursor(horizontal, vertical, extend_selection);
    }

    /// Undo last operation
    pub fn undo(&mut self) -> bool {
        let result = self.editor.undo();
        if result {
            self.editor.update_layout();
        }
        result
    }

    /// Redo last undone operation
    pub fn redo(&mut self) -> bool {
        let result = self.editor.redo();
        if result {
            self.editor.update_layout();
        }
        result
    }

    /// Get full document text
    #[wasm_bindgen(js_name = getText)]
    pub fn get_text(&self) -> String {
        self.editor.text()
    }

    /// Get page count
    #[wasm_bindgen(js_name = getPageCount)]
    pub fn get_page_count(&self) -> usize {
        self.editor.page_count()
    }

    /// Get render data for a viewport (returns JSON)
    #[wasm_bindgen(js_name = getRenderData)]
    pub fn get_render_data(&self, viewport_y: f32, viewport_height: f32) -> JsValue {
        let viewport = Rect::new(0.0, viewport_y, 816.0, viewport_height);
        let display_list = self.editor.build_display_list(viewport);
        
        let render_data = RenderData::from_display_list(&display_list, &self.editor);
        
        serde_wasm_bindgen::to_value(&render_data).unwrap_or(JsValue::NULL)
    }

    /// Get cursor position info (returns JSON)
    #[wasm_bindgen(js_name = getCursorInfo)]
    pub fn get_cursor_info(&self) -> JsValue {
        let cursor_info = CursorInfo {
            para_id: self.editor.cursor.position.para_id.0,
            offset: self.editor.cursor.position.offset,
            has_selection: self.editor.selection.is_some(),
        };
        
        serde_wasm_bindgen::to_value(&cursor_info).unwrap_or(JsValue::NULL)
    }

    /// Get layout constraints
    #[wasm_bindgen(js_name = getLayoutConstraints)]
    pub fn get_layout_constraints(&self) -> JsValue {
        let constraints = LayoutConstraintsJS {
            page_width: self.editor.layout.constraints().page_width,
            page_height: self.editor.layout.constraints().page_height,
            margin_top: self.editor.layout.constraints().margin_top,
            margin_bottom: self.editor.layout.constraints().margin_bottom,
            margin_left: self.editor.layout.constraints().margin_left,
            margin_right: self.editor.layout.constraints().margin_right,
            content_width: self.editor.layout.constraints().content_width(),
            content_height: self.editor.layout.constraints().content_height(),
        };
        
        serde_wasm_bindgen::to_value(&constraints).unwrap_or(JsValue::NULL)
    }

    /// Select all text
    #[wasm_bindgen(js_name = selectAll)]
    pub fn select_all(&mut self) {
        // Move to start
        while self.editor.cursor.position.offset > 0 || self.editor.cursor.position.para_id.0 > 0 {
            self.editor.move_cursor(-1, 0, false);
        }
        // Extend selection to end
        let text_len = self.editor.text().len();
        for _ in 0..text_len {
            self.editor.move_cursor(1, 0, true);
        }
    }

    /// Clear selection
    #[wasm_bindgen(js_name = clearSelection)]
    pub fn clear_selection(&mut self) {
        self.editor.selection = None;
    }

    /// Insert a new paragraph (Enter key)
    #[wasm_bindgen(js_name = insertParagraph)]
    pub fn insert_paragraph(&mut self) {
        self.insert_text("\n");
    }
}

impl Default for WasmEditor {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable render data for JS
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderData {
    pub version: u64,
    pub pages: Vec<PageRenderData>,
    pub cursor: Option<CursorRenderData>,
    pub selections: Vec<SelectionRenderData>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRenderData {
    pub page_index: usize,
    pub y_offset: f32,
    pub width: f32,
    pub height: f32,
    pub lines: Vec<LineRenderData>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineRenderData {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub block_type: String,
    pub is_heading: bool,
    pub heading_level: Option<u8>,
    pub is_list_item: bool,
    pub list_marker: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorRenderData {
    pub x: f32,
    pub y: f32,
    pub height: f32,
    pub page_index: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRenderData {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub page_index: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorInfo {
    pub para_id: u64,
    pub offset: usize,
    pub has_selection: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutConstraintsJS {
    pub page_width: f32,
    pub page_height: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub margin_right: f32,
    pub content_width: f32,
    pub content_height: f32,
}

impl RenderData {
    fn from_display_list(display_list: &crate::render::DisplayList, editor: &Editor) -> Self {
        use crate::render::DisplayItem;
        use crate::document::BlockKind;

        let constraints = editor.layout.constraints();
        let mut pages = Vec::new();
        let mut cursor = None;
        let mut selections = Vec::new();

        for page in &display_list.pages {
            let mut lines = Vec::new();

            for item in &page.items {
                match item {
                    DisplayItem::TextRun { position, text, block_kind, .. } => {
                        let (is_heading, heading_level) = match block_kind {
                            BlockKind::Heading { level } => (true, Some(*level)),
                            _ => (false, None),
                        };

                        let (is_list_item, list_marker) = match block_kind {
                            BlockKind::ListItem { marker, .. } => {
                                (true, Some(marker.display()))
                            }
                            _ => (false, None),
                        };

                        lines.push(LineRenderData {
                            x: position.x,
                            y: position.y,
                            text: text.clone(),
                            block_type: match block_kind {
                                BlockKind::Paragraph => "paragraph".to_string(),
                                BlockKind::Heading { level } => format!("heading-{}", level),
                                BlockKind::ListItem { .. } => "list-item".to_string(),
                            },
                            is_heading,
                            heading_level,
                            is_list_item,
                            list_marker,
                        });
                    }
                    DisplayItem::Caret { position, height } => {
                        cursor = Some(CursorRenderData {
                            x: position.x,
                            y: position.y,
                            height: *height,
                            page_index: page.page_index,
                        });
                    }
                    DisplayItem::SelectionRect { rect } => {
                        selections.push(SelectionRenderData {
                            x: rect.x,
                            y: rect.y,
                            width: rect.width,
                            height: rect.height,
                            page_index: page.page_index,
                        });
                    }
                    _ => {}
                }
            }

            pages.push(PageRenderData {
                page_index: page.page_index,
                y_offset: page.page_index as f32 * constraints.page_height,
                width: constraints.page_width,
                height: constraints.page_height,
                lines,
            });
        }

        RenderData {
            version: display_list.version,
            pages,
            cursor,
            selections,
        }
    }
}
