// Home (spec #53, #226): the waiting room, the view in the terminal column while no session is selected. `homeView` is a pure
// function of the sessions, the projects and the window's Home state: an ordered list of parts, each absent when
// empty. Needs you is the only colour. `drawHome` draws it, patched by key so focus survives updates.

import { age, failure, fullTitle, invoke, patch, pressable, text } from "./dom.js";
import { childrenOf, deleting, glyph, item, landedClear, landedToClear, row, secondLine, selecting, sessionName, showsLanded, status } from "./list.js";
import { logo, logoKey } from "./logos.js";
import { lastSegment, sessionProjectName } from "./projects.js";
import { lastHeadline } from "./story.js";
import { counts } from "./times.js";
import { homeShown, redrawHome } from "./panes.js";
import { studio } from "./state.js";
import { openMap } from "./map.js";
import type { Session } from "./types.js";

export type HomeKey = "needs_you" | "running" | "unseen" | "landed" | "projects";
// A session's row on Home: its second line, and a muted note beside it (the intention's age).
export type HomeRow = { session: Session; line: string; note: string };
// One part of the page: `rows` are its sessions, `more` the sessions hidden behind its "+N" row.
export type HomePart = { key: HomeKey; title: string; colour: boolean; rows: HomeRow[]; more: number; projects: HomeProject[] };
// The window's Home state: the groups Daniel expanded, kept until Home is left.
// `cleared` is the rows Clear hides, the one set `landedClear` keeps; leaving Home does not reset it.
export type HomeState = { expanded: Set<string>; cleared: Set<string> };
// `cleared` is read when drawn, as home.js and list.js import each other.
export const homeState: HomeState = {
  expanded: new Set(),
  get cleared() {
    return landedClear.hidden;
  },
};
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
    part("projects", "Projects", { rows: [], more: 0 }, { projects: homeProjects(projects, fogged) }),
  ];
  return parts.filter((p) => p.rows.length || p.projects.length);
}

// Clear landed (#244) lives with the list (#382), which shows it at its end; Home's Clear is the same one.
export { clearing, CLEAR_WINDOW_MS, landedClear, type Clock } from "./list.js";

// `fogged`: a part of its map that Daniel tracks as Core shows fog (#310); Home marks the row quietly, with no count.
// What the project's quiet dot says on hover: it lights for fog on a Core part, a broken law or a PR brief waiting (#344).
export const markTitle = "Something you track has changed, a law is broken or a PR brief is waiting: open the map";
export type HomeProject = { name: string; path: string; fogged: boolean };

// The projects Home offers: one per repository a session ran in; a row opens the project's map.
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
  const now = (await invoke("map_marks", { projects: paths })).map((mark) => mark.project);
  const same = now.length === marked.size && now.every((path) => marked.has(path));
  if (same) return;
  marked.clear();
  now.forEach((path) => marked.add(path));
  redraw();
}

// Whether a click on `target`, inside the session list, is on its empty space: not on a row, a group
// or project heading or any button.
export function listSpace(target: { closest(selector: string): unknown }): boolean {
  return ![".item", "summary", ".project-head", "button"].some((selector) => target.closest(selector));
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

function projectRow({ name, path, fogged }: HomeProject): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => openMap(path));
  button.dataset.key = `project ${path}`;
  button.classList.add("row");
  button.title = `Open the map of ${path}`;
  const line = text("span", "", "l1");
  line.append(text("span", name, "name"), text("span", "Map", "id"));
  if (fogged) {
    // One quiet mark, no count.
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
  const keyed = (node: HTMLElement, key: string) => {
    node.dataset.key = key;
    return node;
  };
  const heading = (title: string, key: string) => {
    const node = keyed(text("h2", title), key);
    if (key !== "h landed") return node;
    const clear = text("button", "Clear", "btn quiet small clear");
    clear.title = "Clear every landed session; Undo for a few seconds";
    clear.addEventListener("click", () => landedClear.clear(landedToClear(studio.listed, landedClear.hidden)));
    node.append(clear);
    return node;
  };
  const drawPart = (part: HomePart): HTMLElement[] => [
    ...(part.title ? [heading(part.title, `h ${part.key}`)] : []),
    ...part.rows.map((r) => (part.key === "landed" ? sessionRow(r.session, true) : waitingRow(r, part.key, sessionProjectName(r.session)))),
    ...(part.more ? [moreRow(part.key, part.more, redraw)] : []),
    ...part.projects.map(projectRow),
  ];
  patch(box, [
    keyed(text("h1", new Date().toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" }), "title"), "title"),
    ...(parts.some((p) => p.key === "needs_you") ? [] : [keyed(text("p", "Nothing needs you", "empty"), "none")]),
    ...parts.flatMap(drawPart),
  ]);
}
