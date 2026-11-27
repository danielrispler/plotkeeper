// `node scripts/check/slot.mjs <command...>`, the wrapper `npm run check` runs (#783), driven as a process. Each test
// points it at a temporary slot folder and short timing limits through its environment, and wraps a small command that
// records when it starts and ends. These tests run no real `npm run check`. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const script = join(import.meta.dirname, "../../scripts/check/slot.mjs");

// The wrapped command: `node -e <job> <log> <name> <ms> <code>` logs its start, prints a line, sleeps `ms`, logs its
// end and exits with `code`.
const job = `const [log, name, ms, code] = process.argv.slice(1);
const { appendFileSync } = require("node:fs");
appendFileSync(log, JSON.stringify({ name, at: "start", t: Date.now() }) + "\\n");
console.log("job " + name + " ran");
setTimeout(() => { appendFileSync(log, JSON.stringify({ name, at: "end", t: Date.now() }) + "\\n"); process.exit(Number(code)); }, Number(ms));`;

const scratch = () => realpathSync(mkdtempSync(join(tmpdir(), "check-slot-test-")));

/** Starts the wrapper on a job from `cwd`; `done` resolves to its exit code, signal and output. */
const wrap = ({ dir, slots, name, ms = 0, code = 0, cwd = dir, env = {} }) => {
  const limits = { CHECK_SLOT_DIR: slots, CHECK_SLOT_POLL_MS: "50", CHECK_SLOT_TOUCH_MS: "100", CHECK_SLOT_STALE_MS: "2000", ...env };
  const child = spawn(process.execPath, [script, process.execPath, "-e", job, join(dir, "log.jsonl"), name, String(ms), String(code)], {
    cwd,
    env: { ...process.env, ...limits },
  });
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => (stdout += chunk));
  child.stderr.on("data", (chunk) => (stderr += chunk));
  const done = new Promise((resolve) => child.on("close", (status, signal) => resolve({ status, signal, stdout, stderr })));
  return { child, done, stderr: () => stderr };
};

/** The wrapper's result, or "still waiting" when it has not ended within `ms`, which kills it. */
const within = async (wrapper, ms) => {
  let timer;
  const late = new Promise((resolve) => (timer = setTimeout(() => resolve("still waiting"), ms)));
  const ended = await Promise.race([wrapper.done, late]);
  clearTimeout(timer);
  if (ended === "still waiting") {
    wrapper.child.kill("SIGKILL");
    await wrapper.done;
  }
  return ended;
};

const log = (dir) => (existsSync(join(dir, "log.jsonl")) ? readFileSync(join(dir, "log.jsonl"), "utf8").trim().split("\n").map((line) => JSON.parse(line)) : []);

test("the wrapped command's output and exit code pass through", async () => {
  const dir = scratch();
  const ran = await wrap({ dir, slots: join(dir, "slots"), name: "a", code: 3 }).done;
  assert.equal(ran.status, 3, ran.stderr);
  assert.equal(ran.stdout, "job a ran\n");
  assert.deepEqual(log(dir).map((line) => line.at), ["start", "end"]);
});

/** The most jobs of the log that ran at once. */
const mostAtOnce = (lines) => {
  let running = 0;
  let most = 0;
  for (const line of [...lines].sort((a, b) => a.t - b.t || (a.at === "end" ? -1 : 1))) {
    running += line.at === "start" ? 1 : -1;
    most = Math.max(most, running);
  }
  return most;
};

const literal = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

