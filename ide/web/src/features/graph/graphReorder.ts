import type { WorkspaceFile } from "@/types";

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function swapLines(text: string, lineA: string, lineB: string): string | null {
  if (lineA === lineB || !text.includes(lineA) || !text.includes(lineB)) {
    return null;
  }
  const placeholderA = "__SWAP_A__";
  const placeholderB = "__SWAP_B__";
  const staged = text.replace(lineA, placeholderA).replace(lineB, placeholderB);
  return staged.replace(placeholderA, lineB).replace(placeholderB, lineA);
}

export function reorderLdNodes(text: string, fromLabel: string, toLabel: string): string | null {
  if (!fromLabel || !toLabel || fromLabel === toLabel) {
    return null;
  }
  const fromLine = text.match(new RegExp(`^\\s*CONTACT\\s+(?:NOT\\s+)?${escapeRegExp(fromLabel)};.*$`, "m"))?.[0];
  const toLine = text.match(new RegExp(`^\\s*CONTACT\\s+(?:NOT\\s+)?${escapeRegExp(toLabel)};.*$`, "m"))?.[0];
  if (fromLine && toLine) {
    return swapLines(text, fromLine, toLine);
  }
  const fromCoil = text.match(new RegExp(`^\\s*COIL\\s+(?:SET|RESET\\s+)?${escapeRegExp(fromLabel)};.*$`, "m"))?.[0];
  const toCoil = text.match(new RegExp(`^\\s*COIL\\s+(?:SET|RESET\\s+)?${escapeRegExp(toLabel)};.*$`, "m"))?.[0];
  if (fromCoil && toCoil) {
    return swapLines(text, fromCoil, toCoil);
  }
  return null;
}

export function reorderFbdNodes(text: string, fromLabel: string, toLabel: string): string | null {
  if (!fromLabel || !toLabel || fromLabel === toLabel) {
    return null;
  }
  const pattern = new RegExp(`\\b${escapeRegExp(fromLabel)}\\b([^)]*)\\)`, "i");
  const reversePattern = new RegExp(`\\b${escapeRegExp(toLabel)}\\b([^)]*)\\)`, "i");
  const fromMatch = text.match(pattern);
  const toMatch = text.match(reversePattern);
  if (!fromMatch || !toMatch) {
    return null;
  }
  const fromArgs = fromMatch[1] ?? "";
  const toArgs = toMatch[1] ?? "";
  let next = text.replace(pattern, `__FROM__${toArgs})`);
  next = next.replace(reversePattern, `__TO__${fromArgs})`);
  return next.replace("__FROM__", fromLabel).replace("__TO__", toLabel);
}

export function reorderGraphNodes(
  file: WorkspaceFile,
  fromLabel: string,
  toLabel: string
): string | null {
  if (file.languageId === "ld") {
    return reorderLdNodes(file.text, fromLabel, toLabel);
  }
  if (file.languageId === "fbd" || file.languageId === "xml") {
    return reorderFbdNodes(file.text, fromLabel, toLabel);
  }
  return null;
}

export function moveSfcStep(text: string, stepName: string, direction: "up" | "down"): string | null {
  const stepPattern = new RegExp(`^\\s*(?:INITIAL_STEP|STEP)\\s+${escapeRegExp(stepName)}\\s*;\\s*$`, "m");
  const match = text.match(stepPattern);
  if (!match) {
    return null;
  }
  const line = match[0];
  const lines = text.split("\n");
  const index = lines.findIndex((entry) => entry === line);
  if (index < 0) {
    return null;
  }
  const swapIndex = direction === "up" ? index - 1 : index + 1;
  if (swapIndex < 0 || swapIndex >= lines.length) {
    return null;
  }
  const swapLine = lines[swapIndex];
  if (!/^\s*(?:INITIAL_STEP|STEP)\s+/i.test(swapLine)) {
    return null;
  }
  const next = [...lines];
  next[index] = swapLine;
  next[swapIndex] = line;
  return next.join("\n");
}
