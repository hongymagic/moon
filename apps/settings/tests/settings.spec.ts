import { test, expect, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import defaults from "./fixtures/defaults.json" with { type: "json" };

type Message = {
  id: number;
  method: string;
  params: {
    expected_source?: string | null;
    edits?: unknown[];
    [key: string]: unknown;
  };
};
type Host = {
  startup: { available: boolean; enabled: boolean; message: string };
  startupGate?: Promise<void>;
  locationGate?: Promise<void>;
  location: { latitude: number; longitude: number; label: string };
  locations: {
    id: string;
    latitude: number;
    longitude: number;
    label: string;
  }[];
  desktop: ReturnType<typeof desktopFixture>;
  library: ReturnType<typeof libraryFixture>;
  methodErrors: Record<string, { code: string; message: string; help: string }>;
  libraryEdits?: unknown[];
  importDocument?: string | null;
  fileGate?: Promise<void>;
  desktopGate?: Promise<void>;
  calls: Message[];
  error?: { code: string; message: string; help: string };
  loadError?: { code: string; message: string; help: string };
  saveGate?: Promise<void>;
  loadGate?: Promise<void>;
  config?: Record<string, unknown>;
  shortcuts?: {
    hook_enabled: boolean;
    mode: string;
    paused: boolean;
    emergency: string;
    bindings: { chord: string; commands: string[] }[];
  };
  shortcutsError?: { code: string; message: string; help: string };
  overview?: {
    managed_windows: number;
    active_workspace: string | null;
    workspaces: { name: string; windows: number; active: boolean }[];
  };
  application?: { state: string; message: string };
};
function desktopFixture() {
  const bounds = { x: 0, y: 0, width: 1920, height: 1080 };
  const window = (
    id: string,
    process: string,
    title: string,
    workspace: string,
    x: number,
  ) => ({
    id,
    process_id: "4000",
    process_birth: "134391583249052813",
    title,
    process,
    class: "FixtureWindow",
    workspace,
    monitor_id: "3229624234",
    rect: { x, y: 0, width: 960, height: 1080 },
    state: {
      focused: id === "9007199254740993",
      visible: workspace === "1",
      floating: false,
      fullscreen: false,
      suspended: false,
    },
    rules: [
      {
        index: 2,
        commands: ["move-to-workspace 1"],
        matches: [
          {
            index: 0,
            matched: true,
            conditions: [
              {
                field: "process",
                operator: "equals",
                value: process,
                matched: true,
              },
            ],
          },
        ],
      },
    ],
  });
  return {
    generation: "4000:134391583249052813",
    revision: "12345678901234567890",
    paused: false,
    session_suspended: false,
    total_windows: 3,
    truncated: false,
    rule_evaluation: "current-metadata",
    active_workspace: "1",
    undo: {
      available: true,
      label: "Undo resize",
      reason: null as string | null,
    },
    workspaces: [
      { name: "1", monitor_id: "3229624234", active: true, windows: 2, bounds },
      {
        name: "2",
        monitor_id: "3229624234",
        active: false,
        windows: 1,
        bounds,
      },
      {
        name: "3",
        monitor_id: "3229624234",
        active: false,
        windows: 0,
        bounds,
      },
    ],
    windows: [
      window("9007199254740993", "Code", "Project notes", "1", 0),
      window("9007199254740994", "Arc", "Research browser", "1", 960),
      window("9007199254740995", "Code", "Second project", "2", 0),
    ],
  };
}
function libraryFixture() {
  return {
    source: "library-v1" as string | null,
    entries: ["Linen", "Slate", "Forest"].map((name) => ({
      kind: "theme",
      name,
      builtin: true,
      values: {
        theme: "system",
        palette: {
          terracotta: "#4d7056",
          base: "#202020",
          surface0: "#303030",
          text: "#ffffff",
        },
        "light-palette": { terracotta: "#4d7056" },
        bar: { background: "base" },
      } as Record<string, unknown>,
    })),
  };
}

const original = "# fixture preferences\ntheme='dark'\n";
const personalRule = {
  commands: ["set-floating"],
  matches: [{ process: { equals: "FixtureApp" } }],
};

async function open(
  page: Page,
  overrides: Record<string, unknown> = {},
  initial: Pick<
    Host,
    "loadGate" | "loadError" | "shortcuts" | "shortcutsError"
  > & {
    waitForUi?: boolean;
    startPage?: "General" | "Shortcuts" | "Overview";
  } = {},
): Promise<Host> {
  const host: Host = {
    startup: {
      available: true,
      enabled: true,
      message: "Moon launches when you sign in.",
    },
    location: {
      latitude: -33.87,
      longitude: 151.21,
      label: "Current location",
    },
    locations: [
      {
        id: "1",
        latitude: -33.87,
        longitude: 151.21,
        label: "Sydney, New South Wales, Australia",
      },
      {
        id: "2",
        latitude: 46.14,
        longitude: -60.18,
        label: "Sydney, Nova Scotia, Canada",
      },
    ],
    desktop: desktopFixture(),
    library: libraryFixture(),
    methodErrors: {},
    calls: [],
    loadGate: initial.loadGate,
    loadError: initial.loadError,
    shortcutsError: initial.shortcutsError,
    shortcuts: initial.shortcuts ?? {
      hook_enabled: true,
      mode: "default",
      paused: false,
      emergency: "ctrl+alt+shift+f12",
      bindings: [
        { chord: "alt+h", commands: ["focus left"] },
        { chord: "alt+2", commands: ["workspace 2"] },
        { chord: "alt+shift+2", commands: ["move-to-workspace 2 --follow"] },
      ],
    },
    overview: {
      managed_windows: 4,
      active_workspace: "1",
      workspaces: [
        { name: "1", windows: 2, active: true },
        { name: "2", windows: 2, active: false },
        { name: "3", windows: 0, active: false },
      ],
    },
    application: {
      state: "loaded",
      message: "Saved and loaded by Moon.",
    },
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
          message: "Saved and loaded by Moon.",
        },
      };
    }
    if (host.methodErrors[message.method])
      return {
        id: message.id,
        ok: false,
        error: host.methodErrors[message.method],
      };
    if (message.method === "startup-status")
      return { id: message.id, ok: true, result: host.startup };
    if (message.method === "startup-set") {
      await host.startupGate;
      host.startup = {
        ...host.startup,
        enabled: message.params.enabled as boolean,
      };
      return { id: message.id, ok: true, result: host.startup };
    }
    if (message.method === "location-current") {
      await host.locationGate;
      return { id: message.id, ok: true, result: host.location };
    }
    if (message.method === "location-search")
      return { id: message.id, ok: true, result: { results: host.locations } };
    if (message.method === "library-import-file") {
      await host.fileGate;
      return {
        id: message.id,
        ok: true,
        result: host.importDocument ? { document: host.importDocument } : null,
      };
    }
    if (message.method === "library-export-file") {
      await host.fileGate;
      return { id: message.id, ok: true, result: { saved: true } };
    }
    if (message.method === "desktop") {
      await host.desktopGate;
      return { id: message.id, ok: true, result: host.desktop };
    }
    if (message.method === "previews")
      return {
        id: message.id,
        ok: true,
        result: { shown: [], unavailable: [] },
      };
    if (message.method === "library-list")
      return { id: message.id, ok: true, result: host.library };
    if (message.method === "library-apply")
      return {
        id: message.id,
        ok: true,
        result: {
          edits: host.libraryEdits ?? [
            { op: "set", path: ["theme"], value: "light" },
            { op: "set", path: ["borders", "focused"], value: "#4d7056" },
          ],
        },
      };
    if (message.method === "library-export")
      return {
        id: message.id,
        ok: true,
        result: {
          document: JSON.stringify({
            version: 1,
            kind: message.params.kind,
            name: message.params.name,
            values: { theme: "system" },
          }),
        },
      };
    if (
      [
        "library-save",
        "library-rename",
        "library-delete",
        "library-import",
      ].includes(message.method)
    ) {
      if (message.params.expected_source !== host.library.source)
        return {
          id: message.id,
          ok: false,
          error: {
            code: "conflict",
            message: "The library changed. Refresh it before saving again.",
            help: "Your desktop configuration was not changed.",
          },
        };
      const imported =
        message.method === "library-import"
          ? (JSON.parse(String(message.params.document)) as {
              kind: string;
              name: string;
              values: Record<string, unknown>;
            })
          : undefined;
      const kind = imported?.kind ?? String(message.params.kind);
      const name = imported?.name ?? String(message.params.name);
      if (message.method === "library-delete")
        host.library.entries = host.library.entries.filter(
          (entry) => entry.kind !== kind || entry.name !== name,
        );
      else if (message.method === "library-rename")
        host.library.entries = host.library.entries.map((entry) =>
          entry.kind === kind && entry.name === name
            ? { ...entry, name: String(message.params.new_name) }
            : entry,
        );
      else {
        host.library.entries = host.library.entries.filter(
          (entry) => entry.kind !== kind || entry.name !== name,
        );
        host.library.entries.push({
          kind,
          name,
          builtin: false,
          values: imported?.values ?? {
            theme: "dark",
            workspaces: defaults.workspaces,
          },
        });
      }
      host.library.source = `library-v${host.calls.length}`;
      return { id: message.id, ok: true, result: host.library };
    }
    if (message.method === "shortcuts" && host.shortcutsError)
      return { id: message.id, ok: false, error: host.shortcutsError };
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
            : message.method === "shortcuts"
              ? host.shortcuts
              : message.method === "overview"
                ? host.overview
                : message.method === "application-status"
                  ? host.application
                  : {},
    };
  });
  await page.addInitScript((startPage) => {
    const target = window as unknown as {
      moonStartPage: "General" | "Shortcuts" | "Overview";
      ipc: { postMessage(raw: string): void };
      __moonTestRequest(raw: string): Promise<unknown>;
      moonReceive(reply: unknown): void;
      __moonReady: { id: number; heading: string | null; buttons: string[] }[];
    };
    target.moonStartPage = startPage;
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
  }, initial.startPage ?? "General");
  await page.route("http://moon.settings/**", async (route) => {
    const path = new URL(route.request().url()).pathname;
    const file = path === "/" ? "index.html" : path.slice(1);
    if (
      ![
        "index.html",
        "app.js",
        "styles.css",
        "moon.svg",
        "moon-symbols.ttf",
      ].includes(file)
    ) {
      await route.abort();
      return;
    }
    const type = file.endsWith(".ttf")
      ? "font/ttf"
      : file.endsWith(".js")
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
      page.getByRole("heading", {
        name: initial.startPage ?? "General",
        exact: true,
      }),
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
  expect(host.calls.map((call) => call.method)).toEqual([
    "load",
    "overview",
    "startup-status",
  ]);
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
    buttons: ["Try again", "Show current shortcuts", "Open configuration"],
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
  await page.getByText("Advanced commands", { exact: true }).click();
  await change(page, "Commands · one per line", "move-to-workspace 2");
  await page
    .getByText("Match 1 · when these conditions match", { exact: true })
    .click();
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

test("game settings save a selected executable, per-app overrides and shortcut passthrough", async ({
  page,
}) => {
  const personal = structuredClone(personalRule);
  const host = await open(page, { rules: [personal] });
  await navigate(page, "General");
  await page.getByLabel("Detect fullscreen games", { exact: true }).uncheck();
  await page.getByLabel("Pass every key to the game", { exact: true }).check();
  await page.getByRole("button", { name: "Manage game rules" }).click();
  await page.getByLabel("Game mode", { exact: true }).selectOption("never");
  await page.getByRole("button", { name: "Add game", exact: true }).click();
  const game = page.locator(".list-card").last();
  await expect(game.getByLabel("Game mode", { exact: true })).toHaveValue(
    "always",
  );
  const chooser = page.waitForEvent("filechooser");
  await game.getByRole("button", { name: "Choose an executable…" }).click();
  await (
    await chooser
  ).setFiles({
    name: "Race.EXE",
    mimeType: "application/octet-stream",
    buffer: Buffer.from([]),
  });
  await expect(game.getByLabel("App name", { exact: true })).toHaveValue(
    "Race",
  );
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  const edits = saveCalls(host)[0]!.params.edits;
  expect(edits).toEqual(
    expect.arrayContaining([
      { op: "set", path: ["gaming", "automatic"], value: false },
      { op: "set", path: ["gaming", "pass-every-key"], value: true },
      {
        op: "set",
        path: ["rules"],
        value: [
          { ...personal, "game-mode": "never" },
          {
            commands: [],
            "game-mode": "always",
            matches: [{ process: { equals: "Race" } }],
          },
        ],
      },
    ]),
  );
});

test("workspace rename and icon survive reordering", async ({ page }) => {
  const host = await open(page, {
    workspaces: [{ name: "Work", icon: "W" }, { name: "Play" }],
  });
  await navigate(page, "Workspaces");
  const cards = page.locator(".list-card");
  await cards.nth(0).getByLabel("Name", { exact: true }).fill("Code");
  await cards.nth(0).getByLabel("Name", { exact: true }).press("Tab");
  await cards
    .nth(0)
    .getByRole("textbox", { name: /^Icon for workspace / })
    .fill("C");
  await cards
    .nth(0)
    .getByRole("textbox", { name: /^Icon for workspace / })
    .press("Tab");
  await cards
    .nth(0)
    .getByRole("button", { name: /^Move down:/ })
    .click();
  await expect(cards.nth(1).getByLabel("Name", { exact: true })).toHaveValue(
    "Code",
  );
  await expect(
    cards.nth(1).getByRole("textbox", { name: /^Icon for workspace / }),
  ).toHaveValue("C");
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
  await change(page, "Icon for workspace Work", "");
  await navigate(page, "Bar");
  await page
    .getByText("Coordinates and update interval", { exact: true })
    .click();
  await change(page, "Latitude", "91");
  await expect(page.getByLabel("Latitude", { exact: true })).toHaveAttribute(
    "aria-invalid",
    "true",
  );
  await page
    .getByRole("button", { name: "Turn weather off", exact: true })
    .click();
  await expect(page.getByRole("button", { name: "Review errors" })).toHaveCount(
    0,
  );
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
  await page.clock.install();
  await page.keyboard.press("Control+s");
  await expect.poll(() => saveCalls(host).length).toBe(1);
  await page.clock.fastForward(21_000);
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

test("restoring bar defaults before editing a single field keeps other values inherited", async ({
  page,
}) => {
  const host = await open(page, { bar: { ...defaults.bar, height: 60 } });
  await navigate(page, "Appearance");
  await page.getByRole("button", { name: "Reset bar appearance" }).click();
  await page.getByRole("button", { name: "Restore defaults" }).click();
  await change(page, "Height", "60");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]?.params.edits).toEqual([
    { op: "reset", path: ["bar"] },
    { op: "set", path: ["bar", "height"], value: 60 },
  ]);
});

test("valid decimal appearance and weather settings can be saved", async ({
  page,
}) => {
  const host = await open(page, {
    weather: { latitude: 0, longitude: 0, "interval-seconds": 900 },
  });
  await navigate(page, "Appearance");
  await change(page, "Text size", "13.2");
  await change(page, "Opacity", "0.99");
  await navigate(page, "Bar");
  await page
    .getByText("Coordinates and update interval", { exact: true })
    .click();
  await change(page, "Latitude", "33.123456");
  await change(page, "Update interval", "100000");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]?.params.edits).toEqual([
    { op: "set", path: ["bar", "font-size"], value: 13.2 },
    { op: "set", path: ["bar", "opacity"], value: 0.99 },
    { op: "set", path: ["weather", "latitude"], value: 33.123456 },
    { op: "set", path: ["weather", "interval-seconds"], value: 100000 },
  ]);
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
      "Bar",
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
    const captionFits = await page.locator(".preview").evaluate((preview) => {
      const caption = preview
        .querySelector(".preview-caption")!
        .getBoundingClientRect();
      const tiles = preview
        .querySelector(".mini-layout")!
        .getBoundingClientRect();
      const frame = preview.getBoundingClientRect();
      return (
        caption.top >= tiles.bottom &&
        caption.left >= frame.left &&
        caption.right <= frame.right &&
        caption.bottom <= frame.bottom
      );
    });
    expect(captionFits).toBe(true);
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

test("live help: typed launch searches the active mode without changing saved bindings", async ({
  page,
}) => {
  const host = await open(
    page,
    {},
    {
      startPage: "Shortcuts",
      shortcuts: {
        hook_enabled: true,
        mode: "resize",
        paused: false,
        emergency: "ctrl+alt+shift+f12",
        bindings: [
          { chord: "alt+h", commands: ["resize width -2%"] },
          { chord: "escape", commands: ["mode default"] },
          { chord: "alt+x", commands: ["shell custom-tool --flag"] },
        ],
      },
    },
  );
  await expect(page.getByText(/Mode: resize/)).toBeVisible();
  const live = page.getByRole("list", { name: "Available shortcuts" });
  await expect(live).toContainText("Change window width by -2%");
  await expect(live).toContainText("shell custom-tool --flag");
  await page.keyboard.press("Control+f");
  const search = page.getByRole("searchbox", { name: "Find a shortcut" });
  await expect(search).toBeFocused();
  await search.fill("width");
  await expect(live.getByRole("listitem")).toHaveCount(1);
  await search.fill("no-such-action");
  await expect(
    page.getByText(
      "No current shortcuts match. Try a key, action, or workspace name.",
    ),
  ).toBeVisible();
  expect(saveCalls(host)).toHaveLength(0);
  expect((await readyNotifications(page))[0]?.heading).toBe("Shortcuts");
  await search.fill("");
  await page.screenshot({
    path: "../../target/settings-polish/live-shortcuts.png",
    animations: "disabled",
  });
});

test("live help: invalid saved configuration keeps searchable recovery help available", async ({
  page,
}) => {
  const host = await open(
    page,
    {},
    {
      startPage: "Shortcuts",
      loadError: {
        code: "syntax",
        message: "Invalid TOML on line 2.",
        help: "Correct the saved file.",
      },
    },
  );
  await expect(
    page.getByText(
      "Saved settings could not be opened. Current shortcut help remains available.",
    ),
  ).toBeVisible();
  await expect(
    page.getByRole("list", { name: "Available shortcuts" }),
  ).toContainText("Focus the left window");
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toHaveCount(0);
  await page.keyboard.press("Control+f");
  await expect(
    page.getByRole("searchbox", { name: "Find a shortcut" }),
  ).toBeFocused();
  await page.keyboard.press("Control+s");
  expect(saveCalls(host)).toHaveLength(0);
  host.loadError = undefined;
  await page.getByRole("button", { name: "Try settings again" }).click();
  await expect(
    page.getByRole("navigation", { name: "Settings pages" }),
  ).toBeVisible();
  expect(await readyNotifications(page)).toHaveLength(1);
});

test("live help: refresh reflects paused mode and reconnects without discarding a draft", async ({
  page,
}) => {
  const host = await open(page);
  await change(page, "Inner gap", "24");
  await navigate(page, "Shortcuts");
  host.shortcuts = {
    hook_enabled: true,
    mode: "default",
    paused: true,
    emergency: "ctrl+alt+shift+f12",
    bindings: [{ chord: "alt+shift+p", commands: ["pause"] }],
  };
  await page.getByRole("button", { name: "Refresh current shortcuts" }).click();
  await expect(
    page.getByText(/Moon is paused; only the available recovery/),
  ).toBeVisible();
  await expect(
    page
      .getByRole("list", { name: "Available shortcuts" })
      .getByRole("listitem"),
  ).toHaveCount(1);
  host.shortcutsError = {
    code: "offline",
    message: "Moon is stopped.",
    help: "Start it and refresh.",
  };
  await page.getByRole("button", { name: "Refresh current shortcuts" }).click();
  await expect(
    page.getByText(
      "Current shortcuts are unavailable. Start Moon, then refresh this list.",
      { exact: false },
    ),
  ).toBeVisible();
  await expect(
    page.getByRole("list", { name: "Available shortcuts" }),
  ).toHaveCount(0);
  host.shortcutsError = undefined;
  await page.getByRole("button", { name: "Refresh current shortcuts" }).click();
  await expect(
    page.getByRole("list", { name: "Available shortcuts" }),
  ).toBeVisible();
  await navigate(page, "General");
  await expect(page.getByLabel("Inner gap", { exact: true })).toHaveValue("24");
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toBeEnabled();
});

test("live help: a mode with no enabled bindings explains its recovery control", async ({
  page,
}) => {
  await open(
    page,
    {},
    {
      startPage: "Shortcuts",
      shortcuts: {
        hook_enabled: true,
        mode: "quiet",
        paused: false,
        emergency: "ctrl+alt+shift+f12",
        bindings: [],
      },
    },
  );
  await expect(
    page.getByText(
      "No shortcuts are active in this mode. The recovery shortcut above is always available.",
    ),
  ).toBeVisible();
  await expect(
    page.locator("kbd").filter({ hasText: "Ctrl + Alt + Shift + F12" }),
  ).toBeVisible();
});

test("workspace overview reports current assignments and refreshes without editing them", async ({
  page,
}) => {
  const host = await open(page);
  await expect(
    page.getByText("4 managed windows", { exact: true }),
  ).toBeVisible();
  await expect(page.locator(".workspace-summary")).toHaveText(
    "1: 2 windows · active workspace2: 2 windows",
  );
  await page.screenshot({
    path: "../../target/settings-polish/general-overview.png",
    animations: "disabled",
  });
  host.overview = {
    managed_windows: 1,
    active_workspace: "Research",
    workspaces: [{ name: "Research", windows: 1, active: true }],
  };
  await page
    .getByRole("button", { name: "Refresh workspace overview" })
    .click();
  await expect(
    page.getByText("1 managed window", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Current workspace: Research", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Workspace settings", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Workspaces", exact: true }),
  ).toBeVisible();
  expect(saveCalls(host)).toHaveLength(0);
});

test("draft undo restores the previous preview and reset without writing a file", async ({
  page,
}) => {
  const host = await open(page);
  await navigate(page, "Appearance");
  const themes = page.getByRole("group", { name: "Desktop theme" });
  const preview = page.locator(".mini-window").first();
  const dark = await preview.evaluate(
    (node) => getComputedStyle(node).backgroundImage,
  );
  await themes.getByRole("button", { name: "Light", exact: true }).click();
  await expect(preview).not.toHaveCSS("background-image", dark);
  await page.getByRole("button", { name: "Undo last change" }).click();
  await expect(
    themes.getByRole("button", { name: "Dark", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(preview).toHaveCSS("background-image", dark);
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "Undo last change" }),
  ).toBeDisabled();
  await change(page, "Border width", "6");
  await page
    .getByRole("button", { name: "Reset borders", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Restore defaults", exact: true })
    .click();
  await page.getByRole("button", { name: "Undo last change" }).click();
  await expect(page.getByLabel("Border width", { exact: true })).toHaveValue(
    "6",
  );
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]!.params.edits).toEqual([
    { op: "set", path: ["borders", "width"], value: 6 },
  ]);
});

test("saved state persists through navigation and rechecks application without changing a new draft", async ({
  page,
}) => {
  const host = await open(page);
  host.application = {
    state: "failed",
    message:
      "Saved and loaded by Moon. Latest Moon error: Monitor Office is unavailable.",
  };
  await change(page, "Inner gap", "24");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect(page.locator(".application-state")).toContainText(
    "Monitor Office is unavailable.",
  );
  await navigate(page, "Appearance");
  await expect(page.locator(".application-state")).toContainText(
    "Monitor Office is unavailable.",
  );
  await change(page, "Border width", "6");
  host.application = {
    state: "loaded",
    message: "Saved and loaded by Moon.",
  };
  await page
    .getByRole("button", { name: "Check desktop", exact: true })
    .click();
  await expect(page.locator(".application-state")).toHaveText(
    "Saved and loaded by Moon.",
  );
  await expect(page.getByLabel("Border width", { exact: true })).toHaveValue(
    "6",
  );
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toBeEnabled();
  expect(saveCalls(host)).toHaveLength(1);
  expect(
    host.calls
      .filter((call) => call.method === "application-status")
      .every((call) => call.params.expected_source === "# saved fixture\n"),
  ).toBe(true);
});

test("draft undo restores an invalid field and leaves native text Undo available", async ({
  page,
}) => {
  await open(page);
  await page.getByLabel("Inner gap", { exact: true }).fill("");
  await expect(page.getByLabel("Inner gap", { exact: true })).toHaveAttribute(
    "aria-invalid",
    "true",
  );
  await page.getByRole("button", { name: "Undo last change" }).click();
  await expect(page.getByLabel("Inner gap", { exact: true })).toHaveValue(
    String(defaults.gaps.inner),
  );
  await expect(
    page.getByRole("button", { name: "Review errors", exact: true }),
  ).toHaveCount(0);
  await navigate(page, "Advanced");
  await page.getByText("Command aliases", { exact: true }).click();
  await page.getByLabel("Command aliases", { exact: true }).fill("{");
  await page.getByRole("button", { name: "Undo last change" }).click();
  await expect(
    page.getByRole("button", { name: "Review errors", exact: true }),
  ).toHaveCount(0);
  await navigate(page, "Shortcuts");
  const field = page.getByRole("searchbox", { name: "Find a shortcut" });
  await field.focus();
  await field.pressSequentially("workspace");
  await field.press("Control+z");
  await expect(field).not.toHaveValue("workspace");
  await expect(field).toBeFocused();
});

test("current desktop help refreshes on page return and native activation while retaining a draft", async ({
  page,
}) => {
  const host = await open(page);
  await change(page, "Inner gap", "24");
  await navigate(page, "Shortcuts");
  await expect(page.getByText(/Mode: default/)).toBeVisible();
  await navigate(page, "Appearance");
  host.shortcuts = {
    hook_enabled: true,
    mode: "resize",
    paused: false,
    emergency: "ctrl+alt+shift+f12",
    bindings: [{ chord: "alt+l", commands: ["resize width +2%"] }],
  };
  await navigate(page, "Shortcuts");
  await expect(page.getByText(/Mode: resize/)).toBeVisible();
  host.shortcuts = {
    hook_enabled: true,
    mode: "default",
    paused: true,
    emergency: "ctrl+alt+shift+f12",
    bindings: [{ chord: "alt+shift+p", commands: ["pause"] }],
  };
  await page.evaluate(() =>
    (window as unknown as { moonActivated(): void }).moonActivated(),
  );
  await expect(
    page.getByText(/Moon is paused; only the available recovery/),
  ).toBeVisible();
  host.overview = {
    managed_windows: 1,
    active_workspace: "Writing",
    workspaces: [{ name: "Writing", windows: 1, active: true }],
  };
  await navigate(page, "General");
  await expect(
    page.getByText("Current workspace: Writing", { exact: true }),
  ).toBeVisible();
  host.overview = {
    managed_windows: 2,
    active_workspace: "Research",
    workspaces: [{ name: "Research", windows: 2, active: true }],
  };
  await page.evaluate(() => window.dispatchEvent(new Event("focus")));
  await expect(
    page.getByText("Current workspace: Research", { exact: true }),
  ).toBeVisible();
  await expect(page.getByLabel("Inner gap", { exact: true })).toHaveValue("24");
  expect(saveCalls(host)).toHaveLength(0);
});

test("live help: disabled configured hooks remain descriptive and recovery remains discoverable", async ({
  page,
}) => {
  await open(
    page,
    {},
    {
      startPage: "Shortcuts",
      shortcuts: {
        hook_enabled: false,
        mode: "default",
        paused: false,
        emergency: "ctrl+alt+shift+f12",
        bindings: [
          { chord: "alt+h", commands: ["focus left"] },
          { chord: "alt+r", commands: ["restore"] },
        ],
      },
    },
  );
  await expect(
    page.getByRole("heading", { name: "Configured shortcuts", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText(/Configured shortcuts are disabled for this Moon session/),
  ).toBeVisible();
  await expect(
    page.getByRole("list", { name: "Available shortcuts" }),
  ).toHaveCount(0);
  const configured = page.getByRole("list", {
    name: "Configured shortcuts (inactive)",
    exact: true,
  });
  await expect(configured).toContainText("Focus the left window");
  await expect(configured).toContainText(
    "Restore original window positions and stop Moon",
  );
  await expect(
    page.locator("kbd").filter({ hasText: "Ctrl + Alt + Shift + F12" }),
  ).toBeVisible();
});

test("merged draft undo retains a parent reset after undoing its later child edit", async ({
  page,
}) => {
  const host = await open(page, { bar: { ...defaults.bar, height: 60 } });
  await navigate(page, "Appearance");
  await page
    .getByRole("button", { name: "Reset bar appearance", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Restore defaults", exact: true })
    .click();
  await change(page, "Height", "60");
  await page.getByRole("button", { name: "Undo last change" }).click();
  await expect(page.getByLabel("Height", { exact: true })).toHaveValue(
    String(defaults.bar.height),
  );
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]?.params.edits).toEqual([
    { op: "reset", path: ["bar"] },
  ]);
});

test("Overview search uses exact guarded string identities and Escape clears the filter", async ({
  page,
}) => {
  const host = await open(page, {}, { startPage: "Overview" });
  const search = page.getByRole("searchbox", { name: "Find an app or window" });
  await search.fill("project notes");
  await expect(
    page.getByRole("button", {
      name: "Activate Research browser",
      exact: true,
    }),
  ).toHaveCount(0);
  await search.press("Enter");
  await expect
    .poll(() => host.calls.filter((c) => c.method === "window-action").length)
    .toBe(1);
  expect(host.calls.find((c) => c.method === "window-action")!.params).toEqual({
    generation: host.desktop.generation,
    revision: host.desktop.revision,
    target: {
      id: "9007199254740993",
      process_id: "4000",
      process_birth: "134391583249052813",
    },
    action: { type: "focus" },
  });
  await search.press("Escape");
  await expect(search).toHaveValue("");
  await expect(
    page.getByRole("button", {
      name: "Activate Research browser",
      exact: true,
    }),
  ).toBeVisible();
  expect(host.calls.filter((c) => c.method === "save")).toHaveLength(0);
});

test("Window options and internal drag use the same guarded workspace move", async ({
  page,
}) => {
  const host = await open(page, {}, { startPage: "Overview" });
  await page
    .getByRole("button", { name: "Options for Project notes", exact: true })
    .click();
  await page
    .getByRole("combobox", { name: "Move Code to workspace" })
    .selectOption("3");
  await page.getByRole("button", { name: "Move window", exact: true }).click();
  await expect
    .poll(() => host.calls.filter((c) => c.method === "window-action").length)
    .toBe(1);
  await expect(
    page.getByRole("button", {
      name: "Options for Project notes",
      exact: true,
    }),
  ).toBeFocused();
  expect(
    host.calls.find((c) => c.method === "window-action")!.params.action,
  ).toEqual({ type: "move-to-workspace", workspace: "3", follow: false });
  await page
    .getByRole("button", { name: "Move Research browser", exact: true })
    .dragTo(page.locator('.workspace-card[data-workspace="2"]'));
  await expect
    .poll(() => host.calls.filter((c) => c.method === "window-action").length)
    .toBe(2);
  expect(
    host.calls.filter((c) => c.method === "window-action")[1]!.params.action,
  ).toEqual({ type: "move-to-workspace", workspace: "2", follow: false });
  const external = await page.evaluateHandle(() => new DataTransfer());
  await external.evaluate((t) =>
    t.setData("application/x-moon-window", "9007199254740993"),
  );
  await page
    .locator('.workspace-card[data-workspace="3"]')
    .dispatchEvent("drop", { dataTransfer: external });
  expect(host.calls.filter((c) => c.method === "window-action")).toHaveLength(
    2,
  );
});

test("A stale desktop action reports the failure and refreshes without replay", async ({
  page,
}) => {
  const host = await open(page, {}, { startPage: "Overview" });
  host.methodErrors["window-action"] = {
    code: "stale",
    message: "The desktop changed. Refresh and try again.",
    help: "No window was changed.",
  };
  await page
    .getByRole("button", { name: "Activate Project notes", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("The desktop changed");
  await expect
    .poll(() => host.calls.filter((c) => c.method === "desktop").length)
    .toBeGreaterThan(1);
  expect(host.calls.filter((c) => c.method === "window-action")).toHaveLength(
    1,
  );
});

test("Layout Undo preserves the independent Settings draft", async ({
  page,
}) => {
  const host = await open(page);
  await page
    .getByRole("spinbutton", { name: "Inner gap", exact: true })
    .fill("29");
  await page.getByRole("button", { name: "Overview", exact: true }).click();
  await page.getByRole("button", { name: "Undo layout", exact: true }).click();
  await expect
    .poll(() => host.calls.filter((c) => c.method === "layout-undo").length)
    .toBe(1);
  await expect(
    page.getByRole("button", { name: "Undo layout", exact: true }),
  ).toBeFocused();
  expect(host.calls.find((c) => c.method === "layout-undo")!.params).toEqual({
    generation: host.desktop.generation,
    revision: host.desktop.revision,
  });
  await page.getByRole("button", { name: "General", exact: true }).click();
  await expect(
    page.getByRole("spinbutton", { name: "Inner gap", exact: true }),
  ).toHaveValue("29");
  await expect(
    page.getByRole("button", { name: "Save changes" }),
  ).toBeEnabled();
  expect(host.calls.filter((c) => c.method === "save")).toHaveLength(0);
});

test("Rule creation previews current matches and stages one undoable personal rule", async ({
  page,
}) => {
  const host = await open(
    page,
    { rules: [personalRule] },
    { startPage: "Overview" },
  );
  await page
    .getByRole("button", { name: "Options for Project notes", exact: true })
    .click();
  await page
    .getByText("Rules matching this window now", { exact: true })
    .click();
  await expect(page.getByRole("dialog")).toContainText("not the history");
  await page
    .getByRole("button", { name: "Create app rule", exact: true })
    .click();
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "2 managed windows currently match" }),
  ).toBeVisible();
  await page
    .getByRole("combobox", { name: "Rule workspace" })
    .selectOption("2");
  await page.getByRole("button", { name: "Add rule to draft" }).click();
  await expect(
    page.getByText("2 personal rules", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Undo last change" }).click();
  await expect(
    page.getByText("1 personal rule", { exact: true }),
  ).toBeVisible();
  expect(
    host.calls.filter((c) => ["save", "window-action"].includes(c.method)),
  ).toHaveLength(0);
});

test("Profiles use source tokens, apply to draft, rename and confirm removal", async ({
  page,
}) => {
  const host = await open(page);
  await page.getByRole("button", { name: "Workspaces", exact: true }).click();
  await page.getByRole("button", { name: "Save this setup" }).click();
  await page.getByRole("textbox", { name: "Saved item name" }).fill("Desk");
  await page.getByRole("button", { name: "Save to library" }).click();
  await expect(page.getByRole("button", { name: "Apply Desk" })).toBeEnabled();
  expect(
    host.calls.find((c) => c.method === "library-save")!.params,
  ).toMatchObject({
    expected_source: "library-v1",
    kind: "profile",
    name: "Desk",
    replace: false,
    config: { theme: "dark" },
  });
  await expect(
    page.getByRole("button", { name: "Save changes" }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "Apply Desk" }).click();
  await expect(
    page.getByRole("button", { name: "Save changes" }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Undo last change" }).click();
  await expect(
    page.getByRole("button", { name: "Save changes" }),
  ).toBeDisabled();
  await page
    .locator(".library-card")
    .filter({ hasText: "Desk" })
    .getByRole("button", { name: "Rename", exact: true })
    .click();
  await page.getByRole("textbox", { name: "Saved item name" }).fill("Laptop");
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Rename", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Apply Laptop" }),
  ).toBeVisible();
  await page
    .locator(".library-card")
    .filter({ hasText: "Laptop" })
    .getByRole("button", { name: "Remove", exact: true })
    .click();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(host.calls.filter((c) => c.method === "library-delete")).toHaveLength(
    0,
  );
  await page
    .locator(".library-card")
    .filter({ hasText: "Laptop" })
    .getByRole("button", { name: "Remove", exact: true })
    .click();
  await page.getByRole("button", { name: "Remove saved item" }).click();
  await expect(page.getByRole("button", { name: "Apply Laptop" })).toHaveCount(
    0,
  );
  expect(host.calls.filter((c) => c.method === "save")).toHaveLength(0);
});

test("Curated themes keep preview and change only an undoable draft", async ({
  page,
}) => {
  const host = await open(page);
  await page.getByRole("button", { name: "Appearance", exact: true }).click();
  for (const name of ["Linen", "Slate", "Forest"])
    await expect(
      page.getByRole("button", { name: `Apply ${name}`, exact: true }),
    ).toBeVisible();
  await expect(
    page
      .locator(".library-card")
      .getByRole("button", { name: "Remove", exact: true }),
  ).toHaveCount(0);
  await page.getByRole("button", { name: "Apply Forest" }).click();
  await expect(
    page.getByRole("button", { name: "Light", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(page.locator(".preview")).toBeVisible();
  await page.getByRole("button", { name: "Undo last change" }).click();
  await expect(
    page.getByRole("button", { name: "Dark", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(
    page.getByRole("button", { name: "Save changes" }),
  ).toBeDisabled();
  expect(host.calls.filter((c) => c.method === "save")).toHaveLength(0);
});

test("Theme import and clipboard export stay local and require overwrite confirmation", async ({
  page,
}) => {
  const host = await open(page);
  await page.getByRole("button", { name: "Appearance", exact: true }).click();
  await page
    .locator(".library-card")
    .filter({ has: page.getByRole("heading", { name: "Linen", exact: true }) })
    .getByRole("button", { name: "Copy data", exact: true })
    .click();
  const exported = await page
    .getByRole("textbox", { name: "Exported Moon data" })
    .inputValue();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Copy data" })
    .click();
  await expect
    .poll(() =>
      host.calls.some(
        (c) => c.method === "clipboard-copy" && c.params.text === exported,
      ),
    )
    .toBe(true);
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Close", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Paste theme data", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Moon import data" })
    .fill("not json");
  await expect(
    page.getByRole("button", { name: "Import to library" }),
  ).toBeDisabled();
  const document = {
    version: 1,
    kind: "theme",
    name: "Evening",
    values: { theme: "dark" },
  };
  await page
    .getByRole("textbox", { name: "Moon import data" })
    .fill(JSON.stringify(document));
  await page.getByRole("button", { name: "Import to library" }).click();
  await expect(
    page.getByRole("button", { name: "Apply Evening" }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Paste theme data", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Moon import data" })
    .fill(JSON.stringify(document));
  await expect(
    page.getByRole("button", { name: "Import to library" }),
  ).toBeDisabled();
  await page
    .getByRole("checkbox", { name: "Replace the saved item with this name" })
    .check();
  await page.getByRole("button", { name: "Import to library" }).click();
  expect(
    host.calls.filter((c) => c.method === "library-import").at(-1)!.params
      .replace,
  ).toBe(true);
  expect(host.calls.filter((c) => c.method === "save")).toHaveLength(0);
});

test("Library conflicts retain input and never claim a profile was saved", async ({
  page,
}) => {
  const host = await open(page);
  host.methodErrors["library-save"] = {
    code: "conflict",
    message: "The library changed. Refresh it before saving again.",
    help: "Your settings have not changed.",
  };
  await page.getByRole("button", { name: "Workspaces", exact: true }).click();
  await page.getByRole("button", { name: "Save this setup" }).click();
  await page.getByRole("textbox", { name: "Saved item name" }).fill("Desk");
  await page.getByRole("button", { name: "Save to library" }).click();
  await expect(page.getByRole("dialog").getByRole("alert")).toContainText(
    "library changed",
  );
  await expect(
    page.getByRole("textbox", { name: "Saved item name" }),
  ).toHaveValue("Desk");
  await expect(
    page.getByRole("button", { name: "Save to library" }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page.getByRole("button", { name: "Apply Desk" })).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Save changes" }),
  ).toBeDisabled();
});

test("Recovery uses typed guarded actions with an explicit stop confirmation", async ({
  page,
}) => {
  const host = await open(page);
  await page.getByRole("button", { name: "About", exact: true }).click();
  await page.getByRole("button", { name: "Pause Moon", exact: true }).click();
  await expect
    .poll(() => host.calls.filter((c) => c.method === "desktop-action").length)
    .toBe(1);
  expect(host.calls.find((c) => c.method === "desktop-action")!.params).toEqual(
    {
      generation: host.desktop.generation,
      revision: host.desktop.revision,
      action: "pause",
    },
  );
  await page
    .getByRole("button", { name: "Restore windows and stop", exact: true })
    .click();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(host.calls.filter((c) => c.method === "desktop-action")).toHaveLength(
    1,
  );
  await page
    .getByRole("button", { name: "Restore windows and stop", exact: true })
    .click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Restore windows and stop", exact: true })
    .click();
  await expect
    .poll(() => host.calls.filter((c) => c.method === "desktop-action").length)
    .toBe(2);
  expect(
    host.calls.filter((c) => c.method === "desktop-action").at(-1)!.params
      .action,
  ).toBe("restore");
});

test("Native preview rectangles are bounded and clear on dialogs and navigation", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1100, height: 900 });
  const host = await open(page, {}, { startPage: "Overview" });
  await expect
    .poll(() =>
      host.calls.some(
        (c) =>
          c.method === "previews" &&
          Array.isArray(c.params.items) &&
          c.params.items.length > 0,
      ),
    )
    .toBe(true);
  for (const call of host.calls.filter((c) => c.method === "previews")) {
    const items = call.params.items as {
      rect: { x: number; y: number; width: number; height: number };
    }[];
    expect(items.length).toBeLessThanOrEqual(32);
    for (const { rect } of items) {
      expect(rect.x).toBeGreaterThanOrEqual(0);
      expect(rect.y).toBeGreaterThanOrEqual(0);
      expect(rect.x + rect.width).toBeLessThanOrEqual(1100);
      expect(rect.y + rect.height).toBeLessThanOrEqual(900);
    }
  }
  await page
    .getByRole("button", { name: "Options for Project notes", exact: true })
    .click();
  await expect
    .poll(
      () =>
        host.calls.filter((c) => c.method === "previews").at(-1)!.params.items,
    )
    .toEqual([]);
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Close", exact: true })
    .click();
  await page.getByRole("button", { name: "General", exact: true }).click();
  await expect
    .poll(
      () =>
        host.calls.filter((c) => c.method === "previews").at(-1)!.params.items,
    )
    .toEqual([]);
});

test("Native import waits for the file picker and previews before storing; cancel is harmless", async ({
  page,
}) => {
  const host = await open(page);
  await page.getByRole("button", { name: "Appearance", exact: true }).click();
  await page.getByRole("button", { name: "Import theme", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Import theme", exact: true }),
  ).toBeEnabled();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  let finish!: () => void;
  host.fileGate = new Promise<void>((resolve) => {
    finish = resolve;
  });
  host.importDocument = JSON.stringify({
    version: 1,
    kind: "theme",
    name: "Evening",
    values: { theme: "dark" },
  });
  await page.clock.install();
  await page.getByRole("button", { name: "Import theme", exact: true }).click();
  await page.clock.fastForward(21000);
  await expect(
    page.getByRole("button", { name: "Import theme", exact: true }),
  ).toBeDisabled();
  await expect(page.getByRole("alert")).toHaveCount(0);
  finish();
  await expect(
    page.getByRole("textbox", { name: "Moon import data" }),
  ).toHaveValue(host.importDocument);
  expect(host.calls.filter((c) => c.method === "library-import")).toHaveLength(
    0,
  );
  await page.getByRole("button", { name: "Import to library" }).click();
  await expect(
    page.getByRole("button", { name: "Apply Evening" }),
  ).toBeVisible();
  await page
    .locator(".library-card")
    .filter({ hasText: "Evening" })
    .getByRole("button", { name: "Export", exact: true })
    .click();
  await expect
    .poll(() => host.calls.some((c) => c.method === "library-export-file"))
    .toBe(true);
  expect(
    host.calls.find((c) => c.method === "library-export-file")!.params,
  ).toMatchObject({ suggested_name: "Evening.moon-theme.json" });
  expect(host.calls.filter((c) => c.method === "save")).toHaveLength(0);
});

test("Standalone Overview closes after clean activation; dirty Settings stays open", async ({
  page,
}) => {
  const host = await open(page, {}, { startPage: "Overview" });
  await expect(page.locator(".savebar")).toBeHidden();
  await expect(
    page.getByRole("searchbox", { name: "Find an app or window" }),
  ).toBeFocused();
  await page
    .getByRole("button", { name: "Activate Project notes", exact: true })
    .click();
  await expect
    .poll(() => host.calls.filter((c) => c.method === "close").length)
    .toBe(1);
  await page.getByRole("button", { name: "General", exact: true }).click();
  await page
    .getByRole("spinbutton", { name: "Inner gap", exact: true })
    .fill("27");
  await page.getByRole("button", { name: "Overview", exact: true }).click();
  await page
    .getByRole("button", { name: "Activate Research browser", exact: true })
    .click();
  await expect
    .poll(() => host.calls.filter((c) => c.method === "window-action").length)
    .toBe(2);
  expect(host.calls.filter((c) => c.method === "close")).toHaveLength(1);
  await page
    .getByRole("searchbox", { name: "Find an app or window" })
    .press("Escape");
  await expect(page.getByRole("dialog")).toContainText("Leave without saving?");
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Save changes" }),
  ).toBeEnabled();
});

for (const visual of [
  {
    name: "overview-desktop-dark",
    width: 1100,
    height: 850,
    page: "Overview",
    scheme: "dark",
    contrast: "none",
  },
  {
    name: "overview-narrow-light",
    width: 370,
    height: 750,
    page: "Overview",
    scheme: "light",
    contrast: "none",
  },
  {
    name: "overview-high-contrast",
    width: 740,
    height: 650,
    page: "Overview",
    scheme: "dark",
    contrast: "active",
  },
  {
    name: "appearance-desktop-light",
    width: 1100,
    height: 850,
    page: "Appearance",
    scheme: "light",
    contrast: "none",
  },
  {
    name: "profiles-narrow-dark",
    width: 370,
    height: 750,
    page: "Workspaces",
    scheme: "dark",
    contrast: "none",
  },
] as const) {
  test(`new surfaces reflow and capture: ${visual.name}`, async ({ page }) => {
    await page.setViewportSize({ width: visual.width, height: visual.height });
    await page.emulateMedia({
      colorScheme: visual.scheme,
      forcedColors: visual.contrast,
      reducedMotion: "reduce",
    });
    await open(page);
    await page
      .getByRole("navigation")
      .getByRole("button", { name: visual.page, exact: true })
      .click();
    if (visual.page === "Overview")
      await expect(
        page.getByRole("button", {
          name: "Activate Project notes",
          exact: true,
        }),
      ).toBeVisible();
    if (visual.page === "Appearance")
      await expect(
        page.getByRole("button", { name: "Apply Forest", exact: true }),
      ).toBeVisible();
    expect(
      await page.evaluate(
        () => document.documentElement.scrollWidth <= innerWidth,
      ),
    ).toBe(true);
    await page.screenshot({
      path: `../../target/settings-six-polish/${visual.name}.png`,
      fullPage: true,
      animations: "disabled",
    });
    if (visual.page === "Overview") {
      await page
        .getByRole("button", { name: "Options for Project notes", exact: true })
        .click();
      await page
        .getByRole("button", { name: "Create app rule", exact: true })
        .click();
      await expect(
        page.getByRole("button", { name: "Add rule to draft" }),
      ).toBeInViewport();
      const rect = await page.getByRole("dialog").boundingBox();
      expect(rect!.x).toBeGreaterThanOrEqual(0);
      expect(rect!.x + rect!.width).toBeLessThanOrEqual(visual.width);
      await page.screenshot({
        path: `../../target/settings-six-polish/${visual.name}-rule.png`,
        animations: "disabled",
      });
    }
  });
}

test("Pointer move cancels on Escape, restores previews, and ignores suspended windows", async ({
  page,
}) => {
  const host = await open(page, {}, { startPage: "Overview" });
  const handle = page.getByRole("button", {
    name: "Move Research browser",
    exact: true,
  });
  const source = (await handle.boundingBox())!;
  const destination = (await page
    .locator('.workspace-card[data-workspace="2"]')
    .boundingBox())!;
  await page.mouse.move(
    source.x + source.width / 2,
    source.y + source.height / 2,
  );
  await page.mouse.down();
  await page.mouse.move(destination.x + 50, destination.y + 60, { steps: 4 });
  await expect(page.locator('.workspace-card[data-workspace="2"]')).toHaveClass(
    /drop-target/,
  );
  await expect
    .poll(
      () =>
        host.calls.filter((c) => c.method === "previews").at(-1)?.params.items,
    )
    .toEqual([]);
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await expect(page.locator(".window-drag-ghost")).toHaveCount(0);
  await expect(page.locator(".drop-target")).toHaveCount(0);
  expect(host.calls.filter((c) => c.method === "window-action")).toHaveLength(
    0,
  );
  await expect
    .poll(
      () =>
        (
          host.calls.filter((c) => c.method === "previews").at(-1)?.params
            .items as unknown[]
        )?.length,
    )
    .toBeGreaterThan(0);
  host.desktop.windows[1]!.state.suspended = true;
  await page.getByRole("button", { name: "Refresh windows" }).click();
  await expect(handle).toBeDisabled();
});

test("Touch pointer capture moves a window without HTML drag APIs", async ({
  page,
}) => {
  const host = await open(page, {}, { startPage: "Overview" });
  const from = (await page
    .getByRole("button", { name: "Move Project notes", exact: true })
    .boundingBox())!;
  const to = (await page
    .locator('.workspace-card[data-workspace="2"]')
    .boundingBox())!;
  const client = await page.context().newCDPSession(page);
  await client.send("Input.dispatchTouchEvent", {
    type: "touchStart",
    touchPoints: [{ x: from.x + from.width / 2, y: from.y + from.height / 2 }],
  });
  await client.send("Input.dispatchTouchEvent", {
    type: "touchMove",
    touchPoints: [{ x: to.x + 40, y: to.y + 80 }],
  });
  await expect(page.locator('.workspace-card[data-workspace="2"]')).toHaveClass(
    /drop-target/,
  );
  await client.send("Input.dispatchTouchEvent", {
    type: "touchEnd",
    touchPoints: [],
  });
  await expect
    .poll(() => host.calls.filter((c) => c.method === "window-action").length)
    .toBe(1);
  expect(
    host.calls.find((c) => c.method === "window-action")!.params.action,
  ).toEqual({ type: "move-to-workspace", workspace: "2", follow: false });
  await client.detach();
});

test("Ten workspaces prioritize active and occupied cards while empty targets stay compact and usable", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1100, height: 1000 });
  const host = await open(page, {}, { startPage: "Overview" });
  const original = structuredClone(host.desktop.windows[0]!);
  host.desktop.windows[0]!.workspace = "10";
  host.desktop.windows[1]!.state.visible = false;
  host.desktop.windows.push({
    ...original,
    id: "9007199254740996",
    process: "Claude",
    title: "Current conversation",
    workspace: "10",
    state: { ...original.state, focused: false },
  });
  host.desktop.total_windows = 4;
  host.desktop.active_workspace = "10";
  const bounds = host.desktop.workspaces[0]!.bounds;
  host.desktop.workspaces = Array.from({ length: 10 }, (_, index) => ({
    name: String(index + 1),
    monitor_id: "3229624234",
    active: index === 9,
    windows: index === 9 ? 2 : index < 2 ? 1 : 0,
    bounds,
  }));
  await page.getByRole("button", { name: "Refresh windows" }).click();
  const cards = page.locator(".workspace-card");
  const configured = Array.from({ length: 10 }, (_, i) => String(i + 1));
  const ordered = ["10", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
  await expect
    .poll(() =>
      cards.evaluateAll((nodes) =>
        nodes.map((node) => (node as HTMLElement).dataset.workspace),
      ),
    )
    .toEqual(ordered);
  await expect(
    page.getByRole("button", { name: "Activate Current conversation" }),
  ).toBeInViewport();
  await expect(page.locator(".workspace-empty .workspace-canvas")).toHaveCount(
    0,
  );
  await expect(page.locator(".workspace-empty")).toHaveCount(7);
  for (const card of await page.locator(".workspace-empty").all())
    expect((await card.boundingBox())!.height).toBeLessThan(120);
  await expect(
    page.locator('.workspace-card[data-workspace="10"] .workspace-canvas'),
  ).toHaveCount(1);

  // The reordered display must not reorder the explicit keyboard Move choices.
  const handle = page.getByRole("button", {
    name: "Move Research browser",
    exact: true,
  });
  await handle.focus();
  await handle.press("Enter");
  await expect
    .poll(() =>
      page
        .getByRole("combobox", { name: "Move Arc to workspace" })
        .locator("option")
        .evaluateAll((nodes) =>
          nodes.map((node) => (node as HTMLOptionElement).value),
        ),
    )
    .toEqual(configured);
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Close", exact: true })
    .click();

  const source = (await handle.boundingBox())!;
  const target = (await page
    .locator('.workspace-card[data-workspace="3"]')
    .boundingBox())!;
  await page.mouse.move(
    source.x + source.width / 2,
    source.y + source.height / 2,
  );
  await page.mouse.down();
  await page.mouse.move(
    target.x + target.width / 2,
    target.y + target.height / 2,
    { steps: 6 },
  );
  await expect(page.locator('.workspace-card[data-workspace="3"]')).toHaveClass(
    /drop-target/,
  );
  await page.mouse.up();
  await expect
    .poll(
      () => host.calls.filter((call) => call.method === "window-action").length,
    )
    .toBe(1);
  expect(
    host.calls.find((call) => call.method === "window-action")!.params,
  ).toMatchObject({
    generation: host.desktop.generation,
    revision: host.desktop.revision,
    target: { id: "9007199254740994" },
    action: { type: "move-to-workspace", workspace: "3", follow: false },
  });

  const search = page.getByRole("searchbox", { name: "Find an app or window" });
  await search.fill("Second project");
  await expect(cards).toHaveCount(1);
  await expect(cards.first()).toHaveAttribute("data-workspace", "2");
  await search.press("ArrowDown");
  await expect(
    page.getByRole("button", { name: "Activate Second project", exact: true }),
  ).toBeFocused();
  await page.getByRole("button", { name: "Clear search", exact: true }).click();
  await expect
    .poll(() =>
      cards.evaluateAll((nodes) =>
        nodes.map((node) => (node as HTMLElement).dataset.workspace),
      ),
    )
    .toEqual(ordered);
  expect(host.calls.filter((call) => call.method === "save")).toHaveLength(0);
});

test("Recovery claims its guard before an asynchronous desktop query", async ({
  page,
}) => {
  const host = await open(page);
  await page.getByRole("button", { name: "About", exact: true }).click();
  let release!: () => void;
  host.desktopGate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page.getByRole("button", { name: "Pause Moon", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Pause Moon", exact: true }),
  ).toBeDisabled();
  await page
    .getByRole("button", { name: "Pause Moon", exact: true })
    .dispatchEvent("click");
  await expect
    .poll(() => host.calls.filter((c) => c.method === "desktop").length)
    .toBe(1);
  expect(host.calls.filter((c) => c.method === "desktop-action")).toHaveLength(
    0,
  );
  release();
  await expect
    .poll(() => host.calls.filter((c) => c.method === "desktop-action").length)
    .toBe(1);
});

test("Live Overview survives invalid saved settings without enabling draft editing", async ({
  page,
}) => {
  const host = await open(
    page,
    {},
    {
      startPage: "Overview",
      loadError: {
        code: "invalid",
        message: "Saved TOML has an invalid value.",
        help: "Fix the saved file.",
      },
    },
  );
  await expect(
    page.getByText("Saved settings could not be opened.", { exact: false }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Options for Project notes", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Create app rule", exact: true }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "Float window", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Float window", exact: true }).click();
  await expect
    .poll(() => host.calls.filter((c) => c.method === "window-action").length)
    .toBe(1);
  await page
    .getByRole("searchbox", { name: "Find an app or window" })
    .fill("research");
  await page
    .getByRole("searchbox", { name: "Find an app or window" })
    .press("Enter");
  await expect
    .poll(() => host.calls.filter((c) => c.method === "close").length)
    .toBe(1);
  expect(host.calls.filter((c) => c.method === "save")).toHaveLength(0);
});

test("Library names validate Unicode length and palette swatches use actual tokens", async ({
  page,
}) => {
  const host = await open(page);
  await navigate(page, "Appearance");
  await expect(page.getByRole("button", { name: "Apply Linen" })).toBeVisible();
  await expect(
    page.locator(".library-card").first().locator(".swatches i").last(),
  ).toHaveCSS("background-color", "rgb(77, 112, 86)");
  await page.getByRole("button", { name: "Save your theme" }).click();
  const name = page.getByRole("textbox", { name: "Saved item name" });
  await name.fill("x".repeat(65));
  await expect(
    page.getByRole("button", { name: "Save to library" }),
  ).toBeDisabled();
  await name.fill("Desk/Work");
  await expect(
    page.getByRole("button", { name: "Save to library" }),
  ).toBeDisabled();
  await name.fill("🌙".repeat(64));
  await expect(
    page.getByRole("button", { name: "Save to library" }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Save to library" }).click();
  await expect
    .poll(() => host.calls.filter((c) => c.method === "library-save").length)
    .toBe(1);
  expect(host.calls.find((c) => c.method === "library-save")!.params.name).toBe(
    "🌙".repeat(64),
  );
});

test("Rule match preview uses the daemon's ASCII-only case folding", async ({
  page,
}) => {
  const host = await open(page);
  host.desktop.windows[0]!.process = "Äpp";
  host.desktop.windows[1]!.process = "äpp";
  await page.getByRole("button", { name: "Overview", exact: true }).click();
  await page
    .getByRole("button", { name: "Options for Project notes", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Create app rule", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toContainText(
    "1 managed window currently matches",
  );
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  expect(host.calls.filter((c) => c.method === "save")).toHaveLength(0);
});

test("A clean standalone Overview reserves the full client height for its canvas", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1100, height: 850 });
  await open(page, {}, { startPage: "Overview" });
  await expect(page.locator(".savebar")).toBeHidden();
  expect(
    await page
      .locator(".scroll")
      .evaluate((node) => node.getBoundingClientRect().bottom),
  ).toBe(850);
  await page.screenshot({
    path: "../../target/settings-six-polish/overview-standalone-dark.png",
    animations: "disabled",
  });
  await page.getByRole("button", { name: "General", exact: true }).click();
  await expect(page.locator(".savebar")).toBeVisible();
  await page
    .getByRole("spinbutton", { name: "Inner gap", exact: true })
    .fill("27");
  await page.getByRole("button", { name: "Overview", exact: true }).click();
  await expect(page.locator(".savebar")).toBeVisible();
});

test("startup preference changes immediately without saving or discarding the desktop draft", async ({
  page,
}) => {
  const host = await open(page);
  await change(page, "Inner gap", "24");
  let release!: () => void;
  host.startupGate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const startup = page.getByRole("switch", {
    name: "Launch Moon when I sign in",
  });
  await expect(startup).toBeChecked();
  await startup.uncheck();
  await expect(startup).toBeDisabled();
  expect(
    host.calls.filter((call) => call.method === "startup-set"),
  ).toHaveLength(1);
  expect(saveCalls(host)).toHaveLength(0);
  release();
  await expect(startup).toBeEnabled();
  await expect(startup).not.toBeChecked();
  await expect(page.getByLabel("Inner gap", { exact: true })).toHaveValue("24");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]!.params.edits).toEqual([
    { op: "set", path: ["gaps", "inner"], value: 24 },
  ]);
  expect(host.calls.some((call) => call.method.startsWith("location-"))).toBe(
    false,
  );
});

test("startup errors and unavailable installation keep the confirmed switch state", async ({
  page,
}) => {
  const host = await open(page);
  host.methodErrors["startup-set"] = {
    code: "unavailable",
    message: "Owned startup task could not be changed.",
    help: "Repair Moon Setup.",
  };
  const startup = page.getByRole("switch", {
    name: "Launch Moon when I sign in",
  });
  await startup.click();
  await expect(page.getByRole("alert")).toContainText("Owned startup task");
  await expect(startup).toBeChecked();
  host.startup = {
    available: false,
    enabled: true,
    message: "Install Moon to manage launch on sign-in.",
  };
  await page.getByRole("button", { name: "Retry startup status" }).click();
  await expect(startup).toBeDisabled();
  await expect(page.getByText(host.startup.message)).toBeVisible();
  expect(saveCalls(host)).toHaveLength(0);
});

test("weather uses explicit Windows permission then city disambiguation without zero coordinates or automatic writes", async ({
  page,
}) => {
  const host = await open(page);
  await navigate(page, "Bar");
  expect(
    host.calls.filter((call) => call.method.startsWith("location-")),
  ).toHaveLength(0);
  await expect(page.getByText("Weather is off", { exact: true })).toBeVisible();
  host.methodErrors["location-current"] = {
    code: "denied",
    message: "Windows location access is off.",
    help: "Choose a city or open Windows location settings.",
  };
  await page
    .getByRole("button", { name: "Use current location", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText(
    "Windows location access is off",
  );
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toBeDisabled();
  await page.getByRole("button", { name: "Windows location settings" }).click();
  expect(
    host.calls.some((call) => call.method === "location-open-settings"),
  ).toBe(true);
  await page.getByRole("searchbox", { name: "City or town" }).fill("Sydney");
  await page.getByRole("button", { name: "Find city", exact: true }).click();
  await expect(
    page.getByRole("button", {
      name: "Sydney, Nova Scotia, Canada",
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Sydney, Nova Scotia, Canada", exact: true })
    .scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "../../target/settings-new-controls/weather-search.png",
  });
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toBeDisabled();
  await page
    .getByRole("button", {
      name: "Sydney, New South Wales, Australia",
      exact: true,
    })
    .click();
  expect(saveCalls(host)).toHaveLength(0);
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]!.params.edits).toEqual([
    {
      op: "set",
      path: ["weather"],
      value: {
        latitude: -33.87,
        longitude: 151.21,
        location: "Sydney, New South Wales, Australia",
        "interval-seconds": 900,
      },
    },
  ]);
});

test("current location updates the existing draft in one undo step and keeps its refresh preference", async ({
  page,
}) => {
  const weather = { latitude: 50, longitude: 10, "interval-seconds": 1800 };
  const host = await open(page, { weather });
  await navigate(page, "Bar");
  await expect(
    page.getByRole("heading", { name: "Custom location", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Use current location", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Current location", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Undo last change", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Custom location", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Use current location", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Current location", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]!.params.edits).toEqual([
    {
      op: "set",
      path: ["weather"],
      value: {
        ...weather,
        latitude: host.location.latitude,
        longitude: host.location.longitude,
        location: host.location.label,
      },
    },
  ]);
});

test("a delayed location result cannot overwrite a newer weather choice", async ({
  page,
}) => {
  const host = await open(page, {
    weather: { latitude: 50, longitude: 10, "interval-seconds": 900 },
  });
  await navigate(page, "Bar");
  let release!: () => void;
  host.locationGate = new Promise<void>((resolve) => {
    release = resolve;
  });
  await page
    .getByRole("button", { name: "Use current location", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Turn weather off", exact: true })
    .click();
  release();
  await expect(page.getByRole("alert")).toContainText("weather draft changed");
  await expect(page.getByText("Weather is off", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]!.params.edits).toEqual([
    { op: "reset", path: ["weather"] },
  ]);
});

test("workspace icon picker loads the bundled font and preserves unrelated workspace settings", async ({
  page,
}) => {
  const workspace = {
    name: "Work",
    icon: "\uf0b1",
    monitor: "Desk",
    color: "terracotta",
    gaps: { inner: 12, outer: 20 },
  };
  const host = await open(page, {
    workspaces: [workspace, { name: "Personal" }],
  });
  await navigate(page, "Workspaces");
  const input = page.getByLabel("Icon for workspace Work", { exact: true });
  await expect(input).toHaveValue("\uf0b1");
  await page
    .getByRole("button", { name: "Choose icon", exact: true })
    .first()
    .click();
  const glyph = page.getByRole("button", { name: "Code", exact: true });
  await expect(glyph).toBeVisible();
  expect(
    await page.evaluate(async () => {
      await document.fonts.load('24px "Moon Symbols"', "\uf121");
      return [...document.fonts].some(
        (font) => font.family === "Moon Symbols" && font.status === "loaded",
      );
    }),
  ).toBe(true);
  await page.screenshot({
    path: "../../target/settings-new-controls/icon-picker.png",
  });
  await glyph.click();
  await expect(input).toHaveValue("\uf121");
  await expect(
    page.getByLabel("Icon for workspace Work preview", { exact: true }),
  ).toHaveText("\uf121");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]!.params.edits).toEqual([
    { op: "set", path: ["workspaces", 0, "icon"], value: "\uf121" },
  ]);
});

test("common app rules use behavior and workspace choices while custom rules remain untouched", async ({
  page,
}) => {
  const custom = {
    commands: ["set-floating", "move-to-workspace 1"],
    matches: [{ title: { regex: "special.*" }, class: { equals: "Custom" } }],
  };
  const host = await open(page, {
    workspaces: [{ name: "1" }, { name: "Writing room" }],
    rules: [personalRule, custom],
  });
  await navigate(page, "Rules");
  await page.getByLabel("App name", { exact: true }).fill("Code");
  await page
    .getByLabel("App behavior", { exact: true })
    .first()
    .selectOption("workspace");
  await page
    .getByLabel("Destination workspace", { exact: true })
    .first()
    .selectOption("Writing room");
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]!.params.edits).toEqual([
    {
      op: "set",
      path: ["rules"],
      value: [
        {
          commands: ['move-to-workspace "Writing room"'],
          matches: [{ process: { equals: "Code" } }],
        },
        custom,
      ],
    },
  ]);
});

test("bar cards preserve custom commands and styling across order and alignment edits", async ({
  page,
}) => {
  const custom = {
    id: "custom",
    format: "{custom}",
    align: "left",
    exec: "echo safe",
    click: "my-alias",
    background: "surface0",
    icon: "\uf121",
    "icon-color": "terracotta",
    "interval-seconds": 30,
    "max-width": 444,
  };
  const clock = {
    id: "clock",
    format: "{clock}",
    align: "right",
    "interval-seconds": 5,
    "max-width": 300,
  };
  const host = await open(page, {
    bar: { ...defaults.bar, widgets: [custom, clock] },
  });
  await navigate(page, "Bar");
  await page.locator(".widget-summary").first().click();
  await page
    .getByLabel("Position of Custom text", { exact: true })
    .selectOption("center");
  await page
    .getByRole("button", { name: "Move later: Custom text", exact: true })
    .click();
  await expect(page.locator(".widget-summary strong")).toHaveText([
    "Clock",
    "Custom text",
  ]);
  await expect(page.locator(".widget-summary").nth(1)).toBeFocused();
  await page.getByRole("button", { name: "Save changes", exact: true }).click();
  await expect.poll(() => saveCalls(host).length).toBe(1);
  expect(saveCalls(host)[0]!.params.edits).toEqual([
    {
      op: "set",
      path: ["bar", "widgets"],
      value: [clock, { ...custom, align: "center" }],
    },
  ]);
});

for (const width of [320, 740])
  test(`new controls fit ${width}px with expanded cards and visible save actions`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 740 });
    await open(page, { rules: [personalRule] });
    for (const theme of ["light", "dark"] as const) {
      await page.emulateMedia({ colorScheme: theme });
      for (const name of ["General", "Bar", "Workspaces", "Rules"]) {
        await navigate(page, name);
        if (name === "Bar")
          await page.locator(".widget-summary").first().click();
        expect(
          await page.evaluate(
            () => document.documentElement.scrollWidth <= innerWidth,
          ),
        ).toBe(true);
        await page.screenshot({
          path: `../../target/settings-new-controls/${name.toLowerCase()}-${theme}-${width}.png`,
        });
      }
    }
  });

test("weather source credits open only fixed native attribution destinations without editing the draft", async ({
  page,
}) => {
  const host = await open(page);
  await navigate(page, "Bar");
  await expect(page.getByText("Weather data:", { exact: false })).toContainText(
    "CC BY 4.0",
  );
  expect(
    host.calls.filter((call) => call.method === "location-attribution"),
  ).toHaveLength(0);
  await page.getByRole("button", { name: "Open-Meteo", exact: true }).click();
  await page.getByRole("button", { name: "GeoNames", exact: true }).click();
  expect(
    host.calls
      .filter((call) => call.method === "location-attribution")
      .map((call) => call.params),
  ).toEqual([{ provider: "open-meteo" }, { provider: "geonames" }]);
  expect(saveCalls(host)).toHaveLength(0);
  await expect(
    page.getByRole("button", { name: "Save changes", exact: true }),
  ).toBeDisabled();
});

test("bundled icon bytes preload on General before opening the first picker", async ({
  page,
}) => {
  const font = page.waitForResponse(
    (response) => new URL(response.url()).pathname === "/moon-symbols.ttf",
  );
  await open(page);
  await expect(
    page.getByRole("heading", { name: "General", exact: true }),
  ).toBeVisible();
  const response = await font;
  expect(response.status()).toBe(200);
  expect(response.request().resourceType()).toBe("font");
  expect(await response.body()).toEqual(
    await readFile(new URL("../dist/moon-symbols.ttf", import.meta.url)),
  );
});
