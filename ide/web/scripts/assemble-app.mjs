import fs from "node:fs";
import path from "node:path";

const src = path.resolve("src");
const imports = fs
  .readFileSync(path.join(src, "app/App.imports.tmp"), "utf8")
  .replace('import { createRoot } from "react-dom/client";\n', "")
  .replace('import { CodeEditor, type CodeEditorHandle } from "@/features/editor/CodeEditor";\n', "")
  .replace('import { GraphDiagramView } from "@/features/graph/GraphDiagramView";\n', "")
  .replace('import { TargetMappingEditor } from "@/features/target/TargetMappingEditor";\n', "")
  .replace('import { roundTripHint } from "@/features/graph/graphEdits";\n', "")
  .replace(
    'import { isTargetMappingFile, mappingFileName as defaultMappingFilePath, parseTargetMapping } from "@/features/target/targetMapping";\n',
    'import { isTargetMappingFile, mappingFileName as defaultMappingFilePath, parseTargetMapping } from "@/features/target/targetMapping";\n'
  )
  .replace('import { GeneratedCView, SimulatorTrace } from "@/features/panels/SimulatorPanels";\n', "")
  .replace('import { quickFixesForDiagnostic } from "@/lib/diagnosticQuickFixes";\n', "")
  .replace('import "./styles.css";\n', "")
  .replace('import "./index.css";\n', "")
  .concat(
    'import { PaneHeader } from "@/components/layout/PaneHeader";\n' +
      'import { EditorSurface } from "@/app/EditorSurface";\n' +
      'import { BottomPanel } from "@/features/panels/BottomPanel";\n' +
      'import { bootstrappedApp, nowLabel, type DialogMode, type InspectorTab, type LogEntry, type OutputPanel } from "@/app/types";\n'
  );

const body = fs.readFileSync(path.join(src, "app/App.body.tmp"), "utf8");
fs.writeFileSync(path.join(src, "app/App.tsx"), `${imports}\nexport function App() {\n${body}\n}\n`);

fs.unlinkSync(path.join(src, "app/App.imports.tmp"));
fs.unlinkSync(path.join(src, "app/App.body.tmp"));

console.log("assembled app/App.tsx");
