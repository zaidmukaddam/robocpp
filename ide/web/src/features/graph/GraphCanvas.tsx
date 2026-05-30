import { Hand, Maximize2, ZoomIn, ZoomOut } from "lucide-react";
import { useCallback, useRef, useState, type ReactNode, type WheelEvent } from "react";

type GraphCanvasProps = {
  children: ReactNode;
};

const MIN_SCALE = 0.5;
const MAX_SCALE = 2.5;
const ZOOM_STEP = 0.1;
const WHEEL_STEP = 0.08;

function clampScale(value: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, Number(value.toFixed(2))));
}

function isInteractiveTarget(target: HTMLElement): boolean {
  return Boolean(target.closest("button, a, input, textarea, select, [role='button']"));
}

export function GraphCanvas({ children }: GraphCanvasProps) {
  const [scale, setScale] = useState(1);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const dragRef = useRef<{ x: number; y: number; originX: number; originY: number } | null>(null);

  const zoomIn = useCallback(() => {
    setScale((current) => clampScale(current + ZOOM_STEP));
  }, []);

  const zoomOut = useCallback(() => {
    setScale((current) => clampScale(current - ZOOM_STEP));
  }, []);

  const resetView = useCallback(() => {
    setScale(1);
    setOffset({ x: 0, y: 0 });
  }, []);

  const onWheel = (event: WheelEvent<HTMLDivElement>) => {
    if (!event.ctrlKey && !event.metaKey) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const delta = event.deltaY > 0 ? -WHEEL_STEP : WHEEL_STEP;
    setScale((current) => clampScale(current + delta));
  };

  const atMin = scale <= MIN_SCALE;
  const atMax = scale >= MAX_SCALE;
  const isDefaultView = scale === 1 && offset.x === 0 && offset.y === 0;

  return (
    <div className="graph-canvas-shell">
      <div className="graph-canvas-controls" role="toolbar" aria-label="Canvas zoom">
        <span className="graph-canvas-hint">
          <Hand size={12} aria-hidden="true" />
          Scroll canvas
        </span>
        <span className="graph-canvas-controls-divider" aria-hidden="true" />
        <button type="button" className="graph-canvas-icon-btn" aria-label="Zoom in" disabled={atMax} onClick={zoomIn}>
          <ZoomIn size={14} aria-hidden="true" />
        </button>
        <button type="button" className="graph-canvas-icon-btn" aria-label="Zoom out" disabled={atMin} onClick={zoomOut}>
          <ZoomOut size={14} aria-hidden="true" />
        </button>
        <span className="graph-canvas-controls-divider" aria-hidden="true" />
        <button
          type="button"
          className="graph-canvas-reset-btn"
          aria-label="Reset zoom and pan"
          disabled={isDefaultView}
          onClick={resetView}
        >
          <Maximize2 size={12} aria-hidden="true" />
          <span>Reset</span>
        </button>
        <span className="graph-canvas-scale" aria-live="polite">
          {Math.round(scale * 100)}%
        </span>
      </div>
      <div
        className="graph-canvas-viewport"
        aria-label="Scrollable graph canvas"
        onWheel={onWheel}
        onPointerDown={(event) => {
          const target = event.target as HTMLElement;
          const middleButton = event.button === 1;
          const altPan = event.button === 0 && event.altKey;
          const backgroundPan =
            event.button === 0 &&
            !event.altKey &&
            !isInteractiveTarget(target) &&
            (target.classList.contains("graph-canvas-viewport") ||
              target.classList.contains("graph-canvas-stage") ||
              target.classList.contains("graph-preview-stack") ||
              target.classList.contains("ladder-view") ||
              target.classList.contains("fbd-view") ||
              target.classList.contains("sfc-view") ||
              target.classList.contains("plcopen-view") ||
              target.classList.contains("ld-rung") ||
              target.classList.contains("fbd-row") ||
              target.classList.contains("ld-wire") ||
              target.classList.contains("ld-rail") ||
              target.classList.contains("fbd-arrow") ||
              target.classList.contains("sfc-arrow"));

          if (!middleButton && !altPan && !backgroundPan) {
            return;
          }

          event.preventDefault();
          dragRef.current = {
            x: event.clientX,
            y: event.clientY,
            originX: offset.x,
            originY: offset.y
          };
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onPointerMove={(event) => {
          const drag = dragRef.current;
          if (!drag) {
            return;
          }
          setOffset({
            x: drag.originX + (event.clientX - drag.x),
            y: drag.originY + (event.clientY - drag.y)
          });
        }}
        onPointerUp={(event) => {
          dragRef.current = null;
          event.currentTarget.releasePointerCapture(event.pointerId);
        }}
      >
        <div
          className="graph-canvas-stage"
          style={{ transform: `translate(${offset.x}px, ${offset.y}px) scale(${scale})` }}
        >
          {children}
        </div>
      </div>
    </div>
  );
}
