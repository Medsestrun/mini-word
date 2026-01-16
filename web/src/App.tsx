import { useEffect, useRef, useState, useCallback, useMemo } from 'react';
import './App.css';
import {
  type RenderData,
  type PageRenderData,
  type LineRenderData,
  type CursorRenderData,
  type SelectionRenderData,
  type LayoutConstraints,
  type WasmEditorInterface,
  getRenderDataFromEditor,
  getLayoutConstraints,
} from './renderDecoder';

// Text measurement utilities for accurate cursor positioning
const measureTextWidth = (() => {
  let canvas: HTMLCanvasElement | null = null;
  let ctx: CanvasRenderingContext2D | null = null;
  
  return (text: string, font: string): number => {
    if (!canvas) {
      canvas = document.createElement('canvas');
      ctx = canvas.getContext('2d');
    }
    if (!ctx) return 0;
    ctx.font = font;
    return ctx.measureText(text).width;
  };
})();

// Get font string for a line
const getFontForLine = (line: LineRenderData, scale: number): string => {
  const weight = line.isHeading ? 700 : 400;
  let size = 14;
  if (line.isHeading && line.headingLevel) {
    const sizes: Record<number, number> = { 1: 24, 2: 20, 3: 18, 4: 16, 5: 14, 6: 13 };
    size = sizes[line.headingLevel] || 14;
  }
  return `${weight} ${size * scale}px Georgia, "Times New Roman", serif`;
};

const PAGE_GAP = 20;
const SCALE = 1;

