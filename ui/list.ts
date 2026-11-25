// The session list, on the window's right: one line per session, sorted by state, its status shown
// by shape (danielrispler/Falinks#572), each session's subagents nested under it. When the window is narrow it collapses to
// a strip of glyphs and labels, which opens the full list on hover, on click or on ⌘1.

import { logStop } from "./stop-rules.js";
import { age, element, failure, fullTitle, glyphOnly, invoke, newest, patch, prose, pressable, stepText, text } from "./dom.js";
import { hideClearToast, showClearToast } from "./toast.js";
import { counts, times, type Time } from "./times.js";
import { shortPaths } from "./paths.js";
import { historyLink } from "./history.js";
import { logo, logoKey, noContextNote } from "./logos.js";
import { knownProjects, projectGroups, projectOfCwd, type ProjectGroup } from "./projects.js";
import { openMap, shownProject } from "./map.js";
import { pressHeader } from "./map-model.js";
import { closePanel, panelShown } from "./panel.js";
import { redrawHistory, showHistory } from "./panes.js";
import { isSelected, pressSession } from "./selection.js";
import { studio } from "./state.js";
import { titled } from "./keys.js";
import { blockedWaits, stateLabels, type BlockedCommand, type Context, type MarkWhy, type ProjectMark, type Session, type SessionState, type Subagent } from "./types.js";
import { askDelete, lastHeadline, report } from "./story.js";
import { hoverAgents, refreshAgents } from "./map-agents.js";
import { showTitle } from "./alerts.js";

// The project header's quiet dot (#386): Home's mark (#344), unchanged in meaning, drawn as the map's mist. Its hover names which of the three
// lit it. The marks are read when the saved list changes and when the window gains focus, never on a timer.
const marks = new Map<string, MarkWhy[]>();
let marksStale = true;
const whyText: Record<MarkWhy, string> = { fog: "A Core part you track has changed", broken: "A test that backs a law failed on main", brief: "A PR brief is waiting" };
export const dotTitle = (why: MarkWhy[]): string => why.map((reason) => whyText[reason]).join("\n");
export const staleDots = () => {
  marksStale = true;
};
// One read at a time asked for: `marksStale` is cleared when it starts and set again when it fails, so the next draw of the list asks again;
// a read's answer that came after a newer one asked is dropped.
const latestDots = newest();
async function readDots() {
  marksStale = false;
  let now: ProjectMark[] | undefined;
  try {
    now = await latestDots(invoke("map_marks", { projects: knownProjects() }));
  } catch (e) {
    marksStale = true;
    throw e;
  }
  if (!now) return;
  const key = (entries: [string, MarkWhy[]][]) => JSON.stringify(entries);
  if (key(now.map((mark) => [mark.project, mark.why])) === key([...marks])) return;
  marks.clear();
  for (const mark of now) marks.set(mark.project, mark.why);
  drawSidebar();
}
if (typeof window !== "undefined") {
  window.addEventListener?.("focus", () => {
    staleDots();
    drawSidebar();
  });
}

// A press on a project's header (#386): another project's map shows, and a session's panel of another project closes; on the shown project's
// header it folds the group.
function pressHead(path: string) {
  const projectOf = (id: string) => {
    const session = studio.listed.find((s) => s.id === id);
    return session ? projectOfCwd(session.cwd) : null;
  };
  const next = pressHeader({ project: shownProject(), panel: panelShown() }, path, projectOf);
  if (next === "fold") {
    if (!shut.delete(path)) shut.add(path);
    return drawSidebar();
  }
  if (next.panel === null && panelShown() !== null) closePanel();
  openMap(next.project);
}

// The state groups folded in a project, idle only, and the idle groups Daniel opened, by project and state,
// which stay open when the list is drawn again.
const collapsed: SessionState[] = ["idle"];
const opened = new Set<string>();
// The projects whose group Daniel folded: its header stays, with its rollup.
const shut = new Set<string>();
export const projectOpen = (key: string) => !shut.has(key);

// What the strip calls a session: the issue number of an `issue-<n>-…` folder, else its folder's
// first letters.
export function stripLabel(folder: string): string {
  return /^issue-(\d+)/.exec(folder)?.[1] ?? folder.slice(0, 4);
}

