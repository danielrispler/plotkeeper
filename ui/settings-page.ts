// The settings page (#193, #195): a dialog over the window with one scrolling page of rows, each saving at once by editing
// `settings.json` in place (`settings.rs`'s `write_setting`). The rows are one pure function from the parsed files, `settingsPage`;
// the dialog below only draws them and sends each change.

import { element, invoke, patch, text } from "./dom.js";
import { titled } from "./keys.js";
import { FONT_MAX, FONT_MIN, initial, ZOOM_MAX, ZOOM_MIN, type Files, type Settings, type SettingsFiles } from "./settings.js";
import { studio } from "./state.js";

// A row's key in `settings.json`, such as `["terminal", "font_size"]`, and the control that sets it. A new control adds its kind here
// and to `Row`, and its rows to `SECTIONS`; `settingsPage` and the write stay as they are.
type Spec = { title: string; description: string } & (
  | { kind: "toggle"; path: string[] }
  | { kind: "stepper"; path: string[]; min: number; max: number; step: number }
  | { kind: "select"; path: string[]; options: [value: string, label: string][] }
  | { kind: "open"; file: "settings" | "keymap" }
);

// The page's rows, hand-written as Zed's and herdr's are. A setting's default is the UI's own (`initial`), so one source decides
// both the parser's default and the page's reset. Descriptions come from the comments in `docs/studio/settings.md`.
const SECTIONS: { heading: string; rows: Spec[] }[] = [
  {
    heading: "Appearance",
    rows: [
      { kind: "stepper", path: ["zoom"], min: ZOOM_MIN, max: ZOOM_MAX, step: 0.1, title: "Default zoom", description: "The window's zoom when it opens. ⌘+, ⌘− and ⌘0 change it for now only." },
      { kind: "stepper", path: ["terminal", "font_size"], min: FONT_MIN, max: FONT_MAX, step: 1, title: "Terminal font size", description: "The terminal's text size, 8 to 40. Unset, it follows the stylesheet." },
    ],
  },
  {
    heading: "Alerts",
    rows: [
      { kind: "toggle", path: ["sounds"], title: "Sounds", description: "Play a sound when a session needs you or it is your turn." },
      {
        kind: "toggle",
        path: ["notifications", "while_focused"],
        title: 'Notify "your turn" while focused',
        description: "Also notify while the window is focused, for any session but the selected one.",
      },
    ],
  },
  {
    heading: "Summaries",
    rows: [
      {
        kind: "select",
        path: ["summaries", "model"],
        options: [["haiku", "Haiku"], ["sonnet", "Sonnet"], ["opus", "Opus"], ["fable", "Fable"]],
        title: "Model",
        description: "The model that writes a turn's summary.",
      },
      {
        kind: "select",
        path: ["summaries", "effort"],
        options: [["", "Default"], ["low", "low"], ["medium", "medium"], ["high", "high"], ["xhigh", "xhigh"], ["max", "max"]],
        title: "Effort",
        description: "The effort the summary writer works at; Default passes no --effort.",
      },
      { kind: "toggle", path: ["summaries", "on_open"], title: "Write a summary when a turn is opened", description: "Write a turn's summary when you open it, not only when it ends." },
    ],
  },
  {
    heading: "Files",
    rows: [
      { kind: "open", file: "settings", title: "Open settings.json", description: "Every setting, including those this page does not show." },
      { kind: "open", file: "keymap", title: "Open keymap.json", description: "Your keys for the studio's actions." },
    ],
  },
];

// What a row shows. `value` is the value in effect, a key of the wrong type being its default; `changed`: it differs from the default,
// so the undo icon shows. `disabled`: the file does not parse.
export type Row = { title: string; description: string; disabled: boolean } & (
  | { control: "toggle"; path: string[]; value: boolean; changed: boolean }
  | { control: "stepper"; path: string[]; value: number; changed: boolean; min: number; max: number; step: number }
  // `options` holds a value from the file that the list lacks, as its own entry, so opening the page never changes it.
  | { control: "select"; path: string[]; value: string; changed: boolean; options: { value: string; label: string }[] }
  | { control: "open"; file: "settings" | "keymap" }
);
export type Page = { error: string | null; sections: { heading: string; rows: Row[] }[] };

const valueAt = (settings: Settings, path: string[]): unknown =>
  path.reduce<unknown>((at, name) => (at as Record<string, unknown> | undefined)?.[name], settings);

