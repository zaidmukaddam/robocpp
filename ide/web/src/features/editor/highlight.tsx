import React from "react";

const IEC_KEYWORDS = [
  "PROGRAM", "VAR", "END_VAR", "IF", "THEN", "ELSE", "END_IF", "END_PROGRAM",
  "LADDER", "RUNG", "CONTACT", "COIL", "END_RUNG", "END_LADDER",
  "INITIAL_STEP", "STEP", "TRANSITION", "ACTION", "END_ACTION",
  "FBD", "NETWORK", "END_NETWORK", "END_FBD", "OUT"
];

const IEC_TYPES = ["BOOL", "INT", "UINT", "DINT", "REAL", "STRING", "TIME"];
const IEC_LITERALS = ["TRUE", "FALSE"];

const IEC_PATTERN = new RegExp(
  `(\\b(?:${IEC_KEYWORDS.join("|")})\\b|\\b(?:${IEC_TYPES.join("|")})\\b|\\b(?:${IEC_LITERALS.join("|")})\\b|:=|<|>|;|\\/\\/.*)`,
  "gi"
);

export function highlightSource(source: string, languageId: string) {
  if (languageId === "xml") {
    return highlightXml(source);
  }
  return highlightIec(source);
}

function highlightIec(source: string) {
  return source.split("\n").map((line, lineIndex) => (
    <React.Fragment key={lineIndex}>
      {line.split(IEC_PATTERN).map((part, index) => {
        if (!part) {
          return null;
        }
        const upper = part.toUpperCase();
        let className = "";
        if (part.startsWith("//")) {
          className = "tok-comment";
        } else if (IEC_KEYWORDS.includes(upper)) {
          className = "tok-keyword";
        } else if (IEC_TYPES.includes(upper)) {
          className = "tok-type";
        } else if (IEC_LITERALS.includes(upper)) {
          className = "tok-literal";
        } else if ([":=", "<", ">", ";"].includes(part)) {
          className = "tok-operator";
        }
        return className ? (
          <span className={className} key={index}>
            {part}
          </span>
        ) : (
          part
        );
      })}
      {"\n"}
    </React.Fragment>
  ));
}

function highlightXml(source: string) {
  return source.split("\n").map((line, lineIndex) => (
    <React.Fragment key={lineIndex}>
      {line.split(/(<[^>]+>)/g).map((part, index) =>
        part.startsWith("<") ? (
          <span className="tok-keyword" key={index}>
            {part}
          </span>
        ) : (
          part
        )
      )}
      {"\n"}
    </React.Fragment>
  ));
}
