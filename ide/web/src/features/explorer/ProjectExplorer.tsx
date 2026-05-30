import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, type CSSProperties, type MouseEvent } from "react";
import { createPortal } from "react-dom";
import { FilePlus2, Pencil, Trash2 } from "lucide-react";
import { FileTree, useFileTree } from "@pierre/trees/react";
import type {
  ContextMenuItem as FileTreeContextMenuItem,
  ContextMenuOpenContext,
  FileTreeRenameEvent
} from "@pierre/trees";
import { Button } from "@/components/ui/button";
import {
  explorerPaths,
  fileToTreePath,
  findLatestArtifactByName,
  isGeneratedArtifactPath,
  isTargetMappingTreePath,
  isTreeFilePath,
  treePathToArtifactName,
  treePathToProjectFileName,
  treePathsSignature
} from "@/features/explorer/projectTreePaths";
import type { Project, ProjectArtifact } from "@/types";

export type ProjectExplorerHandle = {
  focusFile: (fileName: string) => void;
};

type ProjectExplorerProps = {
  project: Project;
  artifacts: ProjectArtifact[];
  activeFileName: string;
  onSelectFile: (fileName: string) => void;
  onSelectArtifact: (artifactId: string | null) => void;
  onRenameFile: (oldName: string, newName: string) => void;
  onDeleteFile: (fileName: string) => void;
  onReorderFile: (fileName: string, beforeFileName: string) => void;
  onNewFile: () => void;
};

const TREE_THEME_STYLE: CSSProperties = {
  height: "100%",
  minHeight: 0,
  "--trees-bg-override": "transparent",
  "--trees-fg-override": "var(--text-primary)",
  "--trees-fg-muted-override": "var(--text-muted)",
  "--trees-border-color-override": "var(--border-subtle)",
  "--trees-selected-bg-override": "var(--bg-active)",
  "--trees-bg-muted-override": "var(--bg-hover)",
  "--trees-focus-ring-color-override": "var(--border-focus)",
  "--trees-padding-inline-override": "8px",
  "--trees-item-margin-x-override": "0px",
  "--trees-item-padding-x-override": "6px",
  "--trees-item-row-gap-override": "6px",
  "--trees-border-radius-override": "4px",
  "--trees-search-bg-override": "var(--bg-editor)"
} as CSSProperties;

const TREE_UNSAFE_CSS = `
  [data-file-tree-search-container] {
    padding-top: 4px;
    margin-bottom: 2px;
  }

  [data-type='item'] {
    min-width: 0;
  }

  [data-item-section='content'] {
    flex: 0 1 auto;
    min-width: 0;
    max-width: 100%;
  }

  /* Pierre reserves a flex-growing decoration lane for git/custom badges.
     When empty it still eats the row and pushes labels away from the menu. */
  [data-item-section='decoration']:empty {
    flex: 0 0 0;
    width: 0;
    min-width: 0;
    overflow: hidden;
  }

  /* Keep extension-split labels compact when they fit; truncate only when narrow. */
  [data-item-section='content'] [data-truncate-group-container='middle'] {
    width: fit-content;
    max-width: 100%;
  }

  [data-item-section='content']
    [data-truncate-group-container='middle']
    > div[data-truncate-segment-priority='2'] {
    flex: 0 0 auto;
  }

  [data-type='item'][data-item-type='folder']:not([data-item-selected='true']):not(:hover) {
    background-color: var(--trees-bg);
    --truncate-marker-background-overlay-color: transparent;
  }

  [data-type='item'][data-item-context-hover='true']:not([data-item-selected='true']) {
    background-color: var(--trees-bg-muted);
    --truncate-marker-background-overlay-color: var(--trees-bg-muted);
  }
`;

