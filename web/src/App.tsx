import { useEffect, useRef, useState, useCallback } from 'react';
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

const PAGE_GAP = 20;
const SCALE = 1;

function App() {
  const [editor, setEditor] = useState<WasmEditorInterface | null>(null);
  const [wasmMemory, setWasmMemory] = useState<WebAssembly.Memory | null>(null);
  const [renderData, setRenderData] = useState<RenderData | null>(null);
  const [constraints, setConstraints] = useState<LayoutConstraints | null>(null);
  const [cursorVisible, setCursorVisible] = useState(true);
  const [isComposing, setIsComposing] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<HTMLDivElement>(null);
  const inputLayerRef = useRef<HTMLDivElement>(null);

  // Load WASM module
  useEffect(() => {
    const loadWasm = async () => {
      try {
        const wasm = await import('../../pkg/mini_word.js');
        await wasm.default();
        
        // Get WASM memory for zero-copy access
        const memory = wasm.getWasmMemory() as WebAssembly.Memory;
        setWasmMemory(memory);

        const ed = new wasm.WasmEditor() as unknown as WasmEditorInterface;
        setEditor(ed);
        setConstraints(getLayoutConstraints(ed));
        updateRenderData(ed, memory);
      } catch (err) {
        console.error('Failed to load WASM:', err);
      }
    };
    loadWasm();
    // eslint-disable-next-line react-hooks/exhaustive-deps
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

  // Auto-focus input layer on mount
  useEffect(() => {
    if (editor && inputLayerRef.current) {
      inputLayerRef.current.focus();
    }
  }, [editor]);

  // Attach native beforeinput event listener (React's synthetic event doesn't work properly)
  useEffect(() => {
    const inputElement = inputLayerRef.current;
    if (!inputElement || !editor || !wasmMemory) return;

    const handleNativeBeforeInput = (e: Event) => {
      const inputEvent = e as InputEvent;
      const inputType = inputEvent.inputType;
      const data = inputEvent.data;

      console.log('[beforeinput native]', inputType, data, isComposing);

      // Let composition events handle IME input
      if (isComposing) {
        return;
      }

      if (inputType === 'insertFromPaste') {
        return;
      }

      e.preventDefault();

      let handled = true;

      switch (inputType) {
        case 'insertText':
          if (data) {
            editor.insertText(data);
          }
          break;
        case 'insertLineBreak':
        case 'insertParagraph':
          editor.insertParagraph();
          break;
        case 'deleteContentBackward':
          editor.deleteBackward();
          break;
        case 'deleteContentForward':
          editor.deleteForward();
          break;
        case 'insertFromPaste':
          // Paste is handled by onPaste event
          break;
        default:
          handled = false;
          break;
      }

      if (handled) {
        setCursorVisible(true);
        updateRenderData(editor, wasmMemory);
        
        // Clear the contenteditable content to prevent DOM mutation
        inputElement.textContent = '';
      }
    };

    inputElement.addEventListener('beforeinput', handleNativeBeforeInput);

    return () => {
      inputElement.removeEventListener('beforeinput', handleNativeBeforeInput);
    };
  }, [editor, wasmMemory, updateRenderData, isComposing]);

  // Handle keyboard shortcuts (only non-input keys)
  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!editor || !wasmMemory || isComposing) return;

      const isCtrl = e.ctrlKey || e.metaKey;
      const isShift = e.shiftKey;

      // Only handle shortcuts and navigation, let browser handle text input
      let handled = false;

      // Undo/Redo
      if (isCtrl && e.key === 'z') {
        e.preventDefault();
        if (isShift) {
          editor.redo();
        } else {
          editor.undo();
        }
        handled = true;
      } else if (isCtrl && e.key === 'y') {
        e.preventDefault();
        editor.redo();
        handled = true;
      } else if (isCtrl && e.key === 'a') {
        e.preventDefault();
        editor.selectAll();
        handled = true;
      } 
      // Navigation keys
      else if (e.key === 'ArrowLeft') {
        e.preventDefault();
        editor.moveCursor(-1, 0, isShift);
        handled = true;
      } else if (e.key === 'ArrowRight') {
        e.preventDefault();
        editor.moveCursor(1, 0, isShift);
        handled = true;
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        editor.moveCursor(0, -1, isShift);
        handled = true;
      } else if (e.key === 'ArrowDown') {
        e.preventDefault();
        editor.moveCursor(0, 1, isShift);
        handled = true;
      }

      if (handled) {
        setCursorVisible(true);
        updateRenderData(editor, wasmMemory);
      }
    },
    [editor, wasmMemory, updateRenderData, isComposing]
  );

  // Note: beforeinput is now handled via native event listener in useEffect
  // React's synthetic onBeforeInput doesn't properly expose inputType

  // Fallback input event handler (for browsers or tools that don't support beforeinput)
  // const handleInput = useCallback(
  //   (e: React.FormEvent<HTMLDivElement>) => {
  //     if (!editor || !wasmMemory || isComposing) return;

  //     const target = e.target as HTMLDivElement;
  //     const text = target.textContent || '';

  //     console.log('[input]', text);

  //     if (text) {
  //       // Insert the text
  //       editor.insertText(text);
  //       setCursorVisible(true);
  //       updateRenderData(editor, wasmMemory);
        
  //       // Clear the contenteditable content
  //       target.textContent = '';
  //     }
  //   },
  //   [editor, wasmMemory, updateRenderData, isComposing]
  // );

  // Handle composition events (IME input)
  const handleCompositionStart = useCallback(() => {
    console.log('[compositionstart]');
    setIsComposing(true);
  }, []);

  const handleCompositionUpdate = useCallback((e: React.CompositionEvent) => {
    console.log('[compositionupdate]', e.data);
  }, []);

  const handleCompositionEnd = useCallback(
    (e: React.CompositionEvent) => {
      console.log('[compositionend]', e.data);
      setIsComposing(false);

      if (!editor || !wasmMemory) return;

      // Insert the composed text
      if (e.data) {
        editor.insertText(e.data);
        setCursorVisible(true);
        updateRenderData(editor, wasmMemory);
        
        // Clear the contenteditable content
        if (inputLayerRef.current) {
          inputLayerRef.current.textContent = '';
        }
      }
    },
    [editor, wasmMemory, updateRenderData]
  );

  // Handle paste event
  const handlePaste = useCallback(
    (e: React.ClipboardEvent) => {
      e.preventDefault();
      
      if (!editor || !wasmMemory) return;

      const text = e.clipboardData.getData('text/plain');
      if (text) {
        console.log('[paste]', text);
        editor.insertText(text);
        setCursorVisible(true);
        updateRenderData(editor, wasmMemory);
      }
    },
    [editor, wasmMemory, updateRenderData]
  );

  // Handle copy event
  const handleCopy = useCallback(
    (e: React.ClipboardEvent) => {
      if (!editor) return;

      if (editor.hasSelection()) {
        e.preventDefault();
        // Get selected text from editor
        // For now, we'll need to add a method to get selected text from Rust
        // As a workaround, we can let the browser handle it
        console.log('[copy]');
      }
    },
    [editor]
  );

  // Handle cut event
  const handleCut = useCallback(
    (e: React.ClipboardEvent) => {
      if (!editor || !wasmMemory) return;

      if (editor.hasSelection()) {
        e.preventDefault();
        console.log('[cut]');
        // Copy selected text and delete it
        // For now, just delete
        editor.deleteBackward();
        setCursorVisible(true);
        updateRenderData(editor, wasmMemory);
      }
    },
    [editor, wasmMemory, updateRenderData]
  );

  // Handle mouse selection
  const [isMouseDown, setIsMouseDown] = useState(false);

  const getDocumentPositionFromMouse = useCallback((clientX: number, clientY: number): {paraIndex: number, offset: number} | null => {
    if (!renderData || !constraints) return null;

    // Convert screen coordinates to document coordinates
    // This is a simplified version - production would need more precise calculation
    
    // Find which page was clicked
    for (const page of renderData.pages) {
      const pageElement = document.querySelector(`[data-page-index="${page.pageIndex}"]`) as HTMLElement;
      if (!pageElement) continue;

      const rect = pageElement.getBoundingClientRect();
      if (clientY >= rect.top && clientY <= rect.bottom) {
        // Found the page, now find the line
        const docY = (clientY - rect.top) / SCALE;
        const docX = (clientX - rect.left) / SCALE;

        for (let i = 0; i < page.lines.length; i++) {
          const line = page.lines[i];
          const lineHeight = 14 * 1.2; // font-size * line-height
          
          if (docY >= line.y && docY <= line.y + lineHeight) {
            // Found the line, estimate character offset
            // This is simplified - production would use Range API or more precise measurement
            const charWidth = 8; // Rough estimate
            const offset = Math.round((docX - line.x) / charWidth);
            const clampedOffset = Math.max(0, Math.min(offset, line.text.length));
            
            return {
              paraIndex: i, // Simplified - would need actual paragraph ID
              offset: clampedOffset
            };
          }
        }
        
        // Clicked below last line
        return {
          paraIndex: page.lines.length - 1,
          offset: page.lines[page.lines.length - 1]?.text.length || 0
        };
      }
    }

    return null;
  }, [renderData, constraints]);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (!editor || !wasmMemory) return;
    
    setIsMouseDown(true);
    const pos = getDocumentPositionFromMouse(e.clientX, e.clientY);
    
    if (pos) {
      // Move cursor to clicked position
      // Note: This is simplified - production would need proper position-to-cursor mapping
      console.log('[mousedown]', pos);
      
      // For now, just focus the input layer
      // TODO: Implement proper click-to-position in Rust
      inputLayerRef.current?.focus();
    }
  }, [editor, wasmMemory, getDocumentPositionFromMouse]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isMouseDown || !editor || !wasmMemory) return;
    
    const pos = getDocumentPositionFromMouse(e.clientX, e.clientY);
    
    if (pos) {
      // Extend selection while dragging
      console.log('[mousemove]', pos);
      
      // TODO: Implement drag selection in Rust
    }
  }, [isMouseDown, editor, wasmMemory, getDocumentPositionFromMouse]);

  const handleMouseUp = useCallback(() => {
    setIsMouseDown(false);
  }, []);

  // Focus on click
  const handleClick = useCallback(() => {
    inputLayerRef.current?.focus();
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
          onClick={handleClick}
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onMouseLeave={handleMouseUp}
          style={{ height: totalHeight + 40 }}
        >
          {/* Native input layer for browser-native text input */}
          <div
            ref={inputLayerRef}
            className="input-layer"
            contentEditable
            suppressContentEditableWarning
            role="textbox"
            aria-multiline="true"
            aria-label="Document editor"
            onKeyDown={handleKeyDown}
            // onInput={handleInput}
            onCompositionStart={handleCompositionStart}
            onCompositionUpdate={handleCompositionUpdate}
            onCompositionEnd={handleCompositionEnd}
            onPaste={handlePaste}
            onCopy={handleCopy}
            onCut={handleCut}
            style={{
              position: 'absolute',
              top: 0,
              left: 0,
              width: '100%',
              height: '100%',
              outline: 'none',
              caretColor: 'transparent', // Hide browser caret, we render our own
              color: 'transparent',
              background: 'transparent',
              fontFamily: 'monospace',
              fontSize: 14,
              lineHeight: 1.2,
              whiteSpace: 'pre-wrap',
              overflowWrap: 'break-word',
              pointerEvents: 'auto',
              zIndex: 1,
            }}
          />

          {/* Rendered text and pages (read-only visual layer) */}
          <div style={{ position: 'relative', pointerEvents: 'none', zIndex: 0 }}>
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

  // Use Rust-calculated cursor X position
  // Rust handles text measurement using its layout engine
  const cursorX = cursor?.x ?? 0;

  return (
    <div
      className="page"
      data-page-index={page.pageIndex}
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
        <TextLine 
          key={i} 
          line={line} 
          scale={scale}
        />
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

const TextLine = ({ line, scale }: TextLineProps) => {
  console.log('scale', scale);
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
        fontFamily: 'monospace',
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
};

export default App;
