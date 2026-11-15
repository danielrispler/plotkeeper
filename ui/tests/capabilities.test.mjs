// Capabilities in the view (danielrispler/plotkeeper#43, decisions 4, 5 and 12), on a `Session` JSON. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { reports, modelLine } from "../../dist/capabilities.js";
import { rowMark, secondLine } from "../../dist/list.js";
import { subagentHistory } from "../../dist/story.js";

const caps = (agent, rest = {}) => ({
  agent, asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, ...rest,
});
const claude = { capabilities: caps("claude", { asks: true, replies: true, permission_mode: true, transcript_version: true }), model: null, transcript: { context: { tokens: 9000, share: 9 }, model: "claude-opus-5-5" } };
const agy = { capabilities: caps("agy"), model: "gemini-3.8-flash-high", transcript: { context: null, model: null } };
const codex = { capabilities: caps(""), model: null, status_only: true, transcript: { context: null, model: null } };

test("a part of the view shows only when the session's capabilities report it", () => {
  assert.equal(reports(claude, "asks"), true);
  assert.equal(reports(agy, "asks"), false);
  assert.equal(reports(agy, "permission_mode"), false);
});

test("an agy row carries the agy mark, a Claude row its context share, a status-only row its own", () => {
  assert.equal(rowMark({ ...agy, status_only: false }), "agy");
  assert.equal(rowMark({ ...claude, status_only: false }), "context 9%");
  assert.equal(rowMark(codex), "status only");
});

test("an agy session's mode and subagent result are hidden, with its status shown instead", () => {
  const agent = { id: "c", name: "n", started_at: 1, ended_at: 2, kind: "helper", status: "completed", failed: false, result: "done it" };
  assert.equal(subagentHistory({ agents: [agent], capabilities: agy.capabilities }, new Map())[0].result, "completed");
  assert.equal(subagentHistory({ agents: [agent], capabilities: claude.capabilities }, new Map())[0].result, "done it");
});

test("the header shows each session's model by its agent's mark, none for status only", () => {
  assert.equal(modelLine(claude), "Claude · Opus 5.5");
  assert.equal(modelLine(agy), "agy · Flash high");
  assert.equal(modelLine(codex), "");
  assert.equal(modelLine({ ...agy, model: null }), "");
  assert.equal(modelLine({ ...claude, transcript: { model: "claude-haiku-5" } }), "Claude · Haiku 5");
  assert.equal(modelLine({ ...claude, transcript: { model: "claude-3-5-sonnet-20241022" } }), "Claude · Sonnet 3.5");
  assert.equal(modelLine({ ...claude, transcript: { model: "claude-sonnet-4-20250514" } }), "Claude · Sonnet 4");
  assert.equal(modelLine({ ...claude, transcript: { model: "claude-opus-5-5[1m]" } }), "Claude · Opus 5.5");
});

test("an agy session at needs you has no question text, so its row shows its line", () => {
  const row = { ...agy, state: "needs_you", line: "Run a command", questions: [] };
  assert.equal(secondLine(row), "Run a command");
});
