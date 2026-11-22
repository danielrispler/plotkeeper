// Home (spec #53, #226): the waiting room, the view in the terminal column while no session is selected. `homeView` is a pure
// function of the sessions, the projects and the window's Home state: an ordered list of parts, each absent when
// empty. Needs you is the only colour. `drawHome` draws it, patched by key so focus survives updates.

import { age, failure, fullTitle, invoke, patch, pressable, text } from "./dom.js";
import { childrenOf, deleting, glyph, item, row, secondLine, selecting, sessionName, showsLanded, status } from "./list.js";
import { logo, logoKey } from "./logos.js";
import { foundWorkspaces, lastSegment, sessionProjectName } from "./workspaces.js";
import { lastHeadline } from "./story.js";
import { startLast } from "./picker.js";
import { counts } from "./times.js";
import { closePane, homeShown, redrawHome } from "./panes.js";
import { hideClearToast, showClearToast } from "./toast.js";
import { studio } from "./state.js";
import { mapShown, openMap } from "./map.js";
import { watchSeen } from "./seen.js";
import { homeSeen } from "./layout.js";
import { logStop, onHome, reviewNode, reviewOf, type Module } from "./stop-rules.js";
import type { Session } from "./types.js";

export type HomeKey = "needs_you" | "running" | "unseen" | "landed" | "start";
// A session's row on Home: its second line, and a muted note beside it (the intention's age).
export type HomeRow = { session: Session; line: string; note: string };
// One part of the page: `rows` are its sessions, `more` the sessions hidden behind its "+N" row.
export type HomePart = { key: HomeKey; title: string; colour: boolean; rows: HomeRow[]; more: number; projects: HomeProject[] };
// The window's Home state: the groups Daniel expanded, kept until Home is left.
// `cleared` is the rows Clear hides, the one set `landedClear` keeps; leaving Home does not reset it.
export type HomeState = { expanded: Set<string>; cleared: Set<string> };
export const homeState: HomeState = { expanded: new Set(), cleared: new Set() };
export function resetHome(state: HomeState = homeState) {
  state.expanded.clear();
}

// Entering Home from another view starts it afresh; drawing it again while it is on screen keeps what Daniel did there.
export function enterHome(already: boolean, state: HomeState = homeState) {
  if (!already) {
    resetHome(state);
    staleMarks();
  }
}

export const MAX_RUNNING = 4;
export const MAX_UNSEEN = 3;
const endOf = (s: Session) => s.turns.at(-1)?.ended_at ?? s.last_event_at ?? 0;
const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

export function homeView(sessions: Session[], projects: string[], state: HomeState, headline: (s: Session) => string | null = () => null, fogged: Set<string> = new Set()): HomePart[] {
  const allLanded = sessions.filter((s) => showsLanded(s.state, s.landed));
  const landed = allLanded.filter((s) => !state.cleared.has(s.id));
  const needsYou = sessions.filter((s) => s.state === "needs_you");
  const running = sessions.filter((s) => s.state === "running");
  // A landed session that is also unseen is listed as landed, keeping its unseen styling.
  const unseen = sessions.filter((s) => s.unseen && (s.state === "your_turn" || s.state === "idle") && !allLanded.includes(s)).sort((a, b) => endOf(b) - endOf(a));
  const capped = (key: HomeKey, all: Session[], max: number, rowOf: (s: Session) => HomeRow) => {
    const shown = state.expanded.has(key) ? all : all.slice(0, max);
    return { rows: shown.map(rowOf), more: all.length - shown.length };
  };
  const plain = (rowOf: (s: Session) => HomeRow) => (all: Session[]) => ({ rows: all.map(rowOf), more: 0 });
  const part = (key: HomeKey, title: string, rows: { rows: HomeRow[]; more: number }, extra: Partial<HomePart> = {}): HomePart => ({ key, title, colour: key === "needs_you", projects: [], ...rows, ...extra });
  const parts: HomePart[] = [
    part("needs_you", "Needs you", plain((s) => ({ session: s, line: secondLine(s) ?? "", note: "" }))(needsYou)),
    part("running", "Running", capped("running", running, MAX_RUNNING, (s) => {
      const steps = s.transcript?.steps_ago ?? 0;
      return { session: s, line: s.transcript?.intention ?? s.line ?? "", note: steps === 0 ? "" : `${plural(steps, "step")} ago` };
    })),
    part("unseen", "Unseen", capped("unseen", unseen, MAX_UNSEEN, (s) => ({ session: s, line: headline(s) ?? "", note: "" }))),
    part("landed", "Landed", plain((s) => ({ session: s, line: "", note: "" }))(landed)),
    part("start", "Start a session in", { rows: [], more: 0 }, { projects: homeProjects(projects, fogged) }),
  ];
  return parts.filter((p) => p.rows.length || p.projects.length);
}

