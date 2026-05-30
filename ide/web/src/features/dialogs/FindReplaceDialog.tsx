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

export type FindReplaceMode = "find" | "replace";

type FindReplaceDialogProps = {
  open: boolean;
  mode: FindReplaceMode;
  fileName?: string;
  initialQuery?: string;
  onOpenChange: (open: boolean) => void;
  onFindNext: (query: string) => boolean;
  onReplaceNext: (query: string, replacement: string) => boolean;
  onReplaceAll: (query: string, replacement: string) => number;
  onStatus: (message: string, level?: "info" | "error") => void;
};

export function FindReplaceDialog({
  open,
  mode,
  fileName,
  initialQuery = "",
  onOpenChange,
  onFindNext,
  onReplaceNext,
  onReplaceAll,
  onStatus
}: FindReplaceDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        {open ? (
          <FindReplaceDialogForm
            key={`${mode}:${fileName ?? "file"}:${initialQuery}`}
            mode={mode}
            fileName={fileName}
            initialQuery={initialQuery}
            onOpenChange={onOpenChange}
            onFindNext={onFindNext}
            onReplaceNext={onReplaceNext}
            onReplaceAll={onReplaceAll}
            onStatus={onStatus}
          />
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function FindReplaceDialogForm({
  mode,
  fileName,
  initialQuery,
  onOpenChange,
  onFindNext,
  onReplaceNext,
  onReplaceAll,
  onStatus
}: Omit<FindReplaceDialogProps, "open">) {
  const [query, setQuery] = useState(initialQuery ?? "");
  const [replacement, setReplacement] = useState("");
  const [error, setError] = useState<string | null>(null);

  const runFindNext = () => {
    const trimmed = query.trim();
    if (!trimmed) {
      setError("Enter text to find.");
      return;
    }
    if (!onFindNext(trimmed)) {
      onStatus(`No matches for "${trimmed}" in ${fileName ?? "file"}.`, "info");
      return;
    }
    setError(null);
  };

  const runReplaceNext = () => {
    const trimmed = query.trim();
    if (!trimmed) {
      setError("Enter text to find.");
      return;
    }
    if (!onReplaceNext(trimmed, replacement)) {
      onStatus(`No matches for "${trimmed}" in ${fileName ?? "file"}.`, "info");
      return;
    }
    setError(null);
  };

  const runReplaceAll = () => {
    const trimmed = query.trim();
    if (!trimmed) {
      setError("Enter text to find.");
      return;
    }
    const count = onReplaceAll(trimmed, replacement);
    if (count === 0) {
      onStatus(`No matches for "${trimmed}" in ${fileName ?? "file"}.`, "info");
      return;
    }
    onStatus(`Replaced ${count} occurrence(s) in ${fileName ?? "file"}.`);
    setError(null);
  };

  return (
    <>
      <DialogHeader>
        <DialogTitle>{mode === "find" ? "Find in file" : "Replace in file"}</DialogTitle>
        <DialogDescription>
          {fileName ? `Search within ${fileName}.` : "Search within the active editor file."}
        </DialogDescription>
      </DialogHeader>
      <FieldGroup>
        <Field data-invalid={error ? true : undefined}>
          <FieldLabel htmlFor="find-dialog-query">Find</FieldLabel>
          <Input
            id="find-dialog-query"
            value={query}
            spellCheck={false}
            autoFocus
            onChange={(event) => {
              setQuery(event.target.value);
              setError(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                if (mode === "find") {
                  runFindNext();
                } else {
                  runReplaceNext();
                }
              }
            }}
          />
        </Field>
        {mode === "replace" ? (
          <Field>
            <FieldLabel htmlFor="find-dialog-replacement">Replace with</FieldLabel>
            <Input
              id="find-dialog-replacement"
              value={replacement}
              spellCheck={false}
              onChange={(event) => setReplacement(event.target.value)}
            />
          </Field>
        ) : null}
        {error ? <FieldDescription className="text-destructive">{error}</FieldDescription> : null}
      </FieldGroup>
      <DialogFooter className="find-replace-dialog-footer">
        {mode === "replace" ? (
          <Button type="button" variant="secondary" onClick={runReplaceAll}>
            Replace all
          </Button>
        ) : null}
        <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
          Close
        </Button>
        {mode === "find" ? (
          <Button type="button" onClick={runFindNext}>
            Find next
          </Button>
        ) : (
          <Button type="button" onClick={runReplaceNext}>
            Replace next
          </Button>
        )}
      </DialogFooter>
    </>
  );
}
