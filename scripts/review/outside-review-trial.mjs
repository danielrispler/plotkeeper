// The review trial (#502): three outside reviews of the same diff, side by side. Only Daniel starts
// it, by hand, on a branch he picks; agents never do (`AGENTS.md`, "Finish with a PR").
//
//   node scripts/review/outside-review-trial.mjs [--out <folder>]
//
// It runs `outside-review.mjs --force` three times at once, each with its own throwaway folder and
// worktree, and prints the Gemini quota left before it starts, then each variant's usual verdict,
// findings file and cost lines, then the trial's quota cost. The three runs overlap, so each run's
// own quota before and after counts the others' use: only the trial's whole cost is its own.
import { execFileSync, spawn } from "node:child_process";
import { realpathSync } from "node:fs";
import { join } from "node:path";
import { quotaLeft } from "./outside-review.mjs";

const VARIANTS = [
  { name: "medium", args: [] },
  { name: "high", args: ["--model", "gemini-4-neon-high"] },
  { name: "subagents", args: ["--subagents"] },
];

const percent = (fraction) => (typeof fraction === "number" ? `${Math.round(fraction * 100)}%` : "?");
const windows = (left) => (left ? Object.entries(left).map(([w, f]) => `${w} ${percent(f)}`).join(", ") : "unknown");

const main = async (argv) => {
  if (argv.length && !(argv.length === 2 && argv[0] === "--out")) {
    console.error("usage: outside-review-trial [--out <folder>]");
    process.exit(2);
  }
  // Fetched once here, so the three runs' own fetches find nothing new and all review one base.
  try {
    execFileSync("git", ["fetch", "--quiet", "origin", "main"], { stdio: "ignore", timeout: 30_000 });
  } catch {}
  const before = quotaLeft();
  console.log(`Gemini quota left: ${windows(before)}`);
  const trial = `trial-${new Date().toISOString().replace(/[:.]/g, "-")}`;
  const script = join(import.meta.dirname, "outside-review.mjs");
  const children = [];
  // Each signal is passed on to the runs, which may also get it from the process group (Ctrl-C):
  // a run cleans up once, whichever comes first, and ends its agy and removes its own worktree.
  for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"])
    process.once(signal, () => {
      for (const child of children) child.kill(signal);
    });
  const outputs = await Promise.all(
    VARIANTS.map(
      ({ name, args }) =>
        new Promise((done) => {
          const child = spawn(process.execPath, [script, "--force", ...argv, "--variant", name, "--trial", trial, ...args], {
            stdio: ["ignore", "pipe", "inherit"],
          });
          children.push(child);
          let out = "";
          child.stdout.on("data", (data) => (out += data));
          child.on("close", () => done(`== ${name} ==\n${out.trimEnd()}`));
        }),
    ),
  );
  console.log(outputs.join("\n"));
  const after = quotaLeft();
  const spent = Object.keys(before ?? {})
    .filter((w) => typeof before[w] === "number" && typeof after?.[w] === "number")
    .map((w) => `${w} ${((before[w] - after[w]) * 100).toFixed(1)} points`);
  console.log(`trial ${trial} quota cost: ${spent.join(", ") || "unknown"}; Gemini quota left: ${windows(after)}`);
};

if (process.argv[1] && realpathSync(process.argv[1]) === import.meta.filename) await main(process.argv.slice(2));
