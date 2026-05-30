import { readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const distAssets = join(process.cwd(), "dist", "assets");

const budgets = [
  { pattern: /\.wasm$/i, label: "WASM language service", maxBytes: 1_650_000 },
  { pattern: /vendor-.*\.js$/i, label: "Vendor JS chunk", maxBytes: 900_000 },
  { pattern: /editor-.*\.js$/i, label: "Editor JS chunk", maxBytes: 500_000 },
  { pattern: /index-.*\.js$/i, label: "App JS chunk", maxBytes: 350_000 },
  { pattern: /index-.*\.css$/i, label: "App CSS bundle", maxBytes: 130_000 }
];

const files = readdirSync(distAssets);
const failures = [];

for (const { pattern, label, maxBytes } of budgets) {
  const match = files.find((file) => pattern.test(file));
  if (!match) {
    failures.push(`${label}: matching asset not found in dist/assets`);
    continue;
  }
  const bytes = statSync(join(distAssets, match)).size;
  if (bytes > maxBytes) {
    failures.push(`${label} (${match}): ${bytes} bytes exceeds budget ${maxBytes}`);
  } else {
    console.log(`OK ${label}: ${bytes} / ${maxBytes} bytes (${match})`);
  }
}

if (failures.length > 0) {
  console.error("\nBundle size gate failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log("\nBundle size gate passed.");
