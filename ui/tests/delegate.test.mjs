// `node scripts/agents/delegate.mjs`, run as the orchestrator runs it, with a stand-in `agy` first on the PATH.
// These tests run no real `agy`. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
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

// `delegate.mjs run`, in a scratch main checkout with a bare origin and stand-ins for `opencode`, `npm`, `gh` and
// `herdr` first on the PATH. These tests run none of the real ones.

// The stand-in worker. FAKE_OPENCODE says what it does: "edit" (changes a file and exits 0), "commit" (also commits
// it, twice), "moved" (edits, and someone else moves the integration branch), "conflict" (edits a line someone else
// also changes on the integration branch), "pull" (someone else moves the integration branch, the worker pulls it, then
// edits), "pushint" (edits, commits and pushes its commit to the integration branch), "lost" (edits, and origin goes away), "empty" (exits 0 with no change), "fail" (exits 1), "chatty" (prints an event
// every 50 ms and never ends), "push" (edits, commits and pushes its commit to a branch of its own), "branch" (edits and
// pushes the integration tip as \`issue-7-cheap\`, no commit of its own) or "silent" (prints nothing and waits). FAKE_SILENT_MODELS, a comma-separated list, makes those models "silent" whatever FAKE_OPENCODE says;
// a silent worker leaves a stray file first. Every call is logged with the environment it got.
const fakeOpencode = `#!/usr/bin/env node
const { execFileSync } = require("node:child_process");
const { appendFileSync, writeFileSync } = require("node:fs");
const { join } = require("node:path");
const env = process.env;
const seen = Object.fromEntries(Object.entries(env).filter(([key]) => key.startsWith("OPENCODE_") || key === "PLOTKEEPER_INTERNAL"));
appendFileSync(join(env.FAKE_DIR, "opencode-calls.jsonl"), JSON.stringify({ argv: process.argv.slice(2), cwd: process.cwd(), env: seen }) + "\\n");
const model = process.argv[process.argv.indexOf("-m") + 1];
const mode = (env.FAKE_SILENT_MODELS ?? "").split(",").includes(model) ? "silent" : env.FAKE_OPENCODE;
const event = (type, part) => console.log(JSON.stringify({ type, timestamp: 1, sessionID: "ses_test", part: { sessionID: "ses_test", ...part } }));
const other = (file, text) => {
  const git = (...args) => execFileSync("git", args, { cwd: join(env.FAKE_DIR, "other"), stdio: "ignore" });
  git("pull", "--quiet", "origin", "int");
  writeFileSync(join(env.FAKE_DIR, "other", file), text);
  git("add", "--all");
  git("commit", "--quiet", "-m", "someone else");
  git("push", "--quiet", "origin", "HEAD:int");
};
if (mode === "chatty") setInterval(() => event("text", { type: "text", text: "still going" }), 50);
else if (mode === "silent") {
  writeFileSync("stray.txt", "left by a silent worker\\n");
  setTimeout(() => {}, 60000);
}
else {
  event("step_start", { type: "step-start" });
  event("tool_use", { type: "tool", tool: "read", state: { status: "completed", input: { filePath: join(process.cwd(), "AGENTS.md") }, output: "long" } });
  if (mode === "pull") {
    other("other.txt", "someone else's file\\n");
    execFileSync("git", ["pull", "--quiet", "--no-rebase", "--no-edit", "origin", "int"]);
  }
  if (["edit", "commit", "moved", "conflict", "lost", "push", "pushint", "branch", "pull"].includes(mode)) {
    writeFileSync("shared.txt", "the worker's line\\n");
    event("tool_use", { type: "tool", tool: "edit", state: { status: "completed", input: { filePath: "shared.txt" }, output: "ok" } });
  }
  if (mode === "commit") {
    execFileSync("git", ["commit", "--quiet", "-am", "worker one"]);
    writeFileSync("added.txt", "more\\n");
    execFileSync("git", ["add", "added.txt"]);
    execFileSync("git", ["commit", "--quiet", "-m", "worker two"]);
  }
  if (mode === "push") {
    execFileSync("git", ["commit", "--quiet", "-am", "worker"]);
    execFileSync("git", ["push", "--quiet", "origin", "HEAD:refs/heads/sneaky"]);
  }
  if (mode === "pushint") {
    execFileSync("git", ["commit", "--quiet", "-am", "worker"]);
    execFileSync("git", ["push", "--quiet", "origin", "HEAD:int"]);
  }
  if (mode === "branch") execFileSync("git", ["push", "--quiet", "origin", "origin/int:refs/heads/issue-7-cheap"]);
  if (mode === "moved") other("other.txt", "someone else's file\\n");
  if (mode === "conflict") other("shared.txt", "someone else's line\\n");
  if (mode === "lost") require("node:fs").renameSync(join(env.FAKE_DIR, "origin.git"), join(env.FAKE_DIR, "origin-gone.git"));
  event("text", { type: "text", text: "Done: the ticket is implemented." });
  event("step_finish", { type: "step-finish", reason: "stop" });
  if (mode === "fail") process.exit(1);
}
`;

