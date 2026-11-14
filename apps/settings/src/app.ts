export {};
type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
type Obj = { [key: string]: Json };
type Path = (string | number)[];
type Edit =
  { op: "set"; path: Path; value: Json } | { op: "reset"; path: Path };
type Snapshot = {
  source: string | null;
  config: Obj;
  overrides: Obj;
  defaults: Obj;
  schema: Obj;
};
type Loaded = {
  snapshot: Snapshot;
  path: string;
  version: string;
  runtime: { running: boolean; message: string; stats?: Obj };
};
type LiveShortcuts = {
  hook_enabled: boolean;
  mode: string;
  paused: boolean;
  emergency: string;
  bindings: { chord: string; commands: string[] }[];
};
type Overview = {
  managed_windows: number;
  active_workspace: string | null;
  workspaces: { name: string; windows: number; active: boolean }[];
};
type ApplicationState = {
  state:
    | "loaded"
    | "paused"
    | "suspended"
    | "pending"
    | "unavailable"
    | "other-config"
    | "changed"
    | "failed";
  message: string;
};
type Reply = {
  id: number;
  ok: boolean;
  result?: unknown;
  error?: { message: string; help?: string };
};
declare global {
  interface Window {
    ipc?: { postMessage(message: string): void };
    moonStartPage?: "General" | "Shortcuts";
    moonReceive: (reply: Reply) => void;
    moonCloseRequested: () => void;
    moonActivated: () => void;
  }
}
let sequence = 0;
const pending = new Map<
  number,
  {
    resolve(value: unknown): void;
    reject(reason: Error): void;
    timer: ReturnType<typeof setTimeout>;
  }
>();
window.moonReceive = (reply) => {
  const call = pending.get(reply.id);
  if (!call) return;
  clearTimeout(call.timer);
  pending.delete(reply.id);
  reply.ok
    ? call.resolve(reply.result)
    : call.reject(
        new Error(
          [reply.error?.message || "Something went wrong.", reply.error?.help]
            .filter(Boolean)
            .join(" "),
        ),
      );
};
async function request<T>(method: string, params: unknown = {}): Promise<T> {
  if (!window.ipc)
    throw new Error(
      "Open Moon Settings from the Start menu to change your settings.",
    );
  return new Promise((resolve, reject) => {
    const id = ++sequence;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(
        new Error(
          "Moon is taking longer than expected. Your unsaved changes are still here. Try again.",
        ),
      );
    }, 20000);
    const message = JSON.stringify({ id, method, params });
    if (new TextEncoder().encode(message).length > 4 * 1024 * 1024) {
      clearTimeout(timer);
      reject(
        new Error(
          "This change is too large to save in Settings. Reduce the configuration size and try again.",
        ),
      );
      return;
    }
    pending.set(id, { resolve: (value) => resolve(value as T), reject, timer });
    window.ipc!.postMessage(message);
  });
}
const object = (value: Json | undefined): Obj =>
  value && !Array.isArray(value) && typeof value === "object" ? value : {};
const array = (value: Json | undefined): Json[] =>
  Array.isArray(value) ? value : [];
const clone = <T>(value: T): T => structuredClone(value);
function get(root: Json, path: Path): Json {
  let value: Json | undefined = root;
  for (const key of path) value = (value as Obj)?.[key];
  return value ?? null;
}
function put(root: Obj, path: Path, value: Json): void {
  let cursor = root;
  for (let i = 0; i < path.length - 1; i++) {
    const key = path[i]!;
    cursor[key] ??= typeof path[i + 1] === "number" ? [] : {};
    cursor = cursor[key] as Obj;
  }
  cursor[path.at(-1)!] = clone(value);
}
function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className = "",
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}
function button(
  label: string,
  action: () => void | Promise<void>,
  className = "",
): HTMLButtonElement {
  const node = el("button", className, label);
  node.type = "button";
  node.addEventListener("click", () => {
    void Promise.resolve().then(action).catch(showError);
  });
  return node;
}
const icons: Record<string, string> = {
  General: "M5 6h14M5 12h14M5 18h14M9 3v6M15 9v6M10 15v6",
  Appearance:
    "M12 3a9 9 0 1 0 9 9c0-2-2-2-4-2h-2a2 2 0 0 1-2-2V5c0-1-1-2-1-2ZM7 9h.01M7 14h.01M11 17h.01",
  Workspaces: "M3 4h8v7H3zM14 4h7v7h-7zM3 14h8v7H3zM14 14h7v7h-7z",
  Shortcuts:
    "M3 6h18v13H3zM6 10h.01M10 10h.01M14 10h.01M18 10h.01M6 14h.01M9 16h8M18 14h.01",
  Rules: "M4 5h6v6H4zM14 14h6v6h-6zM10 8h7v6M7 11v6h7",
  Advanced: "M7 4 3 8l4 4M17 12l4 4-4 4M14 3l-4 18",
  About: "M12 11v6M12 7h.01M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z",
};
function icon(name: string): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  const p = document.createElementNS(svg.namespaceURI, "path");
  p.setAttribute("d", icons[name] ?? icons.About!);
  svg.append(p);
  return svg;
}
function mark(): HTMLImageElement {
  const image = el("img");
  image.src = "/moon.svg";
  image.alt = "";
  image.className = "moon-mark";
  return image;
}

const pages = Object.keys(icons);
const descriptions: Record<string, string> = {
  General:
    "Make room for the way you work. Set how windows move, focus, and fit together.",
  Appearance:
    "A quieter desktop, made yours. Fine-tune the bar, borders, and color.",
  Workspaces:
    "Give each part of your day a place. Names, icons, and monitor assignments stay together.",
  Shortcuts:
    "Find the shortcuts active on your desktop, or edit your saved bindings below.",
  Rules:
    "Let your apps settle into the right place. Personal rules are applied after Moon’s built-in rules.",
  Advanced:
    "Fine-grained options for monitors, widgets, and automation. Changes are checked before saving.",
  About: "A little more space to focus.",
};
let loaded: Loaded;
let draft: Obj;
let edits: Edit[] = [];
let active = window.moonStartPage === "Shortcuts" ? "Shortcuts" : "General";
let busy = false;
let busyLabel = "Saving your changes…";
const invalidFields = new Map<string, string>();
const fieldPages = new Map<string, string>();
let confirmationOpen = false;
let errorMessage: string | undefined;
let search = "";
let root = document.querySelector<HTMLDivElement>("#app")!;
let content: HTMLElement;
let footer: HTMLElement;
let status: HTMLElement;
let nav: HTMLElement;
let errorBox: HTMLElement;
let undoDraft:
  { draft: Obj; edits: Edit[]; invalid: [string, string][] } | undefined;
