//! Flat buffer protocol for zero-copy WASM bridge
//!
//! Binary format for render data:
//!
//! ## u32 Buffer Layout:
//! ```text
//! Header (offset table for random access):
//! [0]     MAGIC (0x4D575244 = "MWRD" for validation)
//! [1]     SCHEMA_VERSION (protocol version, currently 1)
//! [2]     version_lo (document version)
//! [3]     version_hi (document version)
//! [4]     page_count
//! [5]     cursor_present (0 or 1)
//! [6]     selection_count
//! [7]     text_buffer_len
//! [8]     u32_cursor_offset (index in u32_data where cursor indices start, 0 if no cursor)
//! [9]     u32_selection_offset (index in u32_data where selection indices start, 0 if no selections)
//! [10]    f32_cursor_offset (index in f32_data where cursor geometry starts, 0 if no cursor)
//! [11]    f32_selection_offset (index in f32_data where selection geometries start, 0 if no selections)
//! [12..]  page data...
//!
//! Per-page:
//!   page_index
//!   line_count
//!   per-line: [text_offset, text_len, text_utf16_offset, text_utf16_len, 
//!              block_type, flags, marker_offset, marker_len, marker_utf16_offset, marker_utf16_len]
//!     text_offset/text_len: byte offsets in text_data (UTF-8)
//!     text_utf16_offset/text_utf16_len: offsets for JS substring (after single decode)
//!     flags: bit0=is_heading, bit1=is_list_item, bits2-4=heading_level
//!     marker: only read if marker_len > 0, otherwise marker_offset is ignored
//!
//! At u32_cursor_offset (if cursor_present):
//!   Cursor indices: [page_index, utf16_offset_in_line]
//!
//! At u32_selection_offset (if selection_count > 0):
//!   Per-selection indices: [page_index] (selection_count times)
//! ```
//!
//! ## f32 Buffer Layout:
//! ```text
//! Per-page: [y_offset, width, height]
//! Per-line: [x, y]
//! At f32_cursor_offset (if cursor_present): [x, y, height]
//! At f32_selection_offset (for each selection): [x, y, width, height] (selection_count times)
//! ```

/// Magic number for format validation: "MWRD" (MiniWoRD)
pub const MAGIC: u32 = 0x4D575244;

/// Schema version for protocol compatibility checking
pub const SCHEMA_VERSION: u32 = 1;

/// Header size in u32 elements
pub const HEADER_SIZE: usize = 12;

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

/// Number of u32 values per line in the buffer
/// [text_offset, text_len, text_utf16_offset, text_utf16_len, 
///  block_type, flags, marker_offset, marker_len, marker_utf16_offset, marker_utf16_len,
///  sel_start, sel_end, style_start_idx, style_count]
pub const U32_PER_LINE: usize = 14;

/// Number of u32 values per style span
/// [start, len, font_id]
pub const U32_PER_STYLE: usize = 3;

/// Number of u32 values for cursor indices
pub const U32_PER_CURSOR: usize = 2; // page_index, utf16_offset_in_line

/// Number of f32 values for cursor geometry
pub const F32_PER_CURSOR: usize = 3; // x, y, height

/// Number of u32 values per selection
pub const U32_PER_SELECTION: usize = 1; // page_index

/// Number of f32 values per selection geometry
pub const F32_PER_SELECTION: usize = 4; // x, y, width, height

/// Pending cursor data (written to buffers in finalize())
struct PendingCursor {
    x: f32,
    y: f32,
    height: f32,
    page_index: usize,
    utf16_offset_in_line: usize,
}

/// Pending selection data (written to buffers in finalize())
struct PendingSelection {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    page_index: usize,
}

/// Render buffer for zero-copy WASM transfer
pub struct RenderBuffer {
    /// Integer data (indices, counts, offsets, opcodes)
    pub u32_data: Vec<u32>,
    /// Float data (positions, dimensions)
    pub f32_data: Vec<f32>,
    /// UTF-8 text buffer
    pub text_data: Vec<u8>,
    /// Style data buffer (flat list of style spans)
    pub style_data: Vec<u32>,
    
