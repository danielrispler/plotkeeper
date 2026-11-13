// The story rail, on the window's left (danielrispler/Falinks#574). One header line: the verdict and the phase, with the
// times and the model on hover, and a "⋯" menu. Then "now": the open question, docked, or else the
// newest card; the current intention, boxed; the latest turn's summary. Below, a feed of turns,
// newest first, each with its prompt, summary, learning question, cards, questions, subagents,
// checks and folded response, and one "ask about this". A subagent's view shows its own line,
// questions, brief and cards in one block.

import { age, code, element, invoke, pressable, prose, text } from "./dom.js";
import { select } from "./selection.js";
import { askElement } from "./ask.js";
import { rendered } from "./markdown.js";
import { encode, request } from "./daemon.js";
import { agentState, sessionName } from "./list.js";
import { closePane, resumesAuto, startClaude } from "./panes.js";
import { openDecision } from "./answer.js";
import { cardNode } from "./cards.js";
import { showCleared } from "./decisions.js";
import { decisionElement, questionElement } from "./questions.js";
import { shortPaths, worktreeOf } from "./paths.js";
import { checkTime, counts, estimateText, times } from "./times.js";
import { studio } from "./state.js";
import { typeInto } from "./typing.js";
import {
  stateLabels,
  type CheckLine,
  type Losses,
  type Places,
  type Question,
  type Selection,
  type Session,
  type StoredCard,
  type Subagent,
  type Turn,
  type TurnSummary,
} from "./types.js";

export function showView(selected: Selection | null) {
  element("empty").hidden = selected !== null;
  element("view").hidden = selected === null;
  element("rail-more").hidden = selected === null;
  if (selected === null) {
    element("state").textContent = "";
    element("phase").textContent = "";
    element("mode").textContent = "";
    showQuestions([]);
    showCleared([]);
    return;
  }
  const session = selected.session;
  const agent = session.agents.find((a) => a.id === selected.agent) ?? null;
  const [label, state] = agent === null ? [stateLabels[session.state], session.state] : agentState(session, agent);
  const step = (agent ?? session).last_step;
  const stepSpan = (agent ?? session).step_span;
  // One line: the verdict and the phase; the times and the model show on hover, counted at the hover.
  element("state").textContent = label;
  element("state").dataset.state = state;
  element("phase").textContent = agent === null ? estimateText(session.work) : "";
  // A subagent runs in its session's mode.
  element("mode").textContent = modeText(session.permission_mode);
  // The model and effort the session runs at (danielrispler/Falinks#517, decision 8); none for a subagent, whose transcript the studio does not read.
  const { model, effort } = session.transcript;
  const runsAt = agent === null ? [model, effort && `effort ${effort}`].filter(Boolean).join(" · ") : "";
  element("headline").onmouseenter = () => {
    const now = Date.now();
    element("headline").title = [...times(session, stepSpan).map(([, draw]) => draw(now)), runsAt].filter(Boolean).join(" · ");
  };
  element("view").toggleAttribute("data-agent", agent !== null);
  // "Open" resumes an ended session in a new terminal, in its folder, where Claude Code keeps it.
  element("reopen").hidden = agent !== null || session.state !== "ended";
  element("reopen").onclick = () => startClaude(session.cwd, session.resume_id, "", resumesAuto(session)).catch((e) => console.warn(e));
  // Another agent's row (danielrispler/Falinks#560) has no name of its own to keep, nor turns to mark, and comes back
  // with its status if dropped.
  const own = agent === null && !session.status_only;
  element("rename").hidden = !own;
  element("rename").onclick = () => askName(session);
  element("mark-unseen").hidden = !own || session.state !== "your_turn" || session.unseen === true;
  element("mark-unseen").onclick = () => void invoke("mark_unseen", { id: session.id }).catch((e) => console.warn(e));
  // Only a terminal the studio runs can be closed from here.
  element("close").hidden = agent !== null || session.terminal === null;
  element("close").onclick = () => askClose(session);
  element("delete").hidden = !own;
  element("delete").onclick = () => askDelete(session).catch(report);
  element("tool").textContent = session.status_only
    ? "Status only: another agent, with no steps, intention, cards or brief"
    : (step?.tool ?? (agent === null ? session.returned : null) ?? "No step yet");
  const [detail, fullDetail] = shortPaths(step?.detail ?? "");
  element("detail").textContent = detail;
  element("detail").title = fullDetail;
  element("command").textContent = step?.command ?? "";
  element("step").classList.toggle("has-command", Boolean(step?.command));
  showIntention(storyOf(session, agent));
  showQuestions(session.questions.filter((question) => question.agent === (agent?.id ?? null)));
  showCleared(agent === null ? session.cleared : []);
  // A subagent's view has no intention of its own: its written summary takes that line (danielrispler/Falinks#535).
  if ((agent?.id ?? null) !== shownAgent) {
    shownAgent = agent?.id ?? null;
    failed.clear();
  }
  // Opening another session retries its turns whose summary write failed.
  if (session.id !== shownSession) {
    shownSession = session.id;
    failedTurns.clear();
  }
  if (agent !== null) {
    element("intention-text").textContent = agentSummaries.get(agent.id) ?? "";
    if (agent.ended_at !== null) summarize(agent.id);
  }
  drawFeed();
}

