// The outside review (#279, #284): Gemini, through `agy`, reads a risky diff before its PR is opened.
//
//   node scripts/review/outside-review.mjs [--force] [--model <id>] [--out <folder>]
//
// It reviews the commits of HEAD since the merge-base with `origin/main`, once: there is no second
// round. Its first line is the verdict: `reviewed: <n> findings`,
// `skipped: <reason>` or `review unavailable: <reason>`. It never fails a PR: whatever stops the
// review, it says why and exits 0; only a wrong option exits 2, before any review runs. The
// findings go to a JSON file in `--out`: the session's scratchpad, or the system's temp folder
// without it. Every run, whatever its verdict, also leaves one line in the main checkout's
// `local-evidence/outside-review/runs.jsonl`, where git ignores that folder (#302).
//
// The reviewer cannot write, run a command or read outside its throwaway folder: `deny-hook.mjs`
// denies every tool but a read inside that folder (#283). So the script hands it the diff and the
// file list as files, and checks afterwards that the reviewed tree is still clean.
import { execFileSync, spawn, spawnSync } from "node:child_process";
import { appendFileSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

export const DEFAULT_MODEL = "gemini-4-neon-medium";
/** The turn-writing job queue: each session's jobs, and what runs behind one that panics. */
export const JOB_QUEUE = "understanding/src/turn_writing.rs";
/** The files `CardStore::open` reads at start; one that fails to parse stops Studio's start. */
const STORED = /\b(cards-v2|briefs|live-facts|decisions|agent-summaries|summaries|retests)\.json\b/;
const CODE = /\.(rs|ts|mjs|js)$/;
export const SKIPPED = "skipped: no risk match";
const TIMEOUT_MINUTES = 15;

class Usage extends Error {}

export const options = (argv) => {
  const opts = { force: false, model: DEFAULT_MODEL, out: tmpdir() };
  for (let i = 0; i < argv.length; i++) {
    const name = argv[i].replace(/^--/, "");
    if (argv[i] === "--force") opts.force = true;
    else if (["--model", "--out"].includes(argv[i]) && argv[i + 1]) opts[name] = argv[++i];
    else throw new Usage(`unknown or incomplete option ${argv[i]}`);
  }
  return opts;
};

// Each hunk of a unified diff: its file after the change, the lines of that file it adds or removes
// at, and its text, context included. A deleted file has no lines after the change.
const hunks = (diff) => {
  const out = [];
  let path = null;
  let hunk = null;
  let old = 0;
  let neu = 0;
  let at = 0;
  for (const line of diff.split("\n")) {
    if (old > 0 || neu > 0) {
      hunk.text += line + "\n";
      if (line.startsWith("+")) (hunk.changed.push(at), at++, neu--);
      else if (line.startsWith("-")) (hunk.changed.push(at), old--);
      else if (!line.startsWith("\\")) (at++, old--, neu--);
      continue;
    }
    const header = /^@@ -\d+(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(line);
    if (header) {
      old = Number(header[1] ?? 1);
      neu = Number(header[3] ?? 1);
      at = Number(header[2]);
      hunk = { path, changed: [], text: "" };
      if (path) out.push(hunk);
    } else if (line.startsWith("+++ ")) path = line === "+++ /dev/null" ? null : line.slice("+++ b/".length).replace(/\t$/, "");
  }
  return out;
};

// A line of Rust with its strings, character literals and line comments blanked, so a brace inside
// one opens and closes nothing.
const bare = (line) => line.replace(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)'/g, "").replace(/\/\/.*$/, "");

// The structs and enums of a Rust file that derive Deserialize, each from its first attribute or doc
// line to its closing line. It reads the text, not the grammar: a type that implements Deserialize
// by hand or gets its derive from a macro, a type written on its attribute's own line, and a brace
// inside a block comment are not seen right.
const deserialized = (source) => {
  const lines = source.split("\n");
  const types = [];
  for (let i = 0; i < lines.length; i++) {
    const item = /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+(\w+)/.exec(bare(lines[i]));
    if (!item) continue;
    let start = i;
    while (start > 0) {
      const above = lines[start - 1].trim();
      if (above === "" || above.startsWith("//") || above.startsWith("#[")) start--;
      else if (above.endsWith(")]")) {
        // The last line of an attribute that spans lines: up to the line that opens it.
        let open = start - 1;
        while (open >= 0 && !lines[open].trim().startsWith("#[")) open--;
        if (open < 0) break;
        start = open;
      } else break;
    }
    while (lines[start].trim() === "") start++;
    const attributes = lines.slice(start, i).filter((l) => !l.trim().startsWith("//")).join("\n");
    const derives = [...attributes.matchAll(/derive\s*\(([^)]*)\)/g)];
    if (!derives.some((d) => /\bDeserialize/.test(d[1]))) continue;
    let end = i;
    let depth = 0;
    let inner = 0;
    let opened = false;
    scan: for (; end < lines.length; end++) {
      for (const c of bare(lines[end])) {
        if (c === "{") (depth++, (opened = true));
        else if (c === "}") depth--;
        else if (c === "(" || c === "[") inner++;
        else if (c === ")" || c === "]") inner--;
        // A unit or tuple struct ends at its `;`, one inside a field's `[u8; 4]` aside.
        if ((opened && depth === 0) || (!opened && inner === 0 && c === ";")) break scan;
      }
    }
    types.push({ name: item[1], start: start + 1, end: end + 1 });
  }
  return types;
};

// Why this diff gets a review, or that it gets none. `facts.fileAt(path)` is a file after the
// change; `facts.subIssues()` is how many sub-issues the branch's issue has, asked last.
export const decide = (opts, facts) => {
  if (opts.force) return { review: true, why: "--force" };
  const why = new Set();
  const types = new Map();
  for (const hunk of hunks(facts.diff)) {
    if (!CODE.test(hunk.path)) continue;
    // A type in a test file is a test's own helper: no stored file or wire holds it.
    if (hunk.path.endsWith(".rs") && !/(^|\/)tests\//.test(hunk.path)) {
      if (!types.has(hunk.path)) types.set(hunk.path, deserialized(facts.fileAt(hunk.path)));
      for (const type of types.get(hunk.path))
        if (hunk.changed.some((line) => type.start <= line && line <= type.end))
          why.add(`a type that derives Deserialize changed: ${type.name} in ${hunk.path}`);
    }
    const stored = STORED.exec(hunk.text);
    if (stored) why.add(`code naming ${stored[0]} changed: ${hunk.path}`);
    if (hunk.path === JOB_QUEUE) why.add(`the turn-writing job queue changed: ${JOB_QUEUE}`);
  }
  if (why.size === 0 && facts.subIssues() > 0) why.add("its issue is a spec");
  return why.size ? { review: true, why: [...why].join("; ") } : { review: false, line: SKIPPED };
};

/** The findings' shape, enforced by `agy --json-schema`. */
const SCHEMA = {
  type: "object",
  properties: {
    findings: {
      type: "array",
      items: {
        type: "object",
        properties: {
          file: { type: "string" },
          line: { type: "integer" },
          summary: { type: "string" },
          failure_scenario: { type: "string" },
        },
        required: ["file", "line", "summary", "failure_scenario"],
      },
    },
  },
  required: ["findings"],
};

// What the reviewer is asked. It names files by absolute path, the only form its read tool takes,
// and holds no at-sign: a file mentioned that way may be read with no tool call, past the hook.
export const brief = (folder) => `You are reviewing a code change for defects, before its pull request is opened.

You can change nothing. Your only working tool is read_file, and it takes absolute paths only. Do not try to write a file or run a command: those calls are denied, and so is a read outside ${folder}.

- The change, as a unified diff: ${folder}/review/diff.patch
- The changed files, one absolute path a line: ${folder}/review/changed.txt
- Every file of the repository, one absolute path a line: ${folder}/review/tree.txt
- The repository as it is after the change: ${folder}/repo

Read the whole diff first. Then read the changed files, and the code that calls them or that they call, until you can say how the change behaves.

Look for defects that reading the diff alone would miss:
- Stored data, both ways. Does the new code still read every file the previous code wrote? A field added to a type that derives Deserialize with no serde default, or a renamed field or variant, fails on an existing file. Can the previous code read what the new code writes? A new variant or status value fails there.
- Queued work. When a job crashes, panics or is skipped, what happens to the jobs queued behind it?
- State, ordering and concurrency across components: a lock held across a call that takes it again, events that arrive in another order, a value read before it is written.
- A test that cannot fail, or that asserts less than its name says.

Report only defects you can tie to a line the change adds or edits. Give each one the file's path relative to the repository, the line in the file after the change, one sentence saying what is wrong, and a concrete failure scenario: the input or state, and the wrong result. No style remarks and no praise. An empty list is a valid answer.
`;

/** How `agy` is run for the review: hidden from Studio, never recorded (`PLOTKEEPER_INTERNAL`), cut at 15 minutes, answering in SCHEMA. */
export const agyCall = (opts, { schema, prompt, env }) => {
  const { PLOTKEEPER_TERMINAL_ID: _, ...rest } = env;
  const args = ["--model", opts.model, "--print-timeout", `${TIMEOUT_MINUTES}m`];
  return { args: [...args, "--output-format", "json", "--json-schema", schema, "-p", prompt], env: { ...rest, PLOTKEEPER_INTERNAL: "1" } };
};

// The verdict of a finished `agy` run. `reads` is how many reads the hook allowed: with none, the
// reviewer saw no file (a broken hook denies every tool), so its empty answer is no review.
export const outcome = ({ status, missing, stdout, stderr, reads }) => {
  const unavailable = (reason) => ({ line: `review unavailable: ${reason}`, exit: 0 });
  if (missing) return unavailable("agy is not installed");
  if (status === 3) return unavailable(`the review was cut at ${TIMEOUT_MINUTES} minutes`);
  let result = null;
  try {
    result = JSON.parse(stdout);
  } catch {}
  const findings = result?.structured_output?.findings;
  if (status !== 0 || result?.status !== "SUCCESS" || !Array.isArray(findings)) {
    const said = `${stderr}\n${result?.status === "SUCCESS" ? "" : stdout}`;
    if (/quota|resource.exhausted|rate.limit/i.test(said)) return unavailable("the Gemini quota is used up");
    const tail = said.trim().split("\n").at(-1).slice(0, 200);
    return unavailable(`agy exited with status ${status}${tail ? `: ${tail}` : ""}`);
  }
  if (!reads) return unavailable("the reviewer could read no file; see the hook's log");
  return {
    line: `reviewed: ${findings.length} finding${findings.length === 1 ? "" : "s"}`,
    exit: 0,
    findings,
    tokens: result.usage?.total_tokens ?? null,
    seconds: result.duration_seconds ?? null,
  };
};

/** The Gemini quota left, by window, as fractions, from `agy -p /usage --output-format json`. */
export const quota = (stdout) => {
  try {
    const groups = JSON.parse(stdout).command.data.groups;
    const gemini = groups.find((g) => g.name === "Gemini Models");
    return Object.fromEntries(gemini.buckets.map((b) => [b.window, b.remaining_fraction]));
  } catch {
    return null;
  }
};

const safe = (f, otherwise) => {
  try {
    return f();
  } catch {
    return otherwise;
  }
};

// Runs `agy` without blocking, so a signal still reaches this script while the review runs.
// `started` gets the child; past `limit` milliseconds it is ended, as agy's own exit code 3.
const spawned = (args, more, limit, started) =>
  new Promise((done) => {
    const child = spawn("agy", args, { ...more, stdio: ["ignore", "pipe", "pipe"] });
    started(child);
    const out = { stdout: "", stderr: "" };
    let late = false;
    const timer = setTimeout(() => ((late = true), child.kill()), limit);
    child.stdout.on("data", (data) => (out.stdout += data));
    child.stderr.on("data", (data) => (out.stderr += data));
    child.on("error", (error) => (clearTimeout(timer), done({ ...out, status: null, missing: error.code === "ENOENT" })));
    child.on("close", (status) => (clearTimeout(timer), done({ ...out, status: late ? 3 : status })));
  });

const main = async (opts) => {
  const run = (command, args, more = {}) =>
    execFileSync(command, args, { encoding: "utf8", maxBuffer: 1 << 28, stdio: ["ignore", "pipe", "pipe"], ...more });
  const git = (...args) => run("git", args).trimEnd();
  const notes = [];
  if (safe(() => run("git", ["fetch", "--quiet", "origin", "main"], { timeout: 30_000 }), null) === null)
    notes.push("note: `git fetch` failed; the diff is against the origin/main already here");
  const head = git("rev-parse", "HEAD");
  const base = git("merge-base", "HEAD", "origin/main");
  const about = { branch: git("rev-parse", "--abbrev-ref", "HEAD"), head, base, model: opts.model, round: 1 };
  // One line per run in the main checkout's local-evidence/, and a reviewed run's findings beside
  // it. Only where git ignores that folder, so no other repository gets an untracked file; a record
  // that cannot be written changes nothing else.
  const record = (entry, findings) =>
    safe(() => {
      const main = dirname(git("rev-parse", "--path-format=absolute", "--git-common-dir"));
      git("-C", main, "check-ignore", "--quiet", "local-evidence/outside-review/runs.jsonl");
      const folder = join(main, "local-evidence/outside-review");
      mkdirSync(folder, { recursive: true });
      appendFileSync(join(folder, "runs.jsonl"), JSON.stringify({ at: new Date().toISOString(), ...about, ...entry }) + "\n");
      if (findings) writeFileSync(join(folder, findings.name), findings.text);
    });
  const skip = (line) => {
    record({ verdict: line });
    console.log([line, ...notes].join("\n"));
  };
  const diff = git("-c", "core.quotepath=false", "diff", "--no-color", "--no-ext-diff", "--src-prefix=a/", "--dst-prefix=b/", base, head);
  if (!diff) return skip(`skipped: no change between ${base.slice(0, 9)} and HEAD`);
  const before = git("status", "--porcelain");
  if (before) notes.push("note: uncommitted changes are not reviewed; the review reads HEAD");
  const subIssues = () => {
    const issue = /^issue-(\d+)-/.exec(git("rev-parse", "--abbrev-ref", "HEAD"))?.[1];
    if (!issue) return 0;
    const total = safe(() => run("gh", ["api", `repos/{owner}/{repo}/issues/${issue}`, "--jq", ".sub_issues_summary.total"]), null);
    if (total === null) notes.push(`note: could not ask GitHub whether #${issue} is a spec`);
    return Number(total ?? 0);
  };
  const decided = decide(opts, { diff, fileAt: (path) => git("show", `${head}:${path}`), subIssues });
  if (!decided.review) return skip(decided.line);
  // Before any quota is spent: the findings must have somewhere to go.
  if (safe(() => mkdirSync(opts.out, { recursive: true }), null) === null) throw new Usage(`--out ${opts.out} cannot be made`);

  const env = agyCall(opts, { env: process.env }).env;
  const usage = { env, encoding: "utf8", cwd: tmpdir(), timeout: 30_000 };
  const left = () => quota(spawnSync("agy", ["-p", "/usage", "--output-format", "json"], usage).stdout);
  const folder = realpathSync(mkdtempSync(join(tmpdir(), "outside-review-")));
  const repo = join(folder, "repo");
  const log = join(folder, "hook.log");
  let child = null;
  const cleanup = () => {
    safe(() => git("worktree", "remove", "--force", repo));
    // An agy that is still ending may write its hook's log meanwhile: try again, and prune either way.
    safe(() => rmSync(folder, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 }));
    safe(() => git("worktree", "prune"));
  };
  // Killed mid-review (Ctrl-C, a stopped background task): end agy and leave no worktree behind.
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"])
    process.once(signal, () => {
      child?.kill();
      cleanup();
      process.exit(1);
    });
  const lines = [];
  let verdict = null;
  let cost = {};
  let kept = null;
  try {
    git("worktree", "add", "--quiet", "--detach", repo, head);
    const absolute = (paths) => paths.split("\n").filter(Boolean).map((p) => join(repo, p)).join("\n") + "\n";
    mkdirSync(join(folder, "review"));
    writeFileSync(join(folder, "review/diff.patch"), diff + "\n");
    writeFileSync(join(folder, "review/changed.txt"), absolute(git("diff", "--name-only", "--diff-filter=d", base, head)));
    writeFileSync(join(folder, "review/tree.txt"), absolute(git("ls-tree", "-r", "--name-only", head)));
    writeFileSync(join(folder, "review/schema.json"), JSON.stringify(SCHEMA));
    const hook = join(import.meta.dirname, "deny-hook.mjs");
    const command = [process.execPath, hook, folder, log].map((part) => `'${part}'`).join(" ");
    mkdirSync(join(folder, ".agents"));
    writeFileSync(
      join(folder, ".agents/hooks.json"),
      JSON.stringify({ readonly: { PreToolUse: [{ matcher: "*", hooks: [{ type: "command", command, timeout: 10 }] }] } }),
    );
    const quotaBefore = left();
    const call = agyCall(opts, { schema: join(folder, "review/schema.json"), prompt: brief(folder), env: process.env });
    const ran = await spawned(call.args, { cwd: folder, env: call.env }, (TIMEOUT_MINUTES + 2) * 60_000, (c) => (child = c));
    child = null;
    const logged = safe(() => readFileSync(log, "utf8"), "").split("\n");
    const decisions = logged.flatMap((line) => safe(() => [JSON.parse(line)], []));
    const result = outcome({ ...ran, reads: decisions.filter((d) => d.decision === "allow" && d.tool === "read_file").length });
    const dirty = safe(() => git("-C", repo, "status", "--porcelain"), "");
    if (dirty) notes.push(`warning: the review changed its throwaway tree, so the deny hook let a write through:\n${dirty}`);
    if (safe(() => git("status", "--porcelain"), before) !== before) notes.push("warning: the reviewed checkout changed during the review");
    const name = `outside-review-${head.slice(0, 9)}`;
    verdict = result.line;
    if (result.findings) {
      const quotaAfter = left();
      const denied = decisions.filter((d) => d.decision === "deny").map(({ tool, path, command }) => ({ tool, path, command }));
      const { findings, tokens, seconds } = result;
      const report = { model: opts.model, base, head, why: decided.why, findings, tokens, seconds, denied, quotaBefore, quotaAfter, notes };
      kept = { name: `${name}.json`, text: JSON.stringify(report, null, 2) + "\n" };
      cost = { findings: findings.length, tokens, seconds, quotaBefore, quotaAfter };
      writeFileSync(join(opts.out, kept.name), kept.text);
      const reasons = decided.why.split("; ");
      const more = reasons.length > 2 ? `; and ${reasons.length - 2} more reasons` : "";
      const percent = (fraction) => (typeof fraction === "number" ? `${Math.round(fraction * 100)}%` : "?");
      const windows = Object.keys(quotaAfter ?? quotaBefore ?? {});
      const quotas = windows.map((w) => `${w} ${percent(quotaBefore?.[w])} to ${percent(quotaAfter?.[w])}`).join(", ");
      lines.push(`${result.line} (${opts.model}; ${reasons.slice(0, 2).join("; ")}${more})`);
      lines.push(`findings: ${join(opts.out, `${name}.json`)}`);
      lines.push(`cost: ${((seconds ?? 0) / 60).toFixed(1)} min, ${tokens ?? "?"} tokens; Gemini quota left: ${quotas || "unknown"}`);
    } else {
      lines.push(result.line);
      if (!ran.missing) {
        // What `agy` said, kept for whoever asks why: the throwaway folder is about to go.
        const kept = join(opts.out, `${name}-unavailable.json`);
        writeFileSync(kept, JSON.stringify({ ...ran, decisions }, null, 2));
        notes.push(`note: agy's output is in ${kept}`);
      }
    }
  } catch (error) {
    verdict = `review unavailable: ${String(error.message ?? error).split("\n")[0]}`;
    lines.push(verdict);
  } finally {
    cleanup();
  }
  record({ verdict, why: decided.why, ...cost }, kept);
  console.log([...lines, ...notes].join("\n"));
};

// `import.meta.filename` is the real path; a path that goes through a symlink names the same file.
if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename) {
  try {
    await main(options(process.argv.slice(2)));
  } catch (error) {
    if (error instanceof Usage) {
      console.error(`${error.message}\nusage: outside-review [--force] [--model <id>] [--out <folder>]`);
      process.exit(2);
    }
    // Whatever else stops the review, the PR goes ahead with the reason.
    console.log(`review unavailable: ${String(error.message ?? error).split("\n")[0]}`);
  }
}
