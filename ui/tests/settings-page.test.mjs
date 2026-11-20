// The settings page's rows (#195), run on the `tsc` output: one pure function from the parsed settings files to what each row shows.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El } from "./dom.mjs";
import { initial, reload, reloadedZoom, zoomed } from "../../dist/settings.js";
import { drawSettingsPage, newValue, settingsPage as pageOf, startSettingsPage, stepped, writesOn } from "../../dist/settings-page.js";
import { studio } from "../../dist/state.js";

// The stylesheet's terminal size, which the window reads from `--size-mono`.
const SHEET = 12.5;
const settingsPage = (files) => pageOf(files, SHEET);
const settingsPageAt = (sheet, files) => pageOf(files, sheet);
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
    "Warn at context share",
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
  assert.equal(row(defaults, "Warn at context share").changed, false);
  const changed = settingsPage(load(`{ "sounds": false, "context_warning": 20 }`));
  assert.equal(row(changed, "Sounds").value, false);
  assert.equal(row(changed, "Sounds").changed, true);
  assert.equal(row(changed, "Warn at context share").value, 20);
  assert.equal(row(changed, "Warn at context share").changed, true);
  // Set to the default's own value in the file, it is not changed.
  assert.equal(row(settingsPage(load(`{ "sounds": true }`)), "Sounds").changed, false);
});

test("a row names the key it writes", () => {
  const page = settingsPage(initial);
  assert.deepEqual(row(page, "Sounds").path, ["sounds"]);
  assert.deepEqual(row(page, "Warn at context share").path, ["context_warning"]);
});

test("a file that does not parse disables every setting's control, says so, and shows the settings in effect", () => {
  const last = load(`{ "sounds": false }`);
  const page = settingsPage(load(`{ "sounds": `, last));
  const settings = (page) => rows(page).filter((r) => r.control !== "open");
  assert.equal(page.error, "settings.json has errors: fix it first");
  assert.ok(settings(page).every((r) => r.disabled), "every setting's control is disabled");
  assert.equal(row(page, "Sounds").value, false);
  assert.equal(row(page, "Sounds").changed, true);
  const fine = settingsPage(last);
  assert.equal(fine.error, null);
  assert.ok(settings(fine).every((r) => !r.disabled));
});

test("a key of the wrong type reads as its default and is not a broken file", () => {
  const page = settingsPage(load(`{ "sounds": "yes", "context_warning": "high" }`));
  assert.equal(row(page, "Sounds").value, true);
  assert.equal(row(page, "Sounds").changed, false);
  assert.equal(row(page, "Warn at context share").value, 30);
  assert.equal(page.error, null);
  assert.ok(rows(page).every((r) => r.control === "open" || !r.disabled));
});

test("the context warning steps by 5 from 0, Off, to 100, and its step back to 30 removes the key", () => {
  const warning = row(settingsPage(initial), "Warn at context share");
  assert.equal(warning.control, "stepper");
  assert.deepEqual([warning.value, warning.min, warning.max, warning.step, warning.zero], [30, 0, 100, 5, "Off"]);
  assert.equal(newValue(warning, "up"), 35);
  assert.equal(newValue(row(settingsPage(load(`{ "context_warning": 5 }`)), "Warn at context share"), "down"), 0);
  assert.equal(newValue(row(settingsPage(load(`{ "context_warning": 35 }`)), "Warn at context share"), "down"), null);
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
  // An unset size shows the stylesheet's 12.5: the first step goes to a whole number.
  assert.equal(stepped(size, 1), 13);
  assert.equal(stepped(size, -1), 12);
  assert.equal(stepped({ ...size, value: 13 }, 1), 14);
  assert.equal(stepped({ ...size, value: 13 }, -1), 12);
  assert.equal(stepped({ ...zoom, value: 1.15 }, 1), 1.2);
  assert.equal(stepped({ ...zoom, value: 1.15 }, -1), 1.1);
});

