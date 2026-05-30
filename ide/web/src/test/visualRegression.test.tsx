import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { NarrowViewportGate } from "@/components/layout/NarrowViewportGate";
import { GraphPropertyPanel } from "@/features/graph/GraphPropertyPanel";
import type { GraphModel } from "@/features/graph/graphTypes";
import type { WorkspaceFile } from "@/types";

const file: WorkspaceFile = {
  name: "counter.ld",
  languageId: "ld",
  text: "LADDER\nEND_LADDER\n"
};

const model: GraphModel = {
  uri: "counter.ld",
  pous: [
    {
      name: "Counter",
      language: "ld",
      networks: [
        {
          id: "rung-1",
          label: "Rung 1",
          language: "ld",
          nodes: [
            {
              stableId: "n1",
              kind: "contact",
              label: "InputA",
              position: null,
              size: null,
              attributes: {}
            }
          ],
          edges: []
        }
      ],
      sfc: null
    }
  ],
  plcopenLayout: { nodeIds: [], connectorIds: [], branchGeometry: [], actionBlocks: [], vendorAddData: [] }
};

describe("visual regression snapshots", () => {
  it("renders the narrow viewport gate consistently", () => {
    const html = renderToStaticMarkup(<NarrowViewportGate />);
    expect(html).toMatchSnapshot();
  });

  it("renders the graph property panel empty state consistently", () => {
    const html = renderToStaticMarkup(
      <GraphPropertyPanel
        file={file}
        model={model}
        selection={null}
        onApplyRename={() => undefined}
        onApplyProperty={() => undefined}
        onMoveStep={() => undefined}
      />
    );
    expect(html).toMatchSnapshot();
  });
});
