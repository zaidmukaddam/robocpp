import { describe, expect, it } from "vitest";
import { reorderLdNodes, moveSfcStep } from "@/features/graph/graphReorder";

const ladder = `LADDER
RUNG
    CONTACT InputA;
    CONTACT InputB;
    COIL Output;
END_RUNG
END_LADDER
`;

describe("graphReorder", () => {
  it("swaps ladder contacts when reordering nodes", () => {
    const next = reorderLdNodes(ladder, "InputA", "InputB");
    const inputAIndex = next?.indexOf("InputA") ?? -1;
    const inputBIndex = next?.indexOf("InputB") ?? -1;
    expect(next).toBeTruthy();
    expect(inputAIndex).toBeGreaterThan(inputBIndex);
  });

  it("moves an SFC step down", () => {
    const sfc = `STEP Start;
STEP Run;
END_PROGRAM
`;
    const next = moveSfcStep(sfc, "Start", "down");
    expect(next?.indexOf("STEP Run")).toBeLessThan(next?.indexOf("STEP Start") ?? 0);
  });
});
