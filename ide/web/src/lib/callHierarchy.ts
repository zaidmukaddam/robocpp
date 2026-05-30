import type { DocumentSymbol } from "@/types";

export type CallHierarchyNode = {
  symbol: DocumentSymbol;
  children: CallHierarchyNode[];
};

export function buildCallHierarchy(symbols: DocumentSymbol[]): CallHierarchyNode[] {
  const programs = symbols.filter((symbol) => symbol.kind === "program");
  return programs.map((program) => ({
    symbol: program,
    children: symbols
      .filter((symbol) => symbol.containerName === program.name)
      .map((symbol) => ({
        symbol,
        children: symbols.filter((child) => child.containerName === symbol.name).map((child) => ({
          symbol: child,
          children: []
        }))
      }))
  }));
}

export function flattenCallHierarchy(nodes: CallHierarchyNode[]): DocumentSymbol[] {
  const flat: DocumentSymbol[] = [];
  const walk = (node: CallHierarchyNode) => {
    flat.push(node.symbol);
    for (const child of node.children) {
      walk(child);
    }
  };
  for (const node of nodes) {
    walk(node);
  }
  return flat;
}
