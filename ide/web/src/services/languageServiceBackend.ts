/**
 * Browser editor backend for RoboC++ Studio.
 *
 * The web IDE uses the WASM language-service bridge as an LSP-equivalent backend:
 * full-document parse/check on edit, symbol indexes, completions, simulation,
 * generated C artifacts, and structured debug traces. A local TypeScript fallback
 * keeps the shell usable when the WASM package is absent.
 */
export {
  analyzeFile,
  buildCArtifact,
  debugFile,
  engineStatusText,
  getCapabilities,
  getEngineMode,
  graphModelForFile,
  loadGraphForFile,
  runFile,
  validateGraphForFile,
  type EngineMode
} from "@/services/wasmClient";
