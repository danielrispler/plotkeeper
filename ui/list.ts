// The session list, on the window's right: one line per session, sorted by state, its status shown
// by shape (danielrispler/Falinks#572), each session's subagents nested under it. When the window is narrow it collapses to
// a strip of glyphs and labels, which opens the full list on hover, on click or on ⌘1.

import { age, element, patch, prose, pressable, stepText, text } from "./dom.js";
import { counts, times, type Time } from "./times.js";
import { shortPaths } from "./paths.js";
import { isSelected, select } from "./selection.js";
import { studio } from "./state.js";
import { stateLabels, type Context, type Session, type SessionState, type Subagent } from "./types.js";

// The collapsed groups Daniel opened, which stay open when the list is drawn again.
const collapsed: SessionState[] = ["idle", "ended"];
const opened = new Set<SessionState>();

// What the strip calls a session: the issue number of an `issue-<n>-…` folder, else its folder's
// first letters.
export function stripLabel(folder: string): string {
  return /^issue-(\d+)/.exec(folder)?.[1] ?? folder.slice(0, 4);
}

// What a session is called: Daniel's name for it, else its issue, PR and worktree (danielrispler/Falinks#480), else its
// folder.
export function sessionName(session: Pick<Session, "name" | "identity" | "folder_name">): string {
  return session.name ?? (session.identity.name || session.folder_name);
}

const thousands = (n: number) => `${Math.round(n / 1000).toLocaleString("en-US")}k`;

// How full a session's context is (danielrispler/Falinks#517, decisions 2 and 3): "context 9%", its tokens such as "85k"
// when the window is unknown, or "" before it is known.
export function contextText(context: Context | null): string {
  if (!context) return "";
  return context.share === null ? thousands(context.tokens) : `context ${context.share}%`;
}

// The context share's hover: its tokens of the window, and what Claude's status line shows: its own
// percent left.
export function contextTitle(context: Context | null): string {
  if (!context) return "";
  const used = thousands(context.tokens);
  if (context.window === null || context.left === null) return `${used} tokens used · the window size is unknown`;
  return `${used} of ${thousands(context.window)} tokens used · Claude's status line shows this as ${context.left}% left`;
}

// The muted mark after a row's name: "status only" for another agent's row (danielrispler/Falinks#560), so it never reads
// as a Claude session with missing data; else the context share.
export function rowMark(session: Pick<Session, "status_only" | "transcript">): string {
  return session.status_only ? "status only" : contextText(session.transcript.context);
}

// The mark's node, with the context share's hover.
function markNode(session: Session): HTMLElement {
  const title = session.status_only ? "Another agent, followed by its reported status only" : contextTitle(session.transcript.context);
  return Object.assign(text("span", rowMark(session), "id"), { title });
}

// A subagent's state label and its state: "Needs you" while it asks, else its status.
export function agentState(session: Session, agent: Subagent): [string, string] {
  return session.asker?.agent === agent.id ? [stateLabels.needs_you, "needs_you"] : [agent.status, agent.status];
}

// A status's shape and its word (danielrispler/Falinks#572): a "Needs you" pill, the only colour (danielrispler/Falinks#565, decision 1); a
// spinner while running; a dot at your turn, filled once it finished unseen, idle too (danielrispler/Falinks#556); a ring when idle,
// ended or, for a subagent, done.
export type Glyph = "pill" | "spinner" | "dot" | "filled" | "ring";
export function status(state: string, unseen = false): [Glyph, string] {
  const label = stateLabels[state as SessionState] ?? state;
  if (state === "needs_you") return ["pill", label];
  if (state === "running") return ["spinner", label];
  // The core marks a turn unseen only at your turn and once idle after it, which keeps the mark.
  if (unseen) return ["filled", `${label}, not seen`];
  if (state === "your_turn") return ["dot", label];
  return ["ring", label];
}

// A status's glyph, named by its word, so the shape is never alone. The pill shows its word.
function glyph(state: string, unseen = false): HTMLElement {
  const [shape, word] = status(state, unseen);
  const node = text("span", shape === "pill" ? word : "", "glyph");
  node.dataset.glyph = shape;
  node.setAttribute("role", "img");
  node.setAttribute("aria-label", word);
  return node;
}

// A row's second line, only while it runs or needs Daniel: the current step, or the open question's
// first line, else who asks (a plan or a permission); null otherwise.
export function secondLine(session: Pick<Session, "state" | "line" | "questions"> & { status_only?: boolean }): string | null {
  // A status-only row has no step or question to show (danielrispler/Falinks#560).
  if (session.status_only) return null;
  if (session.state === "running") return session.line ?? "";
  if (session.state !== "needs_you") return null;
  const open = session.questions.filter((question) => question.answer === "open").at(-1);
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
    if (!children.has(parent)) children.set(parent, []);
    children.get(parent)!.push(agent);
  }
  return children;
}

// Whether a subagent's line is not drawn: it faded, and nothing below it is drawn (danielrispler/Falinks#434, decision 43).
export function hidden(children: Children, agent: Subagent): boolean {
  return agent.faded && (children.get(agent.id) ?? []).every((child) => hidden(children, child));
}

// The count of a parent's subagents that faded into its line, workers as helpers (danielrispler/Falinks#535).
export const doneCount = (all: Children, children: Subagent[] = []) => children.filter((agent) => hidden(all, agent)).length;
function done(all: Children, children: Subagent[] = []): HTMLElement[] {
  const faded = doneCount(all, children);
  return faded ? [text("span", `${faded} done`, "id")] : [];
}

