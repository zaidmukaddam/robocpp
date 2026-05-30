import type { Diagnostic } from "@/types";

export type DiagnosticQuickFix = {
  id: string;
  label: string;
  apply: (text: string) => string | null;
};

export function quickFixesForDiagnostic(diagnostic: Diagnostic): DiagnosticQuickFix[] {
  const fixes: DiagnosticQuickFix[] = [];
  const message = diagnostic.message.toLowerCase();
  const code = diagnostic.stableCode.toLowerCase();

  if (message.includes("end_if") || code.includes("end_if")) {
    fixes.push({
      id: "append-end-if",
      label: "Append END_IF",
      apply: (text) => (text.trimEnd().endsWith("END_IF;") ? null : `${text.trimEnd()}\nEND_IF;\n`)
    });
  }

  if (message.includes("end_program") || code.includes("end_program")) {
    fixes.push({
      id: "append-end-program",
      label: "Append END_PROGRAM",
      apply: (text) => (text.includes("END_PROGRAM") ? null : `${text.trimEnd()}\nEND_PROGRAM\n`)
    });
  }

  if (message.includes("end_var") || code.includes("end_var")) {
    fixes.push({
      id: "append-end-var",
      label: "Append END_VAR",
      apply: (text) => (text.includes("END_VAR") ? null : `${text.trimEnd()}\nEND_VAR\n`)
    });
  }

  if (message.includes("unknown symbol") || message.includes("undeclared")) {
    const match = diagnostic.message.match(/'([^']+)'|symbol\s+([A-Za-z_][A-Za-z0-9_]*)/i);
    const symbol = match?.[1] ?? match?.[2];
    if (symbol) {
      fixes.push({
        id: `declare-${symbol}`,
        label: `Declare ${symbol} in VAR`,
        apply: (text) => insertVariableDeclaration(text, symbol)
      });
    }
  }

  return fixes;
}

function insertVariableDeclaration(text: string, symbol: string): string | null {
  if (text.includes(`${symbol} :`)) {
    return null;
  }
  const varMatch = text.match(/^(\s*VAR\b[^\n]*\n)/im);
  if (!varMatch) {
    return null;
  }
  const insertion = `${varMatch[1]}    ${symbol} : BOOL;\n`;
  return text.replace(varMatch[0], insertion);
}
