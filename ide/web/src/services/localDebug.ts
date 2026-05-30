import { runLocally } from "@/services/localRunner";
import { parseWatchList } from "@/stores/settingsStore";
import type { DebugCycle, DebugTrace, TraceVariable, WorkspaceFile } from "@/types";

export function debugLocally(file: WorkspaceFile, cycles: number, watchVariables: string): DebugTrace {
  const runTrace = runLocally(file, cycles);
  const watches = parseWatchList(watchVariables);

  return {
    uri: file.name,
    program: runTrace.program,
    cycles: runTrace.cycles.map((cycle) => toDebugCycle(cycle, watches, file.languageId))
  };
}

function toDebugCycle(
  cycle: { cycle: number; variables: TraceVariable[]; events: string[] },
  watches: string[],
  languageId: WorkspaceFile["languageId"]
): DebugCycle {
  const watchSet = new Set(watches.map((name) => name.toLowerCase()));
  const filteredWatches =
    watchSet.size === 0
      ? []
      : cycle.variables.filter((variable) => watchSet.has(variable.name.toLowerCase()));

  const activeSfcSteps = cycle.variables
    .filter((variable) => variable.name.startsWith("Step.") && variable.value === true)
    .map((variable) => variable.name.replace(/^Step\./, ""));

  return {
    cycle: cycle.cycle,
    recordedAt: new Date().toISOString(),
    watches: filteredWatches,
    variables: cycle.variables,
    accessPaths: [],
    activeSfcSteps: languageId === "sfc" ? activeSfcSteps : [],
    events: cycle.events
  };
}
