import { describe, expect, it } from "vitest";
import {
  DEFAULT_SIMULATOR_TARGET,
  persistTargetConnection,
  readTargetConnection,
  targetStateExplanation,
  targetStateLabel
} from "@/features/target/targetConnection";

describe("target connection", () => {
  it("labels connection states for the status bar", () => {
    expect(targetStateLabel("simulated")).toBe("Simulated");
    expect(targetStateLabel("connecting")).toBe("Connecting");
  });

  it("round-trips the selected target through localStorage", () => {
    persistTargetConnection({ ...DEFAULT_SIMULATOR_TARGET, label: "Bench sim" });
    expect(readTargetConnection().label).toBe("Bench sim");
  });

  it("explains simulator mode without hardware", () => {
    expect(targetStateExplanation(DEFAULT_SIMULATOR_TARGET)).toContain("simulator");
  });
});
