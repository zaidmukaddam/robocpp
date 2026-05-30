import type { GraphSelection, GraphModel } from "@/features/graph/graphTypes";
import type { WorkspaceFile } from "@/types";
import { moveSfcStep, reorderGraphNodes } from "@/features/graph/graphReorder";
import {
  connectPlcopenWire,
  deletePlcopenNode,
  renamePlcopenExpression
} from "@/features/graph/plcopenGraphEdits";
import { setGraphElementComment, setLdCoilMode, type LdCoilMode } from "@/features/graph/graphPropertyEdits";

export function networkIndexFromSelection(
  model: GraphModel,
  selection: GraphSelection | null | undefined
): number | null {
  if (!selection || selection.kind !== "node" || !selection.networkId) {
    return null;
  }
  const pou = model.pous[0];
  if (!pou) {
    return null;
  }
  const index = pou.networks.findIndex((entry) => entry.id === selection.networkId);
  return index >= 0 ? index : null;
}

function mapRungs(text: string, mapper: (body: string, index: number) => string): string {
  let index = 0;
  return text.replace(/RUNG\s*([\s\S]*?)\s*END_RUNG/gi, (full, body: string) => {
    const nextBody = mapper(body, index);
    index += 1;
    return full.replace(body, nextBody);
  });
}

function mapNetworks(text: string, mapper: (body: string, index: number) => string): string {
  let index = 0;
  return text.replace(/NETWORK\s*([\s\S]*?)\s*END_NETWORK/gi, (full, body: string) => {
    const nextBody = mapper(body, index);
    index += 1;
    return full.replace(body, nextBody);
  });
}

function splitFbdArgs(args: string): string[] {
  return args
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
}

export type GraphEditAction =
  | "add-rung"
  | "add-network"
  | "add-step"
  | "add-transition"
  | "add-contact"
  | "add-negated-contact"
  | "add-coil"
  | "add-set-coil"
  | "add-reset-coil"
  | "add-branch"
  | "add-fbd-literal"
  | "toggle-sfc-initial"
  | "rename"
  | "delete-selected"
  | "duplicate-selected"
  | "connect"
  | "reorder"
  | "move-step"
  | "move-rung"
  | "toggle-edge-contact"
  | "add-sfc-jump"
  | "set-property";

export function toggleLdEdgeContact(text: string, label: string): string {
  if (!label) {
    return text;
  }
  const negated = new RegExp(`CONTACT\\s+NOT\\s+${escapeRegExp(label)};`, "i");
  if (negated.test(text)) {
    return text.replace(negated, `CONTACT ${label};`);
  }
  const normal = new RegExp(`CONTACT\\s+${escapeRegExp(label)};`, "i");
  return text.replace(normal, `CONTACT NOT ${label};`);
}

export function moveLdRung(text: string, direction: "up" | "down", rungIndex: number): string | null {
  const blocks = [...text.matchAll(/RUNG\s*([\s\S]*?)\s*END_RUNG/gi)];
  if (rungIndex < 0 || rungIndex >= blocks.length) {
    return null;
  }
  const swapIndex = direction === "up" ? rungIndex - 1 : rungIndex + 1;
  if (swapIndex < 0 || swapIndex >= blocks.length) {
    return null;
  }
  const serialized = blocks.map((match) => match[0]!);
  const next = [...serialized];
  [next[rungIndex], next[swapIndex]] = [next[swapIndex]!, next[rungIndex]!];
  let cursor = 0;
  return text.replace(/RUNG\s*([\s\S]*?)\s*END_RUNG/gi, () => next[cursor++] ?? "");
}

export function appendSfcJump(text: string, targetStep: string): string {
  const jump = `(* SFC jump to ${targetStep} *)\nJUMP ${targetStep};\n`;
  if (text.includes("END_PROGRAM")) {
    return text.replace("END_PROGRAM", `${jump}\nEND_PROGRAM`);
  }
  return `${text}\n${jump}`;
}

