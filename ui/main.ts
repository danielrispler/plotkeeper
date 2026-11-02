// The window: on the left the session list with each session's subagents nested, on the right the
// session Daniel selected, with its state, "now" line, brief, cards, questions and checks. The core
// decides what it shows; this file only draws what the shell sends, on load and on each "sessions",
// "selected", "cards" or "notice" event, and marks a quiz shown when the window becomes visible.

type Step = { tool: string; detail: string | null };
type SessionState = "needs_you" | "running" | "your_turn" | "idle" | "ended";
// A check that ran: a pass or fail line, oldest first. `passed` is null when its exit code is masked.
type CheckLine = { command: string; passed: boolean | null };
// Who spawned a subagent: the session's own agent, another subagent, or no known spawn.
type Parent = "session" | "unattached" | { agent: string };
type Subagent = {
  id: string;
  parent: Parent;
  kind: "worker" | "helper";
  // Its spawn's description, else its short id: the core's one name for it.
  name: string;
  agent_type: string | null;
  // "running", then its wake-up's status, such as "completed" or "killed".
  status: string;
  started_at: number;
  ended_at: number | null;
  last_step: Step | null;
  result: string | null;
  // It ended with a status other than "completed", such as "killed": the core's one rule for failed.
  failed: boolean;
  opened: boolean;
  faded: boolean;
};
// Who a session that needs Daniel waits on, and since when: the subagent who asks, or null for its
// own agent.
type Asker = { agent: string | null; since: number };
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
  folder_name: string;
  last_step: Step | null;
  state: SessionState;
  checks: CheckLine[];
  agents: Subagent[];
  asker: Asker | null;
  prompts: Prompt[];
  questions: Question[];
  // "worker <name> returned", after a wake-up until its next step or Daniel's next prompt.
  returned: string | null;
  // In the session list only: what its row says, built by the card store.
  line?: string;
};
// The shell sends a card's request without its diff and base, which the pane never reads. `agent`
// is the worker whose card it is; `changed_by` holds the files other subagents edited in its
// worktree, by subagent.
type CardRequest = {
  session_id: string;
  id: string;
  agent: string | null;
  changed_by: Record<string, string[]>;
};
// A card's multiple-choice question; `key` is the right option, counting from 0.
type Quiz = { text: string; options: { text: string; why: string }[]; key: number };
// `architecture` is null when the change is not structural.
type Card = {
  summary: string;
  explanation: string;
  paths: string[];
  code: string;
  why_it_matters: string;
  not_checked: string;
  architecture: string | null;
  quiz: Quiz | null;
};
// Daniel's tap on a quiz, graded by the writer's key.
type Guess = { option: number; right: boolean; at: number };
// Daniel's rating of a card's explanation. His fifth reason, "the key is wrong", is `key_wrong`, the
// one record the quiz's flag also sets.
type Level = "good" | "fair" | "poor";
type Reason = "unclear" | "wrong" | "too_shallow" | "too_easy";
type Rating = { level: Level; reasons: Reason[] };
// The store also records when the quiz showed; the pane never shows it, nor any rate. `more` is the
// longer explanation from "explain more".
type StoredCard = {
  request: CardRequest;
  card: Card;
  guess: Guess | null;
  key_wrong: boolean;
  more: string | null;
  rating: Rating | null;
};
// The selected session's brief, and its cards, split at 30 minutes by the shell's tick, each oldest
// first.
type SessionCards = { brief: string | null; recent: StoredCard[]; older: StoredCard[] };

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

// The session list and the selection, as the shell last sent them. The selection is the core's: the
// session Daniel selected and the subagent of it he opened, whose view then shows its own line.
type Selection = { session: Session; agent: string | null };
let listed: Session[] = [];
let selection: Selection | null = null;
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

// A button's type, pressed state and click.
function pressable<T extends HTMLElement>(button: T, pressed: boolean, click: () => void): T {
  button.setAttribute("type", "button");
  button.setAttribute("aria-pressed", String(pressed));
  button.addEventListener("click", click);
  return button;
}

