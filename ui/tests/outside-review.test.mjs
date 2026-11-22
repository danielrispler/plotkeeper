// `node scripts/review/outside-review.mjs`: which diffs get the outside review, how `agy` is called
// and what its result becomes. These tests run no `agy`. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawn, spawnSync } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
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

test("a field removed from such a type gets a review", () => {
  const path = "core/src/checks.rs";
  const removed = [`diff --git a/${path} b/${path}`, `--- a/${path}`, `+++ b/${path}`, "@@ -16,3 +16,2 @@"];
  const diff = [...removed, "     /// The statement it ran.", "-    pub old: String,", "     pub statement: String,", ""].join("\n");
  assert.match(decide(quiet, facts(diff)).why ?? "", /Run in core\/src\/checks\.rs/);
});

test("a type is found past a blank line under its derive, and a tuple struct past a semicolon in a field", () => {
  const source = [
    "#[derive(Deserialize)]",
    "",
    "#[serde(rename_all = \"snake_case\")]",
    "pub struct Spaced {",
    "    pub a: u8,",
    "}",
    "",
    "#[derive(Deserialize)]",
    "pub struct Pair(",
    "    [u8; 4],",
    "    u32,",
    ");",
  ].join("\n");
  const at = (line) => decide(quiet, facts(adds("core/src/pair.rs", line, "    x"), { fileAt: () => source })).why ?? "";
  assert.match(at(5), /Spaced/);
  assert.match(at(11), /Pair/);
});

test("a file whose path holds a space is read like any other", () => {
  const diff = adds("core/src/my checks.rs", 17, "    pub statement: String,").replace(/^(\+\+\+ .*)$/m, "$1\t");
  assert.match(decide(quiet, facts(diff)).why ?? "", /Run in core\/src\/my checks\.rs/);
});

test("a type that derives Deserialize in a test file gets none: no stored file or wire holds it", () => {
  const decided = decide(quiet, facts(adds("core/tests/subagents.rs", 17, "    pub statement: String,")));
  assert.deepEqual(decided, { review: false, line: SKIPPED });
});

test("a hunk of code naming a stored file gets a review, even when the changed line does not name it", () => {
  const before = '        let summaries = read(&path.with_file_name("summaries.json"))?;';
  const decided = decide(quiet, facts(adds("understanding/src/store.rs", 539, "        let more = 1;", before)));
  assert.match(decided.why ?? "", /summaries\.json.*understanding\/src\/store\.rs/);
  const stored = ["cards-v2", "briefs", "live-facts", "decisions", "summaries", "agent-summaries", "retests"];
  for (const name of stored) {
    const named = decide(quiet, facts(adds("ui/story.ts", 3, `const file = "${name}.json";`)));
    assert.match(named.why ?? "", new RegExp(`code naming ${name}\\.json changed: ui/story\\.ts`), name);
  }
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
  const line = (ran) => outcome({ stdout: "", stderr: "", reads: 0, ...ran }).line;
  assert.equal(line({ status: 3 }), "review unavailable: the review was cut at 15 minutes");
  assert.equal(line({ status: 1, stderr: "quota exceeded" }), "review unavailable: the Gemini quota is used up");
  assert.equal(line({ status: 1, stderr: "429: rate limit" }), "review unavailable: the Gemini quota is used up");
});

test("a finished review stays `reviewed` when agy also warns about the quota", () => {
  const result = outcome({ status: 0, stdout: success, stderr: "warn: quota 80% used", reads: 4 });
  assert.equal(result.line, "reviewed: 1 finding");
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
  mkdirSync(join(root, "in-too"));
  writeFileSync(join(root, "in-too/notes.txt"), "beside");
  assert.equal(decision("read_file", { Path: join(root, "in-too/notes.txt") }), "deny");
  assert.equal(decision("write_file", { Path: join(root, "in/b.txt"), Content: "x" }), "deny");
  assert.equal(decision("run_command", { CommandLine: "cat notes.txt" }), "deny");
  assert.equal(decision("some_new_tool", {}), "deny");
  // `agy --json-schema` hands its answer over through this tool; denied, the findings never arrive.
  assert.equal(decision("finish", { findings: [] }), "allow");
  const logged = readFileSync(log, "utf8");
  assert.equal(logged.trim().split("\n").length, 9);
  assert.ok(!logged.includes("inside"));
});

