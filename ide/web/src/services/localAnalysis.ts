import type { Analysis, DocumentSymbol, WorkspaceFile } from "@/types";
import { analyzeTargetMapping, isTargetMappingFile } from "@/features/target/targetMapping";

const typeNames = new Set([
  "BOOL",
  "INT",
  "UINT",
  "DINT",
  "REAL",
  "STRING",
  "WSTRING",
  "TIME"
]);

export function analyzeLocally(file: WorkspaceFile): Analysis {
  if (isTargetMappingFile(file.name) || file.languageId === "mapping") {
    return analyzeTargetMapping(file.name, file.text);
  }
  const symbols = collectSymbols(file);
  const diagnostics = sampleDiagnostics(file.name);

  return {
    uri: file.name,
    diagnostics,
    symbols,
    completions: [
      ...symbols.map((symbol) => ({
        label: symbol.name,
        kind: symbol.kind,
        detail: symbol.detail
      })),
      ...["IF", "THEN", "ELSE", "END_IF", "VAR", "END_VAR", "PROGRAM", "END_PROGRAM"].map(
        (label) => ({
          label,
          kind: "keyword",
          detail: "IEC keyword"
        })
      )
    ]
  };
}

function collectSymbols(file: WorkspaceFile): DocumentSymbol[] {
  const lines = file.text.split("\n");
  const symbols: DocumentSymbol[] = [];
  let containerName: string | null = null;
  let currentVarScope = "VAR";

  lines.forEach((line, index) => {
    const program = line.match(/^\s*PROGRAM\s+([A-Za-z_][A-Za-z0-9_]*)/i);
    if (program) {
      containerName = program[1];
      symbols.push(symbol(file, program[1], "program", "PROGRAM", null, index + 1));
      return;
    }

    const varBlock = line.match(/^\s*(VAR_INPUT|VAR_OUTPUT|VAR_IN_OUT|VAR_GLOBAL|VAR_TEMP|VAR)\b/i);
    if (varBlock) {
      currentVarScope = varBlock[1].toUpperCase();
      return;
    }

    const variable = line.match(/^\s*([A-Za-z_][A-Za-z0-9_]*)\s*:\s*([A-Za-z_][A-Za-z0-9_]*)/);
    if (variable) {
      const typeName = variable[2].toUpperCase();
      symbols.push(
        symbol(
          file,
          variable[1],
          "variable",
          `${currentVarScope} : ${typeNames.has(typeName) ? typeName : variable[2]}`,
          containerName,
          index + 1
        )
      );
      return;
    }

    const step = line.match(/^\s*(INITIAL_STEP|STEP)\s+([A-Za-z_][A-Za-z0-9_]*)/i);
    if (step) {
      symbols.push(symbol(file, step[2], "sfcStep", step[1].toUpperCase(), containerName, index + 1));
      return;
    }

    const action = line.match(/^\s*ACTION\s+([A-Za-z_][A-Za-z0-9_]*)/i);
    if (action) {
      symbols.push(symbol(file, action[1], "sfcAction", "SFC action", containerName, index + 1));
      return;
    }

    const transition = line.match(/^\s*TRANSITION\s+([A-Za-z_][A-Za-z0-9_]*)/i);
    if (transition) {
      symbols.push(symbol(file, transition[1], "sfcStep", "SFC transition", containerName, index + 1));
    }
  });

  if (file.languageId === "xml" && symbols.length === 0) {
    const pou = file.text.match(/pou name="([^"]+)"/i);
    if (pou) {
      symbols.push(symbol(file, pou[1], "program", "PLCopen PROGRAM", null, 5));
    }
  }

  return symbols;
}

function symbol(
  file: WorkspaceFile,
  name: string,
  kind: string,
  detail: string,
  containerName: string | null,
  line: number
): DocumentSymbol {
  return {
    name,
    kind,
    detail,
    containerName,
    range: {
      uri: file.name,
      start: 0,
      end: 0,
      startPosition: { line: line - 1, character: 0 },
      endPosition: { line: line - 1, character: name.length }
    }
  };
}

function sampleDiagnostics(fileName: string): Analysis["diagnostics"] {
  if (fileName !== "counter.st") {
    return [];
  }

  return [
    {
      severity: "warning",
      code: "semantic",
      stableCode: "RBCPP-SEMANTIC",
      message: "Variable 'Done' is assigned but not observed.",
      span: {
        source: "counter.st",
        start: 44,
        end: 48,
        line: 4,
        column: 5
      },
      help: "Bind the output to a PROGRAM instance output, VAR_ACCESS, or target mapping."
    },
    {
      severity: "note",
      code: "compliance",
      stableCode: "RBCPP-COMPLIANCE",
      message: "Document conforms to the 2003-strict source profile.",
      span: null,
      help: null
    }
  ];
}
