import type { DeployValidationIssue } from "@/features/target/targetDeployValidation";

export type SafetyPolicy = {
  watchdogMs: number;
  operatorEnableRequired: boolean;
  eStopChannel: string;
  protectiveStopEnabled: boolean;
  supervisorReporting: boolean;
  retainedStateStore: string;
  safetyGatingEnabled: boolean;
};

export const DEFAULT_SAFETY_POLICY: SafetyPolicy = {
  watchdogMs: 250,
  operatorEnableRequired: true,
  eStopChannel: "DI_ESTOP",
  protectiveStopEnabled: true,
  supervisorReporting: true,
  retainedStateStore: "io/retained.bin",
  safetyGatingEnabled: true
};

const SAFETY_PREFIX = "# safety:";

export function parseSafetyPolicyFromMapping(text: string): SafetyPolicy {
  const policy = { ...DEFAULT_SAFETY_POLICY };
  for (const rawLine of text.split("\n")) {
    const line = rawLine.trim();
    if (!line.toLowerCase().startsWith(SAFETY_PREFIX)) {
      continue;
    }
    const payload = line.slice(SAFETY_PREFIX.length).trim();
    const [key, value] = payload.split("=").map((part) => part.trim());
    if (!key || value === undefined) {
      continue;
    }
    switch (key.toLowerCase()) {
      case "watchdog_ms":
        policy.watchdogMs = Number(value) || policy.watchdogMs;
        break;
      case "operator_enable":
        policy.operatorEnableRequired = value === "true";
        break;
      case "e_stop_channel":
        policy.eStopChannel = value;
        break;
      case "protective_stop":
        policy.protectiveStopEnabled = value === "true";
        break;
      case "supervisor_reporting":
        policy.supervisorReporting = value === "true";
        break;
      case "retained_state_store":
        policy.retainedStateStore = value;
        break;
      case "safety_gating":
        policy.safetyGatingEnabled = value === "true";
        break;
      default:
        break;
    }
  }
  return policy;
}

export function serializeSafetyPolicyLines(policy: SafetyPolicy): string[] {
  return [
    "# [safety]",
    `${SAFETY_PREFIX} watchdog_ms = ${policy.watchdogMs}`,
    `${SAFETY_PREFIX} operator_enable = ${policy.operatorEnableRequired}`,
    `${SAFETY_PREFIX} e_stop_channel = ${policy.eStopChannel}`,
    `${SAFETY_PREFIX} protective_stop = ${policy.protectiveStopEnabled}`,
    `${SAFETY_PREFIX} supervisor_reporting = ${policy.supervisorReporting}`,
    `${SAFETY_PREFIX} retained_state_store = ${policy.retainedStateStore}`,
    `${SAFETY_PREFIX} safety_gating = ${policy.safetyGatingEnabled}`
  ];
}

export function upsertSafetyPolicyInMapping(text: string, policy: SafetyPolicy): string {
  const lines = text.split("\n");
  const withoutSafety = lines.filter((line) => !line.trim().toLowerCase().startsWith(SAFETY_PREFIX) && line.trim() !== "# [safety]");
  const safetyLines = serializeSafetyPolicyLines(policy);
  const headerEnd = withoutSafety.findIndex((line) => {
    const trimmed = line.trim();
    return trimmed && !trimmed.startsWith("#");
  });
  if (headerEnd < 0) {
    return [...safetyLines, "", ...withoutSafety].join("\n").trimEnd() + "\n";
  }
  return [...withoutSafety.slice(0, headerEnd), ...safetyLines, "", ...withoutSafety.slice(headerEnd)].join("\n").trimEnd() + "\n";
}

export function validateSafetyPolicy(policy: SafetyPolicy): DeployValidationIssue[] {
  const issues: DeployValidationIssue[] = [];
  if (policy.watchdogMs < 50 || policy.watchdogMs > 10_000) {
    issues.push({
      severity: "warning",
      message: `Watchdog ${policy.watchdogMs}ms is outside the recommended 50–10000ms range.`,
      remediation: "open-mapping"
    });
  }
  if (policy.safetyGatingEnabled && !policy.eStopChannel.trim()) {
    issues.push({
      severity: "error",
      message: "Safety gating is enabled but no E-stop channel is configured.",
      remediation: "open-mapping"
    });
  }
  if (policy.protectiveStopEnabled && policy.operatorEnableRequired === false) {
    issues.push({
      severity: "warning",
      message: "Protective stop is enabled without operator-enable gating.",
      remediation: "open-mapping"
    });
  }
  if (!policy.retainedStateStore.trim()) {
    issues.push({
      severity: "warning",
      message: "Retained-state store path is empty.",
      remediation: "open-mapping"
    });
  }
  if (policy.retainedStateStore.includes("..")) {
    issues.push({
      severity: "error",
      message: "Retained-state store path escapes the project root.",
      remediation: "open-mapping"
    });
  }
  return issues;
}
