import {
  APPLICATION_FOLDER,
  GENERATED_FOLDER,
  PLCOPEN_FOLDER,
  TARGET_FOLDER,
  isGeneratedArtifactPath
} from "@/features/explorer/projectTreePaths";

const FOLDER_ORDER = [APPLICATION_FOLDER, PLCOPEN_FOLDER, TARGET_FOLDER, GENERATED_FOLDER] as const;

export type ExplorerTreeFolder = {
  kind: "folder";
  path: string;
  name: string;
  children: ExplorerTreeFile[];
};

export type ExplorerTreeFile = {
  kind: "file";
  path: string;
  name: string;
  isArtifact: boolean;
  isEditable: boolean;
};

export type ExplorerTreeNode = ExplorerTreeFolder | ExplorerTreeFile;

function compareNames(left: string, right: string): number {
  return left.localeCompare(right, undefined, { sensitivity: "base" });
}

function sortFiles(files: ExplorerTreeFile[]): ExplorerTreeFile[] {
  return [...files].sort((left, right) => compareNames(left.name, right.name));
}

export function buildExplorerTree(paths: string[]): ExplorerTreeFolder[] {
  const filesByFolder = new Map<string, ExplorerTreeFile[]>();

  for (const path of paths) {
    const segments = path.split("/").filter(Boolean);
    if (segments.length < 2) {
      continue;
    }

    const folderName = segments[0]!;
    const fileName = segments.at(-1)!;
    const file: ExplorerTreeFile = {
      kind: "file",
      path,
      name: fileName,
      isArtifact: isGeneratedArtifactPath(path),
      isEditable: !isGeneratedArtifactPath(path)
    };

    const bucket = filesByFolder.get(folderName) ?? [];
    bucket.push(file);
    filesByFolder.set(folderName, bucket);
  }

  return FOLDER_ORDER.filter((folderName) => filesByFolder.has(folderName)).map((folderName) => ({
    kind: "folder",
    path: folderName,
    name: folderName,
    children: sortFiles(filesByFolder.get(folderName) ?? [])
  }));
}

export function filterExplorerTree(tree: ExplorerTreeFolder[], query: string): ExplorerTreeFolder[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return tree;
  }

  return tree.flatMap((folder) => {
    const folderMatches = folder.name.toLowerCase().includes(normalized);
    const matchingChildren = folder.children.filter((file) => file.name.toLowerCase().includes(normalized));
    if (folderMatches) {
      return [folder];
    }
    if (matchingChildren.length === 0) {
      return [];
    }
    return [{ ...folder, children: matchingChildren }];
  });
}

export function folderPathsFromTree(tree: ExplorerTreeFolder[]): string[] {
  return tree.map((folder) => folder.path);
}