// The session's permission mode on the rail's line (danielrispler/Falinks#541), never only on hover, as it is a safety
// fact; nothing for the default mode, or before an event names one.
const modes: Record<string, string> = { auto: "auto mode", plan: "plan mode", acceptEdits: "accept edits", bypassPermissions: "bypass permissions" };
export const modeText = (mode: string | null) => (!mode || mode === "default" ? "" : (modes[mode] ?? mode));

// The subagent summaries the writer gave (danielrispler/Falinks#535), by subagent id, those being written, and those
// whose call failed while the subagent stays open, so a redraw does not ask again.
const agentSummaries = new Map<string, string>();
const writing = new Set<string>();
const failed = new Set<string>();
let shownAgent: string | null = null;
let shownSession: string | null = null;

// Asks the shell for subagent `id`'s summary: the store's, or one writer call now. A failed call
// leaves its name and first line, and the next opening asks again.
function summarize(id: string) {
  if (agentSummaries.has(id) || writing.has(id) || failed.has(id)) return;
  writing.add(id);
  invoke<string | null>("agent_summary", { agentId: id })
    .then((summary) => {
      if (summary === null) return void failed.add(id);
      agentSummaries.set(id, summary);
      if (studio.selection?.agent === id) element("intention-text").textContent = summary;
      else drawFeed();
    })
    .catch((e) => {
      failed.add(id);
      console.warn(e);
    })
    .finally(() => writing.delete(id));
}

export type HistoryEntry = { agent: string; purpose: string; asked: string; result: string; details: string };

// A session's subagent history (danielrispler/Falinks#535): every subagent it ran, newest first, faded ones too. Each
// entry's purpose is its name and its prompt's first line; its result is its written summary, else
// its last message's first line, or its status when it failed or has none yet.
export function subagentHistory(session: Pick<Session, "agents">, summaries: Map<string, string>, now = Date.now()): HistoryEntry[] {
  return [...session.agents]
    .sort((a, b) => b.started_at - a.started_at)
    .map((a) => ({
      agent: a.id,
      purpose: a.name,
      asked: a.asked ?? "",
      result: summaries.get(a.id) ?? ((a.failed ? null : a.result) || a.status),
      details: [a.agent_type, age(a.started_at, a.ended_at ?? now), a.kind].filter(Boolean).join(" · "),
    }));
}

// A subagent's entry in the turn that ran it; its click opens that subagent.
function historyElement(session: string, entry: HistoryEntry): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => select(session, entry.agent));
  button.classList.add("subagent");
  const lines = [text("span", entry.purpose, "asked"), ...(entry.asked ? [prose("span", entry.asked)] : [])];
  button.append(...lines, prose("span", entry.result), text("span", entry.details, "kind"));
  const item = document.createElement("li");
  item.append(button);
  return item;
}

// After this many steps the intention dims, as it may no longer say what the agent does (danielrispler/Falinks#472).
const FRESH_STEPS = 5;

