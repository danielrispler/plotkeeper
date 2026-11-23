// The writer section (spec #320, ticket #363): the writers found, the choice preset, each Test outcome and the gate, run on the
// `tsc` output. The list itself comes from the login shell's `command -v` (`understanding/tests/contract.rs` fakes its PATH).
import { test } from "node:test";
import assert from "node:assert/strict";
import { briefsLine, choiceOptions, gateNotice, keyLine, keySet, preset, testLine } from "../../dist/writers.js";

const claude = (found) => ({ kind: "claude", name: "Claude", found, models: ["haiku", "sonnet", "opus", "fable"], efforts: ["", "low", "medium", "high"] });
const agy = (found) => ({ kind: "agy", name: "agy", found, models: ["gemini-4-neon-low", "gemini-4-neon-high"], efforts: [] });
const copilot = (found) => ({ kind: "copilot", name: "Copilot", found, models: ["claude-haiku-4.5", "gpt-6-astra"], efforts: [] });
const cursor = (found) => ({ kind: "cursor", name: "Cursor", found, models: ["auto", "gpt-5.6-luna"], efforts: [] });
const codex = (found) => ({ kind: "codex", name: "Codex", found, models: ["gpt-6-luna", "gpt-5.6-luna"], efforts: ["", "low", "medium", "high"] });
const anthropic = (found) => ({ kind: "anthropic", name: "Anthropic API key", found, models: ["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5"], efforts: ["", "low", "medium", "high"] });
const openai = (found) => ({ kind: "openai", name: "OpenAI-compatible API", found, models: ["gpt-6-luna", "gpt-5.6-luna", "gpt-5.6-terra"], efforts: ["", "low", "medium", "high"] });
// `keys`: the API-key writers whose key is set, by default the Anthropic one when it is found; `base_url`: the OpenAI-compatible
// writer's, when one is set.
const list = (writers, summaries_on = false, briefs = true, keys = writers.filter((w) => w.kind === "anthropic" && w.found).map((w) => w.kind), base_url = null) =>
  ({ writers, summaries_on, briefs, keys, base_url });
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

test("Codex is listed and picked as the others are, and a model it does not take is its first", () => {
  const options = choiceOptions(list([claude(false), agy(false), codex(true)]));
  assert.deepEqual(options.map((o) => [o.kind, o.label, o.disabled]), [["claude", "Claude (not found)", true], ["agy", "agy (not found)", true], ["codex", "Codex", false]]);
  assert.deepEqual(preset(list([claude(false), agy(false), codex(true)]), settings()), { writer: "codex", model: "gpt-6-luna", effort: "" });
  assert.deepEqual(preset(list([claude(true), codex(true)], true), settings("codex", "haiku", "low")), { writer: "codex", model: "gpt-6-luna", effort: "low" });
  assert.deepEqual(preset(list([claude(true), codex(true)], true), settings("codex", "gpt-5.6-luna", "high")), { writer: "codex", model: "gpt-5.6-luna", effort: "high" });
});

test("Copilot and Cursor are listed and picked as the others are, and a model either does not take is its first", () => {
  const writers = [claude(false), agy(false), codex(false), copilot(true), cursor(true)];
  assert.deepEqual(choiceOptions(list(writers)).map((o) => [o.kind, o.label, o.disabled]).slice(3), [["copilot", "Copilot", false], ["cursor", "Cursor", false]]);
  assert.deepEqual(preset(list(writers), settings()), { writer: "copilot", model: "claude-haiku-4.5", effort: "" });
  assert.deepEqual(preset(list([claude(false), cursor(true)]), settings()), { writer: "cursor", model: "auto", effort: "" });
  // Neither takes an effort, so a saved one is dropped.
  assert.deepEqual(preset(list(writers, true), settings("cursor", "haiku", "low")), { writer: "cursor", model: "auto", effort: "" });
  assert.deepEqual(preset(list(writers, true), settings("copilot", "gpt-6-astra", "")), { writer: "copilot", model: "gpt-6-astra", effort: "" });
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

test("the OpenAI-compatible API: with no base URL and no key it is listed but cannot be picked", () => {
  const unset = list([anthropic(false), openai(false), claude(true)]);
  assert.deepEqual(choiceOptions(unset)[1], { kind: "openai", label: "OpenAI-compatible API (not set)", disabled: true });
  assert.equal(keySet(unset, "openai"), false);
  assert.match(keyLine(unset, "openai"), /not set/);
});

test("a local endpoint with no key is set by its base URL alone, comes before Claude in the preset, and the row names it", () => {
  const ollama = list([anthropic(false), openai(true), claude(true)], false, true, [], "http://localhost:11434/v1");
  assert.equal(keySet(ollama, "openai"), false);
  assert.match(keyLine(ollama, "openai"), /http:\/\/localhost:11434\/v1.*no key/);
  assert.deepEqual(preset(ollama, settings()), { writer: "openai", model: "gpt-6-luna", effort: "" });
  // With a key and no base URL, it is OpenAI's own.
  const keyed = list([anthropic(false), openai(true), claude(true)], false, true, ["openai"]);
  assert.match(keyLine(keyed, "openai"), /api\.openai\.com.*key: set, kept in the macOS Keychain/);
  // The Anthropic row reads only its own key.
  assert.equal(keySet(keyed, "anthropic"), false);
});

test("the OpenAI-compatible writer takes any model id the endpoint serves, but another writer's alias is its first", () => {
  const on = list([openai(true), claude(true)], true, true, [], "http://localhost:11434/v1");
  assert.deepEqual(preset(on, settings("openai", "llama3.2", "")), { writer: "openai", model: "llama3.2", effort: "" });
  assert.deepEqual(preset(on, settings("openai", "openai/gpt-6-luna", "low")), { writer: "openai", model: "openai/gpt-6-luna", effort: "low" });
  assert.deepEqual(preset(on, settings("openai", "haiku", "")), { writer: "openai", model: "gpt-6-luna", effort: "" });
});