export const BROKEN = "settings.json has errors: fix it first";

// The value a stepper's button gives, `by` steps up or down from the row's value, inside its bounds, to one decimal.
export const stepped = ({ value, min, max, step }: { value: number; min: number; max: number; step: number }, by: 1 | -1) =>
  Math.min(max, Math.max(min, Math.round((value + by * step) * 10) / 10));

// `sheetSize` is the stylesheet's terminal size, what an unset font size shows.
export function settingsPage({ settings, broken }: Files, sheetSize: number): Page {
  const row = (spec: Spec): Row => {
    const { title, description } = spec;
    const base = { title, description, disabled: broken };
    if (spec.kind === "open") return { ...base, control: "open", file: spec.file };
    const { path } = spec;
    // A setting with no default of its own (an unset font size) is the stylesheet's.
    const value = valueAt(settings, path) ?? sheetSize;
    const changed = value !== (valueAt(initial.settings, path) ?? sheetSize);
    if (spec.kind === "toggle") return { ...base, control: "toggle", path, value: value as boolean, changed };
    if (spec.kind === "stepper") return { ...base, control: "stepper", path, value: value as number, changed, min: spec.min, max: spec.max, step: spec.step };
    const options = spec.options.map(([value, label]) => ({ value, label }));
    if (!options.some((o) => o.value === value)) options.push({ value: value as string, label: value as string });
    return { ...base, control: "select", path, value: value as string, changed, options };
  };
  return { error: broken ? BROKEN : null, sections: SECTIONS.map(({ heading, rows }) => ({ heading, rows: rows.map(row) })) };
}

// The dialog.

const UNDO = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 14L4 9l5-5"/><path d="M4 9h10a6 6 0 010 12h-3"/></svg>';

// Why the last write was refused, shown at the top of the page until the next change.
let refused: string | null = null;
// The writes in the order they were made: a second change waits for the first, so it carries the text the first wrote.
let queue: Promise<void> = Promise.resolve();
// What the window does with the text a write returned: parse it as a reload would, without waiting for the file's change.
let saved: (files: SettingsFiles) => void = () => {};

