// The settings page (#193, #195): a dialog over the window with one scrolling page of rows, each saving at once by editing
// `settings.json` in place (`settings.rs`'s `write_setting`). The rows are one pure function from the parsed files, `settingsPage`;
// the dialog below only draws them and sends each change.

import { element, failure, invoke, patch, text } from "./dom.js";
import { titled } from "./keys.js";
import { FONT_MAX, FONT_MIN, initial, ZOOM_MAX, ZOOM_MIN, type Files, type Settings, type SettingsFiles } from "./settings.js";
import { studio } from "./state.js";

// The two files the page opens.
type FileName = "settings" | "keymap";

// A row's key in `settings.json`, such as `["terminal", "font_size"]`, and the control that sets it. A new control adds its kind here
// and to `Row`, and its rows to `SECTIONS`; `settingsPage` and the write stay as they are.
type Spec = { title: string; description: string } & (
  | { kind: "toggle"; path: string[] }
  // `zero`: what the stepper shows at 0, for a setting that 0 turns off.
  | { kind: "stepper"; path: string[]; min: number; max: number; step: number; zero?: string }
  | { kind: "select"; path: string[]; options: [value: string, label: string][] }
  | { kind: "open"; file: FileName }
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
        kind: "stepper",
        path: ["context_warning"],
        min: 0,
        max: 100,
        step: 5,
        zero: "Off",
        title: "Warn at context share",
        description: "Note a session once its context share passes this percent; Off never does.",
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

// What a row shows. `value` is the value in effect, a key of the wrong type being its default; `defaultValue` is the default, and
// `changed`: the value differs from it, so the undo icon shows. `disabled`: the file does not parse.
export type Row = { title: string; description: string; disabled: boolean } & (
  | { control: "toggle"; path: string[]; value: boolean; defaultValue: boolean; changed: boolean }
  | { control: "stepper"; path: string[]; value: number; defaultValue: number; changed: boolean; min: number; max: number; step: number; zero: string | null }
  // `options` holds a value from the file that the list lacks, as its own entry, so opening the page never changes it.
  | { control: "select"; path: string[]; value: string; defaultValue: string; changed: boolean; options: { value: string; label: string }[] }
  | { control: "open"; file: FileName }
);
export type Page = { error: string | null; sections: { heading: string; rows: Row[] }[] };

const valueAt = (settings: Settings, path: string[]): unknown =>
  path.reduce<unknown>((at, name) => (at as Record<string, unknown> | undefined)?.[name], settings);

// What a button or dropdown's `data-action` can be.
export type Action = "reset" | "toggle" | "up" | "down" | "select" | "open";

// Whether the event `what` on a control for `action` sends a write: a dropdown writes on its change, not on the click that opens
// it; every button writes on its click (the undo icon of a dropdown's row included).
export const writesOn = (action: Action, what: "click" | "change"): boolean => action !== "select" || what === "change";

export const BROKEN = "settings.json has errors: fix it first";

// The value a stepper's button gives, `by` steps up or down from the row's value, inside its bounds, to one decimal. A value
// between two steps (the stylesheet's 12.5) goes to the next whole step in that direction, 13 or 12.
export const stepped = ({ value, min, max, step }: { value: number; min: number; max: number; step: number }, by: 1 | -1) => {
  const at = Math.round((value / step) * 1e6) / 1e6;
  const to = Number.isInteger(at) ? at + by : by === 1 ? Math.ceil(at) : Math.floor(at);
  return Math.min(max, Math.max(min, Math.round(to * step * 10) / 10));
};

// What a control's change writes to the file, from the row as it is when the write runs: null removes the key, which is what a
// reset does and what a change to the row's own default does too, so the page never writes a key it cannot undo. `picked` is the
// dropdown's new value.
export function newValue(row: Row, action: Action, picked = ""): boolean | number | string | null {
  if (row.control === "open" || action === "reset") return null;
  const value = row.control === "toggle" ? !row.value : row.control === "stepper" ? stepped(row, action === "up" ? 1 : -1) : picked;
  return value === row.defaultValue ? null : value;
}

// `sheetSize` is the stylesheet's terminal size, what an unset font size shows.
export function settingsPage({ settings, broken }: Files, sheetSize: number): Page {
  const row = (spec: Spec): Row => {
    const { title, description } = spec;
    const base = { title, description, disabled: broken };
    if (spec.kind === "open") return { ...base, control: "open", file: spec.file };
    const { path } = spec;
    // A setting with no default of its own (an unset font size) is the stylesheet's.
    const value = valueAt(settings, path) ?? sheetSize;
    const defaultValue = valueAt(initial.settings, path) ?? sheetSize;
    const changed = value !== defaultValue;
    if (spec.kind === "toggle") return { ...base, control: "toggle", path, value: value as boolean, defaultValue: defaultValue as boolean, changed };
    if (spec.kind === "stepper") {
      const { min, max, step, zero = null } = spec;
      return { ...base, control: "stepper", path, value: value as number, defaultValue: defaultValue as number, changed, min, max, step, zero };
    }
    const options = spec.options.map(([value, label]) => ({ value, label }));
    if (!options.some((o) => o.value === value)) options.push({ value: value as string, label: value as string });
    return { ...base, control: "select", path, value: value as string, defaultValue: defaultValue as string, changed, options };
  };
  return { error: broken ? BROKEN : null, sections: SECTIONS.map(({ heading, rows }) => ({ heading, rows: rows.map(row) })) };
}

// The dialog.

const UNDO = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 14L4 9l5-5"/><path d="M4 9h10a6 6 0 010 12h-3"/></svg>';

// Why the last write was refused and the settings text it was refused against, shown at the top of the page until the next
// change or until the file's text changes, as an edit saved from an editor does.
let refused: { why: string; at: string | null } | null = null;
// The writes in the order they were made: a second change waits for the first, so it carries the text the first wrote.
let queue: Promise<void> = Promise.resolve();
// What the window does with the text a write returned: parse it as a reload would, without waiting for the file's change.
let saved: (files: SettingsFiles) => void = () => {};

// The page as the window shows it: the stylesheet's `--size-mono` is what an unset terminal font size shows.
const current = () => settingsPage(studio.files, parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--size-mono")));
const dialog = () => element("settings-dialog") as HTMLDialogElement;
const keyOf = (row: Row) => (row.control === "open" ? `open:${row.file}` : row.path.join("."));

// A button (`more` is its classes beside the component's) for the page's `action`, on the row `key`: it works through its dataset, which the page's listeners read.
// `hover` says what it does, or, disabled, why it is off (#220).
function button(label: string, more: string, action: Action, key: string, disabled: boolean, hover: string): HTMLElement {
  const b = text("button", label, `btn ${more}`);
  b.setAttribute("type", "button");
  b.dataset.action = action;
  b.dataset.key = key;
  b.toggleAttribute("disabled", disabled);
  b.title = hover;
  return b;
}

function drawRow(row: Row): HTMLElement {
  const node = text("div", "", "setting");
  node.dataset.key = keyOf(row);
  const title = text("div", "", "setting-title");
  title.append(row.title);
  const info = text("div", "", "setting-text");
  info.append(title, text("div", row.description, "setting-description"));
  if (row.control === "open") {
    const open = button("Open", "open", "open", keyOf(row), row.disabled, row.disabled ? BROKEN : `Open ${row.file}.json in your editor`);
    open.setAttribute("aria-label", row.title);
    node.append(info, open);
    return node;
  }
  if (row.changed) {
    const undo = button("", "quiet reset", "reset", keyOf(row), row.disabled, row.disabled ? BROKEN : "Reset to default");
    undo.setAttribute("aria-label", `Reset ${row.title} to default`);
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
    const step = (by: 1 | -1) => {
      const atBound = stepped(row, by) === row.value;
      const hover = row.disabled ? BROKEN : atBound ? `Already at the ${by === 1 ? "highest" : "lowest"} value` : `${by === 1 ? "Increase" : "Decrease"} ${row.title.toLowerCase()}`;
      const b = button(by === 1 ? "+" : "−", "quiet", by === 1 ? "up" : "down", keyOf(row), row.disabled || atBound, hover);
      b.setAttribute("aria-label", `${by === 1 ? "Increase" : "Decrease"} ${row.title}`);
      return b;
    };
    group.append(step(-1), text("span", row.value === 0 && row.zero ? row.zero : String(row.value), "stepper-value"), step(1));
    return group;
  }
  const toggle = button("", "switch", "toggle", keyOf(row), row.disabled, row.disabled ? BROKEN : `Turn ${row.value ? "off" : "on"}: ${row.title}`);
  toggle.setAttribute("role", "switch");
  toggle.setAttribute("aria-checked", String(row.value));
  toggle.setAttribute("aria-label", row.title);
  return toggle;
}

// Draws the page from `studio.files`; an open dialog is patched, so a focused control stays focused. The error lines are drawn in
// their own element above the rows, so one coming or going never moves the rows' places in the patch.
export function drawSettingsPage() {
  const page = current();
  const lines: HTMLElement[] = [];
  if (page.error !== null) {
    const line = text("div", "", "broken");
    line.dataset.key = "broken";
    line.append(text("span", page.error), button("Open settings.json", "", "open", "open:settings", false, "Open settings.json in your editor to fix it"));
    lines.push(line);
  }
  if (refused !== null) {
    const line = text("div", refused.why, "refused");
    line.dataset.key = "refused";
    lines.push(line);
  }
  const sections = page.sections.flatMap(({ heading, rows }) => [text("h2", heading), ...rows.map(drawRow)]);
  patch(element("settings-status"), lines);
  patch(element("settings-body"), sections);
  // A dropdown's selection is a property the patch does not carry: set it from the row.
  for (const select of element("settings-body").querySelectorAll("select")) select.value = select.dataset.value ?? "";
}

const rowFor = (key: string) => current().sections.flatMap((s) => s.rows).find((row) => keyOf(row) === key);

// Changes the row `key` as `action` says (`newValue`), saving it to `settings.json` from the text the window last read. The value is
// worked out when the write runs, from the row as the write before it left it, so two quick clicks on + step twice.
function write(key: string, action: Action, picked?: string) {
  queue = queue.then(async () => {
    const row = rowFor(key);
    if (!row || row.control === "open" || row.disabled) return;
    const expected = studio.texts.settings;
    try {
      const written = await invoke("write_setting", { expected, path: row.path, value: newValue(row, action, picked) });
      refused = null;
      saved({ ...studio.texts, settings: written });
    } catch (e) {
      failure("write_setting", "error")(e);
      // The window's text may be older than the file's (a write's reply that crossed an editor's save): read both again, so the next try is made against the file as it is.
      try {
        saved(await invoke("settings_files"));
      } catch (again) {
        failure("settings_files")(again);
      }
      // Set after the read, so the line stays until the text changes again or a write succeeds.
      refused = { why: String(e), at: studio.texts.settings };
    }
    drawSettingsPage();
  });
}

function openFile(file: FileName) {
  const command = file === "settings" ? "open_settings" : "open_keymap";
  invoke(command, { editor: studio.files.settings.editor }).catch(failure(command, "error"));
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
    const clicked = (target as Element).closest<HTMLElement>("[data-action]");
    const row = clicked?.dataset.key ? rowFor(clicked.dataset.key) : undefined;
    if (!clicked || !row || (clicked as HTMLButtonElement).disabled) return;
    const { action, key = "" } = clicked.dataset as { action: Action; key?: string };
    if (row.control === "open") openFile(row.file);
    else if (writesOn(action, what)) write(key, action, (clicked as HTMLSelectElement).value);
  };
  element("settings-body").addEventListener("click", (event) => change(event.target, "click"));
  element("settings-body").addEventListener("change", (event) => change(event.target, "change"));
  window.addEventListener("settings", () => {
    // The settings text changed (an edit saved elsewhere, say): a refusal against the older text no longer holds.
    if (refused && refused.at !== studio.texts.settings) refused = null;
    if (dialog().open) drawSettingsPage();
  });
}
