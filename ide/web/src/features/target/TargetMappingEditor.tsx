import { useMemo } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/select";
import { MappingKindBadge } from "@/features/target/MappingKindBadge";
import { MAPPING_KIND_META } from "@/features/target/mappingKindMeta";
import {
  DEFAULT_TARGET_MAPPING_TEXT,
  parseTargetMapping,
  serializeTargetMapping,
  type TargetMappingEntry,
  type TargetMappingKind,
  validateTargetMapping
} from "@/features/target/targetMapping";

type TargetMappingEditorProps = {
  text: string;
  onChange: (text: string) => void;
  symbolSuggestions?: string[];
};

function parseModbusTarget(target: string): { unit: string; register: string; address: string } {
  const [unit = "1", register = "coil", address = "0"] = target.split(":");
  return { unit, register, address };
}

function parseEthercatTarget(target: string): { pdo: string; bit: string } {
  const pdoMatch = target.match(/pdo[:=](\d+)/i);
  const bitMatch = target.match(/bit[:=](\d+)/i);
  return { pdo: pdoMatch?.[1] ?? "0", bit: bitMatch?.[1] ?? "0" };
}

function SymbolField({
  entry,
  symbolSuggestions,
  onChange
}: {
  entry: TargetMappingEntry;
  symbolSuggestions: string[];
  onChange: (symbol: string) => void;
}) {
  if (symbolSuggestions.length > 0) {
    return (
      <Select value={entry.symbol} onValueChange={onChange}>
        <SelectTrigger className="mapping-field-control" aria-label={`PLC symbol for ${entry.kind} binding`}>
          <SelectValue placeholder="Pick symbol" />
        </SelectTrigger>
        <SelectContent>
          <SelectGroup>
            {symbolSuggestions.map((name) => (
              <SelectItem key={name} value={name}>
                {name}
              </SelectItem>
            ))}
          </SelectGroup>
        </SelectContent>
      </Select>
    );
  }

  return (
    <Input
      className="mapping-field-control"
      aria-label={`PLC symbol for ${entry.kind} binding`}
      value={entry.symbol}
      onChange={(event) => onChange(event.target.value)}
      spellCheck={false}
    />
  );
}

