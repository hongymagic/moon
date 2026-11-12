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
  loadError?: { code: string; message: string; help: string };
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
  initial: Pick<Host, "loadGate" | "loadError"> & { waitForUi?: boolean } = {},
): Promise<Host> {
  const host: Host = {
    calls: [],
    loadGate: initial.loadGate,
    loadError: initial.loadError,
  };
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
    if (message.method === "load") {
      await host.loadGate;
      if (host.loadError)
        return { id: message.id, ok: false, error: host.loadError };
    }
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
      __moonReady: { id: number; heading: string | null; buttons: string[] }[];
    };
    target.__moonReady = [];
    target.ipc = {
      postMessage: (raw) => {
        const message = JSON.parse(raw) as { id: number; method: string };
        if (message.method === "ready") {
          target.__moonReady.push({
            id: message.id,
            heading: document.querySelector("h1")?.textContent ?? null,
            buttons: Array.from(
              document.querySelectorAll("button"),
              (button) => button.textContent ?? "",
            ),
          });
          return;
        }
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
  if (initial.waitForUi !== false)
    await expect(
      page.getByRole("heading", { name: "General", exact: true }),
    ).toBeVisible();
  return host;
}

async function readyNotifications(page: Page) {
  return page.evaluate(
    () =>
      (
        window as unknown as {
          __moonReady: {
            id: number;
            heading: string | null;
            buttons: string[];
          }[];
        }
      ).__moonReady,
  );
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

test("startup: ready follows the loaded interface and is not a host request", async ({
  page,
}) => {
  const host = await open(page);
  const notices = await readyNotifications(page);
  expect(notices).toHaveLength(1);
  expect(notices[0]).toMatchObject({ id: 0, heading: "General" });
  expect(notices[0]!.buttons).toContain("Save changes");
  expect(host.calls.map((call) => call.method)).toEqual(["load"]);
  await navigate(page, "Appearance");
  expect(await readyNotifications(page)).toHaveLength(1);
});

test("startup: ready remains unsent while initial configuration is pending", async ({
  page,
}) => {
  let release!: () => void;
  const loadGate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const host = await open(page, {}, { loadGate, waitForUi: false });
  await expect.poll(() => host.calls.length).toBe(1);
  expect(await readyNotifications(page)).toEqual([]);
  await expect(
    page.getByRole("heading", { name: "General", exact: true }),
  ).toHaveCount(0);
  release();
  await expect(
    page.getByRole("heading", { name: "General", exact: true }),
  ).toBeVisible();
  expect(await readyNotifications(page)).toHaveLength(1);
});

test("startup: ready follows a usable error page and retry never resends it", async ({
  page,
}) => {
  const host = await open(
    page,
    {},
    {
      loadError: {
        code: "fixture",
        message: "Configuration is unavailable.",
        help: "Try again.",
      },
      waitForUi: false,
    },
  );
  await expect(
    page.getByRole("heading", {
      name: "Let’s get Settings ready",
      exact: true,
    }),
  ).toBeVisible();
  const notices = await readyNotifications(page);
  expect(notices).toHaveLength(1);
  expect(notices[0]).toEqual({
    id: 0,
    heading: "Let’s get Settings ready",
    buttons: ["Try again", "Open configuration"],
  });
  await page.getByRole("button", { name: "Try again", exact: true }).click();
  await expect.poll(() => host.calls.length).toBe(2);
  expect(await readyNotifications(page)).toHaveLength(1);
  host.loadError = undefined;
  await page.getByRole("button", { name: "Try again", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "General", exact: true }),
  ).toBeVisible();
  expect(await readyNotifications(page)).toHaveLength(1);
});

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
  await expect(aliases).toBeVisible();
  await expect(aliases).toHaveAttribute("aria-invalid", "true");
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

test("polish: confirmation owns shortcuts and repeated close requests", async ({
  page,
}) => {
  const host = await open(page);
  const field = page.getByLabel("Inner gap", { exact: true });
  await field.fill("24");
  const close = () =>
    page.evaluate(() =>
      (
        window as unknown as { moonCloseRequested(): void }
      ).moonCloseRequested(),
    );
  await close();
  const cancel = page.getByRole("button", { name: "Cancel", exact: true });
  await expect(cancel).toBeFocused();
  await page.keyboard.press("Control+s");
  await page.keyboard.press("Control+f");
  expect(saveCalls(host)).toHaveLength(0);
  await expect(cancel).toBeFocused();
  await close();
  await expect(page.getByRole("dialog")).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(field).toBeFocused();
  expect(host.calls.filter((call) => call.method === "close")).toHaveLength(0);
});

test("polish: save failure receives focus after the editor becomes available", async ({
  page,
}) => {
  const host = await open(page);
  host.error = {
    code: "conflict",
    message: "Another editor changed this file.",
    help: "Reload before saving.",
  };
  await change(page, "Inner gap", "24");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText(host.error.message);
  await expect(page.getByRole("alert")).toBeFocused();
  await navigate(page, "Appearance");
  await expect(page.getByRole("alert")).toContainText(host.error.message);
});

test("polish: review errors returns to an invalid field with its explanation", async ({
  page,
}) => {
  await open(page);
  await page.getByLabel("Inner gap", { exact: true }).fill("");
  await navigate(page, "Appearance");
  await page
    .getByRole("button", { name: "Review errors", exact: true })
    .click();
  const field = page.getByLabel("Inner gap", { exact: true });
  await expect(field).toBeFocused();
  await expect(field).toHaveAttribute("aria-invalid", "true");
  await expect(field).toHaveValue("");
  expect(
    await field.evaluate(
      (node) =>
        document.getElementById(node.getAttribute("aria-describedby")!)
          ?.textContent,
    ),
  ).toMatch(/fill|value|number/i);
  await field.fill("24");
  await expect(
    page.getByRole("button", { name: "Review errors", exact: true }),
  ).toHaveCount(0);
});

test("polish: resetting a section clears its invalid draft fields", async ({
  page,
}) => {
  await open(page);
  await navigate(page, "Appearance");
  await page.getByLabel("Border width", { exact: true }).fill("");
  await page
    .getByRole("button", { name: "Reset borders", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Restore defaults", exact: true })
    .click();
  await expect(page.getByLabel("Border width", { exact: true })).toHaveValue(
    String(defaults.borders.width),
  );
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toBeEnabled();
});

test("polish: JSON review reveals the invalid editor and its message", async ({
  page,
}) => {
  await open(page);
  await navigate(page, "Advanced");
  await page.getByText("Command aliases", { exact: true }).click();
  await page.getByLabel("Command aliases", { exact: true }).fill("{");
  await navigate(page, "General");
  await page
    .getByRole("button", { name: "Review errors", exact: true })
    .click();
  const input = page.getByLabel("Command aliases", { exact: true });
  await expect(input).toBeFocused();
  await expect(input).toHaveAttribute("aria-invalid", "true");
  await expect(
    page.getByText(
      "Enter valid JSON. This field has not been added to your changes.",
      { exact: true },
    ),
  ).toBeVisible();
});

test("polish: keyboard save returns focus to the edited field", async ({
  page,
}) => {
  const host = await open(page);
  host.config = {
    ...structuredClone(defaults),
    gaps: { inner: 24, outer: 32 },
  };
  const field = page.getByLabel("Inner gap", { exact: true });
  await field.fill("24");
  await page.keyboard.press("Control+s");
  await expect(
    page.getByText("All changes saved", { exact: true }),
  ).toBeVisible();
  await expect(field).toBeFocused();
  await expect(field).toHaveValue("24");
});

test("polish: preview shows both gradient colors and follows system theme changes", async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: "light" });
  await open(page);
  await navigate(page, "Appearance");
  await page.getByLabel("Focused color", { exact: true }).fill("#123456");
  await page.getByLabel("Gradient end", { exact: true }).fill("#abcdef");
  const window = page.locator(".mini-window").first();
  await expect(window).toHaveCSS(
    "background-image",
    /rgb\(18, 52, 86\).*rgb\(171, 205, 239\)/,
  );
  const system = page
    .getByRole("group", { name: "Desktop theme" })
    .getByRole("button", { name: "System", exact: true });
  await system.click();
  await expect(system).toBeFocused();
  const light = await window.evaluate(
    (node) => getComputedStyle(node).backgroundImage,
  );
  await page.emulateMedia({ colorScheme: "dark" });
  await expect
    .poll(() =>
      window.evaluate((node) => getComputedStyle(node).backgroundImage),
    )
    .not.toBe(light);
  await expect(system).toHaveAttribute("aria-pressed", "true");
});

