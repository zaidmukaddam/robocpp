const OPT_IN_KEY = "robocpp-studio-telemetry-opt-in";
const REPORTS_KEY = "robocpp-studio-telemetry-reports-v1";
const MAX_REPORTS = 50;

export type TelemetryReport = {
  id: string;
  timestamp: string;
  message: string;
  stack: string | null;
  context: Record<string, string>;
};

function readReports(): TelemetryReport[] {
  try {
    const raw = localStorage.getItem(REPORTS_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw) as TelemetryReport[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function writeReports(reports: TelemetryReport[]): void {
  localStorage.setItem(REPORTS_KEY, JSON.stringify(reports.slice(0, MAX_REPORTS)));
}

export function isTelemetryEnabled(): boolean {
  return localStorage.getItem(OPT_IN_KEY) === "1";
}

export function setTelemetryEnabled(enabled: boolean): void {
  localStorage.setItem(OPT_IN_KEY, enabled ? "1" : "0");
}

export function listTelemetryReports(): TelemetryReport[] {
  return readReports();
}

export function clearTelemetryReports(): void {
  localStorage.removeItem(REPORTS_KEY);
}

export function reportError(error: unknown, context: Record<string, string> = {}): TelemetryReport | null {
  const message = error instanceof Error ? error.message : String(error);
  const stack = error instanceof Error ? error.stack ?? null : null;
  const report: TelemetryReport = {
    id: crypto.randomUUID(),
    timestamp: new Date().toISOString(),
    message,
    stack,
    context
  };

  const reports = readReports();
  reports.unshift(report);
  writeReports(reports);

  if (isTelemetryEnabled()) {
    console.info("[RoboC++ telemetry]", report);
  }

  return report;
}

export function installGlobalErrorHandlers(): () => void {
  const onError = (event: ErrorEvent) => {
    reportError(event.error ?? event.message, { source: "window.onerror" });
  };
  const onRejection = (event: PromiseRejectionEvent) => {
    reportError(event.reason, { source: "unhandledrejection" });
  };
  window.addEventListener("error", onError);
  window.addEventListener("unhandledrejection", onRejection);
  return () => {
    window.removeEventListener("error", onError);
    window.removeEventListener("unhandledrejection", onRejection);
  };
}