// Clear landed (#244): Clear hides every landed row and offers Undo for CLEAR_WINDOW_MS. When the window closes each hidden session
// that is still landed goes through today's Delete (`commit`); Undo commits nothing. `changed` hears the sessions still pending.
export type Clock = { set(run: () => void, ms: number): unknown; clear(id: unknown): void };
export const CLEAR_WINDOW_MS = 8000;
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

// The window's one Clear: its hidden rows are `homeState.cleared`, the toast offers Undo, and the shell is told which sessions a quit
// would commit (`clearing`). A commit is today's Delete: the terminal ended, the row dropped, the cards kept.
const stillLanded = (id: string) => studio.listed.find((s) => s.id === id && showsLanded(s.state, s.landed));
// Clear's and Delete's one rule (#244, #270): a terminal is in use while a session that is not leaving the list, and not ended, runs in
// it. An ended session keeps the terminal a later one may now use.
export const terminalInUse = (terminal: string, listed: Session[], leaving: (id: string) => boolean) =>
  listed.some((s) => s.terminal === terminal && s.state !== "ended" && !leaving(s.id));
// A terminal is ended once, and never while a session that is not being cleared still runs in it.
const closed = new Set<string>();
const terminalFree = (terminal: string) =>
  !closed.has(terminal) && !terminalInUse(terminal, studio.listed, (id) => landedClear.hidden.has(id)) && (closed.add(terminal), true);
export const landedClear = clearing(
  { set: (run, ms) => setTimeout(run, ms), clear: (id) => clearTimeout(id as number) },
  {
    landed: stillLanded,
    commit: (session) =>
      (async () => {
        if (session.terminal !== null && terminalFree(session.terminal)) await closePane(session.terminal);
        await invoke("delete_session", { id: session.id });
      })().catch((e) => {
        failure("delete_session", "error")(e);
        throw e;
      }),
    changed: (pending, writing) => {
      void invoke("clearing", { sessions: [...pending, ...writing].map((s) => ({ id: s.id, terminal: s.terminal })) }).catch(failure("clearing"));
      if (pending.length) showClearToast(pending.length, () => landedClear.undo());
      else hideClearToast();
      redrawHome();
    },
  },
  homeState.cleared,
);

// `fogged`: a part of its map that Daniel tracks as Core shows fog (#310); Home marks the row quietly, with no count.
// What the project's quiet dot says on hover: it lights for fog on a Core part, a broken law or a PR brief waiting (#344).
export const markTitle = "Something you track has changed, a law is broken or a PR brief is waiting: open the map";
export type HomeProject = { name: string; path: string; fogged: boolean };

// The projects Home offers: one per entry of `projects.json`, in its order.
export function homeProjects(paths: string[], fogged: Set<string> = new Set()): HomeProject[] {
  return paths.map((path) => ({ name: lastSegment(path) || path, path, fogged: fogged.has(path) }));
}

