import { describe, expect, it } from "vitest";
import { graphSnapshotLocal } from "@/features/graph/graphCache";
import { workspaceFiles } from "@/features/project/samples";

describe("graph cache", () => {
  it("returns the same snapshot object for repeated local graph builds", () => {
    const file = workspaceFiles.find((entry) => entry.name === "native_ladder.ld");
    expect(file).toBeTruthy();
    const first = graphSnapshotLocal(file!);
    const second = graphSnapshotLocal(file!);
    expect(second).toBe(first);
    expect(first.model.pous[0]?.networks.length).toBeGreaterThan(0);
  });
});