let application: ApplicationState | undefined;
let checkingApplication = false;
let applicationTimer: ReturnType<typeof setTimeout> | undefined;
let shortcuts: LiveShortcuts | undefined;
let shortcutsError: string | undefined;
let shortcutsLoading = false;
let shortcutFilter = "";
let shortcutPanel: HTMLElement | undefined;
let overview: Overview | undefined;
let overviewError: string | undefined;
let overviewLoading = false;
let overviewPanel: HTMLElement | undefined;
let toastTimer: ReturnType<typeof setTimeout> | undefined;
function toast(message: string): void {
  document.querySelector(".toast")?.remove();
  clearTimeout(toastTimer);
  const n = el("div", "toast", message);
  n.setAttribute("role", "status");
  document.body.append(n);
  toastTimer = setTimeout(() => n.remove(), 6000);
}
function showError(error: unknown): void {
  const message = error instanceof Error ? error.message : String(error);
  errorMessage = message;
  if (errorBox) {
    errorBox.textContent = message;
    errorBox.hidden = false;
    errorBox.focus();
  } else {
    root.replaceChildren(
      el("div", "notice error", message),
      button("Try again", boot),
    );
  }
}
function updateFooter(): void {
  footer.replaceChildren();
  const label = el("p");
  label.append(
    el(
      "strong",
      "",
      busy
        ? busyLabel
        : invalidFields.size
          ? "Fix invalid fields to save"
          : edits.length
            ? "Unsaved changes"
            : "All changes saved",
    ),
  );
  label.append(
    document.createTextNode(
      edits.length
        ? " · Apply when you’re ready."
        : " · Settings are stored on this PC.",
    ),
  );
  const actions = el("div", "actions");
  if (invalidFields.size) {
    const review = button("Review errors", reviewErrors, "quiet");
    review.disabled = busy;
    actions.append(review);
  }
  const discard = button("Discard", discardChanges);
  discard.disabled = (!edits.length && !invalidFields.size) || busy;
  const apply = button("Save changes", save, "primary");
  apply.disabled = !edits.length || busy || invalidFields.size > 0;
  const reload = button("Reload", reloadSettings, "quiet");
  reload.title = "Reload saved settings from disk";
  reload.disabled = busy;
  const undo = button("Undo", undoLastChange, "quiet");
  undo.title = "Undo the last draft change";
  undo.setAttribute("aria-label", "Undo last change");
  undo.disabled = !undoDraft || busy;
  actions.append(undo, reload, discard, apply);
  footer.append(label, actions);
}
function reviewErrors(): void {
  const key = invalidFields.keys().next().value;
  if (!key) return;
  switchPage(fieldPages.get(key) ?? "Advanced");
  const input = Array.from(
    content.querySelectorAll<HTMLElement>("[data-field-path]"),
  ).find((node) => node.dataset.fieldPath === key);
  if (input) {
    const details = input.closest("details");
    if (details) details.open = true;
    input.focus();
  }
}
function rememberDraft(): void {
  // One checkpoint only: memory is bounded by the current editable document.
  undoDraft = {
    draft: clone(draft),
    edits: clone(edits),
    invalid: [...invalidFields],
  };
}
function undoLastChange(): void {
  if (!undoDraft || busy || confirmationOpen) return;
  const previous = undoDraft;
  undoDraft = undefined;
  draft = previous.draft;
  edits = previous.edits;
  invalidFields.clear();
  for (const [key, value] of previous.invalid) invalidFields.set(key, value);
  errorMessage = undefined;
  renderPage();
  updateFooter();
  content.querySelector<HTMLElement>("h1")?.focus();
  toast("Undid the last draft change. Saved settings are unchanged.");
}
function stage(path: Path, value: Json): void {
  if (JSON.stringify(value) !== JSON.stringify(get(draft, path)))
    rememberDraft();
  queueEdit(path, value);
}
function queueEdit(path: Path, value: Json): void {
  put(draft, path, value);
  const ancestor = edits.find(
    (e) => e.path.length < path.length && e.path.every((p, i) => p === path[i]),
  );
  if (ancestor) {
    const parentPath = ancestor.path;
    edits = edits.filter((e) => e !== ancestor);
    queueEdit(parentPath, get(draft, parentPath));
    return;
  }
  edits = edits.filter((e) => !path.every((p, i) => p === e.path[i]));
  if (
    JSON.stringify(value) !== JSON.stringify(get(loaded.snapshot.config, path))
  )
    edits.push(
      value === null
        ? { op: "reset", path }
        : { op: "set", path, value: clone(value) },
    );
  updateFooter();
}
function resetField(path: Path): void {
  rememberDraft();
  edits = edits.filter((e) => !path.every((p, i) => p === e.path[i]));
  edits.push({ op: "reset", path });
  put(draft, path, get(loaded.snapshot.defaults, path));
  for (const key of invalidFields.keys()) {
    const invalidPath = JSON.parse(key) as Path;
    if (path.every((part, index) => part === invalidPath[index]))
      invalidFields.delete(key);
  }
  updateFooter();
  renderPage();
}
async function confirmAction(
  title: string,
  text: string,
  accept: string,
  destructive = false,
): Promise<boolean> {
  if (confirmationOpen) return false;
  confirmationOpen = true;
  return new Promise((resolve) => {
    const dialog = el("dialog");
    dialog.setAttribute("aria-labelledby", "dialog-title");
    const heading = el("h2", "", title);
    heading.id = "dialog-title";
    const description = el("p", "", text);
    description.id = "dialog-description";
    dialog.setAttribute("aria-describedby", description.id);
    const actions = el("div", "actions");
    const finish = (value: boolean) => {
      confirmationOpen = false;
      dialog.close();
      dialog.remove();
      resolve(value);
    };
    const cancel = button("Cancel", () => finish(false));
    actions.append(
      cancel,
      button(accept, () => finish(true), destructive ? "danger" : "primary"),
    );
    dialog.append(heading, description, actions);
    dialog.addEventListener("cancel", (event) => {
      event.preventDefault();
      finish(false);
    });
    document.body.append(dialog);
    dialog.showModal();
    cancel.focus();
  });
}
async function discardChanges(): Promise<void> {
  if (
    (edits.length || invalidFields.size) &&
    !(await confirmAction(
      "Discard your changes?",
      "Your saved settings will stay as they are.",
      "Discard changes",
      true,
    ))
  )
    return;
  draft = clone(loaded.snapshot.config);
  edits = [];
  undoDraft = undefined;
  invalidFields.clear();
  errorMessage = undefined;
  errorBox.hidden = true;
  renderPage();
  updateFooter();
}
async function reloadSettings(): Promise<void> {
  if (busy) return;
  if (
    (edits.length || invalidFields.size) &&
    !(await confirmAction(
      "Reload saved settings?",
      "This discards your draft and reads the latest settings from disk.",
      "Reload settings",
      true,
    ))
  )
    return;
  busy = true;
  busyLabel = "Reloading saved settings…";
  content.inert = true;
  updateFooter();
  try {
    const next = await request<Loaded>("load");
    loaded = next;
    draft = clone(next.snapshot.config);
    edits = [];
    undoDraft = undefined;
    application = undefined;
    clearTimeout(applicationTimer);
    invalidFields.clear();
    errorMessage = undefined;
    renderPage();
    updateStatus();
    toast("Loaded the latest saved settings.");
  } finally {
    busy = false;
    content.inert = false;
    updateFooter();
  }
}

