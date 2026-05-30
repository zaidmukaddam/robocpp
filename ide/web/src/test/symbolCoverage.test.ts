import { describe, expect, it } from "vitest";
import { buildSymbolCoverage } from "@/features/target/symbolCoverage";
import type { GeneratedCMetadata } from "@/types";

const metadata: GeneratedCMetadata = {
  filenameHint: "counter.c",
  scanEntrypoints: [{ name: "scan", signature: "int scan(void)" }],
  stateLayout: [{ name: "Count", typeName: "INT", retained: false, sourceName: "Count" }],
  ioSymbols: [
    { name: "Motor", location: "%QX0.0", direction: "output", typeName: "BOOL" },
    { name: "Sensor", location: "%IX0.0", direction: "input", typeName: "BOOL" }
  ],
  accessPaths: [],
  retainedFields: ["Count"],
  targetHooks: ["target_init"],
  debugSymbols: []
};

describe("symbol coverage", () => {
  it("flags unmapped generated symbols", () => {
    const rows = buildSymbolCoverage(metadata, [
      { id: "1", kind: "file", symbol: "Motor", target: "io/motor.txt", encoding: "bool" }
    ]);
    expect(rows.some((row) => row.symbol === "Sensor" && row.status === "unmapped")).toBe(true);
    expect(rows.some((row) => row.symbol === "Motor" && row.status === "mapped")).toBe(true);
  });

  it("flags incompatible mapping symbols", () => {
    const rows = buildSymbolCoverage(metadata, [
      { id: "1", kind: "modbus", symbol: "Missing", target: "1:coil:0" }
    ]);
    expect(rows.some((row) => row.symbol === "Missing" && row.status === "incompatible")).toBe(true);
  });
});
