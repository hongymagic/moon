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
    moonStartPage?: "General" | "Shortcuts" | "Overview";
    moonReceive: (reply: Reply) => void;
    moonCloseRequested: () => void;
    moonActivated: () => void;
    moonPreviewChanged: () => void;
  }
}
let sequence = 0;
const pending = new Map<
  number,
  {
    resolve(value: unknown): void;
    reject(reason: Error): void;
    timer?: ReturnType<typeof setTimeout>;
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
    const message = JSON.stringify({ id, method, params });
    if (new TextEncoder().encode(message).length > 4 * 1024 * 1024) {
      reject(
        new Error(
          "This change is too large to save in Settings. Reduce the configuration size and try again.",
        ),
      );
      return;
    }
    // A save may have reached disk before its reply. Keep the draft locked
    // until the native host confirms the outcome.
    const timer = [
      "save",
      "library-import-file",
      "library-export-file",
      "location-current",
    ].includes(method)
      ? undefined
      : setTimeout(
          () => {
            pending.delete(id);
            reject(
              new Error("Moon is taking longer than expected. Try again."),
            );
          },
          method === "location-search" ? 30000 : 20000,
        );
    pending.set(id, { resolve: (value) => resolve(value as T), reject, timer });
    try {
      window.ipc!.postMessage(message);
    } catch (error) {
      pending.delete(id);
      clearTimeout(timer);
      reject(error);
    }
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
  Overview: "M3 4h18v13H3zM8 20h8M12 17v3M6 7h7v7H6zM15 7h3v7h-3z",
  General: "M5 6h14M5 12h14M5 18h14M9 3v6M15 9v6M10 15v6",
  Appearance:
    "M12 3a9 9 0 1 0 9 9c0-2-2-2-4-2h-2a2 2 0 0 1-2-2V5c0-1-1-2-1-2ZM7 9h.01M7 14h.01M11 17h.01",
  Bar: "M3 5h18v5H3zM6 7.5h2M11 7.5h2M16 7.5h2M3 14h18v6H3z",
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
  Overview:
    "Find a window, move between workspaces, and keep your desktop in order.",
  General:
    "Make room for the way you work. Set how windows move, focus, and fit together.",
  Appearance:
    "A quieter desktop, made yours. Fine-tune the bar, borders, and color.",
  Bar: "Make the desktop bar yours. Choose the information, order, and actions you use every day.",
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
let settingsAvailable = false;
let draft: Obj;
let edits: Edit[] = [];
const standaloneOverview = window.moonStartPage === "Overview";
let active =
  window.moonStartPage === "Shortcuts" || window.moonStartPage === "Overview"
    ? window.moonStartPage
    : "General";
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
  const modal = document.querySelector<HTMLDialogElement>("dialog[open]");
  if (modal) {
    let notice = modal.querySelector<HTMLDivElement>(".dialog-error");
    if (!notice) {
      notice = el("div", "notice error dialog-error");
      notice.setAttribute("role", "alert");
      notice.tabIndex = -1;
      modal.prepend(notice);
    }
    notice.textContent = message;
    notice.focus();
  } else if (errorBox) {
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
function updateFooterVisibility(): void {
  if (footer)
    footer.hidden =
      standaloneOverview &&
      active === "Overview" &&
      !search &&
      !edits.length &&
      !invalidFields.size &&
      !busy;
}
function updateFooter(): void {
  updateFooterVisibility();
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
function clearInvalidFields(path: Path): void {
  for (const key of invalidFields.keys()) {
    const invalidPath = JSON.parse(key) as Path;
    if (path.every((part, index) => part === invalidPath[index]))
      invalidFields.delete(key);
  }
}
function stage(path: Path, value: Json): void {
  if (JSON.stringify(value) !== JSON.stringify(get(draft, path)))
    rememberDraft();
  applyDraft(path, value);
}
function applyDraft(path: Path, value: Json): void {
  put(draft, path, value);
  clearInvalidFields(path);
  queueEdit(path, value);
  updateFooter();
}
function queueEdit(path: Path, value: Json): void {
  const ancestor = edits.findLast(
    (e) => e.path.length < path.length && e.path.every((p, i) => p === path[i]),
  );
  if (ancestor?.op === "set") {
    const parentPath = ancestor.path;
    edits = edits.filter((e) => e !== ancestor);
    queueEdit(parentPath, get(draft, parentPath));
    return;
  }
  edits = edits.filter((e) => !path.every((p, i) => p === e.path[i]));
  const baseline =
    ancestor?.op === "reset"
      ? loaded.snapshot.defaults
      : loaded.snapshot.config;
  if (JSON.stringify(value) !== JSON.stringify(get(baseline, path)))
    edits.push(
      value === null
        ? { op: "reset", path }
        : { op: "set", path, value: clone(value) },
    );
}
function resetField(path: Path): void {
  rememberDraft();
  edits = edits.filter((e) => !path.every((p, i) => p === e.path[i]));
  edits.push({ op: "reset", path });
  const fallback = get(loaded.snapshot.defaults, path);
  if (path[0] === "bindings" && path.length === 2 && fallback === null)
    delete object(draft.bindings)[path[1]!];
  else put(draft, path, fallback);
  clearInvalidFields(path);
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
  finishWindowDrag();
  clearPreviews();
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
      schedulePreviews();
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
  scroll.addEventListener("scroll", schedulePreviews, { passive: true });
  previewObserver?.disconnect();
  previewObserver = new ResizeObserver(schedulePreviews);
  previewObserver.observe(scroll);
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
  step?: "any";
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
    if (options.step !== undefined) input.step = options.step;
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
    applyDraft(path, v);
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
function renderDesktopOnly(error: unknown): void {
  root.replaceChildren();
  const main = el("main", "main desktop-only");
  const scroll = el("div", "scroll");
  content = el("div", "page");
  scroll.append(content);
  main.append(scroll);
  root.append(main);
  content.append(
    mark(),
    el("h1", "", "Overview"),
    el("p", "lead", "Find your windows across workspaces."),
  );
  const notice = el(
    "div",
    "notice error",
    "Saved settings could not be opened. Your live desktop remains available; draft editing is unavailable until settings are repaired.",
  );
  notice.append(
    el("p", "note", error instanceof Error ? error.message : String(error)),
  );
  const actions = el("div", "actions");
  actions.append(
    button("Try settings again", boot),
    button("Open configuration", () => request("open-config")),
  );
  notice.append(actions);
  errorBox = el("div", "notice error");
  errorBox.hidden = true;
  errorBox.setAttribute("role", "alert");
  errorBox.tabIndex = -1;
  content.append(notice, errorBox);
  scroll.addEventListener("scroll", schedulePreviews, { passive: true });
  previewObserver?.disconnect();
  previewObserver = new ResizeObserver(schedulePreviews);
  previewObserver.observe(scroll);
  renderDesktop();
  content.querySelector<HTMLInputElement>("#window-search")?.focus();
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
type DesktopWindow = {
  id: string;
  process_id: string;
  process_birth: string;
  title: string;
  process: string;
  class: string;
  workspace: string;
  monitor_id: string;
  rect: { x: number; y: number; width: number; height: number };
  icon?: string;
  state: {
    focused: boolean;
    visible: boolean;
    floating: boolean;
    fullscreen: boolean;
    suspended: boolean;
  };
  rules: {
    index: number;
    commands: string[];
    matches: {
      index: number;
      matched: boolean;
      conditions: {
        field: string;
        operator: string;
        value: string;
        matched: boolean;
      }[];
    }[];
  }[];
};
type Desktop = {
  generation: string;
  revision: string;
  paused: boolean;
  session_suspended: boolean;
  undo: { available: boolean; label: string | null; reason: string | null };
  active_workspace: string | null;
  total_windows: number;
  truncated: boolean;
  rule_evaluation: "current-metadata";
  workspaces: {
    name: string;
    monitor_id: string;
    active: boolean;
    windows: number;
    bounds: DesktopWindow["rect"];
  }[];
  windows: DesktopWindow[];
};
type WindowAction =
  | { type: "focus" | "float" | "tile" }
  | { type: "move-to-workspace"; workspace: string; follow: boolean };
const windowTarget = (window: DesktopWindow) => ({
  id: window.id,
  process_id: window.process_id,
  process_birth: window.process_birth,
});
let desktop: Desktop | undefined;
let desktopError: string | undefined;
let desktopLoading = false;
let desktopBusy = false;
let desktopFilter = "";
let desktopPanel: HTMLElement | undefined;
let previewFrame = 0;
let previewObserver: ResizeObserver | undefined;
let previewEpoch = 0;
let previewsRequested = false;
type WindowDrag = {
  snapshot: Desktop;
  window: DesktopWindow;
  handle: HTMLButtonElement;
  pointer: number;
  startX: number;
  startY: number;
  x: number;
  y: number;
  active: boolean;
  workspace?: string;
  ghost?: HTMLElement;
};
let dragWindow: WindowDrag | undefined;
let dragFrame = 0;
function finishWindowDrag(commit = false): void {
  const drag = dragWindow;
  if (!drag) return;
  dragWindow = undefined;
  cancelAnimationFrame(dragFrame);
  drag.ghost?.remove();
  document
    .querySelectorAll(".drop-target")
    .forEach((card) => card.classList.remove("drop-target"));
  document.body.classList.remove("moving-window");
  if (drag.handle.hasPointerCapture(drag.pointer))
    drag.handle.releasePointerCapture(drag.pointer);
  if (drag.active) drag.handle.dataset.suppressClick = "true";
  if (commit && drag.active && drag.workspace) {
    void actOnWindow(drag.snapshot, drag.window, {
      type: "move-to-workspace",
      workspace: drag.workspace,
      follow: false,
    });
  } else schedulePreviews();
}
function updateWindowDrag(): void {
  const drag = dragWindow;
  if (!drag?.active) return;
  const scroll = document.querySelector<HTMLElement>(".scroll")!;
  const view = scroll.getBoundingClientRect();
  // Scroll only while an active gesture is held at the canvas edge.
  const edge = 42;
  if (drag.x >= view.left && drag.x <= view.right) {
    if (drag.y < view.top + edge)
      scroll.scrollTop -= Math.min(14, (view.top + edge - drag.y) / 3);
    else if (drag.y > view.bottom - edge)
      scroll.scrollTop += Math.min(14, (drag.y - view.bottom + edge) / 3);
  }
  const hit = document
    .elementFromPoint(drag.x, drag.y)
    ?.closest<HTMLElement>(".workspace-card");
  drag.workspace =
    hit &&
    desktopPanel?.contains(hit) &&
    hit.dataset.workspace !== drag.window.workspace
      ? hit.dataset.workspace
      : undefined;
  document.querySelectorAll<HTMLElement>(".workspace-card").forEach((card) => {
    card.classList.toggle("drop-target", card === hit && !!drag.workspace);
  });
  drag.ghost!.textContent = drag.workspace
    ? `Move ${drag.window.process} to workspace ${drag.workspace}`
    : `Move ${drag.window.process} · Esc to cancel`;
  drag.ghost!.style.left = `${Math.min(Math.max(8, drag.x + 14), Math.max(8, innerWidth - drag.ghost!.offsetWidth - 8))}px`;
  drag.ghost!.style.top = `${Math.min(Math.max(8, drag.y + 18), Math.max(8, innerHeight - drag.ghost!.offsetHeight - 8))}px`;
  dragFrame = requestAnimationFrame(updateWindowDrag);
}
function windowDragHandle(
  snapshot: Desktop,
  source: DesktopWindow,
  disabled: boolean,
): HTMLButtonElement {
  const handle = el("button", "window-drag", "⠿");
  handle.type = "button";
  handle.disabled = disabled || source.state.suspended;
  handle.setAttribute("aria-label", `Move ${source.title || source.process}`);
  handle.title = "Drag to another workspace, or select to choose where to move";
  handle.addEventListener("click", (event) => {
    if (event.detail && handle.dataset.suppressClick) {
      delete handle.dataset.suppressClick;
      return;
    }
    windowDetails(source);
  });
  handle.addEventListener("pointerdown", (event) => {
    if (
      !event.isPrimary ||
      event.button !== 0 ||
      handle.disabled ||
      desktopBusy ||
      busy ||
      confirmationOpen
    )
      return;
    finishWindowDrag();
    delete handle.dataset.suppressClick;
    handle.focus();
    handle.setPointerCapture(event.pointerId);
    dragWindow = {
      snapshot,
      window: source,
      handle,
      pointer: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      x: event.clientX,
      y: event.clientY,
      active: false,
    };
    clearPreviews();
  });
  handle.addEventListener("lostpointercapture", () => finishWindowDrag());
  return handle;
}
document.addEventListener("pointermove", (event) => {
  const drag = dragWindow;
  if (!drag || event.pointerId !== drag.pointer) return;
  drag.x = event.clientX;
  drag.y = event.clientY;
  if (
    !drag.active &&
    Math.hypot(drag.x - drag.startX, drag.y - drag.startY) >= 6
  ) {
    drag.active = true;
    drag.ghost = el("div", "window-drag-ghost");
    drag.ghost.setAttribute("aria-hidden", "true");
    document.body.append(drag.ghost);
    document.body.classList.add("moving-window");
    updateWindowDrag();
  }
});
document.addEventListener("pointerup", (event) => {
  if (event.pointerId !== dragWindow?.pointer) return;
  dragWindow.x = event.clientX;
  dragWindow.y = event.clientY;
  // Resolve the target at release even if the next animation frame has not run.
  cancelAnimationFrame(dragFrame);
  updateWindowDrag();
  finishWindowDrag(true);
});
document.addEventListener("pointercancel", (event) => {
  if (event.pointerId === dragWindow?.pointer) finishWindowDrag();
});
document.addEventListener(
  "keydown",
  (event) => {
    if (event.key === "Escape" && dragWindow) {
      event.preventDefault();
      event.stopImmediatePropagation();
      finishWindowDrag();
    }
  },
  true,
);
window.addEventListener("blur", () => finishWindowDrag());
function clearPreviews(): void {
  previewEpoch++;
  cancelAnimationFrame(previewFrame);
  document
    .querySelectorAll(".native-preview-shown")
    .forEach((slot) => slot.classList.remove("native-preview-shown"));
  if (window.ipc && previewsRequested) {
    previewsRequested = false;
    void request("previews", { items: [] }).catch(() => {});
  }
}
function schedulePreviews(): void {
  cancelAnimationFrame(previewFrame);
  previewFrame = requestAnimationFrame(() => {
    void updatePreviews();
  });
}
async function updatePreviews(): Promise<void> {
  if (
    active !== "Overview" ||
    search ||
    confirmationOpen ||
    document.hidden ||
    dragWindow ||
    !desktop ||
    !desktopPanel?.isConnected
  ) {
    clearPreviews();
    return;
  }
  const snapshot = desktop;
  const epoch = ++previewEpoch;
  const viewport = document.querySelector(".scroll")!.getBoundingClientRect();
  const slots = Array.from(
    desktopPanel.querySelectorAll<HTMLElement>(".window-preview"),
  );
  const items = slots
    .flatMap((slot) => {
      const window = snapshot.windows.find(
        (item) => item.id === slot.dataset.windowId,
      );
      const rect = slot.getBoundingClientRect();
      const canvas = slot.closest(".workspace-canvas")!.getBoundingClientRect();
      if (
        !window ||
        rect.width < 16 ||
        rect.height < 16 ||
        rect.top < Math.max(0, viewport.top, canvas.top) ||
        rect.left < Math.max(0, viewport.left, canvas.left) ||
        rect.bottom > Math.min(innerHeight, viewport.bottom, canvas.bottom) ||
        rect.right > Math.min(innerWidth, viewport.right, canvas.right)
      )
        return [];
      return [
        {
          target: windowTarget(window),
          rect: {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
          },
        },
      ];
    })
    .slice(0, 32);
  try {
    previewsRequested = true;
    const result = await request<{ shown: string[]; unavailable: string[] }>(
      "previews",
      { generation: snapshot.generation, revision: snapshot.revision, items },
    );
    if (
      epoch !== previewEpoch ||
      desktop !== snapshot ||
      !desktopPanel.isConnected
    )
      return;
    for (const slot of slots)
      slot.classList.toggle(
        "native-preview-shown",
        result.shown.includes(slot.dataset.windowId!),
      );
  } catch {
    /* Geometry and app names remain available when native previews cannot be shown. */
  }
}
async function refreshDesktop(): Promise<void> {
  if (desktopLoading) return;
  desktopLoading = true;
  paintDesktop();
  try {
    desktop = await request<Desktop>("desktop");
    desktopError = undefined;
  } catch (error) {
    desktopError = error instanceof Error ? error.message : String(error);
  } finally {
    desktopLoading = false;
    paintDesktop();
  }
}
async function actOnWindow(
  snapshot: Desktop,
  window: DesktopWindow,
  action: WindowAction,
): Promise<void> {
  if (desktopBusy || busy || confirmationOpen) return;
  let dismissed = false;
  let succeeded = false;
  desktopBusy = true;
  clearPreviews();
  paintDesktop();
  try {
    await request("window-action", {
      generation: snapshot.generation,
      revision: snapshot.revision,
      target: windowTarget(window),
      action,
    });
    succeeded = true;
    if (
      action.type === "focus" &&
      standaloneOverview &&
      !edits.length &&
      !invalidFields.size
    ) {
      await request("close");
      dismissed = true;
    }
    toast(
      action.type === "move-to-workspace"
        ? `Moved ${window.process} to workspace ${action.workspace}.`
        : action.type === "focus"
          ? `Opened ${window.title || window.process}.`
          : `${window.process} is now ${action.type === "float" ? "floating" : "tiled"}.`,
    );
  } catch (error) {
    showError(error);
  } finally {
    desktopBusy = false;
    if (!dismissed) {
      await refreshDesktop();
      if (succeeded && action.type !== "focus") focusDesktopWindow(window.id);
    }
  }
}
function focusDesktopWindow(id: string): void {
  const row = Array.from(
    desktopPanel?.querySelectorAll<HTMLElement>(".desktop-window") ?? [],
  ).find((node) => node.dataset.windowId === id);
  (
    row?.querySelector<HTMLButtonElement>(".window-options") ??
    document.querySelector<HTMLInputElement>("#window-search")
  )?.focus();
}
async function undoLayout(): Promise<void> {
  if (
    !desktop?.undo.available ||
    desktopBusy ||
    busy ||
    desktop.paused ||
    desktop.session_suspended ||
    desktopError
  )
    return;
  const snapshot = desktop;
  let succeeded = false;
  desktopBusy = true;
  clearPreviews();
  paintDesktop();
  try {
    await request("layout-undo", {
      generation: snapshot.generation,
      revision: snapshot.revision,
    });
    succeeded = true;
    toast("Undid the last desktop layout change.");
  } catch (error) {
    showError(error);
  } finally {
    desktopBusy = false;
    await refreshDesktop();
    if (succeeded)
      (
        desktopPanel?.querySelector<HTMLButtonElement>(
          ".layout-undo:not(:disabled)",
        ) ?? document.querySelector<HTMLInputElement>("#window-search")
      )?.focus();
  }
}
function appIcon(window: DesktopWindow): HTMLElement {
  if (window.icon?.startsWith("data:image/bmp;base64,")) {
    const image = el("img", "app-icon");
    image.alt = "";
    image.src = window.icon;
    image.addEventListener(
      "error",
      () =>
        image.replaceWith(
          el(
            "span",
            "app-icon app-initial",
            window.process.slice(0, 1).toUpperCase(),
          ),
        ),
      { once: true },
    );
    return image;
  }
  const fallback = el(
    "span",
    "app-icon app-initial",
    window.process.slice(0, 1).toUpperCase() || "?",
  );
  fallback.setAttribute("aria-hidden", "true");
  return fallback;
}
function paintDesktop(): void {
  if (!desktopPanel?.isConnected) return;
  finishWindowDrag();
  clearPreviews();
  desktopPanel.replaceChildren();
  if (desktopError)
    desktopPanel.append(
      el(
        "div",
        "notice error",
        `${desktopError} Refresh the overview before trying another desktop action.`,
      ),
    );
  const toolbar = el("div", "toolbar");
  toolbar.append(
    el(
      "p",
      "note",
      desktop
        ? `${desktop.total_windows} managed ${desktop.total_windows === 1 ? "window" : "windows"} · Actions here change your desktop immediately.`
        : "Find your windows across workspaces.",
    ),
  );
  const actions = el("div", "actions");
  const refresh = button(
    desktopLoading ? "Refreshing…" : "Refresh windows",
    refreshDesktop,
    "quiet",
  );
  refresh.disabled = desktopLoading || desktopBusy;
  const undo = button("Undo layout", undoLayout, "layout-undo");
  undo.disabled =
    !desktop?.undo.available ||
    desktopBusy ||
    desktopLoading ||
    !!desktopError ||
    !!desktop?.paused ||
    !!desktop?.session_suspended;
  undo.title =
    desktop?.undo.label ?? desktop?.undo.reason ?? "No layout change to undo";
  actions.append(undo, refresh);
  toolbar.append(actions);
  desktopPanel.append(toolbar);
  if (!desktop) {
    desktopPanel.append(
      el(
        "div",
        "empty",
        desktopLoading
          ? "Reading your desktop…"
          : "Start Moon, then refresh to see your windows.",
      ),
    );
    return;
  }
  const snapshot = desktop;
  if (snapshot.paused || snapshot.session_suspended)
    desktopPanel.append(
      el(
        "div",
        "notice",
        snapshot.session_suspended
          ? "Your Windows session is suspended. Desktop actions are unavailable."
          : "Moon is paused. Your workspace assignments are shown; tiling is not active.",
      ),
    );
  if (snapshot.truncated)
    desktopPanel.append(
      el(
        "p",
        "note",
        "This overview reached its display limit. Some windows or rule details are not shown.",
      ),
    );
  if (snapshot.undo.reason && !snapshot.undo.available)
    desktopPanel.append(el("p", "note", snapshot.undo.reason));
  const terms = desktopFilter.toLocaleLowerCase().trim().split(/\s+/);
  const matches = snapshot.windows.filter((window) =>
    terms.every((term) =>
      `${window.process} ${window.title} ${window.workspace}`
        .toLocaleLowerCase()
        .includes(term),
    ),
  );
  if (desktopFilter.trim() && !matches.length)
    desktopPanel.append(
      el(
        "div",
        "empty",
        "No windows match. Try an app name, window title, or workspace.",
      ),
    );
  const grid = el("div", "workspace-grid");
  const disabled =
    desktopBusy ||
    desktopLoading ||
    !!desktopError ||
    snapshot.paused ||
    snapshot.session_suspended;
  // Keep configured order within each group without changing the catalog or
  // the workspace choices used by keyboard-accessible Move controls.
  const workspaces = [
    ...snapshot.workspaces.filter((workspace) => workspace.active),
    ...snapshot.workspaces.filter(
      (workspace) => !workspace.active && workspace.windows > 0,
    ),
    ...snapshot.workspaces.filter(
      (workspace) => !workspace.active && workspace.windows === 0,
    ),
  ];
  for (const workspace of workspaces) {
    const members = matches.filter(
      (window) => window.workspace === workspace.name,
    );
    if (desktopFilter.trim() && !members.length) continue;
    const card = el("section", "workspace-card");
    const empty = workspace.windows === 0;
    card.classList.toggle("workspace-empty", empty);
    card.dataset.workspace = workspace.name;
    if (workspace.active) card.classList.add("workspace-active");
    const heading = el("div", "workspace-card-heading");
    heading.append(
      el("h2", "", `Workspace ${workspace.name}`),
      el(
        "span",
        "workspace-state",
        workspace.active
          ? "Active"
          : `${workspace.windows} ${workspace.windows === 1 ? "window" : "windows"}`,
      ),
    );
    card.append(heading);
    if (!empty) {
      const canvas = el("div", "workspace-canvas");
      canvas.setAttribute(
        "aria-label",
        `Layout of workspace ${workspace.name}`,
      );
      const bounds = workspace.bounds;
      canvas.style.aspectRatio = `${Math.max(1, bounds.width)} / ${Math.max(1, bounds.height)}`;
      for (const window of members) {
        const slot = el("div", "window-preview");
        slot.dataset.windowId = window.id;
        slot.setAttribute("aria-hidden", "true");
        slot.style.left = `${Math.max(0, ((window.rect.x - bounds.x) / Math.max(1, bounds.width)) * 100)}%`;
        slot.style.top = `${Math.max(0, ((window.rect.y - bounds.y) / Math.max(1, bounds.height)) * 100)}%`;
        slot.style.width = `${Math.min(100, (window.rect.width / Math.max(1, bounds.width)) * 100)}%`;
        slot.style.height = `${Math.min(100, (window.rect.height / Math.max(1, bounds.height)) * 100)}%`;
        slot.classList.toggle("preview-focused", window.state.focused);
        slot.append(appIcon(window), el("span", "preview-app", window.process));
        canvas.append(slot);
      }
      card.append(canvas);
    }
    const list = el("ul", "workspace-windows");
    for (const window of members) {
      const row = el("li", "desktop-window");
      row.dataset.windowId = window.id;
      const activate = button(
        "",
        () => actOnWindow(snapshot, window, { type: "focus" }),
        "window-activate",
      );
      activate.disabled = disabled || window.state.suspended;
      activate.setAttribute(
        "aria-label",
        `Activate ${window.title || window.process}`,
      );
      activate.append(appIcon(window));
      const copy = el("span", "window-copy");
      copy.append(
        el("strong", "", window.title || window.process),
        el(
          "span",
          "note",
          `${window.process}${window.state.floating ? " · Floating" : ""}${window.state.fullscreen ? " · Fullscreen" : ""}${window.state.suspended ? " · Unavailable" : ""}`,
        ),
      );
      activate.append(copy);
      const details = button(
        "Options",
        () => windowDetails(window),
        "quiet window-options",
      );
      details.setAttribute(
        "aria-label",
        `Options for ${window.title || window.process}`,
      );
      details.disabled = desktopBusy;
      row.append(
        windowDragHandle(snapshot, window, disabled),
        activate,
        details,
      );
      list.append(row);
    }
    if (!members.length)
      list.append(el("li", "empty", "Drop a window here to move it."));
    card.append(list);
    grid.append(card);
  }
  desktopPanel.append(grid);
  schedulePreviews();
}
function renderDesktop(): void {
  const toolbar = el("div", "toolbar desktop-search");
  const search = el("input", "search");
  search.id = "window-search";
  search.type = "search";
  search.placeholder = "Find an app or window";
  search.setAttribute("aria-label", "Find an app or window");
  search.value = desktopFilter;
  search.addEventListener("input", () => {
    desktopFilter = search.value;
    paintDesktop();
  });
  search.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      desktopPanel
        ?.querySelector<HTMLButtonElement>(".window-activate:not(:disabled)")
        ?.click();
    }
    if (event.key === "Escape" && !search.value && standaloneOverview) {
      event.preventDefault();
      window.moonCloseRequested();
    }
    if (event.key === "Escape" && search.value) {
      event.preventDefault();
      search.value = "";
      desktopFilter = "";
      paintDesktop();
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      desktopPanel
        ?.querySelector<HTMLButtonElement>(".window-activate:not(:disabled)")
        ?.focus();
    }
  });
  const clear = button(
    "Clear search",
    () => {
      desktopFilter = "";
      search.value = "";
      paintDesktop();
      search.focus();
    },
    "quiet",
  );
  toolbar.append(search, clear);
  content.append(toolbar);
  desktopPanel = el("div", "desktop-panel");
  content.append(desktopPanel);
  paintDesktop();
  if (!desktop && !desktopLoading && !desktopError) void refreshDesktop();
}
function editorDialog(
  title: string,
  build: (dialog: HTMLDialogElement, close: () => void) => void,
): void {
  if (confirmationOpen || busy) return;
  finishWindowDrag();
  clearPreviews();
  confirmationOpen = true;
  const previous = document.activeElement as HTMLElement | null;
  const dialog = el("dialog", "editor-dialog");
  const heading = el("h2", "", title);
  heading.id = "editor-title";
  dialog.setAttribute("aria-labelledby", heading.id);
  const close = () => {
    if (libraryBusy) return;
    dialog.close();
    dialog.remove();
    confirmationOpen = false;
    if (previous?.isConnected) previous.focus();
    schedulePreviews();
  };
  dialog.append(heading);
  build(dialog, close);
  dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    close();
  });
  document.body.append(dialog);
  dialog.showModal();
  dialog.querySelector<HTMLElement>("input,select,button")?.focus();
}
function windowDetails(window: DesktopWindow): void {
  const snapshot = desktop;
  if (!snapshot) return;
  editorDialog(window.title || window.process, (dialog, close) => {
    dialog.append(
      el(
        "p",
        "note",
        `${window.process} · Workspace ${window.workspace}${window.state.floating ? " · Floating" : " · Tiled"}`,
      ),
    );
    const choices = el("select");
    choices.setAttribute("aria-label", `Move ${window.process} to workspace`);
    for (const workspace of snapshot.workspaces) {
      const option = el("option", "", `Workspace ${workspace.name}`);
      option.value = workspace.name;
      choices.append(option);
    }
    choices.value = window.workspace;
    const move = button("Move window", () => {
      const name = choices.value;
      close();
      return actOnWindow(snapshot, window, {
        type: "move-to-workspace",
        workspace: name,
        follow: false,
      });
    });
    const toggle = button(
      window.state.floating ? "Tile window" : "Float window",
      () => {
        close();
        return actOnWindow(snapshot, window, {
          type: window.state.floating ? "tile" : "float",
        });
      },
    );
    move.disabled = toggle.disabled =
      snapshot.paused ||
      snapshot.session_suspended ||
      window.state.suspended ||
      !!desktopError;
    const controls = el("div", "actions");
    controls.append(choices, move, toggle);
    dialog.append(controls);
    const rules = el("details");
    rules.append(
      el("summary", "", "Rules matching this window now"),
      el(
        "p",
        "note",
        "Matches use current app and title information. They explain current matches, not the history of this window’s placement.",
      ),
    );
    if (!window.rules.length)
      rules.append(
        el(
          "p",
          "note",
          "No app rule currently matches. New windows normally join their monitor’s active workspace.",
        ),
      );
    for (const rule of window.rules) {
      rules.append(
        el("p", "", `Rule ${rule.index + 1}: ${rule.commands.join("; ")}`),
      );
      for (const match of rule.matches)
        rules.append(
          el(
            "p",
            "note",
            `${match.matched ? "Matches" : "Does not match"}: ${match.conditions.map((condition) => `${condition.field} ${condition.operator} “${condition.value}” (${condition.matched ? "yes" : "no"})`).join(" and ")}`,
          ),
        );
    }
    dialog.append(rules);
    const actions = el("div", "actions");
    const createRule = button(
      "Create app rule",
      () => {
        close();
        createWindowRule(window);
      },
      "primary",
    );
    createRule.disabled = !settingsAvailable;
    if (!settingsAvailable)
      dialog.append(
        el(
          "p",
          "note",
          "Fix the saved settings before creating a rule. Live window controls remain available.",
        ),
      );
    actions.append(button("Close", close), createRule);
    dialog.append(actions);
  });
}
const asciiFold = (text: string) =>
  text.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
