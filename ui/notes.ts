// The note toast: a turn that ended off screen. Neutral, never in colour, under the needs-you toast
// over the map's top-right corner; it fades after a few seconds, and a hover or the keyboard's focus in it holds it (#457). A newer
// note replaces it. Its main button selects its session, and its × hides it.

import { element, fullTitle, text } from "./dom.js";
import { type Clock, realClock } from "./hover.js";
import { sessionName } from "./list.js";
import { select } from "./selection.js";
import { stackToasts, toastButtons, toastSays } from "./toast.js";
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

export type Hold = "pointer" | "focus" | "modal";

// The note's fade on `clock`: `start` (a note shows) fades it in 6 s unless something holds it, the pointer resting on it, the
// keyboard's focus in it or a modal dialog holding it back; once none does, it fades 6 s later. `stop`: the note was closed, and nothing holds it any more.
export function noteFade(clock: Clock, hide: () => void) {
  const holds = new Set<Hold>();
  let timer: number | undefined;
  let shown = false;
  const later = () => {
    if (timer !== undefined) clock.clear(timer);
    timer = shown && !holds.size ? clock.set(() => ((shown = false), hide()), FADE_MS) : undefined;
  };
  return {
    start() {
      shown = true;
      later();
    },
    stop() {
      shown = false;
      holds.clear();
      later();
    },
    hold(why: Hold) {
      holds.add(why);
      later();
    },
    release(why: Hold) {
      holds.delete(why);
      later();
    },
  };
}

let noted: Note | null = null;

const hide = () => {
  noted = null;
  element("note-toast").hidden = true;
  stackToasts();
};
const fade = noteFade(realClock, hide);
const close = () => {
  fade.stop();
  hide();
};
// A modal dialog holds the note back (`stackToasts`), so it holds the fade too: a note never fades before it has shown (#452's outside
// review).
const modalHolds = () => (document.querySelector("dialog:modal") ? fade.hold("modal") : fade.release("modal"));

// Shows `note` in place of the one showing.
export function showNote(note: Note) {
  noted = note;
  const toast = element("note-toast");
  const action = () => {
    const button = text("button", "", "btn quiet toast-main");
    button.addEventListener("click", () => {
      if (noted) select(noted.session.id);
      close();
    });
    return button;
  };
  const { main } = toastButtons(toast, action, close, "Hide this note");
  const name = sessionName(note.session);
  const count = note.count ? [text("span", note.count, "count")] : [];
  main.replaceChildren(fullTitle(text("strong", name, "name"), name), ...count, fullTitle(text("span", note.line, "line"), note.line));
  toast.dataset.say = toastSays([name, note.count, note.line]);
  toast.hidden = false;
  stackToasts();
  modalHolds();
  fade.start();
}

// The toast's hover, focus and a modal dialog hold its fade; once, at start.
export function watchNotes() {
  const toast = element("note-toast");
  document.addEventListener("toggle", modalHolds, true);
  toast.addEventListener("pointerenter", () => fade.hold("pointer"));
  toast.addEventListener("pointerleave", () => fade.release("pointer"));
  toast.addEventListener("focusin", () => fade.hold("focus"));
  toast.addEventListener("focusout", () => fade.release("focus"));
}
