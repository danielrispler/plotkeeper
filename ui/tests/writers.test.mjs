// The writer section (spec #320, ticket #363): the writers found, the choice preset, each Test outcome and the gate, run on the
// `tsc` output. The list itself comes from the login shell's `command -v` (`understanding/tests/contract.rs` fakes its PATH).
import { test } from "node:test";
import assert from "node:assert/strict";
import { briefsLine, choiceOptions, gateNotice, keyLine, keySet, preset, testLine } from "../../dist/writers.js";

const claude = (found) => ({ kind: "claude", name: "Claude", found, models: ["haiku", "sonnet", "opus", "fable"], efforts: ["", "low", "medium", "high"] });
const agy = (found) => ({ kind: "agy", name: "agy", found, models: ["gemini-4-neon-low", "gemini-4-neon-high"], efforts: [] });
const anthropic = (found) => ({ kind: "anthropic", name: "Anthropic API key", found, models: ["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5"], efforts: ["", "low", "medium", "high"] });
const list = (writers, summaries_on = false, briefs = true) => ({ writers, summaries_on, briefs });
const settings = (writer = "claude", model = "haiku", effort = "") => ({ model, effort, on_open: false, writer });

test("the list shows every writer, and one that was not found cannot be picked", () => {
  const options = choiceOptions(list([claude(false), agy(true)]));
  assert.deepEqual(options.map((o) => [o.kind, o.label, o.disabled]), [["claude", "Claude (not found)", true], ["agy", "agy", false]]);
});

test("unconfigured, the choice is preset to the first writer found, in order, at its first model", () => {
  assert.deepEqual(preset(list([claude(true), agy(true)]), settings()), { writer: "claude", model: "haiku", effort: "" });
  assert.deepEqual(preset(list([claude(false), agy(true)]), settings()), { writer: "agy", model: "gemini-4-neon-low", effort: "" });
  // None found: still the first, so Test can say why.
  assert.deepEqual(preset(list([claude(false), agy(false)]), settings()).writer, "claude");
});

test("with summaries on, the choice is the writer that writes now, with its model and effort", () => {
  assert.deepEqual(preset(list([claude(true), agy(true)], true), settings("agy", "gemini-4-neon-high")), { writer: "agy", model: "gemini-4-neon-high", effort: "" });
  assert.deepEqual(preset(list([claude(true), agy(true)], true), settings("claude", "claude-opus-5-5", "low")), { writer: "claude", model: "claude-opus-5-5", effort: "low" });
  // A model agy does not take is its first one, as the writer reads it.
  assert.deepEqual(preset(list([claude(true), agy(true)], true), settings("agy", "haiku", "low")), { writer: "agy", model: "gemini-4-neon-low", effort: "" });
});

test("each Test outcome is its line, passed or failed", () => {
  assert.deepEqual(testLine({ outcome: "passed", line: "Passed: `agy` answered. Summaries are written with it.", settings: "{}" }), { passed: true, text: "Passed: `agy` answered. Summaries are written with it." });
  for (const outcome of ["not_logged_in", "quota", "timeout", "not_installed", "empty", "other"]) {
    const line = testLine({ outcome, line: "`claude` is not logged in. Run `claude` in a terminal and log in with /login.", settings: null });
    assert.equal(line.passed, false, outcome);
    assert.match(line.text, /^Failed: /, outcome);
  }
});

test("the gate: summaries off until a Test passes says so in the writer's notice line", () => {
  assert.match(gateNotice(list([claude(true)], false)), /^Summaries are off: .*Test/);
  assert.equal(gateNotice(list([claude(true)], true)), null);
});

test("without claude, briefs need Claude", () => {
  assert.match(briefsLine(list([claude(false), agy(true)], true, false)), /briefs need Claude/);
  assert.equal(briefsLine(list([claude(true)], true, true)), null);
});

test("the API key: not set, it is listed but cannot be picked, and the key row says so", () => {
  const unset = list([anthropic(false), claude(true), agy(true)]);
  assert.deepEqual(choiceOptions(unset)[0], { kind: "anthropic", label: "Anthropic API key (not set)", disabled: true });
  assert.equal(keySet(unset), false);
  assert.match(keyLine(unset), /not set/);
  assert.equal(preset(unset, settings()).writer, "claude");
});

test("the API key set comes first in the preset order, and the key row says it is set without showing it", () => {
  const set = list([anthropic(true), claude(true), agy(true)]);
  assert.equal(keySet(set), true);
  assert.match(keyLine(set), /: set, kept in the macOS Keychain/);
  assert.deepEqual(preset(set, settings()), { writer: "anthropic", model: "claude-haiku-5-5", effort: "" });
  // A Test of the Anthropic writer reads as any writer's.
  assert.deepEqual(testLine({ outcome: "not_logged_in", line: "`anthropic` has no working API key. Add or replace the API key under Writer, at the top of the page.", settings: null }).passed, false);
});
