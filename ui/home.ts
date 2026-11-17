// Home (spec #53): the view in the terminal column while no session is selected. It lists the
// sessions that need Daniel, the unseen ones and the landed ones, each with its Delete, then a row
// per project to start a session in. The sections are content functions over the studio state.

import { invoke, patch, pressable, text } from "./dom.js";
import { quizItem, quizRegion, seedQuiz } from "./quiz.js";
import { childrenOf, deleting, item, row, showsLanded } from "./list.js";
import { lastSegment } from "./workspaces.js";
import { startLast } from "./picker.js";
import type { Retest, Session } from "./types.js";

export type HomeSection = { key: string; title: string; sessions: Session[] };

// Home's sections in order, each with the core's order of its sessions; one with no session is absent.
export function homeSections(sessions: Session[]): HomeSection[] {
  const needsYou = sessions.filter((s) => s.state === "needs_you");
  const landed = sessions.filter((s) => showsLanded(s.state, s.landed));
  // A landed session that is also unseen is listed as landed, keeping its unseen styling.
  const unseen = sessions.filter((s) => s.unseen && s.state !== "needs_you" && !landed.includes(s));
  const sections: HomeSection[] = [
    { key: "needs_you", title: "Needs you", sessions: needsYou },
    { key: "unseen", title: "Unseen", sessions: unseen },
    { key: "landed", title: "Landed", sessions: landed },
  ];
  return sections.filter((section) => section.sessions.length > 0);
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
  if (!retest.taps.some((t) => t.kind === "shown")) void invoke("quiz_tap", { id: retest.id, tap: { kind: "shown" } }).catch((e) => console.warn(e));
  answeredHere.add(retest.id);
  const region = quizRegion(retest.id, quiz, [], {
    record: (tap) => void invoke("quiz_tap", { id: retest.id, tap }).catch((e) => console.warn(e)),
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

function projectRow({ name, path }: HomeProject): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => void startLast(path, "").catch((e) => console.warn(e)));
  button.dataset.key = `project ${path}`;
  button.classList.add("row");
  button.title = `Start a session in ${path}`;
  const line = text("span", "", "l1");
  line.append(text("span", name, "name"), text("span", "New session", "id"));
  button.append(line);
  return button;
}

// Draws Home into `box`, patched, so focus survives the list's updates.
export function drawHome(box: HTMLElement, sessions: Session[], projects: string[], retests: Retest[] = []) {
  box.classList.add("home");
  const sections = homeSections(sessions);
  const retesting = homeRetests(retests);
  const keyed = (node: HTMLElement, key: string) => {
    node.dataset.key = key;
    return node;
  };
  const heading = (title: string, key: string) => keyed(text("h1", title), key);
  patch(box, [
    ...(sections.length ? [] : [keyed(text("p", "Nothing needs you.", "empty"), "none")]),
    ...sections.flatMap((section) => [heading(section.title, `h ${section.key}`), ...section.sessions.map((session) => sessionRow(session, section.key === "landed"))]),
    ...(retesting.length ? [heading("Retest", "h retest"), ...retesting.flatMap((r) => (r.quiz ? [retestRow(r, r.quiz)] : []))] : []),
    heading("Start a session in", "h projects"),
    ...homeProjects(projects).map(projectRow),
  ]);
}
