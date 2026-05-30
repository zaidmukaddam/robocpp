export type ForcedValue = {
  name: string;
  preparedValue: string;
  persistent: boolean;
};

const STORAGE_KEY = "robocpp-studio-forced-values-v1";

type ForcedStore = Record<string, ForcedValue[]>;

function readStore(): ForcedStore {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return {};
    }
    const parsed = JSON.parse(raw) as ForcedStore;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function writeStore(store: ForcedStore): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(store));
}

export function readForcedValues(projectId: string): ForcedValue[] {
  return readStore()[projectId] ?? [];
}

export function persistForcedValues(projectId: string, values: ForcedValue[]): void {
  const store = readStore();
  store[projectId] = values;
  writeStore(store);
}

export function upsertForcedValue(projectId: string, value: ForcedValue): ForcedValue[] {
  const current = readForcedValues(projectId).filter((entry) => entry.name !== value.name);
  const next = [...current, value];
  persistForcedValues(projectId, next);
  return next;
}

export function removeForcedValue(projectId: string, name: string): ForcedValue[] {
  const next = readForcedValues(projectId).filter((entry) => entry.name !== name);
  persistForcedValues(projectId, next);
  return next;
}
