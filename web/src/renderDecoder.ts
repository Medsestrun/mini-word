/**
 * Zero-copy decoder for WASM render buffer protocol
 * 
 * Binary format:
 * 
 * u32 buffer:
 * Header (offset table for random access):
 *   [0] MAGIC (0x4D575244 = "MWRD" for validation)
 *   [1] SCHEMA_VERSION (protocol version, currently 1)
 *   [2] version_lo (document version)
 *   [3] version_hi (document version)
 *   [4] page_count
 *   [5] cursor_present (0 or 1)
 *   [6] selection_count
 *   [7] text_buffer_len
 *   [8] u32_cursor_offset (index where cursor indices start, 0 if no cursor)
 *   [9] u32_selection_offset (index where selection indices start, 0 if no selections)
 *   [10] f32_cursor_offset (index where cursor geometry starts, 0 if no cursor)
 *   [11] f32_selection_offset (index where selection geometries start, 0 if no selections)
 * 
 * Per page (starts at index 12):
 *   - page_index
 *   - line_count
 *   - per line: [text_offset, text_len, text_utf16_offset, text_utf16_len,
 *               block_type, flags, marker_offset, marker_len, marker_utf16_offset, marker_utf16_len,
 *               sel_start, sel_end]
 *     text_offset/text_len: byte offsets in UTF-8 buffer (for validation)
 *     text_utf16_offset/text_utf16_len: offsets for JS substring (after single decode)
 *     marker: only read if marker_len > 0, otherwise marker_offset is ignored
 *     sel_start/sel_end: UTF-16 offsets relative to line text start (0xFFFFFFFF if no selection)
 * 
 * At u32_cursor_offset (if cursor_present):
 *   - cursor indices: [page_index, utf16_offset_in_line]
 * 
 * f32 buffer:
 * - per page: [y_offset, width, height]
 * - per line: [x, y]
 * - cursor geometry (if present): [x, y, height]
 */

// Protocol constants (must match Rust)
const MAGIC = 0x4D575244; // "MWRD" (MiniWoRD)
const SCHEMA_VERSION = 1;

// Block type opcodes (must match Rust)
const BLOCK_PARAGRAPH = 0;
const BLOCK_HEADING_1 = 1;
const BLOCK_HEADING_2 = 2;
const BLOCK_HEADING_3 = 3;
const BLOCK_HEADING_4 = 4;
const BLOCK_HEADING_5 = 5;
const BLOCK_HEADING_6 = 6;
const BLOCK_LIST_ITEM = 7;

// Flags
const FLAG_IS_HEADING = 0b0001;
const FLAG_IS_LIST_ITEM = 0b0010;

export interface RenderData {
  version: number;
  pages: PageRenderData[];
  cursor: CursorRenderData | null;
}

export interface PageRenderData {
  pageIndex: number;
  yOffset: number;
  width: number;
  height: number;
  lines: LineRenderData[];
}

export interface LineRenderData {
  x: number;
  y: number;
  text: string;
  blockType: string;
  isHeading: boolean;
  headingLevel: number | null;
  isListItem: boolean;
  listMarker: string | null;
  selectionStart: number | null;
  selectionEnd: number | null;
  styles: StyleSpan[];
}

export interface StyleSpan {
  start: number;
  len: number;
  fontId: number;
}

export interface CursorRenderData {
  x: number;
  y: number;
  height: number;
  pageIndex: number;
  /** UTF-16 code unit offset within the line for correct JS text measurement */
  utf16OffsetInLine: number;
}

const blockTypeToString = (blockType: number): string => {
  switch (blockType) {
    case BLOCK_PARAGRAPH: return 'paragraph';
    case BLOCK_HEADING_1: return 'heading-1';
    case BLOCK_HEADING_2: return 'heading-2';
    case BLOCK_HEADING_3: return 'heading-3';
    case BLOCK_HEADING_4: return 'heading-4';
    case BLOCK_HEADING_5: return 'heading-5';
    case BLOCK_HEADING_6: return 'heading-6';
    case BLOCK_LIST_ITEM: return 'list-item';
    default: return 'paragraph';
  }
};

const getHeadingLevel = (blockType: number): number | null => {
  if (blockType >= BLOCK_HEADING_1 && blockType <= BLOCK_HEADING_6) {
    return blockType - BLOCK_HEADING_1 + 1;
  }
  return null;
};

/**
 * Reusable TextDecoder instance (created once, reused for all decodes)
 * PERFORMANCE: Avoids creating new TextDecoder on every render frame
 */
