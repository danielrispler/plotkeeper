// The needs-you toast (#565, decision 3). While the rail's dock cannot show a block, in focus mode or
// while the rail is an overlay, the toast floats over the terminal's top-right corner, with the
// needs-you sound. It never resizes anything and never takes keyboard focus. Several blocks share it,
// with a count, and it stays until Daniel acts on them or dismisses it. The session in the focused
// terminal, the selected one, gets none: its prompt is already there. While the rail is an overlay,
// the top bar's slim header shows the selected session's state and open question.

import { element, text } from "./dom.js";
import type { LayoutMode } from "./layout.js";
import { sessionName } from "./list.js";
import { select } from "./selection.js";
import { studio } from "./state.js";
import { stateLabels, type Session } from "./types.js";

export type Toast = { session: Session; count: number };

export const toastShows = (mode: LayoutMode) => mode === "focus" || mode === "overlay";

// The first line of a session's open question, else of the step it waits on, such as a permission's.
export function questionLine(session: Pick<Session, "questions" | "last_step">): string {
  const open = session.questions.find((q) => q.answer === "open");
  return (open?.questions[0]?.question ?? session.last_step?.detail ?? "").split("\n")[0];
}

// The toast for the sessions that need Daniel, less the `focused` one and those he dismissed.
export function toastFor(listed: Session[], focused: string | null, dismissed: Set<string>): Toast | null {
  const blocked = listed.filter((s) => s.state === "needs_you" && s.id !== focused && !dismissed.has(s.id));
  return blocked.length ? { session: blocked[0], count: blocked.length } : null;
}

// Whether the toast sounds: only for a block not `heard` at the last draw, and only while the window
// has focus. Unfocused, the shell's "Needs you" notification (#434) sounds for it instead.
export function chimes(toast: Toast | null, blocked: Set<string>, heard: Set<string> | null, focused: string | null, dismissed: Set<string>, windowFocused: boolean): boolean {
  if (!toast || heard === null || !windowFocused) return false;
  return [...blocked].some((id) => !heard.has(id) && id !== focused && !dismissed.has(id));
}

const dismissed = new Set<string>();
// The sessions that needed Daniel at the last draw: only a new block sounds. Null before the first
// draw, so the blocks already there at start stay silent.
let heard: Set<string> | null = null;
let shown: Toast | null = null;

// shortcut: a synthesised two-note chime; swap in a sound file if Daniel wants another.
function chime() {
  const audio = new AudioContext();
  const gain = audio.createGain();
  gain.connect(audio.destination);
  gain.gain.setValueAtTime(0.15, audio.currentTime);
  gain.gain.exponentialRampToValueAtTime(0.001, audio.currentTime + 0.5);
  [880, 1320].forEach((pitch, i) => {
    const tone = audio.createOscillator();
    tone.frequency.value = pitch;
    tone.connect(gain);
    tone.start(audio.currentTime + i * 0.12);
    tone.stop(audio.currentTime + 0.5);
  });
  setTimeout(() => audio.close(), 1000);
}

// Draws the toast and the slim header from the list, the selection and the layout.
// `fromList`: the shell sent the list; its first sending seeds `heard`.
export function showToast(fromList = false) {
  const mode = (document.body.dataset.layout ?? "full") as LayoutMode;
  const focused = studio.selection?.session.id ?? null;
  const blocked = new Set(studio.listed.filter((s) => s.state === "needs_you").map((s) => s.id));
  // A block Daniel acted on is forgotten, so the session's next one toasts again.
  for (const id of dismissed) if (!blocked.has(id)) dismissed.delete(id);
  shown = toastShows(mode) ? toastFor(studio.listed, focused, dismissed) : null;
  if (chimes(shown, blocked, heard, focused, dismissed, document.hasFocus())) chime();
  if (heard !== null || fromList) heard = blocked;
  const toast = element("toast");
  toast.hidden = shown === null;
  if (shown) {
    const close = text("button", "×", "btn quiet small");
    close.tabIndex = -1;
    close.setAttribute("aria-label", "Dismiss");
    close.addEventListener("click", (event) => {
      event.stopPropagation();
      for (const id of blocked) if (id !== focused) dismissed.add(id);
      showToast();
    });
    const more = shown.count > 1 ? [text("span", `+${shown.count - 1} more`, "count")] : [];
    toast.replaceChildren(text("strong", sessionName(shown.session), "name"), ...more, close, text("span", questionLine(shown.session), "line"));
  }
  const selected = studio.listed.find((s) => s.id === focused) ?? studio.selection?.session;
  const slim = element("slim");
  slim.hidden = mode !== "overlay" || !selected;
  if (selected) {
    const state = text("span", stateLabels[selected.state], "state");
    state.dataset.state = selected.state;
    slim.replaceChildren(state, ...(selected.state === "needs_you" ? [text("span", questionLine(selected), "line")] : []));
  }
}

// Jumps to the toast's session and leaves focus mode; the next-needs-you key does the same. False:
// no toast shows.
export function jumpToNeedsYou(): boolean {
  if (!shown) return false;
  studio.focus = false;
  window.dispatchEvent(new Event("layout"));
  select(shown.session.id);
  return true;
}
