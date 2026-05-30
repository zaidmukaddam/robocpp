import type { Analysis, DocumentSymbol } from "@/types";

export type TargetMappingKind = "file" | "modbus" | "ethercat" | "ros2";

export type TargetMappingEntry = {
  id: string;
  kind: TargetMappingKind;
  symbol: string;
  target: string;
  encoding?: string;
  notes?: string;
};

export type TargetMappingDocument = {
  entries: TargetMappingEntry[];
};

export const DEFAULT_TARGET_MAPPING_TEXT = `# key, relative_file, encoding
Motor, io/motor.txt, bool
Count, io/count.txt, decimal
`;

export function parseTargetMapping(text: string): TargetMappingDocument {
  const entries: TargetMappingEntry[] = [];
  for (const [index, rawLine] of text.split("\n").entries()) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) {
      continue;
    }
    if (line.startsWith("modbus:")) {
      const [, payload] = line.split(":", 2);
      const [symbol, target] = (payload ?? "").split("=").map((part) => part.trim());
      if (symbol && target) {
        entries.push({ id: `modbus-${index}`, kind: "modbus", symbol, target });
      }
      continue;
    }
    if (line.startsWith("ethercat:")) {
      const [, payload] = line.split(":", 2);
      const [symbol, target] = (payload ?? "").split("=").map((part) => part.trim());
      if (symbol && target) {
        entries.push({ id: `ethercat-${index}`, kind: "ethercat", symbol, target });
      }
      continue;
    }
    if (line.startsWith("ros2:")) {
      const [, payload] = line.split(":", 2);
      const [symbol, target] = (payload ?? "").split("=").map((part) => part.trim());
      if (symbol && target) {
        entries.push({ id: `ros2-${index}`, kind: "ros2", symbol, target });
      }
      continue;
    }
    const parts = line.split(",").map((part) => part.trim());
    const symbol = parts[0];
    const target = parts[1];
    if (!symbol || !target) {
      continue;
    }
    entries.push({
      id: `file-${index}`,
      kind: "file",
      symbol,
      target,
      encoding: parts[2] ?? "decimal"
    });
  }
  return { entries };
}

export function serializeTargetMapping(document: TargetMappingDocument): string {
  const lines = ["# key, relative_file, encoding"];
  for (const entry of document.entries) {
    if (entry.kind === "file") {
      lines.push(`${entry.symbol}, ${entry.target}, ${entry.encoding ?? "decimal"}`);
      continue;
    }
    lines.push(`${entry.kind}: ${entry.symbol} = ${entry.target}`);
  }
  return `${lines.join("\n")}\n`;
}

export function validateTargetMapping(document: TargetMappingDocument): string[] {
  const issues: string[] = [];
  const seen = new Set<string>();
  for (const entry of document.entries) {
    if (!entry.symbol.trim()) {
      issues.push("Every mapping needs a PLC symbol.");
      continue;
    }
    const key = entry.symbol.toLowerCase();
    if (seen.has(key)) {
      issues.push(`Duplicate mapping for symbol ${entry.symbol}.`);
    }
    seen.add(key);
    if (!entry.target.trim()) {
      issues.push(`Mapping for ${entry.symbol} is missing a target.`);
    }
    if (entry.kind === "modbus" && !/^\d+:\w+:\d+$/i.test(entry.target)) {
      issues.push(`Modbus target for ${entry.symbol} should look like 1:coil:0.`);
    }
  }
  return issues;
}

export function mappingFileName(): string {
  return "target/mapping.toml";
}

export function isTargetMappingFile(fileName: string): boolean {
  return fileName === mappingFileName() || fileName.endsWith("/mapping.toml");
}

export function analyzeTargetMapping(fileName: string, text: string): Analysis {
  const document = parseTargetMapping(text);
  const issues = validateTargetMapping(document);
  const diagnostics = issues.map((message) => ({
    severity: "error" as const,
    code: "mapping.validation",
    stableCode: "mapping.validation",
    message,
    span: null,
    help: "Use comma-separated rows: Symbol, target_path, encoding."
  }));
  const symbols: DocumentSymbol[] = document.entries.map((entry) => ({
    name: entry.symbol,
    kind: "mapping",
    detail: `${entry.kind} → ${entry.target}`,
    containerName: "target",
    range: null
  }));
  return {
    uri: fileName,
    diagnostics,
    symbols,
    completions: []
  };
}
