// PreToolUse hook for a read-only agy run: default deny.
// Allows only read_file on a path whose real path is inside the root given as argv[2], and
// `finish`, the tool through which `agy --json-schema` hands over its answer: it touches no file.
// With `--subagents` after them it also allows `invoke_subagent` and `send_message`, for the review
// trial (#502): agy runs this hook inside each subagent too, so a subagent can only read as well, and
// a subagent hands its report back only through `send_message` (agy's subagent prompt: its answer
// "is NOT automatically relayed"), which touches no file.
// Logs each decision (never file contents) to argv[3], with the calling agent's conversation id,
// so a run can tell its subagents' calls from its own.
import { readFileSync, appendFileSync, realpathSync } from "node:fs";
import { resolve, sep } from "node:path";

const [root, log, ...flags] = process.argv.slice(2);
const subagents = flags.includes("--subagents");
let verdict = { decision: "deny", reason: "read-only run: this tool is not allowed" };
let seen = {};
try {
  const event = JSON.parse(readFileSync(0, "utf8"));
  const call = event.toolCall ?? {};
  const args = call.args ?? {};
  seen = { tool: call.name, path: args.Path, command: args.CommandLine, conversation: event.conversationId };
  if (call.name === "finish" || (subagents && (call.name === "invoke_subagent" || call.name === "send_message"))) verdict = { decision: "allow" };
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
