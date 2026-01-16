# Architecture

This document describes the internal architecture of Mini-Word, a high-performance WYSIWYG text editor.

## Design Principles

1. **Incremental Layout** — Editing a single character must not trigger full document relayout
2. **Diff-Based Rendering** — Only emit changes, not full state, for efficient UI updates
3. **Separation of Concerns** — Clear boundaries between document, layout, and rendering
4. **Zero-Copy WASM Bridge** — Minimize data copying across the JS/WASM boundary

---

## Document Model

### Rope Data Structure

The document uses a **rope** for text storage — a balanced binary tree where leaves contain text chunks.

```
        [Rope Node]
       /          \
   [Leaf]        [Node]
   "Hello "     /      \
            [Leaf]   [Leaf]
            "World"  "!"
```

**Benefits:**
- O(log n) insert/delete at any position
- Efficient for large documents (100K+ characters)
- Preserves memory locality in leaves

### Paragraph Index

A B-tree maps byte offsets to paragraph IDs for O(log n) paragraph lookup:

```rust
struct ParagraphIndex {
    // (ParagraphId, start_offset, byte_length)
    entries: BTreeMap<usize, (ParagraphId, usize)>,
}
```

### Block Metadata

Each paragraph has associated metadata:

```rust
enum BlockKind {
    Paragraph,
    Heading { level: u8 },
    ListItem { marker: ListMarker, indent_level: u8 },
}
```

---

## Layout Engine

### Incremental Strategy

The layout engine maintains a **dirty set** of paragraphs needing relayout:

```
Edit → Mark paragraphs dirty → Relayout only dirty paragraphs → Repaginate if heights changed
```

### Paragraph Layout

Each paragraph is broken into lines:

```rust
struct ParagraphLayout {
    lines: Vec<LineLayout>,
    total_height: f32,
}

struct LineLayout {
    byte_range: Range<usize>,
    clusters: Vec<ClusterInfo>,  // For cursor positioning
    height: f32,
    width: f32,
}
```

### Line Breaking