function createWindowRule(selected: DesktopWindow): void {
  if (!desktop || !settingsAvailable) return;
  const windows = desktop.windows;
  editorDialog("Always place this app", (dialog, close) => {
    dialog.append(
      el(
        "p",
        "note",
        "Create a personal rule in your draft. Review it, then save when you’re ready.",
      ),
    );
    const app = el("select");
    app.setAttribute("aria-label", "App for the rule");
    for (const process of [
      ...new Set(windows.map((window) => window.process)),
    ].sort()) {
      const option = el("option", "", process);
      option.value = process;
      app.append(option);
    }
    app.value = selected.process;
    const action = el("select");
    action.setAttribute("aria-label", "Rule behavior");
    for (const [value, title] of [
      ["workspace", "Place in a workspace"],
      ["float", "Keep floating"],
      ["ignore", "Leave unmanaged"],
    ]) {
      const option = el("option", "", title);
      option.value = value!;
      action.append(option);
    }
    const workspace = el("select");
    workspace.setAttribute("aria-label", "Rule workspace");
    for (const value of array(draft.workspaces)) {
      const name = String(object(value).name);
      const option = el("option", "", name);
      option.value = name;
      workspace.append(option);
    }
    workspace.value = selected.workspace;
    const preview = el("div", "notice");
    preview.setAttribute("role", "status");
    const update = () => {
      workspace.hidden = action.value !== "workspace";
      const matched = windows.filter(
        (window) => asciiFold(window.process) === asciiFold(app.value),
      );
      preview.replaceChildren(
        el(
          "strong",
          "",
          `${matched.length} managed ${matched.length === 1 ? "window currently matches" : "windows currently match"}`,
        ),
        el(
          "p",
          "note",
          matched
            .map(
              (window) =>
                `${window.title || window.process} — workspace ${window.workspace}`,
            )
            .join("; "),
        ),
      );
    };
    app.addEventListener("change", update);
    action.addEventListener("change", update);
    update();
    dialog.append(app, action, workspace, preview);
    const actions = el("div", "actions");
    actions.append(
      button("Cancel", close),
      button(
        "Add rule to draft",
        () => {
          const command =
            action.value === "workspace"
              ? `move-to-workspace ${JSON.stringify(workspace.value)}`
              : action.value === "float"
                ? "set-floating"
                : "ignore";
          const rule = {
            commands: [command],
            matches: [{ process: { equals: app.value } }],
          };
          const rules = personalRules();
          if (
            rules.some(
              (existing) => JSON.stringify(existing) === JSON.stringify(rule),
            )
          ) {
            toast("This personal rule is already in your draft.");
            close();
            return;
          }
          stage(["rules"], [...rules, rule]);
          close();
          switchPage("Rules");
          toast("Added the app rule to your draft. Save changes to use it.");
        },
        "primary",
      ),
    );
    dialog.append(actions);
  });
}

