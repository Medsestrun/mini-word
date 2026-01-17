import { useEffect, useRef, useState, useCallback, useLayoutEffect } from 'react';
import { fontService } from './fontService';
import './App.css';
import {
  type RenderData,
  type PageRenderData,
  type LineRenderData,
  type CursorRenderData,
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
  const [fontFamily, setFontFamily] = useState('Menlo');
  const [fontSize, setFontSize] = useState(14);
  const containerRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<HTMLDivElement>(null);
  const inputLayerRef = useRef<HTMLDivElement>(null);


  const updateRenderData = useCallback((ed: WasmEditorInterface, memory: WebAssembly.Memory) => {
    const data = getRenderDataFromEditor(ed, memory, 0, 10000);
    setRenderData(data);
  }, []);

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

  const updateFontMetrics = useCallback(() => {
    if (!editor) return;
    const { id, metrics, isNew } = fontService.getOrRegisterFont(fontFamily, fontSize);
    console.log('[App] updateFontMetrics', { id, fontFamily, fontSize, isNew, metrics });

    if (isNew) {
      editor.addFont(id, metrics.lineHeight, metrics.charWidths, metrics.defaultWidth);
    }

    const hasSelection = editor.hasSelection();
    console.log('[App] hasSelection:', hasSelection);

    // If we have a selection, format it
    if (hasSelection) {
      console.log('[App] Formatting selection with font', id);
      editor.formatSelection(id);
    } else {
      console.log('[App] No selection, updating global defaults');
      editor.setFontMetrics(metrics.lineHeight, metrics.charWidths, metrics.defaultWidth);
    }

  }, [editor, fontFamily, fontSize]);

  // Update fonts when changed or editor ready
  useEffect(() => {
    if (editor && wasmMemory) {
      updateFontMetrics();
      updateRenderData(editor, wasmMemory);
      // Restore focus to editor so user can keep typing
      inputLayerRef.current?.focus();
    }
  }, [editor, wasmMemory, updateFontMetrics, updateRenderData]);



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

  const getDocumentPositionFromMouse = useCallback((clientX: number, clientY: number): { pageIndex: number, x: number, y: number } | null => {
    if (!renderData || !constraints) return null;

    // Find which page was clicked
    for (const page of renderData.pages) {
      const pageElement = document.querySelector(`[data-page-index="${page.pageIndex}"]`) as HTMLElement;
      if (!pageElement) continue;

      const rect = pageElement.getBoundingClientRect();
      if (clientY >= rect.top && clientY <= rect.bottom) {
        // Calculate coordinates relative to page (unscaled)
        const x = (clientX - rect.left) / SCALE;
        const y = (clientY - rect.top) / SCALE;

        return {
          pageIndex: page.pageIndex,
          x,
          y
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
      editor.setCursor(pos.pageIndex, pos.x, pos.y);
      updateRenderData(editor, wasmMemory);
      inputLayerRef.current?.focus();
    }
  }, [editor, wasmMemory, getDocumentPositionFromMouse, updateRenderData]);

  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    if (!isMouseDown || !editor || !wasmMemory) return;

    const pos = getDocumentPositionFromMouse(e.clientX, e.clientY);

    if (pos) {
      editor.selectTo(pos.pageIndex, pos.x, pos.y);
      updateRenderData(editor, wasmMemory);
    }
  }, [isMouseDown, editor, wasmMemory, getDocumentPositionFromMouse, updateRenderData]);

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

        {/* Font Controls */}
        <div className="toolbar-group" style={{ gap: '8px', marginLeft: '20px' }}>
          <select
            value={fontFamily}
            onChange={(e) => setFontFamily(e.target.value)}
            className="toolbar-select"
            style={{ height: '24px', borderRadius: '4px', border: '1px solid #ccc' }}
          >
            <option value="Menlo">Menlo</option>
            <option value="Courier New">Courier New</option>
            <option value="Arial">Arial</option>
            <option value="Times New Roman">Times New Roman</option>
            <option value="Verdana">Verdana</option>
          </select>

          <select
            value={fontSize}
            onChange={(e) => setFontSize(Number(e.target.value))}
            className="toolbar-select"
            style={{ height: '24px', borderRadius: '4px', border: '1px solid #ccc' }}
          >
            {[10, 12, 14, 16, 18, 20, 24, 32].map(size => (
              <option key={size} value={size}>{size}px</option>
            ))}
          </select>
        </div>

        <div className="toolbar-spacer" />

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
              fontFamily: `"${fontFamily}"`,
              fontSize: fontSize,
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
                scale={SCALE}
                pageGap={PAGE_GAP}
                fontFamily={fontFamily}
                fontSize={fontSize}
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
  scale: number;
  pageGap: number;
  fontFamily: string;
  fontSize: number;
}

