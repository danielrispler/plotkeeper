// Home (spec #53): the view in the terminal column while no session is selected. It lists the
// sessions that need Daniel, then the unseen ones (a click selects one), then the landed ones, then one row per project to
// start a session there through the picker. The sections are content functions over the studio
// state, apart from the drawing. The landed ones come between, each with its Delete (the list's).

import { patch, pressable, text } from "./dom.js";
import { deleting, glyph, sessionName, showsLanded } from "./list.js";
import { showPicker } from "./picker.js";
import { select } from "./selection.js";
import type { Session } from "./types.js";

export type HomeSection = { key: string; title: string; sessions: Session[] };

// Home's sections in order, each with the core's order of its sessions; one with no session is absent.
export function homeSections(sessions: Session[]): HomeSection[] {
  const needsYou = sessions.filter((s) => s.state === "needs_you");
  const unseen = sessions.filter((s) => s.unseen && s.state !== "needs_you");
  // A landed session already listed above keeps its place there; its mark shows on that row.
  const above = new Set([...needsYou, ...unseen]);
  const sections: HomeSection[] = [
    { key: "needs_you", title: "Needs you", sessions: needsYou },
    { key: "unseen", title: "Not seen", sessions: unseen },
    { key: "landed", title: "Landed", sessions: sessions.filter((s) => showsLanded(s.state, s.landed) && !above.has(s)) },
  ];
  return sections.filter((section) => section.sessions.length > 0);
}

export type HomeProject = { name: string; path: string };

// The projects Home offers: the configured repository.
export function homeProjects(repo: string): HomeProject[] {
  return repo ? [{ name: repo.split("/").pop() || repo, path: repo }] : [];
}

// Whether a click on `target`, inside the session list, is on its empty space: not on a row, a group
// heading, Workspaces or any button.
export function listSpace(target: { closest(selector: string): unknown }): boolean {
  return ![".item", "summary", "#workspaces", "button"].some((selector) => target.closest(selector));
}

function sessionRow(session: Session): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => void select(session.id));
  button.dataset.key = session.id;
  button.classList.add("row");
  button.title = sessionName(session);
  const line = text("span", "", "l1");
  line.append(text("span", sessionName(session), "name"), glyph(session.state, session.unseen, session.landed));
  button.classList.toggle("unseen", session.unseen);
  button.append(line);
  return button;
}

// A landed session's row with its Delete beside it, the list's (the normal delete dialog).
function landedRow(session: Session): HTMLElement {
  const node = document.createElement("div");
  node.className = "item";
  node.dataset.key = session.id;
  const remove = deleting(session);
  node.append(sessionRow(session), ...(remove ? [remove] : []));
  return node;
}

function projectRow({ name, path }: HomeProject): HTMLElement {
  const button = pressable(document.createElement("button"), false, () => void showPicker());
  button.dataset.key = `project ${path}`;
  button.classList.add("row");
  button.title = `Start a session in ${path}`;
  const line = text("span", "", "l1");
  line.append(text("span", name, "name"), text("span", "New session", "id"));
  button.append(line);
  return button;
}

// Draws Home into `box`, patched, so focus survives the list's updates.
export function drawHome(box: HTMLElement, sessions: Session[], repo: string) {
  box.classList.add("home");
  const sections = homeSections(sessions);
  const keyed = (node: HTMLElement, key: string) => {
    node.dataset.key = key;
    return node;
  };
  const heading = (title: string, key: string) => keyed(text("h1", title), key);
  patch(box, [
    ...(sections.length ? [] : [keyed(text("p", "Nothing needs you.", "empty"), "none")]),
    ...sections.flatMap((section) => [heading(section.title, `h ${section.key}`), ...section.sessions.map(section.key === "landed" ? landedRow : sessionRow)]),
    heading("Start a session in", "h projects"),
    ...homeProjects(repo).map(projectRow),
  ]);
}
