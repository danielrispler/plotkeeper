// The window: on the left the session list with each session's subagents nested, on the right the session Daniel selected, with its state,
// "now" line and cards. The core decides what it shows; this file only draws what the shell sends, on
// load and on each "sessions", "selected" or "cards" event.

type Step = { tool: string; detail: string | null };
type SessionState = "needs_you" | "running" | "your_turn" | "idle" | "ended";
// A check that ran: a pass or fail line, oldest first. `passed` is null when its exit code is masked.
type CheckLine = { command: string; passed: boolean | null };
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
// Who sent a prompt: Daniel, a wake-up naming the task that returned, or a known task id in markup
// the studio does not know, which counts as Daniel's.
type Sender = "daniel" | { wake_up: string } | { unrecognised: string };
type Prompt = { at: number; sender: Sender };
// An AskUserQuestion call, as the tool input gives it, with Daniel's choice for each question by its text
// once he answers in the terminal.
type Choice = { label: string; description?: string };
type Asked = { question: string; header?: string; options: Choice[] };
type Answer = "open" | "not_answered" | { answered: Record<string, string> };
type Question = { agent: string | null; questions: Asked[]; answer: Answer };
type Session = {
  id: string;
  cwd: string;
  last_step: Step | null;
  state: SessionState;
  checks: CheckLine[];
  agents: Agent[];
  waiting: Wait | null;
  prompts: Prompt[];
  questions: Question[];
};
// A session's change since its last card, from its base: HEAD and the files git status listed then.
type Base = { head: string | null; files: string[] };
type CardRequest = { session_id: string; id: string; base: Base; change: string };
// A card's multiple-choice question; `key` is the right option, counting from 0.
type Quiz = { text: string; options: { text: string; why: string }[]; key: number };
type Card = {
  explanation: string;
  paths: string[];
  code: string;
  why_it_matters: string;
  not_checked: string;
  quiz: Quiz | null;
};
// Daniel's tap on a quiz, graded by the writer's key.
type Guess = { option: number; right: boolean; at: number };
// The store also records when the quiz showed; the pane never shows it, nor any rate.
type StoredCard = { request: CardRequest; card: Card; guess: Guess | null; key_wrong: boolean };
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

const folder = (session: Session) => session.cwd.split("/").pop() || session.cwd;

// The session list, as the shell last sent it, and the row Daniel clicked last: a session's, or a
// subagent's, whose view then shows its own line and questions.
let listed: Session[] = [];
let selectedId: string | null = null;
let viewedAgent: string | null = null;
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

// "worker Implement #355 relocation returned", while the latest prompt is a wake-up and the session
// has taken no step since; null otherwise.
function returned(session: Session): string | null {
  const sender = session.prompts[session.prompts.length - 1]?.sender;
  if (session.last_step !== null || typeof sender !== "object" || !("wake_up" in sender)) return null;
  const agent = session.agents.find((agent) => agent.id === sender.wake_up);
  return `worker ${agent?.description ?? sender.wake_up.slice(0, 5)} returned`;
}

// What the session's line says: its last tool step, which worker returned, or while it needs Daniel,
// who asks and for how long, such as "a1191 · Implement #355 asks · 3m".
function rowText(session: Session): string {
  const wait = session.waiting;
  if (wait === null) return returned(session) ?? stepText(session.last_step);
  if (wait.agent === null) return `${stepText(session.last_step)} · ${age(wait.since)}`;
  const asker = session.agents.find((agent) => agent.id === wait.agent);
  return `${wait.agent.slice(0, 5)} · ${asker?.description ?? ""} asks · ${age(wait.since)}`;
}

// One line: the folder, the session's short id, its state and what it is doing.
function row(session: Session): HTMLElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "row";
  button.setAttribute("aria-pressed", String(session.id === selectedId && viewedAgent === null));
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
  button.setAttribute("aria-pressed", String(agent.id === viewedAgent));
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

// Only Daniel's click changes the selected session, and the subagent of it whose view shows, if any.
async function select(id: string, agent: string | null = null) {
  selectedId = id;
  viewedAgent = agent;
  showSessions(listed);
  await window.__TAURI__.core.invoke("select", { id });
  show(await window.__TAURI__.core.invoke<Session | null>("selected"));
  showCards(await window.__TAURI__.core.invoke<SessionCards>("cards"));
}

