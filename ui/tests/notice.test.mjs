// Errors in the notice line (danielrispler/plotkeeper#191): one line per distinct message, a dismiss, and the settings errors beside them. Run: `npm test`.
import { test, beforeEach } from "node:test";
import assert from "node:assert/strict";

const line = { textContent: "", hidden: true };
let present = true;
globalThis.document = { getElementById: () => (present ? line : null) };
const logged = [];
globalThis.window = { __TAURI__: { log: { warn: async (m) => logged.push(["warn", m]), error: async (m) => logged.push(["error", m]) } } };
const { onLogRecord, dismissNotice, setSettingsErrors, showNotice } = await import("../../dist/notice.js");
const { failure, logLine } = await import("../../dist/dom.js");
const ERROR = 5;
const WARN = 4;
const lines = () => line.textContent.split("\n").filter(Boolean);

beforeEach(() => {
  dismissNotice();
  line.textContent = "";
  line.hidden = true;
});

test("an error record from the app shows in the notice line, and a warn record does not", () => {
  onLogRecord({ level: WARN, message: "prune skipped" });
  assert.equal(line.hidden, true);
  onLogRecord({ level: ERROR, message: "cards did not save" });
  assert.deepEqual(lines(), ["cards did not save"]);
  assert.equal(line.hidden, false);
});

test("an error-level failure from the UI helper shows, and a warn-level one does not", () => {
  failure("sounds")(new Error("no sound"));
  assert.equal(line.hidden, true);
  failure("delete_session", "error")(new Error("gone"));
  assert.deepEqual(lines(), ["delete_session: gone"]);
});

test("the same message shows once, also when the app echoes the UI's own line back", () => {
  onLogRecord({ level: ERROR, message: "cards did not save" });
  onLogRecord({ level: ERROR, message: "cards did not save" });
  logLine("error", "delete_session: gone");
  onLogRecord({ level: ERROR, message: "delete_session: gone" });
  assert.deepEqual(lines(), ["cards did not save", "delete_session: gone"]);
});

test("dismissing hides the line and clears its errors, and a later error shows it again", () => {
  onLogRecord({ level: ERROR, message: "cards did not save" });
  dismissNotice();
  assert.equal(line.hidden, true);
  assert.deepEqual(lines(), []);
  onLogRecord({ level: ERROR, message: "cards did not save" });
  assert.equal(line.hidden, false);
  assert.deepEqual(lines(), ["cards did not save"]);
});

test("settings errors share the line with errors, and clearing them keeps the errors", () => {
  line.textContent = "Claude Code 3 changed.";
  line.hidden = false;
  setSettingsErrors(["settings.json: bad"]);
  onLogRecord({ level: ERROR, message: "cards did not save" });
  assert.deepEqual(lines(), ["Claude Code 3 changed.", "settings.json: bad", "cards did not save"]);
  setSettingsErrors([]);
  assert.deepEqual(lines(), ["Claude Code 3 changed.", "cards did not save"]);
  assert.equal(line.hidden, false);
});

test("clearing the settings errors hides a line that holds nothing else", () => {
  setSettingsErrors(["keymap.json: bad"]);
  assert.equal(line.hidden, false);
  setSettingsErrors([]);
  assert.equal(line.hidden, true);
});

test("dismissing removes the errors and keeps the version and writer notices beside them", () => {
  showNotice("Claude Code 3 changed.");
  line.textContent += "\nCards and briefs are off: no writer";
  onLogRecord({ level: ERROR, message: "cards did not save" });
  dismissNotice();
  assert.deepEqual(lines(), ["Claude Code 3 changed.", "Cards and briefs are off: no writer"]);
  assert.equal(line.hidden, false);
});

test("the version notice keeps the errors and settings lines shown beside it", () => {
  setSettingsErrors(["settings.json: bad"]);
  onLogRecord({ level: ERROR, message: "cards did not save" });
  showNotice("Claude Code 3 changed.");
  assert.deepEqual(lines(), ["settings.json: bad", "cards did not save", "Claude Code 3 changed."]);
  assert.equal(line.hidden, false);
});

test("an error with no notice element in the page is still sent to the log, and nothing throws", () => {
  present = false;
  try {
    logged.length = 0;
    logLine("error", "delete_session: gone");
    assert.deepEqual(logged, [["error", "delete_session: gone"]]);
  } finally {
    present = true;
  }
});
