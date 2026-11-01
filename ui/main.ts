// The window: on the left the session list with each session's subagents nested, on the right the session Daniel selected, with its state,
// "now" line and cards. The core decides what it shows; this file only draws what the shell sends, on
// load and on each "sessions", "selected" or "cards" event.

type Step = { tool: string; detail: string | null };
type SessionState = "needs_you" | "running" | "your_turn" | "idle" | "ended";
type GuessPrompt = { session_id: string; tool_use_id: string; step: Step; guess: boolean | null };
type Check = { guess: boolean | null; exit_passed: boolean | null };
// A check that ran: a pass or fail line, oldest first.
type CheckLine = { command: string; check: Check };
// Who spawned a subagent: the session's own agent, another subagent, or no known spawn.
type Parent = "session" | "unattached" | { agent: string };
type Agent = {
  id: string;
  parent: Parent;
  kind: "worker" | "helper";
  description: string | null;
  agent_type: string | null;
  // "running", then its wake-up's status, such as "completed" or "killed".
  status: string;
  started_at: number;
  ended_at: number | null;
  last_step: Step | null;
  result: string | null;
  opened: boolean;
  faded: boolean;
};
// What a session that needs Daniel waits on: the subagent who asks, or null for its own agent.
type Wait = { agent: string | null; since: number };
type Session = {
  id: string;
  cwd: string;
  last_step: Step | null;
  state: SessionState;
  guess: GuessPrompt | null;
  checks: CheckLine[];
  agents: Agent[];
  waiting: Wait | null;
};
// A session's change since its last card, from its base: HEAD and the files git status listed then.
type Base = { head: string | null; files: string[] };
type CardRequest = { session_id: string; id: string; base: Base; change: string; check: Check | null };
type Card = {
  question: string;
  answer: string;
  paths: string[];
  code: string;
  why_it_matters: string;
  not_checked: string;
};
// The store also records when the question showed and how the guess settled; the pane never shows
// either, nor any rate.
type StoredCard = { request: CardRequest; card: Card; revealed_at: number | null; passed: boolean | null };
// The selected session's cards, split at 30 minutes by the shell's tick, each oldest first.
type SessionCards = { recent: StoredCard[]; older: StoredCard[] };

const stateLabels: Record<SessionState, string> = {
  needs_you: "Needs you",
  running: "Running",
  your_turn: "Your turn",
  idle: "Idle",
  ended: "Ended",
};

declare global {
  interface Window {
    __TAURI__: {
      core: { invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> };
      event: { listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<unknown> };
    };
  }
}

const element = (id: string) => document.getElementById(id)!;

// "Will it pass?" on the check that is the wait's card step, open until its result arrives. After a
// tap it shows which answer Daniel gave, never whether he was right.
let openGuess: GuessPrompt | null = null;
const taps = [
  ["guess-yes", true],
  ["guess-no", false],
] as const;

function showGuess(prompt: GuessPrompt | null) {
  openGuess = prompt;
  element("guess").hidden = prompt === null;
  if (prompt === null) return;
  element("guess-command").textContent = prompt.step.detail ?? "";
  for (const [id, willPass] of taps) {
    const button = element(id) as HTMLButtonElement;
    button.disabled = prompt.guess !== null;
    button.setAttribute("aria-pressed", String(prompt.guess === willPass));
  }
}

for (const [id, willPass] of taps) {
  element(id).addEventListener("click", () => {
    if (openGuess) window.__TAURI__.core.invoke("guess", { toolUseId: openGuess.tool_use_id, willPass });
  });
}

const folder = (session: Session) => session.cwd.split("/").pop() || session.cwd;

// The session list, as the shell last sent it, and the row Daniel clicked last.
let listed: Session[] = [];
let selectedId: string | null = null;
// The collapsed groups Daniel opened, which stay open when the list is drawn again.
const collapsed: SessionState[] = ["idle", "ended"];
const opened = new Set<SessionState>();

function text(tag: string, content: string, className = ""): HTMLElement {
  const node = document.createElement(tag);
  node.className = className;
  node.textContent = content;
  return node;
}

const stepText = (step: Step | null) => (step ? `${step.tool} ${step.detail ?? ""}` : "No step yet");

// The time from `from` to `to`, such as "3m".
function age(from: number, to = Date.now()): string {
  const minutes = Math.floor((to - from) / 60_000);
  return minutes < 60 ? `${minutes}m` : `${Math.floor(minutes / 60)}h`;
}

const parentId = (parent: Parent) => (typeof parent === "string" ? parent : parent.agent);
const childrenOf = (session: Session, parent: string) =>
  session.agents.filter((agent) => parentId(agent.parent) === parent);

// The count of a parent's helpers that faded into its line.
function done(session: Session, parent: string): HTMLElement[] {
  const faded = childrenOf(session, parent).filter((agent) => agent.faded).length;
  return faded ? [text("span", `${faded} done`, "id")] : [];
}

