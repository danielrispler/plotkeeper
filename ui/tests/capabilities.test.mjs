// Capabilities in the view (danielrispler/plotkeeper#43, decisions 4, 5 and 12), on a `Session` JSON. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
/** @typedef {import("../../dist/types.js").Session} Session */
/** @typedef {import("../../dist/types.js").Subagent} Subagent */
import { reports, modelLine } from "../../dist/capabilities.js";
import { rowMark, secondLine } from "../../dist/list.js";
import { subagentHistory } from "../../dist/story.js";

/** @returns {import("../../dist/types.js").Capabilities} */
const caps = (agent, rest = {}) => ({
  agent, asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, transcript_model: false, context: false, turn_trees: false, ...rest,
});
const claude = /** @type {Session} */ ({ capabilities: caps("claude", { asks: true, replies: true, permission_mode: true, transcript_version: true, transcript_model: true, context: true, turn_trees: true }), model: null, transcript: { context: { tokens: 9000 }, model: "claude-opus-5-5" } });
const agy = /** @type {Session} */ ({ capabilities: caps("agy"), model: "gemini-3.8-flash-high", transcript: { context: null, model: null } });
const codex = /** @type {Session} */ ({ capabilities: caps(""), model: null, status_only: true, transcript: { context: null, model: null } });

// A transcript with only the fields a test sets.
/** @returns {Session["transcript"]} */
const transcript = (fields) => fields;

test("a part of the view shows only when the session's capabilities report it", () => {
  assert.equal(reports(claude, "asks"), true);
  assert.equal(reports(agy, "asks"), false);
  assert.equal(reports(agy, "permission_mode"), false);
  // Trees give a turn its files; agy reports none (danielrispler/plotkeeper#111).
  assert.equal(reports(claude, "turn_trees"), true);
  assert.equal(reports(agy, "turn_trees"), false);
});

test("an agy row shows no text mark (its logo says it), a Claude row its context size, a status-only row its own", () => {
  assert.equal(rowMark({ ...agy, status_only: false }), "");
  assert.equal(rowMark({ ...claude, status_only: false }), "9k");
  assert.equal(rowMark(codex), "status only");
});

test("an agy session's mode and subagent result are hidden, with its status shown instead", () => {
  const agent = /** @type {Subagent} */ ({ id: "c", name: "n", started_at: 1, ended_at: 2, kind: "helper", status: "completed", failed: false, result: "done it" });
  assert.equal(subagentHistory({ agents: [agent], capabilities: agy.capabilities }, new Map())[0].result, "completed");
  assert.equal(subagentHistory({ agents: [agent], capabilities: claude.capabilities }, new Map())[0].result, "done it");
});

test("the header shows each session's model by its agent's mark, none for status only", () => {
  assert.equal(modelLine(claude), "Claude · Opus 5.5");
  assert.equal(modelLine(agy), "agy · Flash high");
  assert.equal(modelLine(codex), "");
  assert.equal(modelLine({ ...agy, model: null }), "");
  assert.equal(modelLine({ ...claude, transcript: transcript({ model: "claude-haiku-5" }) }), "Claude · Haiku 5");
  assert.equal(modelLine({ ...claude, transcript: transcript({ model: "claude-3-5-sonnet-20241022" }) }), "Claude · Sonnet 3.5");
  assert.equal(modelLine({ ...claude, transcript: transcript({ model: "claude-sonnet-4-20250514" }) }), "Claude · Sonnet 4");
  assert.equal(modelLine({ ...claude, transcript: transcript({ model: "claude-opus-5-5[1m]" }) }), "Claude · Opus 5.5");
});

test("a recorded Codex session's model line names Codex and the model its hooks name (#359)", () => {
  const codex = /** @type {Session} */ ({ capabilities: caps("codex", { asks: true, permission_mode: true }), model: "gpt-5.5-codex", transcript: { context: null, model: null } });
  assert.match(modelLine(codex), /^Codex · /);
});

test("a recorded Qwen Code session's model line names Qwen Code and the model its hooks name (#360)", () => {
  const qwen = /** @type {Session} */ ({ capabilities: caps("qwen", { asks: true, permission_mode: true }), model: "qwen3-coder-plus", transcript: { context: null, model: null } });
  assert.equal(modelLine(qwen), "Qwen Code · Qwen3 coder plus");
});

test("the model line takes the transcript's model by the capability flag, never by the agent's name", () => {
  // A Claude session whose capabilities say its transcript names no model shows the model on the session.
  assert.equal(modelLine({ ...claude, capabilities: { ...claude.capabilities, transcript_model: false }, model: "claude-haiku-5" }), "Claude · Haiku 5");
  // An agent the page does not know by name, whose transcript names its model, shows its own id.
  const newagent = /** @type {Session} */ ({ capabilities: caps("newagent", { transcript_model: true }), model: null, transcript: { context: null, model: "nova-pro-2" } });
  assert.equal(modelLine(newagent), "newagent · Nova pro");
  assert.equal(modelLine({ ...newagent, capabilities: caps("newagent") }), "");
  // A model with no word in its name shows whole, never a broken line.
  assert.equal(modelLine({ ...newagent, transcript: transcript({ model: "4" }) }), "newagent · 4");
});

test("an agy session at needs you has no question text, so its row shows its line", () => {
  /** @type {Session} */
  const row = { ...agy, state: "needs_you", line: "Run a command", questions: [] };
  assert.equal(secondLine(row), "Run a command");
});

test("a recorded Cursor session's model line names Cursor and the model its hooks name (#362)", () => {
  const cursor = /** @type {Session} */ ({ capabilities: caps("cursor"), model: "gpt-5.5", transcript: { context: null, model: null } });
  assert.match(modelLine(cursor), /^Cursor · /);
});
