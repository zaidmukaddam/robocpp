import type { GeneratedCMetadata } from "@/types";
import type { TargetMappingEntry } from "@/features/target/targetMapping";

export type SymbolCoverageStatus =
  | "mapped"
  | "unmapped"
  | "stale"
  | "incompatible"
  | "read-only"
  | "retained"
  | "unsafe";

export type SymbolCoverageRow = {
  symbol: string;
  status: SymbolCoverageStatus;
  targetKind: string;
  detail: string;
};

export function buildSymbolCoverage(
  metadata: GeneratedCMetadata | null,
  entries: TargetMappingEntry[],
  options?: { staleSymbols?: Set<string>; readOnlySymbols?: Set<string> }
): SymbolCoverageRow[] {
  if (!metadata) {
    return [];
  }

  const rows: SymbolCoverageRow[] = [];
  const mapped = new Map(entries.map((entry) => [entry.symbol.toLowerCase(), entry]));
  const ioNames = new Set(metadata.ioSymbols.map((symbol) => symbol.name.toLowerCase()));
  const stateNames = new Set(metadata.stateLayout.map((field) => field.name.toLowerCase()));
  const retained = new Set(metadata.retainedFields.map((field) => field.toLowerCase()));

  const staleSymbols = options?.staleSymbols ?? new Set<string>();
  const readOnlySymbols = options?.readOnlySymbols ?? new Set(
    metadata?.ioSymbols.filter((symbol) => symbol.direction.toLowerCase() === "input").map((symbol) => symbol.name.toLowerCase()) ?? []
  );

  for (const entry of entries) {
    const symbol = entry.symbol;
    const lower = symbol.toLowerCase();
    let status: SymbolCoverageStatus = "mapped";
    let detail = `${entry.kind} → ${entry.target}`;

    if (entry.target.includes("..")) {
      status = "unsafe";
      detail = "Target path escapes project root";
    } else if (staleSymbols.has(lower)) {
      status = "stale";
      detail = "Mapping target no longer matches generated metadata";
    } else if (readOnlySymbols.has(lower) && entry.kind !== "file") {
      status = "read-only";
      detail = "Input symbol mapped to writable transport";
    } else if (!ioNames.has(lower) && !stateNames.has(lower)) {
      status = "incompatible";
      detail = "Symbol missing from generated I/O or state layout";
    } else if (retained.has(lower) && entry.kind === "file" && entry.encoding === "bool") {
      status = "retained";
      detail = "Retained bool file mapping needs deploy review";
    }

    rows.push({
      symbol,
      status,
      targetKind: entry.kind,
      detail
    });
  }

  for (const ioSymbol of metadata.ioSymbols) {
    if (!mapped.has(ioSymbol.name.toLowerCase())) {
      rows.push({
        symbol: ioSymbol.name,
        status: "unmapped",
        targetKind: "io",
        detail: `${ioSymbol.direction} ${ioSymbol.typeName}`
      });
    }
  }

  for (const field of metadata.stateLayout) {
    if (!mapped.has(field.name.toLowerCase())) {
      rows.push({
        symbol: field.name,
        status: "unmapped",
        targetKind: "state",
        detail: field.typeName
      });
    }
  }

  return rows.sort((left, right) => left.symbol.localeCompare(right.symbol));
}

export function coverageStatusLabel(status: SymbolCoverageStatus): string {
  switch (status) {
    case "mapped":
      return "mapped";
    case "unmapped":
      return "unmapped";
    case "stale":
      return "stale";
    case "read-only":
      return "read-only";
    case "incompatible":
      return "incompatible";
    case "retained":
      return "retained";
    case "unsafe":
      return "unsafe";
  }
}
