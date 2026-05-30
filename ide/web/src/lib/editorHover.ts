import type { CompletionItem, Diagnostic, DocumentSymbol } from "@/types";
import type { TargetMappingEntry } from "@/features/target/targetMapping";

export type EditorHoverInfo = {
  title: string;
  detail: string;
  kind: string;
};

function wordAtCursor(text: string, cursor: number): { word: string; start: number; end: number } | null {
  const before = text.slice(0, cursor);
  const after = text.slice(cursor);
  const left = before.match(/[A-Za-z_][A-Za-z0-9_]*$/)?.[0] ?? "";
  const right = after.match(/^[A-Za-z_][A-Za-z0-9_]*/)?.[0] ?? "";
  const word = `${left}${right}`;
  if (!word) {
    return null;
  }
  return { word, start: cursor - left.length, end: cursor + right.length };
}

function lineAtCursor(text: string, cursor: number): number {
  return text.slice(0, cursor).split("\n").length;
}

export function hoverAtCursor(
  text: string,
  cursor: number,
  symbols: DocumentSymbol[],
  diagnostics: Diagnostic[],
  completions: CompletionItem[],
  targetBindings: TargetMappingEntry[] = []
): EditorHoverInfo | null {
  const token = wordAtCursor(text, cursor);
  if (!token) {
    return null;
  }

  const symbol = symbols.find((entry) => entry.name === token.word);
  if (symbol) {
    const binding = targetBindings.find((entry) => entry.symbol === token.word);
    return {
      title: symbol.name,
      detail: binding ? `${symbol.detail} · target ${binding.kind}:${binding.target}` : symbol.detail,
      kind: binding ? "symbol+binding" : symbol.kind
    };
  }

  const binding = targetBindings.find((entry) => entry.symbol === token.word);
  if (binding) {
    return {
      title: binding.symbol,
      detail: `${binding.kind} → ${binding.target}`,
      kind: "target-binding"
    };
  }

  const completion = completions.find((entry) => entry.label === token.word);
  if (completion) {
    return {
      title: completion.label,
      detail: completion.detail,
      kind: completion.kind
    };
  }

  const line = lineAtCursor(text, cursor);
  const diagnostic = diagnostics.find((entry) => entry.span?.line === line && entry.message.includes(token.word));
  if (diagnostic) {
    return {
      title: diagnostic.stableCode,
      detail: diagnostic.message,
      kind: diagnostic.severity
    };
  }

  return null;
}
