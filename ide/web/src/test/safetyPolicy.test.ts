import { describe, expect, it } from "vitest";
import {
  DEFAULT_SAFETY_POLICY,
  parseSafetyPolicyFromMapping,
  upsertSafetyPolicyInMapping,
  validateSafetyPolicy
} from "@/features/target/safetyPolicy";

describe("safetyPolicy", () => {
  it("parses safety directives from mapping comments", () => {
    const text = `# safety: watchdog_ms = 500
# safety: operator_enable = false
Motor, io/motor.txt, bool
`;
    const policy = parseSafetyPolicyFromMapping(text);
    expect(policy.watchdogMs).toBe(500);
    expect(policy.operatorEnableRequired).toBe(false);
  });

  it("upserts safety policy lines into mapping text", () => {
    const next = upsertSafetyPolicyInMapping("Motor, io/motor.txt, bool\n", {
      ...DEFAULT_SAFETY_POLICY,
      watchdogMs: 125
    });
    expect(next).toContain("# safety: watchdog_ms = 125");
  });

  it("flags missing e-stop when safety gating is enabled", () => {
    const issues = validateSafetyPolicy({ ...DEFAULT_SAFETY_POLICY, eStopChannel: "" });
    expect(issues.some((issue) => issue.severity === "error")).toBe(true);
  });
});
