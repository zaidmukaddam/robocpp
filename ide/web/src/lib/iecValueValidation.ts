export function normalizeIecType(type: string): string {
  return type.trim().toUpperCase().replace(/\s+/g, "");
}

export function validateIecValue(value: string, iecType: string): string | null {
  const type = normalizeIecType(iecType);
  const trimmed = value.trim();
  if (!trimmed) {
    return "Value is required.";
  }

  if (type === "BOOL" || type === "BOOLEAN") {
    if (/^(TRUE|FALSE|0|1)$/i.test(trimmed)) {
      return null;
    }
    return "BOOL expects TRUE, FALSE, 0, or 1.";
  }

  if (/^(S?INT|US?INT|DINT|UDINT|LINT|ULINT|BYTE|WORD|DWORD|LWORD)$/.test(type)) {
    if (/^-?\d+$/.test(trimmed)) {
      return null;
    }
    return `${type} expects an integer literal.`;
  }

  if (type === "REAL" || type === "LREAL") {
    if (/^-?\d+(\.\d+)?([eE][+-]?\d+)?$/.test(trimmed)) {
      return null;
    }
    return `${type} expects a numeric literal.`;
  }

  if (type === "STRING" || type.startsWith("STRING(")) {
    if (
      (trimmed.startsWith("'") && trimmed.endsWith("'")) ||
      (trimmed.startsWith('"') && trimmed.endsWith('"'))
    ) {
      return null;
    }
    return "STRING expects a quoted literal, for example 'value'.";
  }

  if (type === "TIME" || type.startsWith("TIME_")) {
    if (/^-?\d+(?:\.\d+)?(?:d|h|m|s|ms)$/i.test(trimmed) || /^T#/i.test(trimmed)) {
      return null;
    }
    return "TIME expects a duration literal such as T#1s or 100ms.";
  }

  return null;
}

export function canonicalIecValue(value: string, iecType: string): string {
  const type = normalizeIecType(iecType);
  const trimmed = value.trim();
  if (type === "BOOL" || type === "BOOLEAN") {
    if (/^(1|TRUE)$/i.test(trimmed)) {
      return "TRUE";
    }
    if (/^(0|FALSE)$/i.test(trimmed)) {
      return "FALSE";
    }
  }
  return trimmed;
}
