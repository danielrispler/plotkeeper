#!/usr/bin/env node
// `node scripts/agents/delegate.mjs pick critic --spec <draft> [--routing <file>]`: names the critic of a draft
// spec from `routing.json`'s `critic` list (spec #592, "The cold critic and the veto gate"). When it is Gemini, it runs
// it and prints its findings, which the orchestrator writes to `reports/critic.md`; when it is Claude, it prints the
// prompt that critic gets: the brief Gemini gets, then the draft. This script is where the critic's brief is written.
// `pick critic --breakdown <draft>` does the same for a draft ticket breakdown, its spec first (#722), with a brief that
// holds its routes to the routing rules.
// Gemini runs as the outside review runs it (#618): in a throwaway folder with a worktree of HEAD, where the deny hook
// allows only reads, answering in a schema. Plan mode ended the run at its first denied shell command.
//
// `node scripts/agents/delegate.mjs run --ticket <n> --integration <branch> --effort <medium|high> [--routing <file>]`:
// takes a cheap ticket through an OpenCode worker in its own worktree and prints the verdict (spec #592,
// "`delegate.mjs run`, a cheap ticket").
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { appendFileSync, closeSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, realpathSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { tmpdir } from "node:os";
import { agyCall, allowedReads, hookDecisions, quotaLeft, readOnly } from "../review/outside-review.mjs";

const MINUTES = 15;

// Each kind of draft's brief, and the name the draft goes by in the prompt.
const KINDS = {
  spec: {
    critic: `You are the critic of a draft spec for this repository. Check every claim of the draft against the repository. Mark each claim verified, with its file:line or a command and its output, or unverified. Flag contradictions with AGENTS.md, system-map.json and existing code. Rate each finding high, medium or low. You can change nothing: answer with your findings only.`,
    draft: "The draft",
  },
  breakdown: {
    critic: `You are the critic of a draft ticket breakdown for this repository: a spec, then the tickets it is cut into, each with its route, its blockers and its Touches Files. Hold the breakdown to the Routing section of docs/agents/issue-tracker.md and the "Repository-specific \`to-tickets\`" section of docs/agents/ticket-authoring.md, read in the repository. Report each route that breaks one of the five cheap rules or the effort rule; each Risk isolation cut that routes no ticket lower; each check that needs CI, which runs as its own non-draft PR before /implement-spec; each place the breakdown departs from the spec's plan; and each choice left open that is Daniel's to make. Mark each claim verified, with its file:line or a command and its output, or unverified. Rate each finding high, medium or low. You can change nothing: answer with your findings only.`,
    draft: "The breakdown, with its spec",
  },
};

/** The Claude critic's prompt: the brief, then the draft. */
const brief = (kind) => `${KINDS[kind].critic}

${KINDS[kind].draft}:

`;

// Gemini's prompt. Like the outside review's, it names files by absolute path, the only form its read tool takes, and
// holds no at-sign, so the draft is a file it reads rather than text in the prompt.
const geminiBrief = (folder, kind) => `${KINDS[kind].critic}

Your only working tool is read_file, and it takes absolute paths only. Do not try to write a file or run a command: those calls are denied, and so is a read outside ${folder}. Where you would run a command, read the files instead, and cite what you read as file:line.

- ${KINDS[kind].draft}: ${folder}/review/draft.md
- Every file of the repository, one absolute path a line: ${folder}/review/tree.txt
- The repository: ${folder}/repo

Give each finding its severity (high, medium or low), the claim it is about, its status (verified, unverified or contradiction), the evidence as file:line, and one sentence saying what is wrong.
`;

/** The findings' shape, enforced by `agy --json-schema`. */
const SCHEMA = {
  type: "object",
  properties: {
    findings: {
      type: "array",
      items: {
        type: "object",
        properties: {
          severity: { type: "string" },
          claim: { type: "string" },
          status: { type: "string" },
          evidence: { type: "string" },
          summary: { type: "string" },
        },
        required: ["severity", "claim", "status", "evidence", "summary"],
      },
    },
  },
  required: ["findings"],
};

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
    const flags = parseFlags(argv.slice(2), ["spec", "breakdown", "routing"], { routing: ROUTING });
    if (!flags.spec === !flags.breakdown) throw new Usage("one of --spec <draft> or --breakdown <draft> is required");
    const kind = flags.spec ? "spec" : "breakdown";
    return { command: "critic", flags: { kind, draft: flags[kind], routing: flags.routing } };
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

/** The verdict of a finished Gemini critic run: its findings as text, or why it gave none. */
const criticOutcome = ({ status, signal, error, stdout, stderr, reads }) => {
  // agy exits 3 when `--print-timeout` cuts it; the spawn's own timeout is the backstop.
  if (status === 3 || signal) return { failed: `was cut at ${MINUTES} minutes` };
  if (error) return { failed: `failed: ${error.message}` };
  if (status !== 0) return { failed: `failed: exit ${status}${stderr.trim() ? `: ${stderr.trim().split("\n").at(-1).slice(0, 200)}` : ""}` };
  let result = null;
  try {
    result = JSON.parse(stdout);
  } catch {}
  if (!result) return { failed: "failed: its answer is not JSON" };
  // The conversation id finds the run's transcript, under ~/.gemini/antigravity-cli/brain/<id>/.
  const conversation = `conversation ${result.conversation_id ?? "unknown"}`;
  if (result.status !== "SUCCESS") return { failed: `failed: status ${result.status} (${conversation})` };
  const findings = result.structured_output?.findings;
  if (!Array.isArray(findings)) {
    // agy may put its answer in a file and reply with a link to it, as it did in plan mode (#618).
    const pointer = result.response?.match(/\[([^\]]+)\]\(file:/)?.[1];
    if (pointer) return { failed: `answered only with a pointer to ${pointer} (${conversation})` };
    return { failed: `answered nothing (status SUCCESS, ${conversation})` };
  }
  if (!reads) return { failed: `read no file, so it saw no draft (${conversation})` };
  if (!findings.length) return { answer: "No findings." };
  return { answer: findings.map((f, i) => `${i + 1}. ${f.severity}, ${f.status}: ${f.claim}\n   Evidence: ${f.evidence}\n   ${f.summary}`).join("\n") };
};

/** One headless, read-only Gemini critic run of `draft`, in a throwaway folder holding a worktree of HEAD. */
const runAgy = (model, draft, kind) => {
  const git = (...args) => execFileSync("git", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  const folder = realpathSync(mkdtempSync(join(tmpdir(), "critic-")));
  const repo = join(folder, "repo");
  const log = join(folder, "hook.log");
  try {
    git("worktree", "add", "--quiet", "--detach", repo, "HEAD");
    mkdirSync(join(folder, "review"));
    writeFileSync(join(folder, "review/draft.md"), draft);
    const tree = git("-C", repo, "ls-tree", "-r", "--name-only", "HEAD").split("\n").filter(Boolean);
    writeFileSync(join(folder, "review/tree.txt"), tree.map((path) => join(repo, path)).join("\n") + "\n");
    writeFileSync(join(folder, "review/schema.json"), JSON.stringify(SCHEMA));
    readOnly(folder, log);
    const call = agyCall({ model }, { schema: join(folder, "review/schema.json"), prompt: geminiBrief(folder, kind), env: process.env });
    const ran = spawnSync("agy", call.args, { cwd: folder, env: call.env, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], timeout: (MINUTES + 1) * 60_000, maxBuffer: 64 * 1024 * 1024 });
    return criticOutcome({ ...ran, reads: allowedReads(hookDecisions(log)) });
  } catch (error) {
    return { failed: `failed: ${error.message.split("\n")[0]}` };
  } finally {
    try {
      git("worktree", "remove", "--force", repo);
    } catch {}
    rmSync(folder, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
    try {
      git("worktree", "prune");
    } catch {}
  }
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

const pickCritic = ({ kind, draft: path, routing }) => {
  let why = null;
  const draft = readFileSync(path, "utf8");
  const prompt = brief(kind) + draft;
  for (const candidate of JSON.parse(readFileSync(routing, "utf8")).critic) {
    const short = shortOf(candidate);
    if (short) {
      why = short;
      continue;
    }
    if (candidate.cli !== "agy") return `${[named(candidate), why && `why: ${why}`].filter(Boolean).join("\n")}\n\n${prompt.trimEnd()}`;
    const { answer, failed } = runAgy(candidate.model, draft, kind);
    if (answer) return `${named(candidate)}\n\n${answer.trimEnd()}`;
    why = `${candidate.cli} ${candidate.model} ${failed}`;
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
// (hook/src/main.rs:45), so `npm run check`'s hook tests go red on correct work (#599's finding). Each child's PWD names
// its own folder, not the one this script was launched from: `opencode run` without `--dir` works in
// `process.env.PWD ?? process.cwd()` (1.19.6), so in #589's run the worker edited the launching checkout (#655).
const childEnv = (cwd) => {
  const env = { ...process.env, PWD: cwd };
  delete env.PLOTKEEPER_INTERNAL;
  return env;
};

/** Runs a command to its end. `out` is a file its output goes to; without one, its output comes back. */
const exec = (cwd, command, args, { out } = {}) => {
  const fd = out && openSync(out, "w");
  try {
    const stdio = fd ? ["ignore", fd, fd] : ["ignore", "pipe", "pipe"];
    return spawnSync(command, args, { cwd, env: childEnv(cwd), encoding: "utf8", stdio, maxBuffer: 64 * 1024 * 1024 });
  } finally {
    if (fd) closeSync(fd);
  }
};

/** Runs a command that must succeed, and returns its trimmed output. */
const must = (cwd, command, args, settings) => {
  const ran = exec(cwd, command, args, settings);
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
    const child = spawn("opencode", args, { cwd: worktree, env: { ...childEnv(worktree), ...WORKER_ENV }, stdio: ["ignore", "pipe", stderr], detached: true });
    let killed = null;
    const kill = (why) => {
      killed = why;
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {}
    };
    let silence = setTimeout(() => kill("silent"), SILENCE_MS);
    const cutoff = setTimeout(() => kill("timeout"), limit);
    // A spawn that fails emits `error` and then `close`: the files are closed once.
    let closed = false;
    const done = () => {
      if (closed) return;
      closed = true;
      clearTimeout(silence);
      clearTimeout(cutoff);
      closeSync(jsonl);
      closeSync(stderr);
    };
    let partial = "";
    let sessionId = null;
    // On its exit the timers stop, and what it started is killed: a command it left running would hold its output
    // open, so that `close` never comes and the silence limit kills a worker that is done.
    let exited = false;
    child.on("exit", () => {
      exited = true;
      clearTimeout(silence);
      clearTimeout(cutoff);
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {}
    });
    child.stdout.on("data", (chunk) => {
      clearTimeout(silence);
      if (!exited) silence = setTimeout(() => kill("silent"), SILENCE_MS);
      appendFileSync(jsonl, chunk);
      const lines = (partial + chunk).split("\n");
      partial = lines.pop();
      sessionId ??= lines.map(sessionOf).find(Boolean) ?? null;
      const shown = lines.map(render).filter(Boolean);
      if (shown.length) appendFileSync(log, `${shown.join("\n")}\n`);
    });
    child.on("error", (error) => {
      done();
      reject(new CouldNotStart(`opencode did not start: ${error.message}`));
    });
    child.on("close", (code, signal) => {
      done();
      resolve({ code, signal, killed, session: sessionId });
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

/**
 * Stages the worker's change as one change, its own commits undone; false when there is none. It goes onto the last
 * integration commit the worker has, as the guard's fetch left `origin`: not the run's base, which would fold the
 * commits the worker pulled into its change, so that the merge of the integration tip could conflict on files the
 * worker never touched. When the worker pulled nothing, that commit is the base.
 */
const stage = (worktree, integration) => {
  const onto = git(worktree, "merge-base", "HEAD", `origin/${integration}`);
  git(worktree, "reset", "--quiet", "--mixed", onto);
  // Not an exclude pathspec: one naming the gitignored `.delegate/` makes `git add` exit 1. The unstage keeps it out
  // when the worker un-ignored it (#662).
  git(worktree, "add", "--all", "--", ".");
  git(worktree, "rm", "-r", "--cached", "--quiet", "--ignore-unmatch", "--", ".delegate");
  return exec(worktree, "git", ["diff", "--cached", "--quiet", "HEAD"]).status !== 0;
};

/** The checkout's `git status --porcelain`, one entry a line, its leading spaces kept. */
const statusOf = (checkout) => {
  const ran = exec(checkout, "git", ["status", "--porcelain"]);
  if (ran.error || ran.status !== 0) throw new CouldNotStart(`git status failed in ${checkout}`);
  return ran.stdout.split("\n").filter(Boolean);
};

/** The paths whose status in the checkout differs from `before`, joined; null when none does. */
const changedSince = (checkout, before) => {
  const after = statusOf(checkout);
  const changed = [...after.filter((entry) => !before.includes(entry)), ...before.filter((entry) => !after.includes(entry))];
  return changed.length ? [...new Set(changed.map((entry) => entry.slice(3)))].join(", ") : null;
};

/** Why a worker run that ended this way stops or escalates, or null when its work goes to the check. */
const failure = (exit) => {
  // Both guards' findings are said, so that the paths outside the worktree are named even when the worker also pushed.
  const stop = [exit.pushed, exit.escaped && `worker wrote outside its worktree (${exit.escaped})`].filter(Boolean).join("; ");
  if (stop) return { result: "stop", stop };
  if (exit.killed === "silent") return { result: "silent", escalate: "silent" };
  if (exit.killed === "timeout") return { result: "error", escalate: "timeout" };
  if (exit.code !== 0) return { result: "error", escalate: `exit ${exit.code ?? exit.signal}` };
  return null;
};

// How much of a red check's output the retry gets.
const TAIL_LINES = 60;

/** Checks the staged work: null when green, otherwise why it is red and what the retry is told. */
const check = (worktree, integration) => {
  if (!stage(worktree, integration)) return { escalate: "red: no change", tell: "No change was made: nothing differs from the integration branch. Implement the ticket in .delegate/ticket.md, then run `npm run check` until it is green. Do not commit or push." };
  const out = join(worktree, ".delegate/check.txt");
  if (exec(worktree, "npm", ["run", "check"], { out }).status === 0) return null;
  const tail = readFileSync(out, "utf8").trimEnd().split("\n").slice(-TAIL_LINES).join("\n");
  return { escalate: "red check", tell: `\`npm run check\` is red. The last lines of its output:\n\n${tail}\n\nFix the cause, then run \`npm run check\` until it is green. Do not commit or push.` };
};

// The reflog entries that only point HEAD at a commit that is already there, such as one the worker pulled.
const MOVES = /Fast-forward|^reset:|^checkout:|^switch:|\(start\)/;

/**
 * The commits the worker made since `base`: the worktree's own HEAD reflog, its moves left out. Not `base..HEAD`, which
 * also holds the commits of others that the worker pulled or merged; a commit it made and then reset away still counts.
 */
const madeBy = (worktree, base) => {
  const made = new Set();
  for (const line of git(worktree, "log", "--walk-reflogs", "--format=%H%x09%gs", "HEAD").split("\n").filter(Boolean)) {
    const [commit, subject] = line.split("\t");
    if (MOVES.test(subject) || exec(worktree, "git", ["merge-base", "--is-ancestor", commit, base]).status === 0) continue;
    made.add(commit);
  }
  return [...made];
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
const attempt = async (worktree, { candidate, effort, base, integration, title, branch, launch }) => {
  const runs = [];
  // Every worker run is followed by the push guard, whatever its end, before anything resets the worker's commits away,
  // and by the escape guard: a change to the checkout this script was launched from is the worker's, whatever its
  // cause (#655), and its attempt is not staged.
  const timed = async (given) => {
    const before = statusOf(launch);
    const started = Date.now();
    const exit = await runWorker(worktree, { candidate, effort, ...given });
    runs.push({ ms: Date.now() - started });
    const escaped = changedSince(launch, before);
    const what = pushed(worktree, branch, madeBy(worktree, base));
    return { ...exit, ...(what && { pushed: what }), ...(escaped && { escaped }) };
  };
  const last = (verdict) => {
    runs.at(-1).result = verdict.result;
    return { runs, ...verdict };
  };
  try {
    let exit = await timed({ message: WORKER_BRIEF, limit: ATTEMPT_MS });
    let stop = failure(exit);
    if (stop) return last(stop);
    let red = check(worktree, integration);
    if (red && exit.session) {
      runs.at(-1).result = "red";
      exit = await timed({ message: red.tell, session: exit.session, limit: RETRY_MS });
      stop = failure(exit);
      if (stop) return last(stop);
      red = check(worktree, integration);
    }
    const stat = git(worktree, "diff", "--cached", "--stat", "HEAD") || undefined;
    if (red) return last({ result: "red", escalate: red.escalate, stat });
    git(worktree, "commit", "--quiet", "-m", `${title}\n\nWorker: ${candidate.model}`);
    git(worktree, "fetch", "--quiet", "origin");
    if (exec(worktree, "git", ["merge", "--quiet", "--no-edit", `origin/${integration}`]).status !== 0) {
      exec(worktree, "git", ["merge", "--abort"]);
      return last({ result: "conflict", escalate: "conflict", stat });
    }
    return last({ result: "pass", stat: git(worktree, "diff", "--stat", `origin/${integration}`, "HEAD") });
  } catch (error) {
    // Once a worker ran, the run started: a failure then is an error attempt, and escalates. One that never ran, such
    // as an `opencode` that is missing, means the run could not start.
    if (!(error instanceof CouldNotStart) || !runs.length) throw error;
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
  // The temporary file is this process's own, so that two runs marking at once do not rename each other's.
  const temporary = `${file}.${process.pid}.tmp`;
  writeFileSync(temporary, `${JSON.stringify({ ...marks, [model]: midnight.toISOString() })}\n`);
  renameSync(temporary, file);
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
    const launch = git(process.cwd(), "rev-parse", "--show-toplevel");
    const marks = join(common, MARKS);
    root = dirname(common);
    worktree = join(root, ".claude/worktrees", branch);
    removeWorktree(root, worktree, branch);
    if (!cheap.length) {
      console.log("escalate medium (silent)");
      return 0;
    }
    // The free models withdrawn: OpenCode serves none of them (spec #592, "`delegate.mjs run`, a cheap ticket").
    const served = must(root, "opencode", ["models", "opencode"]).split("\n").map((line) => line.trim());
    if (!cheap.some((each) => served.includes(each.model))) throw new CouldNotStart(`opencode serves none of the cheap models ${cheap.map((each) => each.model).join(", ")}`);
    // A candidate OpenCode no longer serves is passed over, as one marked out is.
    const next = () => cheap.find((each) => served.includes(each.model) && !isOut(marks, each.model));
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
    verdict = await candidates(worktree, { first: candidate, next, marks, attempts, effort, base, integration, title, branch, launch });
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
    console.error(`${error.message}\nusage: delegate pick critic --spec <draft> | --breakdown <draft> [--routing <file>]\n       delegate run --ticket <n> --integration <branch> --effort <medium|high> [--routing <file>]`);
    process.exit(2);
  }
}
