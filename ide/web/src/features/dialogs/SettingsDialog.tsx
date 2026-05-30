import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/dialog";
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/select";
import type { CompilerProfile, IdeSettings } from "@/types";
import type { SupportDiagnosticsInput } from "@/lib/supportDiagnostics";
import { downloadSupportDiagnostics, buildSupportDiagnostics } from "@/lib/supportDiagnostics";
import { DEFAULT_SETTINGS } from "@/stores/settingsStore";
import { IDE_KEYBOARD_SHORTCUTS } from "@/lib/keyboardShortcuts";

type SettingsDialogProps = {
  open: boolean;
  settings: IdeSettings;
  programNames: string[];
  supportDiagnostics?: SupportDiagnosticsInput | null;
  onOpenChange: (open: boolean) => void;
  onSave: (settings: IdeSettings) => void;
};

const PROFILE_OPTIONS: { value: CompilerProfile; label: string; description: string }[] = [
  {
    value: "2003-strict",
    label: "IEC 61131-3:2003 strict",
    description: "Default RoboC++ compliance profile for the web IDE."
  },
  {
    value: "2003-extended",
    label: "IEC 61131-3:2003 extended",
    description: "Reserved for future profile extensions."
  }
];

export function SettingsDialog({
  open,
  settings,
  programNames,
  supportDiagnostics,
  onOpenChange,
  onSave
}: SettingsDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {open ? (
          <SettingsDialogForm
            key={`${settings.compilerProfile}:${settings.cycleTimeMs}:${settings.simulationCycles}:${settings.watchVariables}`}
            settings={settings}
            programNames={programNames}
            supportDiagnostics={supportDiagnostics}
            onOpenChange={onOpenChange}
            onSave={onSave}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function SettingsDialogForm({
  settings,
  programNames,
  supportDiagnostics,
  onOpenChange,
  onSave
}: Omit<SettingsDialogProps, "open">) {
  const [draft, setDraft] = useState<IdeSettings>(settings);
  const selectedProfile = PROFILE_OPTIONS.find((option) => option.value === draft.compilerProfile);

  return (
    <>
      <DialogHeader>
        <DialogTitle>IDE Settings</DialogTitle>
        <DialogDescription>
          Compiler profile, simulation defaults, and generated artifact paths for this browser workspace.
        </DialogDescription>
      </DialogHeader>

      <form
        className="flex flex-col gap-6"
        onSubmit={(event) => {
          event.preventDefault();
          onSave(draft);
          onOpenChange(false);
        }}
      >
        <FieldGroup className="gap-5">
          <Field>
            <FieldLabel htmlFor="compiler-profile">Compiler profile</FieldLabel>
            <Select
              value={draft.compilerProfile}
              onValueChange={(value) =>
                setDraft((current) => ({ ...current, compilerProfile: value as CompilerProfile }))
              }
            >
              <SelectTrigger id="compiler-profile" className="w-full">
                <SelectValue placeholder="Select profile" />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  {PROFILE_OPTIONS.map((option) => (
                    <SelectItem key={option.value} value={option.value}>
                      {option.label}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
            {selectedProfile ? <FieldDescription>{selectedProfile.description}</FieldDescription> : null}
          </Field>

          <div className="settings-grid">
            <Field>
              <FieldLabel htmlFor="cycle-time">Cycle time (ms)</FieldLabel>
              <Input
                id="cycle-time"
                type="number"
                min={1}
                step={0.01}
                value={draft.cycleTimeMs}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    cycleTimeMs: Number(event.target.value) || DEFAULT_SETTINGS.cycleTimeMs
                  }))
                }
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="sim-cycles">Simulation cycles</FieldLabel>
              <Input
                id="sim-cycles"
                type="number"
                min={1}
                max={100}
                value={draft.simulationCycles}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    simulationCycles: Math.max(1, Number(event.target.value) || 1)
                  }))
                }
              />
            </Field>
          </div>

          <Field>
            <FieldLabel htmlFor="selected-program">Selected program</FieldLabel>
            <Select
              value={draft.selectedProgram || "__auto__"}
              onValueChange={(value) =>
                setDraft((current) => ({
                  ...current,
                  selectedProgram: value === "__auto__" ? "" : value
                }))
              }
            >
              <SelectTrigger id="selected-program" className="w-full">
                <SelectValue placeholder="Auto-detect from active file" />
              </SelectTrigger>
              <SelectContent>
                <SelectGroup>
                  <SelectItem value="__auto__">Auto-detect from active file</SelectItem>
                  {programNames.map((name) => (
                    <SelectItem key={name} value={name}>
                      {name}
                    </SelectItem>
                  ))}
                </SelectGroup>
              </SelectContent>
            </Select>
          </Field>

          <Field>
            <FieldLabel htmlFor="selected-configuration">Selected configuration</FieldLabel>
            <Input
              id="selected-configuration"
              value={draft.selectedConfiguration}
              placeholder="CONFIGURATION MainCfg"
              onChange={(event) =>
                setDraft((current) => ({ ...current, selectedConfiguration: event.target.value }))
              }
            />
          </Field>

          <Field>
            <FieldLabel htmlFor="generated-c-path">Generated C output path</FieldLabel>
            <Input
              id="generated-c-path"
              value={draft.generatedCOutputPath}
              spellCheck={false}
              onChange={(event) =>
                setDraft((current) => ({ ...current, generatedCOutputPath: event.target.value }))
              }
            />
          </Field>

          <Field>
            <FieldLabel htmlFor="target-mapping-path">Target mapping file</FieldLabel>
            <Input
              id="target-mapping-path"
              value={draft.targetMappingPath}
              spellCheck={false}
              onChange={(event) =>
                setDraft((current) => ({ ...current, targetMappingPath: event.target.value }))
              }
            />
          </Field>

          <Field>
            <FieldLabel htmlFor="target-bridge-url">Target bridge URL</FieldLabel>
            <Input
              id="target-bridge-url"
              value={draft.targetBridgeUrl}
              spellCheck={false}
              placeholder="http://127.0.0.1:8787"
              onChange={(event) =>
                setDraft((current) => ({ ...current, targetBridgeUrl: event.target.value }))
              }
            />
            <FieldDescription>Local rbcpp-target-bridge HTTP endpoint for hardware I/O.</FieldDescription>
          </Field>

          <Field>
            <FieldLabel htmlFor="target-workspace-root">Target workspace root</FieldLabel>
            <Input
              id="target-workspace-root"
              value={draft.targetWorkspaceRoot}
              spellCheck={false}
              placeholder="~/.robocpp/studio-target (default)"
              onChange={(event) =>
                setDraft((current) => ({ ...current, targetWorkspaceRoot: event.target.value }))
              }
            />
            <FieldDescription>Optional override for deploy and file-backed I/O on disk.</FieldDescription>
          </Field>

          <Field>
            <FieldLabel htmlFor="target-modbus-port">Modbus TCP port</FieldLabel>
            <Input
              id="target-modbus-port"
              type="number"
              min={1}
              max={65535}
              value={draft.targetModbusPort}
              onChange={(event) =>
                setDraft((current) => ({
                  ...current,
                  targetModbusPort: Math.max(1, Number(event.target.value) || 502)
                }))
              }
            />
          </Field>

          <Field>
            <FieldLabel htmlFor="watch-variables">Watch variables</FieldLabel>
            <Input
              id="watch-variables"
              value={draft.watchVariables}
              placeholder="Count, Done, Motor"
              spellCheck={false}
              onChange={(event) =>
                setDraft((current) => ({ ...current, watchVariables: event.target.value }))
              }
            />
            <FieldDescription>Comma-separated names shown in the simulator watch panel.</FieldDescription>
          </Field>
        </FieldGroup>

        <section aria-label="Keyboard shortcuts" className="settings-shortcuts">
          <h3 className="settings-shortcuts-title">Keyboard shortcuts</h3>
          <ul className="settings-shortcuts-list">
            {IDE_KEYBOARD_SHORTCUTS.map((shortcut) => (
              <li key={shortcut.keys}>
                <kbd>{shortcut.keys}</kbd>
                <span>{shortcut.action}</span>
              </li>
            ))}
          </ul>
        </section>

        <DialogFooter className="gap-2 sm:justify-between">
          {supportDiagnostics ? (
            <Button
              type="button"
              variant="outline"
              aria-label="Export support diagnostics"
              onClick={() => {
                const exportData = buildSupportDiagnostics(supportDiagnostics);
                downloadSupportDiagnostics(exportData, supportDiagnostics.project.name);
              }}
            >
              Export support diagnostics
            </Button>
          ) : (
            <span />
          )}
          <div className="flex gap-2">
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit">Save Settings</Button>
          </div>
        </DialogFooter>
      </form>
    </>
  );
}