// Daniel opened a subagent's line: a failed helper then fades, and its view shows.
async function openAgent(sessionId: string, agentId: string) {
  await window.__TAURI__.core.invoke("open", { agentId });
  listed = await window.__TAURI__.core.invoke<Session[]>("sessions");
  await select(sessionId, agentId);
}

function show(session: Session | null) {
  element("empty").hidden = session !== null;
  element("view").hidden = session === null;
  if ((session?.id ?? null) !== selectedId) {
    selectedId = session?.id ?? null;
    showSessions(listed);
  }
  if (session === null) return;
  // A subagent's view shows its own line and questions; the cards and checks are its session's.
  const agent = session.agents.find((a) => a.id === viewedAgent) ?? null;
  const asks = agent !== null && session.waiting?.agent === agent.id;
  const [label, state] =
    agent === null
      ? [stateLabels[session.state], session.state]
      : asks
        ? [stateLabels.needs_you, "needs_you"]
        : [agent.status, agent.status];
  const step = (agent ?? session).last_step;
  element("view").toggleAttribute("data-agent", agent !== null);
  element("session").textContent =
    agent === null
      ? `${folder(session)} · ${session.id.slice(0, 8)}`
      : `${agent.description ?? ""} · ${agent.id.slice(0, 5)}`;
  element("state").textContent = label;
  element("state").dataset.state = state;
  element("tool").textContent = step?.tool ?? (agent === null ? returned(session) : null) ?? "No step yet";
  element("detail").textContent = step?.detail ?? "";
  showQuestions(session.questions.filter((question) => question.agent === (agent?.id ?? null)));
  showChecks(session.checks);
}

// Whether a question is open, answered in the terminal, or not answered: its call failed, or its turn
// or its agent ended first. An open one needs Daniel, so only it takes the colour.
const answerLabels = { open: "Needs you", not_answered: "Not answered", answered: "Answered" };

// An agent's question laid out as its choices, the one Daniel chose marked. Answering stays in the
// terminal.
function questionElement({ questions, answer }: Question): HTMLElement {
  const block = document.createElement("article");
  block.className = "question";
  const status = typeof answer === "string" ? answer : "answered";
  const state = text("p", answerLabels[status], "state");
  if (status === "open") state.dataset.state = "needs_you";
  block.append(state);
  for (const asked of questions) {
    const chosen = typeof answer === "string" ? null : (answer.answered[asked.question] ?? null);
    const choices = document.createElement("ul");
    choices.className = "choices";
    choices.append(
      ...asked.options.map((choice) => {
        const item = document.createElement("li");
        item.append(text("span", choice.label, "label"), text("span", choice.description ?? "", "description"));
        item.classList.toggle("chosen", chosen?.split(", ").includes(choice.label) ?? false);
        return item;
      }),
    );
    block.append(text("p", asked.header ?? "", "header"), text("p", asked.question, "asked"), choices);
    if (chosen !== null) block.append(text("p", `Your answer: ${chosen}`, "chosen-text"));
  }
  return block;
}

// The questions the viewed agent asked, newest first.
function showQuestions(questions: Question[]) {
  element("questions").replaceChildren(...[...questions].reverse().map(questionElement));
  element("questions-title").hidden = questions.length === 0;
}

// Whether a check passed, from its exit code; with no colour, as colour is only for "needs you".
function outcome(passed: boolean | null): string {
  if (passed === null) return "Ran";
  return passed ? "Passed" : "Failed";
}

