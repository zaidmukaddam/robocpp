export type TargetBridgeHealth = {
  ok: boolean;
  service: string;
  version: string;
};

export type TargetBridgeSession = {
  ok: boolean;
  state: string;
  mode: string;
  label: string;
  address: string;
  endpoint: string;
  projectId?: string;
  runtimeVersion: string | null;
  programHash: string | null;
  deployHash: string | null;
  bindingCount: number;
  running: boolean;
  lastError: string | null;
  editorMatchesTarget: boolean;
  values?: TargetIoValue[];
};

export type TargetIoValue = {
  symbol: string;
  kind: "file" | "modbus" | string;
  target?: string | null;
  value: boolean | number | string | null;
};

export type ConnectTargetRequest = {
  address: string;
  port?: number;
  projectId: string;
  mappingText: string;
  workspaceRoot?: string;
  programHash?: string | null;
  simulate?: boolean;
};

export type DeployTargetRequest = {
  projectId: string;
  mappingText: string;
  workspaceRoot?: string;
  deployPackage?: string;
  generatedC?: string;
  programHash?: string | null;
};

const DEFAULT_BRIDGE_URL = "http://127.0.0.1:8787";

export function targetBridgeUrl(settingsUrl?: string): string {
  const trimmed = settingsUrl?.trim();
  return trimmed ? trimmed.replace(/\/$/, "") : DEFAULT_BRIDGE_URL;
}

async function bridgeFetch<T>(baseUrl: string, path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${baseUrl}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {})
    }
  });
  const payload = (await response.json()) as T & { error?: string; ok?: boolean };
  if (!response.ok || payload.ok === false) {
    throw new Error(payload.error ?? `Target bridge request failed (${response.status})`);
  }
  return payload;
}

export async function pingTargetBridge(baseUrl: string): Promise<TargetBridgeHealth> {
  return bridgeFetch<TargetBridgeHealth>(baseUrl, "/health");
}

export async function fetchTargetSession(baseUrl: string): Promise<TargetBridgeSession> {
  return bridgeFetch<TargetBridgeSession>(baseUrl, "/api/v1/session");
}

export async function connectTargetBridge(
  baseUrl: string,
  request: ConnectTargetRequest
): Promise<TargetBridgeSession> {
  return bridgeFetch<TargetBridgeSession>(baseUrl, "/api/v1/session", {
    method: "POST",
    body: JSON.stringify({
      address: request.address,
      port: request.port,
      project_id: request.projectId,
      mapping_text: request.mappingText,
      workspace_root: request.workspaceRoot,
      program_hash: request.programHash,
      simulate: request.simulate
    })
  });
}

export async function disconnectTargetBridge(baseUrl: string): Promise<void> {
  await bridgeFetch(baseUrl, "/api/v1/session", { method: "DELETE" });
}

export async function readTargetIo(baseUrl: string): Promise<TargetIoValue[]> {
  const payload = await bridgeFetch<{ values: TargetIoValue[] }>(baseUrl, "/api/v1/io");
  return payload.values;
}

export async function writeTargetIo(
  baseUrl: string,
  symbol: string,
  value: boolean | number | string
): Promise<void> {
  await bridgeFetch(baseUrl, "/api/v1/io", {
    method: "POST",
    body: JSON.stringify({ symbol, value })
  });
}

export async function controlTargetBridge(
  baseUrl: string,
  action: "run" | "stop" | "reset"
): Promise<TargetBridgeSession> {
  return bridgeFetch<TargetBridgeSession>(baseUrl, "/api/v1/session/control", {
    method: "POST",
    body: JSON.stringify({ action })
  });
}

export async function deployTargetBridge(
  baseUrl: string,
  request: DeployTargetRequest
): Promise<{ ok: boolean; deployHash: string; workspaceRoot: string }> {
  return bridgeFetch(baseUrl, "/api/v1/deploy", {
    method: "POST",
    body: JSON.stringify({
      project_id: request.projectId,
      mapping_text: request.mappingText,
      workspace_root: request.workspaceRoot,
      deploy_package: request.deployPackage,
      generated_c: request.generatedC,
      program_hash: request.programHash
    })
  });
}

export function indexTargetIo(values: TargetIoValue[]): Map<string, TargetIoValue> {
  return new Map(values.map((entry) => [entry.symbol.toUpperCase(), entry]));
}

export function coerceWriteValue(
  raw: string,
  current: TargetIoValue["value"] | undefined
): boolean | number | string {
  const trimmed = raw.trim();
  if (trimmed === "TRUE" || trimmed === "true" || trimmed === "1") {
    return true;
  }
  if (trimmed === "FALSE" || trimmed === "false" || trimmed === "0") {
    return false;
  }
  if (typeof current === "boolean") {
    return trimmed.toLowerCase() === "true" || trimmed === "1";
  }
  if (typeof current === "number" || /^\d+$/.test(trimmed)) {
    const parsed = Number(trimmed);
    if (!Number.isNaN(parsed)) {
      return parsed;
    }
  }
  return trimmed;
}

export function formatIoValue(value: TargetIoValue["value"]): string {
  if (typeof value === "boolean") {
    return value ? "TRUE" : "FALSE";
  }
  if (value === null || value === undefined) {
    return "—";
  }
  return String(value);
}

export function bridgeErrorMessage(error: unknown): string {
  if (error instanceof TypeError) {
    return "Target bridge is not running. Start it with: cargo run -p rbcpp_target_bridge";
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Target bridge request failed.";
}
