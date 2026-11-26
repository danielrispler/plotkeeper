#!/usr/bin/env node
// `node scripts/agents/delegate.mjs pick critic --spec <draft> [--routing <file>]`: names the critic of a draft
// spec from `routing.json`'s `critic` list (spec #592, "The cold critic and the veto gate"), and runs it when it
// is Gemini. The orchestrator writes what it prints to `reports/critic.md`.
//
// `node scripts/agents/delegate.mjs run --ticket <n> --integration <branch> --effort <medium|high> [--routing <file>]`:
// takes a cheap ticket through an OpenCode worker in its own worktree and prints the verdict (spec #592,
// "`delegate.mjs run`, a cheap ticket").
import { spawn, spawnSync } from "node:child_process";
import { appendFileSync, closeSync, existsSync, mkdirSync, openSync, readFileSync, realpathSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { agyCall, options, quotaLeft } from "../review/outside-review.mjs";

const MINUTES = 15;

// The critic's brief, the one `docs/agents/learning.md` ("Critic and veto gate") gives.
const BRIEF = `You are the critic of a draft spec for this repository. Check every claim of the draft against the repository. Mark each claim verified, with its file:line or a command and its output, or unverified. Flag contradictions with AGENTS.md, system-map.json and existing code. Rate each finding high, medium or low. You can change nothing: answer with your findings only.

The draft:

`;

class Usage extends Error {}

const ROUTING = join(import.meta.dirname, "routing.json");

const parseFlags = (rest, names, defaults) => {
  const flags = { ...defaults };
  for (let i = 0; i < rest.length; i += 2) {
    const name = rest[i]?.replace(/^--/, "");
    if (!names.includes(name) || rest[i + 1] === undefined) throw new Usage(`unknown or incomplete option ${rest[i]}`);
    flags[name] = rest[i + 1];
  }
  return flags;
};

const parse = (argv) => {
  const [verb, role] = argv;
  if (verb === "pick" && role === "critic") {
    const flags = parseFlags(argv.slice(2), ["spec", "routing"], { routing: ROUTING });
    if (!flags.spec) throw new Usage("--spec <draft> is required");
    return { command: "critic", flags };
  }
  if (verb === "run") {
    const flags = parseFlags(argv.slice(1), ["ticket", "integration", "effort", "routing"], { routing: ROUTING });
    if (!/^\d+$/.test(flags.ticket ?? "")) throw new Usage("--ticket <n> is required");
    if (!flags.integration) throw new Usage("--integration <branch> is required");
    if (!["medium", "high"].includes(flags.effort)) throw new Usage("--effort <medium|high> is required");
    return { command: "run", flags };
  }
  throw new Usage(`unknown command ${[verb, role].filter(Boolean).join(" ")}`);
};

/** One headless, read-only Gemini run: its answer, or why it gave none. */
const runAgy = (model, prompt) => {
  const env = agyCall(options([]), { env: process.env }).env;
  const args = ["--mode", "plan", "--model", model, "--print-timeout", `${MINUTES}m`, "--output-format", "json", "-p", prompt];
  const ran = spawnSync("agy", args, { env, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: (MINUTES + 1) * 60_000, maxBuffer: 64 * 1024 * 1024 });
  // agy exits 3 when `--print-timeout` cuts it; the spawn's own timeout is the backstop.
  if (ran.status === 3 || ran.signal) return { failed: `was cut at ${MINUTES} minutes` };
  if (ran.error) return { failed: `failed: ${ran.error.message}` };
  if (ran.status !== 0) return { failed: `failed: exit ${ran.status}${ran.stderr.trim() ? `: ${ran.stderr.trim().split("\n").at(-1).slice(0, 200)}` : ""}` };
  let result = null;
  try {
    result = JSON.parse(ran.stdout);
  } catch {}
  if (result?.status !== "SUCCESS" || !result.response) return { failed: `failed: status ${result?.status ?? "unreadable"}` };
  return { answer: result.response };
};

// The quota a candidate must have left, by window, to be picked (spec #592, "`routing.json`"): Gemini leaves room for
// the largest outside review seen, 39.5% of its 5-hour window. A CLI with no floors has no reader and is always eligible.
const FLOORS = { agy: { "5h": 0.5, weekly: 0.3 } };

const percent = (fraction) => `${Math.round(fraction * 100)}%`;

/** Why the candidate cannot be picked now, or null when it can. */
const shortOf = (candidate) => {
  const floors = FLOORS[candidate.cli];
  if (!floors) return null;
  const left = quotaLeft();
  const name = `${candidate.cli} ${candidate.model}`;
  const windows = Object.keys(floors);
  if (windows.some((window) => typeof left?.[window] !== "number")) return `${name} gave no quota reading`;
  if (windows.every((window) => left[window] >= floors[window])) return null;
  const has = windows.map((window) => `${percent(left[window])} of its ${window} window`).join(" and ");
  return `${name} has ${has} left, under its floors of ${windows.map((window) => percent(floors[window])).join(" and ")}`;
};

const named = (candidate) => [`critic: ${candidate.cli} ${candidate.model}`, candidate.effort && `effort ${candidate.effort}`].filter(Boolean).join(" ");

const pickCritic = ({ spec, routing }) => {
  let why = null;
  for (const candidate of JSON.parse(readFileSync(routing, "utf8")).critic) {
    const short = shortOf(candidate);
    if (short) {
      why = short;
      continue;
    }
    if (candidate.cli !== "agy") return [named(candidate), why && `why: ${why}`].filter(Boolean).join("\n");
    const { answer, failed } = runAgy(candidate.model, BRIEF + readFileSync(spec, "utf8"));
    if (answer) return `${named(candidate)}\n\n${answer.trimEnd()}`;
    why = `${candidate.cli} ${candidate.model} ${failed}; Sonnet is the fallback`;
  }
  throw new Error(`no critic candidate is eligible: ${why}`);
};

// The cheap worker's limits (spec #592, step 4): killed after this long with no output (a silent stop: OpenCode waits
// silently when the free limit is hit), or at this long in all; a retry (step 6) at its own, shorter limit. Tests shorten
// them through these variables, their only use.
const SILENCE_MS = Number(process.env.DELEGATE_SILENCE_MS) || 10 * 60_000;
const ATTEMPT_MS = Number(process.env.DELEGATE_ATTEMPT_MS) || 20 * 60_000;
const RETRY_MS = Number(process.env.DELEGATE_RETRY_MS) || 10 * 60_000;

// The fixed brief, the spike's (#599), which both free models followed.
const WORKER_BRIEF = `Read .delegate/ticket.md (a ticket and its parent spec) and AGENTS.md, then implement the ticket in this worktree. This brief overrides the Workflow section of AGENTS.md: do not commit, do not push, do not open a PR, do not run the outside review. Build test-first: run each new test red, an assertion failure, before writing its fix. Finish with \`npm run check\` green.`;

// The worker's environment (spec #592, step 3): an 8-minute command timeout so that `npm run check` fits, no push and
// no gh, and neither Daniel's global CLAUDE.md nor the external skills in its context.
const WORKER_ENV = {
  OPENCODE_EXPERIMENTAL_BASH_DEFAULT_TIMEOUT_MS: String(8 * 60_000),
  OPENCODE_PERMISSION: JSON.stringify({ bash: { "*": "allow", "git push*": "deny", "gh *": "deny" } }),
  OPENCODE_DISABLE_CLAUDE_CODE_PROMPT: "1",
  OPENCODE_DISABLE_EXTERNAL_SKILLS: "1",
};

/** The run failed before its worker could start; the orchestrator then runs the ticket as `medium`. */
class CouldNotStart extends Error {}

// No child gets PLOTKEEPER_INTERNAL, even when this script inherits it: the hook drops every event while it is set
// (hook/src/main.rs:45), so `npm run check`'s hook tests go red on correct work (#599's finding).
const childEnv = () => {
  const env = { ...process.env };
  delete env.PLOTKEEPER_INTERNAL;
  return env;
};

/** Runs a command to its end. `out` is a file its output goes to; without one, its output comes back. */
const exec = (cwd, command, args, { out } = {}) => {
  const fd = out && openSync(out, "w");
  try {
    const stdio = fd ? ["ignore", fd, fd] : ["ignore", "pipe", "pipe"];
    return spawnSync(command, args, { cwd, env: childEnv(), encoding: "utf8", stdio, maxBuffer: 64 * 1024 * 1024 });
  } finally {
    if (fd) closeSync(fd);
  }
};

/** Runs a command that must succeed, and returns its trimmed output. */
const must = (cwd, command, args, options) => {
  const ran = exec(cwd, command, args, options);
  if (ran.error || ran.status !== 0) {
    const why = ran.error?.message ?? (ran.stderr?.trim().split("\n").at(-1) || `exit ${ran.status}`);
    throw new CouldNotStart(`${command} ${args[0]} failed: ${why}`);
  }
  return ran.stdout?.trim() ?? "";
};

const git = (cwd, ...args) => must(cwd, "git", args);

/** Removes the worktree and its branch, whichever of them is there. */
const removeWorktree = (root, worktree, branch) => {
  exec(root, "git", ["worktree", "remove", "--force", worktree]);
  rmSync(worktree, { recursive: true, force: true });
  exec(root, "git", ["worktree", "prune"]);
  exec(root, "git", ["branch", "-D", branch]);
};

/** The ticket's title, and the ticket with its parent spec as the worker reads them. */
const fetchTicket = (cwd, number) => {
  const view = (n) => JSON.parse(must(cwd, "gh", ["issue", "view", n, "--json", "title,body"]));
  const issue = view(number);
  const parent = issue.body.match(/^## Parent\s+#(\d+)/m)?.[1];
  const parts = [`# ${issue.title}\n\n${issue.body.trim()}\n`];
  if (parent) {
    const spec = view(parent);
    parts.push(`# Parent spec: ${spec.title}\n\n${spec.body.trim()}\n`);
  }
  return { title: issue.title, text: parts.join("\n---\n\n") };
};

/** One line of `run.log` for one line of OpenCode's JSON events: its text and tool calls, nothing else. */
const render = (line) => {
  let event;
  try {
    event = JSON.parse(line);
  } catch {
    return line.trim() || null;
  }
  if (event.type === "text") return event.part?.text?.trim() || null;
  if (event.type === "tool_use") return `> ${event.part?.tool ?? "?"} ${JSON.stringify(event.part?.state?.input ?? {}).slice(0, 200)}`;
  if (event.type === "error") return `error: ${JSON.stringify(event.error ?? event.part ?? event).slice(0, 500)}`;
  return null;
};

/** The session id an OpenCode JSON event names at its top level, as the spike's `run.jsonl` samples do. */
const sessionOf = (line) => {
  try {
    return JSON.parse(line).sessionID || null;
  } catch {
    return null;
  }
};

/**
 * Runs the worker in the worktree with `message`, continuing `session` when one is given, until it exits or is killed
 * at `limit`; `killed` says why it was, `session` is the OpenCode session its events name. The command is the one the
 * spike verified (#599): `--session` goes with `--format json` and `-m`.
 */
const runWorker = (worktree, { candidate, effort, message, session = null, limit }) =>
  new Promise((resolve, reject) => {
    const variant = candidate.variant?.[effort] ?? null;
    const args = ["run", "--auto", "--format", "json", "-m", candidate.model, ...(session ? ["--session", session] : []), ...(variant ? ["--variant", variant] : []), message];
    const jsonl = openSync(join(worktree, ".delegate/run.jsonl"), "a");
    const stderr = openSync(join(worktree, ".delegate/stderr.txt"), "w");
    const log = join(worktree, ".delegate/run.log");
    // Its own process group, so that a kill also ends the commands it started, such as `npm run check`.
    const child = spawn("opencode", args, { cwd: worktree, env: { ...childEnv(), ...WORKER_ENV }, stdio: ["ignore", "pipe", stderr], detached: true });
    let killed = null;
    const kill = (why) => {
      killed = why;
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {}
    };
    let silence = setTimeout(() => kill("silent"), SILENCE_MS);
    const attempt = setTimeout(() => kill("timeout"), limit);
    // A spawn that fails emits `error` and then `close`: the files are closed once.
    let closed = false;
    const done = () => {
      if (closed) return;
      closed = true;
      clearTimeout(silence);
      clearTimeout(attempt);
      closeSync(jsonl);
      closeSync(stderr);
    };
    let partial = "";
    let named = null;
    child.stdout.on("data", (chunk) => {
      clearTimeout(silence);
      silence = setTimeout(() => kill("silent"), SILENCE_MS);
      appendFileSync(jsonl, chunk);
      const lines = (partial + chunk).split("\n");
      partial = lines.pop();
      named ??= lines.map(sessionOf).find(Boolean) ?? null;
      const shown = lines.map(render).filter(Boolean);
      if (shown.length) appendFileSync(log, `${shown.join("\n")}\n`);
    });
    child.on("error", (error) => {
      done();
      reject(new CouldNotStart(`opencode did not start: ${error.message}`));
    });
    child.on("close", (code, signal) => {
      done();
      resolve({ code, signal, killed, session: named });
    });
  });

/** In Herdr, a pane in the `workers` tab (made if missing) that tails the run's log; outside it, none. */
const openPane = (worktree) => {
  if (process.env.HERDR_ENV !== "1") return null;
  const herdr = (...args) => JSON.parse(must(worktree, "herdr", args)).result;
  const workspace = process.env.HERDR_WORKSPACE_ID ? ["--workspace", process.env.HERDR_WORKSPACE_ID] : [];
  let pane = null;
  try {
    const tab = herdr("tab", "list", ...workspace).tabs?.find((each) => each.label === "workers");
    const sibling = tab && herdr("pane", "list", ...workspace).panes?.find((each) => each.tab_id === tab.tab_id);
    pane = sibling
      ? herdr("pane", "split", sibling.pane_id, "--direction", "down", "--cwd", worktree, "--no-focus").pane.pane_id
      : herdr("tab", "create", ...workspace, "--label", "workers", "--cwd", worktree, "--no-focus").root_pane.pane_id;
    herdr("pane", "run", pane, "tail -f .delegate/run.log");
  } catch {
    // The pane only shows progress: the run goes on without it.
  }
  return pane;
};

const closePane = (worktree, pane) => {
  if (pane) exec(worktree, "herdr", ["pane", "close", pane]);
};

/** Stages the worker's change as one change on `base`, its own commits undone; false when there is none. */
const stage = (worktree, base) => {
  git(worktree, "reset", "--quiet", "--mixed", base);
  git(worktree, "add", "--all", "--", ".", ":(exclude).delegate");
  return exec(worktree, "git", ["diff", "--cached", "--quiet", base]).status !== 0;
};

/** Why a worker run that ended this way stops or escalates, or null when its work goes to the check. */
const failed = (exit) => {
  if (exit.pushed) return { result: "stop", stop: exit.pushed };
  if (exit.killed === "silent") return { result: "silent", escalate: "silent" };
  if (exit.killed === "timeout") return { result: "error", escalate: "timeout" };
  if (exit.code !== 0) return { result: "error", escalate: `exit ${exit.code ?? exit.signal}` };
  return null;
};

// How much of a red check's output the retry gets.
const TAIL_LINES = 60;

/** Checks the staged work: null when green, otherwise why it is red and what the retry is told. */
const check = (worktree, base) => {
  if (!stage(worktree, base)) return { escalate: "red: no change", tell: "No change was made: nothing differs from the integration branch. Implement the ticket in .delegate/ticket.md, then run `npm run check` until it is green. Do not commit or push." };
  const out = join(worktree, ".delegate/check.txt");
  if (exec(worktree, "npm", ["run", "check"], { out }).status === 0) return null;
  const tail = readFileSync(out, "utf8").trimEnd().split("\n").slice(-TAIL_LINES).join("\n");
  return { escalate: "red check", tell: `\`npm run check\` is red. The last lines of its output:\n\n${tail}\n\nFix the cause, then run \`npm run check\` until it is green. Do not commit or push.` };
};

/**
 * What the worker left on GitHub, or null (spec #592, step 7): an `origin` ref that holds one of its `commits`, the
 * ticket's branch on `origin`, or a PR from it. Only the worker's own traces count: others move refs during a run.
 */
const pushed = (worktree, branch, commits) => {
  // `--prune`, so that a tracking ref an earlier run left behind is not read as this worker's branch.
  git(worktree, "fetch", "--quiet", "--prune", "origin");
  for (const commit of commits) {
    const refs = git(worktree, "for-each-ref", "--contains", commit, "--format=%(refname:short)", "refs/remotes/origin");
    if (refs) return `${refs.split("\n")[0]} has the worker's commit ${commit.slice(0, 12)}`;
  }
  if (exec(worktree, "git", ["rev-parse", "--verify", "--quiet", `refs/remotes/origin/${branch}`]).status === 0) return `origin has the branch ${branch}`;
  const prs = JSON.parse(must(worktree, "gh", ["pr", "list", "--head", branch, "--json", "url"]) || "[]");
  if (prs.length) return `a PR from ${branch} is open: ${prs[0].url}`;
  return null;
};

/**
 * One candidate's attempt in the worktree: its first run, and on the first red check one retry on the same session.
 * `runs` holds each worker run's result and time; `escalate` is the verdict's reason, absent on pass.
 */
const attempt = async (worktree, { candidate, effort, base, integration, title, branch }) => {
  const runs = [];
  // Every worker run is followed by the push guard, whatever its end, before anything resets the worker's commits away.
  const timed = async (options) => {
    const started = Date.now();
    const exit = await runWorker(worktree, { candidate, effort, ...options });
    runs.push({ ms: Date.now() - started });
    const commits = git(worktree, "rev-list", `${base}..HEAD`).split("\n").filter(Boolean);
    const what = pushed(worktree, branch, commits);
    return what ? { ...exit, pushed: what } : exit;
  };
  const last = (verdict) => {
    runs.at(-1).result = verdict.result;
    return { runs, ...verdict };
  };
  try {
    let exit = await timed({ message: WORKER_BRIEF, limit: ATTEMPT_MS });
    let stop = failed(exit);
    if (stop) return last(stop);
    let red = check(worktree, base);
    if (red && exit.session) {
      runs.at(-1).result = "red";
      exit = await timed({ message: red.tell, session: exit.session, limit: RETRY_MS });
      stop = failed(exit);
      if (stop) return last(stop);
      red = check(worktree, base);
    }
    const stat = git(worktree, "diff", "--cached", "--stat", base) || undefined;
    if (red) return last({ result: "red", escalate: red.escalate, stat });
    git(worktree, "commit", "--quiet", "-m", `${title}\n\nWorker: ${candidate.model}`);
    git(worktree, "fetch", "--quiet", "origin");
    if (exec(worktree, "git", ["merge", "--quiet", "--no-edit", `origin/${integration}`]).status !== 0) {
      exec(worktree, "git", ["merge", "--abort"]);
      return last({ result: "conflict", escalate: "conflict", stat });
    }
    return last({ result: "pass", stat: git(worktree, "diff", "--stat", `origin/${integration}`, "HEAD") });
  } catch (error) {
    // The worker ran, so the run started: a failure now is an error attempt, and escalates.
    if (!(error instanceof CouldNotStart)) throw error;
    return last({ result: "error", escalate: `error: ${error.message}` });
  }
};

const minutes = (ms) => `${Math.max(1, Math.round(ms / 60_000))}m`;

/**
 * This run's segment of the `Route outcome:` line: its tier and effort, then each attempt's worker and the results of
 * its runs (a retry is a second run), and after `|` one time per run, attempts split by `;`.
 */
export const outcomeSegment = ({ tier, effort, attempts }) => {
  const results = attempts.map(({ worker, runs }) => `${worker} → ${runs.map((run) => run.result).join(",")}`).join(" → ");
  const times = attempts.map(({ runs }) => runs.map((run) => minutes(run.ms)).join(", ")).join("; ");
  return `${tier}·${effort} ${results} | ${times}`;
};

// The models a silent stop marked out, each until the next UTC midnight (spec #592, step 4), in a file in the git common
// dir, so that every run of every worktree of this checkout reads the same marks.
const MARKS = "delegate-silent.json";

const readMarks = (file) => {
  try {
    return JSON.parse(readFileSync(file, "utf8"));
  } catch {
    return {};
  }
};

const isOut = (file, model) => Date.parse(readMarks(file)[model] ?? "") > Date.now();

const markOut = (file, model) => {
  const midnight = new Date();
  midnight.setUTCHours(24, 0, 0, 0);
  const marks = Object.fromEntries(Object.entries(readMarks(file)).filter(([, until]) => Date.parse(until) > Date.now()));
  // Written whole and renamed into place, so that a run reading it at the same time never sees half a file.
  writeFileSync(`${file}.tmp`, `${JSON.stringify({ ...marks, [model]: midnight.toISOString() })}\n`);
  renameSync(`${file}.tmp`, file);
};

/**
 * Runs each candidate not marked out in turn, from `first`, until one ends other than silent; each attempt goes into
 * `attempts`. A silent stop marks its model out and starts the next candidate from the integration branch again.
 */
const candidates = async (worktree, { first, next, marks, attempts, base, integration, ...rest }) => {
  let candidate = first;
  let from = base;
  for (;;) {
    const verdict = await attempt(worktree, { candidate, base: from, integration, ...rest });
    attempts.push({ worker: candidate.model, runs: verdict.runs });
    if (verdict.result !== "silent") return verdict;
    markOut(marks, candidate.model);
    candidate = next();
    if (!candidate) return verdict;
    try {
      // `.delegate/` holds this run's files and stays.
      git(worktree, "reset", "--quiet", "--hard", `origin/${integration}`);
      git(worktree, "clean", "--quiet", "-fd", "-e", ".delegate");
      from = git(worktree, "rev-parse", "HEAD");
    } catch (error) {
      if (!(error instanceof CouldNotStart)) throw error;
      return { result: "error", escalate: `error: ${error.message}` };
    }
  }
};

const runTicket = async ({ ticket, integration, effort, routing }) => {
  const cheap = JSON.parse(readFileSync(routing, "utf8")).cheap ?? [];
  const branch = `issue-${ticket}-cheap`;
  let root;
  let worktree;
  let pane = null;
  const attempts = [];
  let verdict = { result: "silent", escalate: "silent" };
  try {
    const common = git(process.cwd(), "rev-parse", "--path-format=absolute", "--git-common-dir");
    const marks = join(common, MARKS);
    root = dirname(common);
    worktree = join(root, ".claude/worktrees", branch);
    removeWorktree(root, worktree, branch);
    const next = () => cheap.find((candidate) => !isOut(marks, candidate.model));
    const candidate = next();
    if (!candidate) {
      console.log("escalate medium (silent)");
      return 0;
    }
    git(root, "fetch", "--quiet", "origin");
    const base = git(root, "rev-parse", `origin/${integration}^{commit}`);
    git(root, "worktree", "add", "--quiet", "--no-track", "-b", branch, worktree, base);
    mkdirSync(join(worktree, ".delegate"));
    const { title, text } = fetchTicket(worktree, ticket);
    writeFileSync(join(worktree, ".delegate/ticket.md"), text);
    writeFileSync(join(worktree, ".delegate/brief.md"), `${WORKER_BRIEF}\n`);
    writeFileSync(join(worktree, ".delegate/run.log"), "");
    must(worktree, "npm", ["ci"], { out: join(worktree, ".delegate/npm-ci.txt") });
    pane = openPane(worktree);
    verdict = await candidates(worktree, { first: candidate, next, marks, attempts, effort, base, integration, title, branch });
  } catch (error) {
    if (!(error instanceof CouldNotStart)) throw error;
    console.error(`could not start: ${error.message}`);
    return 1;
  } finally {
    closePane(worktree, pane);
    // On pass the orchestrator removes it after the merge; on stop it is Daniel's evidence of what the worker did.
    if (worktree && !["pass", "stop"].includes(verdict.result) && existsSync(worktree)) removeWorktree(root, worktree, branch);
  }
  const segment = outcomeSegment({ tier: "cheap", effort, attempts });
  const said = verdict.stop ? `stop: ${verdict.stop}` : verdict.escalate ? `escalate medium (${verdict.escalate})` : "pass";
  console.log([said, verdict.stat, `Route outcome: ${segment}`].filter(Boolean).join("\n"));
  return 0;
};

// `import.meta.filename` is the real path; a path that goes through a symlink names the same file.
if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename) {
  try {
    const { command, flags } = parse(process.argv.slice(2));
    if (command === "critic") console.log(pickCritic(flags));
    else process.exitCode = await runTicket(flags);
  } catch (error) {
    if (!(error instanceof Usage)) throw error;
    console.error(`${error.message}\nusage: delegate pick critic --spec <draft> [--routing <file>]\n       delegate run --ticket <n> --integration <branch> --effort <medium|high> [--routing <file>]`);
    process.exit(2);
  }
}