export function appendLdRung(text: string): string {
  const marker = "END_LADDER";
  const rung = `RUNG
    CONTACT NewInput;
    COIL NewOutput;
END_RUNG
`;
  if (text.includes(marker)) {
    return text.replace(marker, `${rung}${marker}`);
  }
  return `${text}\nLADDER\n${rung}END_LADDER\n`;
}

function insertLdContactInRung(body: string, afterLabel: string | null | undefined, negated: boolean): string {
  const snippet = negated ? "    CONTACT NOT NewContact;\n" : "    CONTACT NewContact;\n";
  if (afterLabel) {
    const pattern = new RegExp(`(CONTACT\\s+(?:NOT\\s+)?${escapeRegExp(afterLabel)};\\s*\\n)`, "i");
    if (pattern.test(body)) {
      return body.replace(pattern, `$1${snippet}`);
    }
  }
  return `${snippet}${body}`;
}

export function appendLdContact(
  text: string,
  afterLabel?: string | null,
  negated = false,
  rungIndex?: number | null
): string {
  if (rungIndex !== undefined && rungIndex !== null && rungIndex >= 0) {
    return mapRungs(text, (body, index) =>
      index === rungIndex ? insertLdContactInRung(body, afterLabel, negated) : body
    );
  }
  const snippet = negated ? "    CONTACT NOT NewContact;\n" : "    CONTACT NewContact;\n";
  if (afterLabel) {
    const pattern = new RegExp(`(CONTACT\\s+(?:NOT\\s+)?${escapeRegExp(afterLabel)};\\s*\\n)`, "i");
    if (pattern.test(text)) {
      return text.replace(pattern, `$1${snippet}`);
    }
  }
  return text.replace(/(RUNG\s*\n)/i, `$1${snippet}`);
}

function insertLdCoilInRung(body: string, kind: "coil" | "set" | "reset"): string {
  const line =
    kind === "set" ? "    SET NewCoil;" : kind === "reset" ? "    RESET NewCoil;" : "    COIL NewCoil;";
  if (/\b(?:COIL|SET|RESET)\s+/i.test(body)) {
    return body.replace(/((?:COIL|SET|RESET)\s+[^;]+;)/i, `$1\n${line}`);
  }
  return `${body}${line}\n`;
}

export function appendLdCoil(
  text: string,
  kind: "coil" | "set" | "reset" = "coil",
  rungIndex?: number | null
): string {
  if (rungIndex !== undefined && rungIndex !== null && rungIndex >= 0) {
    return mapRungs(text, (body, index) => (index === rungIndex ? insertLdCoilInRung(body, kind) : body));
  }
  const line =
    kind === "set" ? "    SET NewCoil;" : kind === "reset" ? "    RESET NewCoil;" : "    COIL NewCoil;";
  if (/\b(?:COIL|SET|RESET)\s+/i.test(text)) {
    return text.replace(/((?:COIL|SET|RESET)\s+[^;]+;)/i, `$1\n${line}`);
  }
  return text.replace(/(RUNG[\s\S]*?)(\s*END_RUNG)/i, `$1\n${line}$2`);
}

export function appendLdBranch(text: string): string {
  const branch = `RUNG
    CONTACT BranchA;
    CONTACT BranchB;
    COIL BranchOut;
END_RUNG
`;
  return appendLdRung(text.replace(/END_LADDER/, `${branch}END_LADDER`));
}

export function appendFbdNetwork(text: string): string {
  const marker = "END_FBD";
  const network = `NETWORK
    OUT NewOutput := AND(NewInputA, NewInputB);
END_NETWORK
`;
  if (text.includes(marker)) {
    return text.replace(marker, `${network}${marker}`);
  }
  return `${text}\nFBD\n${network}END_FBD\n`;
}

