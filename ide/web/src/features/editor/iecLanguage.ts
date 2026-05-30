import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { StreamLanguage } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

const IEC_KEYWORDS = new Set([
  "PROGRAM", "VAR", "VAR_INPUT", "VAR_OUTPUT", "VAR_IN_OUT", "VAR_GLOBAL", "VAR_TEMP", "END_VAR",
  "IF", "THEN", "ELSE", "ELSIF", "END_IF", "FOR", "TO", "BY", "DO", "END_FOR", "WHILE", "END_WHILE",
  "CASE", "OF", "END_CASE", "REPEAT", "UNTIL", "END_REPEAT", "RETURN", "EXIT", "END_PROGRAM",
  "LADDER", "RUNG", "CONTACT", "COIL", "END_RUNG", "END_LADDER", "NOT", "SET", "RESET",
  "INITIAL_STEP", "STEP", "TRANSITION", "FROM", "TO", "ACTION", "END_ACTION", "END_TRANSITION",
  "FBD", "NETWORK", "END_NETWORK", "END_FBD", "OUT", "AND", "OR", "XOR", "MOD"
]);

const IEC_TYPES = new Set(["BOOL", "INT", "UINT", "DINT", "REAL", "STRING", "WSTRING", "TIME", "DATE", "TOD"]);

const iecLanguage = StreamLanguage.define({
  token(stream) {
    if (stream.eatSpace()) {
      return null;
    }
    if (stream.match("//")) {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.match("(*")) {
      while (!stream.eol()) {
        if (stream.match("*)")) {
          break;
        }
        stream.next();
      }
      return "comment";
    }
    if (stream.match(/:=|<=|>=|<>|=>|[<>;=()+\-*/]/)) {
      return "operator";
    }
    if (stream.match(/[0-9]+(\.[0-9]+)?/)) {
      return "number";
    }
    if (stream.match(/'[^']*'/)) {
      return "string";
    }
    if (stream.match(/[A-Za-z_][A-Za-z0-9_]*/)) {
      const word = stream.current().toUpperCase();
      if (IEC_KEYWORDS.has(word)) {
        return "keyword";
      }
      if (IEC_TYPES.has(word)) {
        return "typeName";
      }
      if (word === "TRUE" || word === "FALSE") {
        return "bool";
      }
      return "variableName";
    }
    stream.next();
    return null;
  }
});

const iecHighlight = HighlightStyle.define([
  { tag: t.comment, color: "var(--tok-comment, #6a9955)" },
  { tag: t.keyword, color: "var(--tok-keyword, #569cd6)" },
  { tag: t.typeName, color: "var(--tok-type, #4ec9b0)" },
  { tag: t.bool, color: "var(--tok-literal, #ce9178)" },
  { tag: t.number, color: "var(--tok-literal, #b5cea8)" },
  { tag: t.string, color: "var(--tok-literal, #ce9178)" },
  { tag: t.operator, color: "var(--tok-operator, #d4d4d4)" },
  { tag: t.variableName, color: "var(--text-primary, #d4d4d4)" }
]);

export const iecLanguageExtension = [iecLanguage, syntaxHighlighting(iecHighlight)];
