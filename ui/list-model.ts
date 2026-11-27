// The session list's pure parts (spec #548, Group 4): what a row, the strip and the chip say, which subagents are drawn under a row,
// and Clear landed's window. `list.ts` draws them.

import { blockedWaits, stateLabels, type BlockedCommand, type Context, type Session, type SessionState, type Subagent } from "./types.js";
import { times, type Time } from "./times.js";
import { shortPaths } from "./paths.js";
import { NO_FOOTPRINT } from "./map-live.js";

// What the strip calls a session: the issue number of an `issue-<n>-…` folder, else its folder's
// first letters.
function stripLabel(folder: string): string {
  return /^issue-(\d+)/.exec(folder)?.[1] ?? folder.slice(0, 4);
}

/**
 * What a session is called: Daniel's name for it, else the name written from its first prompt with
 * the identity's issue in front ("#37 · One event internal"), else its issue, PR and worktree
 * (danielrispler/Falinks#480), else its folder.
 */
export function sessionName(
  session: Pick<Session, "name" | "identity" | "folder_name"> & { prompt_name?: string | null },
): string {
  if (session.name) return session.name;
  if (session.prompt_name) return session.identity.issue === null ? session.prompt_name : `#${session.identity.issue} · ${session.prompt_name}`;
  return session.identity.name || session.folder_name;
}

const thousands = (n: number) => `${Math.round(n / 1000).toLocaleString("en-US")}k`;

/** How full a session's context is: its tokens, such as "85k", or "" before they are known. */
export function contextText(context: Context | null): string {
  return context ? thousands(context.tokens) : "";
}

/** The context's hover: its tokens in full words. */
export function contextTitle(context: Context | null): string {
  return context ? `${thousands(context.tokens)} tokens used` : "";
}

/**
 * The muted mark after a row's name: "terminal" for a plain terminal's row, "status only" for another agent's row
 * (danielrispler/Falinks#560), so it never reads as a Claude session with missing data; else the context's tokens. The
 * agent itself is the logo before the name (#158).
 */
export function rowMark(session: Pick<Session, "shell" | "status_only" | "transcript">): string {
  if (session.shell) return "terminal";
  if (session.status_only) return "status only";
  return contextText(session.transcript.context);
}

/** A subagent's state label and its state: "Needs you" while it asks, else its status. */
export function agentState(session: Session, agent: Subagent): [string, string] {
  return session.asker?.agent === agent.id ? [stateLabels.needs_you, "needs_you"] : [agent.status, agent.status];
}

/**
 * A status's shape and its word (danielrispler/Falinks#572): a "Needs you" pill, the only colour (danielrispler/Falinks#565, decision 1); a
 * spinner while running; a dot at your turn, filled once it finished unseen, idle too (danielrispler/Falinks#556); a ring when idle,
 * ended or, for a subagent, done.
 */
export type Glyph = "pill" | "spinner" | "dot" | "filled" | "ring" | "check";
/** A landed task's session at your turn, idle or ended shows a check mark and these words instead (#53, Landed). */
export const landedWord = "Landed — safe to delete";
export const showsLanded = (state: string, landed: boolean) => landed && ["your_turn", "idle", "ended"].includes(state);
/** `unwatched`: no process of the session's agent is known, so its "Idle" is a guess (#330). */
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

// A row's second line (#382, the list holds what Home held): while it needs Daniel, the open question's first line, else who asks
// (a plan or a permission); while it runs, its Intention, else its current step; once it finished unseen, its last turn's Headline
// (`headline`). Null otherwise, and on a landed row, which reads as landed.
type Lined = Pick<Session, "state" | "line" | "questions"> &
  Partial<Pick<Session, "unseen" | "landed" | "turns">> & { status_only?: boolean; blocked_commands?: Pick<BlockedCommand, "status">[]; transcript?: { intention?: string | null } };