type LibraryKind = "profile" | "theme";
type LibraryEntry = {
  kind: LibraryKind;
  name: string;
  builtin: boolean;
  values: Obj;
};
type LibraryCatalog = { source: string | null; entries: LibraryEntry[] };
let library: LibraryCatalog | undefined;
let libraryLoading = false;
let libraryError: string | undefined;
let libraryBusy = false;
const libraryPanels: Partial<Record<LibraryKind, HTMLElement>> = {};
async function refreshLibrary(): Promise<void> {
  if (libraryLoading) return;
  libraryLoading = true;
  try {
    library = await request<LibraryCatalog>("library-list");
    libraryError = undefined;
  } catch (error) {
    libraryError = error instanceof Error ? error.message : String(error);
  } finally {
    libraryLoading = false;
    if (!confirmationOpen) {
      paintLibrary("theme");
      paintLibrary("profile");
    }
  }
}
function stageLibraryEdits(changes: Edit[]): void {
  rememberDraft();
  for (const change of changes) {
    if (change.op === "set") applyDraft(change.path, change.value);
    else {
      edits = edits.filter(
        (existing) =>
          !change.path.every((part, index) => part === existing.path[index]),
      );
      edits.push(change);
      put(draft, change.path, get(loaded.snapshot.defaults, change.path));
      clearInvalidFields(change.path);
    }
  }
  renderPage();
  updateFooter();
}
async function applyLibrary(entry: LibraryEntry): Promise<void> {
  if (libraryBusy || busy || invalidFields.size) return;
  libraryBusy = true;
  busy = true;
  busyLabel = "Preparing your draft…";
  content.inert = true;
  updateFooter();
  try {
    const result = await request<{ edits: Edit[] }>("library-apply", {
      kind: entry.kind,
      name: entry.name,
      config: draft,
    });
    stageLibraryEdits(result.edits);
    toast(
      `${entry.name} is in your draft. Save changes to apply it to your desktop.`,
    );
  } finally {
    libraryBusy = false;
    busy = false;
    content.inert = false;
    updateFooter();
    if (!confirmationOpen) renderPage();
  }
}
function libraryNameDialog(kind: LibraryKind, entry?: LibraryEntry): void {
  if (!library || libraryBusy || busy) return;
  const snapshot = library;
  editorDialog(
    entry
      ? `Rename ${entry.name}`
      : kind === "profile"
        ? "Save this setup"
        : "Save your theme",
    (dialog, close) => {
      dialog.append(
        el(
          "p",
          "note",
          entry
            ? "Rename this saved item without changing your desktop or draft."
            : kind === "profile"
              ? "Save workspace and monitor assignments, spacing, appearance and movement preferences from your current draft. Apps, commands, rules and private file paths are not included."
              : "Save the current draft’s colors, borders and bar style. Widgets, shortcuts and commands are not included.",
        ),
      );
      const label = el("label", "", "Name");
      const name = el("input", "search");
      name.setAttribute("aria-label", "Saved item name");
      name.maxLength = 128; // At most 64 Unicode characters, including surrogate pairs.
      name.value = entry?.name ?? "";
      name.placeholder =
        kind === "profile" ? "Desk, Laptop, Presentation…" : "My theme";
      label.append(name);
      dialog.append(label);
      const replaceLabel = el("label", "replace-label");
      const replace = el("input");
      replace.type = "checkbox";
      replaceLabel.append(
        replace,
        document.createTextNode(" Replace the saved item with this name"),
      );
      replaceLabel.hidden = true;
      dialog.append(replaceLabel);
      const note = el("p", "note");
      dialog.append(note);
      const save = button(
        entry ? "Rename" : "Save to library",
        async () => {
          const text = name.value.trim();
          if (!text || !name.checkValidity()) {
            name.reportValidity();
            return;
          }
          libraryBusy = true;
          save.disabled = true;
          try {
            library = await request<LibraryCatalog>(
              entry ? "library-rename" : "library-save",
              entry
                ? {
                    expected_source: snapshot.source,
                    kind,
                    name: entry.name,
                    new_name: text,
                  }
                : {
                    expected_source: snapshot.source,
                    kind,
                    name: text,
                    config: draft,
                    replace: replace.checked,
                  },
            );
            libraryError = undefined;
            libraryBusy = false;
            close();
            renderPage();
            toast(
              entry
                ? "Renamed the saved item."
                : "Saved to your local library. Your desktop configuration is unchanged.",
            );
          } finally {
            libraryBusy = false;
            if (dialog.isConnected) save.disabled = false;
          }
        },
        "primary",
      );
      name.required = true;
      const update = () => {
        const text = name.value.trim();
        const invalidName =
          Array.from(text).length > 64 ||
          /[\x00-\x1f\x7f-\x9f/\\:]/u.test(text);
        name.setCustomValidity(
          invalidName
            ? "Use up to 64 characters without control characters, slashes or colons."
            : "",
        );
        const match = snapshot.entries.find(
          (item) =>
            item.kind === kind &&
            item.name.toLowerCase() === name.value.trim().toLowerCase(),
        );
        replaceLabel.hidden = !!entry || !match || match.builtin;
        note.textContent = invalidName
          ? name.validationMessage
          : match?.builtin
            ? "This name belongs to a built-in theme. Choose another name."
            : !entry && match
              ? "Saving will replace this library item only after you confirm above."
              : "";
        save.disabled =
          !name.value.trim() ||
          invalidName ||
          !!match?.builtin ||
          (!entry && !!match && !replace.checked);
      };
      name.addEventListener("input", () => {
        replace.checked = false;
        update();
      });
      replace.addEventListener("change", update);
      update();
      const actions = el("div", "actions");
      actions.append(button("Cancel", close), save);
      dialog.append(actions);
    },
  );
}
async function deleteLibrary(entry: LibraryEntry): Promise<void> {
  if (
    !library ||
    entry.builtin ||
    libraryBusy ||
    !(await confirmAction(
      `Remove ${entry.name}?`,
      "This removes the saved library item. Your current draft and desktop settings stay unchanged.",
      "Remove saved item",
      true,
    ))
  )
    return;
  libraryBusy = true;
  try {
    library = await request<LibraryCatalog>("library-delete", {
      expected_source: library.source,
      kind: entry.kind,
      name: entry.name,
    });
    renderPage();
    toast("Removed the saved item from your library.");
  } finally {
    libraryBusy = false;
    if (!confirmationOpen) renderPage();
  }
}
function importLibrary(kind: LibraryKind, initialDocument = ""): void {
  if (!library || libraryBusy) return;
  const snapshot = library;
  editorDialog(`Import a ${kind}`, (dialog, close) => {
    dialog.append(
      el(
        "p",
        "note",
        "Paste exported Moon data. Only the declared theme or profile settings are accepted; executable commands and private paths are not imported.",
      ),
    );
    const input = el("textarea", "library-document");
    input.setAttribute("aria-label", "Moon import data");
    input.spellcheck = false;
    input.maxLength = 65536;
    input.value = initialDocument;
    const preview = el(
      "p",
      "note",
      "Paste a document to review its name and type.",
    );
    preview.setAttribute("role", "status");
    const replaceLabel = el("label", "replace-label");
    const replace = el("input");
    replace.type = "checkbox";
    replaceLabel.append(
      replace,
      document.createTextNode(" Replace the saved item with this name"),
    );
    replaceLabel.hidden = true;
    const apply = button(
      "Import to library",
      async () => {
        libraryBusy = true;
        apply.disabled = true;
        try {
          library = await request<LibraryCatalog>("library-import", {
            expected_source: snapshot.source,
            document: input.value,
            replace: replace.checked,
          });
          libraryError = undefined;
          libraryBusy = false;
          close();
          renderPage();
          toast(
            "Imported to your library. Choose Apply to add it to your draft.",
          );
        } finally {
          libraryBusy = false;
          if (dialog.isConnected) update();
        }
      },
      "primary",
    );
    const update = () => {
      try {
        if (new TextEncoder().encode(input.value).length > 65536)
          throw new Error("Import data must be no larger than 64 KiB.");
        const document = JSON.parse(input.value) as {
          version?: number;
          kind?: string;
          name?: string;
        };
        if (
          document.version !== 1 ||
          document.kind !== kind ||
          typeof document.name !== "string" ||
          !document.name.trim()
        )
          throw new Error(`Paste a version 1 Moon ${kind} document.`);
        const match = snapshot.entries.find(
          (entry) =>
            entry.kind === kind &&
            entry.name.toLowerCase() === document.name!.toLowerCase(),
        );
        replaceLabel.hidden = !match || match.builtin;
        if (match?.builtin)
          throw new Error("Built-in names cannot be replaced.");
        preview.textContent = `${document.name} · ${kind}${match ? " · already in your library" : " · new library item"}. Imported values are validated before anything is stored.`;
        apply.disabled = libraryBusy || (!!match && !replace.checked);
      } catch (error) {
        preview.textContent = input.value
          ? error instanceof Error
            ? error.message
            : String(error)
          : "Paste a document to review its name and type.";
        apply.disabled = true;
        replaceLabel.hidden = true;
      }
    };
    input.addEventListener("input", () => {
      replace.checked = false;
      update();
    });
    replace.addEventListener("change", update);
    update();
    dialog.append(input, preview, replaceLabel);
    const actions = el("div", "actions");
    actions.append(button("Cancel", close), apply);
    dialog.append(actions);
  });
}
async function importLibraryFile(kind: LibraryKind): Promise<void> {
  if (libraryBusy || !library) return;
  libraryBusy = true;
  paintLibrary(kind);
  let result: { document: string } | null;
  try {
    result = await request<{ document: string } | null>("library-import-file");
  } finally {
    libraryBusy = false;
    paintLibrary(kind);
  }
  if (result) importLibrary(kind, result.document);
}
async function exportLibraryFile(entry: LibraryEntry): Promise<void> {
  if (libraryBusy) return;
  libraryBusy = true;
  paintLibrary(entry.kind);
  try {
    const result = await request<{ document: string }>("library-export", {
      kind: entry.kind,
      name: entry.name,
    });
    const saved = await request<{ saved: boolean }>("library-export-file", {
      suggested_name: `${entry.name}.moon-${entry.kind}.json`,
      document: result.document,
    });
    if (saved.saved) toast(`Exported ${entry.name}.`);
  } finally {
    libraryBusy = false;
    paintLibrary(entry.kind);
  }
}
async function exportLibrary(entry: LibraryEntry): Promise<void> {
  const result = await request<{ document: string }>("library-export", {
    kind: entry.kind,
    name: entry.name,
  });
  editorDialog(`Share ${entry.name}`, (dialog, close) => {
    dialog.append(
      el(
        "p",
        "note",
        "This document contains only the saved theme or profile settings. It does not include app titles, commands or private file paths.",
      ),
    );
    const text = el("textarea", "library-document");
    text.readOnly = true;
    text.value = result.document;
    text.setAttribute("aria-label", "Exported Moon data");
    dialog.append(text);
    const actions = el("div", "actions");
    actions.append(
      button("Close", close),
      button(
        "Copy data",
        async () => {
          await request("clipboard-copy", { text: result.document });
          toast("Copied Moon data to the clipboard.");
        },
        "primary",
      ),
    );
    dialog.append(actions);
  });
}
function renderLibrary(kind: LibraryKind): void {
  const panel = el("div", "library-panel");
  libraryPanels[kind] = panel;
  content.append(panel);
  paintLibrary(kind);
}
function paintLibrary(kind: LibraryKind): void {
  const panel = libraryPanels[kind];
  if (!panel?.isConnected) return;
  panel.replaceChildren();
  const sectionTitle = kind === "theme" ? "Theme gallery" : "Saved setups";
  const heading = el("h2", "", sectionTitle);
  panel.append(heading);
  panel.append(
    el(
      "p",
      "note",
      kind === "theme"
        ? "Try a coordinated palette, or keep your own. Applying a theme changes only your draft until you save."
        : "Keep named Desk, Laptop or Presentation setups. Applying one changes your draft; it does not open apps or move windows until you save.",
    ),
  );
  const toolbar = el("div", "toolbar");
  const actions = el("div", "actions");
  const save = button(
    kind === "theme" ? "Save your theme" : "Save this setup",
    () => libraryNameDialog(kind),
  );
  save.disabled = !library || libraryBusy || busy || invalidFields.size > 0;
  const importButton = button(
    `Import ${kind}`,
    () => importLibraryFile(kind),
    "quiet",
  );
  importButton.disabled = !library || libraryBusy;
  const refresh = button("Refresh library", refreshLibrary, "quiet");
  refresh.disabled = libraryLoading || libraryBusy;
  const paste = button(
    `Paste ${kind} data`,
    () => importLibrary(kind),
    "quiet",
  );
  paste.disabled = !library || libraryBusy;
  actions.append(save, importButton, paste, refresh);
  toolbar.append(actions);
  panel.append(toolbar);
  if (libraryError) panel.append(el("div", "notice error", libraryError));
  if (invalidFields.size)
    panel.append(
      el(
        "p",
        "note",
        "Fix the invalid draft fields before saving or applying library items.",
      ),
    );
  const entries = library?.entries.filter((entry) => entry.kind === kind) ?? [];
  const grid = el("div", "library-grid");
  for (const entry of entries) {
    const card = el("article", "library-card");
    const appearance =
      kind === "profile" ? object(entry.values.appearance) : entry.values;
    const palette = object(appearance.palette);
    const swatches = el("div", "swatches");
    swatches.setAttribute("aria-hidden", "true");
    for (const name of ["base", "surface0", "text", "terracotta"]) {
      const swatch = el("i");
      const color = palette[name];
      if (typeof color === "string" && /^#[0-9a-f]{6}$/i.test(color))
        swatch.style.background = color;
      else swatch.style.background = "var(--accent)";
      swatches.append(swatch);
    }
    card.append(
      swatches,
      el("h3", "", entry.name),
      el(
        "p",
        "note",
        entry.builtin
          ? "Built-in theme · light and dark palettes"
          : kind === "profile"
            ? `${array(entry.values.workspaces).length} workspaces · saved locally`
            : "Your saved theme",
      ),
    );
    const apply = button(
      `Apply ${entry.name}`,
      () => applyLibrary(entry),
      "primary",
    );
    apply.disabled = libraryBusy || busy || invalidFields.size > 0;
    const controls = el("div", "actions");
    const exportFile = button(
      "Export",
      () => exportLibraryFile(entry),
      "quiet",
    );
    const share = button("Copy data", () => exportLibrary(entry), "quiet");
    exportFile.disabled = share.disabled = libraryBusy;
    controls.append(apply, exportFile, share);
    if (!entry.builtin)
      controls.append(
        button("Rename", () => libraryNameDialog(kind, entry), "quiet"),
        button("Remove", () => deleteLibrary(entry), "quiet danger"),
      );
    card.append(controls);
    grid.append(card);
  }
  if (!entries.length)
    grid.append(
      el(
        "div",
        "empty",
        libraryLoading
          ? "Reading your local library…"
          : kind === "profile"
            ? "No saved setups yet. Save your current draft to make switching between desk arrangements easier."
            : "No themes are available. Refresh your library to try again.",
      ),
    );
  panel.append(grid);
  if (!library && !libraryLoading && !libraryError) void refreshLibrary();
}
async function desktopControl(
  action: "pause" | "reload" | "restore",
): Promise<void> {
  if (desktopBusy || busy) return;
  desktopBusy = true;
  if (active === "About") renderPage();
  let requested = false;
  try {
    if (
      action === "restore" &&
      !(await confirmAction(
        "Restore your windows and stop Moon?",
        "Moon will stop arranging your desktop and restore the original window positions it retained. Your saved settings and unsaved Settings draft will remain.",
        "Restore windows and stop",
        true,
      ))
    )
      return;
    const snapshot = await request<Desktop>("desktop");
    desktop = snapshot;
    requested = true;
    await request("desktop-action", {
      generation: snapshot.generation,
      revision: snapshot.revision,
      action,
    });
    toast(
      action === "restore"
        ? "Moon has been asked to restore your windows and stop."
        : action === "reload"
          ? "Moon has been asked to read your saved settings."
          : snapshot.paused
            ? "Moon has resumed."
            : "Moon is paused.",
    );
    await refreshStatus();
  } finally {
    desktopBusy = false;
    if (requested && action !== "restore") await refreshDesktop();
    if (active === "About") renderPage();
  }
}
function renderHealth(): void {
  const card = section("Desktop health & recovery");
  const stats = loaded.runtime.stats;
  const paused = stats?.paused === true;
  const suspended = stats?.session_suspended === true;
  card.append(
    el(
      "p",
      "",
      !loaded.runtime.running
        ? "Moon is not managing this desktop."
        : suspended
          ? "Windows has suspended this desktop session. Moon will resume when the session is available."
          : paused
            ? "Moon is paused. Your windows keep their current positions until you resume."
            : "Moon is running and arranging managed windows.",
    ),
  );
  if (stats?.last_error)
    card.append(el("div", "notice error", String(stats.last_error)));
  if (object(stats?.token).ui_access === false)
    card.append(
      el(
        "p",
        "note",
        "This copy has ordinary permissions. Windows from administrator apps may remain outside its control.",
      ),
    );
  card.append(
    el(
      "p",
      "note",
      "Only managed windows appear in Overview. Apps excluded by your rules, special system windows and inaccessible apps can remain outside Moon. Check your Rules page before changing an app’s behavior.",
    ),
  );
  const actions = el("div", "actions");
  for (const [label, action] of [
    [paused ? "Resume Moon" : "Pause Moon", "pause"],
    ["Reload saved settings", "reload"],
    ["Restore windows and stop", "restore"],
  ] as const) {
    const control = button(
      label,
      () => desktopControl(action),
      action === "restore" ? "danger" : "",
    );
    control.disabled = !loaded.runtime.running || suspended || desktopBusy;
    actions.append(control);
  }
  actions.append(button("Check status", refreshStatus, "quiet"));
  card.append(actions);
}

function renderGeneral(): void {
  renderOverview();
  renderStartup();
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
  renderLibrary("theme");
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
    { type: "number", min: 6, max: 72, step: "any" },
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
    { type: "number", min: 0, max: 1, step: "any" },
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
// Use the same licensed symbol font as the native bar. Its private-use glyphs
// are not interchangeable with Segoe Fluent Icons or arbitrary system fonts.
const workspaceSymbols = [
  ["Home", "\uf015"],
  ["Work", "\uf0b1"],
  ["Code", "\uf121"],
  ["Terminal", "\uf120"],
  ["Browser", "\uf0ac"],
  ["Reading", "\uf02d"],
  ["Music", "\uf001"],
  ["Chat", "\uf086"],
  ["Games", "\uf11b"],
  ["Photos", "\uf03e"],
  ["Files", "\uf07b"],
  ["Mail", "\uf0e0"],
] as const;
function selectControl(
  label: string,
  value: string,
  choices: readonly (readonly [string, string])[],
  changed: (value: string) => void,
): HTMLLabelElement {
  const wrap = el("label", "", label);
  const input = el("select");
  input.setAttribute("aria-label", label);
  for (const [id, title] of choices) {
    const option = el("option", "", title);
    option.value = id;
    input.append(option);
  }
  if (!choices.some(([id]) => id === value)) {
    const option = el("option", "", value || "Custom");
    option.value = value;
    input.append(option);
  }
  input.value = value;
  input.addEventListener("change", () => changed(input.value));
  wrap.append(input);
  return wrap;
}
function symbolControl(
  label: string,
  value: string,
  changed: (value: string) => void,
): HTMLElement {
  const wrap = el("div", "symbol-control");
  const preview = el("span", "symbol-preview moon-symbol", value || "—");
  preview.setAttribute("aria-label", `${label} preview`);
  const input = simple(
    label,
    value,
    (next) => {
      preview.textContent = next || "—";
      changed(next);
    },
    { placeholder: "No icon" },
  );
  input.querySelector("input")!.classList.add("moon-symbol");
  const choose = button(
    "Choose icon",
    () =>
      editorDialog(
        label.startsWith("Icon for workspace")
          ? "Choose a workspace icon"
          : "Choose a bar icon",
        (dialog, close) => {
          dialog.append(
            el(
              "p",
              "note",
              "These symbols are included with Moon and also appear in your desktop bar.",
            ),
          );
          const choices = el("div", "symbol-grid");
          choices.setAttribute("role", "group");
          choices.setAttribute("aria-label", "Included icons");
          const commit = (next: string) => {
            input.querySelector("input")!.value = next;
            preview.textContent = next || "—";
            changed(next);
            close();
          };
          for (const [name, glyph] of workspaceSymbols) {
            const item = button("", () => commit(glyph), "symbol-choice");
            item.setAttribute("aria-label", name);
            item.setAttribute(
              "aria-pressed",
              String(input.querySelector("input")!.value === glyph),
            );
            const symbol = el("span", "moon-symbol", glyph);
            symbol.setAttribute("aria-hidden", "true");
            item.append(symbol, el("span", "", name));
            choices.append(item);
          }
          dialog.append(
            choices,
            el(
              "p",
              "note",
              "You can also type text or an emoji in the icon field. Other custom symbols depend on their font.",
            ),
          );
          const actions = el("div", "actions");
          actions.append(
            button("Cancel", close),
            button("No icon", () => commit("")),
          );
          dialog.append(actions);
        },
      ),
    "quiet",
  );
  wrap.append(preview, input, choose);
  return wrap;
}
type StartupState = { available: boolean; enabled: boolean; message: string };
let startupState: StartupState | undefined;
let startupBusy = false;
let startupRequested: boolean | undefined;
let startupError = "";
async function refreshStartup(): Promise<void> {
  if (startupBusy) return;
  startupBusy = true;
  paintStartup();
  try {
    startupState = await request<StartupState>("startup-status");
    startupError = "";
  } catch (error) {
    startupError = error instanceof Error ? error.message : String(error);
  } finally {
    startupBusy = false;
    paintStartup();
  }
}
function paintStartup(): void {
  const card = document.querySelector<HTMLElement>("#startup-control");
  if (!card) return;
  card.replaceChildren();
  const row = el("div", "row");
  const copy = el("div", "copy");
  const id = "launch-at-signin";
  const label = el("label", "", "Launch Moon when I sign in");
  label.htmlFor = id;
  copy.append(
    label,
    el(
      "div",
      "description",
      startupState?.message ||
        (startupBusy
          ? "Checking your installed startup preference…"
          : "Startup preference is unavailable."),
    ),
  );
  const control = el("div", "control");
  const input = el("input");
  input.type = "checkbox";
  input.id = id;
  input.setAttribute("role", "switch");
  input.checked = startupRequested ?? startupState?.enabled ?? false;
  input.disabled = startupBusy || !startupState?.available;
  input.addEventListener("change", () => {
    const enabled = input.checked;
    void (async () => {
      if (startupBusy) return;
      startupBusy = true;
      startupRequested = enabled;
      startupError = "";
      paintStartup();
      try {
        startupState = await request<StartupState>("startup-set", { enabled });
      } catch (error) {
        startupError = error instanceof Error ? error.message : String(error);
      } finally {
        startupBusy = false;
        startupRequested = undefined;
        paintStartup();
        document.querySelector<HTMLInputElement>("#launch-at-signin")?.focus();
      }
    })();
  });
  control.append(input);
  row.append(copy, control);
  card.append(row);
  card.append(
    el(
      "p",
      "note startup-note",
      "This Windows preference changes immediately. Your unsaved desktop settings stay in the draft.",
    ),
  );
  if (startupError) {
    const error = el("div", "notice error", startupError);
    error.setAttribute("role", "alert");
    card.append(error, button("Retry startup status", refreshStartup, "quiet"));
  }
}
function renderStartup(): void {
  const card = section("When you sign in");
  card.id = "startup-control";
  paintStartup();
  if (!startupState && !startupError && !startupBusy) void refreshStartup();
}
type LocationChoice = {
  id?: string;
  label: string;
  latitude: number;
  longitude: number;
};
let locationBusy = false;
let locationQuery = "";
let locationResults: LocationChoice[] = [];
let locationError = "";
let locationSearched = false;
function chooseWeatherLocation(location: LocationChoice): void {
  if (
    !Number.isFinite(location.latitude) ||
    !Number.isFinite(location.longitude) ||
    Math.abs(location.latitude) > 90 ||
    Math.abs(location.longitude) > 180
  )
    throw new Error(
      "That location did not include valid coordinates. Choose another result.",
    );
  stage(["weather"], {
    ...object(draft.weather),
    latitude: location.latitude,
    longitude: location.longitude,
    location: location.label,
    "interval-seconds": object(draft.weather)["interval-seconds"] ?? 900,
  });
  locationResults = [];
  locationError = "";
  locationSearched = false;
  renderPage();
  toast(`Weather location set to ${location.label} in your draft.`);
}
async function findWeatherLocation(current: boolean): Promise<void> {
  if (locationBusy) return;
  if (!current && !locationQuery.trim()) {
    locationError = "Enter a city or town name.";
    paintWeather();
    return;
  }
  const previousWeather = JSON.stringify(draft.weather);
  const previousSource = loaded.snapshot.source;
  locationBusy = true;
  locationError = "";
  locationResults = [];
  locationSearched = false;
  paintWeather();
  try {
    if (current) {
      const location = await request<LocationChoice>("location-current");
      if (
        busy ||
        previousSource !== loaded.snapshot.source ||
        previousWeather !== JSON.stringify(draft.weather)
      )
        throw new Error(
          "Your weather draft changed while finding your location. Choose your location again.",
        );
      chooseWeatherLocation(location);
    } else {
      const result = await request<{ results: LocationChoice[] }>(
        "location-search",
        { query: locationQuery.trim() },
      );
      locationResults = result.results;
      locationSearched = true;
    }
  } catch (error) {
    locationError = error instanceof Error ? error.message : String(error);
  } finally {
    locationBusy = false;
    paintWeather();
  }
}
function paintWeather(): void {
  const card = document.querySelector<HTMLElement>("#weather-control");
  if (!card) return;
  card.replaceChildren();
  const weather = object(draft.weather);
  const heading = el("div", "list-head");
  heading.append(
    el(
      "h3",
      "",
      draft.weather
        ? String(weather.location || "Custom location")
        : "Weather is off",
    ),
  );
  if (draft.weather)
    heading.append(
      button(
        "Turn weather off",
        () => {
          stage(["weather"], null);
          renderPage();
        },
        "quiet",
      ),
    );
  card.append(
    heading,
    el(
      "p",
      "note",
      "Choose a location for the weather shown in your bar. Location requests only happen when you choose an option below. Save changes to apply it.",
    ),
  );
  const actions = el("div", "actions");
  const current = button(
    locationBusy ? "Finding location…" : "Use current location",
    () => findWeatherLocation(true),
  );
  current.disabled = locationBusy;
  actions.append(
    current,
    button(
      "Windows location settings",
      () => request("location-open-settings"),
      "quiet",
    ),
  );
  card.append(actions);
  const form = el("form", "location-search");
  const search = el("input");
  search.type = "search";
  search.placeholder = "City or town";
  search.setAttribute("aria-label", "City or town");
  search.value = locationQuery;
  search.disabled = locationBusy;
  search.addEventListener("input", () => {
    locationQuery = search.value;
  });
  const submit = el("button", "", "Find city");
  submit.type = "submit";
  submit.disabled = locationBusy;
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    void findWeatherLocation(false);
  });
  form.append(search, submit);
  card.append(form);
  if (locationError) {
    const error = el("div", "notice error", locationError);
    error.setAttribute("role", "alert");
    card.append(error);
  }
  const results = el("div", "location-results");
  results.setAttribute("aria-live", "polite");
  if (locationSearched && !locationResults.length)
    results.append(
      el(
        "p",
        "note",
        "No matching places. Try a nearby city or include the country.",
      ),
    );
  for (const location of locationResults)
    results.append(
      button(
        location.label,
        () => chooseWeatherLocation(location),
        "location-choice",
      ),
    );
  card.append(results);
  const credit = el("p", "note source-credit");
  credit.append("Weather data: ");
  const weatherSource = button("Open-Meteo", () =>
    request("location-attribution", { provider: "open-meteo" }),
  );
  weatherSource.title =
    "Weather data and CC BY 4.0 licence (opens your browser)";
  const placesSource = button("GeoNames", () =>
    request("location-attribution", { provider: "geonames" }),
  );
  placesSource.title = "Place names and attribution (opens your browser)";
  credit.append(weatherSource, " (CC BY 4.0) · Places: ", placesSource, ".");
  card.append(credit);
  if (draft.weather) {
    const advanced = el("details");
    advanced.append(el("summary", "", "Coordinates and update interval"));
    field(
      advanced,
      "Latitude",
      "Custom coordinates for the selected location.",
      ["weather", "latitude"],
      { type: "number", min: -90, max: 90, step: "any" },
    );
    field(
      advanced,
      "Longitude",
      "Custom coordinates for the selected location.",
      ["weather", "longitude"],
      { type: "number", min: -180, max: 180, step: "any" },
    );
    advanced
      .querySelectorAll<HTMLInputElement>('input[type="number"]')
      .forEach((input) =>
        input.addEventListener("input", () => {
          if (input.checkValidity()) {
            applyDraft(["weather", "location"], null);
            heading.querySelector("h3")!.textContent = "Custom location";
          }
        }),
      );
    field(
      advanced,
      "Update interval",
      "Seconds between forecasts (at least 60).",
      ["weather", "interval-seconds"],
      { type: "number", min: 60, max: 0xffffffff },
    );
    card.append(advanced);
  }
}
function renderWeather(): void {
  const card = section("Weather location");
  card.id = "weather-control";
  paintWeather();
}
const barKinds = [
  ["logo", "Moon", "☾"],
  ["workspaces", "Workspaces", "{workspaces}"],
  ["clock", "Clock", "{clock}"],
  ["title", "Window title", "{title}"],
  ["state", "Current mode", "{paused}{mode}{direction}"],
  ["media", "Media", "{media}"],
  ["keyboard", "Keyboard", "{keyboard}"],
  ["network", "Network", "{network}"],
  ["memory", "Memory", "{memory}"],
  ["cpu", "CPU", "{cpu}"],
  ["battery", "Battery", "{battery}"],
  ["volume", "Volume", "{volume}"],
  ["weather", "Weather", "{weather}"],
  ["custom", "Custom text", "Moon"],
] as const;
const barActions = [
  ["", "No action"],
  ["overview", "Open window overview"],
  ["cheatsheet", "Open shortcut help"],
  ["settings", "Open Settings"],
  ["pause", "Pause or resume Moon"],
  ["toggle-direction", "Change split direction"],
] as const;
function renderBar(): void {
  content.append(
    el(
      "p",
      "note",
      "Arrange what appears on your desktop bar. All changes stay in the draft until you save.",
    ),
  );
  const toolbar = el("div", "toolbar");
  const existingWidgets = array(get(draft, ["bar", "widgets"]));
  const availableKinds = barKinds.filter(
    ([id]) =>
      id === "custom" ||
      !existingWidgets.some((item) => object(item).id === id),
  );
  let addKind: string = availableKinds[0]![0];
  toolbar.append(
    selectControl(
      "New bar item",
      addKind,
      availableKinds.map(([id, name]) => [id, name]),
      (value) => {
        addKind = value;
      },
    ),
    button("Add bar item", () => {
      const list = clone(array(get(draft, ["bar", "widgets"])));
      const preset = barKinds.find(([id]) => id === addKind)!;
      let id: string = preset[0];
      let suffix = 2;
      while (list.some((item) => object(item).id === id))
        id = `${preset[0]}-${suffix++}`;
      const existingDefault = array(
        object(loaded.snapshot.defaults.bar).widgets,
      ).find((item) => object(item).id === preset[0]);
      list.push({
        ...object(existingDefault),
        id,
        format: preset[2],
        align: "right",
        "interval-seconds": 5,
        "max-width": 300,
      });
      stage(["bar", "widgets"], list);
      renderPage();
    }),
  );
  content.append(toolbar);
  const values = array(get(draft, ["bar", "widgets"]));
  if (!values.length)
    content.append(
      el("div", "card empty", "No bar items. Add an item to build your bar."),
    );
  values.forEach((item, index) => {
    const value = object(item);
    const card = el("details", "card widget-card");
    const name =
      barKinds.find(([id]) => id === value.id)?.[1] ?? String(value.id);
    const summary = el("summary", "widget-summary");
    summary.append(
      el("span", "moon-symbol", String(value.icon || "")),
      el("strong", "", name),
      el("span", "description", String(value.align || "left")),
    );
    card.append(summary);
    const body = el("div", "widget-body");
    const actions = el("div", "actions");
    for (const [title, offset] of [
      ["Move earlier", -1],
      ["Move later", 1],
    ] as const) {
      const move = button(title, () => {
        const next = clone(array(get(draft, ["bar", "widgets"])));
        [next[index], next[index + offset]] = [
          next[index + offset]!,
          next[index]!,
        ];
        stage(["bar", "widgets"], next);
        renderPage();
        const moved =
          document.querySelectorAll<HTMLDetailsElement>(".widget-card")[
            index + offset
          ];
        if (moved) {
          moved.open = true;
          moved.querySelector<HTMLElement>("summary")?.focus();
        }
      });
      move.setAttribute("aria-label", `${title}: ${name}`);
      move.disabled = index + offset < 0 || index + offset >= values.length;
      actions.append(move);
    }
    actions.append(
      button(
        `Remove ${name}`,
        () => {
          stage(
            ["bar", "widgets"],
            array(get(draft, ["bar", "widgets"])).filter((_, i) => i !== index),
          );
          renderPage();
        },
        "quiet danger",
      ),
    );
    body.append(actions);
    const fields = el("div", "fields");
    const update = (key: string, next: Json) =>
      stage(["bar", "widgets", index, key], next);
    fields.append(
      selectControl(
        `Position of ${name}`,
        String(value.align || "left"),
        [
          ["left", "Left"],
          ["center", "Center"],
          ["right", "Right"],
        ],
        (v) => update("align", v),
      ),
      simple(`Text format for ${name}`, value.format ?? "", (v) =>
        update("format", v),
      ),
    );
    const click = String(value.click || "");
    fields.append(
      selectControl(
        `When ${name} is clicked`,
        barActions.some(([id]) => id === click) ? click : "custom",
        [...barActions, ["custom", "Custom command"]],
        (v) => {
          if (v !== "custom") update("click", v || null);
          custom.hidden = v !== "custom";
        },
      ),
    );
    const custom = simple(
      `Custom click command for ${name}`,
      click,
      (v) => update("click", v || null),
      { wide: true },
    );
    custom.hidden = barActions.some(([id]) => id === click);
    fields.append(custom);
    body.append(
      fields,
      el(
        "p",
        "note",
        "Formats can include values such as {clock}, {title}, {cpu}, or {weather}. Optional providers stay hidden when no value is available.",
      ),
    );
    body.append(
      symbolControl(`Icon for ${name}`, String(value.icon || ""), (v) =>
        update("icon", v || null),
      ),
    );
    const style = el("div", "fields");
    for (const [key, label] of [
      ["color", "Text color"],
      ["background", "Background"],
      ["icon-color", "Icon color"],
    ])
      style.append(
        simple(
          `${label} for ${name}`,
          value[key!] ?? "",
          (v) => update(key!, v || null),
          { placeholder: "Default or palette color" },
        ),
      );
    body.append(style);
    const advanced = el("details");
    advanced.append(el("summary", "", "More item settings"));
    field(
      advanced,
      `Maximum width of ${name}`,
      "Logical pixels before text is shortened.",
      ["bar", "widgets", index, "max-width"],
      { type: "number", min: 1, max: 4096 },
    );
    field(
      advanced,
      `Refresh interval for ${name}`,
      "Seconds between custom command updates.",
      ["bar", "widgets", index, "interval-seconds"],
      { type: "number", min: 1, max: 0xffffffff },
    );
    advanced.append(
      simple(`Item ID for ${name}`, value.id!, (v) => update("id", v)),
      simple(
        `Custom output command for ${name}`,
        value.exec ?? "",
        (v) => update("exec", v || null),
        { multiline: true },
      ),
      el(
        "p",
        "note",
        "Custom commands run programs on your PC. Only use commands you trust.",
      ),
    );
    body.append(advanced);
    card.append(body);
    content.append(card);
  });
  renderWeather();
  const advanced = section("Advanced bar configuration");
  jsonEditor(
    advanced,
    "Bar widgets JSON",
    "For configurations that need a raw editor. Existing custom fields are kept when you use the controls above.",
    ["bar", "widgets"],
  );
}
function commonRuleAction(commands: Json[]): {
  action: string;
  workspace: string;
} {
  if (commands.length !== 1) return { action: "custom", workspace: "" };
  const command = String(commands[0]);
  if (["set-floating", "set-tiling", "ignore"].includes(command))
    return { action: command, workspace: "" };
  const match = /^move-to-workspace\s+("(?:[^"\\]|\\.)*"|[^\s"']+)$/.exec(
    command,
  );
  if (match) {
    try {
      return {
        action: "workspace",
        workspace: match[1]!.startsWith('"')
          ? JSON.parse(match[1]!)
          : match[1]!,
      };
    } catch {}
  }
  return { action: "custom", workspace: "" };
}