    // Pending cursor/selections (written in finalize() to guarantee correct offsets)
    pending_cursor: Option<PendingCursor>,
    pending_selections: Vec<PendingSelection>,
    
    // Track cumulative UTF-16 offset for efficient JS decoding
    utf16_text_offset: usize,
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
            style_data: Vec::with_capacity(512),
            pending_cursor: None,
            pending_selections: Vec::new(),
            utf16_text_offset: 0,
        }
    }

    pub fn clear(&mut self) {
        self.u32_data.clear();
        self.f32_data.clear();
        self.text_data.clear();
        self.style_data.clear();
        self.pending_cursor = None;
        self.pending_selections.clear();
        self.utf16_text_offset = 0;
    }

    /// Pre-allocate buffers to avoid reallocation during rendering.
    /// Critical: JS holds pointers to these buffers, so realloc would cause invalid pointers.
    /// 
    /// Call this before write_header() with estimated sizes:
    /// - u32_needed: HEADER_SIZE + pages * (2 + lines * U32_PER_LINE) + cursor (U32_PER_CURSOR) + selections * U32_PER_SELECTION
    /// - f32_needed: pages * 3 + lines * 2 + cursor (F32_PER_CURSOR) + selections * F32_PER_SELECTION
    /// - text_needed: sum of text bytes + marker bytes
    pub fn prepare(&mut self, u32_needed: usize, f32_needed: usize, text_needed: usize) {
        // Target capacities with headroom
        let u32_target = u32_needed + 32;
        let f32_target = f32_needed + 32;
        let text_target = text_needed + 256;
        
        // Reuse buffers if capacity is sufficient (avoids malloc/free on every frame)
        // Only recreate if we need more capacity
        if self.u32_data.capacity() < u32_target {
            self.u32_data = Vec::with_capacity(u32_target);
        } else {
            self.u32_data.clear();
        }
        
        if self.f32_data.capacity() < f32_target {
            self.f32_data = Vec::with_capacity(f32_target);
        } else {
            self.f32_data.clear();
        }
        
        if self.text_data.capacity() < text_target {
            self.text_data = Vec::with_capacity(text_target);
        } else {
            self.text_data.clear();
        }

        // Reserve space for style data (simplistic for now)
        // Assume 1 style per line on average if not specified? 
        // We really should pass style_needed but let's just ensure some capacity.
        if self.style_data.capacity() < u32_target { // Rough heuristic or fix later
             self.style_data = Vec::with_capacity(u32_target);
        } else {
             self.style_data.clear();
        }
        
        // Clear pending data
        self.pending_cursor = None;
        self.pending_selections.clear();
        self.utf16_text_offset = 0;
    }

    /// Write header with offset table for random access
    pub fn write_header(&mut self, version: u64, page_count: u32) {
        self.u32_data.push(MAGIC);                         // [0] magic number
        self.u32_data.push(SCHEMA_VERSION);                // [1] schema version
        self.u32_data.push((version & 0xFFFFFFFF) as u32); // [2] version_lo (document version)
        self.u32_data.push((version >> 32) as u32);        // [3] version_hi (document version)
        self.u32_data.push(page_count);                    // [4] page_count
        self.u32_data.push(0);                             // [5] cursor_present (placeholder)
        self.u32_data.push(0);                             // [6] selection_count (placeholder)
        self.u32_data.push(0);                             // [7] text_buffer_len (placeholder)
        self.u32_data.push(0);                             // [8] u32_cursor_offset (placeholder)
        self.u32_data.push(0);                             // [9] u32_selection_offset (placeholder)
        self.u32_data.push(0);                             // [10] f32_cursor_offset (placeholder)
        self.u32_data.push(0);                             // [11] f32_selection_offset (placeholder)
    }

    /// Finalize buffer: write pending cursor/selections and synchronize header
    /// CRITICAL: Must be called after all page/line operations to ensure correct offsets
    pub fn finalize(&mut self) {
        if self.u32_data.len() < HEADER_SIZE {
            return;
        }
        
        // Write pending cursor (if present) AFTER all pages/lines
        if let Some(cursor) = &self.pending_cursor {
            // Record cursor offsets in header (indices 8 and 10)
            self.u32_data[8] = self.u32_data.len() as u32;   // u32 offset
            self.u32_data[10] = self.f32_data.len() as u32;  // f32 offset
            
            // Write cursor indices to u32_data
            self.u32_data.push(cursor.page_index as u32);
            self.u32_data.push(cursor.utf16_offset_in_line as u32);
            
            // Write cursor geometry to f32_data
            self.f32_data.push(cursor.x);
            self.f32_data.push(cursor.y);
            self.f32_data.push(cursor.height);
            
            // Set cursor_present flag
            self.u32_data[5] = 1;
        } else {
            self.u32_data[5] = 0;
            self.u32_data[10] = 0;
        }
        
        // Write pending selections (if any) AFTER all pages/lines and cursor
        if !self.pending_selections.is_empty() {
            // Record selection offsets in header (indices 9 and 11)
            self.u32_data[9] = self.u32_data.len() as u32;   // u32 offset
            self.u32_data[11] = self.f32_data.len() as u32;  // f32 offset
            
            for selection in &self.pending_selections {
                // Write selection index to u32_data
                self.u32_data.push(selection.page_index as u32);
                
                // Write selection geometry to f32_data
                self.f32_data.push(selection.x);
                self.f32_data.push(selection.y);
                self.f32_data.push(selection.width);
                self.f32_data.push(selection.height);
            }
            
            // Set selection count
            self.u32_data[6] = self.pending_selections.len() as u32;
        } else {
            self.u32_data[6] = 0;
            self.u32_data[11] = 0;
        }
        
        // Sync text buffer length
        self.u32_data[7] = self.text_data.len() as u32;
        
        // Debug validation: verify all text offsets are within bounds
        #[cfg(debug_assertions)]
        self.validate_text_offsets();
    }
    
    /// Validate that all text offsets are within bounds (debug builds only)
    #[cfg(debug_assertions)]
    fn validate_text_offsets(&self) {
        let page_count = self.u32_data[4] as usize;
        let text_len = self.text_data.len();
        let mut idx = HEADER_SIZE;
        
        for page_idx in 0..page_count {
            if idx + 1 >= self.u32_data.len() {
                break;
            }
            
            let _page_index = self.u32_data[idx];
            let line_count = self.u32_data[idx + 1] as usize;
            idx += 2;
            
            for line_idx in 0..line_count {
                if idx + U32_PER_LINE > self.u32_data.len() {
                    break;
                }
                
                let text_offset = self.u32_data[idx] as usize;
                let text_length = self.u32_data[idx + 1] as usize;
                let marker_offset = self.u32_data[idx + 6] as usize;
                let marker_length = self.u32_data[idx + 7] as usize;
                
                // Validate text range
                debug_assert!(
                    text_offset + text_length <= text_len,
                    "Invalid text range for page {}, line {}: offset {} + length {} > text buffer size {}",
                    page_idx, line_idx, text_offset, text_length, text_len
                );
                
                // Validate marker range (only if marker is present)
                if marker_length > 0 {
                    debug_assert!(
                        marker_offset + marker_length <= text_len,
                        "Invalid marker range for page {}, line {}: offset {} + length {} > text buffer size {}",
                        page_idx, line_idx, marker_offset, marker_length, text_len
                    );
                }
                
                idx += U32_PER_LINE;
            }
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
        selection_range: Option<(usize, usize)>,
        styles: &[(usize, usize, u32)], // (start, len, font_id)
    ) {
        // Write text to buffer and record offset
        let text_offset = self.text_data.len() as u32;
        self.text_data.extend_from_slice(text.as_bytes());
        let text_len = text.len() as u32;

        // Calculate UTF-16 offset and length for efficient JS decoding
        let text_utf16_offset = self.utf16_text_offset as u32;
        let text_utf16_len = text.chars().map(|c| c.len_utf16()).sum::<usize>() as u32;
        self.utf16_text_offset += text_utf16_len as usize;

        // Validate text offset range (debug builds only)
        debug_assert!(
            (text_offset as u64) + (text_len as u64) <= u32::MAX as u64,
            "Text offset + length overflow: {} + {} > u32::MAX",
            text_offset, text_len
        );

        // List marker (stored after main text)
        // Note: marker_offset is always written even if marker_len=0, but decoder must check marker_len before reading
        let marker_offset = self.text_data.len() as u32;
        let (marker_len, marker_utf16_offset, marker_utf16_len) = if let Some(marker) = list_marker {
            self.text_data.extend_from_slice(marker.as_bytes());
            let m_len = marker.len() as u32;
            let m_utf16_offset = self.utf16_text_offset as u32;
            let m_utf16_len = marker.chars().map(|c| c.len_utf16()).sum::<usize>() as u32;
            self.utf16_text_offset += m_utf16_len as usize;
            (m_len, m_utf16_offset, m_utf16_len)
        } else {
            (0, 0, 0)  // No marker: offsets are ignored by decoder when len=0
        };

        // Validate marker offset range (debug builds only)
        debug_assert!(
            (marker_offset as u64) + (marker_len as u64) <= u32::MAX as u64,
            "Marker offset + length overflow: {} + {} > u32::MAX",
            marker_offset, marker_len
        );

        // Parse selection range or use MAX for none
        let (sel_start, sel_end) = selection_range
            .map(|(s, e)| (s as u32, e as u32))
            .unwrap_or((u32::MAX, u32::MAX));
            
        // Write styles
        let style_start_idx = self.style_data.len() as u32;
        let style_count = styles.len() as u32;
        
        for &(start, len, font_id) in styles {
            self.style_data.push(start as u32);
            self.style_data.push(len as u32);
            self.style_data.push(font_id);
        }

        // u32: text_offset, text_len, text_utf16_offset, text_utf16_len,
        //      block_type, flags, marker_offset, marker_len, marker_utf16_offset, marker_utf16_len,
        //      sel_start, sel_end, style_start_idx, style_count
        self.u32_data.push(text_offset);
        self.u32_data.push(text_len);
        self.u32_data.push(text_utf16_offset);
        self.u32_data.push(text_utf16_len);
        self.u32_data.push(block_type);
        self.u32_data.push(flags);
        self.u32_data.push(marker_offset);
        self.u32_data.push(marker_len);
        self.u32_data.push(marker_utf16_offset);
        self.u32_data.push(marker_utf16_len);
        self.u32_data.push(sel_start);
        self.u32_data.push(sel_end);
        self.u32_data.push(style_start_idx);
        self.u32_data.push(style_count);

        // f32: x, y
        self.f32_data.push(x);
        self.f32_data.push(y);
    }

    /// Set pending cursor data (will be written to buffers in finalize())
    /// This ensures cursor offset is always correct, regardless of call order
    pub fn write_cursor(&mut self, x: f32, y: f32, height: f32, page_index: usize, utf16_offset_in_line: usize) {
        self.pending_cursor = Some(PendingCursor {
            x,
            y,
            height,
            page_index,
            utf16_offset_in_line,
        });
    }

    /// Add pending selection rectangle (will be written to buffers in finalize())
    /// This ensures selection offset is always correct, regardless of call order
    pub fn write_selection(&mut self, x: f32, y: f32, width: f32, height: f32, page_index: usize) {
        self.pending_selections.push(PendingSelection {
            x,
            y,
            width,
            height,
            page_index,
        });
    }

    // Accessors for WASM
    // Return u32 instead of usize for explicit WASM contract (wasm32 linear memory uses u32 offsets)

    pub fn u32_ptr(&self) -> u32 {
        self.u32_data.as_ptr() as u32
    }

    pub fn u32_len(&self) -> u32 {
        self.u32_data.len() as u32
    }

    pub fn f32_ptr(&self) -> u32 {
        self.f32_data.as_ptr() as u32
    }

    pub fn f32_len(&self) -> u32 {
        self.f32_data.len() as u32
    }

    pub fn text_ptr(&self) -> u32 {
        self.text_data.as_ptr() as u32
    }

    pub fn text_len(&self) -> u32 {
        self.text_data.len() as u32
    }

    pub fn style_ptr(&self) -> u32 {
        self.style_data.as_ptr() as u32
    }

    pub fn style_len(&self) -> u32 {
        self.style_data.len() as u32
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
        buf.write_line(96.0, 96.0, "Hello", BLOCK_PARAGRAPH, 0, None, None, &[]);
        buf.set_line_count(line_idx, 1);
        buf.finalize();

        assert_eq!(buf.u32_data[0], MAGIC); // magic
        assert_eq!(buf.u32_data[1], SCHEMA_VERSION); // schema version
        assert_eq!(buf.u32_data[2], 42); // version_lo
        assert_eq!(buf.u32_data[4], 1);  // page_count
        assert_eq!(buf.text_data, b"Hello");
    }

    #[test]
    fn test_render_buffer_with_cursor() {
        let mut buf = RenderBuffer::new();
        buf.write_header(1, 1);
        buf.write_cursor(100.0, 200.0, 20.0, 0, 5); // page 0, utf16 offset 5
        buf.finalize();

        assert_eq!(buf.u32_data[5], 1); // cursor_present
        
        // Check offset table points to cursor data
        let cursor_offset = buf.u32_data[8] as usize;
        assert_eq!(cursor_offset, HEADER_SIZE); // Cursor starts right after header
        
        // Cursor indices at offset
        assert_eq!(buf.u32_data[cursor_offset], 0); // page_index
        assert_eq!(buf.u32_data[cursor_offset + 1], 5); // utf16_offset_in_line
        
        // f32 data should have cursor geometry: x, y, height
        assert_eq!(buf.f32_data[0], 100.0); // x
        assert_eq!(buf.f32_data[1], 200.0); // y
        assert_eq!(buf.f32_data[2], 20.0);  // height
    }

    #[test]
    fn test_render_buffer_with_selections() {
        let mut buf = RenderBuffer::new();
        buf.write_header(1, 1);
        
        // Write two selections (count is automatic)
        buf.write_selection(10.0, 20.0, 100.0, 15.0, 0); // page 0
        buf.write_selection(50.0, 60.0, 200.0, 15.0, 1); // page 1
        buf.finalize();

        assert_eq!(buf.u32_data[6], 2); // selection_count
        
        // Check offset table points to selection data
        let selection_offset = buf.u32_data[9] as usize;
        assert_eq!(selection_offset, HEADER_SIZE); // Selections start right after header
        
        // Selection indices at offset: page_index for each
        assert_eq!(buf.u32_data[selection_offset], 0); // first selection page_index
        assert_eq!(buf.u32_data[selection_offset + 1], 1); // second selection page_index
        
        // f32 data: geometry for each selection
        // First selection
        assert_eq!(buf.f32_data[0], 10.0);  // x
        assert_eq!(buf.f32_data[1], 20.0);  // y
        assert_eq!(buf.f32_data[2], 100.0); // width
        assert_eq!(buf.f32_data[3], 15.0);  // height
        // Second selection
        assert_eq!(buf.f32_data[4], 50.0);  // x
        assert_eq!(buf.f32_data[5], 60.0);  // y
        assert_eq!(buf.f32_data[6], 200.0); // width
        assert_eq!(buf.f32_data[7], 15.0);  // height
    }

    #[test]
    fn test_prepare_prevents_reallocation() {
        let mut buf = RenderBuffer::new();
        
        // Estimate sizes for: 2 pages, 100 lines total, 1 cursor, 5 selections
        let page_count = 2;
        let line_count = 100;
        let cursor_count = 1;
        let selection_count = 5;
        let avg_text_len = 50;
        
        let u32_needed = HEADER_SIZE + page_count * 2 + line_count * U32_PER_LINE + 
                         cursor_count * U32_PER_CURSOR + selection_count * U32_PER_SELECTION;
        let f32_needed = page_count * 3 + line_count * 2 + 
                         cursor_count * F32_PER_CURSOR + selection_count * F32_PER_SELECTION;
        let text_needed = line_count * avg_text_len;
        
        buf.prepare(u32_needed, f32_needed, text_needed);
        
        // Capture initial capacities
        let u32_capacity = buf.u32_data.capacity();
        let f32_capacity = buf.f32_data.capacity();
        let text_capacity = buf.text_data.capacity();
        
        // Write header
        buf.write_header(1, page_count as u32);
        
        // Write pages and lines
        for p in 0..page_count {
            let line_idx = buf.begin_page(p, 0.0, 816.0, 1056.0);
            
            for _ in 0..50 {
                buf.write_line(96.0, 96.0, "Hello, World! This is a test line with some text.", BLOCK_PARAGRAPH, 0, None, None, &[]);
            }
            
            buf.set_line_count(line_idx, 50);
        }
        
        // Write cursor and selections (count is automatic)
        buf.write_cursor(100.0, 200.0, 20.0, 0, 5);
        for i in 0..selection_count {
            buf.write_selection(10.0, 20.0, 100.0, 15.0, i);
        }
        
        buf.finalize();
        
        // Verify no reallocation occurred
        assert_eq!(buf.u32_data.capacity(), u32_capacity, "u32_data was reallocated");
        assert_eq!(buf.f32_data.capacity(), f32_capacity, "f32_data was reallocated");
        assert_eq!(buf.text_data.capacity(), text_capacity, "text_data was reallocated");
    }

    #[test]
    fn test_automatic_count_synchronization() {
        let mut buf = RenderBuffer::new();
        buf.write_header(1, 2);
        
        // Write selections WITHOUT manually calling set_selection_count
        buf.write_selection(10.0, 20.0, 100.0, 15.0, 0);
        buf.write_selection(20.0, 30.0, 150.0, 20.0, 0);
        buf.write_selection(30.0, 40.0, 200.0, 25.0, 1);
        
        // Before finalize, header might not be synced
        // After finalize, count should be automatic
        buf.finalize();
        
        // Check that selection_count was automatically set to 3
        assert_eq!(buf.u32_data[6], 3, "Selection count should be automatically set to 3");
        
        // Check selection offset was set
        assert_eq!(buf.u32_data[9], HEADER_SIZE as u32, "Selection offset should point to first selection");
    }

    #[test]
    fn test_cursor_flag_synchronization() {
        let mut buf = RenderBuffer::new();
        buf.write_header(1, 1);
        
        // Write cursor WITHOUT manually calling set_cursor_present
        buf.write_cursor(100.0, 200.0, 20.0, 0, 5);
        
        buf.finalize();
        
        // Check that cursor_present was automatically set
        assert_eq!(buf.u32_data[5], 1, "Cursor present should be automatically set");
        
        // Check cursor offset was set
        assert_eq!(buf.u32_data[8], HEADER_SIZE as u32, "Cursor offset should point to cursor data");
    }

    #[test]
    fn test_cursor_offset_correct_regardless_of_call_order() {
        // This test verifies the fix for the critical bug:
        // write_cursor() can be called BEFORE pages are written, and offset will still be correct
        
        let mut buf = RenderBuffer::new();
        buf.write_header(1, 2);
        
        // Call write_cursor EARLY (before pages) - this was the bug scenario!
        buf.write_cursor(100.0, 200.0, 20.0, 0, 5);
        buf.write_selection(10.0, 20.0, 100.0, 15.0, 1);
        
        // Now write pages AFTER cursor/selection
        let line_idx = buf.begin_page(0, 0.0, 816.0, 1056.0);
        buf.write_line(96.0, 96.0, "First page line 1", BLOCK_PARAGRAPH, 0, None, None, &[]);
        buf.write_line(96.0, 120.0, "First page line 2", BLOCK_PARAGRAPH, 0, None, None, &[]);
        buf.set_line_count(line_idx, 2);
        
        let line_idx = buf.begin_page(1, 1056.0, 816.0, 1056.0);
        buf.write_line(96.0, 1152.0, "Second page line 1", BLOCK_PARAGRAPH, 0, None, None, &[]);
        buf.set_line_count(line_idx, 1);
        
        buf.finalize();

        // Cursor offset should point AFTER all pages/lines, not in the middle
        let cursor_offset = buf.u32_data[8] as usize;
        let expected_offset = HEADER_SIZE + 2 + 2 * U32_PER_LINE + 2 + 1 * U32_PER_LINE; // header + page1 + page2
        assert_eq!(cursor_offset, expected_offset, "Cursor offset should point after all pages");
        
        // Verify cursor data is at the correct location
        assert_eq!(buf.u32_data[cursor_offset], 0, "Cursor page_index");
        assert_eq!(buf.u32_data[cursor_offset + 1], 5, "Cursor utf16_offset");
        
        // Selection offset should point after cursor
        let selection_offset = buf.u32_data[9] as usize;
        assert_eq!(selection_offset, cursor_offset + U32_PER_CURSOR, "Selection offset should point after cursor");
        assert_eq!(buf.u32_data[selection_offset], 1, "Selection page_index");
    }

    #[test]
    fn test_f32_offset_table_for_random_access() {
        // This test verifies that f32 geometry offsets are correctly stored in header
        // allowing random access to cursor/selection geometry independent of page count
        
        let mut buf = RenderBuffer::new();
        buf.write_header(1, 2);
        
        // Write cursor and selection BEFORE pages
        buf.write_cursor(150.0, 250.0, 18.0, 0, 10);
        buf.write_selection(30.0, 40.0, 200.0, 20.0, 1);
        
        // Write pages with multiple lines (each line adds 2 f32 values)
        let line_idx = buf.begin_page(0, 0.0, 816.0, 1056.0);
        for _ in 0..5 {
            buf.write_line(96.0, 100.0, "Line with text", BLOCK_PARAGRAPH, 0, None, None, &[]);
        }
        buf.set_line_count(line_idx, 5);
        
        let line_idx = buf.begin_page(1, 1056.0, 816.0, 1056.0);
        for _ in 0..3 {
            buf.write_line(96.0, 1100.0, "Another line", BLOCK_PARAGRAPH, 0, None, None, &[]);
        }
        buf.set_line_count(line_idx, 3);
        
        buf.finalize();

        // Check f32 cursor offset in header[10]
        let f32_cursor_offset = buf.u32_data[10] as usize;
        // f32 layout: 2 pages * 3 floats + 8 lines * 2 floats = 6 + 16 = 22
        let expected_f32_cursor = 2 * 3 + 8 * 2;
        assert_eq!(f32_cursor_offset, expected_f32_cursor, "f32 cursor offset should point after all pages/lines geometry");
        
        // Verify cursor geometry is at f32_cursor_offset
        assert_eq!(buf.f32_data[f32_cursor_offset], 150.0, "Cursor x");
        assert_eq!(buf.f32_data[f32_cursor_offset + 1], 250.0, "Cursor y");
        assert_eq!(buf.f32_data[f32_cursor_offset + 2], 18.0, "Cursor height");
        
        // Check f32 selection offset in header[11]
        let f32_selection_offset = buf.u32_data[11] as usize;
        let expected_f32_selection = expected_f32_cursor + F32_PER_CURSOR;
        assert_eq!(f32_selection_offset, expected_f32_selection, "f32 selection offset should point after cursor geometry");
        
        // Verify selection geometry is at f32_selection_offset
        assert_eq!(buf.f32_data[f32_selection_offset], 30.0, "Selection x");
        assert_eq!(buf.f32_data[f32_selection_offset + 1], 40.0, "Selection y");
        assert_eq!(buf.f32_data[f32_selection_offset + 2], 200.0, "Selection width");
        assert_eq!(buf.f32_data[f32_selection_offset + 3], 20.0, "Selection height");
        
        // Verify u32 offsets are also correct
        let u32_cursor_offset = buf.u32_data[8] as usize;
        assert_eq!(buf.u32_data[u32_cursor_offset], 0, "Cursor page_index in u32");
        assert_eq!(buf.u32_data[u32_cursor_offset + 1], 10, "Cursor utf16_offset in u32");
        
        let u32_selection_offset = buf.u32_data[9] as usize;
        assert_eq!(buf.u32_data[u32_selection_offset], 1, "Selection page_index in u32");
    }

    #[test]
    fn test_utf16_offsets_for_batch_decode() {
        let mut buf = RenderBuffer::new();
        buf.write_header(42, 0);
        
        // Page 1
        let line_count_idx = buf.begin_page(0, 0.0, 800.0, 1200.0);
        
        // Line 1: ASCII text (1 byte = 1 UTF-16 code unit)
        buf.write_line(0.0, 0.0, "Hello World", 0, 0, None, None, &[]);
        
        // Line 2: Text with emoji (4 bytes = 2 UTF-16 code units)
        // "Test 😀 emoji" = "Test " (5) + 😀 (2 UTF-16) + " emoji" (6) = 13 UTF-16 units
        buf.write_line(0.0, 20.0, "Test 😀 emoji", 0, 0, None, None, &[]);
        
        // Line 3: Text with Cyrillic (2 bytes = 1 UTF-16 code unit)
        // "Привет мир" = 10 chars, each 1 UTF-16 unit = 10 UTF-16 units
        buf.write_line(0.0, 40.0, "Привет мир", 0, 0, None, None, &[]);
        
        buf.set_line_count(line_count_idx, 3);
        buf.finalize();
        
        // Verify UTF-16 offsets are cumulative
        // Line 1: starts at 0, length 11 ("Hello World")
        assert_eq!(buf.u32_data[HEADER_SIZE + 2 + 2], 0, "Line 1 utf16 offset");
        assert_eq!(buf.u32_data[HEADER_SIZE + 2 + 3], 11, "Line 1 utf16 len");
        
        // Line 2: starts at 11, length 13 ("Test 😀 emoji" = 5 + 2 + 6)
        assert_eq!(buf.u32_data[HEADER_SIZE + 2 + U32_PER_LINE + 2], 11, "Line 2 utf16 offset");
        assert_eq!(buf.u32_data[HEADER_SIZE + 2 + U32_PER_LINE + 3], 13, "Line 2 utf16 len (emoji is 2 UTF-16 units)");
        
        // Line 3: starts at 24 (11 + 13), length 10 ("Привет мир")
        assert_eq!(buf.u32_data[HEADER_SIZE + 2 + U32_PER_LINE * 2 + 2], 24, "Line 3 utf16 offset");
        assert_eq!(buf.u32_data[HEADER_SIZE + 2 + U32_PER_LINE * 2 + 3], 10, "Line 3 utf16 len");
        
        // Verify cumulative offset after all lines
        assert_eq!(buf.utf16_text_offset, 34, "Total UTF-16 offset should be 11 + 13 + 10");
    }
}