// The stand-in npm: \`ci\` does nothing, \`run check\` passes unless FAKE_NPM_CHECK is "red", or "red-once" and this
// is the first check. Every call is logged.
const fakeNpm = `#!/usr/bin/env node
const { appendFileSync, existsSync, readFileSync } = require("node:fs");
const { join } = require("node:path");
const env = process.env;
const log = join(env.FAKE_DIR, "npm-calls.jsonl");
const checks = existsSync(log) ? readFileSync(log, "utf8").split("\\n").filter((line) => line.includes('"check"')).length : 0;
appendFileSync(log, JSON.stringify({ argv: process.argv.slice(2), cwd: process.cwd(), internal: env.PLOTKEEPER_INTERNAL }) + "\\n");
const red = env.FAKE_NPM_CHECK === "red" || (env.FAKE_NPM_CHECK === "red-once" && checks === 0);
if (process.argv[2] === "run" && process.argv[3] === "check" && red) {
  console.log("test agents::reads ... FAILED");
  process.exit(1);
}
`;

// The stand-in gh: \`issue view <n> --json title,body\` prints \`issues/<n>.json\`; \`pr list\` prints FAKE_GH_PRS, no PR
// by default. Every call is logged.
const fakeGh = `#!/usr/bin/env node
const { appendFileSync, readFileSync } = require("node:fs");
const { join } = require("node:path");
const argv = process.argv.slice(2);
appendFileSync(join(process.env.FAKE_DIR, "gh-calls.jsonl"), JSON.stringify({ argv }) + "\\n");
const [noun, verb, number] = argv;
if (noun === "pr" && verb === "list") {
  console.log(process.env.FAKE_GH_PRS || "[]");
  process.exit(0);
}
if (noun !== "issue" || verb !== "view") process.exit(1);
process.stdout.write(readFileSync(join(process.env.FAKE_DIR, "issues", number + ".json"), "utf8"));
`;

// The stand-in herdr. \`tab list\` gives FAKE_HERDR_TABS, \`pane list\` FAKE_HERDR_PANES; \`tab create\` makes tab w1:t9
// with root pane w1:p9, \`pane split\` makes pane w1:p8. Every call is logged.
const fakeHerdr = `#!/usr/bin/env node
const { appendFileSync } = require("node:fs");
const { join } = require("node:path");
const env = process.env;
const argv = process.argv.slice(2);
appendFileSync(join(env.FAKE_DIR, "herdr-calls.jsonl"), JSON.stringify({ argv }) + "\\n");
const command = argv.slice(0, 2).join(" ");
const result =
  command === "tab list" ? { tabs: JSON.parse(env.FAKE_HERDR_TABS || "[]") }
  : command === "pane list" ? { panes: JSON.parse(env.FAKE_HERDR_PANES || "[]") }
  : command === "tab create" ? { tab: { tab_id: "w1:t9", label: "workers" }, root_pane: { pane_id: "w1:p9", tab_id: "w1:t9" } }
  : command === "pane split" ? { pane: { pane_id: "w1:p8", tab_id: "w1:t2" } }
  : {};
console.log(JSON.stringify({ id: "1", result }));
`;

