import { describe, expect, it } from "vitest";
import { quickFixesForDiagnostic } from "@/lib/diagnosticQuickFixes";

describe("diagnostic quick fixes", () => {
  it("suggests END_IF insertion", () => {
    const fixes = quickFixesForDiagnostic({
      severity: "error",
      code: "parse",
      stableCode: "MISSING_END_IF",
      message: "Expected END_IF before END_PROGRAM",
      span: null,
      help: null
    });
    expect(fixes.some((fix) => fix.id === "append-end-if")).toBe(true);
    const next = fixes.find((fix) => fix.id === "append-end-if")?.apply("IF TRUE THEN\n    x := 1;\n");
    expect(next).toContain("END_IF");
  });
});
