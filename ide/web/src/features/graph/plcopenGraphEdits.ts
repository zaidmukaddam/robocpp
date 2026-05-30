function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export function renamePlcopenExpression(xml: string, currentLabel: string, nextLabel: string): string {
  if (!currentLabel || currentLabel === nextLabel) {
    return xml;
  }
  let next = xml.replace(
    new RegExp(`(<expression>)\\s*${escapeRegExp(currentLabel)}\\s*(</expression>)`, "gi"),
    `$1${nextLabel}$2`
  );
  next = next.replace(
    new RegExp(`(<variable\\s+name=")${escapeRegExp(currentLabel)}(")`, "gi"),
    `$1${nextLabel}$2`
  );
  return next;
}

export function connectPlcopenWire(xml: string, sourceLocalId: string, targetLocalId: string): string {
  if (!sourceLocalId || !targetLocalId || sourceLocalId === targetLocalId) {
    return xml;
  }

  const targetPattern = new RegExp(
    `(<(?:block|outVariable)\\s+localId="${escapeRegExp(targetLocalId)}"[^>]*>[\\s\\S]*?)(</(?:block|outVariable)>)`,
    "i"
  );
  const targetMatch = xml.match(targetPattern);
  if (!targetMatch) {
    return xml;
  }

  const targetBody = targetMatch[1] ?? "";
  if (targetBody.includes(`refLocalId="${sourceLocalId}"`)) {
    return xml;
  }

  const connectionSnippet = `<connection refLocalId="${sourceLocalId}" />`;
  let updatedBody = targetBody;

  if (/<connectionPointIn>\s*<\/connectionPointIn>/i.test(updatedBody)) {
    updatedBody = updatedBody.replace(
      /<connectionPointIn>\s*<\/connectionPointIn>/i,
      `<connectionPointIn>${connectionSnippet}</connectionPointIn>`
    );
  } else if (/<connectionPointIn>/i.test(updatedBody)) {
    updatedBody = updatedBody.replace(
      /(<connectionPointIn>)([\s\S]*?)(<\/connectionPointIn>)/i,
      `$1$2${connectionSnippet}$3`
    );
  } else if (/<inputVariables>/i.test(updatedBody)) {
    updatedBody = updatedBody.replace(
      /(<variable[^>]*>)(\s*)(<\/variable>)/i,
      `$1$2<connectionPointIn>${connectionSnippet}</connectionPointIn>$3`
    );
  } else {
    updatedBody = `${updatedBody}\n              <connectionPointIn>${connectionSnippet}</connectionPointIn>`;
  }

  return xml.replace(targetPattern, `${updatedBody}$2`);
}

export function deletePlcopenNode(xml: string, localId: string): string {
  if (!localId) {
    return xml;
  }
  let next = xml.replace(
    new RegExp(
      `\\s*<(?:inVariable|outVariable|block)\\s+localId="${escapeRegExp(localId)}"[^>]*>[\\s\\S]*?</(?:inVariable|outVariable|block)>\\s*`,
      "gi"
    ),
    "\n"
  );
  next = next.replace(new RegExp(`\\s*<connection\\s+refLocalId="${escapeRegExp(localId)}"\\s*/>\\s*`, "gi"), "\n");
  next = next.replace(
    new RegExp(`<connectionPointIn>\\s*<connection\\s+refLocalId="${escapeRegExp(localId)}"\\s*/>\\s*</connectionPointIn>\\s*`, "gi"),
    ""
  );
  return next;
}

export function plcopenMetadataIntact(before: string, after: string): boolean {
  const localIdsBefore = [...before.matchAll(/localId="(\d+)"/g)].map((match) => match[1]!);
  const localIdsAfter = [...after.matchAll(/localId="(\d+)"/g)].map((match) => match[1]!);
  if (localIdsBefore.length === 0) {
    return true;
  }
  return localIdsBefore.every((id) => localIdsAfter.includes(id));
}