test("the script runs when started through a symlinked path, and a wrong option fails with the usage", () => {
  const link = join(mkdtempSync(join(tmpdir(), "outside-review-link-")), "review");
  symlinkSync(join(import.meta.dirname, "../../scripts/review"), link);
  const ran = spawnSync(process.execPath, [join(link, "outside-review.mjs"), "--modle", "x"], { encoding: "utf8" });
  assert.equal(ran.status, 2);
  assert.match(ran.stderr, /unknown or incomplete option --modle\nusage: outside-review/);
});

// The script itself, in a scratch repository with a stand-in `agy` on the PATH: it reads one file
// through the hook the script wrote, then answers as FAKE_AGY says.
const script = join(import.meta.dirname, "../../scripts/review/outside-review.mjs");
const fakeAgy = `#!/usr/bin/env node
const { execSync } = require("node:child_process");
const { readFileSync, renameSync, writeFileSync } = require("node:fs");
const { join } = require("node:path");
if (process.argv.includes("/usage")) process.exit(0);
const dir = process.env.FAKE_AGY_DIR;
writeFileSync(join(dir, "agy-env.json"), JSON.stringify({ cwd: process.cwd(), keys: Object.keys(process.env) }));
const hooks = JSON.parse(readFileSync(".agents/hooks.json", "utf8"));
const command = hooks.readonly.PreToolUse[0].hooks[0].command;
const call = (name, args) => execSync(command, { input: JSON.stringify({ cwd: process.cwd(), toolCall: { name, args } }) });
call("read_file", { Path: join(process.cwd(), "review/diff.patch") });
if (process.env.FAKE_AGY === "exit3") process.exit(3);
if (process.env.FAKE_AGY === "sleep") {
  // Renamed into place, so the test never reads a half-written file.
  writeFileSync(join(dir, "agy.pid.new"), String(process.pid));
  renameSync(join(dir, "agy.pid.new"), join(dir, "agy.pid"));
  setTimeout(() => {}, 60000);
} else {
  const finding = { file: "src/store.rs", line: 7, summary: "no default", failure_scenario: "an old file" };
  const usage = { total_tokens: 10 };
  console.log(JSON.stringify({ status: "SUCCESS", duration_seconds: 1, structured_output: { findings: [finding] }, usage }));
}
`;

const scratch = () => {
  const dir = mkdtempSync(join(tmpdir(), "outside-review-test-"));
  const repo = join(dir, "work");
  const env = {
    ...process.env,
    PATH: `${join(dir, "bin")}:${process.env.PATH}`,
    TMPDIR: join(dir, "tmp"),
    GIT_CONFIG_GLOBAL: "/dev/null",
    GIT_CONFIG_NOSYSTEM: "1",
    STUDIO_TERMINAL_ID: "claude-test",
    FAKE_AGY_DIR: dir,
  };
  for (const folder of ["bin", "tmp", "work/src"]) mkdirSync(join(dir, folder), { recursive: true });
  writeFileSync(join(dir, "bin/agy"), fakeAgy);
  chmodSync(join(dir, "bin/agy"), 0o755);
  const git = (...args) => execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@example.com", ...args], { cwd: repo, env, encoding: "utf8" });
  const note = (more) => `#[derive(Deserialize)]\npub struct Note {\n    pub id: String,\n${more}}\n`;
  git("init", "--quiet", "--initial-branch=main");
  writeFileSync(join(repo, "src/store.rs"), note(""));
  git("add", "-A");
  git("commit", "--quiet", "-m", "base");
  git("clone", "--quiet", "--bare", ".", join(dir, "origin.git"));
  git("remote", "add", "origin", join(dir, "origin.git"));
  git("fetch", "--quiet", "origin");
  git("checkout", "--quiet", "-b", "change");
  writeFileSync(join(repo, "src/store.rs"), note("    pub author: String,\n"));
  git("commit", "--quiet", "-am", "author");
  const left = () => ({ worktrees: git("worktree", "list").trim().split("\n").length, folders: readdirSync(join(dir, "tmp")), status: git("status", "--porcelain") });
  const args = (more = []) => [script, "--out", join(dir, "out"), ...more];
  return { dir, repo, env, git, left, args };
};
const nothingLeft = { worktrees: 1, folders: [], status: "" };

