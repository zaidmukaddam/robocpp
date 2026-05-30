import { describe, expect, it } from "vitest";
import {
  findAllReferences,
  goToDefinitionTarget,
  renameSymbolInSource,
  wordAtOffset
} from "@/lib/symbolNavigation";
import { analyzeLocally } from "@/services/localAnalysis";
import type { WorkspaceFile } from "@/types";

const counterFile: WorkspaceFile = {
  name: "counter.st",
  languageId: "st",
  text: `PROGRAM Counter
VAR
    Count : INT := 0;
    Done : BOOL := FALSE;
END_VAR
    Count := Count + 1;
END_PROGRAM
`
};

describe("symbolNavigation", () => {
  it("finds the word at the cursor offset", () => {
    const token = wordAtOffset(counterFile.text, counterFile.text.indexOf("Done"));
    expect(token?.word).toBe("Done");
  });

  it("jumps to a variable declaration from a usage", () => {
    const analysis = analyzeLocally(counterFile);
    const usageOffset = counterFile.text.indexOf("Count + 1");
    const target = goToDefinitionTarget(analysis.symbols, counterFile.text, usageOffset);
    expect(target?.line).toBe(3);
  });

  it("finds all references for a symbol", () => {
    const references = findAllReferences(counterFile.text, "Count");
    expect(references.length).toBeGreaterThanOrEqual(2);
  });

  it("renames a symbol across the file", () => {
    const next = renameSymbolInSource(counterFile.text, "Done", "Finished");
    expect(next).toContain("Finished");
    expect(next).not.toContain("Done");
  });
});