// What a session is called: Daniel's name for it, else the name written from its first prompt with
// the identity's issue in front ("#37 · One event internal"), else its issue, PR and worktree
// (danielrispler/Falinks#480), else its folder.
export function sessionName(
  session: Pick<Session, "name" | "identity" | "folder_name"> & { prompt_name?: string | null },
): string {
  if (session.name) return session.name;
  if (session.prompt_name) return session.identity.issue === null ? session.prompt_name : `#${session.identity.issue} · ${session.prompt_name}`;
  return session.identity.name || session.folder_name;
}

const thousands = (n: number) => `${Math.round(n / 1000).toLocaleString("en-US")}k`;

// How full a session's context is: its tokens, such as "85k", or "" before they are known.
export function contextText(context: Context | null): string {
  return context ? thousands(context.tokens) : "";
}

// The context's hover: its tokens in full words.
export function contextTitle(context: Context | null): string {
  return context ? `${thousands(context.tokens)} tokens used` : "";
}

// The muted mark after a row's name: "terminal" for a plain terminal's row, "status only" for another agent's row
// (danielrispler/Falinks#560), so it never reads as a Claude session with missing data; else the context's tokens. The
// agent itself is the logo before the name (#158).
export function rowMark(session: Pick<Session, "shell" | "status_only" | "transcript">): string {
  if (session.shell) return "terminal";
  if (session.status_only) return "status only";
  return contextText(session.transcript.context);
}

// The name's hover, only for a prompt-named session: its full name, then the identity's PR and worktree, which the prompt
// name leaves out. Any other name is in the row's own title.
function nameTitle(session: Session): string {
  return session.name || !session.prompt_name ? "" : `${sessionName(session)} · ${session.identity.name}`;
}

// The mark's node, with the context's hover.
function markNode(session: Session): HTMLElement {
  if (session.shell) return fullTitle(text("span", rowMark(session), "id"), "A plain terminal");
  if (session.status_only) return fullTitle(text("span", rowMark(session), "id"), "Another agent, followed by its reported status only");
  return fullTitle(text("span", rowMark(session), "id"), contextTitle(session.transcript.context));
}

// A subagent's state label and its state: "Needs you" while it asks, else its status.
export function agentState(session: Session, agent: Subagent): [string, string] {
  return session.asker?.agent === agent.id ? [stateLabels.needs_you, "needs_you"] : [agent.status, agent.status];
}

// A status's shape and its word (danielrispler/Falinks#572): a "Needs you" pill, the only colour (danielrispler/Falinks#565, decision 1); a
// spinner while running; a dot at your turn, filled once it finished unseen, idle too (danielrispler/Falinks#556); a ring when idle,
// ended or, for a subagent, done.
export type Glyph = "pill" | "spinner" | "dot" | "filled" | "ring" | "check";
// A landed task's session at your turn, idle or ended shows a check mark and these words instead (#53, Landed).
export const landedWord = "Landed — safe to delete";
export const showsLanded = (state: string, landed: boolean) => landed && ["your_turn", "idle", "ended"].includes(state);
// `unwatched`: no process of the session's agent is known, so its "Idle" is a guess (#330).
export function status(state: string, unseen = false, landed = false, unwatched = false): [Glyph, string] {
  const label = (stateLabels[state as SessionState] ?? state) + (state === "idle" && unwatched ? "?" : "");
  if (showsLanded(state, landed)) return ["check", landedWord];
  if (state === "needs_you") return ["pill", label];
  if (state === "running") return ["spinner", label];
  // The core marks a turn unseen only at your turn and once idle after it, which keeps the mark.
  if (unseen) return ["filled", `${label}, not seen`];
  if (state === "your_turn") return ["dot", label];
  return ["ring", label];
}

// A status's glyph, named by its word, so the shape is never alone. The pill shows its word.
export function glyph(state: string, unseen = false, landed = false, unwatched = false): HTMLElement {
  const [shape, word] = status(state, unseen, landed, unwatched);
  const node = text("span", shape === "pill" ? word : "", "glyph");
  node.dataset.glyph = shape;
  node.setAttribute("role", "img");
  node.setAttribute("aria-label", word);
  if (shape === "check") node.title = word;
  return node;
}

