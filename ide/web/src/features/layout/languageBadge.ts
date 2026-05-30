import type { WorkspaceFile } from "@/types";

const LANGUAGE_BADGE: Record<WorkspaceFile["languageId"], { label: string; tone: string }> = {
  st: { label: "ST", tone: "lang-st" },
  il: { label: "IL", tone: "lang-il" },
  ld: { label: "LD", tone: "lang-ld" },
  fbd: { label: "FBD", tone: "lang-fbd" },
  sfc: { label: "SFC", tone: "lang-sfc" },
  xml: { label: "XML", tone: "lang-xml" },
  mapping: { label: "MAP", tone: "lang-map" }
};

export function languageBadge(languageId: WorkspaceFile["languageId"]) {
  return LANGUAGE_BADGE[languageId] ?? { label: languageId.toUpperCase(), tone: "lang-default" };
}