for (const width of [320, 370, 740]) {
  test(`polish: all pages and save actions fit a ${width}px window`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: width === 740 ? 540 : 720 });
    await open(page);
    for (const name of [
      "General",
      "Appearance",
      "Workspaces",
      "Shortcuts",
      "Rules",
      "Advanced",
      "About",
    ]) {
      await navigate(page, name);
      const layout = await page.evaluate(() => {
        const scroll = document.querySelector<HTMLElement>(".scroll")!;
        const buttons = Array.from(
          document.querySelectorAll<HTMLElement>(".savebar button"),
        );
        return {
          bodyOverflow: document.documentElement.scrollWidth > innerWidth,
          pageOverflow: scroll.scrollWidth > scroll.clientWidth,
          navigationFits: Array.from(
            document.querySelectorAll<HTMLElement>(".nav button"),
          ).every((button) => button.scrollWidth <= button.clientWidth),
          actionsAligned: buttons.every(
            (button) =>
              Math.abs(
                button.getBoundingClientRect().top -
                  buttons[0]!.getBoundingClientRect().top,
              ) < 1,
          ),
          actionsFit: buttons.every((button) => {
            const r = button.getBoundingClientRect();
            return (
              r.left >= 0 && r.right <= innerWidth && r.bottom <= innerHeight
            );
          }),
        };
      });
      expect(layout, name).toEqual({
        bodyOverflow: false,
        pageOverflow: false,
        navigationFits: true,
        actionsAligned: true,
        actionsFit: true,
      });
    }
    await navigate(page, "Appearance");
    await page.screenshot({
      animations: "disabled",
      path: `../../target/settings-polish/appearance-${width}.png`,
    });
  });
}

