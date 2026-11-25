// `node scripts/agents/delegate.mjs`, run as the orchestrator runs it, with a stand-in `agy` first on the PATH.
// These tests run no real `agy`. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const script = join(import.meta.dirname, "../../scripts/agents/delegate.mjs");

// The stand-in `agy`. `/usage` reports FAKE_AGY_5H and FAKE_AGY_WEEKLY as the Gemini windows left, or fails
// with no output when FAKE_AGY_USAGE is "none". A critic run answers as FAKE_AGY_RUN says: "ok", "error"
// (status ERROR), "fail" (exit 1) or "timeout" (exit 3, how `--print-timeout` ends). Every call is logged.
const fakeAgy = `#!/usr/bin/env node
const { appendFileSync } = require("node:fs");
const { join } = require("node:path");
const env = process.env;
const call = { argv: process.argv.slice(2), cwd: process.cwd(), internal: env.PLOTKEEPER_INTERNAL, terminal: env.PLOTKEEPER_TERMINAL_ID };
appendFileSync(join(env.FAKE_AGY_DIR, "agy-calls.jsonl"), JSON.stringify(call) + "\\n");
if (process.argv.includes("/usage")) {
  if (env.FAKE_AGY_USAGE === "none") process.exit(1);
  const bucket = (window, remaining_fraction) => ({ window, remaining_fraction });
  const buckets = [bucket("weekly", Number(env.FAKE_AGY_WEEKLY)), bucket("5h", Number(env.FAKE_AGY_5H))];
  console.log(JSON.stringify({ command: { data: { groups: [{ name: "Gemini Models", buckets }] } } }));
  process.exit(0);
}
const run = env.FAKE_AGY_RUN;
if (run === "fail") { console.error("something broke"); process.exit(1); }
if (run === "timeout") process.exit(3);
if (run === "error") { console.log(JSON.stringify({ status: "ERROR", error: "no" })); process.exit(0); }
console.log(JSON.stringify({ status: "SUCCESS", response: "1. high: the spec cites a line that is not there." }));
`;

const routing = {
  strong: [{ cli: "claude", model: "opus" }],
  medium: [{ cli: "claude", model: "sonnet" }],
  cheap: [],
  critic: [
    { cli: "agy", model: "gemini-test-model" },
    { cli: "claude", model: "sonnet", effort: "high" },
  ],
};

/** Runs `pick critic` on a draft spec with the fake `agy` set by `fake`; returns its output and agy's calls. */
const pickCritic = (fake) => {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), "delegate-test-")));
  const repo = join(dir, "repo");
  mkdirSync(join(dir, "bin"));
  mkdirSync(repo);
  writeFileSync(join(dir, "bin/agy"), fakeAgy);
  chmodSync(join(dir, "bin/agy"), 0o755);
  writeFileSync(join(dir, "routing.json"), JSON.stringify(routing));
  writeFileSync(join(dir, "draft.md"), "# A draft\n\nThe store reads `src/store.rs:7`.\n");
  const env = { ...process.env, PATH: `${join(dir, "bin")}:${process.env.PATH}`, FAKE_AGY_DIR: dir, PLOTKEEPER_TERMINAL_ID: "claude-test", ...fake };
  const args = [script, "pick", "critic", "--spec", join(dir, "draft.md"), "--routing", join(dir, "routing.json")];
  const ran = spawnSync(process.execPath, args, { cwd: repo, env, encoding: "utf8" });
  const log = join(dir, "agy-calls.jsonl");
  const calls = existsSync(log) ? readFileSync(log, "utf8").trim().split("\n").map((line) => JSON.parse(line)) : [];
  return { ...ran, calls, repo };
};

test("with both Gemini windows at their floors, pick critic runs the Gemini critic and prints its answer", () => {
  const ran = pickCritic({ FAKE_AGY_5H: "0.5", FAKE_AGY_WEEKLY: "0.3", FAKE_AGY_RUN: "ok" });
  assert.equal(ran.status, 0, ran.stderr);
  assert.equal(ran.stdout, "critic: agy gemini-test-model\n\n1. high: the spec cites a line that is not there.\n");
  const critic = ran.calls.find((call) => !call.argv.includes("/usage"));
  assert.ok(critic, "agy -p was not called for the critic");
  const argv = critic.argv;
  const after = (flag) => argv[argv.indexOf(flag) + 1];
  assert.equal(after("--mode"), "plan");
  assert.equal(after("--model"), "gemini-test-model");
  assert.equal(after("--print-timeout"), "15m");
  assert.equal(after("--output-format"), "json");
  assert.match(after("-p"), /check every claim/i);
  assert.match(after("-p"), /The store reads `src\/store\.rs:7`\./);
  assert.equal(critic.cwd, ran.repo);
  assert.equal(critic.internal, "1");
  assert.equal(critic.terminal, undefined);
});

const ranCritic = (calls) => calls.some((call) => !call.argv.includes("/usage"));

test("with the 5-hour window below its floor, pick critic names Sonnet at high without running Gemini", () => {
  const ran = pickCritic({ FAKE_AGY_5H: "0.49", FAKE_AGY_WEEKLY: "0.9", FAKE_AGY_RUN: "ok" });
  assert.equal(ran.status, 0, ran.stderr);
  assert.equal(ran.stdout, "critic: claude sonnet effort high\nwhy: agy gemini-test-model has 49% of its 5h window and 90% of its weekly window left, under its floors of 50% and 30%\n");
  assert.equal(ranCritic(ran.calls), false);
});

test("with the weekly window below its floor, pick critic names Sonnet at high without running Gemini", () => {
  const ran = pickCritic({ FAKE_AGY_5H: "0.9", FAKE_AGY_WEEKLY: "0.29", FAKE_AGY_RUN: "ok" });
  assert.equal(ran.status, 0, ran.stderr);
  assert.equal(ran.stdout, "critic: claude sonnet effort high\nwhy: agy gemini-test-model has 90% of its 5h window and 29% of its weekly window left, under its floors of 50% and 30%\n");
  assert.equal(ranCritic(ran.calls), false);
});

test("with no quota reading, pick critic names Sonnet at high without running Gemini", () => {
  const ran = pickCritic({ FAKE_AGY_USAGE: "none", FAKE_AGY_RUN: "ok" });
  assert.equal(ran.status, 0, ran.stderr);
  assert.equal(ran.stdout, "critic: claude sonnet effort high\nwhy: agy gemini-test-model gave no quota reading\n");
  assert.equal(ran.calls.length, 1);
  assert.ok(ran.calls[0].argv.includes("/usage"));
});

for (const [run, said] of [
  ["fail", "agy gemini-test-model failed: exit 1: something broke"],
  ["error", "agy gemini-test-model failed: status ERROR"],
  ["timeout", "agy gemini-test-model was cut at 15 minutes"],
]) {
  test(`a Gemini critic run that ends with "${run}" names Sonnet at high as the fallback`, () => {
    const ran = pickCritic({ FAKE_AGY_5H: "0.9", FAKE_AGY_WEEKLY: "0.9", FAKE_AGY_RUN: run });
    assert.equal(ran.status, 0, ran.stderr);
    assert.equal(ran.stdout, `critic: claude sonnet effort high\nwhy: ${said}; Sonnet is the fallback\n`);
    assert.equal(ranCritic(ran.calls), true);
  });
}
