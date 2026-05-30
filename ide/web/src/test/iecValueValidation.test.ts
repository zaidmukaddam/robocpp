import { describe, expect, it } from "vitest";
import { canonicalIecValue, validateIecValue } from "@/lib/iecValueValidation";

describe("iecValueValidation", () => {
  it("accepts BOOL literals", () => {
    expect(validateIecValue("TRUE", "BOOL")).toBeNull();
    expect(validateIecValue("0", "BOOL")).toBeNull();
    expect(validateIecValue("maybe", "BOOL")).toMatch(/BOOL/);
  });

  it("accepts integer and real literals", () => {
    expect(validateIecValue("-12", "INT")).toBeNull();
    expect(validateIecValue("3.14", "REAL")).toBeNull();
  });

  it("canonicalizes BOOL values", () => {
    expect(canonicalIecValue("1", "BOOL")).toBe("TRUE");
    expect(canonicalIecValue("false", "BOOL")).toBe("FALSE");
  });
});