export function appendFbdLiteral(text: string, networkIndex?: number | null): string {
  const editBody = (body: string) => {
    const pattern = /(OUT\s+\w+\s*:=\s*\w+\()([^)]*)(\))/i;
    if (!pattern.test(body)) {
      return body;
    }
    return body.replace(pattern, (_match, start, inputs, end) => {
      const nextInputs = inputs.trim() ? `${inputs.trim()}, TRUE` : "TRUE";
      return `${start}${nextInputs}${end}`;
    });
  };
  if (networkIndex !== undefined && networkIndex !== null && networkIndex >= 0) {
    return mapNetworks(text, (body, index) => (index === networkIndex ? editBody(body) : body));
  }
  if (!/(OUT\s+\w+\s*:=\s*\w+\()([^)]*)(\))/i.test(text)) {
    return appendFbdNetwork(text);
  }
  return editBody(text);
}

const FBD_BINARY_FUNCS = new Set(["AND", "OR", "XOR", "ADD", "MUL", "MIN", "MAX"]);

function connectFbdWireInBody(body: string, sourceLabel: string, targetLabel: string): string {
  const assignPattern = new RegExp(`(OUT\\s+${escapeRegExp(targetLabel)}\\s*:=\\s*)([^;]+);`, "i");
  const assignMatch = body.match(assignPattern);
  if (!assignMatch) {
    return body;
  }
  const prefix = assignMatch[1] ?? "";
  const rhs = (assignMatch[2] ?? "").trim();
  if (rhs === sourceLabel || new RegExp(`\\b${escapeRegExp(sourceLabel)}\\b`).test(rhs)) {
    return body;
  }

  const funcMatch = rhs.match(/^(\w+)\(([^)]*)\)$/);
  if (funcMatch && FBD_BINARY_FUNCS.has(funcMatch[1]!.toUpperCase())) {
    const func = funcMatch[1]!;
    const args = splitFbdArgs(funcMatch[2] ?? "");
    const nextArgs = args.length > 0 ? `${args.join(", ")}, ${sourceLabel}` : sourceLabel;
    return body.replace(assignPattern, `${prefix}${func}(${nextArgs});`);
  }

  return body.replace(assignPattern, `${prefix}AND(${rhs}, ${sourceLabel});`);
}

export function connectFbdWire(
  text: string,
  sourceLabel: string,
  targetLabel: string,
  networkIndex?: number | null
): string {
  if (!sourceLabel || !targetLabel || sourceLabel === targetLabel) {
    return text;
  }
  if (networkIndex !== undefined && networkIndex !== null && networkIndex >= 0) {
    return mapNetworks(text, (body, index) =>
      index === networkIndex ? connectFbdWireInBody(body, sourceLabel, targetLabel) : body
    );
  }
  let updated = false;
  const next = mapNetworks(text, (body) => {
    const edited = connectFbdWireInBody(body, sourceLabel, targetLabel);
    if (edited !== body) {
      updated = true;
    }
    return edited;
  });
  return updated ? next : text;
}

function deleteFbdLabelInBody(body: string, label: string): string {
  let next = body.replace(new RegExp(`\\s*OUT\\s+${escapeRegExp(label)}\\s*:=\\s*[^;]+;\\s*\\n?`, "gi"), "\n");
  next = next.replace(/OUT\s+(\w+)\s*:=\s*(\w+)\(([^)]*)\)/gi, (_match, out, func, args) => {
    const filtered = splitFbdArgs(args).filter((arg) => arg !== label);
    if (filtered.length === 0) {
      return `OUT ${out} := ${func}();`;
    }
    return `OUT ${out} := ${func}(${filtered.join(", ")});`;
  });
  next = next.replace(
    new RegExp(`(OUT\\s+\\w+\\s*:=\\s*)AND\\(([^)]*)\\)\\s*;`, "gi"),
    (_match, prefix, args) => {
      const filtered = splitFbdArgs(args).filter((arg) => arg !== label);
      if (filtered.length === 0) {
        return "";
      }
      if (filtered.length === 1) {
        return `${prefix}${filtered[0]};`;
      }
      return `${prefix}AND(${filtered.join(", ")});`;
    }
  );
  return next;
}

