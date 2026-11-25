// The note toast: a turn that ended off screen. Neutral, never in colour, under the needs-you toast
// over the map's top-right corner; it fades after a few seconds, and a hover or the keyboard's focus in it holds it (#457). A newer
// note replaces it. Its main button selects its session, and its × hides it.

import { element, fullTitle, pressable, promptBox, text } from "./dom.js";
import { type Clock, realClock } from "./hover.js";
import { sessionName } from "./list-model.js";
import { select } from "./selection.js";
import { stackToasts, toastButtons, toastSays } from "./toast.js";
import type { Session } from "./types.js";

export type Note = { session: Session; count: string; line: string };

const FADE_MS = 6000;

/**
 * The note for the sessions whose turn ended off screen, in the order the server combined them: the first one's name and its
 * turn's intention, and "N finished" when there are more. Null when none is listed.
 */
export function finishedNote(ids: string[], listed: Session[]): Note | null {
  const sessions = ids.flatMap((id) => listed.filter((s) => s.id === id));
  const first = sessions[0];
  if (!first) return null;
  const count = sessions.length > 1 ? `${sessions.length} finished` : "";
  return { session: first, count, line: first.transcript.intention ?? "" };
}

export type Hold = "pointer" | "focus" | "modal";

/**
 * The note's fade on `clock`: `start` (a note shows) fades it in 6 s unless something holds it, the pointer resting on it, the
 * keyboard's focus in it or a modal dialog holding it back; once none does, it fades 6 s later. `stop`: the note was closed, and nothing holds it any more.
 */
export function noteFade(clock: Clock, hide: () => void) {
  const holds = new Set<Hold>();
  let timer: number | undefined;
  let shown = false;
  const later = () => {
    if (timer !== undefined) clock.clear(timer);
    timer = shown && !holds.size ? clock.set(() => {
          shown = false;
          hide();
        }, FADE_MS) : undefined;
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

/** Shows `note` in place of the one showing. */
export function showNote(note: Note) {
  noted = note;
  const toast = element("note-toast");
  const action = () => {
    const button = pressable(text("button", "", "btn quiet toast-main"), null);
    button.addEventListener("click", () => {
      if (noted) void select(noted.session.id);
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

/** The toast's hover, focus and a modal dialog hold its fade; once, at start. */
export function watchNotes() {
  const toast = element("note-toast");
  document.addEventListener("toggle", () => {
    modalHolds();
    copyHolds();
  }, true);
  toast.addEventListener("pointerenter", () => fade.hold("pointer"));
  toast.addEventListener("pointerleave", () => fade.release("pointer"));
  toast.addEventListener("focusin", () => fade.hold("focus"));
  toast.addEventListener("focusout", () => fade.release("focus"));
  const copy = element("copy-toast");
  copy.addEventListener("pointerenter", () => copyFade.hold("pointer"));
  copy.addEventListener("pointerleave", () => copyFade.release("pointer"));
  copy.addEventListener("focusin", () => copyFade.hold("focus"));
  copy.addEventListener("focusout", () => copyFade.release("focus"));
}

// The copy toast (#493): a prompt Copy for the agent put on the clipboard, under the note toast. A copied prompt shows "Prompt
// copied" and its first 3 lines and fades as a note does, 6 s, held by a hover or the keyboard's focus in it; a prompt the clipboard
// refused shows whole in a box with a Copy button, Overturn's manual way, and stays until its ×. A newer copy replaces it.
const COPY_LINES = 3;
let copyBody: HTMLElement | null = null;
let copyButton: HTMLButtonElement | null = null;
let copied = "";
const hideCopy = () => {
  element("copy-toast").hidden = true;
  stackToasts();
};
const copyFade = noteFade(realClock, hideCopy);
const closeCopy = () => {
  copyFade.stop();
  hideCopy();
};
export type Copied = { outcome: "copied" | "manual"; prompt: string };
export function showCopyToast(outcome: Copied["outcome"], prompt: string) {
  const toast = element("copy-toast");
  copied = prompt;
  if (!copyBody || !copyButton) {
    const body = text("div", "", "copy-body");
    const button = pressable(text("button", "Copy", "btn small"), null);
    button.title = "Copy the prompt to paste into your agent";
    // The clipboard refused already, so this is the browser's older route, which a click may use.
    button.addEventListener("click", () => {
      body.querySelector("textarea")?.select();
      if (document.execCommand("copy")) showCopyToast("copied", copied);
    });
    toast.replaceChildren(body, button);
    toastButtons(toast, () => button, closeCopy, "Hide this toast");
    copyBody = body;
    copyButton = button;
  }
  const line = outcome === "copied" ? "Prompt copied, paste it into your agent" : "Copy this prompt and paste it into your agent";
  const lines = prompt.split("\n").slice(0, COPY_LINES).join("\n");
  // Each is cut at the toast's width; its title holds it whole.
  const shown = outcome === "copied" ? fullTitle(text("p", lines, "copy-lines"), lines) : promptBox(prompt, 6);
  copyBody.replaceChildren(fullTitle(text("strong", line, "name"), line), shown);
  // Copy, once it worked, hides; the keyboard's focus on it moves to the ×.
  const moveFocus = outcome === "copied" && document.activeElement === copyButton;
  copyButton.hidden = outcome === "copied";
  if (moveFocus) toast.querySelector<HTMLElement>("[aria-label=Dismiss]")?.focus();
  toast.dataset.say = toastSays([line]);
  toast.hidden = false;
  stackToasts();
  copyHolds();
  if (outcome === "copied") copyFade.start();
  else copyFade.stop();
}
const copyHolds = () => (document.querySelector("dialog:modal") ? copyFade.hold("modal") : copyFade.release("modal"));
