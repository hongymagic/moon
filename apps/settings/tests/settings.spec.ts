import { test, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import defaults from "./fixtures/defaults.json" with { type: "json" };

type Message = {
  id: number;
  method: string;
  params: { expected_source?: string | null; edits?: unknown[] };
};
type Host = {
  calls: Message[];
  error?: { code: string; message: string; help: string };
  saveGate?: Promise<void>;
  loadGate?: Promise<void>;
  config?: Record<string, unknown>;
};
const original = "# fixture preferences\ntheme='dark'\n";
const personalRule = {
  commands: ["set-floating"],
  matches: [{ process: { equals: "FixtureApp" } }],
};

async function open(
  page: Page,
  overrides: Record<string, unknown> = {},
): Promise<Host> {
  const host: Host = { calls: [] };
  const config = {
    ...structuredClone(defaults),
    ...structuredClone(overrides),
  };
  const snapshot = {
    source: original,
    config,
    overrides,
    defaults,
    schema: {},
  };
  const loaded = {
    snapshot,
    path: "C:\\OwnedTest\\config.toml",
    version: "0.1.0",
    runtime: {
      running: true,
      message: "Connected to the test fixture.",
      stats: { managed_windows: 4 },
    },
  };
  await page.exposeFunction("__moonTestRequest", async (raw: string) => {
    const message = JSON.parse(raw) as Message;
    host.calls.push(structuredClone(message));
    if (message.method === "load") await host.loadGate;
    if (message.method === "save") {
      await host.saveGate;
      if (host.error) return { id: message.id, ok: false, error: host.error };
      return {
        id: message.id,
        ok: true,
        result: {
          data: {
            ...loaded,
            snapshot: {
              ...snapshot,
              config: host.config ?? config,
              source: "# saved fixture\n",
            },
          },
          message: "Saved and applied.",
        },
      };
    }
    return {
      id: message.id,
      ok: true,
      result:
        message.method === "load"
          ? host.config
            ? {
                ...loaded,
                snapshot: {
                  ...snapshot,
                  config: host.config,
                  source: "# externally saved fixture\n",
                },
              }
            : loaded
          : message.method === "status"
            ? loaded.runtime
            : {},
    };
  });
  await page.addInitScript(() => {
    const target = window as unknown as {
      ipc: { postMessage(raw: string): void };
      __moonTestRequest(raw: string): Promise<unknown>;
      moonReceive(reply: unknown): void;
    };
    target.ipc = {
      postMessage: (raw) => {
        void target
          .__moonTestRequest(raw)
          .then((reply) => target.moonReceive(reply));
      },
    };
  });
  await page.route("http://moon.settings/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    const file = path === "/" ? "index.html" : path.slice(1);
    if (!["index.html", "app.js", "styles.css", "moon.svg"].includes(file)) {
      await route.abort();
      return;
    }
    const type = file.endsWith(".js")
      ? "text/javascript"
      : file.endsWith(".css")
        ? "text/css"
        : file.endsWith(".svg")
          ? "image/svg+xml"
          : "text/html";
    await route.fulfill({
      status: 200,
      contentType: type,
      body: await readFile(new URL(`../dist/${file}`, import.meta.url)),
    });
  });
  await page.goto("http://moon.settings/");
  await expect(
    page.getByRole("heading", { name: "General", exact: true }),
  ).toBeVisible();
  return host;
}
async function navigate(page: Page, name: string): Promise<void> {
  await page
    .getByRole("navigation", { name: "Settings pages" })
    .getByRole("button", { name, exact: true })
    .click();
  await expect(page.getByRole("heading", { name, exact: true })).toBeVisible();
}
async function change(page: Page, label: string, value: string): Promise<void> {
  const field = page.getByLabel(label, { exact: true });
  await field.fill(value);
  await field.press("Tab");
}
function saveCalls(host: Host): Message[] {
  return host.calls.filter((call) => call.method === "save");
}

test("sequential rule edits retain commands and every matcher in the submitted personal overlay", async ({
  page,
}) => {
  const host = await open(page, { rules: [structuredClone(personalRule)] });
  await navigate(page, "Rules");
  await change(page, "Commands · one per line", "move-to-workspace 2");
  await change(page, "process value", "ChangedFixture");
  await change(page, "class value", "OwnedWindow");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]?.params).toEqual({
    expected_source: original,
    edits: [
      {
        op: "set",
        path: ["rules"],
        value: [
          {
            commands: ["move-to-workspace 2"],
            matches: [
              {
                process: { equals: "ChangedFixture" },
                class: { equals: "OwnedWindow" },
              },
            ],
          },
        ],
      },
    ],
  });
});