test("a control's change gives the value to write from the row as it is now, and the row's default gives none, so the key is removed", () => {
  const page = settingsPage(load(`{ "zoom": 1.1, "sounds": false, "summaries": { "model": "opus" } }`));
  assert.equal(newValue(row(page, "Default zoom"), "up"), 1.2);
  assert.equal(newValue(row(page, "Default zoom"), "down"), null, "1.0 is the default");
  assert.equal(newValue(row(page, "Sounds"), "toggle"), null, "back on is the default");
  assert.equal(newValue(row(page, "Warn at context share"), "down"), 25);
  assert.equal(newValue(row(page, "Model"), "select", "sonnet"), "sonnet");
  assert.equal(newValue(row(page, "Model"), "select", "haiku"), null, "Haiku is the default");
  assert.equal(newValue(row(page, "Effort"), "select", ""), null, "Default is the default");
  assert.equal(newValue(row(page, "Effort"), "select", "high"), "high");
  assert.equal(newValue(row(page, "Model"), "reset"), null);
  // Two quick clicks on +, each from the row the first one wrote, reach 1.2 and 1.3.
  const first = newValue(row(page, "Default zoom"), "up");
  const next = row(settingsPage(load(`{ "zoom": ${first} }`)), "Default zoom");
  assert.equal(newValue(next, "up"), 1.3);
  // A font size stepped to the stylesheet's own whole size is the default too.
  const size = row(settingsPageAt(12, load(`{ "terminal": { "font_size": 13 } }`)), "Terminal font size");
  assert.equal(newValue(size, "down"), null);
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

test("a dropdown writes on its change, every button on its click, so the undo icon of a dropdown's row resets", () => {
  assert.equal(writesOn("select", "click"), false, "the click that opens the list");
  assert.equal(writesOn("select", "change"), true);
  for (const action of ["reset", "toggle", "up", "down"]) assert.equal(writesOn(action, "click"), true, action);
  // A reset on a changed dropdown row writes null, which removes the key.
  const page = settingsPage(load(`{ "summaries": { "model": "opus" } }`));
  assert.equal(writesOn("reset", "click") && newValue(row(page, "Model"), "reset", "opus"), null);
});

// The dialog (#221), drawn into the stand-in DOM: a control is found by its label, and a click is sent to the region's listener.
class Control extends El {
  dataset = {};
  get disabled() {
    return this.hasAttribute("disabled");
  }
  toggleAttribute(name, on) {
    if (on) this.setAttribute(name, "");
    else this.removeAttribute(name);
  }
  closest() {
    return this.dataset.action ? this : null;
  }
  querySelectorAll(tag) {
    return this.find(tag);
  }
}

// The settings page over `settings.json`'s text `settings`, wired, with the shell's calls kept in `calls`.
function dialogOver(settings) {
  const calls = [];
  const regions = Object.fromEntries(["settings-dialog", "settings-close", "settings-status", "settings-body"].map((id) => [id, new Control("div")]));
  globalThis.document = { createElement: (tag) => new Control(tag), getElementById: (id) => regions[id], documentElement: {} };
  globalThis.getComputedStyle = () => ({ getPropertyValue: () => String(SHEET) });
  globalThis.window = { addEventListener() {}, __TAURI__: { core: { invoke: async (command, args) => void calls.push([command, args]) } } };
  studio.files = load(settings);
  startSettingsPage(() => {});
  drawSettingsPage();
  const labelled = (region, label) => regions[region].find("button").filter((b) => (b.getAttribute("aria-label") ?? b.text) === label);
  const click = (region, control) => regions[region].listeners.click?.({ target: control });
  return { calls, labelled, click };
}

test("with a broken settings.json, the status line's Open settings.json asks to open the file", () => {
  const { calls, labelled, click } = dialogOver(`{ "sounds": `);
  const [open] = labelled("settings-status", "Open settings.json");
  assert.ok(open, "the status line has the button");
  click("settings-status", open);
  assert.deepEqual(calls.map(([command]) => command), ["open_settings"]);
});

test("with a broken settings.json, the two Open rows are enabled and open their files, and the other rows stay disabled", () => {
  const { calls, labelled, click } = dialogOver(`{ "sounds": `);
  for (const label of ["Open settings.json", "Open keymap.json"]) {
    const [open] = labelled("settings-body", label);
    assert.equal(open.disabled, false, label);
    click("settings-body", open);
  }
  assert.deepEqual(calls.map(([command]) => command), ["open_settings", "open_keymap"]);
  const [sounds] = labelled("settings-body", "Sounds");
  assert.equal(sounds.disabled, true);
  click("settings-body", sounds);
  assert.equal(calls.length, 2, "a disabled row sends nothing");
});
