import type { WorkspaceFile } from "@/types";
import { DEFAULT_TARGET_MAPPING_TEXT, isTargetMappingFile } from "@/features/target/targetMapping";

export type FileLanguageId = WorkspaceFile["languageId"];

const FILE_TEMPLATES: Record<FileLanguageId, string> = {
  st: `PROGRAM NewProgram
VAR
END_VAR
END_PROGRAM
`,
  il: `PROGRAM NewProgram
VAR
END_VAR
END_PROGRAM
`,
  ld: `PROGRAM NewLd
VAR
    Input : BOOL;
    Output : BOOL;
END_VAR
LADDER
RUNG
    CONTACT Input;
    COIL Output;
END_RUNG
END_LADDER
END_PROGRAM
`,
  sfc: `PROGRAM NewSequence
VAR
    Ready : BOOL := TRUE;
END_VAR

INITIAL_STEP Start;
STEP Run;
TRANSITION Go FROM Start TO Run := Ready;
END_TRANSITION;
END_PROGRAM
`,
  fbd: `PROGRAM NewFbd
VAR
    Enable : BOOL := TRUE;
    Output : BOOL;
END_VAR
FBD
NETWORK
    OUT Output := Enable;
END_NETWORK
END_FBD
END_PROGRAM
`,
  xml: `<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types>
    <pous>
      <pou name="NewProgram" pouType="program">
        <interface />
        <body><ST><xhtml xmlns="http://www.w3.org/1999/xhtml"></xhtml></ST></body>
      </pou>
    </pous>
  </types>
</project>
`,
  mapping: DEFAULT_TARGET_MAPPING_TEXT
};

export const FILE_LANGUAGE_OPTIONS: { id: FileLanguageId; label: string; extension: string }[] = [
  { id: "st", label: "Structured Text", extension: ".st" },
  { id: "il", label: "Instruction List", extension: ".il" },
  { id: "ld", label: "Ladder Diagram", extension: ".ld" },
  { id: "sfc", label: "Sequential Function Chart", extension: ".sfc" },
  { id: "fbd", label: "Function Block Diagram", extension: ".fbd" },
  { id: "xml", label: "PLCopen XML", extension: ".xml" }
];

export function languageFromFileName(name: string): FileLanguageId | null {
  if (isTargetMappingFile(name)) {
    return "mapping";
  }
  const extension = name.includes(".") ? name.slice(name.lastIndexOf(".")).toLowerCase() : "";
  const match = FILE_LANGUAGE_OPTIONS.find((option) => option.extension === extension);
  return match?.id ?? null;
}

export function defaultFileName(languageId: FileLanguageId, existingNames: string[]): string {
  const option = FILE_LANGUAGE_OPTIONS.find((entry) => entry.id === languageId)!;
  let index = 1;
  let candidate = `new${option.extension}`;
  while (existingNames.includes(candidate)) {
    index += 1;
    candidate = `new${index}${option.extension}`;
  }
  return candidate;
}

export function createFileTemplate(languageId: FileLanguageId, name: string): WorkspaceFile {
  return {
    name,
    languageId,
    text: FILE_TEMPLATES[languageId]
  };
}

export function uniqueFileName(name: string, existingNames: string[]): string | null {
  const trimmed = name.trim();
  if (!trimmed) {
    return null;
  }
  if (!existingNames.includes(trimmed)) {
    return trimmed;
  }
  const languageId = languageFromFileName(trimmed);
  if (!languageId) {
    return null;
  }
  const base = trimmed.slice(0, trimmed.lastIndexOf("."));
  const extension = trimmed.slice(trimmed.lastIndexOf("."));
  let index = 2;
  let candidate = `${base}_${index}${extension}`;
  while (existingNames.includes(candidate)) {
    index += 1;
    candidate = `${base}_${index}${extension}`;
  }
  return candidate;
}