// The projects the shell reports with a fogged Core part, read when Home is entered and when the window gains focus, never on a timer.
const marked = new Set<string>();
let marksStale = true;
export const staleMarks = () => void (marksStale = true);
// The window gained focus: the marks are stale, and Home, when it is open, draws again to read them (that draw reads them).
export function onWindowFocus(homeOpen: boolean, redraw: () => void) {
  staleMarks();
  if (homeOpen) redraw();
}
if (typeof window !== "undefined") window.addEventListener?.("focus", () => onWindowFocus(homeShown(), redrawHome));
async function readMarks(paths: string[], redraw: () => void) {
  marksStale = false;
  const now = await invoke("map_marks", { projects: paths });
  const review = await readReviews(paths);
  const same = now.length === marked.size && now.every((path) => marked.has(path));
  if (same && !review) return;
  marked.clear();
  now.forEach((path) => marked.add(path));
  redraw();
}

// The one-time review on Home (#314): only for the projects whose Map view fell below its threshold, so an unused map still reaches
// Daniel. It lists that project's unused modules, never a count. Read with the marks, when Home is entered and on focus.
let homeReview: { modules: Module[]; projects: string[] } | null = null;
async function readReviews(paths: string[]): Promise<boolean> {
  const found: Record<string, Module[] | null> = {};
  await Promise.all(paths.map(async (path) => void (found[path] = await reviewOf(path).catch(() => null))));
  const projects = onHome(found);
  const next = projects.length ? { modules: found[projects[0]]!, projects } : null;
  const changed = JSON.stringify(next) !== JSON.stringify(homeReview);
  homeReview = next;
  return changed;
}

// Whether a click on `target`, inside the session list, is on its empty space: not on a row, a group
// or project heading, a workspace or any button.
export function listSpace(target: { closest(selector: string): unknown }): boolean {
  return ![".item", "summary", ".project-head", ".project-workspaces", "button"].some((selector) => target.closest(selector));
}

// A session's row, the list's, with its Delete beside it on a landed one.
function sessionRow(session: Session, landed: boolean): HTMLElement {
  return item(session.id, 0, null, row(session, childrenOf(session)), landed ? deleting(session) : null);
}

// A Home row of a session that needs Daniel, runs or is unseen: its logo, name, project and clock, then its line, muted.
function waitingRow({ session, line, note }: HomeRow, part: HomeKey, project: string): HTMLElement {
  const button = selecting(session.id, null, `${part} ${session.id}`);
  button.classList.add("row", "home-row");
  button.classList.toggle("unseen", !!session.unseen);
  button.title = `${sessionName(session)} · ${status(session.state, session.unseen, session.landed)[1]}`;
  const first = text("span", "", "l1");
  // The clock counts up like the list's (#226), not frozen at the draw.
  const since = part === "running" ? session.turn_span?.start : part === "unseen" ? (session.turns.at(-1)?.ended_at ?? session.last_event_at) : undefined;
  const clock = since === undefined ? [] : [counts(text("span", "", "clock"), (now) => (part === "unseen" ? `${age(since, now)} ago` : age(since, now)))];
  first.append(logo(logoKey(session)), fullTitle(text("span", sessionName(session), "name"), sessionName(session)), text("span", project, "id"), ...clock, ...(part === "needs_you" || part === "unseen" ? [glyph(session.state, session.unseen, session.landed)] : []));
  button.append(first);
  if (line) {
    const second = text("span", "", "l2");
    second.append(text("span", line, "row-step"), ...(note ? [text("span", note, "id")] : []));
    fullTitle(second, line);
    button.append(second);
  }
  return button;
}

// The "+N running" row, which expands its group inline until Home is left.
function moreRow(key: HomeKey, count: number, redraw: () => void): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => {
    homeState.expanded.add(key);
    redraw();
  });
  button.dataset.key = `more ${key}`;
  button.classList.add("row", "more");
  button.textContent = `+${count} ${key}`;
  return button;
}

// Each project mark's watch, and the node it watches: a redraw that keeps the node keeps its watch, one that drops it stops it.
const marksSeen = new Map<string, { node: Element; stop: () => void }>();