export function secondLine<T extends Lined>(session: T, headline: (session: T) => string | null = () => null): string | null {
  // A status-only row has no step or question to show (danielrispler/Falinks#560).
  if (session.status_only) return null;
  const open = session.questions.filter((question) => question.answer === "open").at(-1);
  // Blocked commands that wait on Daniel until the session moves on (danielrispler/plotkeeper#173, #572).
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

/** The top bar's chip: how many sessions need Daniel, or "" when none. */
export function chipText(sessions: Pick<Session, "state">[]): string {
  const count = sessions.filter((session) => session.state === "needs_you").length;
  return count ? `${count} ${count === 1 ? "needs" : "need"} you` : "";
}

/**
 * A session's subagents by the id of the agent that spawned them: "session", "unattached" or a
 * subagent's id. Built once per draw, so the tree and the "done" counts do not scan every subagent.
 */
export type Children = Map<string, Subagent[]>;
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

/** Whether a subagent's line is not drawn: it faded, and nothing below it is drawn (danielrispler/Falinks#434, decision 43). */
export function hidden(children: Children, agent: Subagent): boolean {
  return agent.faded && (children.get(agent.id) ?? []).every((child) => hidden(children, child));
}

/** The count of a parent's subagents that faded into its line, workers as helpers (danielrispler/Falinks#535). */
export const doneCount = (all: Children, children: Subagent[] = []) => children.filter((agent) => hidden(all, agent)).length;
// Whether a subagent shows under a collapsed row: it needs Daniel, or it failed and he has not opened it.
const standsOut = (session: Pick<Session, "asker">, agent: Subagent) =>
  session.asker?.agent === agent.id || (agent.failed && !agent.opened);


export type Folded = { has(id: string): boolean };

/**
 * The subagents drawn under `parent`, at any depth, each with its depth. Under a collapsed row only
 * those that stand out are drawn, at their own depth.
 */
export type Drawn = { agent: Subagent; depth: number };
export function drawn(session: Pick<Session, "asker">, children: Children, parent: string, depth: number, shut: Folded, under = false): Drawn[] {
  return (children.get(parent) ?? []).flatMap((agent) => {
    const below = drawn(session, children, agent.id, depth + 1, shut, under || shut.has(agent.id));
    const shown = !hidden(children, agent) && (!under || standsOut(session, agent));
    return [...(shown ? [{ agent, depth }] : []), ...below];
  });
}

/**
 * A row's fold toggle, for the row keyed `parents[0]` whose subagents spawn from `parents`: whether it
 * is open and how many rows it hides; null when nothing is drawn under it.
 */
export function folds(session: Pick<Session, "asker">, children: Children, parents: string[], shut: Folded): { open: boolean; hiding: number } | null {
  const count = (under: boolean) => parents.flatMap((parent) => drawn(session, children, parent, 0, shut, under)).length;
  const all = count(false);
  if (!all) return null;
  const open = !parents[0] || !shut.has(parents[0]);
  return { open, hiding: open ? 0 : all - count(true) };
}

/**
 * The row's clock: the turn time, only on the row being looked at; the rail has all three (danielrispler/Falinks#517,
 * decision 4; danielrispler/Falinks#571).
 */
export function rowTime(session: Pick<Session, "state" | "started_at" | "last_event_at" | "turn_span">, selected: boolean): Time | null {
  return (selected && session.turn_span && times(session, null)[1]) || null;
}

/** A row line as plain text (danielrispler/Falinks#571): Markdown's emphasis, headings and links dropped outside code spans. */
export function plain(line: string): string {
  return line
    .split("`")
    .map((part, i) => (i % 2 ? part : part.replace(/\[([^\]]*)\]\([^)]*\)/g, "$1").replace(i ? /\*\*|__/g : /\*\*|__|^#{1,6}\s+/g, "")))
    .join("`");
}

/**
 * A row line as shown, and its hover title: the original line whenever a link's URL or a long path's
 * start is dropped, so the full line stays reachable (danielrispler/Falinks#571); else "".
 */
export function rowText(line: string): [shown: string, title: string] {
  const [shown, full] = shortPaths(plain(line));
  return [shown, full || /\]\([^)]*\)/.test(line) ? line : ""];
}