const textDecoder = new TextDecoder('utf-8');

/**
 * Decode render data from WASM memory buffers
 */
export const decodeRenderData = (
  memory: WebAssembly.Memory,
  u32Ptr: number,
  u32Len: number,
  f32Ptr: number,
  f32Len: number,
  textPtr: number,
  textLen: number,
  stylePtr: number,
  styleLen: number
): RenderData => {
  // Create views into WASM memory
  const u32View = new Uint32Array(memory.buffer, u32Ptr, u32Len);
  const f32View = new Float32Array(memory.buffer, f32Ptr, f32Len);
  const textView = new Uint8Array(memory.buffer, textPtr, textLen);
  const styleView = new Uint32Array(memory.buffer, stylePtr, styleLen);

  // Validate header
  const magic = u32View[0];
  if (magic !== MAGIC) {
    throw new Error(`Invalid render buffer: expected magic 0x${MAGIC.toString(16)}, got 0x${magic.toString(16)}`);
  }
  
  const schemaVersion = u32View[1];
  if (schemaVersion !== SCHEMA_VERSION) {
    throw new Error(`Incompatible schema version: expected ${SCHEMA_VERSION}, got ${schemaVersion}`);
  }
  
  // Read header with offset table
  const versionLo = u32View[2];
  const versionHi = u32View[3];
  const version = versionLo + versionHi * 0x100000000;
  const pageCount = u32View[4];
  const cursorPresent = u32View[5] === 1;
  // const selectionCount = u32View[6]; // Deprecated
  // u32View[7] is text_buffer_len
  const u32CursorOffset = u32View[8];
  // const u32SelectionOffset = u32View[9]; // Deprecated
  const f32CursorOffset = u32View[10];
  // const f32SelectionOffset = u32View[11]; // Deprecated

  let u32Idx = 12; // Pages start after header
  let f32Idx = 0;

  // PERFORMANCE: Decode entire text buffer once, then use substring for each line
  // This is MUCH faster than decoding per-line (1 decode vs N decodes)
  const fullText = textDecoder.decode(textView);

  const pages: PageRenderData[] = [];

  // Decode pages
  for (let p = 0; p < pageCount; p++) {
    const pageIndex = u32View[u32Idx++];
    const lineCount = u32View[u32Idx++];

    const yOffset = f32View[f32Idx++];
    const pageWidth = f32View[f32Idx++];
    const pageHeight = f32View[f32Idx++];

    const lines: LineRenderData[] = [];

    for (let l = 0; l < lineCount; l++) {
      // Read all 14 u32 values per line (was 12)
      u32Idx++;  // skip text_offset
      u32Idx++;  // skip text_length
      const textUtf16Offset = u32View[u32Idx++];
      const textUtf16Len = u32View[u32Idx++];
      const blockType = u32View[u32Idx++];
      const flags = u32View[u32Idx++];
      u32Idx++;  // skip marker_offset
      u32Idx++;  // skip marker_len
      const markerUtf16Offset = u32View[u32Idx++];
      const markerUtf16Len = u32View[u32Idx++];
      const selStart = u32View[u32Idx++];
      const selEnd = u32View[u32Idx++];
      const styleStartIdx = u32View[u32Idx++];
      const styleCount = u32View[u32Idx++];

      const x = f32View[f32Idx++];
      const y = f32View[f32Idx++];

      // PERFORMANCE: Use substring instead of decode (much faster)
      const text = fullText.substring(textUtf16Offset, textUtf16Offset + textUtf16Len);
      
      // Extract list marker using substring if present
      const listMarker = markerUtf16Len > 0 
        ? fullText.substring(markerUtf16Offset, markerUtf16Offset + markerUtf16Len)
        : null;

      const isHeading = (flags & FLAG_IS_HEADING) !== 0;
      const isListItem = (flags & FLAG_IS_LIST_ITEM) !== 0;

      // Check for valid selection range (u32::MAX = 0xFFFFFFFF)
      const hasSelection = selStart !== 0xFFFFFFFF;

      // Decode styles
      const styles: StyleSpan[] = [];
      if (styleCount > 0) {
        let sIdx = styleStartIdx;
        for (let s = 0; s < styleCount; s++) {
          const start = styleView[sIdx++];
          const len = styleView[sIdx++];
          const fontId = styleView[sIdx++];
          styles.push({ start, len, fontId });
        }
      }

      lines.push({
        x,
        y,
        text,
        blockType: blockTypeToString(blockType),
        isHeading,
        headingLevel: getHeadingLevel(blockType),
        isListItem,
        listMarker,
        selectionStart: hasSelection ? selStart : null,
        selectionEnd: hasSelection ? selEnd : null,
        styles,
      });
    }

    pages.push({
      pageIndex,
      yOffset,
      width: pageWidth,
      height: pageHeight,
      lines,
    });
  }

  // Decode cursor using offset table (random access for both u32 and f32)
  let cursor: CursorRenderData | null = null;
  if (cursorPresent && u32CursorOffset > 0 && f32CursorOffset > 0) {
    // u32: indices at u32CursorOffset
    const pageIndex = u32View[u32CursorOffset];
    const utf16OffsetInLine = u32View[u32CursorOffset + 1];
    
    // f32: geometry at f32CursorOffset (random access, not sequential)
    const x = f32View[f32CursorOffset];
    const y = f32View[f32CursorOffset + 1];
    const height = f32View[f32CursorOffset + 2];
    
    cursor = { x, y, height, pageIndex, utf16OffsetInLine };
  }

  return {
    version,
    pages,
    cursor,
  };
};

