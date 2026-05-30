import type { DebugTrace, RunTrace } from "@/types";

export function buildTraceValueMap(runTrace: RunTrace | null, debugTrace: DebugTrace | null): Map<string, string> {
  const values = new Map<string, string>();
  const lastRun = runTrace?.cycles.at(-1);
  for (const variable of lastRun?.variables ?? []) {
    values.set(variable.name, String(variable.value));
  }
  const lastDebug = debugTrace?.cycles.at(-1);
  for (const variable of lastDebug?.variables ?? []) {
    values.set(variable.name, String(variable.value));
  }
  for (const watch of lastDebug?.watches ?? []) {
    values.set(watch.name, String(watch.value));
  }
  return values;
}

export function buildTraceLabelSet(runTrace: RunTrace | null, debugTrace: DebugTrace | null): Set<string> {
  const labels = new Set<string>();
  const lastRun = runTrace?.cycles.at(-1);
  for (const variable of lastRun?.variables ?? []) {
    if (variable.value === true) {
      labels.add(variable.name);
    }
  }
  const lastDebug = debugTrace?.cycles.at(-1);
  for (const variable of lastDebug?.variables ?? []) {
    if (variable.value === true) {
      labels.add(variable.name);
    }
  }
  for (const variable of lastDebug?.watches ?? []) {
    if (variable.value === true) {
      labels.add(variable.name);
    }
  }
  return labels;
}

export function activeSfcSteps(debugTrace: DebugTrace | null): Set<string> {
  const last = debugTrace?.cycles.at(-1);
  return new Set(last?.activeSfcSteps ?? []);
}