// The intention, boxed as the anchor of "now" (mockup C), its age, and the mark when the transcript is
// unreadable.
function showIntention(story: Session["transcript"] | null) {
  const steps = story?.steps_ago ?? 0;
  element("intention-text").textContent = story?.intention ?? "";
  element("intention-age").textContent = steps === 0 ? "" : `· ${steps} ${steps === 1 ? "step" : "steps"} ago`;
  element("intention").classList.toggle("old", steps > FRESH_STEPS);
  element("unavailable").hidden = !story?.unavailable;
  element("unavailable-why").textContent = story?.unavailable ? `The transcript cannot be read: ${story.unavailable}.` : "";
}

// The transcript the rail tells: none for a subagent's, which the studio does not read, nor for
// another agent's status-only row (danielrispler/Falinks#560).
export const storyOf = (session: Session, agent: unknown) => (agent === null && !session.status_only ? session.transcript : null);

// What the rail draws for `selected`: "now"'s card and latest turn, and the feed. A status-only row
// (danielrispler/Falinks#560) has no turns or cards to show.
export function railOf({ session, agent }: Selection, docked: boolean, all: StoredCard[]) {
  if (session.status_only) return { top: null, latest: null, blocks: [] };
  const top = railTop(docked, all);
  const cards = all.filter((card) => card !== top);
  const own = agent === null;
  const turns = own ? session.turns : [];
  const blocks = feedTurns(turns, {
    cards,
    // A subagent's view: its session's checks and subagents are not its own.
    checks: own ? session.checks : [],
    agents: own ? session.agents : [],
    questions: session.questions.filter((question) => question.agent === agent),
  });
  return { top, latest: own ? (turns.at(-1) ?? null) : null, blocks };
}

// When the event card `id` was written at: the part after its `@`, in Unix ms (the core's `card_time`).
export const cardTime = (id: string) => Number(id.slice(id.lastIndexOf("@") + 1)) || 0;

// The index of the turn that runs at `at`: the latest that started at or before it, else null.
export function turnAt(turns: Pick<Turn, "started_at">[], at: number): number | null {
  let index = turns.length - 1;
  while (index >= 0 && turns[index].started_at > at) index--;
  return index < 0 ? null : index;
}

// A block of the feed: turn `index` and what started in it, newest first. `index` is null for the
// block of what started in no turn, such as a subagent's view, whose session's turns are not its own.
export type FeedTurn = {
  index: number | null;
  turn: Turn | null;
  cards: StoredCard[];
  checks: CheckLine[];
  agents: Subagent[];
  questions: Question[];
};

// The feed (danielrispler/Falinks#574): one block per turn, newest first, each holding the cards written in it (by their
// id's time), and the checks, subagents and questions the core put in it. What has no turn goes in
// one last block, so nothing is lost.
export function feedTurns(
  turns: Turn[],
  { cards, checks, agents, questions }: Pick<FeedTurn, "cards" | "checks" | "agents" | "questions">,
): FeedTurn[] {
  const blocks: FeedTurn[] = [...turns.map((turn, index) => ({ index, turn })), { index: null, turn: null }].map((b) => ({
    ...b,
    cards: [],
    checks: [],
    agents: [],
    questions: [],
  }));
  const at = (index: number | null) => blocks[index !== null && index < turns.length ? index : turns.length];
  // Each list is oldest first; the blocks show the newest first.
  for (const card of [...cards].reverse()) at(turnAt(turns, cardTime(card.request.id))).cards.push(card);
  for (const check of [...checks].reverse()) at(check.turn).checks.push(check);
  for (const agent of [...agents].sort((a, b) => b.started_at - a.started_at)) at(agent.turn).agents.push(agent);
  for (const question of [...questions].reverse()) at(question.turn).questions.push(question);
  const rest = blocks.pop()!;
  const empty = !rest.cards.length && !rest.checks.length && !rest.agents.length && !rest.questions.length;
  return [...blocks.reverse(), ...(empty ? [] : [rest])];
}

// What the rail starts with: the open question, docked, when one is; else the newest card, which its
// turn then leaves out. `cards` is oldest first.
export function railTop(docked: boolean, cards: StoredCard[]): StoredCard | null {
  return docked ? null : (cards.at(-1) ?? null);
}

// The turn summaries the store gave (danielrispler/Falinks#526), by turn id, null for a turn with none yet, and those
// being asked for. With summaries written on open, asking writes a waiting turn's summary, a
// `claude -p` call, so the asks go one at a time (danielrispler/Falinks#574). Those whose write failed wait for the next
// opening of their session, not the next redraw.
const summaries = new Map<string, TurnSummary | null>();
const asking = new Set<string>();
const failedTurns = new Set<string>();