// A subagent's state label and its state: "Needs you" while it asks, else its status.
function agentState(session: Session, agent: Subagent): [string, string] {
  return session.asker?.agent === agent.id ? [stateLabels.needs_you, "needs_you"] : [agent.status, agent.status];
}

// Asks the shell for the selected session's cards and draws them.
async function refreshCards() {
  showCards(await window.__TAURI__.core.invoke<SessionCards>("cards"));
}

// A session's subagents by the id of the agent that spawned them: "session", "unattached" or a
// subagent's id. Built once per draw, so the tree and the "done" counts do not scan every subagent.
type Children = Map<string, Subagent[]>;
function childrenOf(session: Session): Children {
  const children: Children = new Map();
  for (const agent of session.agents) {
    const parent = typeof agent.parent === "string" ? agent.parent : agent.parent.agent;
    if (!children.has(parent)) children.set(parent, []);
    children.get(parent)!.push(agent);
  }
  return children;
}

// The count of a parent's helpers that faded into its line.
function done(children: Subagent[] = []): HTMLElement[] {
  const faded = children.filter((agent) => agent.faded).length;
  return faded ? [text("span", `${faded} done`, "id")] : [];
}

// One line: the folder, the session's short id, its state and what it is doing.
function row(session: Session, children: Children): HTMLElement {
  const pressed = session.id === selection?.session.id && selection.agent === null;
  const button = pressable(document.createElement("button"), pressed, () => select(session.id));
  button.className = "row";
  const state = text("span", stateLabels[session.state], "row-state");
  state.dataset.state = session.state;
  button.append(
    text("span", session.folder_name, "name"),
    text("span", session.id.slice(0, 8), "id"),
    state,
    ...done(children.get("session")),
    text("span", session.line ?? "", "row-step"),
  );
  return button;
}

// A subagent's line, indented under the agent that spawned it. A worker shows its task, status and
// last step; a helper its task, type, status, duration and result. A failed child stands out until
// Daniel opens it.
function agentRow(session: Session, children: Children, agent: Subagent, depth: number): HTMLElement {
  const button = pressable(document.createElement("button"), agent.id === selection?.agent, () =>
    select(session.id, agent.id),
  );
  button.className = `row ${agent.kind}`;
  button.style.paddingLeft = `${8 + 16 * depth}px`;
  const [label, stateName] = agentState(session, agent);
  const state = text("span", label, "row-state");
  state.dataset.state = stateName;
  state.classList.toggle("failed", agent.failed && !agent.opened);
  const details =
    agent.kind === "worker"
      ? [stepText(agent.last_step)]
      : [agent.agent_type, age(agent.started_at, agent.ended_at ?? Date.now()), agent.result];
  button.append(
    text("span", agent.name, "name"),
    state,
    ...done(children.get(agent.id)),
    text("span", details.filter(Boolean).join(" · "), "row-step"),
  );
  return button;
}

