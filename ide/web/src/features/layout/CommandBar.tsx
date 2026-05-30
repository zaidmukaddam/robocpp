import type { ReactNode } from "react";
import {
  Box,
  CheckCircle2,
  ChevronDown,
  FileCode2,
  FileInput,
  FileOutput,
  FolderOpen,
  Package,
  Play,
  Plus,
  Rocket,
  Save,
  Settings,
  Upload
} from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuTrigger
} from "@/components/ui/dropdown-menu";

type CommandBarProps = {
  runState: "idle" | "running" | "complete";
  onNewProject: () => void;
  onOpenProject: () => void;
  onSave: () => void;
  onExportBundle: () => void;
  onImportBundle: () => void;
  onNewFile: () => void;
  onCheck: () => void;
  onRun: () => void;
  onBuildC: () => void;
  onImportPlcopen: () => void;
  onExportPlcopen: () => void;
  onDeploy: () => void;
  onSettings: () => void;
};

type CommandMenuProps = {
  label: string;
  ariaLabel: string;
  children: ReactNode;
};

function CommandMenu({ label, ariaLabel, children }: CommandMenuProps) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="sm" aria-label={ariaLabel}>
          {label}
          <ChevronDown data-icon="inline-end" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="min-w-52">
        <DropdownMenuGroup>{children}</DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function CommandBar({
  runState,
  onNewProject,
  onOpenProject,
  onSave,
  onExportBundle,
  onImportBundle,
  onNewFile,
  onCheck,
  onRun,
  onBuildC,
  onImportPlcopen,
  onExportPlcopen,
  onDeploy,
  onSettings
}: CommandBarProps) {
  const running = runState === "running";

  return (
    <div className="commandbar" role="toolbar" aria-label="Studio actions">
      <div className="commandbar-inner">
        <CommandMenu label="Project" ariaLabel="Project menu">
          <DropdownMenuItem onClick={onNewProject}>
            <Plus />
            New project
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onOpenProject}>
            <FolderOpen />
            Open project
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onSave}>
            <Save />
            Save project
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onNewFile}>
            <FileCode2 />
            New file
          </DropdownMenuItem>
        </CommandMenu>

        <CommandMenu label="Build" ariaLabel="Build menu">
          <DropdownMenuItem onClick={onCheck}>
            <CheckCircle2 />
            Check project
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onBuildC}>
            <Box />
            Build portable C
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onDeploy}>
            <Rocket />
            Prepare deploy package
          </DropdownMenuItem>
        </CommandMenu>

        <CommandMenu label="Exchange" ariaLabel="Exchange menu">
          <DropdownMenuItem onClick={onImportBundle}>
            <Upload />
            Import project bundle
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onExportBundle}>
            <Package />
            Export project bundle
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onImportPlcopen}>
            <FileInput />
            Import PLCopen XML
          </DropdownMenuItem>
          <DropdownMenuItem onClick={onExportPlcopen}>
            <FileOutput />
            Export PLCopen XML
          </DropdownMenuItem>
        </CommandMenu>

        <div className="commandbar-spacer" aria-hidden="true" />

        <Button
          size="sm"
          disabled={running}
          title="Run simulation (F5)"
          aria-label={running ? "Simulation running" : "Run simulation"}
          onClick={onRun}
        >
          <Play data-icon="inline-start" />
          {running ? "Running…" : "Run"}
        </Button>

        <Button variant="ghost" size="icon-sm" title="IDE settings" aria-label="Settings" onClick={onSettings}>
          <Settings />
        </Button>
      </div>
    </div>
  );
}