for (const colorScheme of ["light", "dark"] as const) {
  test(`polish: ${colorScheme} appearance retains readable controls`, async ({
    page,
  }) => {
    await page.emulateMedia({ colorScheme });
    await open(page);
    await navigate(page, "Appearance");
    await expect(
      page.getByRole("heading", { name: "Desktop theme", exact: true }),
    ).toBeVisible();
    await expect(
      page.getByText(
        "Choose colors for the bar and window borders. Settings follows your Windows appearance.",
      ),
    ).toBeVisible();
    await page.screenshot({
      animations: "disabled",
      path: `../../target/settings-polish/appearance-${colorScheme}.png`,
    });
  });
}

test("polish: high contrast and reduced motion retain operable controls", async ({
  page,
}) => {
  await page.emulateMedia({
    forcedColors: "active",
    reducedMotion: "reduce",
    colorScheme: "dark",
  });
  await open(page);
  await navigate(page, "Appearance");
  await expect(page.locator(".preview")).toBeHidden();
  await expect(page.locator(".page")).toHaveCSS("animation-name", "none");
  const show = page.getByRole("switch", { name: "Show borders", exact: true });
  await show.focus();
  await page.keyboard.press("Space");
  await expect(show).not.toBeChecked();
  await expect(show).toBeFocused();
  await expect(show).toHaveCSS("transition-duration", "0s");
  await page.screenshot({
    animations: "disabled",
    path: "../../target/settings-polish/appearance-high-contrast.png",
  });
});

test("polish: navigating during save does not focus an unrelated field", async ({
  page,
}) => {
  const host = await open(page);
  let release!: () => void;
  host.saveGate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.getByLabel("Inner gap", { exact: true }).fill("24");
  await page.keyboard.press("Control+s");
  await expect.poll(() => saveCalls(host).length).toBe(1);
  await page
    .getByRole("navigation")
    .getByRole("button", { name: "Appearance", exact: true })
    .click();
  release();
  await expect(
    page.getByRole("heading", { name: "Appearance", exact: true }),
  ).toBeFocused();
});

test("polish: a 200 percent reflow viewport keeps navigation and saving reachable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 370, height: 270 });
  const host = await open(page);
  const gap = page.getByLabel("Inner gap", { exact: true });
  await gap.focus();
  await expect(gap).toBeInViewport();
  await gap.fill("24");
  await page.screenshot({
    animations: "disabled",
    path: "../../target/settings-polish/reflow-200-percent-field.png",
  });
  const save = page.getByRole("button", { name: "Save changes", exact: true });
  await save.focus();
  const bounds = await save.boundingBox();
  expect(bounds).not.toBeNull();
  expect(bounds!.y).toBeGreaterThanOrEqual(0);
  expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(270);
  await page.screenshot({
    animations: "disabled",
    path: "../../target/settings-polish/reflow-200-percent-save.png",
  });
  await page.keyboard.press("Enter");
  await expect.poll(() => saveCalls(host).length).toBe(1);
  for (const name of [
    "Appearance",
    "Workspaces",
    "Shortcuts",
    "Rules",
    "Advanced",
    "About",
    "General",
  ]) {
    const destination = page
      .getByRole("navigation")
      .getByRole("button", { name, exact: true });
    await destination.focus();
    await expect(destination).toBeInViewport();
    await page.keyboard.press("Enter");
    await expect(
      page.getByRole("heading", { name, exact: true }),
    ).toBeFocused();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
  }
});
