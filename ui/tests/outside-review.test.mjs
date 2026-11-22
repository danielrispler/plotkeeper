// `node scripts/review/outside-review.mjs`: which diffs get the outside review, how `agy` is called
// and what its result becomes. These tests run no `agy`. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  JOB_QUEUE,
  SKIPPED,
  agyCall,
  brief,
  decide,
  options,
  outcome,
  quota,
} from "../../scripts/review/outside-review.mjs";

// A diff that adds `text` as line `line` of `path`, after the context line `before`.
const adds = (path, line, text, before = "") =>
  [
    `diff --git a/${path} b/${path}`,
    `--- a/${path}`,
    `+++ b/${path}`,
    `@@ -${line - 1},1 +${line - 1},2 @@`,
    ` ${before}`,
    `+${text}`,
    "",
  ].join("\n");

// `Run` derives Deserialize at line 4; its last field is line 17; `beside` is a function after it.
const checks = [
  "use serde::{Deserialize, Serialize};",
  "",
  "/// One run of a check.",
  "#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]",
  "pub struct Run {",
  "    /// Its id.",
  "    pub id: String,",
  "    /// When it started.",
  "    pub started_at: u64,",
  "    /// When it ended.",
  "    pub ended_at: Option<u64>,",
  "    /// Its exit code; a `}` in a comment closes nothing.",
  "    pub code: Option<i32>,",
  "    /// Its command.",
  "    pub command: String,",
  "    /// The statement it ran.",
  "    pub statement: String,",
  "}",
  "",
  "fn beside(run: &Run) -> usize {",
  "    run.id.len()",
  "}",
  "",
  "#[derive(",
  "    Clone,",
  "    serde::Deserialize,",
  ")]",
  "enum Status {",
  "    Running,",
  "    Passed,",
  "}",
  "",
  "#[derive(Clone, Debug)]",
  "struct Plain {",
  "    a: u8,",
  "}",
].join("\n");
const facts = (diff, more = {}) => ({ diff, fileAt: () => checks, subIssues: () => 0, ...more });
const quiet = options([]);

test("a field edited ten or more lines below its type's derive(Deserialize) gets a review, named by its type", () => {
  const decided = decide(quiet, facts(adds("core/src/checks.rs", 17, "    pub statement: String,")));
  assert.equal(decided.review, true);
  assert.match(decided.why, /Run in core\/src\/checks\.rs/);
});

test("a variant added to an enum whose derive spans several lines gets a review", () => {
  const decided = decide(quiet, facts(adds("core/src/checks.rs", 30, "    Passed,")));
  assert.match(decided.why ?? "", /Status in core\/src\/checks\.rs/);
});

test("a change to a function beside such a type, or to a type that derives no Deserialize, gets none", () => {
  for (const line of [21, 35]) {
    const decided = decide(quiet, facts(adds("core/src/checks.rs", line, "    1")));
    assert.deepEqual(decided, { review: false, line: SKIPPED }, `line ${line}`);
  }
});

test("a type that derives Deserialize in a test file gets none: no stored file or wire holds it", () => {
  const decided = decide(quiet, facts(adds("core/tests/subagents.rs", 17, "    pub statement: String,")));
  assert.deepEqual(decided, { review: false, line: SKIPPED });
});

test("a hunk of code naming a stored file gets a review, even when the changed line does not name it", () => {
  const before = '        let summaries = read(&path.with_file_name("summaries.json"))?;';
  const decided = decide(quiet, facts(adds("understanding/src/store.rs", 539, "        let more = 1;", before)));
  assert.match(decided.why ?? "", /summaries\.json.*understanding\/src\/store\.rs/);
});

test("a change to the turn-writing job queue gets a review, and the queue's file is where the rule looks", () => {
  const decided = decide(quiet, facts(adds(JOB_QUEUE, 288, "    let x = 1;"), { fileAt: () => "" }));
  assert.match(decided.why ?? "", /job queue/);
  assert.ok(existsSync(join(import.meta.dirname, "../..", JOB_QUEUE)), `${JOB_QUEUE} moved: update JOB_QUEUE`);
});

const docs = adds("docs/studio/development.md", 12, "The card store writes `summaries.json`.");

test("a docs-only diff is skipped: no risk match, even when it names a stored file", () => {
  assert.equal(SKIPPED, "skipped: no risk match");
  assert.deepEqual(decide(quiet, facts(docs)), { review: false, line: SKIPPED });
});

test("--force reviews any diff, and --since reviews the fix commits of round 2", () => {
  assert.equal(decide(options(["--force"]), facts(docs)).review, true);
  assert.equal(decide(options(["--since", "abc123"]), facts(docs)).review, true);
});

test("a spec's branch gets a review: its issue has sub-issues", () => {
  const decided = decide(quiet, facts(docs, { subIssues: () => 7 }));
  assert.equal(decided.review, true);
  assert.match(decided.why, /spec/);
});

const call = (argv, env = {}) => agyCall(options(argv), { schema: "/w/review/schema.json", prompt: "p", env });
const flag = (args, name) => args[args.indexOf(name) + 1];

test("the review runs gemini-4-neon-medium unless --model names another", () => {
  assert.equal(flag(call([]).args, "--model"), "gemini-4-neon-medium");
  assert.equal(flag(call(["--model", "gemini-4-neon-high"]).args, "--model"), "gemini-4-neon-high");
});

