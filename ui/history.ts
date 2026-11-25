// History (#94, spec #90): the view in the terminal column that holds the ended sessions, newest first, with one search box
// over session name, project and branch. The functions are content functions over the studio state.

import { fullTitle, patch, text } from "./dom.js";
import { deleting, item, row } from "./list.js";
import { childrenOf, sessionName } from "./list-model.js";
import { studio } from "./state.js";
import { sessionProjectName } from "./projects.js";
import type { Session } from "./types.js";

type Searched = Pick<Session, "name" | "identity" | "folder_name" | "cwd" | "transcript"> & { prompt_name?: string | null };

/** The ended sessions, newest `last_event_at` first. */
export function endedSessions<T extends Pick<Session, "state" | "last_event_at">>(sessions: T[]): T[] {
  return sessions.filter((s) => s.state === "ended").sort((a, b) => b.last_event_at - a.last_event_at);
}

/** The project a session belongs to (#97). */
export const projectName = (session: Pick<Session, "cwd">): string => sessionProjectName(session);

/** A session's branch: the transcript's, else the identity's; "" when unknown. */
export const branchOf = (session: Pick<Session, "identity" | "transcript">) => session.transcript?.branch ?? session.identity?.branch ?? "";

/** Whether `query`, a case-insensitive substring, is in the session's name, project or branch; an empty query matches all. */
export function matches(session: Searched, query: string): boolean {
  const needle = query.trim().toLowerCase();
  return !needle || [sessionName(session), projectName(session), branchOf(session)].some((field) => field.toLowerCase().includes(needle));
}

/** The History's rows: the ended sessions that `query` matches, newest first. */
export const historyRows = <T extends Searched & Pick<Session, "state" | "last_event_at">>(sessions: T[], query: string): T[] =>
  endedSessions(sessions).filter((session) => matches(session, query));

let query = "";

/** Draws History into `box`, patched, so the search box keeps focus across the list's updates. */
export function drawHistory(box: HTMLElement, sessions: Session[]) {
  box.classList.add("history");
  const search = document.createElement("input");
  search.type = "search";
  search.name = "history-search";
  search.autocomplete = "off";
  search.placeholder = "Search by name, project or branch…";
  search.setAttribute("aria-label", "Search history");
  search.dataset.key = "search";
  search.value = query;
  search.oninput = () => {
    query = search.value;
    drawHistory(box, studio.listed);
  };
  const rows = historyRows(sessions, query);
  const empty = text("p", query.trim() ? "No ended session matches." : "No ended sessions.", "empty");
  empty.dataset.key = "none";
  patch(box, [
    search,
    ...(rows.length ? [] : [empty]),
    ...rows.map((session) => {
      const line = row(session, childrenOf(session));
      const detail = [projectName(session), branchOf(session)].filter(Boolean).join(" · ");
      line.append(fullTitle(text("span", detail, "l2"), detail));
      return item(session.id, 0, null, line, deleting(session));
    }),
  ]);
}