const ticket = { title: "Workflow: the thing", body: "## Parent\n\n#5\n\n## What to build\n\nThe thing.\n" };
const spec = { title: "Spec: things", body: "## Problem Statement\n\nNo things.\n" };

/**
 * A main checkout `repo` whose origin has the integration branch `int`, and a second clone `other` of it.
 * @param {{ cheap?: object[] }} [options] the `cheap` list of the routing file, one test candidate by default
 */
const scratchRun = ({ cheap } = {}) => {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), "delegate-run-test-")));
  const repo = join(dir, "repo");
  for (const folder of ["bin", "issues", "repo"]) mkdirSync(join(dir, folder));
  for (const [name, text] of Object.entries({ opencode: fakeOpencode, npm: fakeNpm, gh: fakeGh, herdr: fakeHerdr })) {
    writeFileSync(join(dir, "bin", name), text);
    chmodSync(join(dir, "bin", name), 0o755);
  }
  writeFileSync(join(dir, "issues/7.json"), JSON.stringify(ticket));
  writeFileSync(join(dir, "issues/5.json"), JSON.stringify(spec));
  const candidates = cheap ?? [{ cli: "opencode", model: "opencode/test-free", variant: { medium: null, high: "high" } }];
  writeFileSync(join(dir, "routing.json"), JSON.stringify({ ...routing, cheap: candidates }));
  writeFileSync(join(dir, "gitconfig"), "[user]\n\tname = t\n\temail = t@example.com\n[commit]\n\tgpgsign = false\n");
  const env = {
    ...process.env,
    PATH: `${join(dir, "bin")}:${process.env.PATH}`,
    GIT_CONFIG_GLOBAL: join(dir, "gitconfig"),
    GIT_CONFIG_NOSYSTEM: "1",
    PLOTKEEPER_INTERNAL: "1",
    FAKE_DIR: dir,
  };
  for (const key of ["HERDR_ENV", "HERDR_WORKSPACE_ID", "HERDR_TAB_ID", "HERDR_PANE_ID"]) delete env[key];
  const git = (...args) => execFileSync("git", args, { cwd: repo, env, encoding: "utf8" }).trim();
  git("init", "--quiet", "--initial-branch=main");
  writeFileSync(join(repo, "shared.txt"), "the first line\n");
  git("add", "-A");
  git("commit", "--quiet", "-m", "base");
  git("clone", "--quiet", "--bare", ".", join(dir, "origin.git"));
  git("remote", "add", "origin", join(dir, "origin.git"));
  git("push", "--quiet", "origin", "main:int");
  git("clone", "--quiet", "--branch", "int", join(dir, "origin.git"), join(dir, "other"));
  const run = (fake = {}, more = []) => {
    const args = [script, "run", "--ticket", "7", "--integration", "int", "--effort", "medium", "--routing", join(dir, "routing.json"), ...more];
    const ran = spawnSync(process.execPath, args, { cwd: repo, env: { ...env, ...fake }, encoding: "utf8" });
    const calls = (name) => {
      const log = join(dir, `${name}-calls.jsonl`);
      return existsSync(log) ? readFileSync(log, "utf8").trim().split("\n").map((line) => JSON.parse(line)) : [];
    };
    return { ...ran, calls };
  };
  const worktree = join(repo, ".claude/worktrees/issue-7-cheap");
  return { dir, repo, git, run, worktree };
};

