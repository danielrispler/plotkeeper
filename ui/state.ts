// The session list and the selection, as the server last sent them, and the settings files in effect. The page reads `studio` anywhere
// and changes it only through the setters below (spec #548, Group 4).

import { initial } from "./settings.js";
import type { SavedProject, Selection, Session, SessionCards, SettingsFiles } from "./types.js";

/** The press waiting for its reply: the row Daniel just pressed, shown selected until the server replies (danielrispler/Falinks#570). */
type Pending = { id: string; agent: string | null };

export const studio = {
  listed: [] as Session[],
  // The saved project list, as the server last sent it (`projects.rs`).
  projects: [] as SavedProject[],
  selection: null as Selection | null,
  pending: null as Pending | null,
  // A `selected` or `cards` event came while `pending` waited, so the press's settling fetches both.
  missed: false,
  // The selected session's brief, cards and decisions, as the server last sent them.
  cards: { brief: null, goal: null, answers: [], open: [], design: [], decisions: [], risk_notes: [], decision_notes: [] } as SessionCards,
  // Focus mode (danielrispler/Falinks#573): the list and the rail folded, the terminal takes the window.
  focus: false,
  files: initial,
  // The two files' text as the server last sent it: what a settings write is made from.
  texts: { settings: null, keymap: null } as SettingsFiles,
};

/** The session list, as the server sent it. */
export const setListed = (sessions: Session[]) => {
  studio.listed = sessions;
};
/** The saved project list, as the server sent it. */
export const setProjects = (projects: SavedProject[]) => {
  studio.projects = projects;
};
/** The selection the server returned. */
export const setSelection = (selection: Selection | null) => {
  studio.selection = selection;
};
/** The press now waiting for its reply, or none. */
export const setPending = (pending: Pending | null) => {
  studio.pending = pending;
};
/** Whether an event came while a press waited, so its settling fetches the selection and the cards again. */
export const setMissed = (missed: boolean) => {
  studio.missed = missed;
};
/** The selected session's cards, as the server sent them. */
export const setCards = (cards: SessionCards) => {
  studio.cards = cards;
};
/** Focus mode on or off. */
export const setFocus = (focus: boolean) => {
  studio.focus = focus;
};
/** The settings files' text as the server sent it, and the files read from it. */
export const setSettings = (texts: SettingsFiles, files: typeof studio.files) => {
  studio.texts = texts;
  studio.files = files;
};

/**
 * Whether row `session`, or its subagent `agent`'s line, shows selected: the one Daniel just pressed until the server replies, else the
 * server's selection, so a reply that disagrees moves the mark back (danielrispler/Falinks#570). With no `agent`, any of the session's
 * lines counts, as the strip shows.
 */
export function isSelected(session: string, agent?: string | null): boolean {
  const shown = studio.pending ?? (studio.selection && { id: studio.selection.session.id, agent: studio.selection.agent });
  return shown?.id === session && (agent === undefined || shown.agent === agent);
}
