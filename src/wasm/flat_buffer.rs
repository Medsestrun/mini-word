//! Flat buffer protocol for zero-copy WASM bridge
//!
//! Binary format for render data:
//!
//! ## u32 Buffer Layout:
//! ```text
//! [0]     version_lo
//! [1]     version_hi
//! [2]     page_count
//! [3]     cursor_present (0 or 1)
//! [4]     selection_count
//! [5]     text_buffer_len
//! [6..]   page data...
//! ```
//!
//! ## Per-page layout in u32:
//! ```text
//! page_index
//! line_count
//! per-line: [text_offset, text_len, block_type, flags]
//!   flags: bit0=is_heading, bit1=is_list_item, bits2-4=heading_level
//! ```
//!
//! ## f32 Buffer Layout:
//! ```text
//! Per-page: [y_offset, width, height]
//! Per-line: [x, y]
//! Cursor (if present): [x, y, height, page_index]
//! Per-selection: [x, y, width, height, page_index]
//! ```

/// Opcodes for block types
pub const BLOCK_PARAGRAPH: u32 = 0;
pub const BLOCK_HEADING_1: u32 = 1;
pub const BLOCK_HEADING_2: u32 = 2;
pub const BLOCK_HEADING_3: u32 = 3;
pub const BLOCK_HEADING_4: u32 = 4;
pub const BLOCK_HEADING_5: u32 = 5;
pub const BLOCK_HEADING_6: u32 = 6;
pub const BLOCK_LIST_ITEM: u32 = 7;

/// Flags bitmask
pub const FLAG_IS_HEADING: u32 = 0b0001;
pub const FLAG_IS_LIST_ITEM: u32 = 0b0010;

/// Render buffer for zero-copy WASM transfer
pub struct RenderBuffer {
    /// Integer data (indices, counts, offsets, opcodes)
    pub u32_data: Vec<u32>,
    /// Float data (positions, dimensions)
    pub f32_data: Vec<f32>,
    /// UTF-8 text buffer
    pub text_data: Vec<u8>,
}

impl Default for RenderBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderBuffer {
    pub fn new() -> Self {
        Self {
            u32_data: Vec::with_capacity(1024),
            f32_data: Vec::with_capacity(1024),
            text_data: Vec::with_capacity(4096),
        }
    }

    pub fn clear(&mut self) {
        self.u32_data.clear();
        self.f32_data.clear();
        self.text_data.clear();
    }

    /// Write header and return position for later updates
    pub fn write_header(&mut self, version: u64, page_count: u32) {
        self.u32_data.push((version & 0xFFFFFFFF) as u32); // version_lo
        self.u32_data.push((version >> 32) as u32);        // version_hi
        self.u32_data.push(page_count);                    // page_count
        self.u32_data.push(0);                             // cursor_present (placeholder)
        self.u32_data.push(0);                             // selection_count (placeholder)
        self.u32_data.push(0);                             // text_buffer_len (placeholder)
    }

    /// Set cursor present flag
    pub fn set_cursor_present(&mut self, present: bool) {
        if self.u32_data.len() > 3 {
            self.u32_data[3] = if present { 1 } else { 0 };
        }
    }

    /// Set selection count
    pub fn set_selection_count(&mut self, count: u32) {
        if self.u32_data.len() > 4 {
            self.u32_data[4] = count;
        }
    }

    /// Finalize text buffer length
    pub fn finalize(&mut self) {
        if self.u32_data.len() > 5 {
            self.u32_data[5] = self.text_data.len() as u32;
        }
    }

    /// Write page header, returns index where line_count should be written
    pub fn begin_page(&mut self, page_index: usize, y_offset: f32, width: f32, height: f32) -> usize {
        self.u32_data.push(page_index as u32);
        let line_count_idx = self.u32_data.len();
        self.u32_data.push(0); // line_count placeholder

        self.f32_data.push(y_offset);
        self.f32_data.push(width);
        self.f32_data.push(height);

        line_count_idx
    }

    /// Update line count for a page
    pub fn set_line_count(&mut self, idx: usize, count: u32) {
        if idx < self.u32_data.len() {
            self.u32_data[idx] = count;
        }
    }