export const ProjectExplorer = forwardRef<ProjectExplorerHandle, ProjectExplorerProps>(
  function ProjectExplorer(
    { project, artifacts, activeFileName, onSelectFile, onSelectArtifact, onRenameFile, onDeleteFile, onReorderFile, onNewFile },
    ref
  ) {
    const pathsSignature = `${treePathsSignature(project.files)}|${artifacts.map((entry) => entry.id).join(",")}`;
    const paths = useMemo(() => explorerPaths(project.files, artifacts), [pathsSignature]);
    const activePath = useMemo(() => {
      const file = project.files.find((entry) => entry.name === activeFileName);
      return file ? fileToTreePath(file) : paths[0] ?? null;
    }, [activeFileName, paths, pathsSignature]);

    const handlersRef = useRef({
      onSelectFile,
      onSelectArtifact,
      onRenameFile,
      onDeleteFile,
      onReorderFile,
      onNewFile
    });
    handlersRef.current = {
      onSelectFile,
      onSelectArtifact,
      onRenameFile,
      onDeleteFile,
      onReorderFile,
      onNewFile
    };

    const syncingRef = useRef(false);
    const activeFileNameRef = useRef(activeFileName);
    activeFileNameRef.current = activeFileName;
    const projectRef = useRef(project);
    projectRef.current = project;
    const artifactsRef = useRef(artifacts);
    artifactsRef.current = artifacts;

    const { model } = useFileTree({
      paths,
      search: true,
      density: "compact",
      initialExpansion: "open",
      flattenEmptyDirectories: true,
      fileTreeSearchMode: "hide-non-matches",
      icons: "standard",
      unsafeCSS: TREE_UNSAFE_CSS,
      initialSelectedPaths: activePath ? [activePath] : [],
      dragAndDrop: {
        canDrag: (draggedPaths) =>
          draggedPaths.every((path) => isTreeFilePath(path) && !isTargetMappingTreePath(path)),
        canDrop: ({ draggedPaths, target }) => {
          if (draggedPaths.length !== 1) {
            return false;
          }
          const sourceFolder = draggedPaths[0]?.split("/")[0];
          if (target.hoveredPath && isTreeFilePath(target.hoveredPath)) {
            return target.hoveredPath.split("/")[0] === sourceFolder;
          }
          return target.directoryPath === sourceFolder;
        },
        onDropComplete: ({ draggedPaths, target }) => {
          const sourceName = treePathToProjectFileName(draggedPaths[0] ?? "", projectRef.current.files);
          const hovered = target.hoveredPath;
          if (!sourceName || !hovered || !isTreeFilePath(hovered)) {
            return;
          }
          handlersRef.current.onReorderFile(sourceName, treePathToProjectFileName(hovered, projectRef.current.files));
        }
      },
      composition: {
        contextMenu: {
          enabled: true,
          triggerMode: "both",
          buttonVisibility: "when-needed"
        }
      },
      renaming: {
        canRename: (item) => !item.isFolder && !isGeneratedArtifactPath(item.path) && !isTargetMappingTreePath(item.path),
        onRename: (event: FileTreeRenameEvent) => {
          if (!isTreeFilePath(event.sourcePath)) {
            return;
          }
          const oldName = treePathToProjectFileName(event.sourcePath, projectRef.current.files);
          const newName = treePathToProjectFileName(event.destinationPath, projectRef.current.files);
          handlersRef.current.onRenameFile(oldName, newName);
        }
      }
    });

    useEffect(() => {
      if (!activePath) {
        return;
      }
      const current = model.getSelectedPaths();
      if (current.length === 1 && current[0] === activePath) {
        return;
      }
      syncingRef.current = true;
      for (const selectedPath of current) {
        model.getItem(selectedPath)?.deselect();
      }
      model.getItem(activePath)?.select();
      model.scrollToPath(activePath, { focus: false });
      syncingRef.current = false;
    }, [activeFileName, activePath, model]);

    const handleExplorerClick = useCallback(
      (event: MouseEvent<HTMLDivElement>) => {
        const row = event.nativeEvent.composedPath().find(
          (node): node is HTMLElement => node instanceof HTMLElement && Boolean(node.dataset.itemPath)
        );
        if (!row?.dataset.itemPath) {
          return;
        }
        if (isGeneratedArtifactPath(row.dataset.itemPath)) {
          const artifactName = treePathToArtifactName(row.dataset.itemPath);
          const artifact = findLatestArtifactByName(artifactsRef.current, artifactName);
          if (artifact) {
            handlersRef.current.onSelectArtifact(artifact.id);
          }
          return;
        }
        if (!isTreeFilePath(row.dataset.itemPath)) {
          return;
        }
        const fileName = treePathToProjectFileName(row.dataset.itemPath, projectRef.current.files);
        if (fileName !== activeFileNameRef.current) {
          handlersRef.current.onSelectFile(fileName);
        }
      },
      []
    );

    useImperativeHandle(
      ref,
      () => ({
        focusFile(fileName: string) {
          const file = projectRef.current.files.find((entry) => entry.name === fileName);
          if (!file) {
            return;
          }
          handlersRef.current.onSelectFile(fileName);
        }
      }),
      []
    );

    return (
      <div className="project-explorer-host" onClickCapture={handleExplorerClick}>
        <FileTree
          model={model}
          className="project-file-tree"
          style={TREE_THEME_STYLE}
          aria-label={`${project.name} project files`}
          renderContextMenu={(item, context) => (
            <PortaledTreeContextMenu
              item={item}
              context={context}
              canDelete={project.files.length > 1}
              onDelete={() => {
                if (item.kind === "file") {
                  handlersRef.current.onDeleteFile(treePathToProjectFileName(item.path, project.files));
                }
                context.close();
              }}
              onNewFile={() => {
                handlersRef.current.onNewFile();
                context.close();
              }}
              onRename={() => {
                context.close({ restoreFocus: false });
                model.startRenaming(item.path);
              }}
            />
          )}
        />
      </div>
    );
  }
);

function PortaledTreeContextMenu({
  item,
  context,
  canDelete,
  onNewFile,
  onRename,
  onDelete
}: {
  item: FileTreeContextMenuItem;
  context: ContextMenuOpenContext;
  canDelete: boolean;
  onNewFile: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  const menuWidth = 176;
  const viewportPadding = 8;
  const left = Math.min(
    Math.max(context.anchorRect.left, viewportPadding),
    window.innerWidth - menuWidth - viewportPadding
  );
  const top = Math.min(context.anchorRect.bottom + 4, window.innerHeight - viewportPadding);

  return createPortal(
    <div
      className="tree-context-menu tree-context-menu-portal"
      data-file-tree-context-menu-root="true"
      role="menu"
      style={{ top, left, width: menuWidth }}
    >
      <Button type="button" variant="ghost" size="sm" className="tree-context-item" onClick={onNewFile}>
        <FilePlus2 size={14} aria-hidden="true" />
        New file
      </Button>
      {item.kind === "file" ? (
        <>
          <Button type="button" variant="ghost" size="sm" className="tree-context-item" onClick={onRename}>
            <Pencil size={14} aria-hidden="true" />
            Rename
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="tree-context-item destructive"
            disabled={!canDelete}
            onClick={onDelete}
          >
            <Trash2 size={14} aria-hidden="true" />
            Delete
          </Button>
        </>
      ) : null}
    </div>,
    document.body
  );
}
