// Settings and keymap (#524), run on the `tsc` output. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { initial, reload, zoomed } from "../../dist/settings.js";

test("zoom steps by a tenth and stays between half and double", () => {
  assert.equal(zoomed(1, 1), 1.1);
  assert.equal(zoomed(1, -1), 0.9);
  assert.equal(zoomed(2, 1), 2);
  assert.equal(zoomed(0.5, -1), 0.5);
  assert.equal(zoomed(1.7, 0), 1);
  assert.equal(reload(initial, { settings: `{ "zoom": 9 }`, keymap: null }).settings.zoom, 2);
  assert.equal(reload(initial, { settings: `{ "zoom": 0.1 }`, keymap: null }).settings.zoom, 0.5);
});

test("missing files give the defaults", () => {
  const loaded = reload(initial, { settings: null, keymap: null });
  assert.equal(loaded.settings.zoom, 1);
  assert.equal(loaded.settings.terminal.font_size, null);
  assert.deepEqual(loaded.settings.summaries, { model: "haiku", effort: "low", on_open: false });
  assert.deepEqual(loaded.settings.doctor, [
    { label: "Check the installation", command: "claude doctor", kind: "shell" },
    { label: "Full checkup", command: "/doctor", kind: "session" },
  ]);
  assert.equal(loaded.keymap.Workspace["cmd-1"], "studio::ToggleList");
  assert.deepEqual(loaded.errors, []);
});

test("a Zed-style file, with comments and trailing commas, sets its keys and leaves the rest default", () => {
  const settings = `// Studio settings
  {
    "zoom": 1.5, /* bigger */
    "terminal": { "font_size": 15, },
    "summaries": { "model": "sonnet" },
    "doctor": [{ "label": "Audit", "command": "/audit", "kind": "session" }],
  }`;
  const loaded = reload(initial, { settings, keymap: null });
  assert.equal(loaded.settings.zoom, 1.5);
  assert.equal(loaded.settings.terminal.font_size, 15);
  assert.deepEqual(loaded.settings.summaries, { model: "sonnet", effort: "low", on_open: false });
  assert.deepEqual(loaded.settings.doctor, [{ label: "Audit", command: "/audit", kind: "session" }]);
  assert.deepEqual(loaded.errors, []);
});

test("a string holding // or /* is not read as a comment", () => {
  const settings = `{ "summaries": { "model": "a//b/*c" } }`;
  assert.equal(reload(initial, { settings, keymap: null }).settings.summaries.model, "a//b/*c");
});

test("a key of the wrong type keeps its default", () => {
  const settings = `{ "zoom": "big", "terminal": { "font_size": "large" }, "doctor": [{ "label": "x", "kind": "shell" }] }`;
  const loaded = reload(initial, { settings, keymap: null });
  assert.equal(loaded.settings.zoom, 1);
  assert.equal(loaded.settings.terminal.font_size, null);
  assert.equal(loaded.settings.doctor.length, 0);
  assert.deepEqual(loaded.errors, [
    "settings.json: terminal.font_size: expected a number",
    "settings.json: zoom: expected a number",
    "settings.json: doctor[0]: expected a label, a command and a kind of shell or session",
  ]);
  // The other keys still apply.
  assert.equal(reload(initial, { settings: `{ "zoom": 1.5, "summaries": { "on_open": "yes" } }`, keymap: null }).settings.zoom, 1.5);
});

test("the terminal's font size stays between 8 and 40", () => {
  const size = (n) => reload(initial, { settings: `{ "terminal": { "font_size": ${n} } }`, keymap: null }).settings.terminal.font_size;
  assert.equal(size(2), 8);
  assert.equal(size(100), 40);
  assert.equal(size(15), 15);
});

test("a broken file keeps the last good settings and reports the error", () => {
  const good = reload(initial, { settings: `{ "zoom": 1.5 }`, keymap: `[{ "bindings": { "cmd-l": "studio::ToggleList" } }]` });
  const broken = reload(good, { settings: `{ "zoom": 2, `, keymap: `[{ "bindings": ` });
  assert.equal(broken.settings.zoom, 1.5);
  assert.equal(broken.keymap.Workspace["cmd-l"], "studio::ToggleList");
  assert.equal(broken.errors.length, 2);
  assert.match(broken.errors[0], /^settings\.json: /);
  assert.match(broken.errors[1], /^keymap\.json: /);
  // Fixed again: the error goes.
  assert.deepEqual(reload(broken, { settings: `{}`, keymap: `[]` }).errors, []);
});

test("a later entry's rebind keeps the keys an earlier entry bound", () => {
  const keymap = `[
    { "bindings": { "cmd-1": "studio::ToggleRail" } },
    { "bindings": { "cmd-l": "studio::ToggleList" } },
  ]`;
  const { Workspace } = reload(initial, { settings: null, keymap }).keymap;
  assert.equal(Workspace["cmd-1"], "studio::ToggleRail");
  assert.equal(Workspace["cmd-l"], "studio::ToggleList");
  assert.equal(Workspace["cmd-2"], undefined);
});

test("a file of the wrong shape is reported", () => {
  const loaded = reload(initial, { settings: `[]`, keymap: `{}` });
  assert.equal(loaded.errors.length, 2);
  assert.equal(loaded.settings.zoom, 1);
});
