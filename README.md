# Mini-Word

A high-performance WYSIWYG text editor built in Rust with a React web frontend via WebAssembly.

## Features

- **Incremental Layout Engine** — Only affected paragraphs are recalculated on edits
- **Rope-based Document Model** — O(log n) text operations for large documents
- **Paginated View** — US Letter format with proper margins
- **Full Undo/Redo** — Transaction-based history with grouping
- **Cross-platform** — Runs in any modern browser via WASM

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     React Frontend                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Toolbar   │  │  Page View  │  │    Status Bar       │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└────────────────────────────┬────────────────────────────────┘
                             │ JSON (serde-wasm-bindgen)
┌────────────────────────────▼────────────────────────────────┐
│                     WASM Bridge                             │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  WasmEditor: insertText, delete, moveCursor, etc.   │    │
│  └─────────────────────────────────────────────────────┘    │
└────────────────────────────┬────────────────────────────────┘
                             │
┌────────────────────────────▼────────────────────────────────┐
│                     Rust Core                               │
│  ┌───────────┐  ┌───────────┐  ┌───────────┐  ┌──────────┐  │
│  │ Document  │  │  Layout   │  │  Render   │  │   Undo   │  │
│  │   Model   │  │  Engine   │  │   Diff    │  │  Manager │  │
│  └───────────┘  └───────────┘  └───────────┘  └──────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Core Modules

| Module | Description |
|--------|-------------|
| `document` | Rope-based text storage, paragraph indexing, block metadata |
| `editing` | Cursor, selection, edit operations |
| `layout` | Incremental layout engine, line breaking, pagination |
| `render` | Display list generation, diff computation |
| `undo` | Transaction-based undo/redo with operation merging |
| `wasm` | WebAssembly bindings for JavaScript interop |

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (1.70+)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/)
- [Node.js](https://nodejs.org/) (18+)

### Build & Run

```bash
# Clone the repository
git clone https://github.com/yourusername/mini-word.git
cd mini-word

# Build WASM module
wasm-pack build --target web --out-dir pkg

# Install web dependencies and start dev server
cd web
npm install
npm run dev
```

Open http://localhost:3000 in your browser.

### Run Tests

```bash
# Rust tests
cargo test

# WASM tests (requires wasm-pack)
wasm-pack test --headless --chrome
```

## Usage

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+Z` / `Cmd+Z` | Undo |
| `Ctrl+Y` / `Cmd+Shift+Z` | Redo |
| `Ctrl+A` / `Cmd+A` | Select all |
| `Backspace` | Delete backward |
| `Delete` | Delete forward |
| `Enter` | New paragraph |
| `Arrow keys` | Move cursor |
| `Shift+Arrow` | Extend selection |

### API (JavaScript)

```javascript
import init, { WasmEditor } from './pkg/mini_word.js';

await init();
const editor = new WasmEditor();

// Insert text
editor.insertText("Hello, World!");

// Get render data for viewport
const renderData = editor.getRenderData(0, 1000);
console.log(renderData.pages);

// Undo/Redo
editor.undo();
editor.redo();

// Get document text
console.log(editor.getText());
```

## Project Structure

```
mini-word/
├── src/
│   ├── lib.rs           # Library root, Editor struct
│   ├── main.rs          # CLI entry point
│   ├── document/        # Document model
│   │   ├── mod.rs       # Document struct
│   │   ├── rope.rs      # Rope data structure
│   │   ├── paragraph.rs # Paragraph indexing
│   │   └── block.rs     # Block metadata
│   ├── editing/         # Editing model
│   │   ├── mod.rs
│   │   ├── cursor.rs    # Cursor & selection
│   │   └── operation.rs # Edit operations
│   ├── layout/          # Layout engine
│   │   ├── mod.rs
│   │   ├── engine.rs    # Incremental layout
│   │   ├── line_break.rs# Line breaking
│   │   └── pagination.rs# Page breaks
│   ├── render/          # Rendering
│   │   ├── mod.rs
│   │   ├── display.rs   # Display list
│   │   └── diff.rs      # Render diff
│   ├── undo/            # Undo system
│   │   └── mod.rs
│   └── wasm/            # WASM bindings
│       └── mod.rs
├── web/                 # React frontend
│   ├── src/
│   │   ├── App.tsx      # Main component
│   │   ├── App.css      # Styles
│   │   └── main.tsx     # Entry point
│   ├── index.html
│   ├── vite.config.ts
│   └── package.json
├── pkg/                 # Generated WASM output
├── Cargo.toml
└── README.md
```

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|------------|-------|
| Character insert | O(log n) | Rope tree rebalancing |
| Character delete | O(log n) | Rope tree rebalancing |
| Cursor movement | O(1) | Cached positions |
| Layout (edit) | O(p) | p = affected paragraph lines |
| Pagination | O(P) | P = total pages (only when heights change) |
| Render diff | O(v) | v = visible items in viewport |

## Supported Content

### Current MVP
- Plain text paragraphs
- Multiple pages with automatic pagination
- Cursor and text selection

### Planned
- Headings (H1-H6)
- Bullet and numbered lists
- Line breaks within paragraphs

## Development

### Building for Production

```bash
# Build optimized WASM
wasm-pack build --target web --out-dir pkg --release

# Build web app
cd web
npm run build
```

### Code Quality

```bash
# Format
cargo fmt

# Lint
cargo clippy

# Type check web
cd web && npm run typecheck
```

## License

MIT

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.
