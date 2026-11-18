// The settings page (#193, #195): a dialog over the window with one scrolling page of rows, each saving at once by editing
// `settings.json` in place (`settings.rs`'s `write_setting`). The rows are one pure function from the parsed files, `settingsPage`;
// the dialog below only draws them and sends each change.

import { element, invoke, patch, text } from "./dom.js";
import { titled } from "./keys.js";
import { initial, type Files, type Settings, type SettingsFiles } from "./settings.js";
import { studio } from "./state.js";

// A row's key in `settings.json`, such as `["terminal", "font_size"]`, and the control that sets it. A new control (a stepper, a
// dropdown) adds its kind here and to `Row`, and its rows to `SECTIONS`; `settingsPage` and the write stay as they are.
type Spec = { title: string; description: string } & ({ kind: "toggle"; path: string[] } | { kind: "open"; file: "settings" | "keymap" });

// The page's rows, hand-written as Zed's and herdr's are. A setting's default is the UI's own (`initial`), so one source decides
// both the parser's default and the page's reset. Descriptions come from the comments in `docs/studio/settings.md`.
const SECTIONS: { heading: string; rows: Spec[] }[] = [
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
  | { control: "open"; file: "settings" | "keymap" }
);
export type Page = { error: string | null; sections: { heading: string; rows: Row[] }[] };

const valueAt = (settings: Settings, path: string[]): unknown =>
  path.reduce<unknown>((at, name) => (at as Record<string, unknown> | undefined)?.[name], settings);

export const BROKEN = "settings.json has errors: fix it first";

export function settingsPage({ settings, broken }: Files): Page {
  const row = (spec: Spec): Row => {
    const { title, description } = spec;
    if (spec.kind === "open") return { title, description, disabled: broken, control: "open", file: spec.file };
    const value = valueAt(settings, spec.path) as boolean;
    return { title, description, disabled: broken, control: "toggle", path: spec.path, value, changed: value !== valueAt(initial.settings, spec.path) };
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
  const toggle = text("button", "", "btn switch");
  toggle.setAttribute("type", "button");
  toggle.setAttribute("role", "switch");
  toggle.setAttribute("aria-checked", String(row.value));
  toggle.setAttribute("aria-label", row.title);
  toggle.dataset.action = "toggle";
  toggle.dataset.key = keyOf(row);
  toggle.toggleAttribute("disabled", row.disabled);
  node.append(info, toggle);
  return node;
}

// Draws the page from `studio.files`; an open dialog is patched, so a focused control stays focused.
export function drawSettingsPage() {
  const page = settingsPage(studio.files);
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
  element("settings-close").title = titled(studio.files.keymap, "Close", "studio::Dismiss");
}

const rowFor = (key: string) => settingsPage(studio.files).sections.flatMap((s) => s.rows).find((row) => keyOf(row) === key);

// Writes `value` to `path` in `settings.json`, or removes the key when null, from the text the window last read.
function write(path: string[], value: boolean | null) {
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
  element("settings-body").addEventListener("click", (event) => {
    const button = (event.target as Element).closest<HTMLElement>("[data-action]");
    const row = button?.dataset.key ? rowFor(button.dataset.key) : undefined;
    if (!button || !row || (button as HTMLButtonElement).disabled) return;
    if (row.control === "open") open(row.file);
    else write(row.path, button.dataset.action === "reset" ? null : !row.value);
  });
  window.addEventListener("settings", () => dialog().open && drawSettingsPage());
}