// A row's second line (#382, the list holds what Home held): while it needs Daniel, the open question's first line, else who asks
// (a plan or a permission); while it runs, its Intention, else its current step; once it finished unseen, its last turn's Headline
// (`headline`). Null otherwise, and on a landed row, which reads as landed.
type Lined = Pick<Session, "state" | "line" | "questions"> &
  Partial<Pick<Session, "unseen" | "landed" | "turns">> & { status_only?: boolean; blocked_commands?: Pick<BlockedCommand, "status">[]; transcript?: { intention?: string | null } };
export function secondLine<T extends Lined>(session: T, headline: (session: T) => string | null = () => null): string | null {
  // A status-only row has no step or question to show (danielrispler/Falinks#560).
  if (session.status_only) return null;
  const open = session.questions.filter((question) => question.answer === "open").at(-1);
  // Blocked commands with a button for Daniel (danielrispler/plotkeeper#173, #297).
  const waiting = (session.blocked_commands ?? []).filter((card) => blockedWaits(card.status)).length;
  if (waiting > 0 && session.state !== "ended" && !(session.state === "needs_you" && open)) {
    return `${waiting} blocked ${waiting === 1 ? "command" : "commands"} waiting`;
  }
  if (session.state === "running") return session.transcript?.intention || (session.line ?? "");
  if (session.unseen && !showsLanded(session.state, !!session.landed) && (session.state === "your_turn" || session.state === "idle")) return headline(session);
  if (session.state !== "needs_you") return null;
  // A question with no text, as a redacted log holds, falls back to the row's line rather than throw.
  return open?.questions[0]?.question?.split("\n")[0] ?? session.line ?? "";
}

// The top bar's chip: how many sessions need Daniel, or "" when none.
export function chipText(sessions: Pick<Session, "state">[]): string {
  const count = sessions.filter((session) => session.state === "needs_you").length;
  return count ? `${count} ${count === 1 ? "needs" : "need"} you` : "";
}

// A session's subagents by the id of the agent that spawned them: "session", "unattached" or a
// subagent's id. Built once per draw, so the tree and the "done" counts do not scan every subagent.
type Children = Map<string, Subagent[]>;
export function childrenOf(session: Pick<Session, "agents">): Children {
  const children: Children = new Map();
  for (const agent of session.agents) {
    const parent = typeof agent.parent === "string" ? agent.parent : agent.parent.agent;
    const siblings = children.get(parent) ?? [];
    siblings.push(agent);
    children.set(parent, siblings);
  }
  return children;
}

// Whether a subagent's line is not drawn: it faded, and nothing below it is drawn (danielrispler/Falinks#434, decision 43).
export function hidden(children: Children, agent: Subagent): boolean {
  return agent.faded && (children.get(agent.id) ?? []).every((child) => hidden(children, child));
}

// The count of a parent's subagents that faded into its line, workers as helpers (danielrispler/Falinks#535).
export const doneCount = (all: Children, children: Subagent[] = []) => children.filter((agent) => hidden(all, agent)).length;
// Whether a subagent shows under a collapsed row: it needs Daniel, or it failed and he has not opened it.
const standsOut = (session: Pick<Session, "asker">, agent: Subagent) =>
  session.asker?.agent === agent.id || (agent.failed && !agent.opened);

// The rows collapsed, by session id then "session" or a subagent's id, kept while the app runs.
const folded = new Set<string>();
type Folded = { has(id: string): boolean };
const foldedIn = (session: Pick<Session, "id">): Folded => ({ has: (id) => folded.has(`${session.id} ${id}`) });

// The subagents drawn under `parent`, at any depth, each with its depth. Under a collapsed row only
// those that stand out are drawn, at their own depth.
export type Drawn = { agent: Subagent; depth: number };
export function drawn(session: Pick<Session, "asker">, children: Children, parent: string, depth: number, shut: Folded, under = false): Drawn[] {
  return (children.get(parent) ?? []).flatMap((agent) => {
    const below = drawn(session, children, agent.id, depth + 1, shut, under || shut.has(agent.id));
    const shown = !hidden(children, agent) && (!under || standsOut(session, agent));
    return [...(shown ? [{ agent, depth }] : []), ...below];
  });
}