// A queue that runs `run` on each pushed id in order, one in flight at a time.
export function oneAtATime(run: (id: string) => Promise<void>): (id: string) => void {
  const queue: string[] = [];
  let busy = false;
  const next = () => {
    const id = busy ? undefined : queue.shift();
    if (id === undefined) return;
    busy = true;
    void run(id).finally(() => {
      busy = false;
      next();
    });
  };
  return (id) => {
    queue.push(id);
    next();
  };
}

const askSummary = oneAtATime((id) =>
  invoke<TurnSummary | null>("turn_summary", { id })
    .then((summary) => void summaries.set(id, summary))
    .catch((e) => {
      failedTurns.add(id);
      console.warn(e);
    })
    .finally(() => {
      asking.delete(id);
      drawFeed();
    }),
);

// Turn `id`'s summary, asked for once; the answer draws the feed again. The feed asks for the newest
// turn first, so its summary is written first.
function summaryOf(id: string): TurnSummary | null {
  if (summaries.has(id) || asking.has(id) || failedTurns.has(id)) return summaries.get(id) ?? null;
  asking.add(id);
  askSummary(id);
  return null;
}

// A new card store revision may hold a summary that was missing: ask again for those.
export function forgetMissingSummaries() {
  for (const [id, summary] of summaries) if (summary === null) summaries.delete(id);
}

// The folded blocks Daniel opened, by key, which outlast the feed's redraws, and the older turns'
// replies, fetched at their first opening.
const opened = new Set<string>();
const replies = new Map<string, string>();
// The option Daniel picked on each turn's learning question, by turn id; kept in the window only.
const learned = new Map<string, number>();

// A folded block keyed `key`, open while Daniel keeps it open; `onOpen` runs at each opening.
function folded(key: string, title: string, body: HTMLElement[], onOpen?: () => void): HTMLElement {
  const node = document.createElement("details");
  node.className = "fold";
  node.open = opened.has(key);
  node.append(text("summary", title), ...body);
  node.addEventListener("toggle", () => {
    if (node.open) opened.add(key);
    else opened.delete(key);
    if (node.open) onOpen?.();
  });
  return node;
}

// The learning section (danielrispler/Falinks#526): the turn's main idea and one question, graded at the tap by the
// writer's key, each option's reason shown after it.
function learningElement(id: string, learning: NonNullable<TurnSummary["summary"]["learning"]>): HTMLElement {
  const picked = learned.get(id) ?? null;
  const { question } = learning;
  const options = document.createElement("ol");
  options.className = "options";
  options.append(
    ...question.options.map((option, i) => {
      const item = document.createElement("li");
      const button = pressable(prose("button", option.text) as HTMLButtonElement, picked === i, () => {
        learned.set(id, i);
        drawFeed();
      });
      button.disabled = picked !== null;
      item.append(button);
      if (picked !== null) {
        const why = prose("p", option.why);
        if (i === question.key) why.prepend(Object.assign(text("span", "The right answer. "), { dir: "ltr" }));
        item.append(why);
      }
      return item;
    }),
  );
  const box = document.createElement("div");
  box.className = "card learning";
  const quiz = prose("p", question.text);
  quiz.className = "quiz";
  box.append(prose("p", learning.idea), quiz, options);
  return folded(`${id} learning`, "Learning: the main idea, and one question", [box]);
}

// The latest turn's summary, at the top, and the newest card when no question is docked.
function drawNow(session: Session | null, latest: Turn | null, top: StoredCard | null) {
  const summary = latest?.id ? summaryOf(latest.id) : null;
  const cwd = session?.cwd ?? null;
  element("now-summary").replaceChildren(...(summary ? [rendered("div", summary.summary.summary, "summary", cwd)] : []));
  element("now-card").replaceChildren(...(top ? [cardNode(top)] : []));
}

// A check line: its outcome, its command, and how long it took or, while it runs, its time against
// its typical time. No colour, as colour is only for "needs you".
function checkElement(line: CheckLine): HTMLElement {
  const item = document.createElement("li");
  const label = line.ended_at === null ? "Running" : line.passed === null ? "Ran" : line.passed ? "Passed" : "Failed";
  const time = counts(text("span", "", "check-time"), (now) => checkTime(line, now));
  item.append(text("span", label, "outcome"), code(line.command), time);
  return item;
}

