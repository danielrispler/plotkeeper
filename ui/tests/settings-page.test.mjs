// The settings page's rows (#195), run on the `tsc` output: one pure function from the parsed settings files to what each row shows.
import { test } from "node:test";
import assert from "node:assert/strict";
import { initial, reload, reloadedZoom, zoomed } from "../../dist/settings.js";
import { settingsPage as pageOf, stepped } from "../../dist/settings-page.js";

// The stylesheet's terminal size, which the window reads from `--size-mono`.
const SHEET = 12.5;
const settingsPage = (files) => pageOf(files, SHEET);
const load = (settings, previous = initial) => reload(previous, { settings, keymap: null });
const rows = (page) => page.sections.flatMap((section) => section.rows);
const row = (page, title) => rows(page).find((r) => r.title === title);

test("the page has its four headings, with their rows", () => {
  const page = settingsPage(initial);
  assert.deepEqual(page.sections.map((s) => s.heading), ["Appearance", "Alerts", "Summaries", "Files"]);
  assert.deepEqual(rows(page).map((r) => r.title), [
    "Default zoom",
    "Terminal font size",
    "Sounds",
    'Notify "your turn" while focused',
    "Model",
    "Effort",
    "Write a summary when a turn is opened",
    "Open settings.json",
    "Open keymap.json",
  ]);
  assert.ok(rows(page).every((r) => r.description), "each row says what it does");
  assert.equal(page.error, null);
});

test("a row at its default shows no undo icon, and a changed one does", () => {
  const defaults = settingsPage(initial);
  assert.equal(row(defaults, "Sounds").value, true);
  assert.equal(row(defaults, "Sounds").changed, false);
  assert.equal(row(defaults, 'Notify "your turn" while focused').changed, false);
  const changed = settingsPage(load(`{ "sounds": false, "notifications": { "while_focused": true } }`));
  assert.equal(row(changed, "Sounds").value, false);
  assert.equal(row(changed, "Sounds").changed, true);
  assert.equal(row(changed, 'Notify "your turn" while focused').value, true);
  assert.equal(row(changed, 'Notify "your turn" while focused').changed, true);
  // Set to the default's own value in the file, it is not changed.
  assert.equal(row(settingsPage(load(`{ "sounds": true }`)), "Sounds").changed, false);
});

test("a row names the key it writes", () => {
  const page = settingsPage(initial);
  assert.deepEqual(row(page, "Sounds").path, ["sounds"]);
  assert.deepEqual(row(page, 'Notify "your turn" while focused').path, ["notifications", "while_focused"]);
});

test("a file that does not parse disables every control, says so, and shows the settings in effect", () => {
  const last = load(`{ "sounds": false }`);
  const page = settingsPage(load(`{ "sounds": `, last));
  assert.equal(page.error, "settings.json has errors: fix it first");
  assert.ok(rows(page).every((r) => r.disabled), "every control is disabled");
  assert.equal(row(page, "Sounds").value, false);
  assert.equal(row(page, "Sounds").changed, true);
  const fine = settingsPage(last);
  assert.equal(fine.error, null);
  assert.ok(rows(fine).every((r) => !r.disabled));
});

test("a key of the wrong type reads as its default and is not a broken file", () => {
  const page = settingsPage(load(`{ "sounds": "yes", "notifications": { "while_focused": 3 } }`));
  assert.equal(row(page, "Sounds").value, true);
  assert.equal(row(page, "Sounds").changed, false);
  assert.equal(row(page, 'Notify "your turn" while focused').value, false);
  assert.equal(page.error, null);
  assert.ok(rows(page).every((r) => !r.disabled));
});

test("only the file rows open a file", () => {
  const files = rows(settingsPage(initial)).filter((r) => r.control === "open");
  assert.deepEqual(files.map((r) => r.file), ["settings", "keymap"]);
});

test("an unset terminal font size shows the stylesheet's size and no undo icon", () => {
  const size = row(settingsPage(initial), "Terminal font size");
  assert.equal(size.control, "stepper");
  assert.equal(size.value, SHEET);
  assert.equal(size.changed, false);
  assert.deepEqual(size.path, ["terminal", "font_size"]);
  const set = row(settingsPage(load(`{ "terminal": { "font_size": 16 } }`)), "Terminal font size");
  assert.equal(set.value, 16);
  assert.equal(set.changed, true);
});

