import { ChevronDown } from "lucide-react";
import type { ReactNode } from "react";

type PaneHeaderProps = {
  title: string;
  count?: string;
  onClose?: () => void;
  closeLabel?: string;
  action?: ReactNode;
};

export function PaneHeader({ title, count, onClose, closeLabel, action }: PaneHeaderProps) {
  return (
    <div className="pane-header">
      <span>
        {title}
        {count ? (
          <>
            {" "}
            · <strong>{count}</strong>
          </>
        ) : null}
      </span>
      <div className="pane-header-actions">
        {action}
        {onClose ? (
          <button type="button" className="pane-header-btn" aria-label={closeLabel} onClick={onClose}>
            <ChevronDown size={14} aria-hidden="true" />
          </button>
        ) : null}
      </div>
    </div>
  );
}
