import type { DocumentSymbol } from "@/types";

export type SourceLocation = {
  line: number;
  character: number;
  offset: number;
};

const IDENT = /[A-Za-z_][A-Za-z0-9_]*/;

export function wordAtOffset(
  text: string,
  offset: number
): { word: string; start: number; end: number } | null {
  const clamped = Math.min(Math.max(offset, 0), text.length);
  const before = text.slice(0, clamped);
  const after = text.slice(clamped);
  const left = before.match(new RegExp(`${IDENT.source}$`))?.[0] ?? "";
  const right = after.match(new RegExp(`^${IDENT.source}`))?.[0] ?? "";
  const word = `${left}${right}`;
  if (!word) {
    return null;
  }
  return { word, start: clamped - left.length, end: clamped + right.length };
}

export function offsetFromLineColumn(text: string, line: number, character = 0): number {
  const lines = text.split("\n");
  const target = Math.min(Math.max(line, 1), lines.length);
  let offset = 0;
  for (let index = 0; index < target - 1; index += 1) {
    offset += (lines[index]?.length ?? 0) + 1;
  }
  return offset + character;
}

export function lineColumnFromOffset(text: string, offset: number): { line: number; character: number } {
  const before = text.slice(0, Math.min(Math.max(offset, 0), text.length));
  const line = before.split("\n").length;
  const lastNewline = before.lastIndexOf("\n");
  const character = lastNewline < 0 ? before.length : before.length - lastNewline - 1;
  return { line, character };
}

export function locationFromSymbol(symbol: DocumentSymbol, text: string): SourceLocation {
  const line = (symbol.range?.startPosition.line ?? 0) + 1;
  const character = symbol.range?.startPosition.character ?? 0;
  return { line, character, offset: offsetFromLineColumn(text, line, character) };
}

export function symbolAtCursor(symbols: DocumentSymbol[], text: string, offset: number): DocumentSymbol | null {
  const token = wordAtOffset(text, offset);
  if (!token) {
    return null;
  }
  const matches = symbols.filter((symbol) => symbol.name === token.word);
  if (matches.length === 0) {
    return null;
  }
  const { line } = lineColumnFromOffset(text, offset);
  const onDeclaration = matches.find((symbol) => (symbol.range?.startPosition.line ?? -1) + 1 === line);
  return onDeclaration ?? matches[0] ?? null;
}

export function goToDefinitionTarget(
  symbols: DocumentSymbol[],
  text: string,
  offset: number
): SourceLocation | null {
  const token = wordAtOffset(text, offset);
  if (!token) {
    return null;
  }

  const declaration = symbols.find((symbol) => symbol.name === token.word);
  if (!declaration?.range) {
    return null;
  }

  const { line } = lineColumnFromOffset(text, offset);
  const declLine = declaration.range.startPosition.line + 1;
  if (declLine === line) {
    if (declaration.containerName) {
      const container = symbols.find((symbol) => symbol.name === declaration.containerName);
      if (container) {
        return locationFromSymbol(container, text);
      }
    }
    return locationFromSymbol(declaration, text);
  }

  return locationFromSymbol(declaration, text);
}

export function findAllReferences(text: string, symbolName: string): SourceLocation[] {
  if (!symbolName) {
    return [];
  }
  const pattern = new RegExp(`\\b${escapeRegExp(symbolName)}\\b`, "g");
  const locations: SourceLocation[] = [];
  for (const match of text.matchAll(pattern)) {
    const offset = match.index ?? 0;
    const { line, character } = lineColumnFromOffset(text, offset);
    locations.push({ line, character, offset });
  }
  return locations;
}

export function renameSymbolInSource(text: string, oldName: string, newName: string): string | null {
  if (!oldName || !newName || oldName === newName) {
    return null;
  }
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(newName)) {
    return null;
  }
  const pattern = new RegExp(`\\b${escapeRegExp(oldName)}\\b`, "g");
  if (!pattern.test(text)) {
    return null;
  }
  return text.replace(new RegExp(`\\b${escapeRegExp(oldName)}\\b`, "g"), newName);
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
