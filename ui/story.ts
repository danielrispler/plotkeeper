// The story rail, on the window's left (danielrispler/Falinks#574). A header of two lines (#444): the agent's logo, the
// session's name and a "⋯" menu, then the verdict, the mode and the phase, with the times and the model on hover. Then "now": the open question, docked, or else the
// newest summary; the current intention, boxed; the latest turn's summary. Below, a feed of turns,
// newest first, each with its prompt, summary, questions, subagents,
// checks and folded response. "Ask" sits on a file, a failed check, a subagent line and the Headline. A subagent's view shows its own line,
// questions and brief in one block.

import { modelLine, reports } from "./capabilities.js";
import { logo, logoKey, noContextNote } from "./logos.js";
import { age, code, element, failure, fullTitle, glyphOnly, invoke, pressable, prose, text, unseenWords } from "./dom.js";
import { select } from "./selection.js";
import { askAnchor } from "./ask.js";
import { rendered } from "./markdown.js";
import { agentState, drawSidebar, sessionName, showsLanded } from "./list.js";
import { blockedRequestFor, openDecision, type Ask } from "./answer.js";
import { blockedElement, docksInDock, fillCardRisk, retryLine, riskLine } from "./blocked.js";
import { briefNodes } from "./cards.js";
import { type Banner, bannerElement, bannerFor, bannerLives } from "./banner.js";
import { evidenceElement, hasEvidence } from "./evidence.js";
import { railBriefRow } from "./rail-brief.js";
import { showCleared } from "./decisions.js";
import { decisionElement, questionElement } from "./questions.js";
import { shortPaths } from "./paths.js";
import { checkTime, counts, estimateText, times } from "./times.js";
import { studio } from "./state.js";
import { showError } from "./notice.js";
import { currentMap } from "./map.js";
import { pageStore, stretches, tipText } from "./tips.js";
import {
  askKind,
  stateLabels,
  type AskKind,
  type BlockedCommand,
  type ListedCheck,
  type Question,
  type Selection,
  type SessionState,
  type Session,
  type Subagent,
  type Turn,
  type SummaryLine,
  type TurnSummary,
} from "./types.js";

