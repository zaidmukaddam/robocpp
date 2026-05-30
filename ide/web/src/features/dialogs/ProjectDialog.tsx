import { useMemo, useState } from "react";
import { FolderOpenIcon, Trash2Icon } from "lucide-react";
import { Badge } from "@/components/ui/badge";
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
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import type { Project } from "@/types";
import { PROJECT_TEMPLATES, type ProjectTemplateId } from "@/features/project/projectTemplates";

type ProjectDialogProps = {
  open: boolean;
  mode: "new" | "open";
  projects: Project[];
  onOpenChange: (open: boolean) => void;
  onCreate: (name: string, template: ProjectTemplateId) => void;
  onOpen: (project: Project) => void;
  onDelete?: (id: string) => void;
};

export function ProjectDialog({
  open,
  mode,
  projects,
  onOpenChange,
  onCreate,
  onOpen,
  onDelete
}: ProjectDialogProps) {
  const [name, setName] = useState("");
  const [template, setTemplate] = useState<ProjectTemplateId>("sample");

  const selectedTemplate = useMemo(
    () => PROJECT_TEMPLATES.find((option) => option.id === template),
    [template]
  );

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{mode === "new" ? "New Project" : "Open Project"}</DialogTitle>
          <DialogDescription>
            {mode === "new"
              ? "Name your project and pick a workflow template."
              : "Open a sample workspace or a project saved in this browser."}
          </DialogDescription>
        </DialogHeader>

        {mode === "new" ? (
          <form
            className="flex flex-col gap-6"
            onSubmit={(event) => {
              event.preventDefault();
              const trimmed = name.trim();
              if (!trimmed) {
                return;
              }
              onCreate(trimmed, template);
            }}
          >
            <FieldGroup className="gap-6">
              <Field>
                <FieldLabel htmlFor="project-name">Project name</FieldLabel>
                <Input
                  id="project-name"
                  value={name}
                  placeholder="PackagingLine"
                  autoFocus
                  required
                  className="w-full"
                  onChange={(event) => setName(event.target.value)}
                />
              </Field>

              <Field>
                <FieldLabel htmlFor="project-template">Template</FieldLabel>
                <Select value={template} onValueChange={(value) => setTemplate(value as ProjectTemplateId)}>
                  <SelectTrigger id="project-template" className="w-full">
                    <SelectValue placeholder="Select a template" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectGroup>
                      {PROJECT_TEMPLATES.map((option) => (
                        <SelectItem key={option.id} value={option.id}>
                          {option.title}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  </SelectContent>
                </Select>
                {selectedTemplate ? (
                  <FieldDescription>{selectedTemplate.description}</FieldDescription>
                ) : null}
              </Field>
            </FieldGroup>

            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
                Cancel
              </Button>
              <Button type="submit">Create Project</Button>
            </DialogFooter>
          </form>
        ) : (
          <div className="flex flex-col gap-4">
            <ScrollArea className="max-h-72 rounded-none border border-border">
              <div className="flex flex-col">
                {projects.map((project, index) => (
                  <div key={project.id}>
                    {index > 0 ? <Separator /> : null}
                    <div className="flex items-stretch gap-2 p-2">
                      <Button
                        type="button"
                        variant="ghost"
                        className="h-auto flex-1 justify-start gap-3 px-3 py-3 text-left normal-case tracking-normal"
                        onClick={() => onOpen(project)}
                      >
                        <FolderOpenIcon data-icon="inline-start" />
                        <span className="flex min-w-0 flex-1 flex-col gap-1">
                          <span className="flex items-center gap-2">
                            <span className="truncate text-sm font-semibold">{project.name}</span>
                            {project.builtIn ? <Badge variant="secondary">Sample</Badge> : null}
                          </span>
                          <span className="text-sm font-normal text-muted-foreground">
                            {project.files.length} file{project.files.length === 1 ? "" : "s"}
                          </span>
                        </span>
                      </Button>
                      {!project.builtIn && onDelete ? (
                        <Button
                          type="button"
                          variant="destructive"
                          size="icon-sm"
                          aria-label={`Delete ${project.name}`}
                          onClick={() => onDelete(project.id)}
                        >
                          <Trash2Icon />
                        </Button>
                      ) : null}
                    </div>
                  </div>
                ))}
              </div>
            </ScrollArea>

            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
                Cancel
              </Button>
            </DialogFooter>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