// The checks that ran, newest first, one line each.
function showChecks(checks: CheckLine[]) {
  element("checks").replaceChildren(
    ...[...checks].reverse().map((line) => {
      const item = document.createElement("li");
      item.append(text("span", outcome(line.passed), "outcome"), code(line.command));
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

// Records Daniel's tap, or his flag on the key, and draws the cards again at once.
async function record(command: "guess" | "key_wrong", args: Record<string, unknown>) {
  await window.__TAURI__.core.invoke(command, args);
  showCards(await window.__TAURI__.core.invoke<SessionCards>("cards"));
}

// A card's quiz and its options, one tap each. Once Daniel taps, it shows whether he was right and,
// under each option, why it is right or wrong, with the right one named; a tapped option stays
// marked, with no colour, as colour is only for "needs you".
function quizElements(id: string, quiz: Quiz, guess: Guess | null): HTMLElement[] {
  const options = document.createElement("ol");
  options.className = "options";
  options.append(
    ...quiz.options.map((option, i) => {
      const item = document.createElement("li");
      const button = hebrew("button", option.text) as HTMLButtonElement;
      button.type = "button";
      button.disabled = guess !== null;
      button.setAttribute("aria-pressed", String(guess?.option === i));
      button.addEventListener("click", () => record("guess", { id, option: i }));
      item.append(button);
      if (guess) {
        item.append(hebrew("p", `${i === quiz.key ? "התשובה הנכונה. " : ""}${option.why}`));
      }
      return item;
    }),
  );
  const title = hebrew("p", quiz.text);
  title.className = "quiz";
  if (!guess) return [title, options];
  const verdict = hebrew("p", guess.right ? "צדקת." : "לא צדקת.");
  verdict.className = "verdict";
  return [title, verdict, options];
}

// A card in Hebrew, right to left: its quiz first, then, once Daniel taps an option, the
// explanation, the file paths, the code left to right, why the change matters, what was not checked
// and his flag on the key. A card with no quiz shows the explanation at once. The shell keeps each
// tap in the card store, so after a restart a guessed card is drawn guessed.
function cardElement({ request, card, guess, key_wrong }: StoredCard): HTMLElement {
  const node = document.createElement("article");
  node.className = "card";
  node.dir = "rtl";
  node.lang = "he";
  const quiz = card.quiz;
  if (quiz) node.append(...quizElements(request.id, quiz, guess));
  if (quiz && !guess) return node;
  const explanation = hebrew("p", card.explanation);
  explanation.className = "explanation";
  const paths = document.createElement("p");
  paths.className = "paths";
  paths.append(...card.paths.map(code));
  const source = document.createElement("pre");
  source.dir = "ltr";
  source.textContent = card.code;
  node.append(
    explanation,
    paths,
    source,
    hebrew("h2", "למה זה חשוב"),
    hebrew("p", card.why_it_matters),
    hebrew("h2", "מה לא נבדק"),
    hebrew("p", card.not_checked),
  );
  if (quiz) {
    const flag = hebrew("button", key_wrong ? "סימנת שהמפתח שגוי" : "המפתח שגוי") as HTMLButtonElement;
    flag.type = "button";
    flag.className = "key-wrong";
    flag.disabled = key_wrong;
    flag.addEventListener("click", () => record("key_wrong", { id: request.id }));
    node.append(flag);
  }
  return node;
}

// Unguessed cards whose quiz the pane drew while the window was hidden. A quiz counts as shown when
// it is drawn in a visible window, or when the window next becomes visible; the store keeps the first
// time, which the weekly rates measure guesses from.
const unshown = new Set<string>();

function markShown() {
  if (document.visibilityState !== "visible") return;
  for (const id of unshown) window.__TAURI__.core.invoke("shown", { id });
  unshown.clear();
}

document.addEventListener("visibilitychange", markShown);

// Cards already drawn, keyed by their id, with the record each was drawn from. A card is drawn again
// only when its record changed, such as after a tap, and is moved whole to the older group.
const drawn = new Map<string, { record: string; node: HTMLElement }>();

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
      const record = JSON.stringify(stored);
      if (drawn.get(id)?.record !== record) {
        if (!drawn.has(id) && stored.guess === null) unshown.add(id);
        drawn.set(id, { record, node: cardElement(stored) });
      }
      return drawn.get(id)!.node;
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
// Claude Code's version changed: re-record the subagent fixture.
window.__TAURI__.event.listen<string>("notice", (event) => {
  element("notice").textContent = event.payload;
  element("notice").hidden = false;
});
window.__TAURI__.core.invoke<SessionCards>("cards").then(showCards);

export {};
