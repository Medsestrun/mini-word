//! WASM bindings for the editor
//!
//! Provides a zero-copy bridge using flat typed arrays instead of JSON serialization.

pub mod flat_buffer;

use wasm_bindgen::prelude::*;
use crate::document::BlockKind;
use crate::{Editor, LayoutConstraints, Rect};
use flat_buffer::{
    RenderBuffer, 
    block_kind_to_opcode,
    HEADER_SIZE,
    U32_PER_LINE,
    U32_PER_CURSOR,
    U32_PER_SELECTION,
    F32_PER_CURSOR,
    F32_PER_SELECTION,
};

/// Get access to WASM memory for zero-copy data access
#[wasm_bindgen(js_name = getWasmMemory)]
pub fn get_wasm_memory() -> JsValue {
    wasm_bindgen::memory()
}

/// Initialize panic hook for better error messages
#[wasm_bindgen(start)]
pub fn init() {
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

/// WASM-exposed editor wrapper with zero-copy render buffer
#[wasm_bindgen]
pub struct WasmEditor {
    editor: Editor,
    render_buffer: RenderBuffer,
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
        editor.update_layout();
        
        Self { 
            editor,
            render_buffer: RenderBuffer::new(),
        }
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
        editor.update_layout();
        
        Self { 
            editor,
            render_buffer: RenderBuffer::new(),
        }
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

    /// Select all text
    #[wasm_bindgen(js_name = selectAll)]
    pub fn select_all(&mut self) {
        while self.editor.cursor.position.offset > 0 || self.editor.cursor.position.para_id.0 > 0 {
            self.editor.move_cursor(-1, 0, false);
        }
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

    // =========================================================================
    // Zero-copy render buffer API
    // =========================================================================

    /// Build render data into internal buffers for the given viewport.
    /// Call this before accessing the buffer pointers.
    #[wasm_bindgen(js_name = buildRenderData)]
    pub fn build_render_data(&mut self, viewport_y: f32, viewport_height: f32) {
        let viewport = Rect::new(0.0, viewport_y, 816.0, viewport_height);
        let display_list = self.editor.build_display_list(viewport);
        let constraints = self.editor.layout.constraints();

        // Pre-calculate buffer sizes to avoid reallocation (critical: JS holds pointers to these buffers)
        let mut total_lines = 0;
        let mut total_text_bytes = 0;
        let mut cursor_count = 0;
        let mut selection_count = 0;

        for page in &display_list.pages {
            for item in &page.items {
                match item {
                    crate::render::DisplayItem::TextRun { text, block_kind, .. } => {
                        total_lines += 1;
                        total_text_bytes += text.len();
                        
                        // Add marker length if present
                        if let BlockKind::ListItem { marker, .. } = block_kind {
                            total_text_bytes += marker.display().len();
                        }
                    }
                    crate::render::DisplayItem::Caret { .. } => {
                        cursor_count = 1;
                    }
                    crate::render::DisplayItem::SelectionRect { .. } => {
                        selection_count += 1;
                    }
                    _ => {}
                }
            }
        }

        // Estimate buffer sizes
        let page_count = display_list.pages.len();
        let u32_needed = HEADER_SIZE + page_count * 2 + total_lines * U32_PER_LINE + cursor_count * U32_PER_CURSOR + selection_count * U32_PER_SELECTION;
        let f32_needed = page_count * 3 + total_lines * 2 + cursor_count * F32_PER_CURSOR + selection_count * F32_PER_SELECTION;
        let text_needed = total_text_bytes;

        // Pre-allocate buffers to avoid reallocation during rendering
        self.render_buffer.prepare(u32_needed, f32_needed, text_needed);

        // Write header
        self.render_buffer.write_header(
            display_list.version,
            display_list.pages.len() as u32,
        );

        // Collect cursor and selections separately - they must be written AFTER all pages/lines
        // cursor_data: (x, y, height, page_index, utf16_offset_in_line)
        let mut cursor_data: Option<(f32, f32, f32, usize, usize)> = None;
        let mut selections: Vec<(f32, f32, f32, f32, usize)> = Vec::new();

        // First pass: write pages and lines, collect cursor and selections
        for page in &display_list.pages {
            let page_y = page.page_index as f32 * constraints.page_height;
            let line_count_idx = self.render_buffer.begin_page(
                page.page_index,
                page_y,
                constraints.page_width,
                constraints.page_height,
            );

            let mut line_count: u32 = 0;

            for item in &page.items {
                match item {
                    crate::render::DisplayItem::TextRun { position, text, block_kind, .. } => {
                        let (block_type, flags) = block_kind_to_opcode(block_kind);
                        
                        let list_marker = if let BlockKind::ListItem { marker, .. } = block_kind {
                            Some(marker.display())
                        } else {
                            None
                        };

                        self.render_buffer.write_line(
                            position.x,
                            position.y,
                            text,
                            block_type,
                            flags,
                            list_marker.as_deref(),
                        );
                        line_count += 1;
                    }
                    crate::render::DisplayItem::Caret { position, height, utf16_offset_in_line } => {
                        // Collect cursor data to write after all pages
                        cursor_data = Some((position.x, position.y, *height, page.page_index, *utf16_offset_in_line));
                    }
                    crate::render::DisplayItem::SelectionRect { rect } => {
                        // Collect selection data to write after all pages
                        selections.push((rect.x, rect.y, rect.width, rect.height, page.page_index));
                    }
                    _ => {}
                }
            }

            self.render_buffer.set_line_count(line_count_idx, line_count);
        }

        // Second pass: write cursor and selections after all pages/lines
        if let Some((x, y, height, page_index, utf16_offset)) = cursor_data {
            self.render_buffer.write_cursor(x, y, height, page_index, utf16_offset);
        }

        for (x, y, width, height, page_index) in &selections {
            self.render_buffer.write_selection(*x, *y, *width, *height, *page_index);
        }

        // Selection count is automatically tracked and written in finalize()
        self.render_buffer.finalize();
    }

    /// Get pointer to u32 buffer (call buildRenderData first)
    /// Returns u32 offset in WASM linear memory
    #[wasm_bindgen(js_name = getU32Ptr)]
    pub fn get_u32_ptr(&self) -> u32 {
        self.render_buffer.u32_ptr()
    }

    /// Get length of u32 buffer
    #[wasm_bindgen(js_name = getU32Len)]
    pub fn get_u32_len(&self) -> u32 {
        self.render_buffer.u32_len()
    }

    /// Get pointer to f32 buffer
    /// Returns u32 offset in WASM linear memory
    #[wasm_bindgen(js_name = getF32Ptr)]
    pub fn get_f32_ptr(&self) -> u32 {
        self.render_buffer.f32_ptr()
    }

    /// Get length of f32 buffer
    #[wasm_bindgen(js_name = getF32Len)]
    pub fn get_f32_len(&self) -> u32 {
        self.render_buffer.f32_len()
    }

    /// Get pointer to text buffer
    /// Returns u32 offset in WASM linear memory
    #[wasm_bindgen(js_name = getTextPtr)]
    pub fn get_text_ptr(&self) -> u32 {
        self.render_buffer.text_ptr()
    }

    /// Get length of text buffer
    #[wasm_bindgen(js_name = getTextLen)]
    pub fn get_text_len(&self) -> u32 {
        self.render_buffer.text_len()
    }

    // =========================================================================
    // Direct accessors for layout constraints (no serialization needed)
    // =========================================================================

    #[wasm_bindgen(js_name = getPageWidth)]
    pub fn get_page_width(&self) -> f32 {
        self.editor.layout.constraints().page_width
    }

    #[wasm_bindgen(js_name = getPageHeight)]
    pub fn get_page_height(&self) -> f32 {
        self.editor.layout.constraints().page_height
    }

    #[wasm_bindgen(js_name = getMarginTop)]
    pub fn get_margin_top(&self) -> f32 {
        self.editor.layout.constraints().margin_top
    }

    #[wasm_bindgen(js_name = getMarginBottom)]
    pub fn get_margin_bottom(&self) -> f32 {
        self.editor.layout.constraints().margin_bottom
    }

    #[wasm_bindgen(js_name = getMarginLeft)]
    pub fn get_margin_left(&self) -> f32 {
        self.editor.layout.constraints().margin_left
    }

    #[wasm_bindgen(js_name = getMarginRight)]
    pub fn get_margin_right(&self) -> f32 {
        self.editor.layout.constraints().margin_right
    }

    #[wasm_bindgen(js_name = getContentWidth)]
    pub fn get_content_width(&self) -> f32 {
        self.editor.layout.constraints().content_width()
    }

    #[wasm_bindgen(js_name = getContentHeight)]
    pub fn get_content_height(&self) -> f32 {
        self.editor.layout.constraints().content_height()
    }

    // =========================================================================
    // Cursor info accessors (no serialization needed)
    // =========================================================================

    #[wasm_bindgen(js_name = getCursorParaId)]
    pub fn get_cursor_para_id(&self) -> u64 {
        self.editor.cursor.position.para_id.0
    }

    #[wasm_bindgen(js_name = getCursorOffset)]
    pub fn get_cursor_offset(&self) -> usize {
        self.editor.cursor.position.offset
    }

    #[wasm_bindgen(js_name = hasSelection)]
    pub fn has_selection(&self) -> bool {
        self.editor.selection.is_some()
    }
}

impl Default for WasmEditor {
    fn default() -> Self {
        Self::new()
    }
}