// biome-ignore lint/complexity/noExcessiveLinesPerFunction: `showView`: spec #548 Group 4 splits it; remove this when it does
export function showView(selected: Selection | null) {
  element("empty").hidden = selected !== null;
  element("view").hidden = selected === null;
  element("rail-more").hidden = selected === null;
  if (selected === null) {
    for (const id of ["state", "block", "mode", "mark", "session-name", "phase"]) element(id).textContent = "";
    for (const id of ["session-name", "block", "phase"]) element(id).removeAttribute("title");
    showQuestions([], []);
    showCleared([]);
    shownSession = null;
    banner = null;
    return;
  }
  const session = selected.session;
  const agent = session.agents.find((a) => a.id === selected.agent) ?? null;
  const [label, state] = agent === null ? [stateLabels[session.state], session.state] : agentState(session, agent);
  const step = (agent ?? session).last_step;
  const stepSpan = (agent ?? session).step_span;
  // Line 1: the agent's mark and the session's name, whole: CSS ends it in "…" when it is too long, and the full name is its
  // hover text (#444). Line 2: the verdict, the kind of block when the session needs Daniel, the permission mode and, only
  // while it runs, the estimate. The times and the model show on hover, counted at the hover.
  // A landed task's session at your turn, idle or ended says so in words (#53, Landed).
  element("state").textContent = agent === null && showsLanded(state, session.landed) ? "Landed" : label;
  element("state").dataset.state = state;
  const open = session.questions.filter((question) => question.agent === (agent?.id ?? null));
  element("block").textContent = (state === "needs_you" ? blockKind(open, openDecision()) : null) ?? "";
  // A subagent runs in its session's mode.
  element("mode").textContent = reports(session, "permission_mode") ? modeText(session.permission_mode) : "";
  // The agent's logo, titled with its model line when it has one (#158); agy's keeps its note that it has no context size.
  const key = logoKey(session);
  const title = [agent === null ? modelLine(session) : "", noContextNote(session)].filter(Boolean).join("\n");
  element("mark").replaceChildren(logo(key, title || undefined));
  for (const id of ["block", "phase"]) element(id).removeAttribute("title");
  const name = sessionName(session);
  element("session-name").textContent = name;
  element("session-name").title = name;
  element("phase").textContent = agent === null && state === "running" ? estimateText(session.work) : "";
  // The block's kind and the estimate may end in "…" at a narrow rail: each is its own hover text.
  for (const id of ["block", "phase"]) fullTitle(element(id), element(id).textContent);
  // The model and effort the session runs at (danielrispler/Falinks#517, decision 8); none for a subagent, whose transcript the studio does not read.
  const effort = session.transcript.effort;
  const runsAt = agent === null ? [modelLine(session), effort && `effort ${effort}`].filter(Boolean).join(" · ") : "";
  element("headline").onmouseenter = () => {
    const now = Date.now();
    element("headline").title = [...times(session, stepSpan).map(([, draw]) => draw(now)), runsAt].filter(Boolean).join(" · ");
  };
  element("view").toggleAttribute("data-agent", agent !== null);
  // Another agent's row (danielrispler/Falinks#560) has no name of its own to keep, nor turns to mark, and comes back
  // with its status if dropped.
  const own = agent === null && !session.status_only;
  element("rename").hidden = !own;
  element("rename").onclick = () => askName(session);
  element("mark-unseen").hidden = !own || session.state !== "your_turn" || session.unseen === true;
  element("mark-unseen").onclick = () => void invoke("mark_unseen", { id: session.id }).catch(failure("mark_unseen", "error"));
  element("delete").hidden = !own;
  element("delete").onclick = () => askDelete(session).catch(report);
  element("tool").textContent = session.shell
    ? "A plain terminal"
    : session.status_only
    ? "Status only: another agent, with no steps, intention or brief"
    : (step?.tool ?? (agent === null ? session.returned : null) ?? "No step yet");
  const [detail, fullDetail] = shortPaths(step?.detail ?? "");
  element("detail").textContent = detail;
  element("detail").removeAttribute("title");
  fullTitle(element("detail"), fullDetail);
  element("command").textContent = step?.command ?? "";
  element("step").classList.toggle("has-command", Boolean(step?.command));
  // Without a command the step has nothing to unfold, but its line keeps its hover, the full path among it (#220).
  const stepLine = element("step-line");
  stepLine.title = step?.command ? "Show or hide the step's full command" : "The step the agent is on";
  stepLine.onclick = (event) => void (step?.command || event.preventDefault());
  showIntention(storyOf(session, agent));
  showTip(session);
  drawDock(selected);
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
    // "Where we were" is decided as the session is selected, and kept while Daniel reads it (danielrispler/plotkeeper#124).
    banner = bannerFor(session, Date.now());
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
const modeText = (mode: string | null) => (!mode || mode === "default" ? "" : (modes[mode] ?? mode));

// The subagent summaries the writer gave (danielrispler/Falinks#535), by subagent id, those being written, and those
// whose call failed while the subagent stays open, so a redraw does not ask again.
const agentSummaries = new Map<string, string>();
const writing = new Set<string>();
const failed = new Set<string>();
let shownAgent: string | null = null;
let shownSession: string | null = null;
let banner: Banner | null = null;

// Asks the shell for subagent `id`'s summary: the store's, or one writer call now. A failed call
// leaves its name and first line, and the next opening asks again.
function summarize(id: string) {
  if (agentSummaries.has(id) || writing.has(id) || failed.has(id)) return;
  writing.add(id);
  invoke("agent_summary", { agentId: id })
    .then((summary) => {
      if (summary === null) return void failed.add(id);
      agentSummaries.set(id, summary);
      if (studio.selection?.agent === id) element("intention-text").textContent = summary;
      else drawFeed();
    })
    .catch((e) => {
      failed.add(id);
      failure("subagent summary")(e);
    })
    .finally(() => writing.delete(id));
}

export type HistoryEntry = { agent: string; purpose: string; asked: string; result: string; details: string };

// A session's subagent history (danielrispler/Falinks#535): every subagent it ran, newest first, faded ones too. Each
// entry's purpose is its name and its prompt's first line; its result is its written summary, else
// its last message's first line, or its status when it failed or has none yet.
export function subagentHistory(session: Pick<Session, "agents" | "capabilities">, summaries: Map<string, string>, now = Date.now()): HistoryEntry[] {
  return [...session.agents]
    .sort((a, b) => b.started_at - a.started_at)
    .map((a) => ({
      agent: a.id,
      purpose: a.name,
      asked: a.asked ?? "",
      result: summaries.get(a.id) ?? ((a.failed || !reports(session, "replies") ? null : a.result) || a.status),
      details: [a.agent_type, age(a.started_at, a.ended_at ?? now), a.kind].filter(Boolean).join(" · "),
    }));
}

// A subagent's entry in the turn that ran it; its click opens that subagent.
function historyElement(session: string, entry: HistoryEntry): HTMLElement {
  const button = pressable(document.createElement("button"), null, () => void select(session, entry.agent));
  button.classList.add("subagent");
  const lines = [text("span", entry.purpose, "asked"), ...(entry.asked ? [prose("span", entry.asked)] : [])];
  button.append(...lines, prose("span", entry.result), text("span", entry.details, "kind"));
  fullTitle(button, [entry.purpose, entry.asked, entry.result, entry.details].filter(Boolean).join(" · "));
  const item = document.createElement("li");
  item.append(button, askAnchor(session, { kind: "subagent", agent: entry.agent }, `the subagent ${entry.purpose}`));
  return item;
}

// The kind of block a session needs Daniel for: a question, else a plan or permission by its tool; null when
// none is open.
export function blockKind(questions: Pick<Question, "answer">[], decision: Pick<Ask, "tool_name"> | null): AskKind | null {
  if (decision) return askKind(decision.tool_name);
  return questions.some((question) => question.answer === "open") ? "question" : null;
}

// A turn's Headline and Body lines (danielrispler/plotkeeper#115, #116): the writer's, when the summary has them; a summary
// stored before them is adapted, its first line the Headline and the rest `reported` lines. Until the summary arrives, for a
// turn with none, and when the guards stripped its Headline (empty), the Headline is the first line of the final response.
export type BodyLine = SummaryLine;
export function headlineOf(summary: TurnSummary | null, message: string | null): { headline: string | null; body: BodyLine[] } {
  const stored = summary?.summary;
  if (stored && stored.headline != null) {
    return { headline: stored.headline.trim() || message?.trim() || null, body: stored.lines ?? [] };
  }
  const lines = (summary?.summary.summary ?? "").split("\n").map((line) => line.trim()).filter(Boolean);
  if (lines[0]) return { headline: lines[0], body: lines.slice(1).map((text) => ({ kind: "reported", text })) };
  return { headline: message?.trim() || null, body: [] };
}

// A folded turn's line (danielrispler/plotkeeper#117): Daniel's prompt (its first line, which the style cuts to fit) and the
// Headline, with one mark: a check that ended failed. A check's status is its last run's, so a failure the agent fixed leaves no mark.
export type FoldedLine = { prompt: string; headline: string | null; failed: boolean };
export function foldedLine(turn: Pick<Turn, "prompt" | "checks">, headline: string | null): FoldedLine {
  const prompt = turn.prompt.split("\n").map((line) => line.trim()).find(Boolean) ?? "No prompt";
  return { prompt, headline, failed: turn.checks.some((check) => check.status === "failed") };
}

// What "now" holds in each state: one thing per state. While running, what Daniel asked and the intention, boxed.
// At your turn (and idle or ended), what he asked, the Headline, the Body lines and the full response, with no
// intention box. When the session needs him, the intention small as "why it asks", and the turn's facts so far.
export type Now = { asked: string | null; intention: "box" | "why" | null; headline: string | null; body: BodyLine[]; soFar: boolean; fullResponse: boolean };
export function nowOf(
  state: SessionState,
  turn: { prompt: string; message: string | null },
  summary: TurnSummary | null,
): Now {
  if (state === "running") return { asked: turn.prompt, intention: "box", headline: null, body: [], soFar: false, fullResponse: false };
  if (state === "needs_you") return { asked: null, intention: "why", headline: null, body: [], soFar: true, fullResponse: false };
  return { asked: turn.prompt, intention: null, ...headlineOf(summary, turn.message), soFar: false, fullResponse: true };
}

// The Wait tip (#523): one per Running stretch of the session, at the foot of its Intention box; gone at any other state.
const tipFor = stretches(pageStore);
function showTip(session: Session) {
  const map = currentMap();
  const context = { map: map !== null, brief: (map?.state.ready ?? []).length > 0, trees: reports(session, "turn_trees") };
  const tip = tipFor(session, studio.listed, context, studio.files.settings.tips);
  const line = tip ? tipText(tip, studio.files.keymap) : "";
  Object.assign(element("tip"), { textContent: line, title: line, hidden: tip === null });
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

// What the rail draws for `selected`: its latest turn, which "now" tells, and the feed. A status-only row
// (danielrispler/Falinks#560) has no turns to show.
// `fed`: the blocked commands the dock does not hold, which the feed places (danielrispler/plotkeeper#297).
export function railOf({ session, agent }: Selection, fed: BlockedCommand[] = []) {
  if (session.status_only) return { latest: null, blocks: [] };
  const own = agent === null;
  const turns = own ? session.turns : [];
  const blocks = feedTurns(turns, {
    // A subagent's view: its session's subagents are not its own.
    agents: own ? session.agents : [],
    questions: session.questions.filter((question) => question.agent === agent),
    blocked: fed,
  });
  return { latest: own ? (turns.at(-1) ?? null) : null, blocks };
}

// When the event `id` was written at: the part after its `@`, in Unix ms (the core's `card_time`). A design line's citation may name a
// card stored before cards were retired.
export const cardTime = (id: string) => Number(id.slice(id.lastIndexOf("@") + 1)) || 0;

// The index of the turn that runs at `at`: the latest that started at or before it, else null.
export function turnAt(turns: Pick<Turn, "started_at">[], at: number): number | null {
  let index = turns.length - 1;
  while (index >= 0 && (turns[index]?.started_at ?? 0) > at) index--;
  return index < 0 ? null : index;
}

// A block of the feed: turn `index` and what started in it, newest first. `index` is null for the
// block of what started in no turn, such as a subagent's view, whose session's turns are not its own.
export type FeedTurn = {
  index: number | null;
  turn: Turn | null;
  agents: Subagent[];
  questions: Question[];
  // The blocked commands with no button, in the turn of their latest step (danielrispler/plotkeeper#297).
  blocked: BlockedCommand[];
};

// The feed (danielrispler/Falinks#574): one block per turn, newest first, each holding the subagents and questions the core put in it.
// What has no turn goes in one last block, so nothing is lost.
export function feedTurns(
  turns: Turn[],
  { agents, questions, blocked = [] }: Pick<FeedTurn, "agents" | "questions"> & { blocked?: BlockedCommand[] },
): FeedTurn[] {
  const blocks: FeedTurn[] = [...turns.map((turn, index) => ({ index, turn })), { index: null, turn: null }].map((b) => ({
    ...b,
    agents: [],
    questions: [],
    blocked: [],
  }));
  const rest = blocks[turns.length] ?? { index: null, turn: null, agents: [], questions: [], blocked: [] };
  const at = (index: number | null) => (index !== null && blocks[index]) || rest;
  // Each list is oldest first; the blocks show the newest first.
  for (const agent of [...agents].sort((a, b) => b.started_at - a.started_at)) at(agent.turn).agents.push(agent);
  for (const question of [...questions].reverse()) at(question.turn).questions.push(question);
  // A blocked command's card is in the turn running when its retry started, else when it was blocked.
  for (const card of [...blocked].reverse()) at(turnAt(turns, card.run_at ?? card.at)).blocked.push(card);
  blocks.pop();
  const empty = !rest.agents.length && !rest.questions.length && !rest.blocked.length;
  return [...blocks.reverse(), ...(empty ? [] : [rest])];
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
    run(id)
      .catch(failure("queued run"))
      .finally(() => {
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
  invoke("turn_summary", { id })
    .then((summary) => {
      summaries.set(id, summary);
    })
    .catch((e) => {
      failedTurns.add(id);
      failure("turn summary")(e);
    })
    .finally(() => {
      asking.delete(id);
      drawFeed();
      // The list's unseen rows show this Headline (#382).
      drawSidebar();
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

// The Headline of a session's latest turn, for the list's unseen rows; null when it has no turn.
export function lastHeadline(session: Pick<Session, "turns">): string | null {
  const turn = session.turns.at(-1);
  return turn ? headlineOf(turn.id ? summaryOf(turn.id) : null, turn.message).headline : null;
}

// A new card store revision may hold a summary that was missing: ask again for those.
export function forgetMissingSummaries() {
  for (const [id, summary] of summaries) if (summary === null) summaries.delete(id);
}

// The folded blocks Daniel opened, by key, which outlast the feed's redraws, and the older turns'
// replies, fetched at their first opening.
const opened = new Set<string>();
const replies = new Map<string, string>();

// A folded block keyed `key`, open while Daniel keeps it open; `onOpen` runs at each opening. `hover` says what it holds.
function folded(key: string, title: string, hover: string, body: HTMLElement[], onOpen?: () => void): HTMLElement {
  const node = document.createElement("details");
  node.className = "fold";
  node.open = opened.has(key);
  node.append(fullTitle(text("summary", title), hover), ...body);
  node.addEventListener("toggle", () => {
    if (node.open) opened.add(key);
    else opened.delete(key);
    if (node.open) onOpen?.();
  });
  return node;
}

// "Now" (danielrispler/plotkeeper#116): what the latest turn's state puts above the feed. While running, what Daniel asked;
// the intention's box shows while running, small as "why it asks" when the session needs him, and not at all once
// the turn ended. A subagent's view and a turn-less session keep the box.
function drawNow(session: Session | null, latest: Turn | null): Now | null {
  const now = session && latest ? nowOf(session.state, latest, latest.id ? summaryOf(latest.id) : null) : null;
  const asked = now && session?.state === "running" ? [text("p", "You asked", "label"), text("p", latest?.prompt || "No prompt", "prompt")] : [];
  element("now-asked").replaceChildren(...asked);
  element("intention").hidden = now !== null && now.intention === null;
  element("intention-label").hidden = now?.intention !== "why";
  return now;
}

// A check line: its outcome, its command, and how long it took or, while it runs, its time against
// its typical time. No colour, as colour is only for "needs you".
export function checkElement(line: ListedCheck): HTMLElement {
  const item = document.createElement("li");
  const label = line.status === "running" ? "Running" : "Failed";
  const time = counts(text("span", "", "check-time"), (now) => checkTime(line, now));
  item.append(text("span", label, "outcome"), fullTitle(code(line.statement), line.statement), time);
  return item;
}

// One block of the feed. The newest turn's summary is at the top, so its block leaves it out.
// biome-ignore lint/complexity/noExcessiveLinesPerFunction: `turnContent`: spec #548 Group 4 splits it; remove this when it does
function turnContent(session: Session, block: FeedTurn, now: Now | null): HTMLElement {
  const { turn, index } = block;
  const node = document.createElement("li");
  node.className = now ? "turn now-turn" : "turn";
  const key = `${session.id} ${index ?? "rest"}`;
  // While running, "now" holds what Daniel asked.
  if (turn && !(now && session.state === "running")) {
    const prompt = fullTitle(text("p", turn.prompt || "No prompt", "prompt"), turn.prompt || "No prompt");
    node.append(...(now?.asked ? [text("p", "You asked", "label")] : []), prompt);
  }
  if (now?.soFar) node.append(text("p", "So far this turn", "label"));
  const summary = turn?.id ? summaryOf(turn.id) : null;
  const bodyLine = (line: BodyLine) =>
    // A `reported` line is the agent's claim, which no recorded fact shows.
    rendered("li", line.kind === "reported" ? `Agent says: ${line.text}` : line.text, line.kind);
  // An earlier turn's Body is derived as the folded line's Headline is, which that line already shows.
  const lines = (now ?? (turn ? headlineOf(summary, turn.message) : null))?.body ?? [];
  if (now) {
    if (now.headline) {
      node.append(rendered("p", now.headline, "headline-text"));
      if (index !== null) node.append(askAnchor(session.id, { kind: "headline", turn: index }, "this turn's Headline"));
    }
  }
  // The evidence line, under the Headline: a running turn shows only a check that ended failed.
  if (turn && index !== null && hasEvidence(turn, session.capabilities)) {
    const line = evidenceElement(session.id, index, turn, { only: session.state === "running" && now ? "failed" : "all" });
    if (line) node.append(line);
  }
  // A PR brief waiting for this session's PR: one quiet row in the newest turn, which opens it on the Map view.
  if (now && index !== null) {
    const row = railBriefRow(session, drawFeed);
    if (row) node.append(row);
  }
  // The Body shows whole.
  if (lines.length) {
    const list = document.createElement("ul");
    list.className = "body-lines";
    list.append(...lines.map(bodyLine));
    node.append(list);
  }
  node.append(...block.questions.filter((q) => q.answer === "not_answered").map(questionElement));
  node.append(...blockedElements(session, block.blocked));
  if (block.agents.length) {
    const list = document.createElement("ol");
    list.className = "subagents";
    // In the latest turn each is one line: its name, result and times.
    const entries = subagentHistory({ agents: block.agents, capabilities: session.capabilities }, agentSummaries).map((e) =>
      now ? { ...e, asked: "" } : e,
    );
    list.append(...entries.map((entry) => historyElement(session.id, entry)));
    node.append(list);
  }
  const checksShown = turn?.listed ?? [];
  if (checksShown.length) {
    const list = document.createElement("ul");
    list.className = "checks";
    list.append(...checksShown.map(checkElement));
    node.append(list);
  }
  // The full response, folded: the latest turn's from the transcript, an older one's fetched at its
  // first opening.
  const latest = index !== null && index === session.turns.length - 1;
  const responses = latest ? session.transcript.responses : [];
  if (turn && (latest ? responses.length : turn.id)) {
    const body = document.createElement("div");
    const fill = (texts: string[]) => body.replaceChildren(...texts.map((t) => rendered("div", t, "response")));
    const fetched = replies.get(key);
    if (latest) fill(responses);
    else if (fetched !== undefined) fill([fetched]);
    const fetch = () => {
      if (latest || index === null || replies.has(key)) return;
      invoke("turn_reply", { id: session.id, index })
        .then((reply) => {
          replies.set(key, reply ?? "");
          fill([reply ?? ""]);
        })
        .catch(failure("turn_reply"));
    };
    const fold = folded(`${key} response`, "Full response", "Show or hide the agent's whole reply", [body], fetch);
    if (fold.hasAttribute("open")) fetch();
    node.append(fold);
  }
  return node;
}

// A turn before the newest (danielrispler/plotkeeper#117): one line, which opens the turn's block under it and folds it
// again. Only this node changes, so the others and the scroll position stay; the opened turns outlast a redraw.
export function foldedTurn(session: Session, block: FeedTurn): HTMLElement {
  const node = document.createElement("li");
  node.className = "turn past";
  const key = `${session.id} ${block.index} turn`;
  const turn = block.turn;
  if (!turn) return node;
  const draw = () => {
    const summary = turn.id ? summaryOf(turn.id) : null;
    const { headline } = headlineOf(summary, turn.message);
    const open = opened.has(key);
    const line = foldedLine(turn, headline);
    const button = pressable(document.createElement("button"), null, () => {
      if (opened.has(key)) opened.delete(key);
      else opened.add(key);
      draw();
    });
    button.classList.add("turn-line");
    // The line's prompt and Headline carry their own full text; the rest of the line says what a click does (#220).
    button.title = "Fold or unfold this turn";
    button.setAttribute("aria-expanded", String(open));
    button.append(
      fullTitle(text("span", line.prompt, "prompt"), turn.prompt.trim() || line.prompt),
      fullTitle(text("span", line.headline ?? "", "headline-text"), line.headline),
      // The ✕ is for the eye; assistive tech hears "failed" once (#456).
      ...(line.failed ? [glyphOnly("✕", "mark check-failed"), unseenWords("failed")] : []),
    );
    if (line.failed) button.querySelector(".check-failed")?.setAttribute("title", "A check ended failed");
    node.replaceChildren(button, ...(open ? [...turnContent(session, block, null).children] : []));
  };
  draw();
  return node;
}

// The index of the turn a design line's citation `id` names (danielrispler/plotkeeper#123): the turn with that id, or else
// the turn a card of that id was written in (an older line may cite one), null when neither is in `turns`.
export function citedTurn(turns: Pick<Turn, "id" | "started_at">[], id: string): number | null {
  const own = turns.findIndex((turn) => turn.id === id);
  return own >= 0 ? own : turnAt(turns, cardTime(id));
}

// Opens the turn a citation names in the rail: a folded turn opens, and the feed scrolls to it. Only that node changes.
export function openCitation(id: string) {
  const selected = studio.selection;
  if (selected === null || selected.agent !== null) return;
  const { session } = selected;
  const index = citedTurn(session.turns, id);
  if (index === null) return;
  opened.add(`${session.id} ${index} turn`);
  drawFeed();
  const node = element("feed").children[session.turns.length - 1 - index];
  (node as HTMLElement | undefined)?.scrollIntoView?.({ block: "start" });
}

const turnElement = (session: Session, block: FeedTurn, now: Now | null) =>
  now || block.index === null ? turnContent(session, block, now) : foldedTurn(session, block);

// Draws "now" and the feed from the selection and the brief the shell last sent.
export function drawFeed() {
  const selected = studio.selection;
  if (selected === null) {
    drawNow(null, null);
    element("banner-slot").replaceChildren();
    element("feed").replaceChildren();
    return;
  }
  const session = selected.session;
  const own = selected.agent === null;
  const { latest, blocks } = railOf(selected, fedCards(session, selected.agent));
  const now = drawNow(session, latest);
  // The banner sits above the newest turn, and holds the Brief while it shows.
  const last = session.turns.at(-1);
  const kept = own && banner && bannerLives(banner, session) ? banner : null;
  const lives = kept !== null && last !== undefined;
  const cards = studio.cards;
  element("banner-slot").replaceChildren(
    ...(lives ? [bannerElement({ headline: headlineOf(last.id ? summaryOf(last.id) : null, last.message).headline, endedAt: last.ended_at, since: kept.since, open: cards.open, brief: briefNodes(cards) })] : []),
  );
  element("brief-fold").hidden = lives || briefNodes(cards).length === 0;
  element("feed").replaceChildren(...blocks.map((block, i) => turnElement(session, block, own && i === 0 && block.index !== null ? now : null)));
}
// Rename (danielrispler/Falinks#469): the row's name, kept in the event log. An empty name gives the row its folder's again.
function askName(session: Session) {
  const dialog = element("rename-dialog", HTMLDialogElement);
  const field = element("rename-field", HTMLInputElement);
  field.value = session.name ?? "";
  dialog.onclose = async () => {
    if (dialog.returnValue !== "rename") return;
    await invoke("rename", { id: session.id, name: field.value.trim() }).catch(failure("rename", "error"));
  };
  dialog.returnValue = "";
  dialog.showModal();
}

// Shows a failed delete's reason over the story.
export function report(error: unknown) {
  failure("delete", "error")(error);
}

// Opens `dialog` and resolves to the button Daniel chose, "" for Esc.
function choice(dialog: HTMLDialogElement): Promise<string> {
  return new Promise((resolve) => {
    dialog.onclose = () => resolve(dialog.returnValue);
    dialog.returnValue = "";
    dialog.showModal();
  });
}

// Delete: asks, then drops the row; the cards are kept.
export async function askDelete(session: Session) {
  const dialog = element("delete-dialog", HTMLDialogElement);
  element("delete-name").textContent = sessionName(session);
  if ((await choice(dialog)) !== "delete") return;
  await invoke("delete_session", { id: session.id });
}

// A late Allow reaches no terminal from the page: it shows the line to tell the session, so the agent retries the command. The line
// is no failure: it shows in the notice line until dismissed, under no command's name, and the Studio log gets none of it.
export function retryNotice(session: Pick<Session, "state">, card: BlockedCommand, agent: string | null) {
  if (session.state === "ended") return showError("The session has ended: start it again and ask it to run the approved command.");
  showError(`Allowed. Tell the session: ${retryLine(card, agent)}`);
}

// The session's blocked commands the view shows, in block order: its own view shows them all, and a subagent's view
// only its own (danielrispler/plotkeeper#170), less the declined, which have no card.
const viewed = (session: Session, agent: string | null) =>
  (session.blocked_commands ?? []).filter((card) => card.status !== "declined" && (agent === null || card.agent === agent));

// The cards the dock holds: those that show a button (danielrispler/plotkeeper#297).
const docks = (card: BlockedCommand) => docksInDock(card, blockedRequestFor(card.call) !== null);
const dockedCards = (session: Session, agent: Subagent | null) => blockedElements(session, viewed(session, agent?.id ?? null).filter(docks));
// The cards the feed places, neutral: those with no button.
const fedCards = (session: Session, agent: string | null) => viewed(session, agent).filter((card) => !docks(card));

// `cards` drawn, each naming the subagent that asked.
function blockedElements(session: Session, cards: BlockedCommand[]): HTMLElement[] {
  const named = (id: string | null) => (id === null ? null : (session.agents.find((a) => a.id === id)?.name ?? id.slice(0, 5)));
  const retry = (card: BlockedCommand) => retryNotice(session, card, named(card.agent));
  const note = (card: BlockedCommand) => studio.cards.risk_notes.find((n) => n.call === card.call);
  return cards.flatMap((card) => blockedElement(session.resume_id || session.id, card, named(card.agent), retry, note(card), session.state !== "ended") ?? []);
}

// The dock of the viewed agent: its open questions and the blocked commands that show a button.
function drawDock({ session, agent: id }: Selection) {
  const agent = session.agents.find((a) => a.id === id) ?? null;
  showQuestions(
    reports(session, "asks") ? session.questions.filter((question) => question.agent === (agent?.id ?? null)) : [],
    dockedCards(session, agent),
  );
}

// Shows the note of each docked blocked command, from the cards the window holds: in the place that stands, where it stands, since a note
// arriving changes no card's buttons and rebuilding the dock would make the live region a new node and drop the focus on Allow
// (danielrispler/plotkeeper#298); and in a new place for a card drawn before its note was known, as when a selection draws before its cards.
export function fillDockNotes() {
  const session = studio.selection?.session;
  if (!session) return;
  for (const node of element("dock").querySelectorAll<HTMLElement>(".blocked-command[data-call]")) {
    const call = node.dataset.call;
    const card = (session.blocked_commands ?? []).find((c) => c.call === call);
    const note = studio.cards.risk_notes.find((n) => n.call === call);
    const risk = card ? riskLine(note, docks(card), session.state !== "ended") : null;
    if (risk) fillCardRisk(node, risk);
  }
}

// The open question of the viewed agent, docked at the top of the rail, newest first; a plan or a
// permission the answer hook waits on docks too, whichever agent asks (danielrispler/Falinks#478). A question never
// answered shows in its turn; an answered one is in the decision history (danielrispler/Falinks#479).
function showQuestions(questions: Question[], blocked: HTMLElement[]) {
  const open = [...questions].reverse().filter((question) => question.answer === "open");
  const decision = openDecision();
  const docked = [...(decision ? [decisionElement(decision)] : []), ...open.map(questionElement), ...blocked];
  element("dock").replaceChildren(...docked);
  element("dock").hidden = docked.length === 0;
}
