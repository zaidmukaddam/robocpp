import { buildLocalGraphModel } from "@/features/graph/localGraphModel";
import type { GraphModel } from "@/features/graph/graphTypes";
import type { Diagnostic } from "@/types";

export type GraphValidation = {
  valid: boolean;
  diagnostics: Diagnostic[];
};

export function validateGraphLocal(model: GraphModel): GraphValidation {
  const diagnostics: Diagnostic[] = [];

  for (const pou of model.pous) {
    for (const network of pou.networks) {
      if (network.nodes.length === 0) {
        diagnostics.push(makeGraphDiagnostic(`Network ${network.id} has no nodes.`));
      }
      const coils = network.nodes.filter((node) => node.kind === "coil");
      const contacts = network.nodes.filter((node) => node.kind === "contact");
      if (network.language.toLowerCase().includes("ladder")) {
        const hasNegated = contacts.some((node) => node.attributes.negated === "true");
        if (hasNegated) {
          diagnostics.push(makeGraphDiagnostic(`Ladder network ${network.label ?? network.id} uses edge/negated contacts; verify power flow.`));
        }
        if (coils.length === 0 && contacts.length > 0) {
          diagnostics.push(makeGraphDiagnostic(`Ladder network ${network.label ?? network.id} has contacts but no coil.`));
        }
        if (contacts.length > 1 && coils.length === 0) {
          diagnostics.push(makeGraphDiagnostic(`Ladder branch in ${network.label ?? network.id} has no terminating coil.`));
        }
      }
      if (network.language.toLowerCase().includes("fbd")) {
        const blocks = network.nodes.filter((node) => node.kind === "block");
        for (const block of blocks) {
          const incoming = network.edges.filter((edge) => edge.to === block.stableId);
          if (incoming.length === 0) {
            diagnostics.push(makeGraphDiagnostic(`FBD block ${block.label ?? block.stableId} has no wired inputs.`));
          }
          const formalPins = network.edges.filter((edge) => edge.formalParameter);
          if (formalPins.length === 0 && incoming.length > 1) {
            diagnostics.push(makeGraphDiagnostic(`FBD network ${network.label ?? network.id} uses implicit pin wiring; review formal pins.`));
          }
        }
      }
      if (network.edges.length === 0 && network.nodes.length > 1) {
        diagnostics.push(makeGraphDiagnostic(`Network ${network.label ?? network.id} has disconnected nodes.`));
      }
    }
    if (pou.sfc) {
      if (!pou.sfc.steps.some((step) => step.initial)) {
        diagnostics.push(makeGraphDiagnostic(`SFC program ${pou.name} is missing an initial step.`));
      }
      if (pou.sfc.transitions.length === 0 && pou.sfc.steps.length > 1) {
        diagnostics.push(makeGraphDiagnostic(`SFC program ${pou.name} has multiple steps but no transitions.`));
      }
      const reachable = new Set<string>();
      const initial = pou.sfc.steps.find((step) => step.initial);
      if (initial) {
        reachable.add(initial.name);
      }
      let changed = true;
      while (changed) {
        changed = false;
        for (const transition of pou.sfc.transitions) {
          if (transition.from.some((name) => reachable.has(name))) {
            for (const target of transition.to) {
              if (!reachable.has(target)) {
                reachable.add(target);
                changed = true;
              }
            }
          }
        }
      }
      for (const step of pou.sfc.steps) {
        if (!reachable.has(step.name)) {
          diagnostics.push(makeGraphDiagnostic(`SFC step ${step.name} is unreachable from the initial step.`));
        }
      }
      const divergent = pou.sfc.transitions.filter((transition) => transition.to.length > 1);
      if (divergent.length > 0) {
        diagnostics.push(makeGraphDiagnostic(`SFC program ${pou.name} has divergence transitions; review priorities.`));
      }
    }
  }

  return { valid: diagnostics.length === 0, diagnostics };
}

export function validateGraphLocalFile(file: Parameters<typeof buildLocalGraphModel>[0]): GraphValidation {
  return validateGraphLocal(buildLocalGraphModel(file));
}

function makeGraphDiagnostic(message: string): Diagnostic {
  return {
    severity: "warning",
    code: "graph.validation",
    stableCode: "graph.validation",
    message,
    span: null,
    help: "Fix the graphical structure or underlying textual source."
  };
}