test("run: a worker that edits and exits 0 with a green check gives pass and one commit with the Worker trailer", () => {
  const { git, run, worktree } = scratchRun();
  const ran = run({ FAKE_OPENCODE: "edit" });
  assert.equal(ran.status, 0, ran.stderr);
  const [verdict, ...rest] = ran.stdout.trimEnd().split("\n");
  assert.equal(verdict, "pass");
  assert.match(rest.join("\n"), /shared\.txt \| 2 \+-/);
  assert.match(rest.at(-1), /^Route outcome: cheap·medium opencode\/test-free → pass \| \d+m$/);
  const commits = git("log", "--format=%B%x00", "origin/int..issue-7-cheap").split("\0").map((body) => body.trim()).filter(Boolean);
  assert.deepEqual(commits, ["Workflow: the thing\n\nWorker: opencode/test-free"]);
  assert.equal(readFileSync(join(worktree, "shared.txt"), "utf8"), "the worker's line\n");
});

/** The commit bodies on `branch` that are not on the integration branch, merges left out. */
const commitsOn = (git, branch) =>
  git("log", "--no-merges", "--format=%B%x00", `origin/int..${branch}`)
    .split("\0")
    .map((body) => body.trim())
    .filter(Boolean);

test("run: on pass, the worker's own commits are squashed into the one commit, and the moved integration tip is merged in", () => {
  const { git, run, worktree } = scratchRun();
  const ran = run({ FAKE_OPENCODE: "commit" });
  assert.equal(ran.status, 0, ran.stderr);
  assert.match(ran.stdout, /^pass\n/);
  assert.deepEqual(commitsOn(git, "issue-7-cheap"), ["Workflow: the thing\n\nWorker: opencode/test-free"]);
  assert.equal(readFileSync(join(worktree, "added.txt"), "utf8"), "more\n");

  const moved = scratchRun();
  const after = moved.run({ FAKE_OPENCODE: "moved" });
  assert.match(after.stdout, /^pass\n/);
  moved.git("fetch", "--quiet", "origin");
  assert.doesNotThrow(() => moved.git("merge-base", "--is-ancestor", "origin/int", "issue-7-cheap"));
  assert.deepEqual(commitsOn(moved.git, "issue-7-cheap"), ["Workflow: the thing\n\nWorker: opencode/test-free"]);
  assert.doesNotMatch(after.stdout, /other\.txt/, "the diff stat shows only the worker's change");
});

