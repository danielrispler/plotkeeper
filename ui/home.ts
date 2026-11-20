// Home (spec #53, #226): the waiting room, the view in the terminal column while no session is selected. `homeView` is a pure
// function of the sessions, the retests, the projects and the window's Home state: an ordered list of parts, each absent when
// empty. Needs you is the only colour. `drawHome` draws it, patched by key so focus survives updates.

import { age, failure, fullTitle, invoke, patch, pressable, text } from "./dom.js";
import { quizItem, quizRegion, seedQuiz } from "./quiz.js";
import { childrenOf, deleting, glyph, item, row, secondLine, selecting, sessionName, showsLanded, status } from "./list.js";
import { logo, logoKey } from "./logos.js";
import { foundWorkspaces, lastSegment, sessionProjectName } from "./workspaces.js";
import { lastHeadline } from "./story.js";
import { startLast } from "./picker.js";
import type { Retest, Session } from "./types.js";

export type HomeKey = "needs_you" | "running" | "unseen" | "learning" | "landed" | "start";
// A session's row on Home: its second line, and a muted note beside it (the intention's age).
export type HomeRow = { session: Session; line: string; note: string };
// One part of the page: `rows` are its sessions, `more` the sessions hidden behind its "+N" row.
export type HomePart = { key: HomeKey; title: string; colour: boolean; rows: HomeRow[]; more: number; retests: Retest[]; projects: HomeProject[] };
// The window's Home state: the groups Daniel expanded, kept until Home is left.
export type HomeState = { expanded: Set<string> };
export const homeState: HomeState = { expanded: new Set() };

export const MAX_RUNNING = 4;
export const MAX_UNSEEN = 3;
const endOf = (s: Session) => s.turns.at(-1)?.ended_at ?? s.last_event_at ?? 0;
const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;

export function homeView(sessions: Session[], retests: Retest[], projects: string[], state: HomeState, headline: (s: Session) => string | null = () => null): HomePart[] {
  const landed = sessions.filter((s) => showsLanded(s.state, s.landed));
  const needsYou = sessions.filter((s) => s.state === "needs_you");
  const running = sessions.filter((s) => s.state === "running");
  // A landed session that is also unseen is listed as landed, keeping its unseen styling.
  const unseen = sessions.filter((s) => s.unseen && s.state !== "needs_you" && !landed.includes(s)).sort((a, b) => endOf(b) - endOf(a));
  const offered = homeRetests(retests);
  const capped = (key: HomeKey, all: Session[], max: number, rowOf: (s: Session) => HomeRow) => {
    const shown = state.expanded.has(key) ? all : all.slice(0, max);
    return { rows: shown.map(rowOf), more: all.length - shown.length };
  };
  const plain = (rowOf: (s: Session) => HomeRow) => (all: Session[]) => ({ rows: all.map(rowOf), more: 0 });
  const part = (key: HomeKey, title: string, rows: { rows: HomeRow[]; more: number }, extra: Partial<HomePart> = {}): HomePart => ({ key, title, colour: key === "needs_you", retests: [], projects: [], ...rows, ...extra });
  const parts: HomePart[] = [
    part("needs_you", "Needs you", plain((s) => ({ session: s, line: secondLine(s) ?? "", note: "" }))(needsYou)),
    part("running", "Running", capped("running", running, MAX_RUNNING, (s) => {
      const steps = s.transcript?.steps_ago ?? 0;
      return { session: s, line: s.transcript?.intention ?? s.line ?? "", note: steps === 0 ? "" : `${plural(steps, "step")} ago` };
    })),
    part("unseen", "Unseen", capped("unseen", unseen, MAX_UNSEEN, (s) => ({ session: s, line: headline(s) ?? "", note: "" }))),
    part("learning", "Retest", { rows: [], more: 0 }, { retests: offered }),
    part("landed", "Landed", plain((s) => ({ session: s, line: "", note: "" }))(landed)),
    part("start", "Start a session in", { rows: [], more: 0 }, { projects: homeProjects(projects) }),
  ];
  return parts.filter((p) => p.rows.length || p.retests.length || p.projects.length);
}