    /// Write a text line
    pub fn write_line(
        &mut self,
        x: f32,
        y: f32,
        text: &str,
        block_type: u32,
        flags: u32,
        list_marker: Option<&str>,
    ) {
        // Write text to buffer and record offset
        let text_offset = self.text_data.len() as u32;
        self.text_data.extend_from_slice(text.as_bytes());
        let text_len = text.len() as u32;

        // List marker (stored after main text)
        let marker_offset = self.text_data.len() as u32;
        let marker_len = if let Some(marker) = list_marker {
            self.text_data.extend_from_slice(marker.as_bytes());
            marker.len() as u32
        } else {
            0
        };

        // u32: text_offset, text_len, block_type, flags, marker_offset, marker_len
        self.u32_data.push(text_offset);
        self.u32_data.push(text_len);
        self.u32_data.push(block_type);
        self.u32_data.push(flags);
        self.u32_data.push(marker_offset);
        self.u32_data.push(marker_len);

        // f32: x, y
        self.f32_data.push(x);
        self.f32_data.push(y);
    }

    /// Write cursor data with line text offset for frontend text measurement
    pub fn write_cursor(&mut self, x: f32, y: f32, height: f32, page_index: usize, line_text_offset: usize) {
        self.set_cursor_present(true);
        self.f32_data.push(x);
        self.f32_data.push(y);
        self.f32_data.push(height);
        self.f32_data.push(page_index as f32);
        self.f32_data.push(line_text_offset as f32); // Character offset within line for frontend measurement
    }

    /// Write selection rectangle
    pub fn write_selection(&mut self, x: f32, y: f32, width: f32, height: f32, page_index: usize) {
        self.f32_data.push(x);
        self.f32_data.push(y);
        self.f32_data.push(width);
        self.f32_data.push(height);
        self.f32_data.push(page_index as f32);
    }

    // Accessors for WASM

    pub fn u32_ptr(&self) -> *const u32 {
        self.u32_data.as_ptr()
    }

    pub fn u32_len(&self) -> usize {
        self.u32_data.len()
    }

    pub fn f32_ptr(&self) -> *const f32 {
        self.f32_data.as_ptr()
    }

    pub fn f32_len(&self) -> usize {
        self.f32_data.len()
    }

    pub fn text_ptr(&self) -> *const u8 {
        self.text_data.as_ptr()
    }

    pub fn text_len(&self) -> usize {
        self.text_data.len()
    }
}

/// Convert BlockKind to block type opcode
pub fn block_kind_to_opcode(kind: &crate::document::BlockKind) -> (u32, u32) {
    use crate::document::BlockKind;
    
    match kind {
        BlockKind::Paragraph => (BLOCK_PARAGRAPH, 0),
        BlockKind::Heading { level } => {
            let opcode = match level {
                1 => BLOCK_HEADING_1,
                2 => BLOCK_HEADING_2,
                3 => BLOCK_HEADING_3,
                4 => BLOCK_HEADING_4,
                5 => BLOCK_HEADING_5,
                6 => BLOCK_HEADING_6,
                _ => BLOCK_HEADING_1,
            };
            (opcode, FLAG_IS_HEADING | ((*level as u32) << 2))
        }
        BlockKind::ListItem { .. } => (BLOCK_LIST_ITEM, FLAG_IS_LIST_ITEM),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_buffer_basic() {
        let mut buf = RenderBuffer::new();
        buf.write_header(42, 1);
        
        let line_idx = buf.begin_page(0, 0.0, 816.0, 1056.0);
        buf.write_line(96.0, 96.0, "Hello", BLOCK_PARAGRAPH, 0, None);
        buf.set_line_count(line_idx, 1);
        buf.finalize();

        assert_eq!(buf.u32_data[0], 42); // version_lo
        assert_eq!(buf.u32_data[2], 1);  // page_count
        assert_eq!(buf.text_data, b"Hello");
    }

    #[test]
    fn test_render_buffer_with_cursor() {
        let mut buf = RenderBuffer::new();
        buf.write_header(1, 1);
        buf.write_cursor(100.0, 200.0, 20.0, 0, 5); // 5 = line char offset
        buf.finalize();

        assert_eq!(buf.u32_data[3], 1); // cursor_present
        // Check that line_char_offset is written (5th value in cursor f32 data)
        assert_eq!(buf.f32_data[4], 5.0);
    }
}