// One block of the feed. The newest turn's summary is at the top, so its block leaves it out.
function turnElement(session: Session, block: FeedTurn, newest: boolean): HTMLElement {
  const { turn, index } = block;
  const node = document.createElement("li");
  node.className = "turn";
  const key = `${session.id} ${index ?? "rest"}`;
  if (turn) {
    const prompt = text("p", turn.prompt || "No prompt", "prompt");
    prompt.title = turn.prompt;
    node.append(prompt);
  }
  const summary = turn?.id ? summaryOf(turn.id) : null;
  if (summary && !newest) node.append(rendered("div", summary.summary.summary, "summary", session.cwd));
  if (summary?.summary.learning && turn?.id) node.append(learningElement(turn.id, summary.summary.learning));
  node.append(...block.cards.map(cardNode));
  node.append(...block.questions.filter((q) => q.answer === "not_answered").map(questionElement));
  if (block.agents.length) {
    const list = document.createElement("ol");
    list.className = "subagents";
    const entries = subagentHistory({ agents: block.agents }, agentSummaries);
    list.append(...entries.map((entry) => historyElement(session.id, entry)));
    node.append(list);
  }
  if (block.checks.length) {
    const list = document.createElement("ul");
    list.className = "checks";
    list.append(...block.checks.map(checkElement));
    node.append(list);
  }
  // The full response, folded: the latest turn's from the transcript, an older one's fetched at its
  // first opening.
  const latest = index !== null && index === session.turns.length - 1;
  const responses = latest ? session.transcript.responses : [];
  if (turn && (latest ? responses.length : turn.id)) {
    const body = document.createElement("div");
    const fill = (texts: string[]) => body.replaceChildren(...texts.map((t) => rendered("div", t, "response", session.cwd)));
    const fetched = replies.get(key);
    if (latest) fill(responses);
    else if (fetched !== undefined) fill([fetched]);
    const fetch = () => {
      if (latest || replies.has(key)) return;
      invoke<string | null>("turn_reply", { id: session.id, index })
        .then((reply) => {
          replies.set(key, reply ?? "");
          fill([reply ?? ""]);
        })
        .catch((e) => console.warn(e));
    };
    const fold = folded(`${key} response`, "Full response", [body], fetch);
    if (fold.hasAttribute("open")) fetch();
    node.append(fold);
  }
  // One "ask about this" per turn, shown on hover; it asks about the turn's summary, else its reply.
  const about = summary?.summary.summary || (latest ? responses.join("\n\n") : "") || turn?.message || "";
  if (about) node.append(askElement(session.id, about));
  return node;
}

// Draws "now" and the feed from the selection and the cards the shell last sent.
export function drawFeed() {
  const selected = studio.selection;
  const { recent, older } = studio.cards;
  const all = [...older, ...recent];
  const docked = !element("dock").hidden;
  if (selected === null) {
    drawNow(null, null, null);
    element("feed").replaceChildren();
    return;
  }
  const session = selected.session;
  const own = selected.agent === null;
  const { top, latest, blocks } = railOf(selected, docked, all);
  drawNow(session, latest, top);
  element("feed").replaceChildren(...blocks.map((block, i) => turnElement(session, block, own && i === 0 && block.index !== null)));
}
// Rename (danielrispler/Falinks#469): the row's name, kept in the event log, and `/rename` typed into the session while
// its claude runs, so Claude Code shows the same name. An empty name gives the row its folder's again.
// The command waits for Daniel's draft and never answers a session that needs him (danielrispler/Falinks#544).
function askName(session: Session) {
  const dialog = element("rename-dialog") as HTMLDialogElement;
  const field = element("rename-field") as HTMLInputElement;
  field.value = session.name ?? "";
  dialog.onclose = async () => {
    if (dialog.returnValue !== "rename") return;
    const name = field.value.trim();
    await invoke("rename", { id: session.id, name }).catch((e) => console.warn(e));
    if (!name || session.terminal === null || session.state === "ended") return;
    const terminal = session.terminal;
    const blocked = () => studio.listed.find((s) => s.id === session.id)?.state === "needs_you";
    await typeInto(terminal, `/rename ${name}`, blocked, {
      now: Date.now,
      sleep: (ms) => new Promise((done) => setTimeout(done, ms)),
      send: (data) => request({ op: "input", id: terminal, data: encode(new TextEncoder().encode(data)) }),
      notice: (text) => {
        element("notice").textContent = text;
        element("notice").hidden = false;
      },
    }).catch((e) => console.warn(e));
  };
  dialog.returnValue = "";
  dialog.showModal();
}

