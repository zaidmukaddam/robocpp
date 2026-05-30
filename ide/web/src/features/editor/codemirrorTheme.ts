import { EditorView } from "@codemirror/view";

export const robocppEditorTheme = EditorView.theme(
  {
    "&": {
      height: "100%",
      width: "100%",
      fontSize: "13px",
      fontFamily: "var(--font-mono)",
      backgroundColor: "var(--bg-editor)",
      color: "var(--text-primary)"
    },
    "&.cm-focused": {
      outline: "none"
    },
    ".cm-scroller": {
      overflow: "auto",
      lineHeight: "20px",
      fontFamily: "inherit"
    },
    ".cm-content": {
      padding: "12px 16px",
      caretColor: "var(--text-primary)",
      backgroundColor: "var(--bg-editor)",
      color: "var(--text-primary)"
    },
    ".cm-line": {
      backgroundColor: "transparent"
    },
    ".cm-gutters": {
      backgroundColor: "var(--bg-editor)",
      color: "var(--text-muted)",
      borderRight: "1px solid var(--border-subtle)",
      fontFamily: "var(--font-mono)",
      fontSize: "12px"
    },
    ".cm-breakpoint-gutter": {
      width: "14px",
      minWidth: "14px",
      backgroundColor: "var(--bg-editor)",
      borderRight: "1px solid var(--border-subtle)"
    },
    ".cm-breakpoint-gutter .cm-gutterElement": {
      cursor: "pointer",
      padding: "0 2px"
    },
    ".cm-activeLineGutter": {
      backgroundColor: "rgba(0, 120, 212, 0.12)"
    },
    ".cm-activeLine": {
      backgroundColor: "rgba(0, 120, 212, 0.08)"
    },
    ".cm-current-line": {
      backgroundColor: "rgba(255, 193, 7, 0.18)"
    },
    ".cm-breakpoint": {
      color: "#e51400",
      fontWeight: "700"
    },
    ".cm-diagnostic-error": {
      textDecoration: "underline wavy #e51400"
    },
    ".cm-diagnostic-warning": {
      textDecoration: "underline wavy #bf8803"
    },
    ".cm-diagnostic-note": {
      textDecoration: "underline wavy #3794ff"
    },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
      backgroundColor: "rgba(0, 120, 212, 0.35) !important"
    },
    ".cm-cursor": {
      borderLeftColor: "var(--text-primary)"
    }
  },
  { dark: true }
);
