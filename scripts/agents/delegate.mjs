#!/usr/bin/env node
// `node scripts/agents/delegate.mjs pick critic --spec <draft> [--routing <file>]`: names the critic of a draft
// spec from `routing.json`'s `critic` list (spec #592, "The cold critic and the veto gate"), and runs it when it
// is Gemini. The orchestrator writes what it prints to `reports/critic.md`.
import { spawnSync } from "node:child_process";
import { readFileSync, realpathSync } from "node:fs";
import { join } from "node:path";
import { agyCall, options, quotaLeft } from "../review/outside-review.mjs";

const MINUTES = 15;

// The critic's brief, the one `docs/agents/learning.md` ("Critic and veto gate") gives.
const BRIEF = `You are the critic of a draft spec for this repository. Check every claim of the draft against the repository. Mark each claim verified, with its file:line or a command and its output, or unverified. Flag contradictions with AGENTS.md, system-map.json and existing code. Rate each finding high, medium or low. You can change nothing: answer with your findings only.

The draft:

`;

class Usage extends Error {}

const parse = (argv) => {
  const [verb, role, ...rest] = argv;
  if (verb !== "pick" || role !== "critic") throw new Usage(`unknown command ${[verb, role].filter(Boolean).join(" ")}`);
  const flags = { routing: join(import.meta.dirname, "routing.json") };
  for (let i = 0; i < rest.length; i += 2) {
    const name = rest[i]?.replace(/^--/, "");
    if (!["spec", "routing"].includes(name) || rest[i + 1] === undefined) throw new Usage(`unknown or incomplete option ${rest[i]}`);
    flags[name] = rest[i + 1];
  }
  if (!flags.spec) throw new Usage("--spec <draft> is required");
  return flags;
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

// `import.meta.filename` is the real path; a path that goes through a symlink names the same file.
if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename) {
  try {
    console.log(pickCritic(parse(process.argv.slice(2))));
  } catch (error) {
    if (!(error instanceof Usage)) throw error;
    console.error(`${error.message}\nusage: delegate pick critic --spec <draft> [--routing <file>]`);
    process.exit(2);
  }
}
