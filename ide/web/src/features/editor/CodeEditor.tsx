import { forwardRef, useCallback, useImperativeHandle, useMemo, useRef, useState } from "react";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { autocompletion, completionKeymap } from "@codemirror/autocomplete";
import { linter, type Diagnostic as LintDiagnostic } from "@codemirror/lint";
import { searchKeymap } from "@codemirror/search";
import {
  EditorView,
  Decoration,
  keymap,
  lineNumbers,
  gutter,
  GutterMarker
} from "@codemirror/view";
import { EditorState, type Extension } from "@codemirror/state";
import { hoverAtCursor, type EditorHoverInfo } from "@/lib/editorHover";
import { robocppEditorTheme } from "@/features/editor/codemirrorTheme";
import { iecLanguageExtension } from "@/features/editor/iecLanguage";
import type { CompletionItem, Diagnostic, DocumentSymbol } from "@/types";

export type CodeEditorHandle = {
  scrollToLine: (line: number) => void;
  getSelectedText: () => string;
  getCursorOffset: () => number;
  goToOffset: (offset: number) => void;
  findNext: (query: string) => boolean;
  replaceNext: (query: string, replacement: string) => boolean;
  replaceAll: (query: string, replacement: string) => number;
};

type CodeEditorProps = {
  value: string;
  languageId: string;
  onChange: (value: string) => void;
  readOnly?: boolean;
  diagnostics?: Diagnostic[];
  completions?: CompletionItem[];
  symbols?: DocumentSymbol[];
  targetBindings?: import("@/features/target/targetMapping").TargetMappingEntry[];
  currentLine?: number | null;
  breakpoints?: Set<number>;
  onToggleBreakpoint?: (line: number) => void;
  onDiagnosticClick?: (diagnostic: Diagnostic) => void;
  onAddWatch?: (name: string) => void;
};

class BreakpointMarker extends GutterMarker {
  constructor(private readonly active: boolean) {
    super();
  }

  toDOM() {
    const span = document.createElement("span");
    if (this.active) {
      span.textContent = "●";
      span.className = "cm-breakpoint";
    }
    span.title = this.active ? "Remove breakpoint" : "Add breakpoint";
    return span;
  }
}

function diagnosticLintExtension(diagnostics: Diagnostic[]): Extension {
  return linter(() =>
    diagnostics
      .filter((diagnostic) => diagnostic.span)
      .map((diagnostic) => {
        const span = diagnostic.span!;
        return {
          from: span.start,
          to: span.end,
          severity: diagnostic.severity === "error" ? "error" : diagnostic.severity === "warning" ? "warning" : "info",
          message: diagnostic.message
        } satisfies LintDiagnostic;
      })
  );
}

function diagnosticMarkExtension(diagnostics: Diagnostic[]): Extension {
  const marks = diagnostics
    .filter((diagnostic) => diagnostic.span)
    .map((diagnostic) => {
      const span = diagnostic.span!;
      const className =
        diagnostic.severity === "error"
          ? "cm-diagnostic-error"
          : diagnostic.severity === "warning"
            ? "cm-diagnostic-warning"
            : "cm-diagnostic-note";
      return Decoration.mark({ class: className }).range(span.start, span.end);
    });
  return EditorView.decorations.compute(["doc"], () => Decoration.set(marks, true));
}

function currentLineExtension(currentLine: number | null | undefined): Extension {
  if (!currentLine) {
    return [];
  }
  const line = currentLine - 1;
  return EditorView.decorations.compute(["doc"], (state) => {
    const lineObj = state.doc.line(Math.min(line + 1, state.doc.lines));
    return Decoration.set([Decoration.line({ class: "cm-current-line" }).range(lineObj.from)]);
  });
}