// Watches `node`, the mark of `path` as it is in the DOM now (null: the project has no mark); the mark counts as seen when it is in view
// in a focused window with Home open (the shared Seen), and a day is logged once.
export function watchMark(path: string, node: Element | null) {
  const had = marksSeen.get(path);
  if (had && had.node === node) return;
  had?.stop();
  marksSeen.delete(path);
  if (!node) return;
  // Once seen, the entry goes: the next redraw watches the same node again, and `logStop` sends one event a day per project, so a mark
  // still in view on a later day is logged while Studio stays open.
  const stop = watchSeen(node, {
    active: () => homeSeen(homeShown(), mapShown()),
    onSeen: () => {
      if (marksSeen.get(path)?.stop === stop) marksSeen.delete(path);
      logStop("mark-seen", path);
    },
  });
  marksSeen.set(path, { node, stop });
}

function projectRow({ name, path, fogged }: HomeProject): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => void startLast(path, "").catch(failure("start", "error")));
  button.dataset.key = `project ${path}`;
  button.classList.add("row");
  button.title = `Start a session in ${path}`;
  const line = text("span", "", "l1");
  line.append(text("span", name, "name"), text("span", "New session", "id"));
  if (fogged) {
    // One quiet mark, no count; pressing it opens the project's Map view and not the new session the row starts.
    const mark = text("span", "", "fog-mark");
    mark.setAttribute("role", "button");
    mark.title = markTitle;
    mark.addEventListener("click", (event) => {
      event.stopPropagation();
      openMap(path);
    });
    line.append(mark);
  }
  button.append(line);
  return button;
}

// Draws Home into `box`, patched, so focus survives the list's updates; `redraw` draws it again after an expansion.
export function drawHome(box: HTMLElement, sessions: Session[], projects: string[], redraw: () => void = () => {}) {
  box.classList.add("home");
  if (marksStale && projects.length) readMarks(projects, redraw).catch(failure("map_marks"));
  const parts = homeView(sessions, projects, homeState, lastHeadline, marked);
  const found = foundWorkspaces();
  const keyed = (node: HTMLElement, key: string) => {
    node.dataset.key = key;
    return node;
  };
  const heading = (title: string, key: string) => {
    const node = keyed(text("h2", title), key);
    if (key !== "h landed") return node;
    const clear = text("button", "Clear", "btn quiet small clear");
    clear.title = "Clear every landed session; Undo for a few seconds";
    clear.addEventListener("click", () => landedClear.clear(studio.listed.filter((s) => showsLanded(s.state, s.landed) && !landedClear.hidden.has(s.id))));
    node.append(clear);
    return node;
  };
  const drawPart = (part: HomePart): HTMLElement[] => [
    ...(part.title ? [heading(part.title, `h ${part.key}`)] : []),
    ...part.rows.map((r) => (part.key === "landed" ? sessionRow(r.session, true) : waitingRow(r, part.key, sessionProjectName(projects, found, r.session)))),
    ...(part.more ? [moreRow(part.key, part.more, redraw)] : []),
    ...part.projects.map(projectRow),
  ];
  patch(box, [
    keyed(text("h1", new Date().toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" }), "title"), "title"),
    ...(parts.some((p) => p.key === "needs_you") ? [] : [keyed(text("p", "Nothing needs you", "empty"), "none")]),
    ...parts.flatMap(drawPart),
    ...(homeReview
      ? [keyed(reviewNode(homeReview.modules, homeReview.projects, () => {
          homeReview = null;
          redraw();
        }), "review")]
      : []),
  ]);
  // The marks that patch left in the DOM, not the ones just built and discarded.
  const rows = new Map([...box.children].map((child) => [(child as HTMLElement).dataset.key, child]));
  for (const path of new Set([...marksSeen.keys(), ...marked])) watchMark(path, rows.get(`project ${path}`)?.querySelector(".fog-mark") ?? null);
}
