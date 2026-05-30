import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/dialog";

export type TargetHardwareAction = "download" | "run" | "stop" | "reset";

type TargetActionDialogProps = {
  open: boolean;
  action: TargetHardwareAction | null;
  targetLabel: string;
  targetAddress: string;
  programHash: string;
  deployHash: string | null;
  editorMatchesTarget: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
};

const ACTION_COPY: Record<
  TargetHardwareAction,
  { title: string; consequence: string; confirmLabel: string; destructive?: boolean }
> = {
  download: {
    title: "Download to target",
    consequence: "Replaces the active deploy package on the connected controller.",
    confirmLabel: "Download"
  },
  run: {
    title: "Start target program",
    consequence: "Begins executing the downloaded program on the connected controller.",
    confirmLabel: "Run"
  },
  stop: {
    title: "Stop target program",
    consequence: "Halts program execution while keeping the downloaded image on the controller.",
    confirmLabel: "Stop",
    destructive: true
  },
  reset: {
    title: "Reset controller",
    consequence: "Clears runtime state and restarts the controller session.",
    confirmLabel: "Reset",
    destructive: true
  }
};

export function TargetActionDialog({
  open,
  action,
  targetLabel,
  targetAddress,
  programHash,
  deployHash,
  editorMatchesTarget,
  onOpenChange,
  onConfirm
}: TargetActionDialogProps) {
  if (!action) {
    return null;
  }
  const copy = ACTION_COPY[action];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{copy.title}</DialogTitle>
          <DialogDescription>{copy.consequence}</DialogDescription>
        </DialogHeader>
        <dl className="target-action-dialog-meta">
          <div>
            <dt>Target</dt>
            <dd>
              {targetLabel} ({targetAddress})
            </dd>
          </div>
          <div>
            <dt>Program hash</dt>
            <dd>{programHash || "—"}</dd>
          </div>
          <div>
            <dt>Deploy hash</dt>
            <dd>{deployHash || "—"}</dd>
          </div>
          <div>
            <dt>Editor match</dt>
            <dd>{editorMatchesTarget ? "Editor matches target" : "Editor differs from target"}</dd>
          </div>
        </dl>
        {!editorMatchesTarget && action === "run" ? (
          <p className="target-action-dialog-warning" role="alert">
            The editor program hash does not match the connected target. Download before running if you
            need the latest workspace build.
          </p>
        ) : null}
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            type="button"
            variant={copy.destructive ? "destructive" : "default"}
            onClick={() => {
              onConfirm();
              onOpenChange(false);
            }}
          >
            {copy.confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