// What the session's line says: its last tool step, or while it needs Daniel, who asks and for how
// long, such as "a1191 · Implement #355 asks · 3m".
function rowText(session: Session): string {
  const wait = session.waiting;
  if (wait === null) return stepText(session.last_step);
  if (wait.agent === null) return `${stepText(session.last_step)} · ${age(wait.since)}`;
  const asker = session.agents.find((agent) => agent.id === wait.agent);
  return `${wait.agent.slice(0, 5)} · ${asker?.description ?? ""} asks · ${age(wait.since)}`;
}

// One line: the folder, the session's short id, its state and what it is doing.
function row(session: Session): HTMLElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "row";
  button.setAttribute("aria-pressed", String(session.id === selectedId));
  const state = text("span", stateLabels[session.state], "row-state");
  state.dataset.state = session.state;
  button.append(
    text("span", folder(session), "name"),
    text("span", session.id.slice(0, 8), "id"),
    state,
    ...done(session, "session"),
    text("span", rowText(session), "row-step"),
  );
  button.addEventListener("click", () => select(session.id));
  return button;
}

// A subagent's line, indented under the agent that spawned it. A worker shows its task, status and
// last step; a helper its task, type, status, duration and result. A failed child stands out until
// Daniel opens it.
function agentRow(session: Session, agent: Agent, depth: number): HTMLElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `row ${agent.kind}`;
  button.style.paddingLeft = `${8 + 16 * depth}px`;
  const asks = session.waiting?.agent === agent.id;
  const state = text("span", asks ? stateLabels.needs_you : agent.status, "row-state");
  if (asks) state.dataset.state = "needs_you";
  const failed = agent.status !== "running" && agent.status !== "completed";
  state.classList.toggle("failed", failed && !agent.opened);
  const details =
    agent.kind === "worker"
      ? [stepText(agent.last_step)]
      : [agent.agent_type, age(agent.started_at, agent.ended_at ?? Date.now()), agent.result];
  button.append(
    text("span", agent.description ?? agent.id.slice(0, 5), "name"),
    state,
    ...done(session, agent.id),
    text("span", details.filter(Boolean).join(" · "), "row-step"),
  );
  button.addEventListener("click", () => openAgent(session.id, agent.id));
  return button;
}

// A session's row, then its subagents under the agent that spawned them, at any depth, and those
// with no known spawn under "Unattached". A faded helper is not drawn; its parent's line counts it.
function tree(session: Session): HTMLElement[] {
  const nested = (parent: string, depth: number): HTMLElement[] =>
    childrenOf(session, parent).flatMap((agent) => [
      ...(agent.faded ? [] : [agentRow(session, agent, depth)]),
      ...nested(agent.id, depth + 1),
    ]);
  const unattached = nested("unattached", 2);
  const label = text("p", "Unattached", "unattached");
  return [row(session), ...nested("session", 1), ...(unattached.length ? [label, ...unattached] : [])];
}

// The core sends the sessions grouped by state, so a group starts where the state changes. Idle and
// ended sessions collapse at the bottom.
function showSessions(sessions: Session[]) {
  listed = sessions;
  const groups: Session[][] = [];
  for (const session of sessions) {
    const last = groups[groups.length - 1];
    if (last?.[0].state === session.state) last.push(session);
    else groups.push([session]);
  }
  element("groups").replaceChildren(
    ...groups.map((rows) => {
      const state = rows[0].state;
      const title = `${stateLabels[state]} · ${rows.length}`;
      if (!collapsed.includes(state)) {
        const group = document.createElement("section");
        group.append(text("h1", title), ...rows.flatMap(tree));
        return group;
      }
      const group = document.createElement("details");
      group.open = opened.has(state);
      group.addEventListener("toggle", () => (group.open ? opened.add(state) : opened.delete(state)));
      group.append(text("summary", title), ...rows.flatMap(tree));
      return group;
    }),
  );
  element("no-sessions").hidden = sessions.length > 0;
}

// Only Daniel's click changes the selected session.
async function select(id: string) {
  await window.__TAURI__.core.invoke("select", { id });
  show(await window.__TAURI__.core.invoke<Session | null>("selected"));
  showCards(await window.__TAURI__.core.invoke<SessionCards>("cards"));
}

// Daniel opened a subagent's line: a failed helper then fades, and its session is selected.
async function openAgent(sessionId: string, agentId: string) {
  await window.__TAURI__.core.invoke("open", { agentId });
  showSessions(await window.__TAURI__.core.invoke<Session[]>("sessions"));
  await select(sessionId);
}

function show(session: Session | null) {
  showGuess(session?.guess ?? null);
  element("empty").hidden = session !== null;
  element("view").hidden = session === null;
  if ((session?.id ?? null) !== selectedId) {
    selectedId = session?.id ?? null;
    showSessions(listed);
  }
  if (session === null) return;
  element("session").textContent = `${folder(session)} · ${session.id.slice(0, 8)}`;
  element("state").textContent = stateLabels[session.state];
  element("state").dataset.state = session.state;
  element("tool").textContent = session.last_step?.tool ?? "No step yet";
  element("detail").textContent = session.last_step?.detail ?? "";
  showChecks(session.checks);
}