test("the review is cut at 15 minutes and answers in the findings' JSON shape", () => {
  const { args } = call([]);
  assert.equal(flag(args, "--print-timeout"), "15m");
  assert.equal(flag(args, "--output-format"), "json");
  assert.equal(flag(args, "--json-schema"), "/w/review/schema.json");
  assert.equal(flag(args, "-p"), "p");
});

test("the review's environment has no STUDIO_TERMINAL_ID, and keeps the rest", () => {
  const { env } = call([], { STUDIO_TERMINAL_ID: "claude-1", PATH: "/bin" });
  assert.deepEqual(env, { PATH: "/bin" });
});

const finding = { file: "core/src/checks.rs", line: 17, summary: "no default", failure_scenario: "old file" };
const success = JSON.stringify({
  status: "SUCCESS",
  duration_seconds: 120,
  structured_output: { findings: [finding] },
  usage: { total_tokens: 21626 },
});

test("a finished review is `reviewed`, with its findings and tokens", () => {
  const result = outcome({ status: 0, stdout: success, stderr: "", reads: 4 });
  assert.match(result.line, /^reviewed: 1 finding$/);
  assert.deepEqual(result.findings, [finding]);
  assert.equal(result.tokens, 21626);
  assert.equal(result.exit, 0);
});

test("a review that cannot run is `review unavailable`, and the script still exits 0", () => {
  const cases = {
    "the turn was cut by --print-timeout": { status: 3, stdout: "", stderr: "" },
    "the Gemini quota is used up": { status: 1, stdout: "", stderr: "Error: RESOURCE_EXHAUSTED: quota exceeded" },
    "agy is not installed": { status: null, missing: true, stdout: "", stderr: "" },
    "agy printed no JSON": { status: 0, stdout: "not json", stderr: "" },
    "the reviewer could read no file": { status: 0, stdout: success, stderr: "", reads: 0 },
  };
  for (const [name, ran] of Object.entries(cases)) {
    const result = outcome({ reads: 4, ...ran });
    assert.match(result.line, /^review unavailable: \S/, name);
    assert.equal(result.exit, 0, name);
    assert.equal(result.findings, undefined, name);
  }
  assert.match(outcome({ status: 3, stdout: "", stderr: "", reads: 0 }).line, /15 minutes/);
  assert.match(outcome({ status: 1, stdout: "", stderr: "quota exceeded", reads: 0 }).line, /quota/);
});

test("the brief gives absolute paths and no @ mention, which agy may read without a tool call", () => {
  const prompt = brief("/tmp/outside-review-x");
  assert.ok(!prompt.includes("@"));
  for (const path of ["review/diff.patch", "review/changed.txt", "review/tree.txt", "repo"])
    assert.ok(prompt.includes(`/tmp/outside-review-x/${path}`), path);
});

test("the Gemini quota left is read from agy's usage report", () => {
  const bucket = (window, remaining_fraction) => ({ window, remaining_fraction });
  const usage = (groups) => JSON.stringify({ status: "SUCCESS", command: { name: "usage", data: { groups } } });
  const report = usage([
    { name: "Gemini Models", buckets: [bucket("weekly", 0.968), bucket("5h", 0.982)] },
    { name: "Claude and GPT models", buckets: [bucket("weekly", 1), bucket("5h", 1)] },
  ]);
  assert.deepEqual(quota(report), { weekly: 0.968, "5h": 0.982 });
  assert.equal(quota("no report"), null);
});

// The deny hook, as `agy` runs it: the tool call on stdin, the decision on stdout.
const hook = join(import.meta.dirname, "../../scripts/review/deny-hook.mjs");

test("the deny hook allows a read inside its root and the answer's hand-over, and denies writes, commands and reads that leave the root", () => {
  const root = mkdtempSync(join(tmpdir(), "deny-hook-"));
  mkdirSync(join(root, "in"));
  writeFileSync(join(root, "in/notes.txt"), "inside");
  writeFileSync(join(root, "outside.txt"), "outside");
  symlinkSync(join(root, "outside.txt"), join(root, "in/link.txt"));
  const log = join(root, "hook.log");
  const decision = (name, args) => {
    const input = JSON.stringify({ cwd: join(root, "in"), toolCall: { name, args } });
    const ran = spawnSync(process.execPath, [hook, join(root, "in"), log], { input, encoding: "utf8" });
    return JSON.parse(ran.stdout).decision;
  };
  assert.equal(decision("read_file", { Path: join(root, "in/notes.txt") }), "allow");
  assert.equal(decision("read_file", { Path: join(root, "outside.txt") }), "deny");
  assert.equal(decision("read_file", { Path: join(root, "in/link.txt") }), "deny");
  assert.equal(decision("read_file", { Path: "../outside.txt" }), "deny");
  assert.equal(decision("write_file", { Path: join(root, "in/b.txt"), Content: "x" }), "deny");
  assert.equal(decision("run_command", { CommandLine: "cat notes.txt" }), "deny");
  assert.equal(decision("some_new_tool", {}), "deny");
  // `agy --json-schema` hands its answer over through this tool; denied, the findings never arrive.
  assert.equal(decision("finish", { findings: [] }), "allow");
  const logged = readFileSync(log, "utf8");
  assert.equal(logged.trim().split("\n").length, 8);
  assert.ok(!logged.includes("inside"));
});