export function deleteFbdLabel(text: string, label: string, networkIndex?: number | null): string {
  if (!label) {
    return text;
  }
  if (networkIndex !== undefined && networkIndex !== null && networkIndex >= 0) {
    return mapNetworks(text, (body, index) => (index === networkIndex ? deleteFbdLabelInBody(body, label) : body));
  }
  return mapNetworks(text, (body) => deleteFbdLabelInBody(body, label));
}

export function toggleSfcInitialStep(text: string, stepName: string): string {
  if (!stepName) {
    return text;
  }
  let next = text.replace(/^\s*INITIAL_STEP\s+/gm, "STEP ");
  next = next.replace(new RegExp(`^\\s*STEP\\s+${escapeRegExp(stepName)}\\s*;`, "m"), `INITIAL_STEP ${stepName};`);
  return next;
}

export function appendSfcStep(text: string, stepName = "NewStep"): string {
  const action = `ACTION ${stepName}:
    (* TODO *)
END_ACTION;
`;
  if (text.includes("END_PROGRAM")) {
    return text.replace("END_PROGRAM", `STEP ${stepName};\n${action}\nEND_PROGRAM`);
  }
  return `${text}\nSTEP ${stepName};\n${action}\n`;
}

function parseSfcStepNames(text: string): string[] {
  const steps: string[] = [];
  const initial = text.match(/INITIAL_STEP\s+(\w+)\s*;/);
  if (initial?.[1]) {
    steps.push(initial[1]);
  }
  for (const match of text.matchAll(/^\s*STEP\s+(\w+)\s*;/gm)) {
    steps.push(match[1]!);
  }
  return steps;
}

export function appendSfcTransition(text: string, fromStep?: string | null): string {
  const steps = parseSfcStepNames(text);
  const from =
    fromStep && steps.includes(fromStep) ? fromStep : steps[0] ?? "Start";
  const fromIndex = steps.indexOf(from);
  const to =
    fromIndex >= 0 && fromIndex + 1 < steps.length
      ? steps[fromIndex + 1]!
      : steps.find((step) => step !== from) ?? "Run";
  const transitionName = `${from}To${to}`;
  const transition = `TRANSITION ${transitionName} FROM ${from} TO ${to} := TRUE;\nEND_TRANSITION;\n`;
  if (text.includes("END_PROGRAM")) {
    return text.replace("END_PROGRAM", `${transition}\nEND_PROGRAM`);
  }
  return `${text}\n${transition}`;
}

export function renameGraphLabel(text: string, currentLabel: string, nextLabel: string): string {
  if (!currentLabel || currentLabel === nextLabel) {
    return text;
  }
  return text.replaceAll(currentLabel, nextLabel);
}

export function deleteLdLabel(text: string, label: string, rungIndex?: number | null): string {
  if (!label) {
    return text;
  }
  const removeFromRung = (body: string) =>
    body
      .replace(new RegExp(`\\s*CONTACT\\s+(?:NOT\\s+)?${escapeRegExp(label)};\\s*\\n`, "gi"), "\n")
      .replace(new RegExp(`\\s*COIL\\s+(?:SET|RESET\\s+)?${escapeRegExp(label)};\\s*\\n`, "gi"), "\n");
  if (rungIndex !== undefined && rungIndex !== null && rungIndex >= 0) {
    return mapRungs(text, (body, index) => (index === rungIndex ? removeFromRung(body) : body));
  }
  return removeFromRung(text);
}

export function deleteSfcTransition(text: string, transitionName: string): string {
  if (!transitionName) {
    return text;
  }
  const named = new RegExp(
    `TRANSITION\\s+${escapeRegExp(transitionName)}\\s+[\\s\\S]*?END_TRANSITION;\\s*\\n?`,
    "gi"
  );
  if (named.test(text)) {
    return text.replace(named, "");
  }
  const generic = /TRANSITION\s+[\s\S]*?END_TRANSITION;\s*\n?/gi;
  for (const match of text.matchAll(generic)) {
    if (match[0]?.includes(transitionName)) {
      return text.replace(match[0], "");
    }
  }
  return text;
}

