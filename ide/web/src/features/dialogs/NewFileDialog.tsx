import { useMemo, useState } from "react";
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
  FieldError,
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
import {
  FILE_LANGUAGE_OPTIONS,
  defaultFileName,
  languageFromFileName,
  type FileLanguageId
} from "@/features/project/fileTemplates";

function replaceExtension(name: string, extension: string): string {
  const trimmed = name.trim() || "new";
  const base = trimmed.includes(".") ? trimmed.slice(0, trimmed.lastIndexOf(".")) : trimmed;
  return `${base}${extension}`;
}

type NewFileDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  existingNames: string[];
  onCreate: (name: string, languageId: FileLanguageId) => void;
};

export function NewFileDialog({ open, onOpenChange, existingNames, onCreate }: NewFileDialogProps) {
  const [languageId, setLanguageId] = useState<FileLanguageId>("st");
  const [name, setName] = useState(() => defaultFileName("st", existingNames));

  const resolvedLanguage = languageFromFileName(name) ?? languageId;
  const nameTaken = existingNames.includes(name.trim());
  const selectedOption = useMemo(
    () => FILE_LANGUAGE_OPTIONS.find((option) => option.id === resolvedLanguage),
    [resolvedLanguage]
  );

  const handleLanguageChange = (nextLanguage: string) => {
    const option = FILE_LANGUAGE_OPTIONS.find((entry) => entry.id === nextLanguage);
    if (!option) {
      return;
    }
    setLanguageId(option.id);
    setName((current) => replaceExtension(current, option.extension));
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>New File</DialogTitle>
          <DialogDescription>
            Pick a language and file name. The extension updates when you change either field.
          </DialogDescription>
        </DialogHeader>

        <form
          className="flex flex-col gap-6"
          onSubmit={(event) => {
            event.preventDefault();
            const trimmed = name.trim();
            if (!trimmed || nameTaken) {
              return;
            }
            onCreate(trimmed, resolvedLanguage);
          }}
        >
          <FieldGroup className="gap-6">
            <Field>
              <FieldLabel htmlFor="new-file-language">Language</FieldLabel>
              <Select value={resolvedLanguage} onValueChange={handleLanguageChange}>
                <SelectTrigger id="new-file-language" className="w-full">
                  <SelectValue placeholder="Select a language" />
                </SelectTrigger>
                <SelectContent>
                  <SelectGroup>
                    {FILE_LANGUAGE_OPTIONS.map((option) => (
                      <SelectItem key={option.id} value={option.id}>
                        {option.label} ({option.extension})
                      </SelectItem>
                    ))}
                  </SelectGroup>
                </SelectContent>
              </Select>
              {selectedOption ? (
                <FieldDescription>
                  Creates a {selectedOption.label.toLowerCase()} source file.
                </FieldDescription>
              ) : null}
            </Field>

            <Field data-invalid={nameTaken || undefined}>
              <FieldLabel htmlFor="new-file-name">File name</FieldLabel>
              <Input
                id="new-file-name"
                value={name}
                aria-invalid={nameTaken}
                placeholder="program.ld"
                autoFocus
                required
                className="w-full"
                onChange={(event) => {
                  const nextName = event.target.value;
                  setName(nextName);
                  const detected = languageFromFileName(nextName);
                  if (detected) {
                    setLanguageId(detected);
                  }
                }}
              />
              <FieldDescription>
                Must be unique in this project. Typing an extension updates the language dropdown.
              </FieldDescription>
              {nameTaken ? (
                <FieldError>A file named &quot;{name.trim()}&quot; already exists.</FieldError>
              ) : null}
            </Field>
          </FieldGroup>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button type="submit" disabled={nameTaken}>
              Add File
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