function App() {
  const [editor, setEditor] = useState<WasmEditorInterface | null>(null);
  const [wasmMemory, setWasmMemory] = useState<WebAssembly.Memory | null>(null);
  const [renderData, setRenderData] = useState<RenderData | null>(null);
  const [constraints, setConstraints] = useState<LayoutConstraints | null>(null);
  const [cursorVisible, setCursorVisible] = useState(true);
  const containerRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<HTMLDivElement>(null);

  // Load WASM module
  useEffect(() => {
    const loadWasm = async () => {
      try {
        const wasm = await import('../../pkg/mini_word.js');
        await wasm.default();
        
        // Get WASM memory for zero-copy access
        const memory = wasm.getWasmMemory() as WebAssembly.Memory;
        setWasmMemory(memory);

        const ed = new wasm.WasmEditor() as WasmEditorInterface;
        setEditor(ed);
        setConstraints(getLayoutConstraints(ed));
        updateRenderData(ed, memory);
      } catch (err) {
        console.error('Failed to load WASM:', err);
      }
    };
    loadWasm();
  }, []);

  const updateRenderData = useCallback((ed: WasmEditorInterface, memory: WebAssembly.Memory) => {
    const data = getRenderDataFromEditor(ed, memory, 0, 10000);
    setRenderData(data);
  }, []);

  // Cursor blink
  useEffect(() => {
    const interval = setInterval(() => {
      setCursorVisible((v) => !v);
    }, 530);
    return () => clearInterval(interval);
  }, []);

  // Handle keyboard input
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!editor || !wasmMemory) return;

      const isCtrl = e.ctrlKey || e.metaKey;
      const isShift = e.shiftKey;

      let handled = true;

      if (isCtrl && e.key === 'z') {
        if (isShift) {
          editor.redo();
        } else {
          editor.undo();
        }
      } else if (isCtrl && e.key === 'y') {
        editor.redo();
      } else if (isCtrl && e.key === 'a') {
        editor.selectAll();
      } else if (e.key === 'Backspace') {
        editor.deleteBackward();
      } else if (e.key === 'Delete') {
        editor.deleteForward();
      } else if (e.key === 'Enter') {
        editor.insertParagraph();
      } else if (e.key === 'ArrowLeft') {
        editor.moveCursor(-1, 0, isShift);
      } else if (e.key === 'ArrowRight') {
        editor.moveCursor(1, 0, isShift);
      } else if (e.key === 'ArrowUp') {
        editor.moveCursor(0, -1, isShift);
      } else if (e.key === 'ArrowDown') {
        editor.moveCursor(0, 1, isShift);
      } else if (e.key.length === 1 && !isCtrl) {
        editor.insertText(e.key);
      } else {
        handled = false;
      }

      if (handled) {
        e.preventDefault();
        setCursorVisible(true);
        updateRenderData(editor, wasmMemory);
      }
    },
    [editor, wasmMemory, updateRenderData]
  );

  // Focus on click
  const handleClick = useCallback(() => {
    editorRef.current?.focus();
  }, []);

  if (!editor || !constraints || !wasmMemory) {
    return (
      <div className="loading">
        <div className="loading-spinner" />
        <p>Loading Mini-Word...</p>
      </div>
    );
  }

  const pageCount = editor.getPageCount();
  const totalHeight = pageCount * (constraints.pageHeight + PAGE_GAP) * SCALE;

  return (
    <div className="app">
      {/* Toolbar */}
      <header className="toolbar">
        <div className="toolbar-group">
          <span className="app-title">Mini-Word</span>
        </div>
        <div className="toolbar-group">
          <button
            className="toolbar-btn"
            onClick={() => {
              editor.undo();
              updateRenderData(editor, wasmMemory);
            }}
            title="Undo (Ctrl+Z)"
            aria-label="Undo"
            tabIndex={0}
          >
            ↶
          </button>
          <button
            className="toolbar-btn"
            onClick={() => {
              editor.redo();
              updateRenderData(editor, wasmMemory);
            }}
            title="Redo (Ctrl+Y)"
            aria-label="Redo"
            tabIndex={0}
          >
            ↷
          </button>
        </div>
        <div className="toolbar-spacer" />
        <div className="toolbar-group">
          <span className="page-info">
            Page 1 of {pageCount}
          </span>
        </div>
      </header>

      {/* Document area */}
      <div className="document-container" ref={containerRef}>
        <div
          className="editor-area"
          ref={editorRef}
          tabIndex={0}
          role="textbox"
          aria-multiline="true"
          aria-label="Document editor"
          onKeyDown={handleKeyDown}
          onClick={handleClick}
          style={{ height: totalHeight + 40 }}
        >
          {renderData?.pages.map((page) => (
            <Page
              key={page.pageIndex}
              page={page}
              constraints={constraints}
              cursor={
                renderData.cursor?.pageIndex === page.pageIndex
                  ? renderData.cursor
                  : null
              }
              cursorVisible={cursorVisible}
              selections={renderData.selections.filter(
                (s) => s.pageIndex === page.pageIndex
              )}
              scale={SCALE}
              pageGap={PAGE_GAP}
            />
          ))}
          {/* Empty state */}
          {(!renderData?.pages.length || renderData.pages.every(p => p.lines.length === 0)) && (
            <div 
              className="empty-placeholder"
              style={{
                position: 'absolute',
                top: constraints.marginTop * SCALE + PAGE_GAP,
                left: constraints.marginLeft * SCALE,
                color: '#999',
                fontFamily: 'Georgia, serif',
                fontSize: 14,
                pointerEvents: 'none',
              }}
            >
              Start typing...
            </div>
          )}
        </div>
      </div>

      {/* Status bar */}
      <footer className="status-bar">
        <span>{editor.getText().length} characters</span>
        <span>{pageCount} page{pageCount !== 1 ? 's' : ''}</span>
      </footer>
    </div>
  );
}

interface PageProps {
  page: PageRenderData;
  constraints: LayoutConstraints;
  cursor: CursorRenderData | null;
  cursorVisible: boolean;
  selections: SelectionRenderData[];
  scale: number;
  pageGap: number;
}

