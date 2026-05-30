import type { Layout } from "react-resizable-panels";

export type LayoutPresetId = "text" | "graph" | "simulation" | "deployment";

export type LayoutPreset = {
  id: LayoutPresetId;
  label: string;
  description: string;
  layout: Layout;
  leftOpen: boolean;
  rightOpen: boolean;
  bottomOpen: boolean;
};

export const LAYOUT_PRESETS: LayoutPreset[] = [
  {
    id: "text",
    label: "Text editing",
    description: "Wide editor with inspector for symbols and completions.",
    layout: { explorer: 14, editor: 66, inspector: 20 },
    leftOpen: true,
    rightOpen: true,
    bottomOpen: false
  },
  {
    id: "graph",
    label: "Graph editing",
    description: "Balanced layout for LD, FBD, and SFC diagrams.",
    layout: { explorer: 16, editor: 64, inspector: 20 },
    leftOpen: true,
    rightOpen: false,
    bottomOpen: true
  },
  {
    id: "simulation",
    label: "Simulation",
    description: "Bottom panel open for scan trace and watches.",
    layout: { explorer: 12, editor: 68, inspector: 20 },
    leftOpen: true,
    rightOpen: true,
    bottomOpen: true
  },
  {
    id: "deployment",
    label: "Deployment review",
    description: "Inspector and artifacts focused for target review.",
    layout: { explorer: 18, editor: 52, inspector: 30 },
    leftOpen: true,
    rightOpen: true,
    bottomOpen: true
  }
];

export function layoutPresetById(id: LayoutPresetId): LayoutPreset {
  return LAYOUT_PRESETS.find((preset) => preset.id === id) ?? LAYOUT_PRESETS[0]!;
}