/** A subagent row's second line, as a session row's (danielrispler/Falinks#572): only while it runs or needs Daniel. */
export function agentLine(state: string, details: (string | null)[]): string | null {
  return state === "running" || state === "needs_you" ? details.filter(Boolean).join(" · ") : null;
}

/**
 * The strip's labels, one per session, from its name: its issue number, else the name's first
 * letters; sessions whose labels would match are numbered, so none reads "Fali" five times (danielrispler/Falinks#572).
 */
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

/**
 * Clear landed (#244): Clear hides every landed row and offers Undo for CLEAR_WINDOW_MS. When the window closes each hidden session
 * that is still landed goes through today's Delete (`commit`); Undo commits nothing. `changed` hears the sessions still pending.
 */
export type Clock = { set(run: () => void, ms: number): unknown; clear(id: unknown): void };
export const CLEAR_WINDOW_MS = 8000;
type ClearDeps = { landed(id: string): Session | undefined; commit(session: Session): Promise<unknown> | void; changed(pending: Session[]): void };
// What Clear holds: the rows it hides, and those in its window (`pending`).
type ClearState = { hidden: Set<string>; pending: Map<string, Session>; tell(): void };

// The window closed: each session still landed goes through Delete, and one whose Delete failed comes back.
function commitDue({ hidden, pending, tell }: ClearState, deps: ClearDeps) {
  const due = [...pending.keys()].map((id) => ({ id, session: deps.landed(id) }));
  pending.clear();
  tell();
  for (const { id, session } of due) {
    if (!session) hidden.delete(id);
    // A row whose Delete failed comes back.
    else
      void Promise.resolve(deps.commit(session)).catch(() => {
        hidden.delete(id);
        tell();
      });
  }
}

export function clearing(time: Clock, deps: ClearDeps, hidden = new Set<string>()) {
  const state: ClearState = { hidden, pending: new Map(), tell: () => deps.changed([...state.pending.values()]) };
  const { pending, tell } = state;
  let timer: unknown = null;
  const stop = () => {
    if (timer !== null) time.clear(timer);
    timer = null;
  };
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
        commitDue(state, deps);
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
      }
      if (!dropped) return;
      if (pending.size === 0) stop();
      tell();
    },
  };
}

/**
 * Whether a click on `target`, inside the session list, is on its empty space: not on a row, a group or project heading or any button.
 * Such a click goes back to the bare map.
 */
export function listSpace(target: { closest(selector: string): unknown }): boolean {
  return ![".item", "summary", ".project-head", "button"].some((selector) => target.closest(selector));
}

/** The sessions the list shows: all but the ended ones, which are in History (#94), and those Clear landed hid (`hidden`, #382). */
export const sidebarSessions = <T extends Pick<Session, "state" | "id">>(sessions: T[], hidden: ReadonlySet<string> = new Set()): T[] =>
  sessions.filter((session) => session.state !== "ended" && !hidden.has(session.id));

/** The sessions "Clear landed" would clear: every landed one it has not hidden yet; none, and the list shows no Clear landed. */
export const landedToClear = <T extends Pick<Session, "state" | "id" | "landed">>(sessions: T[], hidden: ReadonlySet<string>): T[] =>
  sessions.filter((session) => showsLanded(session.state, session.landed) && !hidden.has(session.id));

/** A row's hover text with the footprint's note as its last line, or without it: a row kept across redraws may carry an older one. */
export function withNote(title: string, note: string | null): string {
  const base = title.split("\n").filter((line) => line !== NO_FOOTPRINT).join("\n");
  return note ? `${base}\n${note}` : base;
}
