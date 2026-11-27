// plotkeeper's plugin for Cline (#371): appends Cline's events to plotkeeper's event log through
// `plotkeeper-hook --agent cline`, one line an event, as the other harnesses' hook entries do. `plotkeeper install`
// renders it with its `package.json` into the data folder and installs it with `cline plugin install`.
//
// It is a Cline SDK plugin (read in Cline CLI 3.0.76's `@cline/shared` 0.2.2 types, `extensions/contribution-registry`
// and `agent`): a default export with a `name`, a `manifest` and runtime `hooks`. Cline loads it for each session, and
// its `setup` names the session; in the CLI it runs inside `cline`'s own process, so the hook records `cline`'s pid
// (#371's real runs). Cline's hook files are not used: their prompt hook
// never sees the user's prompt, which Cline adds to the conversation before the run's hooks can see it (#371), while
// a plugin's `beforeRun` gets the run's messages, the prompt last.

import { spawn } from "node:child_process";

const HOOK = "__PLOTKEEPER_BIN__/plotkeeper-hook";

/** Runs the hook with `line`; resolves when it has exited, or could not start, which never fails. */
function append(line) {
  return new Promise((done) => {
    try {
      const hook = spawn(HOOK, ["--agent", "cline"], { stdio: ["pipe", "ignore", "ignore"] });
      hook.on("error", () => done());
      hook.on("close", () => done());
      hook.stdin.on("error", () => {});
      hook.stdin.end(JSON.stringify(line));
    } catch {
      // The same: no hook, no line.
      done();
    }
  });
}

// The lines go one at a time, in the order Cline calls the hooks: hooks that each start their own process can append
// out of order, as a run's end did before its last call's end in a real run.
let queue = Promise.resolve();

/** Appends `line` to the event log after the lines before it, without making Cline wait: Cline gives a plugin's hook 3
 * seconds, and stops calling a plugin whose hooks fail 5 times in a row (Cline CLI 3.0.76, `@cline/core`'s plugin
 * registry), while the hook may wait 5 for the log's lock. */
function send(line) {
  queue = queue.then(() => append(line));
}

/** Waits for the lines sent so far, up to 2.5 seconds, so a one-shot `cline` writes its run's last line before it
 * exits. */
const flushed = () => Promise.race([queue, new Promise((done) => setTimeout(done, 2500).unref?.())]);

/** The text of message `message`, its text parts joined. */
const text = (message) =>
  (message?.content ?? [])
    .filter((part) => part.type === "text" && typeof part.text === "string")
    .map((part) => part.text)
    .join("");

/** How each part of a call went, for a tool that does several things in one call (`run_commands`' commands,
 * `read_files`' files): each entry's `query`, `success` and `error`, without its `result`, which can be a whole file. */
const outcomes = (output) =>
  Array.isArray(output)
    ? output.map((entry) => ({ query: entry?.query ?? null, success: entry?.success !== false, error: entry?.error ?? null }))
    : null;

// The session this sandbox runs the plugin for, and its folder, from `setup`.
let session = null;
let cwd = "";

/** A line of event `name` from `snapshot`, the run's state, with `fields`. */
const line = (name, snapshot, fields) => ({
  hook_event_name: name,
  session_id: session ?? snapshot.conversationId ?? null,
  cwd,
  // The run, which is the turn.
  run_id: snapshot.runId ?? null,
  agent_id: snapshot.agentId ?? null,
  parent_agent_id: snapshot.parentAgentId ?? null,
  ...fields,
});

export default {
  name: "plotkeeper",
  manifest: { capabilities: ["hooks"] },
  setup(_api, ctx) {
    session = ctx?.session?.sessionId ?? null;
    cwd = ctx?.workspaceInfo?.rootPath ?? ctx?.cwd ?? "";
  },
  hooks: {
    // A run is a turn: the prompt is the run's last message.
    beforeRun({ snapshot }) {
      const last = snapshot.messages?.at(-1);
      send(line("beforeRun", snapshot, { prompt: last?.role === "user" ? text(last) : "" }));
    },
    beforeTool({ snapshot, toolCall, input }) {
      send(line("beforeTool", snapshot, { tool_name: toolCall.toolName, tool_use_id: toolCall.toolCallId, tool_input: input ?? {} }));
    },
    afterTool({ snapshot, toolCall, input, result }) {
      send(
        line("afterTool", snapshot, {
          tool_name: toolCall.toolName,
          tool_use_id: toolCall.toolCallId,
          tool_input: input ?? {},
          is_error: result?.isError === true,
          outcomes: outcomes(result?.output),
        }),
      );
    },
    // `completed`, `aborted` or `failed`, with the run's final text and the text of its last message that has any: a
    // run that ends with `submit_and_exit` has Cline's `Submission recorded (…)` line as its final text, and the answer
    // it showed in that message (#372).
    async afterRun({ snapshot, result }) {
      // This run's messages: those after its prompt, the last `user` one (a tool's result is a `tool` message).
      const messages = result?.messages ?? [];
      const prompt = messages.findLastIndex((m) => m?.role === "user");
      const said = messages.slice(prompt + 1).filter((m) => m?.role === "assistant").map(text).filter(Boolean);
      send(
        line("afterRun", snapshot, {
          status: result?.status ?? "",
          last_assistant_message: result?.outputText ?? "",
          last_text: said.at(-1) ?? "",
          error: result?.error?.message ?? null,
        }),
      );
      await flushed();
    },
  },
};