// The page as the window shows it: the stylesheet's `--size-mono` is what an unset terminal font size shows.
const current = () => settingsPage(studio.files, parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--size-mono")));
const dialog = () => element("settings-dialog") as HTMLDialogElement;
const keyOf = (row: Row) => (row.control === "open" ? `open:${row.file}` : row.path.join("."));

function drawRow(row: Row): HTMLElement {
  const node = text("div", "", "setting");
  node.dataset.key = keyOf(row);
  const title = text("div", "", "setting-title");
  title.append(row.title);
  const info = text("div", "", "setting-text");
  info.append(title, text("div", row.description, "setting-description"));
  if (row.control === "open") {
    const button = text("button", "Open", "btn open");
    button.setAttribute("type", "button");
    button.dataset.action = "open";
    button.dataset.key = keyOf(row);
    button.toggleAttribute("disabled", row.disabled);
    button.setAttribute("aria-label", row.title);
    node.append(info, button);
    return node;
  }
  if (row.changed) {
    const undo = text("button", "", "btn quiet reset");
    undo.setAttribute("type", "button");
    undo.dataset.action = "reset";
    undo.dataset.key = keyOf(row);
    undo.title = "Reset to default";
    undo.setAttribute("aria-label", `Reset ${row.title} to default`);
    undo.toggleAttribute("disabled", row.disabled);
    undo.innerHTML = UNDO;
    title.append(undo);
  }
  node.append(info, control(row));
  return node;
}

// The row's control, other than the file rows' button.
function control(row: Exclude<Row, { control: "open" }>): HTMLElement {
  if (row.control === "select") {
    const select = text("select", "", "select");
    select.dataset.action = "select";
    select.dataset.key = keyOf(row);
    select.dataset.value = row.value;
    select.setAttribute("aria-label", row.title);
    select.toggleAttribute("disabled", row.disabled);
    select.append(...row.options.map((o) => text("option", o.label)));
    row.options.forEach((o, i) => ((select.children[i] as HTMLOptionElement).value = o.value));
    return select;
  }
  if (row.control === "stepper") {
    const group = text("div", "", "stepper");
    const button = (by: 1 | -1) => {
      const b = text("button", by === 1 ? "+" : "−", "btn quiet");
      b.setAttribute("type", "button");
      b.setAttribute("aria-label", `${by === 1 ? "Increase" : "Decrease"} ${row.title}`);
      b.dataset.action = by === 1 ? "up" : "down";
      b.dataset.key = keyOf(row);
      b.toggleAttribute("disabled", row.disabled || stepped(row, by) === row.value);
      return b;
    };
    group.append(button(-1), text("span", String(row.value), "stepper-value"), button(1));
    return group;
  }
  const toggle = text("button", "", "btn switch");
  toggle.setAttribute("type", "button");
  toggle.setAttribute("role", "switch");
  toggle.setAttribute("aria-checked", String(row.value));
  toggle.setAttribute("aria-label", row.title);
  toggle.dataset.action = "toggle";
  toggle.dataset.key = keyOf(row);
  toggle.toggleAttribute("disabled", row.disabled);
  return toggle;
}

// Draws the page from `studio.files`; an open dialog is patched, so a focused control stays focused.
export function drawSettingsPage() {
  const page = current();
  const lines: HTMLElement[] = [];
  if (page.error !== null) {
    const line = text("div", "", "broken");
    line.dataset.key = "broken";
    const open = text("button", "Open settings.json", "btn");
    open.setAttribute("type", "button");
    open.dataset.action = "open";
    open.dataset.key = "open:settings";
    line.append(text("span", page.error), open);
    lines.push(line);
  }
  if (refused !== null) {
    const line = text("div", refused, "refused");
    line.dataset.key = "refused";
    lines.push(line);
  }
  const sections = page.sections.flatMap(({ heading, rows }) => [text("h2", heading), ...rows.map(drawRow)]);
  patch(element("settings-body"), [...lines, ...sections]);
  // A dropdown's selection is a property the patch does not carry: set it from the row.
  for (const select of element("settings-body").querySelectorAll("select")) select.value = select.dataset.value ?? "";
  element("settings-close").title = titled(studio.files.keymap, "Close", "studio::Dismiss");
}

const rowFor = (key: string) => current().sections.flatMap((s) => s.rows).find((row) => keyOf(row) === key);

// Writes `value` to `path` in `settings.json`, or removes the key when null, from the text the window last read.
function write(path: string[], value: boolean | number | string | null) {
  queue = queue.then(async () => {
    try {
      const written = await invoke("write_setting", { expected: studio.texts.settings, path, value });
      refused = null;
      saved({ ...studio.texts, settings: written });
    } catch (e) {
      refused = String(e);
    }
    drawSettingsPage();
  });
}

function open(file: "settings" | "keymap") {
  const command = file === "settings" ? "open_settings" : "open_keymap";
  invoke(command, { editor: studio.files.settings.editor }).catch((e) => console.warn(e));
}

export const settingsPageOpen = () => dialog().open;
export function closeSettingsPage() {
  if (dialog().open) dialog().close();
}
export function showSettingsPage() {
  refused = null;
  drawSettingsPage();
  if (!dialog().open) dialog().showModal();
}

// Wires the dialog's buttons; `apply` is what the window does with a settings file's new text.
export function startSettingsPage(apply: (files: SettingsFiles) => void) {
  saved = apply;
  element("settings-close").addEventListener("click", closeSettingsPage);
  // A click on the backdrop lands on the dialog itself: the frame fills it.
  dialog().addEventListener("click", (event) => event.target === dialog() && closeSettingsPage());
  const change = (target: EventTarget | null, what: "click" | "change") => {
    const button = (target as Element).closest<HTMLElement>("[data-action]");
    const row = button?.dataset.key ? rowFor(button.dataset.key) : undefined;
    if (!button || !row || (button as HTMLButtonElement).disabled) return;
    const action = button.dataset.action;
    if (row.control === "open") open(row.file);
    else if (action === "reset") write(row.path, null);
    else if (row.control === "toggle") write(row.path, !row.value);
    else if (row.control === "stepper") write(row.path, stepped(row, action === "up" ? 1 : -1));
    else if (row.control === "select" && what === "change") write(row.path, (button as HTMLSelectElement).value);
  };
  element("settings-body").addEventListener("click", (event) => change(event.target, "click"));
  element("settings-body").addEventListener("change", (event) => change(event.target, "change"));
  window.addEventListener("settings", () => dialog().open && drawSettingsPage());
}