test("workspace rename and icon survive reordering", async ({ page }) => {
  const host = await open(page, {
    workspaces: [{ name: "Work", icon: "W" }, { name: "Play" }],
  });
  await navigate(page, "Workspaces");
  const cards = page.locator(".list-card");
  await cards.nth(0).getByLabel("Name", { exact: true }).fill("Code");
  await cards.nth(0).getByLabel("Name", { exact: true }).press("Tab");
  await cards.nth(0).getByLabel("Icon", { exact: true }).fill("C");
  await cards.nth(0).getByLabel("Icon", { exact: true }).press("Tab");
  await cards
    .nth(0)
    .getByRole("button", { name: /^Move down:/ })
    .click();
  await expect(cards.nth(1).getByLabel("Name", { exact: true })).toHaveValue(
    "Code",
  );
  await expect(cards.nth(1).getByLabel("Icon", { exact: true })).toHaveValue(
    "C",
  );
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]?.params.edits).toEqual([
    {
      op: "set",
      path: ["workspaces"],
      value: [{ name: "Play" }, { name: "Code", icon: "C" }],
    },
  ]);
});

test("clearing an optional workspace field and disabling weather request removals", async ({
  page,
}) => {
  const host = await open(page, {
    workspaces: [{ name: "Work", icon: "W" }],
    weather: { latitude: 10, longitude: 20, "interval-seconds": 900 },
  });
  await navigate(page, "Workspaces");
  await change(page, "Icon", "");
  await navigate(page, "Advanced");
  await page.getByRole("button", { name: "Turn off", exact: true }).click();
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]?.params.edits).toEqual([
    { op: "reset", path: ["workspaces", 0, "icon"] },
    { op: "reset", path: ["weather"] },
  ]);
});

for (const kind of ["validation", "conflict"])
  test(`${kind} save errors retain the draft and exact concurrency token`, async ({
    page,
  }) => {
    const host = await open(page);
    host.error = {
      code: kind,
      message:
        kind === "conflict"
          ? "The configuration changed after these settings were opened."
          : "Inner gap is smaller than the border width.",
      help: "Reload or correct the draft before saving.",
    };
    await change(page, "Inner gap", "24");
    await page
      .getByRole("button", { name: "Save changes", exact: true })
      .click();
    await expect(page.getByRole("alert")).toContainText(host.error.message);
    await expect(page.getByLabel("Inner gap", { exact: true })).toHaveValue(
      "24",
    );
    await expect(
      page.getByRole("button", { name: "Save changes", exact: true }),
    ).toBeEnabled();
    await page.keyboard.press("Control+s");
    await expect.poll(() => saveCalls(host).length).toBe(2);
    expect(saveCalls(host).map((call) => call.params.expected_source)).toEqual([
      original,
      original,
    ]);
  });

test("invalid advanced JSON blocks saving and survives navigation until corrected", async ({
  page,
}) => {
  const host = await open(page);
  await navigate(page, "Advanced");
  await page.getByText("Command aliases", { exact: true }).click();
  const aliases = page.getByLabel("Command aliases", { exact: true });
  await aliases.fill('{"arrange":["focus left"]}');
  await aliases.fill("{");
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toBeDisabled();
  await navigate(page, "General");
  await page.keyboard.press("Control+s");
  expect(saveCalls(host)).toHaveLength(0);
  await navigate(page, "Advanced");
  await page.getByText("Command aliases", { exact: true }).click();
  await expect(aliases).toHaveValue("{");
  await aliases.fill('{"arrange":["focus right"]}');
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]?.params.edits).toEqual([
    { op: "set", path: ["aliases"], value: { arrange: ["focus right"] } },
  ]);
});

test("dirty close can be cancelled or discarded without saving", async ({
  page,
}) => {
  const host = await open(page);
  await change(page, "Outer gap", "48");
  const requestClose = () =>
    page.evaluate(() =>
      (
        window as unknown as { moonCloseRequested(): void }
      ).moonCloseRequested(),
    );
  await requestClose();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Cancel", exact: true }),
  ).toBeFocused();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByLabel("Outer gap", { exact: true })).toHaveValue("48");
  expect(host.calls.filter((call) => call.method === "close")).toHaveLength(0);
  await requestClose();
  await page
    .getByRole("button", { name: "Discard and close", exact: true })
    .click();
  await expect
    .poll(() => host.calls.filter((call) => call.method === "close").length)
    .toBe(1);
  expect(saveCalls(host)).toHaveLength(0);
});

