// The settings page's rows (#195), run on the `tsc` output: one pure function from the parsed settings files to what each row shows.
import { test } from "node:test";
import assert from "node:assert/strict";
import { initial, reload } from "../../dist/settings.js";
import { settingsPage } from "../../dist/settings-page.js";

const load = (settings, previous = initial) => reload(previous, { settings, keymap: null });
const rows = (page) => page.sections.flatMap((section) => section.rows);
const row = (page, title) => rows(page).find((r) => r.title === title);

test("the page has the Alerts and Files headings, with their rows", () => {
  const page = settingsPage(initial);
  assert.deepEqual(page.sections.map((s) => s.heading), ["Alerts", "Files"]);
  assert.deepEqual(rows(page).map((r) => r.title), ["Sounds", 'Notify "your turn" while focused', "Open settings.json", "Open keymap.json"]);
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
