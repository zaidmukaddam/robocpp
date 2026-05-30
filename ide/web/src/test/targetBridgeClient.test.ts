import { describe, expect, it } from "vitest";
import { coerceWriteValue, formatIoValue, indexTargetIo, targetBridgeUrl } from "@/features/target/targetBridgeClient";

describe("target bridge client", () => {
  it("normalizes the bridge base url", () => {
    expect(targetBridgeUrl()).toBe("http://127.0.0.1:8787");
    expect(targetBridgeUrl("http://localhost:9000/")).toBe("http://localhost:9000");
  });

  it("indexes live io by symbol", () => {
    const map = indexTargetIo([{ symbol: "Motor", kind: "file", value: true }]);
    expect(map.get("MOTOR")?.value).toBe(true);
  });

  it("coerces write values for bool and numeric io", () => {
    expect(coerceWriteValue("TRUE", false)).toBe(true);
    expect(coerceWriteValue("17", 0)).toBe(17);
  });

  it("formats live io values for the inspector", () => {
    expect(formatIoValue(true)).toBe("TRUE");
    expect(formatIoValue(false)).toBe("FALSE");
    expect(formatIoValue(42)).toBe("42");
    expect(formatIoValue(null)).toBe("—");
  });
});