/**
 * Compute cursor X position using browser's DOM Range API
 * This ensures cursor position matches browser-rendered text exactly
 */
const computeCursorX = (cursor: CursorRenderData, page: PageRenderData, scale: number): number => {
  // Find the line containing the cursor by matching Y position
  let targetLineIndex = -1;
  const lineHeight = 14 * 1.2; // font-size * line-height

  for (let i = 0; i < page.lines.length; i++) {
    const line = page.lines[i];
    if (cursor.y >= line.y && cursor.y < line.y + lineHeight) {
      targetLineIndex = i;
      break;
    }
  }

  // Fallback to line X if line not found
  if (targetLineIndex === -1) {
    return page.lines[0]?.x ?? 0;
  }

  const line = page.lines[targetLineIndex];

  // Find the DOM element for this line
  const lineElement = document.querySelector(
    `[data-page-index="${page.pageIndex}"][data-line-index="${targetLineIndex}"]`
  ) as HTMLElement;

  if (!lineElement) {
    return line.x;
  }

  // Handle empty line or offset at start
  if (cursor.utf16OffsetInLine === 0 || !line.text) {
    return line.x;
  }

  // Find the text node (skip list marker span if present)
  let textNode: Text | null = null;
  for (let i = 0; i < lineElement.childNodes.length; i++) {
    const node = lineElement.childNodes[i];
    if (node.nodeType === Node.TEXT_NODE) {
      textNode = node as Text;
      break;
    }
  }

  if (!textNode) {
    return line.x;
  }

  // Clamp offset to text length
  const offset = Math.min(cursor.utf16OffsetInLine, textNode.length);

  const range = document.createRange();
  range.setStart(textNode, 0);
  range.setEnd(textNode, offset);

  const rect = range.getBoundingClientRect();

  // rect.width is the width from start of text to cursor offset in scaled pixels
  // Convert back to unscaled coordinates by dividing by scale
  return line.x + rect.width / scale;
};

// Component to render selection highlights based on DOM measurements
const SelectionHighlights: React.FC<{
  page: PageRenderData;
  scale: number;
}> = ({ page, scale }) => {
  const [rects, setRects] = useState<{ x: number, y: number, width: number, height: number }[]>([]);

  useLayoutEffect(() => {
    const newRects: { x: number, y: number, width: number, height: number }[] = [];

    page.lines.forEach((line, lineIndex) => {
      // Check if line has selection (and not just null)
      if (line.selectionStart !== null && line.selectionEnd !== null) {
        const lineElement = document.querySelector(
          `[data-page-index="${page.pageIndex}"][data-line-index="${lineIndex}"]`
        ) as HTMLElement;

        if (lineElement) {
          // Find text node
          let textNode: Text | null = null;
          for (let i = 0; i < lineElement.childNodes.length; i++) {
            const node = lineElement.childNodes[i];
            if (node.nodeType === Node.TEXT_NODE) {
              const t = node as Text;
              // Only use this text node if it's the main text (check length against line text?)
              // Usually there's only one main text node per line div due to our rendering
              textNode = t;
              break;
            }
          }

          if (textNode) {
            // Bounds checks
            const start = Math.max(0, Math.min(line.selectionStart, textNode.length));
            const end = Math.max(0, Math.min(line.selectionEnd, textNode.length));

            if (start < end) {
              // Range for start offset (to determine X)
              const rangeStart = document.createRange();
              rangeStart.setStart(textNode, 0);
              rangeStart.setEnd(textNode, start);
              const rectStart = rangeStart.getBoundingClientRect();
              const startWidth = rectStart.width / scale;

              // Range for selection width
              const rangeSel = document.createRange();
              rangeSel.setStart(textNode, start);
              rangeSel.setEnd(textNode, end);
              const rectSel = rangeSel.getBoundingClientRect();
              const selWidth = rectSel.width / scale;

              // Position relative to line start
              // line.x is the absolute X position of the line start in the page
              const x = line.x + startWidth;
              const y = line.y;
              const height = 14 * 1.2; // Use standard line height

              newRects.push({ x, y, width: selWidth, height });
            }
          }
        }
      }
    });
    setRects(newRects);
  }, [page, scale]);

  return (
    <>
      {rects.map((rect, i) => (
        <div
          key={i}
          className="selection"
          style={{
            position: 'absolute',
            left: rect.x * scale,
            top: rect.y * scale,
            width: rect.width * scale,
            height: rect.height * scale,
            background: 'rgba(59, 130, 246, 0.3)',
            pointerEvents: 'none',
          }}
        />
      ))}
    </>
  );
};