Uses Unicode line break opportunities (UAX #14) with a greedy algorithm:

1. Iterate grapheme clusters
2. Track accumulated width
3. Break at allowed positions when width exceeds content width
4. Handle explicit line breaks (`\n`)

### Pagination

Pages are computed by flowing lines until content height is exceeded:

```rust
struct PageLayout {
    page_index: usize,
    start_para: ParagraphId,
    start_line: usize,
    end_para: ParagraphId,
    end_line: usize,
}
```

---

## Rendering Pipeline

### Display List

Layout produces a **display list** — a flat list of render items:

```rust
enum DisplayItem {
    TextRun { id, position, text, block_kind },
    ListMarker { id, position, marker },
    Caret { position, height },
    SelectionRect { rect },
}
```

### Viewport Culling

Only pages intersecting the viewport are included in the display list:

```rust
let first_visible = (viewport.y / page_height).floor();
let last_visible = ((viewport.y + viewport.height) / page_height).ceil();
```

### Render Diff (Future)

The diff engine compares consecutive display lists:

```rust
enum RenderPatch {
    Add { id, item },
    Update { id, item },
    Remove { id },
}
```

---

## Undo/Redo System

### Transaction Model

Edits are grouped into **transactions** for atomic undo:

```rust
struct Transaction {
    id: u64,
    description: String,
    operations: Vec<EditOp>,
    reverse_operations: Vec<EditOp>,
    cursor_before: Cursor,
    selection_before: Option<Selection>,
}
```

### Operation Types

```rust
enum EditOp {
    Insert { position: AbsoluteOffset, text: String },
    Delete { start: AbsoluteOffset, end: AbsoluteOffset },
    Replace { start: AbsoluteOffset, end: AbsoluteOffset, text: String },
}
```

### Reverse Computation

Each operation has an inverse computed at apply time:

| Operation | Reverse |
|-----------|---------|
| Insert "abc" at 5 | Delete 5..8 |
| Delete 5..8 (was "abc") | Insert "abc" at 5 |

---

## WASM Bridge

### Zero-Copy Flat Buffer Protocol

Instead of using JSON serialization via `serde-wasm-bindgen`, render data uses flat typed arrays for zero-copy transfer:

```rust
pub struct RenderBuffer {
    /// Integer data (indices, counts, offsets, opcodes)
    pub u32_data: Vec<u32>,
    /// Float data (positions, dimensions)
    pub f32_data: Vec<f32>,
    /// UTF-8 text buffer
    pub text_data: Vec<u8>,
}
```

### Binary Protocol Layout

**u32 Buffer Header:**
```text
[0] version_lo
[1] version_hi
[2] page_count
[3] cursor_present (0 or 1)
[4] selection_count
[5] text_buffer_len
```

**Per-page data (u32):**
```text
page_index
line_count
per-line: [text_offset, text_len, block_type, flags, marker_offset, marker_len]
```

**f32 Buffer:**
```text
per-page: [y_offset, width, height]
per-line: [x, y]
cursor (if present): [x, y, height, page_index]
per-selection: [x, y, width, height, page_index]
```

### API Surface

```rust
#[wasm_bindgen]
impl WasmEditor {
    // Editing
    pub fn new() -> Self;
    pub fn insert_text(&mut self, text: &str);
    pub fn delete_backward(&mut self) -> bool;
    pub fn delete_forward(&mut self) -> bool;
    pub fn move_cursor(&mut self, h: i32, v: i32, extend: bool);
    pub fn undo(&mut self) -> bool;
    pub fn redo(&mut self) -> bool;
    pub fn get_text(&self) -> String;
    pub fn get_page_count(&self) -> usize;
    
    // Zero-copy buffer API
    pub fn build_render_data(&mut self, viewport_y: f32, viewport_height: f32);
    pub fn get_u32_ptr(&self) -> *const u32;
    pub fn get_u32_len(&self) -> usize;
    pub fn get_f32_ptr(&self) -> *const f32;
    pub fn get_f32_len(&self) -> usize;
    pub fn get_text_ptr(&self) -> *const u8;
    pub fn get_text_len(&self) -> usize;
    
    // Direct constraint accessors (no serialization)
    pub fn get_page_width(&self) -> f32;
    pub fn get_page_height(&self) -> f32;
    // ... etc
}
```

### TypeScript Decoder

The frontend uses a decoder that reads directly from WASM memory:

```typescript
const decodeRenderData = (
  memory: WebAssembly.Memory,
  u32Ptr: number,
  u32Len: number,
  f32Ptr: number,
  f32Len: number,
  textPtr: number,
  textLen: number
): RenderData => {
  const u32View = new Uint32Array(memory.buffer, u32Ptr, u32Len);
  const f32View = new Float32Array(memory.buffer, f32Ptr, f32Len);
  const textView = new Uint8Array(memory.buffer, textPtr, textLen);
  // ... decode
};
```

---

## React Frontend

### Component Hierarchy

```
App
├── Toolbar
│   └── ToolbarButton (undo, redo)
├── DocumentContainer
│   └── EditorArea (focusable)
│       └── Page (per visible page)
│           ├── SelectionRect[]
│           ├── TextLine[]
│           └── Cursor
└── StatusBar
```

### Rendering Strategy

1. On mount: Load WASM, create editor instance
2. On keydown: Call editor methods, get render data, setState
3. React re-renders only changed components

### Cursor Blink

CSS animation with visibility toggle:

```css
.cursor {
    animation: blink 1.06s step-end infinite;
}

@keyframes blink {
    50% { opacity: 0; }
}
```

---

## Performance Optimizations

### Current

- Rope for O(log n) text operations
- Paragraph-level layout caching
- Viewport culling for render
- React key-based reconciliation

### Planned

- Render diff to minimize DOM updates
- Virtual scrolling for very long documents
- Web Workers for background layout
- SharedArrayBuffer for zero-copy WASM bridge

---

## Data Flow

```
User Input (keydown)
       │
       ▼
┌─────────────────┐
│  React Handler  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  WasmEditor.*   │◄──── JavaScript
├─────────────────┤
│  Editor.*       │◄──── WASM boundary
└────────┬────────┘
         │
    ┌────┴────┐
    │         │
    ▼         ▼
┌───────┐ ┌────────┐
│ Undo  │ │Document│
│Manager│ │.apply()│
└───────┘ └───┬────┘
              │
              ▼
        ┌───────────┐
        │EditResult │
        │(affected  │
        │paragraphs)│
        └─────┬─────┘
              │
              ▼
        ┌───────────┐
        │ Layout    │
        │.invalidate│
        │.relayout()│
        └─────┬─────┘
              │
              ▼
        ┌───────────┐
        │DisplayList│
        │.build()   │
        └─────┬─────┘
              │
              ▼
        ┌───────────┐
        │RenderData │◄──── Serialized to JS
        └─────┬─────┘
              │
              ▼
        ┌───────────┐
        │React      │
        │setState() │
        └───────────┘
```

---

## Future Considerations

### Rich Text

Block-level formatting (headings, lists) is supported. Inline formatting (bold, italic) would require:

1. Span-based formatting model
2. Extended `DisplayItem::TextRun` with style info
3. CSS class mapping in React

### Collaborative Editing

The rope structure is compatible with CRDTs. Integration would require:

1. Operation-based CRDT (e.g., RGA)
2. Vector clocks for ordering
3. WebSocket sync layer

### Native Desktop

The same Rust core can power a native app:

1. Replace WASM bridge with direct FFI
2. Use GPU renderer (wgpu, skia-safe)
3. Native windowing (winit, tauri)