test("run: the worker runs in the worktree with the brief, the ticket and its spec, and no PLOTKEEPER_INTERNAL anywhere", () => {
  const { run, worktree } = scratchRun();
  const ran = run({ FAKE_OPENCODE: "edit" });
  assert.equal(ran.status, 0, ran.stderr);
  const [worker] = ran.calls("opencode");
  assert.equal(worker.cwd, worktree);
  assert.deepEqual(worker.argv.slice(0, 6), ["run", "--auto", "--format", "json", "-m", "opencode/test-free"]);
  assert.equal(worker.argv.length, 7, "effort medium maps to no --variant for this model");
  assert.match(worker.argv[6], /Read \.delegate\/ticket\.md/);
  assert.equal(readFileSync(join(worktree, ".delegate/brief.md"), "utf8").trim(), worker.argv[6]);
  assert.deepEqual(worker.env, {
    OPENCODE_EXPERIMENTAL_BASH_DEFAULT_TIMEOUT_MS: "480000",
    OPENCODE_PERMISSION: JSON.stringify({ bash: { "*": "allow", "git push*": "deny", "gh *": "deny" } }),
    OPENCODE_DISABLE_CLAUDE_CODE_PROMPT: "1",
    OPENCODE_DISABLE_EXTERNAL_SKILLS: "1",
  });
  const npm = ran.calls("npm");
  assert.deepEqual(npm.map((call) => call.argv.join(" ")), ["ci", "run check"]);
  assert.ok(npm.every((call) => call.cwd === worktree && call.internal === undefined));
  const brief = readFileSync(join(worktree, ".delegate/ticket.md"), "utf8");
  assert.match(brief, /^# Workflow: the thing\n\n## Parent\n\n#5\n/);
  assert.match(brief, /\n---\n\n# Parent spec: Spec: things\n\n## Problem Statement\n\nNo things\.\n$/);
  assert.equal(readFileSync(join(worktree, ".delegate/run.log"), "utf8"), `> read {"filePath":"${worktree}/AGENTS.md"}\n> edit {"filePath":"shared.txt"}\nDone: the ticket is implemented.\n`);
});

test("run: effort high passes the candidate's variant for high", () => {
  const { run } = scratchRun();
  const ran = run({ FAKE_OPENCODE: "edit" }, ["--effort", "high"]);
  assert.equal(ran.status, 0, ran.stderr);
  const [worker] = ran.calls("opencode");
  assert.deepEqual(worker.argv.slice(6, 8), ["--variant", "high"]);
  assert.match(ran.stdout, /Route outcome: cheap·high opencode\/test-free → pass/);
});

const gone = ({ git, worktree }) => {
  assert.equal(existsSync(worktree), false, "the worktree is removed");
  assert.equal(git("branch", "--list", "issue-7-cheap"), "", "the branch is removed");
};

for (const [what, fake, reason, result] of [
  ["an empty change, twice", { FAKE_OPENCODE: "empty" }, "red: no change", "red,red"],
  ["an error exit", { FAKE_OPENCODE: "fail" }, "exit 1", "error"],
  ["a red check, twice", { FAKE_OPENCODE: "edit", FAKE_NPM_CHECK: "red" }, "red check", "red,red"],
  ["a merge conflict", { FAKE_OPENCODE: "conflict" }, "conflict", "conflict"],
  ["a worker silent past the limit", { FAKE_OPENCODE: "silent", DELEGATE_SILENCE_MS: "300" }, "silent", "silent"],
  ["a worker past the attempt limit", { FAKE_OPENCODE: "chatty", DELEGATE_ATTEMPT_MS: "600" }, "timeout", "error"],
]) {
  test(`run: ${what} gives escalate medium (${reason}) and removes the worktree and branch`, () => {
    const scratch = scratchRun();
    const ran = scratch.run(fake);
    assert.equal(ran.status, 0, ran.stderr);
    const lines = ran.stdout.trimEnd().split("\n");
    assert.equal(lines[0], `escalate medium (${reason})`);
    assert.match(lines.at(-1), new RegExp(`^Route outcome: cheap·medium opencode/test-free → ${result} \\| \\d+m(, \\d+m)*$`));
    gone(scratch);
  });
}

test("run: a red check, then green after one retry on the same session with the check's tail, gives pass", () => {
  const { git, run, worktree } = scratchRun();
  const ran = run({ FAKE_OPENCODE: "edit", FAKE_NPM_CHECK: "red-once" });
  assert.equal(ran.status, 0, ran.stderr);
  const lines = ran.stdout.trimEnd().split("\n");
  assert.equal(lines[0], "pass");
  assert.match(lines.at(-1), /^Route outcome: cheap·medium opencode\/test-free → red,pass \| \d+m, \d+m$/);
  const [first, retry, ...more] = ran.calls("opencode");
  assert.equal(more.length, 0, "one retry only");
  assert.equal(first.argv.includes("--session"), false);
  assert.equal(retry.cwd, worktree);
  assert.deepEqual(retry.argv.slice(0, 8), ["run", "--auto", "--format", "json", "-m", "opencode/test-free", "--session", "ses_test"]);
  assert.match(retry.argv.at(-1), /test agents::reads \.\.\. FAILED/);
  assert.equal(retry.env.OPENCODE_DISABLE_CLAUDE_CODE_PROMPT, "1");
  assert.deepEqual(ran.calls("npm").map((call) => call.argv.join(" ")), ["ci", "run check", "run check"]);
  assert.deepEqual(commitsOn(git, "issue-7-cheap"), ["Workflow: the thing\n\nWorker: opencode/test-free"]);
});

test("run: an empty change is retried with the message that no change was made", () => {
  const ran = scratchRun().run({ FAKE_OPENCODE: "empty" });
  const [, retry] = ran.calls("opencode");
  assert.ok(retry, "the empty change is retried");
  assert.match(retry.argv.at(-1), /no change was made/i);
});

const two = [
  { cli: "opencode", model: "opencode/a-free", variant: { medium: null, high: "high" } },
  { cli: "opencode", model: "opencode/b-free", variant: { medium: "medium", high: "high" } },
];

// A silence limit long enough that a worker that does answer is never killed while Node starts it, even on a loaded
// machine: at 300 ms the healthy candidate was once killed as silent too.
const HEALTHY_SILENCE_MS = "2000";

const models = (calls) => calls.map((call) => call.argv[call.argv.indexOf("-m") + 1]);

test("run: a silent worker is killed and marked out, and the next candidate runs in the reset worktree", () => {
  const { repo, run, worktree } = scratchRun({ cheap: two });
  const ran = run({ FAKE_OPENCODE: "edit", FAKE_SILENT_MODELS: "opencode/a-free", DELEGATE_SILENCE_MS: HEALTHY_SILENCE_MS });
  assert.equal(ran.status, 0, ran.stderr);
  const lines = ran.stdout.trimEnd().split("\n");
  assert.equal(lines[0], "pass");
  assert.match(lines.at(-1), /^Route outcome: cheap·medium opencode\/a-free → silent → opencode\/b-free → pass \| \d+m; \d+m$/);
  assert.deepEqual(models(ran.calls("opencode")), ["opencode/a-free", "opencode/b-free"]);
  assert.equal(existsSync(join(worktree, "stray.txt")), false, "the silent worker's file is cleaned away");
  assert.equal(readFileSync(join(worktree, "shared.txt"), "utf8"), "the worker's line\n");
  const out = JSON.parse(readFileSync(join(repo, ".git/delegate-silent.json"), "utf8"));
  const midnight = new Date();
  midnight.setUTCHours(24, 0, 0, 0);
  assert.deepEqual(out, { "opencode/a-free": midnight.toISOString() });
});

test("run: a model marked out today is skipped by the next run, and with none left the run escalates as silent", () => {
  const scratch = scratchRun({ cheap: two });
  scratch.run({ FAKE_OPENCODE: "edit", FAKE_SILENT_MODELS: "opencode/a-free", DELEGATE_SILENCE_MS: HEALTHY_SILENCE_MS });
  const second = scratch.run({ FAKE_OPENCODE: "edit", FAKE_SILENT_MODELS: "opencode/b-free", DELEGATE_SILENCE_MS: "300" });
  assert.equal(second.status, 0, second.stderr);
  assert.deepEqual(models(second.calls("opencode")), ["opencode/a-free", "opencode/b-free", "opencode/b-free"], "the second run skips a-free");
  const lines = second.stdout.trimEnd().split("\n");
  assert.equal(lines[0], "escalate medium (silent)");
  assert.match(lines.at(-1), /^Route outcome: cheap·medium opencode\/b-free → silent \| \d+m$/);
  gone(scratch);
  const third = scratch.run({ FAKE_OPENCODE: "edit" });
  assert.equal(third.stdout, "escalate medium (silent)\n");
  assert.equal(third.calls("opencode").length, 3, "no worker starts when every model is marked out");
  assert.equal(third.calls("npm").filter((call) => call.argv[0] === "ci").length, 2, "nor npm ci");
});

test("run: a mark from an earlier UTC day has expired", () => {
  const scratch = scratchRun({ cheap: two });
  writeFileSync(join(scratch.repo, ".git/delegate-silent.json"), JSON.stringify({ "opencode/a-free": "2020-01-02T00:00:00.000Z" }));
  const ran = scratch.run({ FAKE_OPENCODE: "edit" });
  assert.deepEqual(models(ran.calls("opencode")), ["opencode/a-free"]);
});

for (const [what, fake, said] of /** @type {[string, Record<string, string>, RegExp][]} */ ([
  ["a worker that pushed its commit", { FAKE_OPENCODE: "push" }, /^stop: origin\/sneaky has the worker's commit [0-9a-f]{12}$/],
  ["a worker that pushed its commit to the integration branch", { FAKE_OPENCODE: "pushint" }, /^stop: origin\/int has the worker's commit [0-9a-f]{12}$/],
  ["an issue-<n>-cheap branch on origin", { FAKE_OPENCODE: "branch" }, /^stop: origin has the branch issue-7-cheap$/],
  ["an open PR from issue-<n>-cheap", { FAKE_OPENCODE: "edit", FAKE_GH_PRS: JSON.stringify([{ url: "https://github.com/o/r/pull/9" }]) }, /^stop: a PR from issue-7-cheap is open: https:\/\/github\.com\/o\/r\/pull\/9$/],
])) {
  test(`run: ${what} gives stop, exit 0, and leaves the worktree for Daniel`, () => {
    const { run, worktree } = scratchRun();
    const ran = run(fake);
    assert.equal(ran.status, 0, ran.stderr);
    const lines = ran.stdout.trimEnd().split("\n");
    assert.match(lines[0], said);
    assert.match(lines.at(-1), /^Route outcome: cheap·medium opencode\/test-free → stop \| \d+m$/);
    assert.ok(existsSync(worktree), "the worktree is kept as evidence");
    assert.deepEqual(ran.calls("npm").map((call) => call.argv.join(" ")), ["ci"], "no check runs once the worker pushed");
  });
}

test("run: the guard asks gh for a PR from the ticket's branch, and a ref someone else moved gives no stop", () => {
  const ran = scratchRun().run({ FAKE_OPENCODE: "moved" });
  assert.match(ran.stdout, /^pass\n/);
  const pr = ran.calls("gh").find((call) => call.argv[0] === "pr");
  assert.ok(pr, "gh pr list was called");
  assert.deepEqual(pr.argv.slice(0, 4), ["pr", "list", "--head", "issue-7-cheap"]);
});

test("run: a worker that pulls the moved integration branch gives pass, not stop", () => {
  const ran = scratchRun().run({ FAKE_OPENCODE: "pull" });
  assert.equal(ran.status, 0, ran.stderr);
  assert.match(ran.stdout, /^pass\n/);
});

/** A PATH with the stand-ins and node only, so that a stand-in removed from `bin` is missing, never the real one. */
const bare = (dir) => {
  mkdirSync(join(dir, "node-bin"));
  symlinkSync(process.execPath, join(dir, "node-bin/node"));
  return `${join(dir, "bin")}:${join(dir, "node-bin")}:/usr/bin:/bin`;
};

test("run: with opencode missing, the run could not start: exit 1, and no worktree or branch left", () => {
  const scratch = scratchRun();
  rmSync(join(scratch.dir, "bin/opencode"));
  const ran = scratch.run({ FAKE_OPENCODE: "edit", PATH: bare(scratch.dir) });
  assert.equal(ran.status, 1, ran.stderr);
  assert.match(ran.stderr, /^could not start: opencode .*ENOENT/);
  assert.equal(ran.stdout, "");
  gone(scratch);
});

test("run: a leftover issue-<n>-cheap worktree and branch are removed first", () => {
  const scratch = scratchRun();
  const { git, run, worktree } = scratch;
  git("worktree", "add", "--quiet", "-b", "issue-7-cheap", worktree, "main");
  writeFileSync(join(worktree, "leftover.txt"), "old\n");
  execFileSync("git", ["add", "leftover.txt"], { cwd: worktree });
  execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "--quiet", "-m", "leftover"], { cwd: worktree });
  const ran = run({ FAKE_OPENCODE: "edit" });
  assert.equal(ran.status, 0, ran.stderr);
  assert.match(ran.stdout, /^pass\n/);
  assert.equal(existsSync(join(worktree, "leftover.txt")), false);
  assert.deepEqual(commitsOn(git, "issue-7-cheap"), ["Workflow: the thing\n\nWorker: opencode/test-free"]);
});

test("run: outside Herdr, no pane is opened", () => {
  const ran = scratchRun().run({ FAKE_OPENCODE: "edit" });
  assert.equal(ran.status, 0, ran.stderr);
  assert.deepEqual(ran.calls("herdr"), []);
});

test("run: in Herdr, a new workers tab tails the log at the worktree, and only its pane is closed", () => {
  const { run, worktree } = scratchRun();
  const ran = run({ FAKE_OPENCODE: "edit", HERDR_ENV: "1", HERDR_WORKSPACE_ID: "w1", FAKE_HERDR_TABS: JSON.stringify([{ tab_id: "w1:t1", label: "main" }]) });
  assert.equal(ran.status, 0, ran.stderr);
  assert.deepEqual(ran.calls("herdr").map((call) => call.argv), [
    ["tab", "list", "--workspace", "w1"],
    ["tab", "create", "--workspace", "w1", "--label", "workers", "--cwd", worktree, "--no-focus"],
    ["pane", "run", "w1:p9", "tail -f .delegate/run.log"],
    ["pane", "close", "w1:p9"],
  ]);
});

test("run: in Herdr, an existing workers tab gets a split pane without focus, and only that pane is closed", () => {
  const { run, worktree } = scratchRun();
  const ran = run({
    FAKE_OPENCODE: "fail",
    HERDR_ENV: "1",
    HERDR_WORKSPACE_ID: "w1",
    FAKE_HERDR_TABS: JSON.stringify([{ tab_id: "w1:t1", label: "main" }, { tab_id: "w1:t2", label: "workers" }]),
    FAKE_HERDR_PANES: JSON.stringify([{ pane_id: "w1:p1", tab_id: "w1:t1" }, { pane_id: "w1:p5", tab_id: "w1:t2" }]),
  });
  assert.equal(ran.status, 0, ran.stderr);
  assert.match(ran.stdout, /^escalate medium \(exit 1\)/);
  const argv = ran.calls("herdr").map((call) => call.argv);
  assert.deepEqual(argv.slice(2), [
    ["pane", "split", "w1:p5", "--direction", "down", "--cwd", worktree, "--no-focus"],
    ["pane", "run", "w1:p8", "tail -f .delegate/run.log"],
    ["pane", "close", "w1:p8"],
  ]);
  assert.equal(argv.filter(([, verb]) => verb === "close").length, 1);
});

test("run: with no cheap candidate, it escalates as silent without starting a worker", () => {
  const ran = scratchRun({ cheap: [] }).run({ FAKE_OPENCODE: "edit" });
  assert.equal(ran.status, 0, ran.stderr);
  assert.equal(ran.stdout, "escalate medium (silent)\n");
  assert.deepEqual(ran.calls("opencode"), []);
});

test("outcomeSegment writes each attempt's worker and results, and one time per worker run", async () => {
  const { outcomeSegment } = await import(script);
  const run = (result, minutes) => ({ result, ms: minutes * 60_000 });
  assert.equal(outcomeSegment({ tier: "cheap", effort: "high", attempts: [{ worker: "opencode/beam-free", runs: [run("red", 9.6)] }] }), "cheap·high opencode/beam-free → red | 10m");
  assert.equal(outcomeSegment({ tier: "cheap", effort: "medium", attempts: [{ worker: "opencode/beam-free", runs: [{ result: "pass", ms: 5_000 }] }] }), "cheap·medium opencode/beam-free → pass | 1m");
  const attempts = [
    { worker: "opencode/deepseek-v4.1-flash-free", runs: [run("silent", 10)] },
    { worker: "opencode/beam-free", runs: [run("red", 14), run("red", 6)] },
  ];
  assert.equal(outcomeSegment({ tier: "cheap", effort: "medium", attempts }), "cheap·medium opencode/deepseek-v4.1-flash-free → silent → opencode/beam-free → red,red | 10m; 14m, 6m");
});

test("run: a failure after the worker ran escalates with exit 0, not as a run that could not start", () => {
  const scratch = scratchRun();
  const ran = scratch.run({ FAKE_OPENCODE: "lost" });
  assert.equal(ran.status, 0, ran.stderr);
  const lines = ran.stdout.trimEnd().split("\n");
  assert.match(lines[0], /^escalate medium \(error: git fetch failed: .+\)$/);
  assert.match(lines.at(-1), /^Route outcome: cheap·medium opencode\/test-free → error \| \d+m$/);
  gone(scratch);
});