export function TargetMappingEditor({ text, onChange, symbolSuggestions = [] }: TargetMappingEditorProps) {
  const document = useMemo(() => parseTargetMapping(text), [text]);
  const issues = useMemo(() => validateTargetMapping(document), [document]);

  const updateEntries = (entries: TargetMappingEntry[]) => {
    onChange(serializeTargetMapping({ entries }));
  };

  const addEntry = (kind: TargetMappingKind) => {
    const entry: TargetMappingEntry = {
      id: crypto.randomUUID(),
      kind,
      symbol: symbolSuggestions[0] ?? "NewSymbol",
      target:
        kind === "file"
          ? "io/new.txt"
          : kind === "modbus"
            ? "1:coil:0"
            : kind === "ethercat"
              ? "pdo:0/bit:0"
              : "/topic/command"
    };
    updateEntries([...document.entries, entry]);
  };

  const updateEntry = (id: string, patch: Partial<TargetMappingEntry>) => {
    updateEntries(document.entries.map((entry) => (entry.id === id ? { ...entry, ...patch } : entry)));
  };

  const removeEntry = (id: string) => {
    updateEntries(document.entries.filter((entry) => entry.id !== id));
  };

  return (
    <div className="target-mapping-editor">
      <div className="mapping-editor-bar">
        <span className="mapping-editor-label">Target mappings</span>
        <span className="mapping-editor-meta tabular-nums">
          {document.entries.length} binding{document.entries.length === 1 ? "" : "s"}
          {issues.length === 0 ? " · valid" : ` · ${issues.length} issue${issues.length === 1 ? "" : "s"}`}
        </span>
      </div>

      <div className="mapping-editor-toolbar toolbar-group file-actions" role="toolbar" aria-label="Add binding">
        {(Object.keys(MAPPING_KIND_META) as TargetMappingKind[]).map((kind) => (
          <button key={kind} type="button" title={MAPPING_KIND_META[kind].hint} onClick={() => addEntry(kind)}>
            <span>{MAPPING_KIND_META[kind].label}</span>
          </button>
        ))}
        <button type="button" onClick={() => onChange(DEFAULT_TARGET_MAPPING_TEXT)}>
          Reset sample
        </button>
      </div>

      {issues.length > 0 ? (
        <ul className="target-mapping-issues" aria-live="polite">
          {issues.map((issue) => (
            <li key={issue}>{issue}</li>
          ))}
        </ul>
      ) : null}

      <div className="mapping-binding-list" aria-label="Target bindings">
        {document.entries.length === 0 ? (
          <div className="empty-row">No bindings yet. Add a transport row to map PLC symbols to target I/O.</div>
        ) : (
          document.entries.map((entry) => (
            <article key={entry.id} className="mapping-binding-card">
              <div className="mapping-binding-card-head">
                <MappingKindBadge kind={entry.kind} compact />
                <code className="mapping-binding-target-preview" title={entry.target}>
                  {entry.target}
                </code>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="mapping-remove-btn"
                  aria-label={`Remove ${entry.symbol} binding`}
                  onClick={() => removeEntry(entry.id)}
                >
                  Remove
                </Button>
              </div>

              <div className="mapping-binding-grid">
                <label className="mapping-field">
                  <span>PLC symbol</span>
                  <SymbolField
                    entry={entry}
                    symbolSuggestions={symbolSuggestions}
                    onChange={(symbol) => updateEntry(entry.id, { symbol })}
                  />
                </label>

                {entry.kind === "file" ? (
                  <>
                    <label className="mapping-field">
                      <span>Relative path</span>
                      <Input
                        className="mapping-field-control"
                        aria-label={`Target path for ${entry.symbol}`}
                        value={entry.target}
                        onChange={(event) => updateEntry(entry.id, { target: event.target.value })}
                        spellCheck={false}
                      />
                    </label>
                    <label className="mapping-field">
                      <span>Encoding</span>
                      <Select
                        value={entry.encoding ?? "decimal"}
                        onValueChange={(value) => updateEntry(entry.id, { encoding: value })}
                      >
                        <SelectTrigger className="mapping-field-control">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="bool">bool</SelectItem>
                          <SelectItem value="decimal">decimal</SelectItem>
                          <SelectItem value="hex">hex</SelectItem>
                        </SelectContent>
                      </Select>
                    </label>
                  </>
                ) : null}

                {entry.kind === "modbus" ? (
                  <TransportFields
                    fields={[
                      {
                        label: "Unit",
                        value: parseModbusTarget(entry.target).unit,
                        onChange: (unit) => {
                          const current = parseModbusTarget(entry.target);
                          updateEntry(entry.id, { target: `${unit}:${current.register}:${current.address}` });
                        }
                      },
                      {
                        label: "Register",
                        value: parseModbusTarget(entry.target).register,
                        onChange: (register) => {
                          const current = parseModbusTarget(entry.target);
                          updateEntry(entry.id, { target: `${current.unit}:${register}:${current.address}` });
                        }
                      },
                      {
                        label: "Address",
                        value: parseModbusTarget(entry.target).address,
                        onChange: (address) => {
                          const current = parseModbusTarget(entry.target);
                          updateEntry(entry.id, { target: `${current.unit}:${current.register}:${address}` });
                        }
                      }
                    ]}
                  />
                ) : null}

                {entry.kind === "ethercat" ? (
                  <TransportFields
                    fields={[
                      {
                        label: "PDO",
                        value: parseEthercatTarget(entry.target).pdo,
                        onChange: (pdo) => {
                          const current = parseEthercatTarget(entry.target);
                          updateEntry(entry.id, { target: `pdo:${pdo}/bit:${current.bit}` });
                        }
                      },
                      {
                        label: "Bit",
                        value: parseEthercatTarget(entry.target).bit,
                        onChange: (bit) => {
                          const current = parseEthercatTarget(entry.target);
                          updateEntry(entry.id, { target: `pdo:${current.pdo}/bit:${bit}` });
                        }
                      }
                    ]}
                  />
                ) : null}

                {entry.kind === "ros2" ? (
                  <label className="mapping-field mapping-field-wide">
                    <span>Topic</span>
                    <Input
                      className="mapping-field-control"
                      aria-label={`ROS 2 topic for ${entry.symbol}`}
                      value={entry.target}
                      placeholder="/robot/command"
                      onChange={(event) => updateEntry(entry.id, { target: event.target.value })}
                      spellCheck={false}
                    />
                  </label>
                ) : null}
              </div>
            </article>
          ))
        )}
      </div>
    </div>
  );
}

function TransportFields({
  fields
}: {
  fields: { label: string; value: string; onChange: (value: string) => void }[];
}) {
  return (
    <>
      {fields.map((field) => (
        <label key={field.label} className="mapping-field">
          <span>{field.label}</span>
          <Input
            className="mapping-field-control"
            value={field.value}
            onChange={(event) => field.onChange(event.target.value)}
            spellCheck={false}
          />
        </label>
      ))}
    </>
  );
}
