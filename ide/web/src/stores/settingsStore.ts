import type { CompilerProfile, IdeSettings } from "@/types";

const STORAGE_KEY = "robocpp-studio-settings-v1";

export const DEFAULT_SETTINGS: IdeSettings = {
  compilerProfile: "2003-strict",
  cycleTimeMs: 4,
  selectedProgram: "",
  selectedConfiguration: "",
  generatedCOutputPath: "build/generated.c",
  targetMappingPath: "target/mapping.toml",
  simulationCycles: 6,
  watchVariables: "",
  targetBridgeUrl: "http://127.0.0.1:8787",
  targetWorkspaceRoot: "",
  targetModbusPort: 502
};

export function loadSettings(): IdeSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return { ...DEFAULT_SETTINGS };
    }
    const parsed = JSON.parse(raw) as Partial<IdeSettings>;
    return {
      ...DEFAULT_SETTINGS,
      ...parsed,
      compilerProfile: isCompilerProfile(parsed.compilerProfile)
        ? parsed.compilerProfile
        : DEFAULT_SETTINGS.compilerProfile,
      targetModbusPort:
        typeof parsed.targetModbusPort === "number" && parsed.targetModbusPort > 0
          ? parsed.targetModbusPort
          : DEFAULT_SETTINGS.targetModbusPort
    };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(settings: IdeSettings): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}

export function parseWatchList(input: string): string[] {
  return input
    .split(/[,\n]/)
    .map((entry) => entry.trim())
    .filter(Boolean);
}

function isCompilerProfile(value: unknown): value is CompilerProfile {
  return value === "2003-strict" || value === "2003-extended";
}
