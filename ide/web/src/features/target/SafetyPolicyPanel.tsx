import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { SafetyPolicy } from "@/features/target/safetyPolicy";

type SafetyPolicyPanelProps = {
  policy: SafetyPolicy;
  onChange: (policy: SafetyPolicy) => void;
  onSave: () => void;
};

export function SafetyPolicyPanel({ policy, onChange, onSave }: SafetyPolicyPanelProps) {
  const update = <K extends keyof SafetyPolicy>(key: K, value: SafetyPolicy[K]) => {
    onChange({ ...policy, [key]: value });
  };

  return (
    <section className="target-binding-block safety-policy-panel" aria-label="Safety policy">
      <div className="target-binding-header">
        <span>Safety policy</span>
      </div>
      <div className="safety-policy-grid">
        <div className="safety-policy-field">
          <Label htmlFor="safety-watchdog">Watchdog (ms)</Label>
          <Input
            id="safety-watchdog"
            type="number"
            min={50}
            max={10000}
            value={policy.watchdogMs}
            onChange={(event) => update("watchdogMs", Number(event.target.value) || policy.watchdogMs)}
          />
        </div>
        <div className="safety-policy-field">
          <Label htmlFor="safety-estop">E-stop channel</Label>
          <Input
            id="safety-estop"
            value={policy.eStopChannel}
            onChange={(event) => update("eStopChannel", event.target.value)}
            spellCheck={false}
          />
        </div>
        <div className="safety-policy-field safety-policy-field-wide">
          <Label htmlFor="safety-retained">Retained-state store</Label>
          <Input
            id="safety-retained"
            value={policy.retainedStateStore}
            onChange={(event) => update("retainedStateStore", event.target.value)}
            spellCheck={false}
          />
        </div>
        <label className="safety-policy-toggle">
          <input
            type="checkbox"
            checked={policy.operatorEnableRequired}
            onChange={(event) => update("operatorEnableRequired", event.target.checked)}
          />
          Operator enable required
        </label>
        <label className="safety-policy-toggle">
          <input
            type="checkbox"
            checked={policy.protectiveStopEnabled}
            onChange={(event) => update("protectiveStopEnabled", event.target.checked)}
          />
          Protective stop enabled
        </label>
        <label className="safety-policy-toggle">
          <input
            type="checkbox"
            checked={policy.supervisorReporting}
            onChange={(event) => update("supervisorReporting", event.target.checked)}
          />
          Supervisor reporting
        </label>
        <label className="safety-policy-toggle">
          <input
            type="checkbox"
            checked={policy.safetyGatingEnabled}
            onChange={(event) => update("safetyGatingEnabled", event.target.checked)}
          />
          Safety gating enabled
        </label>
      </div>
      <Button type="button" size="sm" variant="secondary" className="safety-policy-save" onClick={onSave}>
        Save safety policy to mapping
      </Button>
    </section>
  );
}
