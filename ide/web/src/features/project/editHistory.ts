type FileHistory = {
  past: string[];
  present: string;
  future: string[];
};

export type EditHistory = {
  init: (fileName: string, text: string) => void;
  has: (fileName: string) => boolean;
  push: (fileName: string, nextText: string) => void;
  undo: (fileName: string) => string | null;
  redo: (fileName: string) => string | null;
  canUndo: (fileName: string) => boolean;
  canRedo: (fileName: string) => boolean;
};

export function createEditHistory(limit = 80): EditHistory {
  const stacks = new Map<string, FileHistory>();

  const get = (fileName: string): FileHistory | null => stacks.get(fileName) ?? null;

  return {
    init(fileName, text) {
      if (!stacks.has(fileName)) {
        stacks.set(fileName, { past: [], present: text, future: [] });
      }
    },
    has(fileName) {
      return stacks.has(fileName);
    },
    push(fileName, nextText) {
      const current = stacks.get(fileName);
      if (!current) {
        stacks.set(fileName, { past: [], present: nextText, future: [] });
        return;
      }
      if (current.present === nextText) {
        return;
      }
      const past = [...current.past, current.present].slice(-limit);
      stacks.set(fileName, { past, present: nextText, future: [] });
    },
    undo(fileName) {
      const current = get(fileName);
      if (!current || current.past.length === 0) {
        return null;
      }
      const previous = current.past.at(-1)!;
      const past = current.past.slice(0, -1);
      const future = [current.present, ...current.future];
      stacks.set(fileName, { past, present: previous, future });
      return previous;
    },
    redo(fileName) {
      const current = get(fileName);
      if (!current || current.future.length === 0) {
        return null;
      }
      const [next, ...future] = current.future;
      const past = [...current.past, current.present];
      stacks.set(fileName, { past, present: next, future });
      return next;
    },
    canUndo(fileName) {
      return (get(fileName)?.past.length ?? 0) > 0;
    },
    canRedo(fileName) {
      return (get(fileName)?.future.length ?? 0) > 0;
    }
  };
}