// A row's lines: its name and status glyph, then, if any, what it is doing, muted. Its title pairs
// the name with the status's word.
function rowLines(button: HTMLElement, state: string, name: HTMLElement[], below: HTMLElement[] | null, unseen = false) {
  const first = text("span", "", "l1");
  first.append(...name, glyph(state, unseen));
  button.append(first);
  button.title = `${name[0].textContent} · ${status(state, unseen)[1]}`;
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
function selecting(id: string, agent: string | null, key = `${id} ${agent ?? ""}`): HTMLButtonElement {
  const button = pressable(document.createElement("button"), isSelected(id, agent), (event: MouseEvent) => event.detail === 0 && select(id, agent));
  button.dataset.key = key;
  button.addEventListener("pointerdown", (event) => event.button === 0 && select(id, agent));
  return button;
}

// One row: the session's name, its mark and its status glyph; while it runs or needs
// Daniel, what it is doing and, on the row being looked at, the turn's clock.
function row(session: Session, children: Children): HTMLElement {
  const button = selecting(session.id, null);
  button.classList.add("row");
  const line = secondLine(session);
  const turn = line === null ? null : rowTime(session, isSelected(session.id));
  const clock = text("span", "", "clock");
  rowLines(
    button,
    session.state,
    [text("span", sessionName(session), "name"), markNode(session)],
    line === null
      ? null
      : [...done(children, children.get("session")), rowLine(line), ...(turn ? [Object.assign(counts(clock, turn[1]), { title: turn[0] })] : [])],
    session.unseen,
  );
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
  if (full) node.title = full;
  return node;
}

// A subagent row's second line, as a session row's (danielrispler/Falinks#572): only while it runs or needs Daniel.
export function agentLine(state: string, details: (string | null)[]): string | null {
  return state === "running" || state === "needs_you" ? details.filter(Boolean).join(" · ") : null;
}

// A subagent's row, indented under the agent that spawned it. A worker shows its task, status and
// last step; a helper its task, type, status, duration and result. A failed child stands out until
// Daniel opens it.
function agentRow(session: Session, children: Children, agent: Subagent, depth: number): HTMLElement {
  const button = selecting(session.id, agent.id);
  button.classList.add("row", agent.kind);
  button.style.paddingInlineStart = `calc(var(--sp-4) + var(--indent) * ${depth})`;
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

// A session's row, then its subagents under the agent that spawned them, at any depth, and those
// with no known spawn under "Unattached". A faded subagent is not drawn, and its parent's line counts
// it, unless anything below it is drawn: then it keeps its line, so its children stay under it.
function tree(session: Session): HTMLElement[] {
  const children = childrenOf(session);
  const nested = (parent: string, depth: number): HTMLElement[] =>
    (children.get(parent) ?? []).flatMap((agent) => {
      const below = nested(agent.id, depth + 1);
      return [...(hidden(children, agent) ? [] : [agentRow(session, children, agent, depth)]), ...below];
    });
  const unattached = nested("unattached", 2);
  const label = text("p", "Unattached", "unattached");
  return [row(session, children), ...nested("session", 1), ...(unattached.length ? [label, ...unattached] : [])];
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
function strip(sessions: Session[]): HTMLElement[] {
  const toggle = () => document.body.classList.toggle("list-open");
  const open = pressable(text("button", "☰", "open"), false, toggle);
  open.title = "Open list (⌘1)";
  const shown = sessions.filter((session) => !collapsed.includes(session.state));
  const labels = stripLabels(shown);
  const rest = sessions.length - shown.length;
  return [
    open,
    ...shown.map((session, i) => {
      const button = selecting(session.id, null, `strip ${session.id}`);
      button.setAttribute("aria-pressed", String(isSelected(session.id)));
      button.title = `${sessionName(session)} · ${status(session.state, session.unseen)[1]}`;
      button.append(glyph(session.state, session.unseen), labels[i]);
      return button;
    }),
    ...(rest ? [Object.assign(pressable(text("button", String(rest)), false, toggle), { title: "Idle and ended" })] : []),
  ];
}

// The core sends the sessions sorted by state: needs you, running, your turn, then idle and ended,
// which collapse at the bottom. The rows above them carry their state by shape, so they have no group
// heading (danielrispler/Falinks#572).
export function showSessions(sessions: Session[]) {
  studio.listed = sessions;
  const groups: Session[][] = [];
  for (const session of sessions) {
    const last = groups[groups.length - 1];
    if (last?.[0].state === session.state) last.push(session);
    else groups.push([session]);
  }
  // Patched, not rebuilt, so focus and text selection survive the update (danielrispler/Falinks#570).
  patch(element("groups"), groups.map((rows) => {
      const state = rows[0].state;
      const title = `${stateLabels[state]} · ${rows.length}`;
      if (!collapsed.includes(state)) {
        const group = document.createElement("section");
        group.dataset.key = state;
        group.append(...rows.flatMap(tree));
        return group;
      }
      const group = document.createElement("details");
      group.dataset.key = state;
      group.open = opened.has(state);
      group.addEventListener("toggle", () => (group.open ? opened.add(state) : opened.delete(state)));
      group.append(text("summary", title), ...rows.flatMap(tree));
      return group;
    }),
  );
  patch(element("strip"), strip(sessions));
  element("no-sessions").hidden = sessions.length > 0;
  const chip = element("needs-you");
  chip.textContent = chipText(sessions);
  chip.hidden = chip.textContent === "";
}
