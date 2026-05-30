export function addOpenTab(openTabs: string[], fileName: string): string[] {
  if (openTabs.includes(fileName)) {
    return openTabs;
  }
  return [...openTabs, fileName];
}

export function removeOpenTab(openTabs: string[], fileName: string): string[] {
  return openTabs.filter((entry) => entry !== fileName);
}

export function renameOpenTab(openTabs: string[], oldName: string, newName: string): string[] {
  return openTabs.map((entry) => (entry === oldName ? newName : entry));
}

export function activeFileAfterClose(openTabs: string[], closedName: string, activeName: string): string {
  if (activeName !== closedName) {
    return activeName;
  }

  const index = openTabs.indexOf(closedName);
  const remaining = removeOpenTab(openTabs, closedName);
  if (remaining.length === 0) {
    return activeName;
  }

  const nextIndex = Math.min(Math.max(index, 0), remaining.length - 1);
  return remaining[nextIndex] ?? remaining[0] ?? activeName;
}

export function syncOpenTabsWithProject(openTabs: string[], projectFileNames: string[]): string[] {
  const projectFiles = new Set(projectFileNames);
  const synced = openTabs.filter((name) => projectFiles.has(name));
  if (synced.length > 0) {
    return synced;
  }
  return projectFileNames[0] ? [projectFileNames[0]] : [];
}
