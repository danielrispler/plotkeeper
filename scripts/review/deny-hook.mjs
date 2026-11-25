// PreToolUse hook for a read-only agy run: default deny.
// Allows only read_file on a path whose real path is inside the root given as argv[2], and
// `finish`, the tool through which `agy --json-schema` hands over its answer: it touches no file.
// Logs each decision (never file contents) to argv[3].
import { readFileSync, appendFileSync, realpathSync } from "node:fs";
import { resolve, sep } from "node:path";

const [root, log] = process.argv.slice(2);
let verdict = { decision: "deny", reason: "read-only run: this tool is not allowed" };
let seen = {};
try {
  const event = JSON.parse(readFileSync(0, "utf8"));
  const call = event.toolCall ?? {};
  const args = call.args ?? {};
  seen = { tool: call.name, path: args.Path, command: args.CommandLine };
  if (call.name === "finish") verdict = { decision: "allow" };
  else if (call.name === "read_file" && typeof args.Path === "string") {
    const base = realpathSync(root);
    // realpath follows symlinks, so a link inside the root that points outside is refused.
    const target = realpathSync(resolve(event.cwd ?? base, args.Path));
    if (target === base || target.startsWith(base + sep)) verdict = { decision: "allow" };
    else verdict.reason = "read-only run: reads outside the workspace are not allowed";
  }
} catch (e) {
  seen.error = String(e.message ?? e);
}
try {
  appendFileSync(log, JSON.stringify({ ...seen, decision: verdict.decision }) + "\n");
} catch {}
process.stdout.write(JSON.stringify(verdict));