test("three checks at once run two at a time, past the staleness limit: the third waits, names the holders, and starts after one ends", async () => {
  const dir = scratch();
  const slots = join(dir, "slots");
  const worktrees = ["one", "two", "three"].map((name) => {
    mkdirSync(join(dir, name));
    return join(dir, name);
  });
  // Each check runs past the staleness limit, so a holder keeps its slot only by touching it.
  const env = { CHECK_SLOT_TOUCH_MS: "100", CHECK_SLOT_STALE_MS: "1500" };
  const wrappers = worktrees.map((cwd, i) => wrap({ dir, slots, cwd, name: String(i), ms: 2000, env }));
  const ran = await Promise.all(wrappers.map((wrapper) => wrapper.done));
  for (const one of ran) assert.equal(one.status, 0, one.stderr);
  const lines = log(dir);
  assert.equal(lines.length, 6);
  assert.equal(mostAtOnce(lines), 2);
  const starts = lines.filter((line) => line.at === "start").sort((a, b) => a.t - b.t);
  const firstEnd = Math.min(...lines.filter((line) => line.at === "end").map((line) => line.t));
  assert.ok(starts[2].t >= firstEnd, "the third started before a slot freed");
  const waited = ran.filter((one) => /waiting for a check slot/.test(one.stderr));
  assert.equal(waited.length, 1, ran.map((one) => one.stderr).join("\n"));
  assert.equal(waited[0].stderr.match(/waiting for a check slot/g).length, 1, "the waiting line is printed once");
  const holders = worktrees.filter((_, i) => ran[i] !== waited[0]);
  for (const holder of holders) assert.match(waited[0].stderr, new RegExp(`${literal(holder)} \\(\\d+s\\)`));
});

/** Fills a slot as another check would have: its file names `pid` and was last touched `ago` ms ago. */
const holdSlot = (slots, slot, pid, ago = 0) => {
  mkdirSync(slots, { recursive: true });
  const path = join(slots, slot);
  writeFileSync(path, JSON.stringify({ pid, worktree: `/elsewhere/${slot}`, since: Date.now() - ago }));
  const at = new Date(Date.now() - ago);
  utimesSync(path, at, at);
  return path;
};

test("a slot whose file names a dead pid is taken back", async () => {
  const dir = scratch();
  const slots = join(dir, "slots");
  const dead = spawnSync(process.execPath, ["-e", ""]).pid;
  for (const slot of ["1", "2"]) holdSlot(slots, slot, dead);
  const ran = await within(wrap({ dir, slots, name: "a", env: { CHECK_SLOT_STALE_MS: "60000" } }), 5000);
  assert.notEqual(ran, "still waiting");
  assert.equal(ran.status, 0, ran.stderr);
  assert.equal(ran.stdout, "job a ran\n");
});

test("a slot whose live holder left it untouched past the limit is taken back, and one touched recently is not", async () => {
  const dir = scratch();
  const slots = join(dir, "slots");
  holdSlot(slots, "1", process.pid, 30_000);
  const fresh = holdSlot(slots, "2", process.pid);
  const before = readFileSync(fresh, "utf8");
  const ran = await within(wrap({ dir, slots, name: "a", env: { CHECK_SLOT_STALE_MS: "10000" } }), 5000);
  assert.notEqual(ran, "still waiting");
  assert.equal(ran.status, 0, ran.stderr);
  assert.equal(existsSync(fresh) && readFileSync(fresh, "utf8"), before, "the recently touched slot was taken");
  assert.equal(existsSync(join(slots, "1")), false, "the check gave its slot back");
});

test("SIGTERM to a waiting check ends it without running the command, and leaves nothing in the slot folder", async () => {
  const dir = scratch();
  const slots = join(dir, "slots");
  for (const slot of ["1", "2"]) holdSlot(slots, slot, process.pid);
  const wrapper = wrap({ dir, slots, name: "a", env: { CHECK_SLOT_STALE_MS: "60000" } });
  while (!/waiting for a check slot/.test(wrapper.stderr())) await new Promise((resolve) => setTimeout(resolve, 20));
  wrapper.child.kill("SIGTERM");
  const ran = await within(wrapper, 3000);
  assert.notEqual(ran, "still waiting");
  assert.equal(ran.status, 143, ran.stderr);
  assert.deepEqual(log(dir), []);
  assert.deepEqual(readdirSync(slots).sort(), ["1", "2"]);
});

test("a slot folder that can't be made runs the command without a slot and says so", async () => {
  const dir = scratch();
  writeFileSync(join(dir, "file"), "");
  const slots = join(dir, "file/slots");
  const ran = await wrap({ dir, slots, name: "a" }).done;
  assert.equal(ran.status, 0, ran.stderr);
  assert.equal(ran.stdout, "job a ran\n");
  assert.equal(ran.stderr, `check: the check slot folder ${slots} can't be written (ENOTDIR), so this check runs without a slot\n`);
});