// A row's fold toggle, for the row keyed `parents[0]` whose subagents spawn from `parents`: whether it
// is open and how many rows it hides; null when nothing is drawn under it.
export function folds(session: Pick<Session, "asker">, children: Children, parents: string[], shut: Folded): { open: boolean; hiding: number } | null {
  const count = (under: boolean) => parents.flatMap((parent) => drawn(session, children, parent, 0, shut, under)).length;
  const all = count(false);
  if (!all) return null;
  const open = !parents[0] || !shut.has(parents[0]);
  return { open, hiding: open ? 0 : all - count(true) };
}

function done(all: Children, children: Subagent[] = []): HTMLElement[] {
  const faded = doneCount(all, children);
  return faded ? [text("span", `${faded} done`, "id")] : [];
}

// A row's lines: its name and status glyph, then, if any, what it is doing, muted. Its title pairs
// the name with the status's word.
function rowLines(button: HTMLElement, state: string, name: HTMLElement[], below: HTMLElement[] | null, unseen = false, landed = false, unwatched = false) {
  const first = text("span", "", "l1");
  first.append(...name, glyph(state, unseen, landed, unwatched));
  button.append(first);
  button.title = `${name[0]?.textContent ?? ""} · ${status(state, unseen, landed, unwatched)[1]}`;
  button.classList.toggle("unseen", unseen);
  if (below) {
    const second = text("span", "", "l2");
    second.append(...below);
    button.append(second);
  }
  return first;
}

// A row's button, keyed by what it selects (`patch` keeps it across updates). It selects on the press,
// so the row shows selected at once (danielrispler/Falinks#570); a click without a pointer, from the keyboard, selects too.
export function selecting(id: string, agent: string | null, key = `${id} ${agent ?? ""}`): HTMLButtonElement {
  const button = pressable(document.createElement("button"), isSelected(id, agent), (event: MouseEvent) => {
    if (event.detail === 0) void pressSession(id, agent);
  });
  button.dataset.key = key;
  button.addEventListener("pointerdown", (event) => event.button === 0 && pressSession(id, agent));
  return button;
}

// One row: the session's name, its mark and its status glyph; while it runs or needs
// Daniel, what it is doing and, on the row being looked at, the turn's clock.
export function row(session: Session, children: Children, token: string | null = null): HTMLElement {
  const button = selecting(session.id, null);
  button.classList.add("row", "session-row");
  const line = secondLine(session, lastHeadline);
  const turn = line === null ? null : rowTime(session, isSelected(session.id));
  const clock = text("span", "", "clock");
  const first = rowLines(
    button,
    session.state,
    [fullTitle(text("span", sessionName(session), "name"), nameTitle(session)), markNode(session), ...(token ? [fullTitle(text("span", token, "id token"), token)] : [])],
    line === null
      ? null
      : [...done(children, children.get("session")), rowLine(line), ...(turn ? [Object.assign(counts(clock, turn[1]), { title: turn[0] })] : [])],
    session.unseen,
    session.landed,
    !session.pid,
  );
  // Hovering the row lights its session's chips on the map (#382).
  button.addEventListener("pointerenter", () => hoverAgents(session.id));
  button.addEventListener("pointerleave", () => hoverAgents(null));
  // The agent's logo leads the row, apart from its state glyph (#158).
  first.prepend(logo(logoKey(session), noContextNote(session) || undefined));
  return button;
}

// The row's clock: the turn time, only on the row being looked at; the rail has all three (danielrispler/Falinks#517,
// decision 4; danielrispler/Falinks#571).
export function rowTime(session: Pick<Session, "state" | "started_at" | "last_event_at" | "turn_span">, selected: boolean): Time | null {
  return (selected && session.turn_span && times(session, null)[1]) || null;
}

// A row line as plain text (danielrispler/Falinks#571): Markdown's emphasis, headings and links dropped outside code spans.
export function plain(line: string): string {
  return line
    .split("`")
    .map((part, i) => (i % 2 ? part : part.replace(/\[([^\]]*)\]\([^)]*\)/g, "$1").replace(i ? /\*\*|__/g : /\*\*|__|^#{1,6}\s+/g, "")))
    .join("`");
}

// A row line as shown, and its hover title: the original line whenever a link's URL or a long path's
// start is dropped, so the full line stays reachable (danielrispler/Falinks#571); else "".
export function rowText(line: string): [shown: string, title: string] {
  const [shown, full] = shortPaths(plain(line));
  return [shown, full || /\]\([^)]*\)/.test(line) ? line : ""];
}

