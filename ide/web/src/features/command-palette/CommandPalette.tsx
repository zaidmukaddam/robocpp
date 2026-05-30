import { useEffect, useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { WorkspaceFile } from "@/types";

export type CommandPaletteItem = {
  id: string;
  label: string;
  detail?: string;
  group: "Files" | "Commands" | "Navigation";
  shortcut?: string;
  run: () => void;
};

type CommandPaletteProps = {
  open: boolean;
  query: string;
  items: CommandPaletteItem[];
  onOpenChange: (open: boolean) => void;
  onQueryChange: (query: string) => void;
};

export function CommandPalette({ open, query, items, onOpenChange, onQueryChange }: CommandPaletteProps) {
  const [activeIndex, setActiveIndex] = useState(0);
  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) {
      return items;
    }
    return items.filter(
      (item) =>
        item.label.toLowerCase().includes(normalized) ||
        item.detail?.toLowerCase().includes(normalized) ||
        item.group.toLowerCase().includes(normalized)
    );
  }, [items, query]);

  useEffect(() => {
    setActiveIndex(0);
  }, [query, open]);

  useEffect(() => {
    if (!open) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onOpenChange(false);
      }
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setActiveIndex((index) => Math.min(index + 1, Math.max(filtered.length - 1, 0)));
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setActiveIndex((index) => Math.max(index - 1, 0));
      }
      if (event.key === "Enter" && filtered[activeIndex]) {
        event.preventDefault();
        filtered[activeIndex]?.run();
        onOpenChange(false);
        onQueryChange("");
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [activeIndex, filtered, onOpenChange, onQueryChange, open]);

  if (!open) {
    return null;
  }

  return (
    <div className="command-palette-backdrop" onClick={() => onOpenChange(false)}>
      <div className="command-palette" role="dialog" aria-label="Command palette" onClick={(event) => event.stopPropagation()}>
        <Input
          autoFocus
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
          placeholder="Type a command or file name…"
          aria-label="Command palette search"
        />
        <div className="command-palette-list" role="listbox">
          {filtered.length === 0 ? (
            <div className="empty-row">No matching commands.</div>
          ) : (
            filtered.map((item, index) => (
              <button
                key={item.id}
                type="button"
                role="option"
                aria-selected={index === activeIndex}
                className={`command-palette-item${index === activeIndex ? " active" : ""}`}
                onMouseEnter={() => setActiveIndex(index)}
                onClick={() => {
                  item.run();
                  onOpenChange(false);
                  onQueryChange("");
                }}
              >
                <span className="command-palette-group">{item.group}</span>
                <span className="command-palette-label">{item.label}</span>
                {item.detail ? <span className="command-palette-detail">{item.detail}</span> : null}
                {item.shortcut ? <span className="command-palette-shortcut">{item.shortcut}</span> : null}
              </button>
            ))
          )}
        </div>
        <div className="command-palette-footer">
          <Button type="button" variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
            Close
          </Button>
        </div>
      </div>
    </div>
  );
}

export function buildFilePaletteItems(files: WorkspaceFile[], onOpenFile: (fileName: string) => void): CommandPaletteItem[] {
  return files.map((file) => ({
    id: `file:${file.name}`,
    label: file.name,
    detail: file.languageId.toUpperCase(),
    group: "Files" as const,
    run: () => onOpenFile(file.name)
  }));
}
