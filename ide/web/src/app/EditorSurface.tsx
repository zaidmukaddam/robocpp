import { forwardRef, useRef, useState } from "react";
import { CodeEditor, type CodeEditorHandle } from "@/features/editor/CodeEditor";
import { GraphDiagramView } from "@/features/graph/GraphDiagramView";
import { isGraphicalLanguage } from "@/features/graph/graphCache";
import { roundTripHint } from "@/features/graph/graphEdits";
import type { GraphValidation } from "@/features/graph/validateGraph";
import type { GraphModel } from "@/features/graph/graphTypes";
import { TargetMappingEditor } from "@/features/target/TargetMappingEditor";
import { isTargetMappingFile } from "@/features/target/targetMapping";
import type { Analysis, DebugTrace, Diagnostic, DocumentSymbol, RunTrace, WorkspaceFile } from "@/types";

export type EditorSurfaceProps = {
  file: WorkspaceFile;
  graphModel: GraphModel | null;
  graphValidation: GraphValidation | null;
  runTrace: RunTrace | null;
  debugTrace: DebugTrace | null;
  diagnostics: Diagnostic[];
  completions: Analysis["completions"];
  symbols: DocumentSymbol[];
  mappingSymbolSuggestions: string[];
  targetBindings: import("@/features/target/targetMapping").TargetMappingEntry[];
  currentLine: number | null;
  breakpoints?: Set<number>;
  onToggleBreakpoint?: (line: number) => void;
  onDiagnosticClick: (diagnostic: Diagnostic) => void;
  onAddWatch: (name: string) => void;
  canUndo: boolean;
  canRedo: boolean;
  onChange: (text: string) => void;
  onUndo: () => void;
  onRedo: () => void;
};

export const EditorSurface = forwardRef<CodeEditorHandle, EditorSurfaceProps>(function EditorSurface(
  {
    file,
    graphModel,
    graphValidation,
    runTrace,
    debugTrace,
    diagnostics,
    completions,
    symbols,
    mappingSymbolSuggestions,
    targetBindings,
    currentLine,
    breakpoints,
    onToggleBreakpoint,
    onDiagnosticClick,
    onAddWatch,
    canUndo,
    canRedo,
    onChange,
    onUndo,
    onRedo
  },
  ref
) {
  const graphical = isGraphicalLanguage(file.languageId);
  const mappingFile = isTargetMappingFile(file.name);
  const [diagramHeight, setDiagramHeight] = useState(300);
  const resizeRef = useRef<{ startY: number; startHeight: number } | null>(null);

  if (mappingFile) {
    return (
      <div className="editor-pane mapping-editor-pane">
        <TargetMappingEditor text={file.text} onChange={onChange} symbolSuggestions={mappingSymbolSuggestions} />
      </div>
    );
  }

  return (
    <div className="editor-pane">
      {graphical ? (
        <>
          <div className="diagram-panel" style={{ flex: `0 0 ${diagramHeight}px` }}>
            <div className="diagram-panel-label" title={roundTripHint(file.languageId)}>
              Diagram · {file.languageId.toUpperCase()}
            </div>
            <GraphDiagramView
              file={file}
              model={graphModel}
              validation={graphValidation}
              runTrace={runTrace}
              debugTrace={debugTrace}
              canUndo={canUndo}
              canRedo={canRedo}
              onChange={onChange}
              onUndo={onUndo}
              onRedo={onRedo}
            />
          </div>
          <div
            className="diagram-resize-handle"
            role="separator"
            aria-orientation="horizontal"
            aria-label="Resize diagram panel"
            onPointerDown={(event) => {
              resizeRef.current = { startY: event.clientY, startHeight: diagramHeight };
              event.currentTarget.setPointerCapture(event.pointerId);
            }}
            onPointerMove={(event) => {
              const resize = resizeRef.current;
              if (!resize) {
                return;
              }
              const next = resize.startHeight + (event.clientY - resize.startY);
              setDiagramHeight(Math.min(560, Math.max(180, next)));
            }}
            onPointerUp={(event) => {
              resizeRef.current = null;
              event.currentTarget.releasePointerCapture(event.pointerId);
            }}
          />
        </>
      ) : null}
      <CodeEditor
        ref={ref}
        value={file.text}
        languageId={file.languageId}
        onChange={onChange}
        diagnostics={diagnostics}
        completions={completions}
        symbols={symbols}
        targetBindings={targetBindings}
        currentLine={currentLine}
        breakpoints={breakpoints}
        onToggleBreakpoint={onToggleBreakpoint}
        onDiagnosticClick={onDiagnosticClick}
        onAddWatch={onAddWatch}
      />
    </div>
  );
});
