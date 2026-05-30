import { PanelLeftClose, PanelRightClose, X } from "lucide-react";
import type { WorkspaceFile } from "@/types";
import { languageBadge } from "@/features/layout/languageBadge";

type EditorTabBarProps = {
  openTabs: WorkspaceFile[];
  activeFileName: string;
  dirtyFileNames: Set<string>;
  leftOpen: boolean;
  rightOpen: boolean;
  onSelectFile: (name: string) => void;
  onCloseTab: (name: string) => void;
  onOpenLeft: () => void;
  onOpenRight: () => void;
};

function tabPanelId(fileName: string) {
  return `editor-panel-${fileName.replace(/[^a-zA-Z0-9_-]/g, "-")}`;
}

export function EditorTabBar({
  openTabs,
  activeFileName,
  dirtyFileNames,
  leftOpen,
  rightOpen,
  onSelectFile,
  onCloseTab,
  onOpenLeft,
  onOpenRight
}: EditorTabBarProps) {
  const canCloseAny = openTabs.length > 1;

  return (
    <div className="editor-tab-bar">
      {!leftOpen ? (
        <button
          type="button"
          className="pane-reopen-btn"
          aria-label="Show explorer"
          title="Show explorer"
          onClick={onOpenLeft}
        >
          <PanelLeftClose size={14} aria-hidden="true" />
        </button>
      ) : null}

      <div className="editor-tabs" role="tablist" aria-label="Open files">
        {openTabs.map((file) => {
          const active = file.name === activeFileName;
          const canClose = canCloseAny;
          const dirty = dirtyFileNames.has(file.name);
          const badge = languageBadge(file.languageId);
          const panelId = tabPanelId(file.name);

          return (
            <div
              key={file.name}
              className={`editor-tab${active ? " active-tab" : ""}`}
              onMouseDown={(event) => {
                if (event.button === 1 && canClose) {
                  event.preventDefault();
                  onCloseTab(file.name);
                }
              }}
            >
              <button
                type="button"
                role="tab"
                id={`tab-${panelId}`}
                aria-selected={active}
                aria-controls={panelId}
                tabIndex={active ? 0 : -1}
                className="editor-tab-label"
                title={file.name}
                onClick={() => onSelectFile(file.name)}
              >
                <span className={`editor-tab-lang ${badge.tone}`} aria-hidden="true">
                  {badge.label}
                </span>
                <span className="editor-tab-name">{file.name}</span>
                {dirty ? (
                  <span className="editor-tab-dirty" aria-label="Unsaved changes">
                    ●
                  </span>
                ) : null}
              </button>
              {canClose ? (
                <button
                  type="button"
                  className="editor-tab-close"
                  aria-label={`Close ${file.name}`}
                  title={`Close ${file.name}`}
                  onClick={(event) => {
                    event.stopPropagation();
                    onCloseTab(file.name);
                  }}
                >
                  <X size={12} aria-hidden="true" />
                </button>
              ) : null}
            </div>
          );
        })}
      </div>

      {!rightOpen ? (
        <button
          type="button"
          className="pane-reopen-btn pane-reopen-right"
          aria-label="Show inspector"
          title="Show inspector"
          onClick={onOpenRight}
        >
          <PanelRightClose size={14} aria-hidden="true" />
        </button>
      ) : null}
    </div>
  );
}

export { tabPanelId };
