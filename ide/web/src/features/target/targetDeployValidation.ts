import type { GeneratedCMetadata, Project } from "@/types";
import { isTargetMappingFile, parseTargetMapping } from "@/features/target/targetMapping";
import { parseSafetyPolicyFromMapping, validateSafetyPolicy } from "@/features/target/safetyPolicy";

export type DeployRemediation = "create-mapping" | "open-mapping" | "build-c";

export type DeployValidationIssue = {
  severity: "error" | "warning" | "note";
  message: string;
  remediation?: DeployRemediation;
};

export function validateTargetDeployment(
  project: Project,
  metadata: GeneratedCMetadata | null
): DeployValidationIssue[] {
  const issues: DeployValidationIssue[] = [];
  const mappingFile = project.files.find((file) => isTargetMappingFile(file.name));
  const mapping = parseTargetMapping(mappingFile?.text ?? "");
  const ioNames = new Set(metadata?.ioSymbols.map((symbol) => symbol.name.toLowerCase()) ?? []);
  const stateNames = new Set(metadata?.stateLayout.map((field) => field.name.toLowerCase()) ?? []);
  const retained = new Set(metadata?.retainedFields.map((field) => field.toLowerCase()) ?? []);

  if (!mappingFile) {
    issues.push({
      severity: "error",
      message: "Missing target/mapping.toml in the project.",
      remediation: "create-mapping"
    });
  }

  if (mapping.entries.length === 0) {
    issues.push({
      severity: "warning",
      message: "Target mapping file has no bindings.",
      remediation: "open-mapping"
    });
  }

  for (const entry of mapping.entries) {
    const symbol = entry.symbol.toLowerCase();
    if (metadata && ioNames.size > 0 && !ioNames.has(symbol) && !stateNames.has(symbol)) {
      issues.push({
        severity: "warning",
        message: `Mapping symbol ${entry.symbol} is not present in generated I/O or state layout.`,
        remediation: "open-mapping"
      });
    }
    if (entry.kind === "file" && entry.encoding === "bool" && retained.has(symbol)) {
      issues.push({
        severity: "warning",
        message: `Retained field ${entry.symbol} uses bool file encoding; verify retained-state shape on deploy.`,
        remediation: "open-mapping"
      });
    }
    if (entry.target.includes("..")) {
      issues.push({
        severity: "error",
        message: `Mapping target for ${entry.symbol} escapes the project root.`,
        remediation: "open-mapping"
      });
    }
  }

  if (metadata && metadata.targetHooks.length === 0) {
    issues.push({
      severity: "warning",
      message: "Generated C metadata exposes no target hooks yet. Deployment adapters may be incomplete.",
      remediation: "build-c"
    });
  }

  if (!metadata) {
    issues.push({
      severity: "note",
      message: "Build C on a PLC program, then revisit this panel to verify symbol coverage.",
      remediation: "build-c"
    });
  }

  if (mappingFile) {
    const safetyPolicy = parseSafetyPolicyFromMapping(mappingFile.text);
    issues.push(...validateSafetyPolicy(safetyPolicy));
  }

  return issues;
}
