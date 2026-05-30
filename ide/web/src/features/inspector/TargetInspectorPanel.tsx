import { Box, FileText } from "lucide-react";
import type { GeneratedCMetadata } from "@/types";
import type { DeployValidationIssue, DeployRemediation } from "@/features/target/targetDeployValidation";
import type { TargetMappingEntry } from "@/features/target/targetMapping";
import { MappingKindBadge } from "@/features/target/MappingKindBadge";
import { buildSymbolCoverage, coverageStatusLabel, type SymbolCoverageRow } from "@/features/target/symbolCoverage";
import type { DeployDiffEntry } from "@/features/target/deployDiff";
import { formatIoValue, type TargetIoValue } from "@/features/target/targetBridgeClient";
import { SafetyPolicyPanel } from "@/features/target/SafetyPolicyPanel";
import type { SafetyPolicy } from "@/features/target/safetyPolicy";

export type TargetInspectorPanelProps = {
  entries: TargetMappingEntry[];
  issues: DeployValidationIssue[];
  metadata: GeneratedCMetadata | null;
  mappingFileName: string;
  buildSourceName: string | null;
  deployPreview: string | null;
  deployDiff: DeployDiffEntry[];
  adapterArtifacts: { name: string; content: string }[];
  safetyPolicy: SafetyPolicy;
  staleMappedSymbols?: Set<string>;
  programHash: string | null;
  editorMatchesTarget: boolean;
  liveIoValues?: TargetIoValue[];
  hardwareConnected?: boolean;
  onOpenMapping: () => void;
  onCreateMapping: () => void;
  onBuildC: () => void;
  onPreviewDeploy: () => void;
  onRevalidateDeploy: () => void;
  onSaveDeployBaseline: () => void;
  onSafetyPolicyChange: (policy: SafetyPolicy) => void;
  onSaveSafetyPolicy: () => void;
};

const REMEDIATION_LABEL: Record<DeployRemediation, string> = {
  "create-mapping": "Create mapping file",
  "open-mapping": "Open mapping editor",
  "build-c": "Build C"
};