export function deleteGraphLabel(
  text: string,
  label: string,
  languageId: WorkspaceFile["languageId"],
  networkIndex?: number | null,
  selectionKind?: GraphSelection["kind"]
): string {
  if (!label) {
    return text;
  }
  if (languageId === "ld") {
    return deleteLdLabel(text, label, networkIndex);
  }
  if (languageId === "fbd" || languageId === "xml") {
    return deleteFbdLabel(text, label, networkIndex);
  }
  if (languageId === "sfc") {
    if (selectionKind === "transition") {
      return deleteSfcTransition(text, label);
    }
    return text
      .replace(new RegExp(`^\\s*(?:INITIAL_STEP|STEP)\\s+${escapeRegExp(label)};\\s*$\n?`, "gim"), "")
      .replace(new RegExp(`ACTION\\s+${escapeRegExp(label)}:[\\s\\S]*?END_ACTION;\\s*`, "gi"), "");
  }
  return text;
}

export function duplicateGraphLabel(text: string, label: string): string {
  if (!label) {
    return text;
  }
  const lineMatch = text.match(new RegExp(`^.*\\b${escapeRegExp(label)}\\b.*$`, "m"));
  if (!lineMatch) {
    return text;
  }
  const line = lineMatch[0];
  const copyLine = line.replace(new RegExp(`\\b${escapeRegExp(label)}\\b`, "g"), `${label}_Copy`);
  return text.replace(line, `${line}\n${copyLine}`);
}

export function selectionLabel(
  _file: WorkspaceFile,
  model: GraphModel,
  selection: GraphSelection | null
): string | null {
  if (!selection) {
    return null;
  }
  const pou = model.pous[0];
  if (!pou) {
    return null;
  }
  if (selection.kind === "node") {
    const network = pou.networks.find((entry) => entry.id === selection.networkId) ?? pou.networks[0];
    return network?.nodes.find((node) => node.stableId === selection.stableId)?.label ?? null;
  }
  if (selection.kind === "step") {
    return pou.sfc?.steps.find((step) => step.stableId === selection.stableId)?.name ?? null;
  }
  if (selection.kind === "transition") {
    return pou.sfc?.transitions.find((transition) => transition.stableId === selection.stableId)?.name ?? selection.stableId;
  }
  if (selection.kind === "action") {
    return pou.sfc?.actions.find((action) => action.stableId === selection.stableId)?.name ?? null;
  }
  return null;
}