// Close (danielrispler/Falinks#469): ends the session's terminal, asking first while it works or has background tasks.
function askClose(session: Session) {
  const close = () => closePane(session.terminal!).catch((e) => console.warn(e));
  if (!session.asks_before_closing) return close();
  const dialog = element("close-dialog") as HTMLDialogElement;
  element("close-name").textContent = sessionName(session);
  dialog.onclose = () => dialog.returnValue === "close" && close();
  dialog.returnValue = "";
  dialog.showModal();
}

// Shows a failed delete's reason over the story.
function report(error: unknown) {
  element("notice").textContent = String(error);
  element("notice").hidden = false;
}

// Opens `dialog` and resolves to the button Daniel chose, "" for Esc.
function choice(dialog: HTMLDialogElement): Promise<string> {
  return new Promise((resolve) => {
    dialog.onclose = () => resolve(dialog.returnValue);
    dialog.returnValue = "";
    dialog.showModal();
  });
}

// Delete: ends the session's terminal, removes its worktree when ticked, asking first when that would
// lose work, and its branch when the main checkout holds it, then drops its row. A branch git keeps
// is reported, and the row goes anyway.
async function askDelete(session: Session) {
  const dialog = element("delete-dialog") as HTMLDialogElement;
  const worktree = worktreeOf(session.cwd);
  const ticked = element("delete-worktree") as HTMLInputElement;
  ticked.checked = false;
  element("delete-name").textContent = sessionName(session);
  element("delete-working").hidden = !(session.terminal !== null && session.asks_before_closing);
  element("delete-worktree-option").hidden = worktree === null;
  element("delete-worktree-path").textContent = worktree ?? "";
  const others = studio.listed.filter((s) => s.id !== session.id && s.state !== "ended" && worktreeOf(s.cwd) === worktree);
  element("delete-shared").hidden = worktree === null || others.length === 0;
  if ((await choice(dialog)) !== "delete") return;
  if (session.terminal !== null) await closePane(session.terminal);
  if (worktree !== null && ticked.checked) await removeWorktree(worktree);
  await invoke("delete_session", { id: session.id });
}

// The branch is git's, read before the worktree goes: a session with no turn yet has none in its
// transcript.
export async function removeWorktree(worktree: string) {
  const { tasks } = await invoke<Places>("places");
  const branch = tasks.find((task) => task.path === worktree)?.branch ?? null;
  const losses = await invoke<Losses | null>("remove_task", { path: worktree, confirmed: false });
  if (losses) {
    const list = (title: string, lines: string[]) => {
      if (!lines.length) return [];
      const items = document.createElement("ul");
      items.append(...lines.map((line) => text("li", line, "mono")));
      return [text("p", title), items];
    };
    element("delete-losses-list").replaceChildren(
      ...list("Uncommitted changes", losses.uncommitted),
      ...list("Commits not on origin (they stay on the branch)", losses.unpushed),
    );
    if ((await choice(element("delete-losses") as HTMLDialogElement)) !== "remove") return;
    await invoke("remove_task", { path: worktree, confirmed: true });
  }
  if (branch === null) return;
  await invoke("delete_merged_branch", { branch }).catch((e) => report(`Branch ${branch} kept: ${e}`));
}


// The open question of the viewed agent, docked at the top of the rail, newest first; a plan or a
// permission the answer hook waits on docks too, whichever agent asks (danielrispler/Falinks#478). A question never
// answered shows in its turn; an answered one is in the decision history (danielrispler/Falinks#479).
function showQuestions(questions: Question[]) {
  const open = [...questions].reverse().filter((question) => question.answer === "open");
  const decision = openDecision();
  const docked = [...(decision ? [decisionElement(decision)] : []), ...open.map(questionElement)];
  element("dock").replaceChildren(...docked);
  element("dock").hidden = docked.length === 0;
}