test("saving makes the draft inert and returns to the saved state", async ({
  page,
}) => {
  const host = await open(page);
  let release!: () => void;
  host.saveGate = new Promise<void>((resolve) => {
    release = resolve;
  });
  host.config = {
    ...structuredClone(defaults),
    gaps: { inner: 24, outer: 32 },
  };
  await change(page, "Inner gap", "24");
  await page.keyboard.press("Control+s");
  await expect.poll(() => saveCalls(host).length).toBe(1);
  await expect(page.locator(".page")).toHaveAttribute("inert", "");
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toBeDisabled();
  release();
  await expect(page.locator(".page")).not.toHaveAttribute("inert", "");
  await expect(page.getByLabel("Inner gap", { exact: true })).toHaveValue("24");
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toBeDisabled();
});

test("keyboard search and navigation expose labels, focus and current page", async ({
  page,
}) => {
  await open(page);
  await page.keyboard.press("Control+f");
  await expect(
    page.getByRole("searchbox", { name: "Find a setting" }),
  ).toBeFocused();
  await page.getByRole("searchbox", { name: "Find a setting" }).fill("border");
  await page.getByRole("button", { name: "Window borders Appearance" }).click();
  const heading = page.getByRole("heading", {
    name: "Appearance",
    exact: true,
  });
  await expect(heading).toBeFocused();
  await expect(
    page
      .getByRole("navigation")
      .getByRole("button", { name: "Appearance", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await expect(
    page.getByRole("switch", { name: "Show borders", exact: true }),
  ).toBeChecked();
  await expect(
    page.getByLabel("Border width", { exact: true }),
  ).toHaveAttribute("max", "32");
  await expect(
    page.getByLabel("Corner radius", { exact: true }),
  ).toHaveAttribute("max", "128");
});

test("a first invalid JSON edit can be discarded without any previously valid change", async ({
  page,
}) => {
  const host = await open(page);
  await navigate(page, "Advanced");
  await page.getByText("Command aliases", { exact: true }).click();
  await page.getByLabel("Command aliases", { exact: true }).fill("{");
  await expect(
    page.getByRole("button", { name: "Discard", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Discard", exact: true }).click();
  await page
    .getByRole("button", { name: "Discard changes", exact: true })
    .click();
  await page.getByText("Command aliases", { exact: true }).click();
  await expect(page.getByLabel("Command aliases", { exact: true })).toHaveValue(
    "{}",
  );
  await expect(
    page.getByRole("button", { name: "Discard", exact: true }),
  ).toBeDisabled();
  expect(saveCalls(host)).toHaveLength(0);
});

test("conflict reload requires confirmation and replaces the draft only after acceptance", async ({
  page,
}) => {
  const host = await open(page);
  host.error = {
    code: "conflict",
    message: "The configuration changed after these settings were opened.",
    help: "Reload settings before saving.",
  };
  await change(page, "Inner gap", "24");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(host.error.message);
  host.config = {
    ...structuredClone(defaults),
    gaps: { inner: 48, outer: 32 },
  };
  await page.getByRole("button", { name: "Reload", exact: true }).click();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByLabel("Inner gap", { exact: true })).toHaveValue("24");
  expect(host.calls.filter((call) => call.method === "load")).toHaveLength(1);
  await page.getByRole("button", { name: "Reload", exact: true }).click();
  await page
    .getByRole("button", { name: "Reload settings", exact: true })
    .click();
  await expect(page.getByLabel("Inner gap", { exact: true })).toHaveValue("48");
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toBeDisabled();
  expect(host.calls.filter((call) => call.method === "load")).toHaveLength(2);
});

test("workspace animation can be disabled without changing window layouts", async ({
  page,
}) => {
  const host = await open(page);
  const setting = page.getByRole("switch", {
    name: "Slide between workspaces",
    exact: true,
  });
  await expect(setting).toBeChecked();
  await setting.uncheck();
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]?.params.edits).toEqual([
    { op: "set", path: ["workspace-animation"], value: false },
  ]);
});

test("reload keeps the draft inert until the requested file has arrived", async ({
  page,
}) => {
  const host = await open(page);
  let release!: () => void;
  await change(page, "Inner gap", "24");
  host.loadGate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.getByRole("button", { name: "Reload", exact: true }).click();
  await page
    .getByRole("button", { name: "Reload settings", exact: true })
    .click();
  await expect(page.locator(".page")).toHaveAttribute("inert", "");
  await expect(
    page.getByRole("button", { name: "Reload", exact: true }),
  ).toBeDisabled();
  await expect(page.getByLabel("Inner gap", { exact: true })).toHaveValue("24");
  release();
  await expect(page.locator(".page")).not.toHaveAttribute("inert", "");
  await expect(page.getByLabel("Inner gap", { exact: true })).toHaveValue("16");
});
