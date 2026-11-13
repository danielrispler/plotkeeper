// Focus mode and the needs-you toast (danielrispler/Falinks#573). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { bound } from "../../dist/keys.js";
import { initial } from "../../dist/settings.js";
import { questionLine, toastFor, toastShows } from "../../dist/toast.js";

const ui = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, ui), "utf8");

const session = (id, state = "running", questions = []) => ({
  id, state, questions, name: null, folder_name: id, identity: { name: id }, last_step: null,
});
const asking = (id, text) =>
  session(id, "needs_you", [{ tool_use_id: "t", agent: null, answer: "open", questions: [{ question: text, options: [] }] }]);

test("focus mode has one key, rebindable in keymap.json", () => {
  const key = { key: "Enter", code: "Enter", metaKey: true, shiftKey: true, ctrlKey: false, altKey: false };
  assert.equal(bound(initial.keymap, "Workspace", key), "studio::ToggleFocus");
});

test("the toast shows in focus mode and while the rail is an overlay, never beside the rail", () => {
  assert.equal(toastShows("focus"), true);
  assert.equal(toastShows("overlay"), true);
  assert.equal(toastShows("full"), false);
  assert.equal(toastShows("strip"), false);
});

test("a needs-you in focus mode shows one toast with the session and its question's first line", () => {
  const listed = [session("a"), asking("b", "Which branch?\nIt matters for the merge.")];
  const toast = toastFor(listed, "a", new Set());
  assert.equal(toast.session.id, "b");
  assert.equal(toast.count, 1);
  assert.equal(questionLine(toast.session), "Which branch?");
});

test("several blocks share one toast, with a count", () => {
  const toast = toastFor([asking("b", "One?"), asking("c", "Two?")], "a", new Set());
  assert.equal(toast.session.id, "b");
  assert.equal(toast.count, 2);
});

test("a block in the focused session shows no toast", () => {
  assert.equal(toastFor([asking("a", "Mine?")], "a", new Set()), null);
});

test("a dismissed block stays dismissed; the toast goes once Daniel acts", () => {
  assert.equal(toastFor([asking("b", "One?")], "a", new Set(["b"])), null);
  assert.equal(toastFor([session("b", "your_turn")], "a", new Set()), null);
});

test("the toast never takes keyboard focus", () => {
  // shortcut: a line check, not a DOM; upgrade to a DOM test if the toast grows controls.
  const html = read("index.html").match(/<div id="toast"[^>]*>/)[0];
  assert.match(html, /role="status"/);
  assert.doesNotMatch(html, /tabindex|autofocus/);
  const script = read("toast.ts");
  assert.match(script, /tabIndex = -1/);
  assert.doesNotMatch(script, /\.focus\(\)/);
  // A press on it keeps the terminal's focus.
  assert.match(read("main.ts"), /element\("toast"\)\.addEventListener\("pointerdown", \(event\) => event\.preventDefault\(\)\)/);
});