async function save(): Promise<void> {
  if (
    !loaded ||
    busy ||
    confirmationOpen ||
    !edits.length ||
    invalidFields.size
  )
    return;
  const focusedId = document.activeElement?.id;
  const focusedPage = active;
  let failure: unknown;
  busy = true;
  busyLabel = "Saving your changes…";
  content.inert = true;
  updateFooter();
  errorBox.hidden = true;
  try {
    const result = await request<{ data: Loaded; message: string }>("save", {
      expected_source: loaded.snapshot.source,
      edits,
    });
    loaded = result.data;
    draft = clone(loaded.snapshot.config);
    edits = [];
    undoDraft = undefined;
    application = {
      state: "pending",
      message: "Saved on this PC. Checking the active desktop settings…",
    };
    clearTimeout(applicationTimer);
    applicationTimer = setTimeout(() => {
      void checkApplication();
    }, 1200);
    errorMessage = undefined;
    renderPage();
    updateStatus();
    toast(result.message);
  } catch (e) {
    failure = e;
  } finally {
    busy = false;
    content.inert = false;
    updateFooter();
  }
  if (failure) showError(failure);
  else
    (focusedId && active === focusedPage
      ? document.getElementById(focusedId)
      : content.querySelector("h1")
    )?.focus();
}
window.moonCloseRequested = () => {
  if (busy || confirmationOpen) return;
  void (async () => {
    if (
      !busy &&
      ((!edits.length && !invalidFields.size) ||
        (await confirmAction(
          "Leave without saving?",
          "Your changes have not been saved. You can keep editing or discard them and close Settings.",
          "Discard and close",
          true,
        )))
    )
      await request("close");
  })().catch(showError);
};
function updateStatus(): void {
  status.replaceChildren();
  const line = el("div");
  line.append(
    el("span", `status-dot ${loaded.runtime.running ? "running" : ""}`),
    document.createTextNode(
      loaded.runtime.running ? "Moon is running" : "Moon is not running",
    ),
  );
  status.append(line, el("div", "", `Version ${loaded.version}`));
}
function switchPage(name: string): void {
  active = name;
  search = "";
  (document.querySelector("#search") as HTMLInputElement).value = "";
  renderPage();
  refreshVisibleHelp();
  document.querySelector<HTMLElement>("h1")?.focus();
}
function renderShell(): void {
  root.replaceChildren();
  const shell = el("div", "shell");
  const side = el("aside", "sidebar");
  const brand = el("div", "brand");
  const name = el("div");
  name.append(el("strong", "", "moon"), el("small", "", "SETTINGS"));
  brand.append(mark(), name);
  const find = el("input", "search");
  find.id = "search";
  find.type = "search";
  find.placeholder = "Find a setting";
  find.setAttribute("aria-label", "Find a setting");
  find.addEventListener("input", () => {
    search = find.value;
    renderPage();
  });
  nav = el("nav", "nav");
  nav.setAttribute("aria-label", "Settings pages");
  pages.forEach((page) => {
    const b = button(page, () => switchPage(page));
    b.prepend(icon(page));
    b.dataset.page = page;
    nav.append(b);
  });
  status = el("div", "sidebar-foot");
  side.append(brand, find, nav, status);
  const main = el("main", "main");
  const scroll = el("div", "scroll");
  content = el("div", "page");
  scroll.append(content);
  footer = el("footer", "savebar");
  main.append(scroll, footer);
  shell.append(side, main);
  root.append(shell);
  renderPage();
  updateFooter();
  updateStatus();
}
function section(title: string): HTMLElement {
  content.append(el("h2", "", title));
  const card = el("div", "card");
  content.append(card);
  return card;
}
let nextField = 0;
type FieldOptions = {
  type?: "text" | "number" | "checkbox" | "color";
  min?: number;
  max?: number;
  step?: number;
  choices?: string[];
  placeholder?: string;
};
function field(
  parent: HTMLElement,
  label: string,
  detail: string,
  path: Path,
  options: FieldOptions = {},
): void {
  const row = el("div", "row");
  const copy = el("div", "copy");
  const title = el("label", "", label);
  const id = `field-${++nextField}`;
  title.htmlFor = id;
  copy.append(title);
  const desc = el("div", "description", detail);
  desc.id = `${id}-hint`;
  if (detail) copy.append(desc);
  const control = el("div", "control");
  const value = get(draft, path);
  const input = options.choices ? el("select") : el("input");
  input.id = id;
  const fieldKey = JSON.stringify(path);
  input.dataset.fieldPath = fieldKey;
  fieldPages.set(fieldKey, active);
  input.setAttribute("aria-describedby", desc.id);
  if (input instanceof HTMLSelectElement)
    options.choices!.forEach((option) => {
      const node = el("option", "", option[0]!.toUpperCase() + option.slice(1));
      node.value = option;
      input.append(node);
    });
  else {
    input.type = options.type ?? "text";
    if (input.type === "checkbox") {
      input.checked = value === true;
      input.setAttribute("role", "switch");
    }
    if (options.min !== undefined) input.min = String(options.min);
    if (options.max !== undefined) input.max = String(options.max);
    if (options.step !== undefined) input.step = String(options.step);
    input.placeholder = options.placeholder ?? "";
    if (input.type === "number") input.required = true;
  }
  if (!(input instanceof HTMLInputElement && input.type === "checkbox"))
    input.value = value === null ? "" : String(value);
  if (invalidFields.has(fieldKey)) {
    input.value = invalidFields.get(fieldKey)!;
    input.setAttribute("aria-invalid", "true");
    if (input instanceof HTMLInputElement)
      desc.textContent = input.validationMessage;
  }
  input.addEventListener("input", () => {
    rememberDraft();
    if (input instanceof HTMLInputElement && !input.checkValidity()) {
      invalidFields.set(JSON.stringify(path), input.value);
      input.setAttribute("aria-invalid", "true");
      desc.textContent = input.validationMessage;
      updateFooter();
      return;
    }
    invalidFields.delete(JSON.stringify(path));
    input.removeAttribute("aria-invalid");
    desc.textContent = detail;
    const v =
      input instanceof HTMLInputElement && input.type === "checkbox"
        ? input.checked
        : options.type === "number"
          ? Number(input.value)
          : input.value;
    queueEdit(path, v);
    if (active === "Appearance") updatePreview();
  });
  control.append(input);
  if (
    path[0] === "borders" &&
    ["focused", "focused-end", "unfocused"].includes(String(path[1]))
  ) {
    const picker = el("input");
    picker.type = "color";
    picker.value = resolveColor(value, "#d17a4f");
    picker.setAttribute("aria-label", `Choose ${label.toLowerCase()}`);
    picker.addEventListener("input", () => {
      input.value = picker.value;
      stage(path, picker.value);
      updatePreview();
    });
    control.append(picker);
  }
  row.append(copy, control);
  parent.append(row);
}
function simple(
  label: string,
  value: Json,
  changed: (value: string) => void,
  options: {
    wide?: boolean;
    multiline?: boolean;
    placeholder?: string;
    type?: string;
  } = {},
): HTMLLabelElement {
  const node = el("label", options.wide ? "wide" : "", label);
  const input = options.multiline ? el("textarea") : el("input");
  if (input instanceof HTMLInputElement) input.type = options.type ?? "text";
  input.value = value === null ? "" : String(value);
  input.placeholder = options.placeholder ?? "";
  input.addEventListener("input", () => changed(input.value));
  node.append(input);
  return node;
}
function resetButton(parent: HTMLElement, label: string, path: Path): void {
  parent.append(
    button(
      label,
      async () => {
        if (
          await confirmAction(
            "Restore the defaults?",
            "This changes your draft. Save changes when you’re ready to apply it.",
            "Restore defaults",
          )
        )
          resetField(path);
      },
      "quiet",
    ),
  );
}
function readableChord(chord: string): string {
  const names: Record<string, string> = {
    ctrl: "Ctrl",
    control: "Ctrl",
    alt: "Alt",
    shift: "Shift",
    win: "Win",
    super: "Win",
    meta: "Win",
    space: "Space",
    escape: "Esc",
    enter: "Enter",
    left: "←",
    right: "→",
    up: "↑",
    down: "↓",
    oem_question: "/",
    oem_plus: "Plus",
    oem_minus: "Minus",
    oem_comma: ",",
    oem_period: ".",
  };
  return chord
    .split("+")
    .map((key) => names[key.toLowerCase()] ?? key.toUpperCase())
    .join(" + ");
}
function describeCommand(command: string): string {
  const names: Record<string, string> = {
    "toggle-direction": "Change the next split direction",
    "toggle-floating": "Switch between floating and tiled",
    "toggle-fullscreen": "Toggle fullscreen",
    "set-floating": "Float this window",
    "set-tiling": "Tile this window",
    "recent-workspace": "Return to the previous workspace",
    pause: "Pause or resume Moon",
    reload: "Reload saved settings",
    exit: "Stop Moon and restore windows",
    restore: "Restore original window positions and stop Moon",
    close: "Close the focused window",
    settings: "Open Settings",
    cheatsheet: "Show current shortcuts",
  };
  if (names[command]) return names[command]!;
  const words = command.split(/\s+/);
  if (words.length === 2 && words[0] === "focus")
    return `Focus the ${words[1]} window`;
  if (words.length === 2 && words[0] === "move")
    return `Move the window ${words[1]}`;
  if (words.length === 2 && words[0] === "workspace")
    return `Switch to workspace ${words[1]}`;
  if (
    words[0] === "move-to-workspace" &&
    (words.length === 2 || (words.length === 3 && words[2] === "--follow"))
  )
    return `Move to workspace ${words[1]}${words[2] ? " and follow" : ""}`;
  if (words.length === 3 && words[0] === "resize")
    return `Change window ${words[1]} by ${words[2]}`;
  if (words.length === 2 && words[0] === "mode") return `Use ${words[1]} mode`;
  return command; // Custom commands remain exact, with no invented interpretation.
}
function paintShortcuts(): void {
  if (!shortcutPanel?.isConnected) return;
  shortcutPanel.replaceChildren();
  const heading = document.querySelector("#shortcut-status-heading");
  if (heading)
    heading.textContent =
      shortcuts?.hook_enabled === false
        ? "Configured shortcuts"
        : "Available now";
  const refresh =
    document.querySelector<HTMLButtonElement>("#refresh-shortcuts");
  if (refresh) refresh.disabled = shortcutsLoading;
  if (shortcutsLoading && !shortcuts) {
    shortcutPanel.append(el("p", "note", "Loading current shortcuts…"));
    return;
  }
  if (!shortcuts) {
    const notice = el(
      "div",
      "notice",
      "Current shortcuts are unavailable. Start Moon, then refresh this list.",
    );
    if (shortcutsError) notice.append(el("div", "note", shortcutsError));
    shortcutPanel.append(notice);
    return;
  }
  shortcutPanel.append(
    el(
      "p",
      shortcuts.hook_enabled ? "note" : "notice",
      `Mode: ${shortcuts.mode}${!shortcuts.hook_enabled ? " · Configured shortcuts are disabled for this Moon session. The recovery shortcut above remains available." : shortcuts.paused ? " · Moon is paused; only the available recovery controls are listed." : " · These shortcuts are active on your desktop now."}`,
    ),
  );
  const terms = shortcutFilter.toLocaleLowerCase().trim().split(/\s+/);
  const entries = shortcuts.bindings.filter(
    ({ chord, commands }) =>
      commands.length > 0 &&
      terms.every((term) =>
        `${chord} ${readableChord(chord)} ${commands.join(" ")} ${commands.map(describeCommand).join(" ")}`
          .toLocaleLowerCase()
          .includes(term),
      ),
  );
  const list = el("div", "card live-shortcuts");
  list.setAttribute("role", "list");
  list.setAttribute(
    "aria-label",
    shortcuts.hook_enabled
      ? "Available shortcuts"
      : "Configured shortcuts (inactive)",
  );
  for (const entry of entries) {
    const row = el("div", "live-shortcut-row");
    row.setAttribute("role", "listitem");
    const actions = el("div");
    for (const command of entry.commands)
      actions.append(el("div", "", describeCommand(command)));
    row.append(el("kbd", "", readableChord(entry.chord)), actions);
    list.append(row);
  }
  if (!entries.length)
    shortcutPanel.append(
      el(
        "div",
        "empty",
        shortcutFilter.trim()
          ? "No current shortcuts match. Try a key, action, or workspace name."
          : "No shortcuts are active in this mode. The recovery shortcut above is always available.",
      ),
    );
  else shortcutPanel.append(list);
}
async function refreshShortcuts(): Promise<void> {
  if (shortcutsLoading) return;
  shortcutsLoading = true;
  paintShortcuts();
  try {
    shortcuts = await request<LiveShortcuts>("shortcuts");
    shortcutsError = undefined;
  } catch (error) {
    shortcuts = undefined;
    shortcutsError = error instanceof Error ? error.message : String(error);
  } finally {
    shortcutsLoading = false;
    paintShortcuts();
  }
}
function renderShortcutHelp(parent: HTMLElement): void {
  const heading = el("h2", "", "Available now");
  heading.id = "shortcut-status-heading";
  parent.append(heading);
  const recovery = el("div", "notice");
  recovery.append(
    el("strong", "", "Always a way back"),
    el("div", "", "Restore original window positions and stop Moon:"),
    el("kbd", "", readableChord(shortcuts?.emergency ?? "ctrl+alt+shift+f12")),
  );
  const toolbar = el("div", "toolbar live-help-toolbar");
  const search = el("input", "search");
  search.type = "search";
  search.id = "shortcut-search";
  search.placeholder = "Find a shortcut";
  search.setAttribute("aria-label", "Find a shortcut");
  search.value = shortcutFilter;
  search.addEventListener("input", () => {
    shortcutFilter = search.value;
    paintShortcuts();
  });
  const refresh = button("Refresh", refreshShortcuts);
  refresh.id = "refresh-shortcuts";
  refresh.setAttribute("aria-label", "Refresh current shortcuts");
  toolbar.append(search, refresh);
  shortcutPanel = el("div");
  parent.append(recovery, toolbar, shortcutPanel);
  paintShortcuts();
  if (!shortcuts && !shortcutsError && !shortcutsLoading)
    void refreshShortcuts();
}
function renderHelpOnly(error: unknown): void {
  root.replaceChildren();
  const page = el("main", "page help-only");
  page.append(
    mark(),
    el("h1", "", "Shortcuts"),
    el("p", "lead", "Help for the desktop you’re using now."),
  );
  const notice = el(
    "div",
    "notice error",
    "Saved settings could not be opened. Current shortcut help remains available.",
  );
  notice.append(
    el("div", "note", error instanceof Error ? error.message : String(error)),
  );
  errorBox = el("div", "notice error");
  errorBox.hidden = true;
  errorBox.setAttribute("role", "alert");
  errorBox.tabIndex = -1;
  page.append(notice, errorBox);
  root.append(page);
  renderShortcutHelp(page);
  const actions = el("div", "actions");
  actions.append(
    button("Try settings again", boot),
    button("Open configuration", () => request("open-config")),
  );
  page.append(actions);
}
function paintOverview(): void {
  if (!overviewPanel?.isConnected) return;
  overviewPanel.replaceChildren();
  const refresh = button("Refresh", refreshOverview, "quiet");
  refresh.setAttribute("aria-label", "Refresh workspace overview");
  refresh.disabled = overviewLoading;
  if (overview) {
    overviewPanel.append(
      el(
        "strong",
        "",
        `${overview.managed_windows} managed ${overview.managed_windows === 1 ? "window" : "windows"}`,
      ),
      el(
        "p",
        "note",
        overview.active_workspace === null
          ? "No workspace is active."
          : `Current workspace: ${overview.active_workspace}`,
      ),
    );
    const list = el("ul", "workspace-summary");
    for (const workspace of overview.workspaces.filter((w) => w.windows > 0)) {
      list.append(
        el(
          "li",
          "",
          `${workspace.name}: ${workspace.windows} ${workspace.windows === 1 ? "window" : "windows"}${workspace.active ? " · active workspace" : ""}`,
        ),
      );
    }
    overviewPanel.append(
      list,
      el(
        "p",
        "note",
        "Moon arranges existing windows using your workspace rules. While tiling is active, Moon keeps other workspaces out of view. Focusing an app brings its workspace into view; its workspace position is kept.",
      ),
    );
    if (object(loaded.runtime.stats?.token).ui_access === false)
      overviewPanel.append(
        el(
          "p",
          "note",
          "This copy of Moon runs with normal permissions. Windows from administrator apps may remain outside its control.",
        ),
      );
  } else
    overviewPanel.append(
      el(
        "p",
        "note",
        overviewLoading
          ? "Reading your current workspaces…"
          : (overviewError ??
              "Start Moon to see where your windows are arranged."),
      ),
    );
  const actions = el("div", "actions");
  actions.append(
    refresh,
    button("Workspace settings", () => switchPage("Workspaces"), "quiet"),
    button("Find a shortcut", () => switchPage("Shortcuts"), "quiet"),
  );
  overviewPanel.append(actions);
}
async function refreshOverview(): Promise<void> {
  if (overviewLoading) return;
  overviewLoading = true;
  paintOverview();
  try {
    overview = await request<Overview>("overview");
    overviewError = undefined;
  } catch {
    overview = undefined;
    overviewError =
      "Moon is not reachable. Start it, then refresh to see your current workspaces.";
  } finally {
    overviewLoading = false;
    paintOverview();
  }
}
function renderOverview(): void {
  const card = section("On your desktop");
  overviewPanel = el("div", "overview");
  card.append(overviewPanel);
  paintOverview();
  if (!overview && !overviewError && !overviewLoading) void refreshOverview();
}
function renderApplicationState(): void {
  content.querySelector(".application-state")?.remove();
  if (!application) return;
  const panel = el(
    "div",
    `notice application-state${application.state === "failed" || application.state === "changed" ? " error" : ""}`,
  );
  panel.setAttribute("role", "status");
  panel.append(el("div", "", application.message));
  if (application.state !== "loaded") {
    const check = button(
      checkingApplication ? "Checking…" : "Check desktop",
      checkApplication,
      "quiet",
    );
    check.disabled = checkingApplication || busy;
    panel.append(check);
  }
  errorBox.after(panel);
}
async function checkApplication(): Promise<void> {
  if (!loaded || checkingApplication) return;
  checkingApplication = true;
  const source = loaded.snapshot.source;
  renderApplicationState();
  try {
    const next = await request<ApplicationState>("application-status", {
      expected_source: source,
    });
    if (loaded.snapshot.source === source) application = next;
  } catch (error) {
    if (loaded.snapshot.source === source)
      application = {
        state: "unavailable",
        message: `Saved on this PC. Could not confirm the desktop settings. ${error instanceof Error ? error.message : String(error)}`,
      };
  } finally {
    checkingApplication = false;
    renderApplicationState();
  }
}
function renderGeneral(): void {
  renderOverview();
  const layout = section("Workspace motion");
  field(
    layout,
    "Slide between workspaces",
    "Move the whole workspace beneath the screen. Focusing a window in another workspace brings its workspace into view. Respects Windows animation preferences.",
    ["workspace-animation"],
    { type: "checkbox" },
  );
  const behavior = section("Focus & movement");
  field(
    behavior,
    "Move the pointer with focus",
    "Keep the pointer with the window you focus using a shortcut.",
    ["cursor-jump"],
    { type: "checkbox" },
  );
  field(
    behavior,
    "Focus follows the pointer",
    "Focus a window when the pointer moves over it.",
    ["focus-follows-cursor"],
    { type: "checkbox" },
  );
  const space = section("Space between windows");
  field(
    space,
    "Inner gap",
    "Space between neighboring windows, in logical pixels.",
    ["gaps", "inner"],
    { type: "number", min: 0, max: 512 },
  );
  field(
    space,
    "Outer gap",
    "Space along the edge of the desktop, in logical pixels.",
    ["gaps", "outer"],
    { type: "number", min: 0, max: 512 },
  );
  const note = el("div", "notice");
  note.append(
    el("strong", "", "Always a way back"),
    el(
      "div",
      "",
      "Press Ctrl + Alt + Shift + F12 to restore your windows and stop Moon. Your original window positions are kept for recovery.",
    ),
  );
  content.append(note);
  if (!loaded.runtime.running) {
    const card = section("Getting started");
    const row = el("div", "row");
    row.append(
      el(
        "div",
        "copy",
        "Moon is not managing your desktop right now. Settings can still be saved.",
      ),
    );
    row.append(button("Check status", refreshStatus));
    card.append(row);
    content.append(
      el(
        "p",
        "note",
        "Start Moon from your installed setup. If another window manager is running, use Moon Setup to switch safely.",
      ),
    );
  }
}
const lightScheme = matchMedia("(prefers-color-scheme: light)");
lightScheme.addEventListener("change", () => {
  if (draft?.theme === "system") updatePreview();
});
function resolveColor(name: Json, fallback: string): string {
  const color = String(name ?? "");
  const light =
    draft.theme === "light" ||
    (draft.theme === "system" && lightScheme.matches);
  const palette = light
    ? { ...object(draft.palette), ...object(draft["light-palette"]) }
    : object(draft.palette);
  return /^#[\da-f]{6}$/i.test(color)
    ? color
    : String(palette[color] ?? fallback);
}
function updatePreview(): void {
  const preview = document.querySelector<HTMLElement>(".preview");
  if (!preview) return;
  const bar = object(draft.bar);
  const borders = object(draft.borders);
  const miniBar = preview.querySelector<HTMLElement>(".mini-bar")!;
  miniBar.style.background = resolveColor(bar.background, "#141417");
  miniBar.style.color = resolveColor(bar.foreground, "#f0ebe0");
  miniBar.style.visibility = bar.enabled ? "visible" : "hidden";
  preview.querySelectorAll<HTMLElement>(".mini-window").forEach((window, i) => {
    const surface = resolveColor(i ? "surface0" : "base", "#fffdf9");
    const start = borders.enabled
      ? resolveColor(i ? borders.unfocused : borders.focused, "#d17a4f")
      : "transparent";
    const end =
      !i && borders.enabled
        ? resolveColor(borders["focused-end"], start)
        : start;
    window.style.borderColor = "transparent";
    window.style.background = `linear-gradient(${surface}, ${surface}) padding-box, linear-gradient(135deg, ${start}, ${end}) border-box`;
    window.style.borderWidth = `${Math.min(Number(borders.width), 6)}px`;
    window.style.borderRadius = `${Math.min(Number(borders.radius), 16)}px`;
  });
}
function renderAppearance(): void {
  const preview = el("div", "preview");
  preview.setAttribute("aria-label", "Illustrative desktop preview");
  const bar = el("div", "mini-bar");
  ["☾", "1", "2", "3", "4"].forEach((s, i) =>
    bar.append(el("span", i === 1 ? "active" : "", s)),
  );
  const layout = el("div", "mini-layout");
  for (let i = 0; i < 2; i++) {
    const window = el("div", "mini-window");
    for (let j = 0; j < 3; j++) window.append(el("div", "mini-line"));
    layout.append(window);
  }
  preview.append(bar, layout, el("div", "preview-caption", "Draft preview"));
  content.append(
    preview,
    el("p", "note preview-note", "Your desktop changes only after saving."),
  );
  content.append(
    el("h2", "", "Desktop theme"),
    el(
      "p",
      "note",
      "Choose colors for the bar and window borders. Settings follows your Windows appearance.",
    ),
  );
  const choices = el("div", "theme-choices");
  choices.setAttribute("role", "group");
  choices.setAttribute("aria-label", "Desktop theme");
  for (const theme of ["light", "dark", "system"]) {
    const b = button(
      theme[0]!.toUpperCase() + theme.slice(1),
      () => {
        stage(["theme"], theme);
        choices
          .querySelectorAll<HTMLButtonElement>("button")
          .forEach((choice) =>
            choice.setAttribute(
              "aria-pressed",
              String(choice.dataset.theme === theme),
            ),
          );
        updatePreview();
      },
      "theme-choice",
    );
    b.setAttribute("aria-pressed", String(draft.theme === theme));
    b.dataset.theme = theme;
    const swatches = el("div", "swatches");
    (theme === "light"
      ? ["#f1ede5", "#d7cfc3", "#9d4b28"]
      : theme === "dark"
        ? ["#141417", "#29292e", "#d17a4f"]
        : ["#f1ede5", "#29292e", "#d17a4f"]
    ).forEach((c) => {
      const s = el("i");
      s.style.background = c;
      swatches.append(s);
    });
    b.prepend(swatches);
    choices.append(b);
  }
  content.append(choices);
  const barCard = section("Desktop bar");
  field(
    barCard,
    "Show the bar",
    "Workspaces and everyday information at a glance.",
    ["bar", "enabled"],
    { type: "checkbox" },
  );
  field(
    barCard,
    "Height",
    "Logical pixels. Scales with your display.",
    ["bar", "height"],
    { type: "number", min: 16, max: 160 },
  );
  field(
    barCard,
    "Text size",
    "Adjust readability without changing display scaling.",
    ["bar", "font-size"],
    { type: "number", min: 6, max: 72, step: 0.5 },
  );
  field(
    barCard,
    "Background material",
    "Availability depends on Windows and transparency preferences.",
    ["bar", "backdrop"],
    { choices: ["none", "mica", "acrylic", "system"] },
  );
  field(
    barCard,
    "Opacity",
    "1 is solid; lower values allow more transparency.",
    ["bar", "opacity"],
    { type: "number", min: 0, max: 1, step: 0.05 },
  );
  const borderCard = section("Window borders");
  field(
    borderCard,
    "Show borders",
    "A subtle outline makes the focused window easy to find.",
    ["borders", "enabled"],
    { type: "checkbox" },
  );
  field(
    borderCard,
    "Border width",
    "Keep the inner gap at least this width.",
    ["borders", "width"],
    { type: "number", min: 0, max: 32 },
  );
  field(
    borderCard,
    "Corner radius",
    "Round the decorative border corners.",
    ["borders", "radius"],
    { type: "number", min: 0, max: 128 },
  );
  field(
    borderCard,
    "Focused color",
    "A palette name or a six-digit hex color.",
    ["borders", "focused"],
  );
  field(borderCard, "Gradient end", "Use the same color for a solid border.", [
    "borders",
    "focused-end",
  ]);
  field(borderCard, "Unfocused color", "Outline for other visible windows.", [
    "borders",
    "unfocused",
  ]);
  const actions = el("div", "actions");
  resetButton(actions, "Reset bar appearance", ["bar"]);
  resetButton(actions, "Reset borders", ["borders"]);
  content.append(actions);
  updatePreview();
}
function renderWorkspaces(): void {
  const values = array(draft.workspaces);
  const toolbar = el("div", "toolbar");
  toolbar.append(
    el(
      "p",
      "note",
      `${values.length} workspaces · Order follows the desktop bar.`,
    ),
    button("Add workspace", () => {
      let name = values.length + 1;
      while (values.some((v) => object(v).name === String(name))) name++;
      stage(["workspaces"], [...values, { name: String(name) }]);
      renderPage();
    }),
  );
  content.append(toolbar);
  values.forEach((entry, index) => {
    const value = object(entry);
    const card = el("div", "card list-card");
    const head = el("div", "list-head");
    head.append(el("h3", "", `Workspace ${index + 1}`));
    const actions = el("div", "actions");
    for (const [label, direction] of [
      ["Move up", -1],
      ["Move down", 1],
    ] as const) {
      const b = button(direction < 0 ? "↑" : "↓", () => {
        const list = clone(array(draft.workspaces));
        [list[index], list[index + direction]] = [
          list[index + direction]!,
          list[index]!,
        ];
        stage(["workspaces"], list);
        renderPage();
      });
      b.title = label;
      b.setAttribute("aria-label", `${label}: workspace ${String(value.name)}`);
      b.disabled = index + direction < 0 || index + direction >= values.length;
      actions.append(b);
    }
    const remove = button(
      "Remove",
      async () => {
        if (
          await confirmAction(
            "Remove this workspace?",
            "Windows on this workspace move to a remaining workspace when you save. Shortcuts and rules that name it must be updated too.",
            "Remove workspace",
            true,
          )
        ) {
          stage(
            ["workspaces"],
            array(draft.workspaces).filter((_, i) => i !== index),
          );
          renderPage();
        }
      },
      "quiet danger",
    );
    remove.disabled = values.length <= 1;
    actions.append(remove);
    head.append(actions);
    const fields = el("div", "fields");
    const set = (key: string, v: Json) => stage(["workspaces", index, key], v);
    fields.append(
      simple("Name", value.name!, (v) => set("name", v)),
      simple("Icon", value.icon ?? "", (v) => set("icon", v || null), {
        placeholder: "Optional symbol",
      }),
      simple("Color", value.color ?? "", (v) => set("color", v || null), {
        placeholder: "Palette name or #d17a4f",
      }),
      simple("Monitor", value.monitor ?? "", (v) => set("monitor", v || null), {
        placeholder: "Automatic",
      }),
    );
    card.append(head, fields);
    content.append(card);
  });
  resetButton(content, "Restore default workspaces", ["workspaces"]);
}
function renderShortcuts(): void {
  renderShortcutHelp(content);
  content.append(
    el("h2", "", "Edit saved shortcuts"),
    el(
      "p",
      "note",
      "These are your default-mode settings, including unsaved edits. The available shortcuts above come from the running desktop.",
    ),
  );
  content.append(
    el(
      "div",
      "notice",
      "Use a key chord such as alt+shift+h. Add one command per line. Clearing all commands disables that shortcut. The emergency recovery shortcut is always available.",
    ),
  );
  const toolbar = el("div", "toolbar");
  const filter = el("input", "search");
  filter.placeholder = "Filter shortcuts";
  filter.setAttribute("aria-label", "Filter shortcuts");
  toolbar.append(
    filter,
    button("Add shortcut", () => {
      const panel = el("div", "card list-card");
      const fields = el("div", "fields");
      let chord = "";
      let commands = "focus left";
      fields.append(
        simple("Key chord", "", (v) => (chord = v), {
          placeholder: "alt+shift+x",
        }),
        simple("Commands", commands, (v) => (commands = v), {
          multiline: true,
        }),
      );
      panel.append(
        fields,
        button(
          "Add",
          () => {
            if (!chord.trim()) throw new Error("Enter a key chord.");
            if (object(draft.bindings)[chord] !== undefined)
              throw new Error(
                "That shortcut already exists. Edit its row below.",
              );
            stage(
              ["bindings", chord.trim()],
              commands
                .split("\n")
                .map((s) => s.trim())
                .filter(Boolean),
            );
            renderPage();
          },
          "primary",
        ),
      );
      toolbar.after(panel);
      panel.querySelector("input")?.focus();
    }),
  );
  content.append(toolbar);
  const card = section("Keyboard shortcuts");
  const renderRows = () => {
    card.replaceChildren();
    const entries = Object.entries(object(draft.bindings)).filter(([key, v]) =>
      `${key} ${JSON.stringify(v)}`
        .toLowerCase()
        .includes(filter.value.toLowerCase()),
    );
    entries.forEach(([chord, values]) => {
      const row = el("div", "shortcut-row");
      row.append(el("div", "", chord));
      const commands = el("textarea");
      commands.value = array(values).join("\n");
      commands.setAttribute("aria-label", `Commands for ${chord}`);
      commands.addEventListener("input", () =>
        stage(
          ["bindings", chord],
          commands.value
            .split("\n")
            .map((v) => v.trim())
            .filter(Boolean),
        ),
      );
      row.append(
        commands,
        button("Reset", () => resetField(["bindings", chord]), "quiet"),
      );
      card.append(row);
    });
    if (!entries.length)
      card.append(el("div", "empty", "No shortcuts match your search."));
  };
  filter.addEventListener("input", renderRows);
  renderRows();
  resetButton(content, "Restore all default shortcuts", ["bindings"]);
}
function personalRules(): Json[] {
  const edit = edits.find(
    (e) => e.op === "set" && e.path.length === 1 && e.path[0] === "rules",
  );
  return edit?.op === "set"
    ? array(edit.value)
    : array(loaded.snapshot.overrides.rules);
}
function renderRules(): void {
  const rules = personalRules();
  const toolbar = el("div", "toolbar");
  toolbar.append(
    el("p", "note", `${rules.length} personal rules`),
    button("Add rule", () => {
      stage(
        ["rules"],
        [
          ...rules,
          {
            commands: ["set-floating"],
            matches: [{ process: { equals: "" } }],
          },
        ],
      );
      renderPage();
    }),
  );
  content.append(toolbar);
  if (!rules.length)
    content.append(
      el(
        "div",
        "card empty",
        "No personal rules yet. Add one to place an app in a workspace, keep it floating, or leave it unmanaged.",
      ),
    );
  rules.forEach((entry, index) => {
    const rule = object(entry);
    const card = el("div", "card list-card");
    const head = el("div", "list-head");
    head.append(
      el("h3", "", `Rule ${index + 1}`),
      button(
        "Remove",
        () => {
          stage(
            ["rules"],
            personalRules().filter((_, i) => i !== index),
          );
          renderPage();
        },
        "quiet danger",
      ),
    );
    card.append(head);
    const update = (changed: Obj) => {
      const next = clone(personalRules());
      next[index] = { ...object(next[index]), ...changed };
      stage(["rules"], next);
    };
    const fields = el("div", "fields");
    fields.append(
      simple(
        "Commands · one per line",
        array(rule.commands).join("\n"),
        (v) =>
          update({
            commands: v
              .split("\n")
              .map((s) => s.trim())
              .filter(Boolean),
          }),
        { wide: true, multiline: true, placeholder: "move-to-workspace 2" },
      ),
    );
    card.append(fields);
    array(rule.matches).forEach((entry, matchIndex) => {
      const match = object(entry);
      const group = el("details");
      group.open = true;
      group.append(
        el(
          "summary",
          "",
          `Match ${matchIndex + 1} · ${matchIndex ? "or" : "when"} these conditions match`,
        ),
      );
      const matchFields = el("div", "fields");
      for (const key of ["process", "class", "title"]) {
        const matcher = object(match[key]);
        const mode =
          Object.keys(matcher).find((k) => matcher[k] !== null) ?? "equals";
        const label = el(
          "label",
          "",
          `${key[0]!.toUpperCase()}${key.slice(1)} matching`,
        );
        const select = el("select");
        ["equals", "regex", "not-regex"].forEach((v) => {
          const opt = el(
            "option",
            "",
            v === "equals"
              ? "Equals"
              : v === "regex"
                ? "Matches expression"
                : "Does not match expression",
          );
          opt.value = v;
          select.append(opt);
        });
        select.value = mode;
        const setMatcher = (newMode: string, text: string) => {
          const matches = clone(array(object(personalRules()[index]).matches));
          const next = object(matches[matchIndex]);
          if (text) next[key] = { [newMode]: text };
          else delete next[key];
          update({ matches });
        };
        const textInput = simple(
          `${key} value`,
          matcher[mode] ?? "",
          (v) => setMatcher(select.value, v),
          {
            placeholder:
              key === "process" ? "e.g. Code" : "Leave blank to ignore",
          },
        );
        select.addEventListener("change", () =>
          setMatcher(select.value, textInput.querySelector("input")!.value),
        );
        label.append(select);
        matchFields.append(label, textInput);
      }
      group.append(
        matchFields,
        button(
          "Remove match",
          () => {
            update({
              matches: array(object(personalRules()[index]).matches).filter(
                (_, i) => i !== matchIndex,
              ),
            });
            renderPage();
          },
          "quiet danger",
        ),
      );
      card.append(group);
    });
    card.append(
      button(
        "Add alternative match",
        () => {
          update({
            matches: [
              ...array(object(personalRules()[index]).matches),
              { process: { equals: "" } },
            ],
          });
          renderPage();
        },
        "quiet",
      ),
    );
    content.append(card);
  });
  const builtIn = section("Built-in app behavior");
  const details = el("details");
  details.append(el("summary", "", "View the rules Moon provides"));
  const list = el("div");
  array(loaded.snapshot.defaults.rules).forEach((value) => {
    const rule = object(value);
    const row = el("div", "row");
    row.append(
      el("div", "copy", array(rule.commands).join(", ")),
      el(
        "div",
        "description",
        `${array(rule.matches).length} app or window matches`,
      ),
    );
    list.append(row);
    const pre = el("pre", "path", JSON.stringify(rule.matches, null, 2));
    list.append(pre);
  });
  details.append(list);
  builtIn.append(details);
}
function jsonEditor(
  parent: HTMLElement,
  title: string,
  hint: string,
  path: Path,
): void {
  const details = el("details");
  details.append(el("summary", "", title), el("p", "note", hint));
  const input = el("textarea");
  const fieldKey = JSON.stringify(path);
  input.dataset.fieldPath = fieldKey;
  fieldPages.set(fieldKey, active);
  input.className = "path";
  input.style.width = "100%";
  input.style.minHeight = "150px";
  input.value =
    invalidFields.get(JSON.stringify(path)) ??
    JSON.stringify(get(draft, path), null, 2);
  input.setAttribute("aria-label", title);
  const error = el("div", "description");
  error.id = `field-${++nextField}-error`;
  input.setAttribute("aria-describedby", error.id);
  error.setAttribute("role", "alert");
  const invalidMessage =
    "Enter valid JSON. This field has not been added to your changes.";
  if (invalidFields.has(fieldKey)) {
    input.setAttribute("aria-invalid", "true");
    input.setCustomValidity(invalidMessage);
    error.textContent = invalidMessage;
    details.open = true;
  }
  input.addEventListener("input", () => {
    rememberDraft();
    try {
      invalidFields.delete(JSON.stringify(path));
      queueEdit(path, JSON.parse(input.value) as Json);
      error.textContent = "";
      input.setCustomValidity("");
      input.removeAttribute("aria-invalid");
    } catch {
      invalidFields.set(JSON.stringify(path), input.value);
      updateFooter();
      error.textContent = invalidMessage;
      input.setCustomValidity(error.textContent);
      input.setAttribute("aria-invalid", "true");
    }
  });
  details.append(input, error);
  parent.append(details);
}
function renderAdvanced(): void {
  const typography = section("Bar text & colors");
  field(
    typography,
    "Font family",
    "The bundled Nerd Font supplies the default bar icons.",
    ["bar", "font"],
  );
  field(typography, "Bar background", "A palette name or hex color.", [
    "bar",
    "background",
  ]);
  field(typography, "Bar text", "A palette name or hex color.", [
    "bar",
    "foreground",
  ]);
  const colors = section("Color palettes");
  for (const key of ["palette", "light-palette"]) {
    const details = el("details");
    details.append(
      el("summary", "", key === "palette" ? "Dark palette" : "Light palette"),
    );
    Object.keys(object(draft[key])).forEach((name) =>
      field(details, name, "", [key, name], { type: "color" }),
    );
    colors.append(details);
  }
  const weather = section("Weather location");
  const enabled = el("div", "row");
  enabled.append(
    el(
      "div",
      "copy",
      draft.weather ? "Weather is configured" : "Weather is off",
    ),
  );
  enabled.append(
    button(
      draft.weather ? "Turn off" : "Set location",
      () => {
        stage(
          ["weather"],
          draft.weather
            ? null
            : { latitude: 0, longitude: 0, "interval-seconds": 900 },
        );
        renderPage();
      },
      "quiet",
    ),
  );
  weather.append(enabled);
  if (draft.weather) {
    field(
      weather,
      "Latitude",
      "Enter a coordinate for your location.",
      ["weather", "latitude"],
      { type: "number", min: -90, max: 90, step: 0.0001 },
    );
    field(
      weather,
      "Longitude",
      "Used only to request the weather forecast.",
      ["weather", "longitude"],
      { type: "number", min: -180, max: 180, step: 0.0001 },
    );
    field(
      weather,
      "Update interval",
      "Seconds between weather requests.",
      ["weather", "interval-seconds"],
      { type: "number", min: 60, max: 86400 },
    );
  }
  const advanced = section("Structured settings");
  content.append(
    el(
      "p",
      "note",
      "These advanced editors use JSON values; Moon saves them into your existing TOML file. Invalid changes are rejected. Commands may launch programs, so only add commands you trust.",
    ),
  );
  jsonEditor(
    advanced,
    "Monitor overrides",
    "Set gaps and bar preferences for named monitors.",
    ["monitors"],
  );
  jsonEditor(
    advanced,
    "Bar widgets",
    "Reorder widgets or change their format, alignment, icons, and actions.",
    ["bar", "widgets"],
  );
  jsonEditor(
    advanced,
    "Workspace details",
    "Includes optional per-workspace gaps. Normal names and icons are also available on the Workspaces page.",
    ["workspaces"],
  );
  jsonEditor(advanced, "Key modes", "Named sets of keyboard shortcuts.", [
    "modes",
  ]);
  jsonEditor(
    advanced,
    "Command aliases",
    "Give a sequence of commands a name.",
    ["aliases"],
  );
  jsonEditor(
    advanced,
    "Lifecycle commands",
    "Commands run at startup, shutdown, and reload.",
    ["hooks"],
  );
  const actions = section("Configuration file");
  const row = el("div", "row");
  row.append(
    el("div", "path", loaded.path),
    button("Open file", () => request("open-config")),
  );
  actions.append(row);
}
async function refreshStatus(): Promise<void> {
  const runtime = await request<Loaded["runtime"]>("status");
  loaded.runtime = runtime;
  updateStatus();
  if (active === "About" || active === "General") renderPage();
}
function renderAbout(): void {
  const brand = el("div", "about-brand");
  const copy = el("div");
  copy.append(
    el("strong", "", "moon"),
    el("p", "", `Version ${loaded.version} · Windows 11`),
  );
  brand.append(mark(), copy);
  content.append(brand);
  const card = section("On this PC");
  const row = el("div", "row");
  const info = el("div", "copy");
  info.append(
    el(
      "h3",
      "",
      loaded.runtime.running ? "Moon is running" : "Moon is not running",
    ),
    el("div", "description", loaded.runtime.message),
  );
  row.append(info, button("Refresh", refreshStatus));
  card.append(row);
  if (loaded.runtime.stats) {
    const stats = loaded.runtime.stats;
    const row = el("div", "row");
    row.append(
      el("div", "", "Managed windows"),
      el("div", "", String(stats.managed_windows ?? 0)),
    );
    card.append(row);
  }
  const config = section("Your configuration");
  const configRow = el("div", "row");
  configRow.append(
    el("div", "path", loaded.path),
    button("Open file", () => request("open-config")),
  );
  config.append(configRow);
  const help = section("Help & recovery");
  const helpRow = el("div", "row");
  helpRow.append(
    el("div", "copy", "Emergency recovery"),
    el("kbd", "", "Ctrl + Alt + Shift + F12"),
  );
  help.append(helpRow);
  const docs = el("div", "row");
  docs.append(
    el("div", "copy", "Configuration reference"),
    button("Open reference", () => request("open-reference")),
  );
  help.append(docs);
  content.append(
    el(
      "p",
      "note",
      "Settings stay on this PC. This window uses the shared Microsoft Edge WebView2 Runtime and closes independently of the window manager.",
    ),
  );
}
const searchIndex: [string, string, string][] = [
  [
    "General",
    "Workspace motion",
    "workspace carousel canvas animation sliding motion",
  ],
  ["General", "Focus & movement", "cursor pointer focus follows keyboard"],
  ["General", "Window spacing", "gaps inner outer spacing"],
  ["General", "Recovery", "emergency restore stop"],
  ["Appearance", "Theme", "light dark system colors"],
  [
    "Appearance",
    "Desktop bar",
    "height size opacity material backdrop mica acrylic",
  ],
  ["Appearance", "Window borders", "width radius gradient focused color"],
  ["Workspaces", "Workspaces", "name icon monitor order desktop"],
  ["Shortcuts", "Keyboard shortcuts", "keys bindings commands hotkey"],
  [
    "Rules",
    "App rules",
    "process class title match regex floating workspace ignore",
  ],
  ["Advanced", "Bar text & colors", "font palette foreground background"],
  ["Advanced", "Weather", "latitude longitude location forecast interval"],
  [
    "Advanced",
    "Structured settings",
    "monitors widgets modes aliases lifecycle hooks startup shutdown reload",
  ],
  [
    "About",
    "About Moon",
    "version status running configuration file help reference",
  ],
];
function renderPage(): void {
  if (!content) return;
  content.replaceChildren();
  nextField = 0;
  nav.querySelectorAll("button").forEach((b) => {
    if (b.dataset.page === active && !search)
      b.setAttribute("aria-current", "page");
    else b.removeAttribute("aria-current");
  });
  content.append(el("div", "eyebrow", "Your desktop, considered"));
  const heading = el("h1", "", search ? "Search settings" : active);
  heading.tabIndex = -1;
  content.append(
    heading,
    el("p", "lead", search ? `Results for “${search}”` : descriptions[active]),
  );
  errorBox = el("div", "notice error", errorMessage);
  errorBox.setAttribute("role", "alert");
  errorBox.tabIndex = -1;
  errorBox.hidden = !errorMessage;
  content.append(errorBox);
  renderApplicationState();
  if (search) {
    const card = el("div", "card");
    const terms = search.toLowerCase().split(/\s+/);
    const matches = searchIndex.filter((entry) =>
      terms.every((term) => entry.join(" ").toLowerCase().includes(term)),
    );
    matches.forEach(([page, title]) => {
      const hit = button("", () => switchPage(page), "search-hit");
      hit.append(el("strong", "", title), el("span", "", page));
      card.append(hit);
    });
    if (!matches.length)
      card.append(
        el(
          "div",
          "empty",
          "No settings found. Try “border”, “shortcut”, or “workspace”.",
        ),
      );
    content.append(card);
    return;
  }
  ({
    General: renderGeneral,
    Appearance: renderAppearance,
    Workspaces: renderWorkspaces,
    Shortcuts: renderShortcuts,
    Rules: renderRules,
    Advanced: renderAdvanced,
    About: renderAbout,
  })[active]!();
}
let readySent = false;
async function boot(): Promise<void> {
  const help = active === "Shortcuts" ? refreshShortcuts() : Promise.resolve();
  try {
    loaded = await request<Loaded>("load");
    await help;
    draft = clone(loaded.snapshot.config);
    edits = [];
    undoDraft = undefined;
    renderShell();
  } catch (e) {
    await help;
    if (active === "Shortcuts") {
      renderHelpOnly(e);
    } else {
      root.replaceChildren();
      const page = el("div", "page");
      page.append(
        mark(),
        el("h1", "", "Let’s get Settings ready"),
        el("p", "lead", e instanceof Error ? e.message : String(e)),
      );
      const actions = el("div", "actions");
      actions.append(
        button("Try again", boot, "primary"),
        button("Show current shortcuts", () => {
          active = "Shortcuts";
          void boot();
        }),
        button("Open configuration", () => request("open-config")),
      );
      page.append(actions);
      root.append(page);
    }
  }
  if (!readySent) {
    readySent = true;
    window.ipc?.postMessage(JSON.stringify({ id: 0, method: "ready" }));
  }
}
function refreshVisibleHelp(): void {
  if (!readySent || search) return;
  if (active === "Shortcuts" && shortcutPanel?.isConnected)
    void refreshShortcuts();
  if (active === "General" && overviewPanel?.isConnected)
    void refreshOverview();
}
window.moonActivated = refreshVisibleHelp;
window.addEventListener("focus", refreshVisibleHelp);
document.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    if (confirmationOpen) return;
    void save();
  }
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
    event.preventDefault();
    if (confirmationOpen) return;
    document
      .querySelector<HTMLInputElement>(
        active === "Shortcuts" && !search ? "#shortcut-search" : "#search",
      )
      ?.focus();
  }
});
void boot();
