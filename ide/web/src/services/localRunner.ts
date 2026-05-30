import type { RunTrace, WorkspaceFile } from "@/types";

export function runLocally(file: WorkspaceFile, cycles = 5): RunTrace {
  if (file.languageId === "ld") {
    return runLadder(file, cycles);
  }
  if (file.languageId === "fbd" || file.languageId === "xml") {
    return runFbd(file, cycles);
  }
  if (file.languageId === "sfc") {
    return runSfc(file, cycles);
  }
  return runCounter(file, cycles);
}

function runCounter(file: WorkspaceFile, cycles: number): RunTrace {
  return {
    program: "Counter",
    source: file.name,
    cycles: Array.from({ length: cycles }, (_, index) => {
      const count = index + 1;
      return {
        cycle: count,
        variables: [
          { name: "Count", value: count },
          { name: "Done", value: count >= 10 }
        ],
        events: [count === 1 ? "Initial scan" : "Incremented Count"]
      };
    }),
    generatedC: `typedef struct {
    int64_t count;
    bool done;
} counter_state;

void counter_scan(counter_state *s) {
    if (s->count < 10) {
        s->count += 1;
    } else {
        s->done = true;
    }
}`
  };
}

function runLadder(file: WorkspaceFile, cycles: number): RunTrace {
  return {
    program: "NativeLd",
    source: file.name,
    cycles: Array.from({ length: cycles }, (_, index) => {
      const start = index % 2 === 0;
      return {
        cycle: index + 1,
        variables: [
          { name: "Start", value: start },
          { name: "Motor", value: start }
        ],
        events: [start ? "Rung energized: Motor coil set" : "Rung open: Motor coil cleared"]
      };
    }),
    generatedC: `void nativeld_scan(nativeld_state *s) {
    s->motor = s->start;
}`
  };
}

function runFbd(file: WorkspaceFile, cycles: number): RunTrace {
  return {
    program: file.languageId === "xml" ? "Blocks" : "NativeFbd",
    source: file.name,
    cycles: Array.from({ length: cycles }, (_, index) => {
      const enabled = index > 0;
      const interlock = index !== 3;
      return {
        cycle: index + 1,
        variables: [
          { name: "Enable", value: enabled },
          { name: "Interlock", value: interlock },
          { name: "MotorCmd", value: enabled && interlock }
        ],
        events: [enabled && interlock ? "FBD network output true" : "FBD network output false"]
      };
    }),
    generatedC: `void nativefbd_scan(nativefbd_state *s) {
    s->motorcmd = s->enable && s->interlock;
}`
  };
}

function runSfc(file: WorkspaceFile, cycles: number): RunTrace {
  return {
    program: "Sequence",
    source: file.name,
    cycles: Array.from({ length: cycles }, (_, index) => {
      const running = index > 0;
      return {
        cycle: index + 1,
        variables: [
          { name: "Ready", value: true },
          { name: "Step.Start", value: !running },
          { name: "Step.Run", value: running },
          { name: "Done", value: running }
        ],
        events: [running ? "Transition Go fired; Run action active" : "Initial step Start active"]
      };
    }),
    generatedC: `void sequence_scan(sequence_state *s) {
    if (s->step_start && s->ready) {
        s->step_start = false;
        s->step_run = true;
    }
    if (s->step_run) {
        s->done = true;
    }
}`
  };
}
