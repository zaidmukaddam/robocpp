import { useState } from "react";
import { ChevronDown } from "lucide-react";
import {
  TargetActionDialog,
  type TargetHardwareAction
} from "@/features/dialogs/TargetActionDialog";
import {
  bridgeErrorMessage,
  connectTargetBridge,
  controlTargetBridge,
  deployTargetBridge,
  disconnectTargetBridge,
  pingTargetBridge,
  readTargetIo,
  targetBridgeUrl
} from "@/features/target/targetBridgeClient";
import {
  DEFAULT_HARDWARE_TARGET,
  DEFAULT_SIMULATOR_TARGET,
  persistTargetConnection,
  targetStateExplanation,
  targetStateLabel,
  type TargetConnection,
  type TargetConnectionState
} from "@/features/target/targetConnection";

type TargetConnectionBarProps = {
  connection: TargetConnection;
  onChange: (connection: TargetConnection) => void;
  runState: "idle" | "running" | "complete";
  bridgeUrl?: string;
  modbusPort?: number;
  projectId: string;
  mappingText: string;
  workspaceRoot?: string;
  programHash: string;
  generatedC?: string | null;
  deployPackage?: string | null;
  onIoSnapshot?: (values: Awaited<ReturnType<typeof readTargetIo>>) => void;
};

const HARDWARE_ACTIONS = [
  { id: "connect", label: "Connect" },
  { id: "refresh", label: "Refresh I/O" },
  { id: "download", label: "Download", confirm: true as const },
  { id: "run", label: "Run", confirm: true as const },
  { id: "stop", label: "Stop", confirm: true as const },
  { id: "reset", label: "Reset", confirm: true as const },
  { id: "disconnect", label: "Disconnect" }
] as const;

function mapBridgeState(
  state: string,
  running: boolean
): TargetConnectionState {
  if (running || state === "running") {
    return "running";
  }
  if (state === "offline") {
    return "offline";
  }
  if (state === "stopped") {
    return "stopped";
  }
  return "online";
}