/**
 * Helper to get render data from a WasmEditor instance
 */
export const getRenderDataFromEditor = (
  editor: WasmEditorInterface,
  memory: WebAssembly.Memory,
  viewportY: number,
  viewportHeight: number
): RenderData => {
  // Build render data into buffers
  editor.buildRenderData(viewportY, viewportHeight);

  // Get buffer info
  const u32Ptr = editor.getU32Ptr();
  const u32Len = editor.getU32Len();
  const f32Ptr = editor.getF32Ptr();
  const f32Len = editor.getF32Len();
  const textPtr = editor.getTextPtr();
  const textLen = editor.getTextLen();
  const stylePtr = editor.getStylePtr();
  const styleLen = editor.getStyleLen();

  return decodeRenderData(memory, u32Ptr, u32Len, f32Ptr, f32Len, textPtr, textLen, stylePtr, styleLen);
};

/**
 * Interface for WasmEditor with the new buffer API
 */
export interface WasmEditorInterface {
  // Editing methods
  insertText(text: string): void;
  deleteBackward(): boolean;
  deleteForward(): boolean;
  moveCursor(horizontal: number, vertical: number, extendSelection: boolean): void;
  setCursor(pageIndex: number, x: number, y: number): void;
  selectTo(pageIndex: number, x: number, y: number): void;
  undo(): boolean;
  redo(): boolean;
  setFontMetrics(lineHeight: number, charWidths: Float32Array, defaultWidth: number): void;
  getText(): string;
  getPageCount(): number;
  selectAll(): void;
  clearSelection(): void;
  insertParagraph(): void;

  // Buffer API
  buildRenderData(viewportY: number, viewportHeight: number): void;
  getU32Ptr(): number;
  getU32Len(): number;
  getF32Ptr(): number;
  getF32Len(): number;
  getTextPtr(): number;
  getTextLen(): number;
  getStylePtr(): number;
  getStyleLen(): number;

  // Font/Style methods
  addFont(id: number, lineHeight: number, charWidths: Float32Array, defaultWidth: number): void;
  formatSelection(fontId: number): void;

  // Direct layout constraint accessors
  getPageWidth(): number;
  getPageHeight(): number;
  getMarginTop(): number;
  getMarginBottom(): number;
  getMarginLeft(): number;
  getMarginRight(): number;
  getContentWidth(): number;
  getContentHeight(): number;

  // Cursor info
  getCursorParaId(): bigint;
  getCursorOffset(): number;
  hasSelection(): boolean;
}

export interface LayoutConstraints {
  pageWidth: number;
  pageHeight: number;
  marginTop: number;
  marginBottom: number;
  marginLeft: number;
  marginRight: number;
  contentWidth: number;
  contentHeight: number;
}

/**
 * Get layout constraints from editor
 */
export const getLayoutConstraints = (editor: WasmEditorInterface): LayoutConstraints => ({
  pageWidth: editor.getPageWidth(),
  pageHeight: editor.getPageHeight(),
  marginTop: editor.getMarginTop(),
  marginBottom: editor.getMarginBottom(),
  marginLeft: editor.getMarginLeft(),
  marginRight: editor.getMarginRight(),
  contentWidth: editor.getContentWidth(),
  contentHeight: editor.getContentHeight(),
});