// A session's row, then its subagents under the agent that spawned them, at any depth, and those
// with no known spawn under "Unattached". A faded helper is not drawn; its parent's line counts it.
function tree(session: Session): HTMLElement[] {
  const children = childrenOf(session);
  const nested = (parent: string, depth: number): HTMLElement[] =>
    (children.get(parent) ?? []).flatMap((agent) => [
      ...(agent.faded ? [] : [agentRow(session, children, agent, depth)]),
      ...nested(agent.id, depth + 1),
    ]);
  const unattached = nested("unattached", 2);
  const label = text("p", "Unattached", "unattached");
  return [row(session, children), ...nested("session", 1), ...(unattached.length ? [label, ...unattached] : [])];
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

// Only Daniel's click changes the selected session, and the subagent of it whose view shows, if any:
// its own line, questions, brief and cards. Opening a subagent's line also fades a failed helper.
// The core asks for it, and the pane draws the selection the core returns.
async function select(id: string, agent: string | null = null) {
  let selected = await window.__TAURI__.core.invoke<Selection>("select", { id });
  if (agent !== null) {
    selected = await window.__TAURI__.core.invoke<Selection>("open", { agentId: agent });
    listed = await window.__TAURI__.core.invoke<Session[]>("sessions");
  }
  show(selected);
  await refreshCards();
}

function show(selected: Selection | null) {
  selection = selected;
  showSessions(listed);
  element("empty").hidden = selected !== null;
  element("view").hidden = selected === null;
  if (selected === null) return;
  const session = selected.session;
  // A subagent's view shows its own line, questions, brief and cards; the checks are its session's.
  const agent = session.agents.find((a) => a.id === selected.agent) ?? null;
  const [label, state] = agent === null ? [stateLabels[session.state], session.state] : agentState(session, agent);
  const step = (agent ?? session).last_step;
  element("view").toggleAttribute("data-agent", agent !== null);
  element("session").textContent =
    agent === null
      ? `${session.folder_name} · ${session.id.slice(0, 8)}`
      : agent.name;
  element("state").textContent = label;
  element("state").dataset.state = state;
  element("tool").textContent = step?.tool ?? (agent === null ? session.returned : null) ?? "No step yet";
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
    if (chosen !== null) block.append(text("p", `Your answer: ${chosen}`));
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

// Records Daniel's tap, his flag on the key or his rating, and draws the cards again at once.
async function record(command: "guess" | "key_wrong" | "rate", args: Record<string, unknown>) {
  await window.__TAURI__.core.invoke(command, args);
  await refreshCards();
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
      const button = pressable(hebrew("button", option.text) as HTMLButtonElement, guess?.option === i, () =>
        record("guess", { id, option: i }),
      );
      button.disabled = guess !== null;
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
// explanation, the file paths, those other subagents changed, the code left to right, the architecture section when the change is
// structural, why the change matters, what was not checked, his flag on the key, "explain more" and
// his rating. A card with no quiz shows the explanation at once. The shell keeps each tap in the card
// store, so after a restart a guessed card is drawn guessed and rated.
function cardElement({ request, card, guess, key_wrong, more, rating }: StoredCard): HTMLElement {
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
  node.append(explanation, paths, ...changedBy(request), source);
  if (card.architecture) node.append(hebrew("h2", "ארכיטקטורה"), hebrew("p", card.architecture));
  node.append(
    hebrew("h2", "למה זה חשוב"),
    hebrew("p", card.why_it_matters),
    hebrew("h2", "מה לא נבדק"),
    hebrew("p", card.not_checked),
  );
  if (quiz) node.append(keyWrongButton(request.id, key_wrong));
  node.append(moreElement(request.id, more), ratingElement(request.id, rating, quiz ? key_wrong : null));
  return node;
}

// Who changed what: the files another subagent, such as a helper, edited in the card's worktree, one
// line per subagent, by the name the core gave it.
function changedBy({ session_id, changed_by }: CardRequest): HTMLElement[] {
  const agents = listed.find((session) => session.id === session_id)?.agents ?? [];
  return Object.entries(changed_by).map(([id, files]) => {
    const name = agents.find((agent) => agent.id === id)?.name ?? id;
    const line = document.createElement("p");
    line.className = "paths";
    line.append(text("span", `שונו בידי ${name}:`), ...files.map(code));
    return line;
  });
}

// "The key is wrong": one record, flagged from the quiz or from the rating, and never undone.
function keyWrongButton(id: string, keyWrong: boolean): HTMLElement {
  const label = keyWrong ? "סימנת שהמפתח שגוי" : "המפתח שגוי";
  const flag = pressable(hebrew("button", label) as HTMLButtonElement, keyWrong, () => record("key_wrong", { id }));
  flag.className = "key-wrong";
  flag.disabled = keyWrong;
  return flag;
}

// The cards whose longer explanation Daniel opened, and those whose writer call is running; both
// outlast a card's drawing again.
const moreOpen = new Set<string>();
const asking = new Set<string>();

// "Explain more": the writer's longer explanation, under the card. The first tap asks the writer;
// later taps open and close the text the store keeps.
function moreElement(id: string, more: string | null): HTMLElement {
  const node = document.createElement("details");
  node.className = "more";
  node.open = moreOpen.has(id);
  const text = hebrew("p", more ?? "כותב…");
  text.className = "explanation";
  node.append(hebrew("summary", "הסבר עוד"), text);
  node.addEventListener("toggle", () => {
    if (node.open) moreOpen.add(id);
    else moreOpen.delete(id);
    if (node.open && more === null && !asking.has(id)) explainMore(id);
  });
  return node;
}

// Asks the writer, for seconds, then draws the card again: with its text, or closed when the writer
// gave none, so the next tap asks again.
async function explainMore(id: string) {
  asking.add(id);
  const more = await window.__TAURI__.core.invoke<string | null>("explain_more", { id });
  asking.delete(id);
  if (more === null) moreOpen.delete(id);
  drawn.delete(id);
  await refreshCards();
}

const levels: [Level, string][] = [
  ["good", "טוב"],
  ["fair", "בינוני"],
  ["poor", "חלש"],
];
const reasons: [Reason, string][] = [
  ["unclear", "לא ברור"],
  ["wrong", "שגוי"],
  ["too_shallow", "שטחי מדי"],
  ["too_easy", "קל מדי"],
];

// Daniel's rating of the explanation: a level, then, once he picked one, the reasons, each tap
// toggling one. `keyWrong` is the quiz's flag, offered as a reason on a card with a quiz.
function ratingElement(id: string, rating: Rating | null, keyWrong: boolean | null): HTMLElement {
  const choice = (label: string, pressed: boolean, picked: Rating) =>
    pressable(hebrew("button", label), pressed, () => record("rate", { id, rating: picked }));
  const node = document.createElement("div");
  node.className = "rating";
  const row = document.createElement("p");
  row.append(
    hebrew("span", "דירוג ההסבר:"),
    ...levels.map(([level, label]) =>
      choice(label, rating?.level === level, { level, reasons: rating?.reasons ?? [] }),
    ),
  );
  node.append(row);
  if (!rating) return node;
  const why = document.createElement("p");
  why.append(
    ...reasons.map(([reason, label]) => {
      const has = rating.reasons.includes(reason);
      const toggled = has ? rating.reasons.filter((r) => r !== reason) : [...rating.reasons, reason];
      return choice(label, has, { level: rating.level, reasons: toggled });
    }),
  );
  if (keyWrong !== null) why.append(keyWrongButton(id, keyWrong));
  node.append(why);
  return node;
}

// Unguessed cards whose quiz the pane drew while the window was hidden. A quiz counts as shown when
// it is drawn in a visible window, or when the window next becomes visible; the store keeps the first
// time, which the weekly rates measure guesses from.
const unshown = new Set<string>();

function markShown() {
  if (document.visibilityState !== "visible") return;
  if (unshown.size) window.__TAURI__.core.invoke("shown", { ids: [...unshown] });
  unshown.clear();
}

document.addEventListener("visibilitychange", markShown);

// Cards already drawn, keyed by their id, with the record each was drawn from. A card is drawn again
// only when its record changed, such as after a tap, and is moved whole to the older group.
const drawn = new Map<string, { record: string; node: HTMLElement }>();

// The session's brief at the top of its view, in Hebrew, one paragraph per line.
function showBrief(brief: string | null) {
  element("brief").replaceChildren(...(brief ?? "").split("\n").map((line) => hebrew("p", line)));
  element("brief").hidden = brief === null;
}

function showCards({ brief, recent, older }: SessionCards) {
  showBrief(brief);
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
window.__TAURI__.event.listen<Selection | null>("selected", (event) => show(event.payload));
window.__TAURI__.core.invoke<Selection | null>("selected").then(show);
window.__TAURI__.event.listen<SessionCards>("cards", (event) => showCards(event.payload));
// Claude Code's version changed: re-record the subagent fixture.
window.__TAURI__.event.listen<string>("notice", (event) => {
  element("notice").textContent = event.payload;
  element("notice").hidden = false;
});
window.__TAURI__.core.invoke<SessionCards>("cards").then(showCards);

export {};
