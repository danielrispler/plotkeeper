// The note toast: a turn that ended off screen, and a context warning. Neutral, never in colour, under the needs-you toast
// over the terminal's top-right corner; it fades after a few seconds, and a hover holds it. A newer note replaces it. A press
// selects its session. It never takes keyboard focus.

import { element, fullTitle, text } from "./dom.js";
import { pastWarning, sessionName } from "./list.js";
import { select } from "./selection.js";
import type { Session } from "./types.js";

export type Note = { session: Session; count: string; line: string };

const FADE_MS = 6000;

// The note for the sessions whose turn ended off screen, in the order the shell combined them: the first one's name and its
// turn's intention, and "N finished" when there are more. Null when none is listed.
export function finishedNote(ids: string[], listed: Session[]): Note | null {
  const sessions = ids.flatMap((id) => listed.filter((s) => s.id === id));
  if (!sessions.length) return null;
  const count = sessions.length > 1 ? `${sessions.length} finished` : "";
  return { session: sessions[0], count, line: sessions[0].transcript.intention ?? "" };
}

// The note for the sessions whose context share passed `threshold`: the first one's share.
export function contextNote(sessions: Session[], threshold: number): Note | null {
  if (!sessions.length) return null;
  const share = sessions[0].transcript.context?.share;
  const count = sessions.length > 1 ? `${sessions.length} past ${threshold}%` : "";
  return { session: sessions[0], count, line: `Context passed ${threshold}%: now ${share}%` };
}

// The sessions whose context share passed `threshold`, in the list's order, by `armed`: whether each session would warn
// on passing it. A session's first known share only arms it, when it is not past, so one already past when the app starts,
// or when its share first shows, raises no note. Each warns once, and is armed again when its share falls back to the
// threshold or under, as after `/compact` or `/clear`. A share not known keeps its place; an ended session is left as it is.
export function crossings(sessions: Session[], threshold: number, armed: Map<string, boolean>): Session[] {
  const crossed: Session[] = [];
  for (const session of sessions) {
    const context = session.transcript.context;
    if (session.state === "ended" || context?.share == null) continue;
    const past = pastWarning(context, threshold);
    if (past && armed.get(session.id)) crossed.push(session);
    armed.set(session.id, !past);
  }
  return crossed;
}

// Each session's arming, and the threshold it was armed at, whose change arms every session again.
const armed = new Map<string, boolean>();
let armedAt: number | null = null;

// Notes the sessions in `sessions` that passed `threshold`.
export function warnContext(sessions: Session[], threshold: number) {
  if (armedAt !== null && threshold !== armedAt) for (const id of armed.keys()) armed.set(id, true);
  armedAt = threshold;
  const note = contextNote(crossings(sessions, threshold, armed), threshold);
  if (note) showNote(note);
}

let fade: ReturnType<typeof setTimeout> | undefined;
let noted: Note | null = null;

const hide = () => {
  clearTimeout(fade);
  noted = null;
  element("note-toast").hidden = true;
};
const fadeLater = () => {
  clearTimeout(fade);
  fade = setTimeout(hide, FADE_MS);
};

// Shows `note` in place of the one showing.
export function showNote(note: Note) {
  noted = note;
  const toast = element("note-toast");
  const name = sessionName(note.session);
  const count = note.count ? [text("span", note.count, "count")] : [];
  toast.replaceChildren(fullTitle(text("strong", name, "name"), name), ...count, fullTitle(text("span", note.line, "line"), note.line));
  toast.hidden = false;
  fadeLater();
}

// The toast's press, hover and leave; once, at start.
export function watchNotes() {
  const toast = element("note-toast");
  toast.addEventListener("pointerdown", (event) => event.preventDefault());
  toast.addEventListener("click", () => {
    if (noted) select(noted.session.id);
    hide();
  });
  toast.addEventListener("pointerenter", () => clearTimeout(fade));
  toast.addEventListener("pointerleave", () => noted && fadeLater());
}