test("Default zoom is a stepper over the file's zoom, and 1.2 shows the undo icon", () => {
  const defaults = row(settingsPage(initial), "Default zoom");
  assert.equal(defaults.control, "stepper");
  assert.deepEqual([defaults.value, defaults.changed, defaults.min, defaults.max], [1, false, 0.5, 2]);
  assert.deepEqual(defaults.path, ["zoom"]);
  const zoomed = row(settingsPage(load(`{ "zoom": 1.2 }`)), "Default zoom");
  assert.equal(zoomed.value, 1.2);
  assert.equal(zoomed.changed, true);
});

test("steppers stay within their bounds and step without float noise", () => {
  const zoom = row(settingsPage(initial), "Default zoom");
  const size = row(settingsPage(initial), "Terminal font size");
  assert.equal(stepped(zoom, 1), 1.1);
  assert.equal(stepped({ ...zoom, value: 0.7 }, 1), 0.8);
  assert.equal(stepped({ ...zoom, value: 2 }, 1), 2);
  assert.equal(stepped({ ...zoom, value: 0.5 }, -1), 0.5);
  assert.deepEqual([size.min, size.max], [8, 40]);
  assert.equal(stepped({ ...size, value: 40 }, 1), 40);
  assert.equal(stepped({ ...size, value: 8 }, -1), 8);
  assert.equal(stepped(size, 1), 13.5);
});

test("the model dropdown offers Haiku, Sonnet, Opus and Fable, and a model from the file that is not listed is its own selected entry", () => {
  const listed = row(settingsPage(initial), "Model");
  assert.equal(listed.control, "select");
  assert.deepEqual(listed.options, [
    { value: "haiku", label: "Haiku" },
    { value: "sonnet", label: "Sonnet" },
    { value: "opus", label: "Opus" },
    { value: "fable", label: "Fable" },
  ]);
  assert.equal(listed.value, "haiku");
  assert.equal(listed.changed, false);
  assert.deepEqual(listed.path, ["summaries", "model"]);
  const other = row(settingsPage(load(`{ "summaries": { "model": "claude-opus-4-1" } }`)), "Model");
  assert.equal(other.value, "claude-opus-4-1");
  assert.equal(other.changed, true);
  assert.deepEqual(other.options.at(-1), { value: "claude-opus-4-1", label: "claude-opus-4-1" });
  assert.equal(other.options.length, 5);
  assert.equal(row(settingsPage(load(`{ "summaries": { "model": "opus" } }`)), "Model").options.length, 4);
});

test("the effort dropdown shows an empty effort as Default, then low to max", () => {
  const effort = row(settingsPage(initial), "Effort");
  assert.deepEqual(effort.options.map((o) => o.value), ["", "low", "medium", "high", "xhigh", "max"]);
  assert.equal(effort.options.find((o) => o.value === effort.value).label, "Default");
  assert.equal(effort.changed, false);
  const high = row(settingsPage(load(`{ "summaries": { "effort": "high" } }`)), "Effort");
  assert.equal(high.options.find((o) => o.value === high.value).label, "high");
  assert.equal(high.changed, true);
});

test("Write a summary when a turn is opened is a toggle, off by default", () => {
  const open = row(settingsPage(initial), "Write a summary when a turn is opened");
  assert.deepEqual([open.control, open.value, open.changed, open.path], ["toggle", false, false, ["summaries", "on_open"]]);
  assert.equal(row(settingsPage(load(`{ "summaries": { "on_open": true } }`)), "Write a summary when a turn is opened").changed, true);
});

test("a changed default zoom resets the window's zoom, and ⌘+ never writes the file", () => {
  // ⌘+ moves the window's zoom only: a reload that leaves the file's zoom alone keeps it.
  const keys = zoomed(1, 1);
  assert.equal(reloadedZoom(keys, initial.settings, load(`{ "sounds": false }`).settings), keys);
  // The page changing the file's zoom sets the window's.
  assert.equal(reloadedZoom(keys, initial.settings, load(`{ "zoom": 1.5 }`).settings), 1.5);
  assert.equal(reloadedZoom(keys, load(`{ "zoom": 1.5 }`).settings, load(`{ "zoom": 1.5 }`).settings), keys);
});
