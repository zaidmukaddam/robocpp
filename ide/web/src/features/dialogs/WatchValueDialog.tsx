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
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/select";
import { canonicalIecValue, validateIecValue } from "@/lib/iecValueValidation";
import type { DocumentSymbol } from "@/types";

export type WatchDialogMode = "add" | "write" | "force";

type WatchValueDialogProps = {
  open: boolean;
  mode: WatchDialogMode;
  variableName?: string;
  iecType?: string;
  initialValue?: string;
  symbols: DocumentSymbol[];
  onOpenChange: (open: boolean) => void;
  onSubmit: (name: string, value: string, persistent: boolean) => void;
};

const MODE_COPY: Record<
  WatchDialogMode,
  { title: string; description: string; submit: string; persistent: boolean }
> = {
  add: {
    title: "Add watch",
    description: "Monitor a program variable during simulation or live target I/O.",
    submit: "Add watch",
    persistent: false
  },
  write: {
    title: "Write value",
    description: "Send a one-shot prepared value to the target for this symbol.",
    submit: "Write",
    persistent: false
  },
  force: {
    title: "Force value",
    description: "Hold this value across scans until you clear the force.",
    submit: "Force",
    persistent: true
  }
};

export function WatchValueDialog({
  open,
  mode,
  variableName = "",
  iecType = "BOOL",
  initialValue = "",
  symbols,
  onOpenChange,
  onSubmit
}: WatchValueDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        {open ? (
          <WatchValueDialogForm
            key={`${mode}:${variableName}:${initialValue}`}
            mode={mode}
            variableName={variableName}
            iecType={iecType}
            initialValue={initialValue}
            symbols={symbols}
            onOpenChange={onOpenChange}
            onSubmit={onSubmit}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function WatchValueDialogForm({
  mode,
  variableName,
  iecType,
  initialValue,
  symbols,
  onOpenChange,
  onSubmit
}: Omit<WatchValueDialogProps, "open">) {
  const copy = MODE_COPY[mode];
  const [name, setName] = useState(variableName ?? "");
  const [value, setValue] = useState(initialValue ?? "");
  const [selectedType, setSelectedType] = useState(iecType);
  const [error, setError] = useState<string | null>(null);

  const resolvedType =
    symbols.find((entry) => entry.name === name)?.detail.match(/:\s*([A-Za-z0-9_()]+)/)?.[1] ??
    selectedType ??
    "BOOL";

  const handleSubmit = () => {
    const trimmedName = name.trim();
    if (!trimmedName) {
      setError("Variable name is required.");
      return;
    }
    const validationError = validateIecValue(value, resolvedType);
    if (validationError) {
      setError(validationError);
      return;
    }
    onSubmit(trimmedName, canonicalIecValue(value, resolvedType), copy.persistent);
    onOpenChange(false);
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>{copy.title}</DialogTitle>
        <DialogDescription>{copy.description}</DialogDescription>
      </DialogHeader>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="watch-dialog-name">Variable</FieldLabel>
          {mode === "add" ? (
            <Select
              value={name || undefined}
              onValueChange={(next) => {
                setName(next);
                setError(null);
                const symbol = symbols.find((entry) => entry.name === next);
                const typeMatch = symbol?.detail.match(/:\s*([A-Za-z0-9_()]+)/);
                if (typeMatch?.[1]) {
                  setSelectedType(typeMatch[1]);
                }
              }}
            >
              <SelectTrigger id="watch-dialog-name" className="w-full">
                <SelectValue placeholder="Choose a symbol" />
              </SelectTrigger>
              <SelectContent>
                {symbols.map((symbol) => (
                  <SelectItem key={symbol.name} value={symbol.name}>
                    {symbol.name}
                    {symbol.detail ? ` (${symbol.detail})` : ""}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : (
            <Input id="watch-dialog-name" value={name} readOnly />
          )}
        </Field>
        <Field>
          <FieldLabel htmlFor="watch-dialog-type">IEC type</FieldLabel>
          <Input id="watch-dialog-type" value={resolvedType} readOnly />
        </Field>
        <Field data-invalid={error ? true : undefined}>
          <FieldLabel htmlFor="watch-dialog-value">Value</FieldLabel>
          <Input
            id="watch-dialog-value"
            value={value}
            spellCheck={false}
            placeholder={resolvedType.toUpperCase().includes("BOOL") ? "TRUE or FALSE" : "Literal value"}
            onChange={(event) => {
              setValue(event.target.value);
              setError(null);
            }}
          />
          {error ? <FieldDescription className="text-destructive">{error}</FieldDescription> : null}
        </Field>
      </FieldGroup>
      <DialogFooter>
        <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
          Cancel
        </Button>
        <Button type="button" onClick={handleSubmit}>
          {copy.submit}
        </Button>
      </DialogFooter>
    </>
  );
}