// A row's line, such as a card summary stored in Hebrew, laid out by its own direction, its code spans
// cleaned as a card's are (danielrispler/Falinks#434, decision 40), its paths shortened with the full line on hover (danielrispler/Falinks#571).
function rowLine(line: string): HTMLElement {
  const [shown, full] = rowText(line);
  const node = prose("span", shown);
  node.className = "row-step";
  node.dir = "auto";
  return fullTitle(node, full || shown);
}

// A subagent row's second line, as a session row's (danielrispler/Falinks#572): only while it runs or needs Daniel.
export function agentLine(state: string, details: (string | null)[]): string | null {
  return state === "running" || state === "needs_you" ? details.filter(Boolean).join(" · ") : null;
}

// A subagent's row, indented under the agent that spawned it. A worker shows its task, status and
// last step; a helper its task, type, status, duration and result. A failed child stands out until
// Daniel opens it.
export function agentRow(session: Session, children: Children, agent: Subagent, depth: number): HTMLElement {
  const button = selecting(session.id, agent.id);
  button.classList.add("row", agent.kind);
  button.style.paddingInlineStart = `calc(var(--sp-4) + var(--indent) * ${depth + 1})`;
  const [, stateName] = agentState(session, agent);
  const details =
    agent.kind === "worker"
      ? [stepText(agent.last_step)]
      : [agent.agent_type, age(agent.started_at, agent.ended_at ?? Date.now()), agent.result];
  const line = agentLine(stateName, details);
  const first = rowLines(
    button,
    stateName,
    [text("span", agent.name, "name")],
    line === null ? null : [...done(children, children.get(agent.id)), rowLine(line)],
  );
  first.classList.toggle("failed", agent.failed && !agent.opened);
  return button;
}

// A row control's icon: a 14px inline SVG on the current colour, in a 24x24 button (the tokens' --control and --icon).
const CHEVRON = '<path d="M5 3l4 4-4 4"/>';
const CROSS = '<path d="M3.5 3.5l7 7M10.5 3.5l-7 7"/>';
function icon(name: string, path: string, click: () => void): HTMLElement {
  const button = pressable(text("button", "", name), null, click);
  button.innerHTML = `<svg viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">${path}</svg>`;
  return button;
}

// A row in the list: its toggle, when anything is drawn under it, then the row, then its delete. They
// are the row's siblings, as a button holds no button, so pressing them does not select the row.
export function item(key: string, depth: number, toggle: { open: boolean; hiding: number } | null, row: HTMLElement, remove: HTMLElement | null) {
  const node = document.createElement("div");
  node.className = "item";
  node.dataset.key = key;
  if (toggle) {
    const fold = icon("fold", CHEVRON, () => {
      if (!folded.delete(key)) folded.add(key);
      drawSidebar();
    });
    // A fixed name and hover; whether it is open is said by `aria-expanded` alone (#456).
    fold.title = "Show or hide its subagents";
    fold.setAttribute("aria-label", "Subagents");
    fold.setAttribute("aria-expanded", String(toggle.open));
    // The count goes after the name, as the toggle's column has room only for its chevron.
    if (toggle.hiding) row.querySelector(".name")?.after(text("span", `${toggle.hiding} hidden`, "id"));
    fold.style.insetInlineStart = `calc(var(--indent) * ${depth})`;
    node.append(fold);
  }
  node.append(row, ...(remove ? [remove] : []));
  return node;
}

// A session row's delete (decision of the session list's fold and delete): the rail's dialog, asked from the row.
export function deleting(session: Session): HTMLElement | null {
  if (session.status_only) return null;
  // `patch` keeps the first button and its listener, so the press reads the session as it is now.
  const now = () => studio.listed.find((listed) => listed.id === session.id) ?? session;
  const button = icon("delete", CROSS, () => void askDelete(now()).catch(report));
  button.title = "Delete this session from the list…";
  button.setAttribute("aria-label", `Delete ${sessionName(session)}…`);
  return button;
}