function Page({
  page,
  constraints,
  cursor,
  cursorVisible,
  selections,
  scale,
  pageGap,
}: PageProps) {
  const pageTop = page.pageIndex * (constraints.pageHeight + pageGap) * scale + pageGap;

  // Calculate accurate cursor X position using text measurement
  const cursorX = useMemo(() => {
    if (!cursor) return 0;
    
    // Find the line the cursor is on by matching Y position
    const cursorLine = page.lines.find(line => Math.abs(line.y - cursor.y) < 1);
    if (!cursorLine) {
      console.log('[Cursor] No line found, using Rust X:', cursor.x);
      return cursor.x; // Fallback to Rust-calculated position
    }
    
    // Measure the text up to the cursor position using unscaled font (same as render)
    const textBeforeCursor = cursorLine.text.slice(0, cursor.lineCharOffset);
    // Use unscaled font for measurement, since we measure in document coords
    const font = getFontForLine(cursorLine, 1); // Use scale=1 for document-space measurement
    const measuredWidth = measureTextWidth(textBeforeCursor, font);
    
    console.log('[Cursor]', {
      lineText: `"${cursorLine.text}"`,
      lineTextLen: cursorLine.text.length,
      lineCharOffset: cursor.lineCharOffset,
      textBeforeCursor: `"${textBeforeCursor}"`,
      font,
      measuredWidth,
      lineX: cursorLine.x,
      result: cursorLine.x + measuredWidth,
    });
    
    // cursorLine.x is in document space, measuredWidth is now also in document space
    return cursorLine.x + measuredWidth;
  }, [cursor, page.lines]);

  return (
    <div
      className="page"
      style={{
        position: 'absolute',
        top: pageTop,
        left: '50%',
        transform: 'translateX(-50%)',
        width: constraints.pageWidth * scale,
        height: constraints.pageHeight * scale,
        background: 'white',
        boxShadow: '0 2px 8px rgba(0,0,0,0.15)',
        borderRadius: 2,
      }}
    >
      {/* Selection highlights */}
      {selections.map((sel, i) => (
        <div
          key={i}
          className="selection"
          style={{
            position: 'absolute',
            left: sel.x * scale,
            top: sel.y * scale,
            width: sel.width * scale,
            height: sel.height * scale,
            background: 'rgba(59, 130, 246, 0.3)',
            pointerEvents: 'none',
          }}
        />
      ))}

      {/* Text lines */}
      {page.lines.map((line, i) => (
        <TextLine key={i} line={line} scale={scale} />
      ))}

      {/* Cursor */}
      {cursor && cursorVisible && (
        <div
          className="cursor"
          style={{
            position: 'absolute',
            left: cursorX * scale,
            top: cursor.y * scale,
            width: 2,
            height: cursor.height * scale,
            background: '#000',
            pointerEvents: 'none',
          }}
        />
      )}

      {/* Page number */}
      <div
        className="page-number"
        style={{
          position: 'absolute',
          bottom: 20 * scale,
          left: 0,
          right: 0,
          textAlign: 'center',
          fontSize: 11,
          color: '#999',
        }}
      >
        {page.pageIndex + 1}
      </div>
    </div>
  );
}

interface TextLineProps {
  line: LineRenderData;
  scale: number;
}

function TextLine({ line, scale }: TextLineProps) {
  const getFontSize = () => {
    if (line.isHeading && line.headingLevel) {
      const sizes: Record<number, number> = {
        1: 24,
        2: 20,
        3: 18,
        4: 16,
        5: 14,
        6: 13,
      };
      return sizes[line.headingLevel] || 14;
    }
    return 14;
  };

  const getFontWeight = () => {
    if (line.isHeading) return 700;
    return 400;
  };

  return (
    <div
      className="text-line"
      style={{
        position: 'absolute',
        left: line.x * scale,
        top: line.y * scale,
        fontFamily: 'Georgia, "Times New Roman", serif',
        fontSize: getFontSize() * scale,
        fontWeight: getFontWeight(),
        lineHeight: 1.2,
        whiteSpace: 'pre',
        color: '#1a1a1a',
      }}
    >
      {line.listMarker && (
        <span style={{ marginRight: 8 }}>{line.listMarker}</span>
      )}
      {line.text || '\u200B'}
    </div>
  );
}

export default App;
