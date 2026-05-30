import type { ProjectArtifact } from "@/types";

export type CompareLine = {
  kind: "same" | "add" | "remove" | "change";
  left?: string;
  right?: string;
  line: number;
};

export function diffLines(left: string, right: string): CompareLine[] {
  const leftLines = left.split("\n");
  const rightLines = right.split("\n");
  const max = Math.max(leftLines.length, rightLines.length);
  const rows: CompareLine[] = [];

  for (let index = 0; index < max; index += 1) {
    const l = leftLines[index];
    const r = rightLines[index];
    if (l === undefined && r !== undefined) {
      rows.push({ kind: "add", right: r, line: index + 1 });
    } else if (l !== undefined && r === undefined) {
      rows.push({ kind: "remove", left: l, line: index + 1 });
    } else if (l !== r) {
      rows.push({ kind: "change", left: l, right: r, line: index + 1 });
    } else {
      rows.push({ kind: "same", left: l, right: r, line: index + 1 });
    }
  }

  return rows;
}

export function compareSummary(left: ProjectArtifact, right: ProjectArtifact): string {
  const changes = diffLines(left.content, right.content).filter((row) => row.kind !== "same");
  if (changes.length === 0) {
    return "Artifacts are identical.";
  }
  const added = changes.filter((row) => row.kind === "add").length;
  const removed = changes.filter((row) => row.kind === "remove").length;
  const changed = changes.filter((row) => row.kind === "change").length;
  return `${changes.length} line(s): +${added} -${removed} ~${changed}`;
}

export function findComparableArtifacts(
  artifacts: ProjectArtifact[],
  artifact: ProjectArtifact
): ProjectArtifact[] {
  return artifacts.filter(
    (entry) => entry.id !== artifact.id && entry.kind === artifact.kind && entry.sourceFile === artifact.sourceFile
  );
}
