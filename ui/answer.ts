// Answering from the card (danielrispler/Falinks#461, decision 16). While a session's view shows and the window is
// visible, the shell watches it, so the answer hook waits for the card; the shell sends the session's
// open request in its read loop (danielrispler/Falinks#569), and the answer echoes its nonce. The hook waits only while the
// request's card shows: a question asked in a view that is not shown is handed to the terminal.

import { invoke, listen } from "./dom.js";
import { studio } from "./state.js";
import { askKind, type Question } from "./types.js";

// The answer hook's request (`hook/src/answer.rs`): a question or a plan at its `PreToolUse`, or a
// permission request, which names no call (danielrispler/Falinks#478). `agent_id` is the asking subagent's, null for the
// session's own agent.
export type Ask = {
  nonce: string;
  hook_event_name: string;
  tool_name: string;
  tool_use_id: string | null;
  tool_input: Record<string, unknown>;
  agent_id: string | null;
};

// Whether `ask`'s card shows in the view of `agent` (null: the session's own): a question docks only
// in its asker's view, a plan or a permission in every view of its session (`story.ts`).
export const cardShown = (ask: Ask, agent: string | null) =>
  askKind(ask.tool_name) !== "question" || (ask.agent_id ?? null) === agent;
// A question's answer: each question's text to its label, or a multi-select's to its labels. A plan's
// or a permission's: allow or deny. Null answers in the terminal.
export type Reply = { answers: Record<string, string | string[]> } | { decision: "allow" | "deny" } | null;

let current: { session: string; ask: Ask } | null = null;
let redraw = () => {};
// The request last answered, which the shell may still return until the hook takes the answer.
let sent = "";

// The open request for `question`, if the card can answer it.
export const askFor = (question: Question) =>
  current?.ask.tool_use_id === question.tool_use_id ? current.ask : null;

// The open plan or permission request, if any (danielrispler/Falinks#478): the core keeps no question for either.
export const openDecision = () =>
  current !== null && askKind(current.ask.tool_name) !== "question" ? current.ask : null;

const watch = (session: string | null) =>
  invoke<Ask | null>("watch", { session }).catch((e) => (console.warn(e), null));

// The view watched last, so a redraw of the same one asks the shell nothing.
let watched = "";

// The session and subagent whose view shows, null while the window is hidden.
const shown = () => {
  const selection = document.visibilityState === "visible" ? studio.selection : null;
  const selected = selection?.session;
  // The hook's payload names the latest conversation, which a `/clear` changes, not the row (danielrispler/Falinks#474).
  const session = selected ? selected.resume_id || selected.id : null;
  return { session, agent: selection?.agent ?? null };
};

// Docks `ask`, the shown session's open request, or undocks when it is null.
async function take(ask: Ask | null) {
  const { session, agent } = shown();
  if (ask !== null && !cardShown(ask, agent)) {
    // Its card is not shown, so stop watching: the hook hands the call to the terminal at once. Watch
    // again once the hook, which looks every 100 ms, has let go, so the view's next request docks.
    await watch(null);
    watched = "";
    setTimeout(rewatch, 300);
    ask = null;
  }
  const next = session !== null && ask !== null && ask.nonce !== sent ? { session, ask } : null;
  if (next?.ask.nonce === current?.ask.nonce) return;
  current = next;
  redraw();
}

// Tells the shell which session's view shows, when that changed (danielrispler/Falinks#569): the shell keeps the
// heartbeat and sends the session's open request as an `ask` event, with no poll here.
export async function rewatch() {
  const { session, agent } = shown();
  const key = JSON.stringify([session, agent]);
  if (key === watched) return;
  watched = key;
  await take(await watch(session));
}

export function startWatching(draw: () => void) {
  redraw = draw;
  listen<Ask | null>("ask", (ask) => take(ask));
  document.addEventListener("visibilitychange", rewatch);
  rewatch();
  // Shows the shell the window is alive, so a frozen window lets the hook hand a question to the
  // terminal within seconds (danielrispler/Falinks#569). It asks for nothing; the open request still comes as an event.
  setInterval(() => {
    if (shown().session !== null) invoke("alive").catch(() => {});
  }, 1000);
}

export async function sendAnswer(ask: Ask, reply: Reply) {
  if (current?.ask.nonce !== ask.nonce) return;
  const { session } = current;
  sent = ask.nonce;
  current = null;
  redraw();
  const args = { session, nonce: ask.nonce, answers: null, decision: null, ...reply };
  await invoke("answer", args).catch((e) => console.warn(e));
}