// A session's row, then its subagents under the agent that spawned them, at any depth, and those
// with no known spawn under "Unattached". A faded subagent is not drawn, and its parent's line counts
// it, unless anything below it is drawn: then it keeps its line, so its children stay under it. A
// collapsed row hides its subagents, but not one that needs Daniel or failed unopened.
function tree(session: Session, token: string | null): HTMLElement[] {
  const children = childrenOf(session);
  const shut = foldedIn(session);
  const rows = (parent: string, depth: number, under: boolean) =>
    drawn(session, children, parent, depth, shut, under).map(({ agent, depth }) =>
      item(`${session.id} ${agent.id}`, depth, folds(session, children, [agent.id], shut), agentRow(session, children, agent, depth), null),
    );
  const under = shut.has("session");
  const unattached = rows("unattached", 2, under);
  const label = text("p", "Unattached", "unattached");
  const top = item(`${session.id} session`, 0, folds(session, children, ["session", "unattached"], shut), row(session, children, token), deleting(session));
  return [top, ...rows("session", 1, under), ...(unattached.length ? [label, ...unattached] : [])];
}

// The strip's labels, one per session, from its name: its issue number, else the name's first
// letters; sessions whose labels would match are numbered, so none reads "Fali" five times (danielrispler/Falinks#572).
export function stripLabels(sessions: Pick<Session, "name" | "identity" | "folder_name">[]): string[] {
  const base = sessions.map((session) =>
    session.identity.issue === null ? stripLabel(sessionName(session)) : String(session.identity.issue),
  );
  const nth = new Map<string, number>();
  return base.map((label) => {
    if (base.indexOf(label) === base.lastIndexOf(label)) return label;
    nth.set(label, (nth.get(label) ?? 0) + 1);
    return `${label}·${nth.get(label)}`;
  });
}

// The strip: a button per session that is not idle or ended, by its glyph and strip label, then one
// count for those collapsed, which opens the full list.
export function strip(sessions: Session[]): HTMLElement[] {
  const toggle = () => document.body.classList.toggle("list-open");
  const open = pressable(document.createElement("button"), null, toggle);
  open.classList.add("open");
  open.setAttribute("aria-label", "Session list");
  open.append(glyphOnly("☰"));
  open.title = titled(studio.files.keymap, "Open the session list", "studio::ToggleList");
  const shown = sessions.filter((session) => !collapsed.includes(session.state));
  const labels = stripLabels(shown);
  const rest = sessions.length - shown.length;
  return [
    open,
    ...shown.map((session, i) => {
      const button = selecting(session.id, null, `strip ${session.id}`);
      button.setAttribute("aria-pressed", String(isSelected(session.id)));
      button.title = `${sessionName(session)} · ${status(session.state, session.unseen, session.landed)[1]}`;
      button.append(glyph(session.state, session.unseen, session.landed), labels[i] ?? "");
      return button;
    }),
    ...(rest ? [Object.assign(pressable(text("button", `${rest} idle`), null, toggle), { title: titled(studio.files.keymap, "Open the session list, idle sessions among it", "studio::ToggleList") })] : []),
  ];
}

// Clear landed (#244): Clear hides every landed row and offers Undo for CLEAR_WINDOW_MS. When the window closes each hidden session
// that is still landed goes through today's Delete (`commit`); Undo commits nothing. `changed` hears the sessions still pending.
export type Clock = { set(run: () => void, ms: number): unknown; clear(id: unknown): void };
export const CLEAR_WINDOW_MS = 8000;
// biome-ignore lint/complexity/noExcessiveLinesPerFunction: spec #548 Group 4 splits list.ts's pure parts from its DOM; remove this when it does
export function clearing(
  time: Clock,
  deps: { landed(id: string): Session | undefined; commit(session: Session): Promise<unknown> | void; changed(pending: Session[], writing: Session[]): void },
  hidden = new Set<string>(),
) {
  const pending = new Map<string, Session>();
  // The sessions whose delete is in flight: the shell still hears them, so a quit before the write commits them (#244).
  const writing = new Map<string, Session>();
  let timer: unknown = null;
  const stop = () => {
    if (timer !== null) time.clear(timer);
    timer = null;
  };
  const tell = () => deps.changed([...pending.values()], [...writing.values()]);
  return {
    // The rows Clear hides: those pending and those committed, until they leave the list.
    hidden,
    clear(sessions: Session[]) {
      for (const s of sessions) {
        pending.set(s.id, s);
        hidden.add(s.id);
      }
      stop();
      timer = time.set(() => {
        timer = null;
        const due = [...pending.keys()].map((id) => ({ id, session: deps.landed(id) }));
        pending.clear();
        for (const { id, session } of due) if (session) writing.set(id, session);
        tell();
        for (const { id, session } of due) {
          if (!session) hidden.delete(id);
          else
            void Promise.resolve(deps.commit(session)).then(
              () => {
                writing.delete(id);
                tell();
              },
              // A row whose Delete failed comes back.
              () => {
                writing.delete(id);
                hidden.delete(id);
                tell();
              },
            );
        }
      }, CLEAR_WINDOW_MS);
      tell();
    },
    undo() {
      stop();
      for (const id of pending.keys()) hidden.delete(id);
      pending.clear();
      tell();
    },
    // A session that left the list, or stopped being landed, is no longer hidden; one still pending is left alone: it comes back and is
    // not committed.
    sync() {
      let dropped = false;
      for (const id of [...hidden]) {
        if (deps.landed(id)) continue;
        hidden.delete(id);
        dropped = pending.delete(id) || dropped;
        dropped = writing.delete(id) || dropped;
      }
      if (!dropped) return;
      if (pending.size === 0) stop();
      tell();
    },
  };
}