// Whether a check passed, from its exit code; with no colour, as colour is only for "needs you".
function outcome(check: Check): string {
  if (check.exit_passed === null) return "Ran";
  return check.exit_passed ? "Passed" : "Failed";
}

// The checks that ran, newest first, one line each.
function showChecks(checks: CheckLine[]) {
  element("checks").replaceChildren(
    ...[...checks].reverse().map((line) => {
      const item = document.createElement("li");
      item.append(text("span", outcome(line.check), "outcome"), code(line.command));
      return item;
    }),
  );
  element("checks-title").hidden = checks.length === 0;
}

// Code, left to right, in its own isolate.
function code(text: string): HTMLElement {
  const node = document.createElement("code");
  node.dir = "ltr";
  node.textContent = text;
  return node;
}

// Hebrew text whose code is in backticks. Each code span becomes a left-to-right isolate, so `a <= b`
// is not mirrored inside the right-to-left text.
function hebrew(tag: string, text: string): HTMLElement {
  const node = document.createElement(tag);
  node.append(...text.split("`").map((part, i) => (i % 2 ? code(part) : part)));
  return node;
}

// Whether a check passed, as settled from its exit code or the card writer's verdict.
function verdict(passed: boolean | null): string {
  if (passed === null) return "הפלט לא מראה אם הבדיקה עברה.";
  return passed ? "הבדיקה עברה." : "הבדיקה נכשלה.";
}

// A card in Hebrew, right to left: the question, then, on a tap, whether a check passed, the answer,
// the file paths, the code left to right, why the step matters and what was not checked. The shell
// keeps each reveal in the card store, so after a restart a revealed card is drawn open.
function cardElement({ request, card, revealed_at, passed }: StoredCard): HTMLElement {
  const details = document.createElement("details");
  details.className = "card";
  details.dir = "rtl";
  details.lang = "he";
  details.dataset.id = request.id;
  details.open = revealed_at !== null;
  details.addEventListener("toggle", () => {
    if (details.open) window.__TAURI__.core.invoke("reveal", { id: request.id });
  });
  const answer = hebrew("p", card.answer);
  answer.className = "answer";
  const paths = document.createElement("p");
  paths.className = "paths";
  paths.append(...card.paths.map(code));
  const source = document.createElement("pre");
  source.dir = "ltr";
  source.textContent = card.code;
  const result = hebrew("p", verdict(passed));
  result.className = "verdict";
  details.append(
    hebrew("summary", card.question),
    ...(request.check ? [result] : []),
    answer,
    paths,
    source,
    hebrew("h2", "למה זה חשוב"),
    hebrew("p", card.why_it_matters),
    hebrew("h2", "מה לא נבדק"),
    hebrew("p", card.not_checked),
  );
  return details;
}

// Closed cards whose question the pane drew while the window was hidden. A question counts as shown
// when it is drawn in a visible window, or when the window next becomes visible; the store keeps the
// first time, which the weekly rates measure reveals from.
const unshown = new Set<string>();

function markShown() {
  if (document.visibilityState !== "visible") return;
  for (const id of unshown) window.__TAURI__.core.invoke("shown", { id });
  unshown.clear();
}

document.addEventListener("visibilitychange", markShown);

// Each card is a <details>: its question shows, and a tap reveals the rest. Cards already drawn are
// kept, keyed by their id, so one Daniel opened stays open on the next event and when
// it moves to the older group.
const drawn = new Map<string, HTMLElement>();

function showCards({ recent, older }: SessionCards) {
  const ids = new Set([...recent, ...older].map(({ request }) => request.id));
  for (const id of drawn.keys()) {
    if (ids.has(id)) continue;
    drawn.delete(id);
    unshown.delete(id);
  }
  // The newest card first.
  const nodes = (cards: StoredCard[]) =>
    [...cards].reverse().map((stored) => {
      const id = stored.request.id;
      if (!drawn.has(id)) {
        drawn.set(id, cardElement(stored));
        if (stored.revealed_at === null) unshown.add(id);
      }
      return drawn.get(id)!;
    });
  element("recent").replaceChildren(...nodes(recent));
  element("older").replaceChildren(...nodes(older));
  element("recent-title").hidden = recent.length === 0;
  element("older-title").hidden = older.length === 0;
  markShown();
}

window.__TAURI__.event.listen<Session[]>("sessions", (event) => showSessions(event.payload));
window.__TAURI__.core.invoke<Session[]>("sessions").then(showSessions);
window.__TAURI__.event.listen<Session | null>("selected", (event) => show(event.payload));
window.__TAURI__.core.invoke<Session | null>("selected").then(show);
window.__TAURI__.event.listen<SessionCards>("cards", (event) => showCards(event.payload));
window.__TAURI__.core.invoke<SessionCards>("cards").then(showCards);

export {};