export function applyGraphEdit(
  file: WorkspaceFile,
  action: GraphEditAction,
  payload?: string,
  selection?: GraphSelection | null,
  model?: GraphModel | null
): string {
  const networkIndex = model ? networkIndexFromSelection(model, selection) : null;

  if (action === "add-rung" && file.languageId === "ld") {
    return appendLdRung(file.text);
  }
  if (action === "add-network" && (file.languageId === "fbd" || file.languageId === "xml")) {
    return appendFbdNetwork(file.text);
  }
  if (action === "add-step" && file.languageId === "sfc") {
    return appendSfcStep(file.text, payload ?? "NewStep");
  }
  if (action === "add-transition" && file.languageId === "sfc") {
    return appendSfcTransition(file.text, payload ?? null);
  }
  if (action === "add-contact" && file.languageId === "ld") {
    return appendLdContact(file.text, payload ?? null, false, networkIndex);
  }
  if (action === "add-negated-contact" && file.languageId === "ld") {
    return appendLdContact(file.text, payload ?? null, true, networkIndex);
  }
  if (action === "add-coil" && file.languageId === "ld") {
    return appendLdCoil(file.text, "coil", networkIndex);
  }
  if (action === "add-set-coil" && file.languageId === "ld") {
    return appendLdCoil(file.text, "set", networkIndex);
  }
  if (action === "add-reset-coil" && file.languageId === "ld") {
    return appendLdCoil(file.text, "reset", networkIndex);
  }
  if (action === "add-branch" && file.languageId === "ld") {
    return appendLdBranch(file.text);
  }
  if (action === "add-fbd-literal" && (file.languageId === "fbd" || file.languageId === "xml")) {
    return appendFbdLiteral(file.text, networkIndex);
  }
  if (action === "connect" && (file.languageId === "fbd" || file.languageId === "xml") && payload) {
    const [sourceLabel, targetLabel] = payload.split("->");
    if (file.languageId === "xml" && sourceLabel && targetLabel) {
      return connectPlcopenWire(file.text, sourceLabel, targetLabel);
    }
    if (sourceLabel && targetLabel) {
      return connectFbdWire(file.text, sourceLabel, targetLabel, networkIndex);
    }
  }
  if (action === "toggle-sfc-initial" && file.languageId === "sfc" && payload) {
    return toggleSfcInitialStep(file.text, payload);
  }
  if (action === "rename" && payload) {
    const [currentLabel, nextLabel] = payload.split("->");
    if (currentLabel && nextLabel) {
      if (file.languageId === "xml") {
        return renamePlcopenExpression(file.text, currentLabel, nextLabel);
      }
      return renameGraphLabel(file.text, currentLabel, nextLabel);
    }
  }
  if (action === "delete-selected" && payload) {
    if (file.languageId === "xml" && selection?.kind === "node") {
      return deletePlcopenNode(file.text, selection.stableId);
    }
    return deleteGraphLabel(file.text, payload, file.languageId, networkIndex, selection?.kind);
  }
  if (action === "duplicate-selected" && payload) {
    return duplicateGraphLabel(file.text, payload);
  }
  if (action === "reorder" && payload) {
    const [fromLabel, toLabel] = payload.split("->");
    if (fromLabel && toLabel) {
      return reorderGraphNodes(file, fromLabel, toLabel) ?? file.text;
    }
  }
  if (action === "move-step" && file.languageId === "sfc" && payload) {
    const [stepName, direction] = payload.split("->");
    if (stepName && (direction === "up" || direction === "down")) {
      return moveSfcStep(file.text, stepName, direction) ?? file.text;
    }
  }
  if (action === "toggle-edge-contact" && file.languageId === "ld" && payload) {
    return toggleLdEdgeContact(file.text, payload);
  }
  if (action === "move-rung" && file.languageId === "ld" && payload) {
    const [indexText, direction] = payload.split("->");
    const rungIndex = Number(indexText);
    if (!Number.isNaN(rungIndex) && (direction === "up" || direction === "down")) {
      return moveLdRung(file.text, direction, rungIndex) ?? file.text;
    }
  }
  if (action === "add-sfc-jump" && file.languageId === "sfc" && payload) {
    return appendSfcJump(file.text, payload);
  }
  if (action === "set-property" && payload) {
    const [label, property, ...valueParts] = payload.split(":");
    const value = valueParts.join(":");
    if (!label || !property) {
      return file.text;
    }
    if (property === "comment") {
      return setGraphElementComment(file, label, value);
    }
    if (property === "coil-mode" && file.languageId === "ld") {
      const mode = value as LdCoilMode;
      if (mode === "normal" || mode === "set" || mode === "reset") {
        return setLdCoilMode(file.text, label, mode);
      }
    }
  }
  return file.text;
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function roundTripHint(languageId: WorkspaceFile["languageId"]): string {
  switch (languageId) {
    case "ld":
    case "fbd":
    case "sfc":
      return "Native textual source is the source of truth. Graphical edits append or rename constructs in the text buffer.";
    case "xml":
      return "PLCopen XML preserves node localId and connector metadata. Graphical edits append networks in text; re-export via PLCopen to keep vendor layout.";
    default:
      return "This language is text-only in the current IDE shell.";
  }
}