// The page's one Clear: the list's "Clear landed" (#244, #382), and the toast offers Undo. A commit is today's Delete: the row
// dropped, the cards kept.
const stillLanded = (id: string) => studio.listed.find((s) => s.id === id && showsLanded(s.state, s.landed));
export const landedClear = clearing(
  { set: (run, ms) => setTimeout(run, ms), clear: (id) => clearTimeout(id as number) },
  {
    landed: stillLanded,
    commit: (session) =>
      invoke("delete_session", { id: session.id }).catch((e) => {
        failure("delete_session", "error")(e);
        throw e;
      }),
    changed: (pending) => {
      if (pending.length) showClearToast(pending.length, () => landedClear.undo());
      else hideClearToast();
      drawShown();
    },
  },
);

// Whether a click on `target`, inside the session list, is on its empty space: not on a row, a group or project heading or any button.
// Such a click goes back to the bare map.
export function listSpace(target: { closest(selector: string): unknown }): boolean {
  return ![".item", "summary", ".project-head", "button"].some((selector) => target.closest(selector));
}

// The sessions the list shows: all but the ended ones, which are in History (#94), and those Clear landed hid (`hidden`, #382).
export const sidebarSessions = <T extends Pick<Session, "state" | "id">>(sessions: T[], hidden: ReadonlySet<string> = new Set()): T[] =>
  sessions.filter((session) => session.state !== "ended" && !hidden.has(session.id));

// The sessions "Clear landed" would clear: every landed one it has not hidden yet; none, and the list shows no Clear landed.
export const landedToClear = <T extends Pick<Session, "state" | "id" | "landed">>(sessions: T[], hidden: ReadonlySet<string>): T[] =>
  sessions.filter((session) => showsLanded(session.state, session.landed) && !hidden.has(session.id));

// A project's "⋯" (#381): the one menu of the list's headers, opened under it for its project, flipped at the window's edges (#458), and
// closed by a second press, as the rail's "⋯" toggles its menu (#452's review); Remove project is in it. It is drawn only for a project with no live session: the server saves a project again for its sessions' next event, so
// removing one with live sessions would undo itself (#380's review).
function projectMore(group: ProjectGroup<Session>): HTMLElement {
  const openHere = () => element("project-actions").matches(":popover-open") && element("project-actions").dataset.project === group.key;
  // A pointer's press outside the menu light-dismisses it before the click, so whether it was open here is read at the press.
  let wasOpen = false;
  const more = pressable(text("button", "⋯", "project-more"), null, () => {
    const menu = element("project-actions");
    const close = wasOpen || openHere();
    wasOpen = false;
    if (close) return menu.hidePopover();
    menu.dataset.project = group.key;
    // Its button is its implicit anchor (#458), which places it by CSS as the rail's "⋯" menu.
    menu.showPopover({ source: more });
  });
  more.addEventListener("pointerdown", () => (wasOpen = openHere()));
  more.title = "Project";
  more.setAttribute("aria-label", `More actions for ${group.name}`);
  return more;
}

