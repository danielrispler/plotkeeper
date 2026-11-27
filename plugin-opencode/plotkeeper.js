// plotkeeper's plugin for OpenCode 2 (#370): appends OpenCode's events to plotkeeper's event log through
// `plotkeeper-hook --agent opencode`, one line an event, as the other harnesses' hook entries do. `plotkeeper install`
// renders it into `~/.config/opencode/plugins/plotkeeper.js`, a folder OpenCode loads every `.js` file of.
//
// It is a V2 plugin (https://opencode.ai/docs/build/plugins/server, read at anomalyco/opencode v2.4.2): a default
// export with an `id` and a `setup`. OpenCode loads it once in its background service for each folder a session runs
// in. Its hooks (`session` `prompt`, `tool` `execute.before` and `execute.after`) see only that folder's sessions, but
// its event stream is the whole service's, so it records the events of the sessions its own hooks saw and no other.
// A subagent is a child session of its own: its prompt and turns are not recorded, and its tool calls and permission
// asks go under the session it was spawned from. A background subagent's result starts its parent's next turn with no
// prompt, which is sent as `session.execution.started`, a wake-up.

import { spawn } from "node:child_process";

const HOOK = "__PLOTKEEPER_BIN__/plotkeeper-hook";

// The service keeps the environment of the terminal that started it, not that of the session's.
const ENV = { ...process.env };
delete ENV.PLOTKEEPER_AGENT_PID;

const TURN_ENDS = ["session.execution.succeeded", "session.execution.failed", "session.execution.interrupted"];

/** Appends `line` to the event log; a hook that cannot run never holds OpenCode up. */
function send(line) {
  try {
    const hook = spawn(HOOK, ["--agent", "opencode"], { stdio: ["pipe", "ignore", "ignore"], env: ENV });
    hook.on("error", () => {});
    hook.stdin.on("error", () => {});
    hook.stdin.end(JSON.stringify(line));
  } catch {
    // The same: no hook, no line.
  }
}

/** What one plugin instance knows of the sessions it records, and how it writes their lines. */
function recorder(ctx) {
  const cwd = ctx.location.directory;
  // The top-level session of each session this instance's hooks saw, by id.
  const roots = new Map();
  const root = (sessionID) => {
    if (!roots.has(sessionID)) {
      const found = ctx.session.get({ sessionID }).then(
        (session) => (session?.parentID ? root(session.parentID) : sessionID),
        () => sessionID,
      );
      roots.set(sessionID, found);
    }
    return roots.get(sessionID);
  };
  return {
    seen: (sessionID) => roots.has(sessionID),
    root,
    // The model and the last reply text of each top-level session's turn, for its end.
    turns: new Map(),
    // The sessions prompted since their last turn started.
    prompted: new Set(),
    // Each running call's tool and input, by call id, for the permission it asks.
    calls: new Map(),
    // A permission ask whose call has not started yet, by its call id.
    asks: new Map(),
    line: (name, session, fields) => ({ hook_event_name: name, session_id: session, cwd, ...fields }),
  };
}

/** Each prompt to a top-level session starts its turn. */
function recordPrompts(ctx, r) {
  ctx.session.hook("prompt", async (event) => {
    r.prompted.add(event.sessionID);
    const top = await r.root(event.sessionID);
    if (top !== event.sessionID) return;
    r.turns.set(top, { model: r.turns.get(top)?.model, reply: "" });
    send(r.line("session.prompt", top, { prompt_id: event.messageID, prompt: event.prompt?.text ?? "" }));
  });
}

/** A permission ask's line, with its call's tool and input, by which the studio finds the call it waits on. */
const askLine = (r, top, data, call) =>
  r.line("permission.asked", top, { tool_name: call.tool, tool_use_id: data.source?.id, tool_input: call.input, permission: data });

/** Each tool call's start and end, a subagent's under the session it was spawned from. */
function recordTools(ctx, r) {
  const tool = async (name, event, fields) => {
    const top = await r.root(event.sessionID);
    const subagent = top === event.sessionID ? {} : { subagent_session_id: event.sessionID };
    send(r.line(name, top, { tool_name: event.tool, tool_use_id: event.id, tool_input: event.input ?? {}, ...subagent, ...fields }));
  };
  ctx.tool.hook("execute.before", async (event) => {
    r.calls.set(event.id, { tool: event.tool, input: event.input ?? {} });
    await tool("tool.execute.before", event, {});
    const ask = r.asks.get(event.id);
    r.asks.delete(event.id);
    if (ask) send(ask(r.calls.get(event.id)));
  });
  ctx.tool.hook("execute.after", (event) => {
    r.calls.delete(event.id);
    return tool("tool.execute.after", event, {
      status: event.status,
      error: event.error?.message ?? null,
      exit: event.result?.metadata?.exit ?? null,
    });
  });
}

/** The event stream's permission asks and turn starts and ends, of the sessions this instance's hooks saw. */
async function recordEvents(ctx, r, signal) {
  const types = ["session.execution.started", "session.step.started", "session.text.ended", "permission.asked", ...TURN_ENDS];
  for await (const event of ctx.event.subscribe({ types, signal })) {
    const data = event.data ?? {};
    if (!r.seen(data.sessionID)) continue;
    const top = await r.root(data.sessionID);
    if (event.type === "permission.asked") {
      // An ask can come before its call's start; it is sent once the call starts.
      const id = data.source?.type === "tool" ? data.source.id : undefined;
      const call = r.calls.get(id);
      if (id && !call) r.asks.set(id, (started) => askLine(r, top, data, started));
      else send(askLine(r, top, data, call ?? {}));
      continue;
    }
    if (top !== data.sessionID) continue;
    const turn = r.turns.get(top) ?? { reply: "" };
    r.turns.set(top, turn);
    if (event.type === "session.execution.started") {
      if (!r.prompted.delete(top)) {
        turn.reply = "";
        send(r.line(event.type, top, {}));
      }
    } else if (event.type === "session.step.started") turn.model = data.model?.id;
    else if (event.type === "session.text.ended") turn.reply = data.text ?? "";
    else send(r.line(event.type, top, { model: turn.model ?? null, last_assistant_message: turn.reply }));
  }
}

export default {
  id: "plotkeeper",
  setup(ctx) {
    const r = recorder(ctx);
    recordPrompts(ctx, r);
    recordTools(ctx, r);
    const controller = new AbortController();
    recordEvents(ctx, r, controller.signal).catch(() => {});
    return () => controller.abort();
  },
};