export function TargetInspectorPanel({
  entries,
  issues,
  metadata,
  mappingFileName,
  buildSourceName,
  deployPreview,
  deployDiff,
  adapterArtifacts,
  safetyPolicy,
  staleMappedSymbols,
  programHash,
  editorMatchesTarget,
  liveIoValues = [],
  hardwareConnected = false,
  onOpenMapping,
  onCreateMapping,
  onBuildC,
  onPreviewDeploy,
  onRevalidateDeploy,
  onSaveDeployBaseline,
  onSafetyPolicyChange,
  onSaveSafetyPolicy
}: TargetInspectorPanelProps) {
  const actionableIssues = issues.filter((issue) => issue.severity !== "note");
  const notes = issues.filter((issue) => issue.severity === "note");
  const coverage = buildSymbolCoverage(metadata, entries, {
    staleSymbols: staleMappedSymbols ?? new Set<string>()
  });
  const diffChanges = deployDiff.filter((entry) => entry.status !== "unchanged");

  const runRemediation = (remediation: DeployRemediation) => {
    if (remediation === "create-mapping") {
      onCreateMapping();
      return;
    }
    if (remediation === "open-mapping") {
      onOpenMapping();
      return;
    }
    onBuildC();
  };

  return (
    <section aria-label="Target deployment" className="target-inspector inspector-section">
      <h2>Target</h2>

      <div className="target-identity-card">
        <span className="tabular-nums">Program hash: {programHash ?? "—"}</span>
        <span>Editor match: {editorMatchesTarget ? "yes" : "no"}</span>
      </div>

      <SafetyPolicyPanel policy={safetyPolicy} onChange={onSafetyPolicyChange} onSave={onSaveSafetyPolicy} />

      {!metadata ? (
        <div className="target-status-card target-status-pending" role="status">
          <strong>Deployment coverage pending</strong>
          <p>Run Build C, then review symbol coverage and deploy preview before download.</p>
          {buildSourceName ? <p className="target-status-hint">Suggested source: {buildSourceName}</p> : null}
        </div>
      ) : actionableIssues.length === 0 ? (
        <div className="target-status-card target-status-ok" role="status">
          <strong>Deployment checks passed</strong>
          <p>Current mapping aligns with the latest generated C metadata.</p>
        </div>
      ) : null}

      {actionableIssues.length > 0 ? (
        <ul className="deploy-validation-list">
          {actionableIssues.map((issue) => (
            <li key={issue.message} className={issue.severity}>
              <span>{issue.message}</span>
              {issue.remediation ? (
                <button
                  type="button"
                  className="deploy-remediation-btn"
                  onClick={() => runRemediation(issue.remediation!)}
                  disabled={issue.remediation === "build-c" && !buildSourceName}
                >
                  {REMEDIATION_LABEL[issue.remediation]}
                </button>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}

      {notes.length > 0 ? (
        <ul className="deploy-validation-notes">
          {notes.map((issue) => (
            <li key={issue.message}>
              <span>{issue.message}</span>
            </li>
          ))}
        </ul>
      ) : null}

      {coverage.length > 0 ? (
        <div className="target-binding-block">
          <div className="target-binding-header">
            <span>Symbol coverage</span>
            <span className="target-binding-count tabular-nums">{coverage.length}</span>
          </div>
          <ul className="symbol-coverage-list">
            {coverage.map((row) => (
              <CoverageRow key={`${row.symbol}-${row.status}`} row={row} />
            ))}
          </ul>
        </div>
      ) : null}

      <div className="target-binding-block">
        <div className="target-binding-header">
          <span>Bindings</span>
          <span className="target-binding-count tabular-nums">{entries.length}</span>
        </div>
        {entries.length === 0 ? (
          <p className="target-binding-empty">No bindings yet. Open the mapping editor to add transport targets.</p>
        ) : (
          <ul className="target-binding-list">
            {entries.map((entry) => (
              <li key={entry.id} className="target-binding-row">
                <div className="target-binding-row-head">
                  <MappingKindBadge kind={entry.kind} compact />
                  <span className="target-binding-symbol">{entry.symbol}</span>
                </div>
                <span className="target-binding-meta">
                  {entry.kind === "file" && entry.encoding ? entry.encoding : entry.kind}
                </span>
                <span className="target-binding-target" title={entry.target}>
                  {entry.target}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>

      {hardwareConnected ? (
        <div className="target-binding-block">
          <div className="target-binding-header">
            <span>Live I/O</span>
            <span className="target-binding-count tabular-nums">{liveIoValues.length}</span>
          </div>
          {liveIoValues.length === 0 ? (
            <p className="target-binding-empty">Connected. Use Refresh I/O in the target menu to read mapped values.</p>
          ) : (
            <ul className="target-binding-list" aria-label="Live target I/O values">
              {liveIoValues.map((row) => (
                <li key={`${row.symbol}-${row.kind}`} className="target-binding-row">
                  <div className="target-binding-row-head">
                    <span className="target-binding-symbol">{row.symbol}</span>
                    <span className="target-binding-meta">{row.kind}</span>
                  </div>
                  <span className="target-binding-target" title={row.target ?? undefined}>
                    {row.target ?? "—"}
                  </span>
                  <span className="target-live-io-value tabular-nums">{formatIoValue(row.value)}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      ) : null}

      {adapterArtifacts.length > 0 ? (
        <div className="target-binding-block">
          <div className="target-binding-header">
            <span>Adapter artifacts</span>
            <span className="target-binding-count tabular-nums">{adapterArtifacts.length}</span>
          </div>
          <ul className="adapter-artifacts-list">
            {adapterArtifacts.map((artifact) => (
              <li key={artifact.name}>{artifact.name}</li>
            ))}
          </ul>
        </div>
      ) : null}

      <div className="target-inspector-actions">
        <button type="button" className="target-open-mapping-btn" onClick={onOpenMapping}>
          <FileText size={13} aria-hidden="true" />
          Open {mappingFileName.split("/").pop()}
        </button>
        <button
          type="button"
          className="target-build-c-btn"
          onClick={onBuildC}
          disabled={!buildSourceName}
          title={buildSourceName ? `Build C from ${buildSourceName}` : "Add a PLC program file first"}
        >
          <Box size={13} aria-hidden="true" />
          Build C
        </button>
        <button type="button" className="target-open-mapping-btn" onClick={onPreviewDeploy} disabled={!metadata}>
          Preview deploy
        </button>
        <button type="button" className="target-open-mapping-btn" onClick={onRevalidateDeploy} disabled={!metadata}>
          Revalidate
        </button>
        <button type="button" className="target-open-mapping-btn" onClick={onSaveDeployBaseline} disabled={!deployPreview}>
          Save baseline
        </button>
      </div>

      {diffChanges.length > 0 ? (
        <ul className="deploy-diff-list" aria-label="Deploy diff">
          {diffChanges.map((entry) => (
            <li key={`${entry.path}-${entry.status}`} className={`deploy-diff-${entry.status}`}>
              <strong>{entry.status}</strong> {entry.path}: {entry.detail}
            </li>
          ))}
        </ul>
      ) : null}

      {deployPreview ? (
        <pre className="deploy-preview" aria-label="Deploy package preview">
          {deployPreview}
        </pre>
      ) : null}
    </section>
  );
}

function CoverageRow({ row }: { row: SymbolCoverageRow }) {
  return (
    <li className={`symbol-coverage-row status-${row.status}`}>
      <span className="symbol-coverage-name">{row.symbol}</span>
      <span className="symbol-coverage-status">{coverageStatusLabel(row.status)}</span>
      <span className="symbol-coverage-detail">{row.detail}</span>
    </li>
  );
}
