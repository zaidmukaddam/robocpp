import { expect, type Page, test } from "@playwright/test";

async function openFile(page: Page, name: string) {
  await page.getByRole("treeitem", { name }).click();
  await expect(page.getByRole("tab", { name: new RegExp(name.replace(".", "\\.")) })).toBeVisible();
}

async function graphLayout(page: Page) {
  return await page.evaluate(() => {
    const viewport = document.querySelector<HTMLElement>(".graph-canvas-viewport");
    const toolbar = document.querySelector<HTMLElement>(".graph-toolbar");
    if (!viewport || !toolbar) {
      throw new Error("Graph canvas or toolbar was not rendered");
    }
    const viewportStyle = getComputedStyle(viewport);
    const toolbarStyle = getComputedStyle(toolbar);
    return {
      viewport: {
        clientWidth: viewport.clientWidth,
        scrollWidth: viewport.scrollWidth,
        clientHeight: viewport.clientHeight,
        scrollHeight: viewport.scrollHeight,
        overflowX: viewportStyle.overflowX,
        overflowY: viewportStyle.overflowY
      },
      toolbar: {
        clientHeight: toolbar.clientHeight,
        clientWidth: toolbar.clientWidth,
        scrollWidth: toolbar.scrollWidth,
        flexWrap: toolbarStyle.flexWrap,
        overflowX: toolbarStyle.overflowX
      }
    };
  });
}

test.describe("graph and generated-project CRUD", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await expect(page).toHaveTitle("RoboC++ Studio");
    await expect(page.locator('[title="Language service: wasm"]')).toContainText("Ready");
  });

  test("keeps oversized LD diagrams navigable without wrapping the graph toolbar", async ({ page }) => {
    await openFile(page, "native_ladder.ld");
    await page.locator(".ld-contact").first().click();

    for (let index = 0; index < 10; index += 1) {
      await page.getByRole("button", { name: "Add contact" }).click();
    }

    const layout = await graphLayout(page);
    expect(layout.viewport.overflowX).toBe("auto");
    expect(layout.viewport.overflowY).toBe("auto");
    expect(layout.viewport.scrollWidth).toBeGreaterThan(layout.viewport.clientWidth);
    expect(layout.toolbar.flexWrap).toBe("nowrap");
    expect(layout.toolbar.overflowX).toBe("auto");
    expect(layout.toolbar.clientHeight).toBeLessThanOrEqual(32);
  });

  test("creates unique SFC step names when adding repeated steps", async ({ page }) => {
    await openFile(page, "sequence.sfc");

    for (let index = 0; index < 4; index += 1) {
      await page.getByRole("button", { name: "Add step" }).click();
    }

    await expect(page.getByText("NewStep", { exact: true }).first()).toBeVisible();
    await expect(page.getByText("NewStep1", { exact: true }).first()).toBeVisible();
    await expect(page.getByText("NewStep2", { exact: true }).first()).toBeVisible();

    const layout = await graphLayout(page);
    expect(layout.viewport.scrollHeight).toBeGreaterThan(layout.viewport.clientHeight);
  });

  test("adds valid FBD networks, enables selected-node actions, and explains invalid connects", async ({ page }) => {
    await openFile(page, "native_fbd.fbd");

    await page.getByRole("button", { name: "Add network" }).click();
    await page.getByRole("button", { name: "Add network" }).click();

    await expect(page.getByText("NewOutput", { exact: true }).first()).toBeVisible();
    await expect(page.getByText("NewInputA", { exact: true }).first()).toBeVisible();
    await expect(page.getByText("NewOutput1", { exact: true }).first()).toBeVisible();
    await expect(page.getByText("NewInputA1", { exact: true }).first()).toBeVisible();
    await expect(page.locator(".graph-validation-banner")).toHaveCount(0);

    await page.getByRole("button", { name: "MotorCmd output" }).click();
    await expect(page.getByText("Selected: MotorCmd")).toBeVisible();
    await expect(page.getByRole("button", { name: "Rename" })).toBeEnabled();
    await expect(page.getByRole("button", { name: "Duplicate" })).toBeEnabled();
    await expect(page.getByRole("button", { name: "Delete" })).toBeEnabled();

    await page.getByRole("button", { name: "Connect" }).click();
    await page.getByRole("button", { name: "MotorCmd output" }).click();
    await expect(page.getByText("Choose two different FBD nodes to create a connection.")).toBeVisible();
  });

  test("does not expose native FBD add actions on PLCopen XML graphs", async ({ page }) => {
    await openFile(page, "plcopen_fbd.xml");

    const toolbar = page.getByRole("toolbar", { name: "Diagram editing" });
    await expect(toolbar.getByRole("button", { name: "Connect" })).toBeVisible();
    await expect(toolbar.getByRole("button", { name: "Add network" })).toHaveCount(0);
    await expect(toolbar.getByRole("button", { name: "Add literal" })).toHaveCount(0);

    const layout = await graphLayout(page);
    expect(layout.viewport.scrollWidth).toBeGreaterThan(layout.viewport.clientWidth);
  });

  test("keeps mapping symbols visible after Build C", async ({ page }) => {
    await openFile(page, "mapping.toml");

    await page.getByRole("button", { name: "Build C" }).click();

    await expect(page.getByText("Deployment checks passed")).toBeVisible();
    await expect(page.getByRole("combobox").filter({ hasText: "Motor" })).toBeVisible();
    await expect(page.getByRole("combobox").filter({ hasText: "Count" })).toBeVisible();

    const blankComboboxes = await page.locator('[role="combobox"]').evaluateAll((nodes) =>
      nodes.filter((node) => !node.textContent?.trim()).length
    );
    expect(blankComboboxes).toBe(0);
  });

  test("opens generated artifacts without leaving the center editor on mapping", async ({ page }) => {
    await openFile(page, "mapping.toml");
    await page.getByRole("button", { name: "Build C" }).click();
    await expect(page.getByText("Deployment checks passed")).toBeVisible();

    await page.getByRole("treeitem", { name: "native_ladder.generated.c" }).click();

    await expect(page.getByRole("navigation", { name: "Project path" })).toContainText("native_ladder.ld");
    await expect(page.getByRole("tab", { name: /native_ladder\.ld/ })).toHaveAttribute("aria-selected", "true");
    await expect(page.getByRole("tab", { name: "Artifacts" })).toHaveAttribute("aria-selected", "true");
    await expect(page.getByRole("region", { name: "Output panel" })).toContainText("native_ladder.generated.c");
  });
});
