/**
 * Zero-copy decoder for WASM render buffer protocol
 * 
 * Binary format:
 * 
 * u32 buffer header:
 * [0] version_lo
 * [1] version_hi
 * [2] page_count
 * [3] cursor_present (0 or 1)
 * [4] selection_count
 * [5] text_buffer_len
 * 
 * Per page in u32:
 * - page_index
 * - line_count
 * - per line: [text_offset, text_len, block_type, flags, marker_offset, marker_len]
 * 
 * f32 buffer:
 * - per page: [y_offset, width, height]
 * - per line: [x, y]
 * - cursor (if present): [x, y, height, page_index]
 * - per selection: [x, y, width, height, page_index]
 */

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
  selections: SelectionRenderData[];
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
}

export interface CursorRenderData {
  x: number;
  y: number;
  height: number;
  pageIndex: number;
  /** Character offset within the line for frontend text measurement */
  lineCharOffset: number;
}

export interface SelectionRenderData {
  x: number;
  y: number;
  width: number;
  height: number;
  pageIndex: number;
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
    return blockType; // 1-6
  }
  return null;
};

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
  textLen: number
): RenderData => {
  // Create views into WASM memory
  const u32View = new Uint32Array(memory.buffer, u32Ptr, u32Len);
  const f32View = new Float32Array(memory.buffer, f32Ptr, f32Len);
  const textView = new Uint8Array(memory.buffer, textPtr, textLen);

  // Decode UTF-8 text once
  const textDecoder = new TextDecoder('utf-8');

  // Read header
  const versionLo = u32View[0];
  const versionHi = u32View[1];
  const version = versionLo + versionHi * 0x100000000;
  const pageCount = u32View[2];
  const cursorPresent = u32View[3] === 1;
  const selectionCount = u32View[4];
  // u32View[5] is text_buffer_len (we already know this from textLen)

  let u32Idx = 6;
  let f32Idx = 0;

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
      const textOffset = u32View[u32Idx++];
      const textLength = u32View[u32Idx++];
      const blockType = u32View[u32Idx++];
      const flags = u32View[u32Idx++];
      const markerOffset = u32View[u32Idx++];
      const markerLen = u32View[u32Idx++];

      const x = f32View[f32Idx++];
      const y = f32View[f32Idx++];

      // Decode text from buffer
      const text = textDecoder.decode(textView.subarray(textOffset, textOffset + textLength));
      
      // Decode list marker if present
      const listMarker = markerLen > 0 
        ? textDecoder.decode(textView.subarray(markerOffset, markerOffset + markerLen))
        : null;

      const isHeading = (flags & FLAG_IS_HEADING) !== 0;
      const isListItem = (flags & FLAG_IS_LIST_ITEM) !== 0;

      lines.push({
        x,
        y,
        text,
        blockType: blockTypeToString(blockType),
        isHeading,
        headingLevel: getHeadingLevel(blockType),
        isListItem,
        listMarker,
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

  // Decode cursor
  let cursor: CursorRenderData | null = null;
  if (cursorPresent) {
    cursor = {
      x: f32View[f32Idx++],
      y: f32View[f32Idx++],
      height: f32View[f32Idx++],
      pageIndex: Math.floor(f32View[f32Idx++]),
      lineCharOffset: Math.floor(f32View[f32Idx++]),
    };
  }

  // Decode selections
  const selections: SelectionRenderData[] = [];
  for (let s = 0; s < selectionCount; s++) {
    selections.push({
      x: f32View[f32Idx++],
      y: f32View[f32Idx++],
      width: f32View[f32Idx++],
      height: f32View[f32Idx++],
      pageIndex: Math.floor(f32View[f32Idx++]),
    });
  }

  return {
    version,
    pages,
    cursor,
    selections,
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

  return decodeRenderData(memory, u32Ptr, u32Len, f32Ptr, f32Len, textPtr, textLen);
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
  undo(): boolean;
  redo(): boolean;
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
  getCursorParaId(): number;
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
