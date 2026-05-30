import { describe, expect, it } from "vitest";
import { validateIecValue } from "@/lib/iecValueValidation";

describe("editor search helpers", () => {
  it("counts replace-all occurrences", () => {
    const source = "COUNT := COUNT + 1;\nIF COUNT < 10 THEN\nEND_IF;";
    const replaceAll = (text: string, query: string, replacement: string) => {
      const normalized = query.trim();
      if (!normalized || !text.includes(normalized)) {
        return { text, count: 0 };
      }
      const parts = text.split(normalized);
      return { text: parts.join(replacement), count: parts.length - 1 };
    };
    const result = replaceAll(source, "COUNT", "Total");
    expect(result.count).toBe(3);
    expect(result.text).toContain("Total := Total + 1");
  });

  it("rejects invalid BOOL watch values", () => {
    expect(validateIecValue("yes", "BOOL")).not.toBeNull();
    expect(validateIecValue("FALSE", "BOOL")).toBeNull();
  });
});
