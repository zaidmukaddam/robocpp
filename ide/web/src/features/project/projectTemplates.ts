import { DEFAULT_TARGET_MAPPING_TEXT } from "@/features/target/targetMapping";
import { workspaceFiles } from "@/features/project/samples";
import type { WorkspaceFile } from "@/types";

export type ProjectTemplateId =
  | "empty"
  | "sample"
  | "conveyor-line"
  | "pick-place"
  | "safety-motion";

export type ProjectTemplate = {
  id: ProjectTemplateId;
  title: string;
  description: string;
  buildFiles: () => WorkspaceFile[];
};

function clone(files: WorkspaceFile[]): WorkspaceFile[] {
  return files.map((file) => ({ ...file }));
}

const CONVEYOR_MAPPING = `# key, relative_file, encoding
RunRequest, io/run_request.txt, bool
MotorOn, io/motor_on.txt, bool
PhotoEye, io/photo_eye.txt, bool
`;

const PICK_PLACE_MAPPING = `# key, relative_file, encoding
GripperClosed, io/gripper_closed.txt, bool
AtPick, io/at_pick.txt, bool
AtPlace, io/at_place.txt, bool
`;

const SAFETY_MAPPING = `# key, relative_file, encoding
EStopOk, io/estop_ok.txt, bool
GuardClosed, io/guard_closed.txt, bool
DriveEnable, io/drive_enable.txt, bool
`;

export const PROJECT_TEMPLATES: ProjectTemplate[] = [
  {
    id: "sample",
    title: "Sample workspace",
    description: "Starter files for ST, LD, SFC, FBD, and PLCopen XML.",
    buildFiles: () => [
      ...clone(workspaceFiles),
      { name: "target/mapping.toml", languageId: "mapping", text: DEFAULT_TARGET_MAPPING_TEXT }
    ]
  },
  {
    id: "empty",
    title: "Empty project",
    description: "Single Structured Text program named main.st.",
    buildFiles: () => [
      {
        name: "main.st",
        languageId: "st",
        text: `PROGRAM Main
VAR
END_VAR
END_PROGRAM
`
      }
    ]
  },
  {
    id: "conveyor-line",
    title: "Conveyor line",
    description: "Start/stop conveyor with photo-eye indexing and motor output.",
    buildFiles: () => [
      {
        name: "conveyor.st",
        languageId: "st",
        text: `PROGRAM ConveyorLine
VAR
    RunRequest : BOOL;
    PhotoEye : BOOL;
    MotorOn : BOOL;
    IndexCount : INT := 0;
END_VAR

IF RunRequest AND PhotoEye THEN
    IndexCount := IndexCount + 1;
END_IF;

MotorOn := RunRequest;
END_PROGRAM
`
      },
      {
        name: "conveyor.ld",
        languageId: "ld",
        text: `PROGRAM ConveyorLd
VAR
    RunRequest : BOOL;
    MotorOn : BOOL;
END_VAR
LADDER
RUNG
    CONTACT RunRequest;
    COIL MotorOn;
END_RUNG
END_LADDER
END_PROGRAM
`
      },
      { name: "target/mapping.toml", languageId: "mapping", text: CONVEYOR_MAPPING }
    ]
  },
  {
    id: "pick-place",
    title: "Pick and place",
    description: "Two-position sequence with gripper and station sensors.",
    buildFiles: () => [
      {
        name: "pick_place.sfc",
        languageId: "sfc",
        text: `PROGRAM PickPlace
VAR
    AtPick : BOOL;
    AtPlace : BOOL;
    GripperClosed : BOOL;
END_VAR
SFC
INITIAL_STEP Idle;
STEP Idle
    TRANSITION ToPick WHEN AtPick;
STEP Pick
    ACTION CloseGripper : GripperClosed := TRUE;
    TRANSITION ToPlace WHEN AtPlace;
STEP Place
    ACTION OpenGripper : GripperClosed := FALSE;
    TRANSITION ToIdle WHEN NOT AtPlace;
END_SFC
END_PROGRAM
`
      },
      { name: "target/mapping.toml", languageId: "mapping", text: PICK_PLACE_MAPPING }
    ]
  },
  {
    id: "safety-motion",
    title: "Safety-gated motion",
    description: "Drive enable only when E-stop and guard inputs are healthy.",
    buildFiles: () => [
      {
        name: "safety_motion.st",
        languageId: "st",
        text: `PROGRAM SafetyMotion
VAR
    EStopOk : BOOL;
    GuardClosed : BOOL;
    DriveEnable : BOOL;
    MotionRequest : BOOL;
    MotionActive : BOOL;
END_VAR

DriveEnable := EStopOk AND GuardClosed;
MotionActive := MotionRequest AND DriveEnable;
END_PROGRAM
`
      },
      { name: "target/mapping.toml", languageId: "mapping", text: SAFETY_MAPPING }
    ]
  }
];

export function projectTemplateById(id: ProjectTemplateId): ProjectTemplate {
  return PROJECT_TEMPLATES.find((template) => template.id === id) ?? PROJECT_TEMPLATES[0]!;
}
