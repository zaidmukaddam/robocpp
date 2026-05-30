import { describe, expect, it } from "vitest";
import { recordTrendSeries } from "@/lib/traceTrend";
import { diffLines } from "@/lib/artifactCompare";
import { buildCallHierarchy } from "@/lib/callHierarchy";
import { graphStaleWarnings } from "@/lib/graphStaleSource";
import { buildLocalGraphModel } from "@/features/graph/localGraphModel";
import type { DebugTrace } from "@/types";

const trace: DebugTrace = {
  uri: "counter.st",
  program: "Counter",
  cycles: [
    {
      cycle: 1,
      recordedAt: new Date().toISOString(),
      watches: [{ name: "Count", value: 1 }],
      variables: [{ name: "Count", value: 1 }],
      accessPaths: [],
      activeSfcSteps: [],
      events: []
    },
    {
      cycle: 2,
      recordedAt: new Date().toISOString(),
      watches: [{ name: "Count", value: 2 }],
      variables: [{ name: "Count", value: 2 }],
      accessPaths: [],
      activeSfcSteps: [],
      events: []
    }
  ]
};

describe("remaining product features", () => {
  it("records trend series across simulation cycles", () => {
    const series = recordTrendSeries([], trace, ["Count"]);
    expect(series[0]?.points.length).toBe(2);
  });

  it("diffs artifact text lines", () => {
    const rows = diffLines("a\nb\n", "a\nc\n");
    expect(rows.some((row) => row.kind === "change")).toBe(true);
  });

  it("builds a call hierarchy from symbols", () => {
    const tree = buildCallHierarchy([
      { name: "Counter", kind: "program", detail: "PROGRAM", containerName: null, range: null },
      { name: "Count", kind: "variable", detail: "VAR : INT", containerName: "Counter", range: null }
    ]);
    expect(tree[0]?.children.length).toBe(1);
  });

  it("warns when graph model diverges from source", () => {
    const file = { name: "test.ld", languageId: "ld" as const, text: "LADDER\nRUNG\n    CONTACT A;\n    COIL B;\nEND_RUNG\nEND_LADDER\n" };
    const model = buildLocalGraphModel(file);
    const warnings = graphStaleWarnings({ ...file, text: "LADDER\nEND_LADDER\n" }, model);
    expect(warnings.length).toBeGreaterThan(0);
  });
});
