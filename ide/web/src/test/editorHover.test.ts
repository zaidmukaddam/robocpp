import { describe, expect, it } from "vitest";
import { hoverAtCursor } from "@/lib/editorHover";

describe("editor hover", () => {
  it("resolves symbols under the cursor", () => {
    const text = "PROGRAM Main\nVAR\n    Count : INT;\nEND_VAR\nEND_PROGRAM\n";
    const cursor = text.indexOf("Count");
    const hover = hoverAtCursor(
      text,
      cursor + 2,
      [
        {
          name: "Count",
          kind: "variable",
          detail: "VAR : INT",
          containerName: "Main",
          range: null
        }
      ],
      [],
      []
    );
    expect(hover?.title).toBe("Count");
    expect(hover?.kind).toBe("variable");
  });
});
