import type { WorkspaceFile } from "@/types";

export type LdCoilMode = "normal" | "set" | "reset";

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function formatComment(comment: string): string {
  const trimmed = comment.trim();
  if (!trimmed) {
    return "";
  }
  return `(* ${trimmed} *)`;
}

function commentPattern(): RegExp {
  return /^\s*\(\*([\s\S]*?)\*\)\s*$/;
}

function stripCommentLine(line: string): string | null {
  const match = line.match(commentPattern());
  return match ? match[1]?.trim() ?? "" : null;
}

function ldElementLinePattern(label: string): RegExp {
  return new RegExp(
    `^\\s*(?:CONTACT(?:_NOT|\\s+NOT)?\\s+${escapeRegExp(label)}|COIL(?:_NOT|\\s+NOT)?\\s+${escapeRegExp(label)}|SET\\s+${escapeRegExp(label)}|RESET\\s+${escapeRegExp(label)})\\s*;`,
    "i"
  );
}

function fbdOutputLinePattern(label: string): RegExp {
  return new RegExp(`^\\s*OUT\\s+${escapeRegExp(label)}\\s*:=`, "i");
}

function replaceLineBlock(lines: string[], index: number, nextLines: string[]): string {
  const updated = [...lines];
  updated.splice(index, 1, ...nextLines);
  return updated.join("\n");
}

function updateLineComment(lines: string[], index: number, comment: string): string {
  const hasLeadingComment = index > 0 && commentPattern().test(lines[index - 1] ?? "");
  const commentLine = formatComment(comment);

  if (!comment.trim()) {
    if (hasLeadingComment) {
      lines.splice(index - 1, 1);
      return lines.join("\n");
    }
    return lines.join("\n");
  }

  if (hasLeadingComment) {
    lines[index - 1] = `    ${commentLine}`;
    return lines.join("\n");
  }

  const indent = (lines[index]?.match(/^\s*/)?.[0] ?? "    ").replace(/\S/g, "");
  lines.splice(index, 0, `${indent}${commentLine}`);
  return lines.join("\n");
}

function editMatchingLine(text: string, linePattern: RegExp, mapper: (lines: string[], index: number) => string): string {
  const lines = text.split("\n");
  const index = lines.findIndex((line) => linePattern.test(line));
  if (index < 0) {
    return text;
  }
  return mapper(lines, index);
}

export function coilModeLine(label: string, mode: LdCoilMode): string {
  if (mode === "set") {
    return `    SET ${label};`;
  }
  if (mode === "reset") {
    return `    RESET ${label};`;
  }
  return `    COIL ${label};`;
}

export function setLdCoilMode(text: string, label: string, mode: LdCoilMode): string {
  if (!label) {
    return text;
  }
  const linePattern = new RegExp(
    `^\\s*(?:COIL(?:_NOT|\\s+NOT)?\\s+${escapeRegExp(label)}|SET\\s+${escapeRegExp(label)}|RESET\\s+${escapeRegExp(label)})\\s*;`,
    "i"
  );
  return editMatchingLine(text, linePattern, (lines, index) =>
    replaceLineBlock(lines, index, [coilModeLine(label, mode)])
  );
}

export function setGraphElementComment(
  file: WorkspaceFile,
  label: string,
  comment: string
): string {
  if (!label) {
    return file.text;
  }

  if (file.languageId === "ld") {
    return editMatchingLine(file.text, ldElementLinePattern(label), (lines, index) =>
      updateLineComment(lines, index, comment)
    );
  }

  if (file.languageId === "fbd" || file.languageId === "xml") {
    return editMatchingLine(file.text, fbdOutputLinePattern(label), (lines, index) =>
      updateLineComment(lines, index, comment)
    );
  }

  if (file.languageId === "sfc") {
    const actionPattern = new RegExp(`^\\s*ACTION\\s+${escapeRegExp(label)}\\s*:`, "i");
    const stepPattern = new RegExp(`^\\s*(?:INITIAL_)?STEP\\s+${escapeRegExp(label)}\\s*;?`, "i");
    const transitionPattern = new RegExp(`^\\s*TRANSITION\\s+${escapeRegExp(label)}\\b`, "i");
    for (const pattern of [actionPattern, stepPattern, transitionPattern]) {
      const next = editMatchingLine(file.text, pattern, (lines, index) => updateLineComment(lines, index, comment));
      if (next !== file.text) {
        return next;
      }
    }
  }

  return file.text;
}

export function parseLeadingComment(body: string, elementLine: string): string {
  const lines = body.split("\n");
  const index = lines.findIndex((line) => line.trim() === elementLine.trim());
  if (index <= 0) {
    return "";
  }
  return stripCommentLine(lines[index - 1] ?? "") ?? "";
}