function renderWorkspaces(): void {
  renderLibrary("profile");
  const values = array(draft.workspaces);
  const toolbar = el("div", "toolbar");
  toolbar.append(
    el(
      "p",
      "note",
      `${values.length} ${values.length === 1 ? "workspace" : "workspaces"} · Order follows the desktop bar.`,
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

      simple("Color", value.color ?? "", (v) => set("color", v || null), {
        placeholder: "Palette name or #d17a4f",
      }),
      simple("Monitor", value.monitor ?? "", (v) => set("monitor", v || null), {
        placeholder: "Automatic",
      }),
    );
    card.append(
      head,
      symbolControl(
        `Icon for workspace ${String(value.name)}`,
        String(value.icon || ""),
        (v) => set("icon", v || null),
      ),
      fields,
    );
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
            const key = chord.trim();
            if (!key) throw new Error("Enter a key chord.");
            if (Object.hasOwn(object(draft.bindings), key))
              throw new Error(
                "That shortcut already exists. Edit its row below.",
              );
            stage(
              ["bindings", key],
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
    el(
      "p",
      "note",
      `${rules.length} personal ${rules.length === 1 ? "rule" : "rules"}`,
    ),
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
      const commandInput = card.querySelector<HTMLTextAreaElement>("textarea");
      if (changed.commands && commandInput)
        commandInput.value = array(changed.commands).join("\n");
    };
    const fields = el("div", "fields");
    const behavior = commonRuleAction(array(rule.commands));
    const workspaceNames = array(draft.workspaces).map((value) =>
      String(object(value).name),
    );
    let destination = behavior.workspace || workspaceNames[0] || "1";
    const advanced = el("details");
    advanced.open = behavior.action === "custom";
    advanced.append(
      el("summary", "", "Advanced commands"),
      el(
        "p",
        "note",
        "Custom command sequences are preserved. Changing the behavior above replaces only this rule’s commands.",
      ),
    );
    fields.append(
      selectControl(
        "App behavior",
        behavior.action,
        [
          ["workspace", "Place in a workspace"],
          ["set-floating", "Keep floating"],
          ["set-tiling", "Keep tiled"],
          ["ignore", "Leave unmanaged"],
          ["custom", "Custom commands"],
        ],
        (value) => {
          destinationField.hidden = value !== "workspace";
          advanced.open = value === "custom";
          if (value !== "custom")
            update({
              commands: [
                value === "workspace"
                  ? `move-to-workspace ${JSON.stringify(destination)}`
                  : value,
              ],
            });
        },
      ),
    );
    const destinationField = selectControl(
      "Destination workspace",
      destination,
      workspaceNames.map((name) => [name, `Workspace ${name}`]),
      (value) => {
        destination = value;
        update({ commands: [`move-to-workspace ${JSON.stringify(value)}`] });
      },
    );
    destinationField.hidden = behavior.action !== "workspace";
    fields.append(destinationField);
    card.append(fields);
    advanced.append(
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
    card.append(advanced);
    array(rule.matches).forEach((entry, matchIndex) => {
      const match = object(entry);
      const group = el("details");
      const appOnly =
        Object.keys(match).length === 1 &&
        Object.keys(object(match.process)).length === 1 &&
        typeof object(match.process).equals === "string";
      group.open = !appOnly;
      let primaryApp: HTMLLabelElement | undefined;
      if (appOnly) {
        const app = simple(
          "App name",
          object(match.process).equals!,
          (value) => {
            const matches = clone(
              array(object(personalRules()[index]).matches),
            );
            matches[matchIndex] = {
              ...object(matches[matchIndex]),
              process: {
                ...object(object(matches[matchIndex]).process),
                equals: value,
              },
            };
            update({ matches });
          },
          { placeholder: "e.g. Code or Arc" },
        );
        primaryApp = app;
        card.append(
          app,
          el(
            "p",
            "note",
            "Use the app’s process name without .exe. Matching ignores ASCII letter case. For window titles, classes or patterns, expand the conditions below.",
          ),
        );
      }
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
      group.addEventListener("toggle", () => {
        const current = object(
          array(object(personalRules()[index]).matches)[matchIndex],
        );
        if (primaryApp) {
          primaryApp.hidden =
            group.open || typeof object(current.process).equals !== "string";
          primaryApp.querySelector("input")!.value = String(
            object(current.process).equals || "",
          );
        }
        if (group.open) {
          for (const [position, key] of [
            "process",
            "class",
            "title",
          ].entries()) {
            const matcher = object(current[key]);
            const mode =
              Object.keys(matcher).find((name) => matcher[name] !== null) ||
              "equals";
            matchFields.querySelectorAll("select")[position]!.value = mode;
            matchFields.querySelectorAll("input")[position]!.value = String(
              matcher[mode] || "",
            );
          }
        }
      });
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
      applyDraft(path, JSON.parse(input.value) as Json);
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
  const everyday = section("Everyday controls");
  everyday.append(
    button("Arrange bar items and weather", () => switchPage("Bar")),
    button("Workspace names and icons", () => switchPage("Workspaces")),
    button("App placement rules", () => switchPage("Rules")),
  );
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
  renderHealth();
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
    "Overview",
    "Find and arrange windows",
    "apps titles search window move floating undo layout",
  ],
  [
    "Workspaces",
    "Saved setups",
    "profiles desk laptop presentation import export",
  ],
  [
    "Appearance",
    "Theme gallery",
    "presets linen slate forest import export palettes",
  ],
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
  ["General", "Launch on sign-in", "startup login automatic launch start"],
  ["Bar", "Bar items", "widgets clock format align actions order weather"],
  ["Advanced", "Bar text & colors", "font palette foreground background"],
  [
    "Bar",
    "Weather",
    "city town current latitude longitude location forecast interval",
  ],
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
  finishWindowDrag();
  clearPreviews();
  updateFooterVisibility();
  content.replaceChildren();
  nextField = 0;
  nav.querySelectorAll("button").forEach((b) => {
    if (b.dataset.page === active && !search)
      b.setAttribute("aria-current", "page");
    else b.removeAttribute("aria-current");
  });
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
    Overview: renderDesktop,
    General: renderGeneral,
    Appearance: renderAppearance,
    Bar: renderBar,
    Workspaces: renderWorkspaces,
    Shortcuts: renderShortcuts,
    Rules: renderRules,
    Advanced: renderAdvanced,
    About: renderAbout,
  })[active]!();
}
let readySent = false;
async function boot(): Promise<void> {
  const help =
    active === "Shortcuts"
      ? refreshShortcuts()
      : active === "Overview"
        ? refreshDesktop()
        : Promise.resolve();
  try {
    loaded = await request<Loaded>("load");
    await help;
    settingsAvailable = true;
    draft = clone(loaded.snapshot.config);
    edits = [];
    undoDraft = undefined;
    renderShell();
    if (standaloneOverview && active === "Overview")
      document.querySelector<HTMLInputElement>("#window-search")?.focus();
  } catch (e) {
    settingsAvailable = false;
    await help;
    if (active === "Overview") {
      renderDesktopOnly(e);
    } else if (active === "Shortcuts") {
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
  if (active === "General" && settingsAvailable) void refreshStartup();
  if (!readySent || search) return;
  if (active === "Overview" && desktopPanel?.isConnected) void refreshDesktop();
  if (active === "Shortcuts" && shortcutPanel?.isConnected)
    void refreshShortcuts();
  if (active === "General" && overviewPanel?.isConnected)
    void refreshOverview();
}
window.moonActivated = refreshVisibleHelp;
window.moonPreviewChanged = schedulePreviews;
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
        !search && active === "Overview"
          ? "#window-search"
          : active === "Shortcuts" && !search
            ? "#shortcut-search"
            : "#search",
      )
      ?.focus();
  }
});
window.addEventListener("resize", schedulePreviews);
document.addEventListener("visibilitychange", () => {
  if (document.hidden) {
    finishWindowDrag();
    clearPreviews();
  } else schedulePreviews();
});
void boot();
