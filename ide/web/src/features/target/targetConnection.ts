export type TargetKind = "simulator" | "hardware";

export type TargetConnectionState =
  | "offline"
  | "connecting"
  | "online"
  | "running"
  | "stopped"
  | "simulated"
  | "error"
  | "stale";

export type TargetConnection = {
  kind: TargetKind;
  label: string;
  address: string;
  state: TargetConnectionState;
  runtimeVersion: string | null;
  programHash: string | null;
  deployHash: string | null;
  editorMatchesTarget: boolean;
  lastError: string | null;
};

const STORAGE_KEY = "robocpp-studio-target-connection-v1";

export const DEFAULT_SIMULATOR_TARGET: TargetConnection = {
  kind: "simulator",
  label: "Local simulator",
  address: "sim://localhost",
  state: "simulated",
  runtimeVersion: "robocpp-sim",
  programHash: null,
  deployHash: null,
  editorMatchesTarget: true,
  lastError: null
};

export const DEFAULT_HARDWARE_TARGET: TargetConnection = {
  kind: "hardware",
  label: "PLC target",
  address: "file://local",
  state: "offline",
  runtimeVersion: null,
  programHash: null,
  deployHash: null,
  editorMatchesTarget: false,
  lastError: null
};

export function readTargetConnection(): TargetConnection {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return DEFAULT_SIMULATOR_TARGET;
    }
    const parsed = JSON.parse(raw) as Partial<TargetConnection>;
    const base = parsed.kind === "hardware" ? DEFAULT_HARDWARE_TARGET : DEFAULT_SIMULATOR_TARGET;
    return {
      ...base,
      ...parsed,
      kind: parsed.kind === "hardware" ? "hardware" : "simulator",
      state: isTargetConnectionState(parsed.state) ? parsed.state : base.state
    };
  } catch {
    return DEFAULT_SIMULATOR_TARGET;
  }
}

export function persistTargetConnection(connection: TargetConnection): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(connection));
}

export function targetStateLabel(state: TargetConnectionState): string {
  switch (state) {
    case "offline":
      return "Offline";
    case "connecting":
      return "Connecting";
    case "online":
      return "Online";
    case "running":
      return "Running";
    case "stopped":
      return "Stopped";
    case "simulated":
      return "Simulated";
    case "error":
      return "Error";
    case "stale":
      return "Stale";
  }
}

export function targetStateExplanation(connection: TargetConnection): string {
  if (connection.lastError) {
    return connection.lastError;
  }
  if (connection.kind === "simulator") {
    return "F5 runs the local scan simulator. No hardware connection is required.";
  }
  if (connection.state === "offline") {
    return "Start the target bridge, then connect to read mapped I/O from file or Modbus.";
  }
  if (!connection.editorMatchesTarget) {
    return "Editor project differs from the program on the target.";
  }
  return "Hardware target uses the local bridge for file-backed I/O and Modbus TCP.";
}

function isTargetConnectionState(value: unknown): value is TargetConnectionState {
  return (
    value === "offline" ||
    value === "connecting" ||
    value === "online" ||
    value === "running" ||
    value === "stopped" ||
    value === "simulated" ||
    value === "error" ||
    value === "stale"
  );
}
