import { buildLocalGraphModel } from "@/features/graph/localGraphModel";
import type { GraphModel } from "@/features/graph/graphTypes";
import type { WorkspaceFile } from "@/types";
import type { GraphValidation } from "@/features/graph/validateGraph";
import { validateGraphLocal } from "@/features/graph/validateGraph";

export type GraphSnapshot = {
  model: GraphModel;
  validation: GraphValidation;
};

const LOCAL_CACHE_LIMIT = 48;
const ENGINE_CACHE_LIMIT = 32;

const localCache = new Map<string, GraphSnapshot>();
const engineCache = new Map<string, GraphSnapshot>();

function cacheKey(file: WorkspaceFile): string {
  return `${file.name}\0${file.languageId}\0${file.text}`;
}

function trimCache<T>(cache: Map<string, T>, limit: number) {
  while (cache.size > limit) {
    const oldest = cache.keys().next().value;
    if (!oldest) {
      break;
    }
    cache.delete(oldest);
  }
}

export function isGraphicalLanguage(languageId: string): boolean {
  return languageId === "ld" || languageId === "fbd" || languageId === "sfc" || languageId === "xml";
}

export function graphSnapshotLocal(file: WorkspaceFile): GraphSnapshot {
  const key = cacheKey(file);
  const cached = localCache.get(key);
  if (cached) {
    return cached;
  }
  const model = buildLocalGraphModel(file);
  const snapshot = { model, validation: validateGraphLocal(model) };
  localCache.set(key, snapshot);
  trimCache(localCache, LOCAL_CACHE_LIMIT);
  return snapshot;
}

export function readEngineGraphCache(file: WorkspaceFile): GraphSnapshot | null {
  return engineCache.get(cacheKey(file)) ?? null;
}

export function writeEngineGraphCache(file: WorkspaceFile, snapshot: GraphSnapshot) {
  engineCache.set(cacheKey(file), snapshot);
  trimCache(engineCache, ENGINE_CACHE_LIMIT);
}

export function clearEngineGraphCache() {
  engineCache.clear();
}
