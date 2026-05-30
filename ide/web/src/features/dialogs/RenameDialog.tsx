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

type RenameDialogProps = {
  open: boolean;
  title: string;
  description?: string;
  currentName: string;
  fieldLabel?: string;
  validate?: (nextName: string) => string | null;
  onOpenChange: (open: boolean) => void;
  onSubmit: (nextName: string) => void;
};

export function RenameDialog({
  open,
  title,
  description,
  currentName,
  fieldLabel = "Name",
  validate,
  onOpenChange,
  onSubmit
}: RenameDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        {open ? (
          <RenameDialogForm
            key={currentName}
            title={title}
            description={description}
            currentName={currentName}
            fieldLabel={fieldLabel}
            validate={validate}
            onOpenChange={onOpenChange}
            onSubmit={onSubmit}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function RenameDialogForm({
  title,
  description,
  currentName,
  fieldLabel,
  validate,
  onOpenChange,
  onSubmit
}: Omit<RenameDialogProps, "open">) {
  const [name, setName] = useState(currentName);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = () => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError("Name is required.");
      return;
    }
    if (trimmed === currentName) {
      onOpenChange(false);
      return;
    }
    const validationError = validate?.(trimmed) ?? null;
    if (validationError) {
      setError(validationError);
      return;
    }
    onSubmit(trimmed);
    onOpenChange(false);
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>{title}</DialogTitle>
        {description ? <DialogDescription>{description}</DialogDescription> : null}
      </DialogHeader>
      <FieldGroup>
        <Field data-invalid={error ? true : undefined}>
          <FieldLabel htmlFor="rename-dialog-input">{fieldLabel}</FieldLabel>
          <Input
            id="rename-dialog-input"
            value={name}
            spellCheck={false}
            autoFocus
            onChange={(event) => {
              setName(event.target.value);
              setError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                handleSubmit();
              }
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
          Rename
        </Button>
      </DialogFooter>
    </>
  );
}
