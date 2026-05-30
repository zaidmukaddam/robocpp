import { Cable, FileText, Plug, Radio } from "lucide-react";
import type { TargetMappingKind } from "@/features/target/targetMapping";

export const MAPPING_KIND_META: Record<
  TargetMappingKind,
  { label: string; short: string; tone: string; icon: typeof FileText; hint: string }
> = {
  file: {
    label: "File I/O",
    short: "FILE",
    tone: "kind-file",
    icon: FileText,
    hint: "Maps a PLC symbol to a relative file under the deploy package."
  },
  modbus: {
    label: "Modbus",
    short: "MB",
    tone: "kind-modbus",
    icon: Plug,
    hint: "Unit, register type, and address in unit:register:address form."
  },
  ethercat: {
    label: "EtherCAT",
    short: "EC",
    tone: "kind-ethercat",
    icon: Cable,
    hint: "PDO index and bit offset for process data."
  },
  ros2: {
    label: "ROS 2",
    short: "ROS",
    tone: "kind-ros2",
    icon: Radio,
    hint: "Topic path published or subscribed at runtime."
  }
};