// A project's header (#97): its name and, even while folded, its most urgent state: needs you over running. A saved project
// with no live session is its header only, with its "⋯"; one whose folder is gone is struck through (#381).
export function projectHead(group: ProjectGroup<Session>): HTMLElement {
  const head = document.createElement("div");
  head.className = ["project-head", ...(group.gone ? ["gone"] : []), ...(group.key === shownProject() ? ["current"] : [])].join(" ");
  head.dataset.key = `head ${group.key}`;
  head.dataset.project = group.key;
  const toggle = pressable(text("button", "", "project-toggle"), null, () => pressHead(group.key));
  toggle.setAttribute("aria-expanded", String(projectOpen(group.key)));
  fullTitle(toggle, `${group.gone ? `${group.key} is gone` : group.key || group.name}\n${group.key === shownProject() ? "Fold or unfold its sessions" : "Show its map"}`);
  toggle.append(text("span", group.name, "name"));
  if (group.rollup) toggle.append(glyph(group.rollup));
  const why = marks.get(group.key);
  if (why?.length) {
    const dot = text("span", "", "project-dot mist-swatch");
    dot.title = dotTitle(why);
    dot.setAttribute("role", "img");
    dot.setAttribute("aria-label", dot.title);
    toggle.append(dot);
  }
  head.append(toggle, ...(group.live ? [] : [projectMore(group)]));
  return head;
}

// A project's group: its header, then, unless folded, its live sessions by state.
function projectNode(group: ProjectGroup<Session>): HTMLElement {
  const node = document.createElement("section");
  node.className = "project";
  node.dataset.key = group.key;
  node.append(projectHead(group));
  if (!projectOpen(group.key) || !group.states.length) return node;
  const body = document.createElement("div");
  body.className = "project-body";
  body.dataset.key = "body";
  for (const { state, sessions } of group.states) {
    const rows = sessions.flatMap((session) => tree(session, null));
    if (!collapsed.includes(state)) {
      const states = document.createElement("div");
      states.dataset.key = state;
      states.append(...rows);
      body.append(states);
      continue;
    }
    const folded = document.createElement("details");
    folded.dataset.key = state;
    const id = `${group.key} ${state}`;
    folded.open = opened.has(id);
    folded.addEventListener("toggle", () => (folded.open ? opened.add(id) : opened.delete(id)));
    folded.append(fullTitle(text("summary", `${stateLabels[state]} · ${sessions.length}`), `Show or hide the ${stateLabels[state].toLowerCase()} sessions`), ...rows);
    body.append(folded);
  }
  node.append(body);
  return node;
}

// The project groups (#97): one per repository, in the order its sessions are listed, then the saved projects with none (#381).
// Patched, not rebuilt, so focus and text selection survive the update (danielrispler/Falinks#570).
export function drawSidebar() {
  if (marksStale && studio.projects.length) readDots().catch(failure("map_marks"));
  patch(element("groups"), projectGroups(studio.listed, studio.projects, landedClear.hidden).map(projectNode));
  patch(element("history-link"), [...historyLink(studio.listed, showHistory), ...clearLanded(landedToClear(studio.listed, landedClear.hidden))]);
}

// "Clear landed" at the list's end (#382), quiet, only while a landed session shows: Home's Clear (#244), with its Undo.
function clearLanded(landed: Session[]): HTMLElement[] {
  if (!landed.length) return [];
  const button = pressable(text("button", "Clear landed", "clear-landed quiet"), null, () => landedClear.clear(landedToClear(studio.listed, landedClear.hidden)));
  button.title = "Clear every landed session; Undo for a few seconds";
  button.dataset.key = "clear";
  return [button];
}

// The sessions are grouped by project, then by state: needs you, running, your turn, idle; the idle ones
// fold. Ended ones are in History. The rows carry their state by shape, so the groups have no heading
// (danielrispler/Falinks#572).
export function showSessions(sessions: Session[]) {
  studio.listed = sessions;
  if (sessions.length) logStop("session");
  void refreshAgents();
  landedClear.sync();
  redrawHistory();
  drawShown();
  const chip = element("needs-you");
  chip.textContent = chipText(sessions);
  chip.hidden = chip.textContent === "";
  chip.title = "Go to the first session that needs you";
}

// What shows the listed sessions but those Clear landed hid: the list, the strip and the tab's title.
function drawShown() {
  const live = sidebarSessions(studio.listed, landedClear.hidden);
  drawSidebar();
  patch(element("strip"), strip(live));
  showTitle(live.filter((session) => session.unseen).length);
}