export function TargetConnectionBar({
  connection,
  onChange,
  runState,
  bridgeUrl,
  modbusPort = 502,
  projectId,
  mappingText,
  workspaceRoot,
  programHash,
  generatedC,
  deployPackage,
  onIoSnapshot
}: TargetConnectionBarProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [bridgeReachable, setBridgeReachable] = useState<boolean | null>(null);
  const [pendingAction, setPendingAction] = useState<TargetHardwareAction | null>(null);
  const resolvedBridgeUrl = targetBridgeUrl(bridgeUrl);

  const probeBridge = async () => {
    try {
      await pingTargetBridge(resolvedBridgeUrl);
      setBridgeReachable(true);
    } catch {
      setBridgeReachable(false);
    }
  };

  const openMenu = () => {
    setMenuOpen((open) => {
      const next = !open;
      if (next && connection.kind === "hardware") {
        void probeBridge();
      }
      return next;
    });
  };
  const displayState: TargetConnectionState =
    connection.kind === "simulator"
      ? runState === "running"
        ? "running"
        : runState === "complete"
          ? "stopped"
          : "simulated"
      : connection.state;

  const update = (patch: Partial<TargetConnection>) => {
    const next = { ...connection, ...patch };
    onChange(next);
    persistTargetConnection(next);
  };

  const selectTarget = (kind: TargetConnection["kind"]) => {
    const next = kind === "hardware" ? { ...DEFAULT_HARDWARE_TARGET } : { ...DEFAULT_SIMULATOR_TARGET };
    onChange(next);
    persistTargetConnection(next);
    setMenuOpen(false);
  };

  const runHardwareAction = async (
    actionId: (typeof HARDWARE_ACTIONS)[number]["id"],
    needsConfirm?: boolean
  ) => {
    if (
      needsConfirm &&
      (actionId === "download" || actionId === "run" || actionId === "stop" || actionId === "reset")
    ) {
      setPendingAction(actionId);
      return;
    }
    await executeHardwareAction(actionId);
  };

  const executeHardwareAction = async (actionId: (typeof HARDWARE_ACTIONS)[number]["id"]) => {
    if (busy) {
      return;
    }

    setBusy(true);
    try {
      if (actionId === "connect") {
        update({ state: "connecting", lastError: null });
        const session = await connectTargetBridge(resolvedBridgeUrl, {
          address: connection.address,
          port: modbusPort,
          projectId,
          mappingText,
          workspaceRoot,
          programHash,
          simulate: connection.address.startsWith("sim://")
        });
        update({
          state: mapBridgeState(session.state, session.running),
          runtimeVersion: session.runtimeVersion,
          programHash: session.programHash ?? programHash,
          deployHash: session.deployHash,
          editorMatchesTarget: session.editorMatchesTarget,
          lastError: null
        });
        onIoSnapshot?.(session.values ?? []);
        setMenuOpen(false);
        return;
      }

      if (actionId === "disconnect") {
        await disconnectTargetBridge(resolvedBridgeUrl);
        update({ ...DEFAULT_HARDWARE_TARGET });
        onIoSnapshot?.([]);
        setMenuOpen(false);
        return;
      }

      if (actionId === "refresh") {
        const values = await readTargetIo(resolvedBridgeUrl);
        onIoSnapshot?.(values);
        setMenuOpen(false);
        return;
      }

      if (actionId === "download") {
        const result = await deployTargetBridge(resolvedBridgeUrl, {
          projectId,
          mappingText,
          workspaceRoot,
          deployPackage: deployPackage ?? undefined,
          generatedC: generatedC ?? undefined,
          programHash
        });
        update({
          state: "stopped",
          deployHash: result.deployHash,
          editorMatchesTarget: true,
          lastError: null
        });
        setMenuOpen(false);
        return;
      }

      if (actionId === "run" || actionId === "stop" || actionId === "reset") {
        const session = await controlTargetBridge(resolvedBridgeUrl, actionId);
        update({
          state: mapBridgeState(session.state, session.running),
          lastError: null
        });
        if (actionId === "run" || actionId === "reset") {
          const values = await readTargetIo(resolvedBridgeUrl);
          onIoSnapshot?.(values);
        }
        setMenuOpen(false);
      }
    } catch (error) {
      update({
        state: "error",
        lastError: bridgeErrorMessage(error)
      });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="target-connection-bar">
      <TargetActionDialog
        open={pendingAction !== null}
        action={pendingAction}
        targetLabel={connection.label}
        targetAddress={connection.address}
        programHash={programHash}
        deployHash={connection.deployHash}
        editorMatchesTarget={connection.editorMatchesTarget ?? false}
        onOpenChange={(open) => {
          if (!open) {
            setPendingAction(null);
          }
        }}
        onConfirm={() => {
          if (pendingAction) {
            void executeHardwareAction(pendingAction);
          }
        }}
      />
      <button
        type="button"
        className="target-connection-trigger"
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        title={targetStateExplanation({ ...connection, state: displayState })}
        onClick={openMenu}
        disabled={busy}
      >
        <span className={`target-state-dot state-${displayState}`} aria-hidden="true" />
        <span>{targetStateLabel(displayState)}</span>
        <span className="target-connection-label">{connection.label}</span>
        <span className="target-connection-address">{connection.address}</span>
        <ChevronDown size={12} aria-hidden="true" />
      </button>
      {menuOpen ? (
        <div className="target-connection-menu" role="menu">
          <div className="target-connection-menu-section">
            <span>Target</span>
            <button type="button" role="menuitem" onClick={() => selectTarget("simulator")}>
              Local simulator
            </button>
            <button type="button" role="menuitem" onClick={() => selectTarget("hardware")}>
              Hardware PLC
            </button>
          </div>
          {connection.kind === "hardware" ? (
            <>
              <div className="target-connection-menu-section">
                <label className="target-connection-field">
                  <span>Target address</span>
                  <input
                    type="text"
                    value={connection.address}
                    spellCheck={false}
                    placeholder="file://local or 192.168.0.10 or sim://modbus"
                    onChange={(event) => update({ address: event.target.value })}
                  />
                </label>
              </div>
              <div className="target-connection-menu-section">
                <span>Commands</span>
              {HARDWARE_ACTIONS.map((action) => (
                <button
                  key={action.id}
                  type="button"
                  role="menuitem"
                  disabled={busy}
                  onClick={() =>
                    void runHardwareAction(action.id, "confirm" in action ? action.confirm : undefined)
                  }
                >
                  {action.label}
                </button>
              ))}
              </div>
            </>
          ) : null}
          <div className="target-connection-meta">
            <span>
              Bridge: {bridgeReachable === null ? resolvedBridgeUrl : bridgeReachable ? "online" : "offline"}
            </span>
            <span>Runtime: {connection.runtimeVersion ?? "—"}</span>
            <span>Program: {connection.programHash ?? "—"}</span>
            <span>Deploy: {connection.deployHash ?? "—"}</span>
            <span>Match: {connection.editorMatchesTarget ? "yes" : "no"}</span>
            {connection.lastError ? <span>{connection.lastError}</span> : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}
