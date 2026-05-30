export type KeyboardShortcut = {
  keys: string;
  action: string;
};

export const IDE_KEYBOARD_SHORTCUTS: KeyboardShortcut[] = [
  { keys: "F5", action: "Run simulation" },
  { keys: "F7", action: "Check project" },
  { keys: "Shift+F7", action: "Compliance report" },
  { keys: "Cmd+S / Ctrl+S", action: "Save project" },
  { keys: "Cmd+F / Ctrl+F", action: "Find in active file" },
  { keys: "Cmd+Shift+F / Ctrl+Shift+F", action: "Replace in active file" },
  { keys: "Ctrl+Space", action: "Show completions at cursor" },
  { keys: "Cmd+Shift+P / Ctrl+Shift+P", action: "Command palette" },
  { keys: "Cmd+Z / Ctrl+Z", action: "Undo edit" },
  { keys: "Cmd+Shift+Z / Ctrl+Shift+Z", action: "Redo edit" }
];