function breakpointGutterExtension(
  breakpoints: Set<number>,
  onToggleBreakpoint?: (line: number) => void
): Extension {
  if (!onToggleBreakpoint) {
    return [];
  }
  return gutter({
    class: "cm-breakpoint-gutter",
    domEventHandlers: {
      mousedown(_view, _block, event) {
        if ((event as MouseEvent).button !== 0) {
          return false;
        }
        const lineNumber = _view.state.doc.lineAt(_block.from).number;
        onToggleBreakpoint(lineNumber);
        return true;
      }
    },
    lineMarker(view, block) {
      const lineNumber = view.state.doc.lineAt(block.from).number;
      return new BreakpointMarker(breakpoints.has(lineNumber));
    }
  });
}

export const CodeEditor = forwardRef<CodeEditorHandle, CodeEditorProps>(function CodeEditor(
  {
    value,
    languageId,
    onChange,
    readOnly = false,
    diagnostics = [],
    completions = [],
    symbols = [],
    targetBindings = [],
    currentLine = null,
    breakpoints = new Set<number>(),
    onToggleBreakpoint,
    onDiagnosticClick,
    onAddWatch
  },
  ref
) {
  const editorRef = useRef<ReactCodeMirrorRef>(null);
  const [hoverInfo, setHoverInfo] = useState<EditorHoverInfo | null>(null);
  const [hoverPosition, setHoverPosition] = useState({ top: 0, left: 0 });

  const completionSource = useMemo(
    () =>
      autocompletion({
        override: [
          (context) => {
            const word = context.matchBefore(/[A-Za-z_][A-Za-z0-9_]*/);
            if (!word || (word.from === word.to && !context.explicit)) {
              return null;
            }
            const prefix = word.text.toLowerCase();
            const options = completions
              .filter((item) => item.label.toLowerCase().startsWith(prefix))
              .slice(0, 20)
              .map((item) => ({
                label: item.label,
                detail: item.detail,
                type: item.kind
              }));
            return { from: word.from, options };
          }
        ]
      }),
    [completions]
  );

  const extensions = useMemo(() => {
    const base: Extension[] = [
      lineNumbers(),
      history(),
      iecLanguageExtension,
      EditorView.lineWrapping,
      completionSource,
      diagnosticLintExtension(diagnostics),
      diagnosticMarkExtension(diagnostics),
      currentLineExtension(currentLine),
      breakpointGutterExtension(breakpoints, onToggleBreakpoint),
      keymap.of([...defaultKeymap, ...historyKeymap, ...completionKeymap, ...searchKeymap]),
      EditorView.domEventHandlers({
        contextmenu(event, view) {
          if (!onAddWatch) {
            return false;
          }
          const selection = view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to).trim();
          if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(selection)) {
            return false;
          }
          event.preventDefault();
          onAddWatch(selection);
          return true;
        },
        mousemove(event, view) {
          const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
          if (pos == null) {
            setHoverInfo(null);
            return false;
          }
          const doc = view.state.doc.toString();
          const info = hoverAtCursor(doc, pos, symbols, diagnostics, completions, targetBindings);
          if (!info) {
            setHoverInfo(null);
            return false;
          }
          const coords = view.coordsAtPos(pos);
          const container = view.dom.getBoundingClientRect();
          if (coords) {
            setHoverPosition({
              top: coords.bottom - container.top + 4,
              left: coords.left - container.left
            });
          }
          setHoverInfo(info);
          return false;
        },
        mouseleave() {
          setHoverInfo(null);
          return false;
        }
      })
    ];
    if (readOnly) {
      base.push(EditorState.readOnly.of(true));
    }
    return base;
  }, [
    breakpoints,
    completionSource,
    completions,
    currentLine,
    diagnostics,
    onAddWatch,
    onToggleBreakpoint,
    readOnly,
    symbols,
    targetBindings,
    value
  ]);

  const scrollToLine = useCallback((line: number) => {
    const view = editorRef.current?.view;
    if (!view) {
      return;
    }
    const lineObj = view.state.doc.line(Math.min(Math.max(line, 1), view.state.doc.lines));
    view.dispatch({
      selection: { anchor: lineObj.from },
      effects: EditorView.scrollIntoView(lineObj.from, { y: "center" })
    });
    view.focus();
  }, []);

  const goToOffset = useCallback((offset: number) => {
    const view = editorRef.current?.view;
    if (!view) {
      return;
    }
    const clamped = Math.min(Math.max(offset, 0), view.state.doc.length);
    view.dispatch({
      selection: { anchor: clamped },
      effects: EditorView.scrollIntoView(clamped, { y: "center" })
    });
    view.focus();
  }, []);

  useImperativeHandle(
    ref,
    () => ({
      scrollToLine,
      getSelectedText: () => {
        const view = editorRef.current?.view;
        if (!view) {
          return "";
        }
        return view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to).trim();
      },
      getCursorOffset: () => editorRef.current?.view?.state.selection.main.head ?? 0,
      goToOffset,
      findNext: (query: string) => {
        const normalized = query.trim();
        if (!normalized) {
          return false;
        }
        const view = editorRef.current?.view;
        if (!view) {
          return false;
        }
        const from = view.state.selection.main.to;
        let index = value.indexOf(normalized, from);
        if (index < 0) {
          index = value.indexOf(normalized);
        }
        if (index < 0) {
          return false;
        }
        goToOffset(index);
        view.dispatch({ selection: { anchor: index, head: index + normalized.length } });
        return true;
      },
      replaceNext: (query: string, replacement: string) => {
        const normalized = query.trim();
        if (!normalized) {
          return false;
        }
        const view = editorRef.current?.view;
        if (!view) {
          return false;
        }
        const { from, to } = view.state.selection.main;
        if (value.slice(from, to) === normalized) {
          const next = `${value.slice(0, from)}${replacement}${value.slice(to)}`;
          onChange(next);
          const position = from + replacement.length;
          window.requestAnimationFrame(() => goToOffset(position));
          return true;
        }
        let index = value.indexOf(normalized, to);
        if (index < 0) {
          index = value.indexOf(normalized);
        }
        if (index < 0) {
          return false;
        }
        const next = `${value.slice(0, index)}${replacement}${value.slice(index + normalized.length)}`;
        onChange(next);
        const position = index + replacement.length;
        window.requestAnimationFrame(() => goToOffset(position));
        return true;
      },
      replaceAll: (query: string, replacement: string) => {
        const normalized = query.trim();
        if (!normalized || !value.includes(normalized)) {
          return 0;
        }
        const parts = value.split(normalized);
        const count = parts.length - 1;
        onChange(parts.join(replacement));
        return count;
      }
    }),
    [goToOffset, onChange, scrollToLine, value]
  );

  return (
    <div className="code-editor code-editor-codemirror">
      <CodeMirror
        ref={editorRef}
        value={value}
        height="100%"
        width="100%"
        theme={robocppEditorTheme}
        basicSetup={false}
        extensions={extensions}
        onChange={(next) => onChange(next)}
        aria-label="Source editor"
      />
      {hoverInfo ? (
        <div className="editor-hover-popup" style={{ top: hoverPosition.top, left: hoverPosition.left }}>
          <strong>{hoverInfo.title}</strong>
          <span>{hoverInfo.detail}</span>
          <small>{hoverInfo.kind}</small>
        </div>
      ) : null}
      {diagnostics.some((diagnostic) => diagnostic.span) ? (
        <div className="editor-diagnostic-rail" aria-hidden="true">
          {diagnostics.map((diagnostic) => {
            if (!diagnostic.span) {
              return null;
            }
            const top = Math.max(0, (diagnostic.span.line - 1) * 20);
            return (
              <button
                key={`${diagnostic.stableCode}-${diagnostic.message}-${diagnostic.span.line}`}
                type="button"
                className={`editor-diagnostic-marker ${diagnostic.severity}`}
                style={{ top }}
                title={diagnostic.message}
                onClick={() => onDiagnosticClick?.(diagnostic)}
              />
            );
          })}
        </div>
      ) : null}
    </div>
  );
});