function Page({
  page,
  constraints,
  cursor,
  cursorVisible,
  scale,
  pageGap,
  fontFamily,
  fontSize,
}: PageProps) {
  const pageTop = page.pageIndex * (constraints.pageHeight + pageGap) * scale + pageGap;
  const [cursorX, setCursorX] = useState(0);

  // Compute cursor X position after DOM updates using useLayoutEffect
  // This ensures the DOM has the latest text before we measure
  useLayoutEffect(() => {
    if (cursor) {
      const x = computeCursorX(cursor, page, scale);
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setCursorX(x);
    } else {
      setCursorX(0);
    }
  }, [cursor, page, scale]);

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
      <SelectionHighlights page={page} scale={scale} />
      {/* Text lines */}
      {page.lines.map((line, i) => (
        <TextLine
          key={i}
          line={line}
          lineIndex={i}
          pageIndex={page.pageIndex}
          scale={scale}
          fontFamily={fontFamily}
          fontSize={fontSize}
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
  lineIndex: number;
  pageIndex: number;
  scale: number;
  fontFamily: string;
  fontSize: number;
}

const TextLine = ({ line, lineIndex, pageIndex, scale, fontFamily, fontSize }: TextLineProps) => {
  // If we have specific styles, render them
  if (line.styles && line.styles.length > 0) {
    const content = [];
    let currentIdx = 0;

    // Sort styles by start index just in case
    // (Rust should guarantee this but good to be safe)

    for (const span of line.styles) {
      // Gap before span?
      if (span.start > currentIdx) {
        content.push(
          <span key={`gap-${currentIdx}`} style={{
            fontFamily: `"${fontFamily}"`,
            fontSize: fontSize * scale,
          }}>
            {line.text.substring(currentIdx, span.start)}
          </span>
        );
      }

      const fontData = fontService.getFontDetails(span.fontId);
      const spanFamily = fontData?.family || fontFamily;
      const spanSize = (fontData?.size || fontSize) * scale;

      content.push(
        <span key={`span-${span.start}`} style={{
          fontFamily: `"${spanFamily}"`,
          fontSize: spanSize,
        }}>
          {line.text.substring(span.start, span.start + span.len)}
        </span>
      );

      currentIdx = span.start + span.len;
    }

    // Remaining text
    if (currentIdx < line.text.length) {
      content.push(
        <span key={`gap-${currentIdx}`} style={{
          fontFamily: `"${fontFamily}"`,
          fontSize: fontSize * scale,
        }}>
          {line.text.substring(currentIdx)}
        </span>
      );
    }

    return (
      <div
        className="text-line"
        data-page-index={pageIndex}
        data-line-index={lineIndex}
        style={{
          position: 'absolute',
          left: line.x * scale,
          top: line.y * scale,
          lineHeight: 1.2,
          whiteSpace: 'pre',
          color: '#1a1a1a',
        }}
      >
        {line.listMarker && (
          <span style={{ marginRight: 8, fontFamily: `"${fontFamily}"`, fontSize: fontSize * scale }}>
            {line.listMarker}
          </span>
        )}
        {content.length > 0 ? content : '\u200B'}
      </div>
    );
  }

  // Fallback / legacy rendering
  const getFontSize = () => {
    const baseSize = fontSize;
    if (line.isHeading && line.headingLevel) {
      const sizes: Record<number, number> = {
        1: 24,
        2: 20,
        3: 18,
        4: 16,
        5: 14,
        6: 13,
      };
      // Scale heading based on default 14px base
      const defaultSize = sizes[line.headingLevel] || 14;
      return (defaultSize / 14) * baseSize;
    }
    return baseSize;
  };

  const getFontWeight = () => {
    if (line.isHeading) return 700;
    return 400;
  };

  return (
    <div
      className="text-line"
      data-page-index={pageIndex}
      data-line-index={lineIndex}
      style={{
        position: 'absolute',
        left: line.x * scale,
        top: line.y * scale,
        fontFamily: `"${fontFamily}"`,
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
