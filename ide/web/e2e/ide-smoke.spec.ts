import { expect, test } from "@playwright/test";

test("loads the sample IDE workspace and runs the active program", async ({ page }) => {
  const consoleIssues: string[] = [];
  page.on("console", (message) => {
    if (["error", "warning"].includes(message.type())) {
      consoleIssues.push(`${message.type()}: ${message.text()}`);
    }
  });
  page.on("pageerror", (error) => {
    consoleIssues.push(`pageerror: ${error.message}`);
  });

  await page.goto("/");

  await expect(page).toHaveTitle("RoboC++ Studio");
  await expect(page.getByRole("toolbar", { name: "Studio actions" })).toBeVisible();
  await expect(page.getByRole("treeitem", { name: "counter.st" })).toBeVisible();
  await expect(page.locator('[title="Language service: wasm"]')).toContainText("Ready");
  await expect(page.getByRole("tab", { name: /Symbols 3/ })).toBeVisible();
  await expect(page.getByRole("button", { name: "program Counter PROGRAM L1" })).toBeVisible();

  await page.getByRole("button", { name: "Run simulation" }).click();

  const outputPanel = page.getByRole("region", { name: "Output panel" });
  await expect(page.getByRole("tab", { name: "Scan Trace" })).toHaveAttribute("aria-selected", "true");
  await expect(outputPanel).toContainText("Cycle 0");
  await expect(outputPanel).toContainText("COUNT=1");
  await expect(outputPanel).toContainText("scan complete");
  await expect(page.locator('[title="Language service: wasm"]')).toContainText("Ready");
  await expect(page.locator('[title="Simulation state"]')).toContainText("Done");

  expect(consoleIssues).toEqual([]);
});