// The retests Home offers (#129): those with a Quiz that he has not answered, and those he answered while this window showed
// them, so the answer's reasons stay until the window is closed. Nothing else shows a retest.
const answeredHere = new Set<string>();
export function homeRetests(retests: Retest[]): Retest[] {
  const answered = (r: Retest) => r.taps.some((t) => t.kind === "pick" || t.kind === "skip");
  return retests.filter((r) => r.quiz && (answeredHere.has(r.id) || !answered(r)));
}

function retestRow(retest: Retest, quiz: NonNullable<Retest["quiz"]>): HTMLElement {
  seedQuiz(retest.id, retest.taps);
  // The self-test times the pick from the retest's first showing (#130).
  if (!retest.taps.some((t) => t.kind === "shown")) void invoke("quiz_tap", { id: retest.id, tap: { kind: "shown" } }).catch(failure("quiz_tap"));
  answeredHere.add(retest.id);
  const region = quizRegion(retest.id, quiz, [], {
    record: (tap) => void invoke("quiz_tap", { id: retest.id, tap }).catch(failure("quiz_tap")),
    explain: () => invoke("ask", { session: retest.session_id, item: quizItem(quiz), question: "I did not get this yet. Explain more." }),
    line: () => document.createElement("li"),
    trial: true,
  });
  region.dataset.key = `retest ${retest.id}`;
  return region;
}

export type HomeProject = { name: string; path: string };

// The projects Home offers: one per entry of `projects.json`, in its order.
export function homeProjects(paths: string[]): HomeProject[] {
  return paths.map((path) => ({ name: lastSegment(path) || path, path }));
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
  const clock = part === "running" && session.turn_span ? age(session.turn_span.start) : part === "unseen" ? `${age(session.turns.at(-1)?.ended_at ?? session.last_event_at)} ago` : "";
  first.append(logo(logoKey(session)), fullTitle(text("span", sessionName(session), "name"), sessionName(session)), text("span", project, "id"), ...(clock ? [text("span", clock, "clock")] : []), ...(part === "needs_you" ? [glyph(session.state)] : []));
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

function projectRow({ name, path }: HomeProject): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => void startLast(path, "").catch(failure("start", "error")));
  button.dataset.key = `project ${path}`;
  button.classList.add("row");
  button.title = `Start a session in ${path}`;
  const line = text("span", "", "l1");
  line.append(text("span", name, "name"), text("span", "New session", "id"));
  button.append(line);
  return button;
}

// Draws Home into `box`, patched, so focus survives the list's updates; `redraw` draws it again after an expansion.
export function drawHome(box: HTMLElement, sessions: Session[], projects: string[], retests: Retest[] = [], redraw: () => void = () => {}) {
  box.classList.add("home");
  const parts = homeView(sessions, retests, projects, homeState, lastHeadline);
  const found = foundWorkspaces();
  const keyed = (node: HTMLElement, key: string) => {
    node.dataset.key = key;
    return node;
  };
  const heading = (title: string, key: string) => keyed(text("h2", title), key);
  const drawPart = (part: HomePart): HTMLElement[] => [
    heading(part.title, `h ${part.key}`),
    ...part.rows.map((r) => (part.key === "landed" ? sessionRow(r.session, true) : waitingRow(r, part.key, sessionProjectName(projects, found, r.session)))),
    ...(part.more ? [moreRow(part.key, part.more, redraw)] : []),
    ...part.retests.flatMap((r) => (r.quiz ? [retestRow(r, r.quiz)] : [])),
    ...part.projects.map(projectRow),
  ];
  patch(box, [
    keyed(text("h1", new Date().toLocaleDateString("en-GB", { weekday: "long", day: "numeric", month: "long" }), "title"), "title"),
    ...(parts.some((p) => p.key === "needs_you") ? [] : [keyed(text("p", "Nothing needs you", "empty"), "none")]),
    ...parts.flatMap(drawPart),
  ]);
}
