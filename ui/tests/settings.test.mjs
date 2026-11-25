// Settings and keymap (danielrispler/Falinks#524), run on the `tsc` output. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { initial, reload } from "../../dist/settings.js";

test("missing files give the defaults", () => {
  const loaded = reload(initial, { settings: null, keymap: null });
  assert.deepEqual(loaded.settings.summaries, { model: "haiku", effort: "", on_open: false, writer: "claude" });
  assert.equal(loaded.keymap.Workspace["cmd-1"], "studio::ToggleList");
  assert.deepEqual(loaded.errors, []);
});

test("a Zed-style file, with comments and trailing commas, sets its keys and leaves the rest default", () => {
  const settings = `// Studio settings
  {
    "summaries": { "model": "sonnet", /* later */ },
  }`;
  const loaded = reload(initial, { settings, keymap: null });
  assert.deepEqual(loaded.settings.summaries, { model: "sonnet", effort: "", on_open: false, writer: "claude" });
  assert.deepEqual(loaded.errors, []);
});

test("the removed context_warning and notifications keys are ignored (#416)", () => {
  const old = reload(initial, { settings: `{ "context_warning": 50, "notifications": { "while_focused": true } }`, keymap: null });
  assert.deepEqual(old.errors, []);
  assert.equal(old.settings.context_warning, undefined);
});

test("a string holding // or /* is not read as a comment", () => {
  const settings = `{ "summaries": { "model": "a//b/*c" } }`;
  assert.equal(reload(initial, { settings, keymap: null }).settings.summaries.model, "a//b/*c");
});

test("a key of the wrong type keeps its default", () => {
  const settings = `{ "summaries": { "model": 3, "on_open": "yes", "effort": "high" } }`;
  const loaded = reload(initial, { settings, keymap: null });
  assert.equal(loaded.settings.summaries.model, "haiku");
  assert.equal(loaded.settings.summaries.on_open, false);
  assert.deepEqual(loaded.errors, ["settings.json: summaries.model: expected a string", "settings.json: summaries.on_open: expected a boolean"]);
  // The other keys still apply.
  assert.equal(loaded.settings.summaries.effort, "high");
});

test("a broken file keeps the last good settings and reports the error", () => {
  const good = reload(initial, { settings: `{ "summaries": { "model": "sonnet" } }`, keymap: `[{ "bindings": { "cmd-l": "studio::ToggleList" } }]` });
  const broken = reload(good, { settings: `{ "summaries": { "model": "opus" }, `, keymap: `[{ "bindings": ` });
  assert.equal(broken.settings.summaries.model, "sonnet");
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

test("a key that is not a keystroke is reported and the rest of the keymap stays", () => {
  const keymap = `[{ "bindings": { "": "studio::Dismiss", "cmd-l": "studio::ToggleList" } }]`;
  const loaded = reload(initial, { settings: null, keymap });
  assert.deepEqual(loaded.errors, [`keymap.json: "" is not a keystroke`]);
  assert.equal(loaded.keymap.Workspace["cmd-l"], "studio::ToggleList");
  // The skipped binding does not move Dismiss off its default key.
  assert.equal(loaded.keymap.Workspace.escape, "studio::Dismiss");
});

test("a file of the wrong shape is reported", () => {
  const loaded = reload(initial, { settings: `[]`, keymap: `{}` });
  assert.equal(loaded.errors.length, 2);
  assert.equal(loaded.settings.summaries.model, "haiku");
});

test("the check lists are commands per project folder, none by default, and anything else is reported (danielrispler/plotkeeper#112)", () => {
  assert.deepEqual(reload(initial, { settings: null, keymap: null }).settings.checks, {});
  const set = reload(initial, { settings: `{ "checks": { "/work/app": ["make verify", "npm run check"] } }`, keymap: null });
  assert.deepEqual(set.settings.checks, { "/work/app": ["make verify", "npm run check"] });
  const wrong = reload(initial, { settings: `{ "checks": { "/a": "make", "/b": ["ok"] } }`, keymap: null });
  assert.deepEqual(wrong.settings.checks, { "/b": ["ok"] });
  assert.deepEqual(wrong.errors, ["settings.json: checks./a: expected an array of strings"]);
  assert.deepEqual(reload(initial, { settings: `{ "checks": [] }`, keymap: null }).errors.length, 1);
});

test("summaries.writer chooses agy, codex, copilot, cursor, anthropic or openai, defaults to claude, and a name that is none of them is reported", () => {
  const read = (summaries) => reload(initial, { settings: `{ "summaries": ${summaries} }`, keymap: null });
  assert.equal(read(`{ "writer": "agy" }`).settings.summaries.writer, "agy");
  assert.equal(read(`{ "writer": "codex" }`).settings.summaries.writer, "codex");
  assert.equal(read(`{ "writer": "copilot" }`).settings.summaries.writer, "copilot");
  assert.equal(read(`{ "writer": "cursor" }`).settings.summaries.writer, "cursor");
  // The API-key writer, which a passed Test of it saves (#366).
  assert.equal(read(`{ "writer": "anthropic" }`).settings.summaries.writer, "anthropic");
  assert.equal(read(`{ "writer": "openai" }`).settings.summaries.writer, "openai");
  assert.equal(read(`{}`).settings.summaries.writer, "claude");
  const bad = read(`{ "writer": "opencode" }`);
  assert.equal(bad.settings.summaries.writer, "claude");
  assert.equal(bad.errors.length, 1);
});

test("explanations are on by default, read from the file, and a wrong type is reported and keeps them on (#522)", () => {
  assert.equal(reload(initial, { settings: null, keymap: null }).settings.explanations, true);
  assert.equal(reload(initial, { settings: `{ "explanations": false }`, keymap: null }).settings.explanations, false);
  const wrong = reload(initial, { settings: `{ "explanations": "no" }`, keymap: null });
  assert.equal(wrong.settings.explanations, true);
  assert.deepEqual(wrong.errors, ["settings.json: explanations: expected a boolean"]);
});

test("tips are on by default, `\"tips\": false` turns them off, and a wrong type is reported (#523)", () => {
  assert.equal(reload(initial, { settings: null, keymap: null }).settings.tips, true);
  assert.equal(reload(initial, { settings: `{ "tips": false }`, keymap: null }).settings.tips, false);
  const wrong = reload(initial, { settings: `{ "tips": "no" }`, keymap: null });
  assert.equal(wrong.settings.tips, true);
  assert.deepEqual(wrong.errors, ["settings.json: tips: expected a boolean"]);
});
