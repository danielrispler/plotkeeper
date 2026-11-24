// The note toast: a turn that ended off screen. Neutral, never in colour, under the needs-you toast
// over the terminal's top-right corner; it fades after a few seconds, and a hover holds it. A newer note replaces it. A press
// selects its session. It never takes keyboard focus.

import { element, fullTitle, text } from "./dom.js";
import { sessionName } from "./list.js";
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