test("a risky diff is reviewed in a throwaway worktree that is gone afterwards, hidden from Studio", () => {
  const s = scratch();
  const ran = spawnSync(process.execPath, s.args(), { cwd: s.repo, env: { ...s.env, FAKE_AGY: "ok" }, encoding: "utf8" });
  const [verdict, findings] = ran.stdout.split("\n");
  assert.equal(verdict, "reviewed: 1 finding (gemini-4-neon-medium; a type that derives Deserialize changed: Note in src/store.rs)");
  assert.equal(ran.status, 0);
  const report = JSON.parse(readFileSync(findings.replace("findings: ", ""), "utf8"));
  assert.equal(report.findings[0].summary, "no default");
  const agy = JSON.parse(readFileSync(join(s.dir, "agy-env.json"), "utf8"));
  assert.ok(!agy.keys.includes("STUDIO_TERMINAL_ID"));
  assert.ok(agy.cwd.includes("outside-review-") && !agy.cwd.startsWith(s.repo));
  assert.deepEqual(s.left(), nothingLeft);
});

test("a review cut by agy's exit code 3 is `review unavailable`, exits 0 and leaves nothing behind", () => {
  const s = scratch();
  const ran = spawnSync(process.execPath, s.args(), { cwd: s.repo, env: { ...s.env, FAKE_AGY: "exit3" }, encoding: "utf8" });
  assert.equal(ran.stdout.split("\n")[0], "review unavailable: the review was cut at 15 minutes");
  assert.equal(ran.status, 0);
  assert.deepEqual(s.left(), nothingLeft);
});

test("a review that is killed ends its agy and removes its worktree", async () => {
  const s = scratch();
  const child = spawn(process.execPath, s.args(), { cwd: s.repo, env: { ...s.env, FAKE_AGY: "sleep" }, stdio: "ignore" });
  const closed = new Promise((done) => child.on("close", done));
  const pidFile = join(s.dir, "agy.pid");
  for (let i = 0; i < 200 && !existsSync(pidFile); i++) await new Promise((done) => setTimeout(done, 50));
  const agy = Number(readFileSync(pidFile, "utf8"));
  // Never 0: `process.kill(0, …)` would signal this test's own process group.
  assert.ok(agy > 0, "the stand-in agy wrote no pid");
  child.kill("SIGTERM");
  await closed;
  assert.deepEqual(s.left(), nothingLeft);
  let alive = true;
  for (let i = 0; i < 40 && alive; i++) {
    try {
      process.kill(agy, 0);
      await new Promise((done) => setTimeout(done, 50));
    } catch {
      alive = false;
    }
  }
  if (alive) process.kill(agy, "SIGKILL");
  assert.equal(alive, false, "the stand-in agy outlived the review");
});

test("--since must name a commit of the branch, and --out a folder that can be made: both fail before agy runs", () => {
  const s = scratch();
  const run = (more) => spawnSync(process.execPath, s.args(more), { cwd: s.repo, env: { ...s.env, FAKE_AGY: "ok" }, encoding: "utf8" });
  s.git("checkout", "--quiet", "-b", "elsewhere", "main");
  writeFileSync(join(s.repo, "other.txt"), "x");
  s.git("add", "-A");
  s.git("commit", "--quiet", "-m", "elsewhere");
  const stray = s.git("rev-parse", "HEAD").trim();
  s.git("checkout", "--quiet", "change");
  for (const [name, ran] of Object.entries({ stray: run(["--since", stray]), unknown: run(["--since", "nope"]) })) {
    assert.equal(ran.status, 2, name);
    assert.match(ran.stderr, /--since .* is not a commit of this branch/, name);
  }
  writeFileSync(join(s.dir, "file"), "");
  const blocked = spawnSync(process.execPath, [script, "--out", join(s.dir, "file/out")], { cwd: s.repo, env: { ...s.env, FAKE_AGY: "ok" }, encoding: "utf8" });
  assert.equal(blocked.status, 2);
  assert.match(blocked.stderr, /--out/);
  assert.ok(!existsSync(join(s.dir, "agy-env.json")), "agy ran");
  assert.deepEqual(s.left(), nothingLeft);
});
