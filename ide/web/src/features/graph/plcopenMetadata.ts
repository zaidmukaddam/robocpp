import type { GraphModel } from "@/features/graph/graphTypes";

export type PlcopenMetadataBundle = {
  nodeIds: string[];
  connectorIds: string[];
  vendorAddData: string[];
  geometry: string[];
};

export function extractPlcopenMetadata(xml: string): PlcopenMetadataBundle {
  const nodeIds = [...xml.matchAll(/localId="(\d+)"/g)].map((match) => match[1]!);
  const connectorIds = [...xml.matchAll(/connector[^>]*localId="(\d+)"/gi)].map((match) => match[1]!);
  const vendorAddData = [...xml.matchAll(/<addData[^>]*>([\s\S]*?)<\/addData>/gi)].map((match) => match[0]!);
  const geometry = [...xml.matchAll(/<position[^>]*x="([^"]+)"[^>]*y="([^"]+)"/gi)].map((match) => match[0]!);
  return { nodeIds, connectorIds, vendorAddData, geometry };
}

export function mergePlcopenMetadata(xml: string, model: GraphModel): string {
  const geometry = model.pous.flatMap((pou) =>
    pou.networks.flatMap((network) =>
      network.nodes
        .filter((node) => node.position)
        .map((node) => `localId:${node.stableId}@${node.position?.x},${node.position?.y}`)
    )
  );
  const bundle = {
    nodeIds: model.plcopenLayout.nodeIds,
    connectorIds: model.plcopenLayout.connectorIds,
    vendorAddData: model.plcopenLayout.vendorAddData,
    geometry:
      geometry.length > 0
        ? geometry
        : model.plcopenLayout.branchGeometry.map((edge) => `${edge.from}->${edge.to}`)
  };
  const marker = "<!-- robocpp-plcopen-metadata";
  const endMarker = "-->";
  const payload = `${marker}\n${JSON.stringify(bundle, null, 2)}\n${endMarker}`;
  if (xml.includes(marker)) {
    return xml.replace(new RegExp(`${marker}[\\s\\S]*?${endMarker}`), payload);
  }
  if (xml.includes("</project>")) {
    return xml.replace("</project>", `${payload}\n</project>`);
  }
  return `${xml}\n${payload}\n`;
}

export function readEmbeddedPlcopenMetadata(xml: string): PlcopenMetadataBundle | null {
  const match = xml.match(/<!-- robocpp-plcopen-metadata\s*([\s\S]*?)\s*-->/);
  if (!match?.[1]) {
    return extractPlcopenMetadata(xml);
  }
  try {
    return JSON.parse(match[1]) as PlcopenMetadataBundle;
  } catch {
    return extractPlcopenMetadata(xml);
  }
}
