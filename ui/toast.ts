// The needs-you toast (danielrispler/Falinks#565, decision 3). While the rail's dock cannot show a block, in focus mode or
// while the rail is an overlay, the toast floats over the map's top-right corner; the shell plays
// the needs-you sound (danielrispler/Falinks#556). It never resizes anything. Several blocks share it,
// with a count, and it stays until Daniel acts on them or dismisses it. The session in the focused
// terminal, the selected one, gets none: its prompt is already there.
//
// The toasts and the keyboard (#457): every toast is a manual popover in the top layer, in the region named "Notifications".
// Its main action and its × are buttons Tab reaches, a pointer press keeps the keyboard's focus where it was, and its visible
// text is announced with `ariaNotify`. A modal dialog holds them back until it closes. A toast closed with the focus in it hands
// the focus to the next toast, else back to where it was before it entered the toasts.

import { element, fullTitle, pressable, text } from "./dom.js";
import type { LayoutMode } from "./layout.js";
import { sessionName } from "./list-model.js";
import { setFocus, studio } from "./state.js";
import { titled } from "./keys.js";
import type { Session } from "./types.js";
import { HOVERS } from "./hovers.js";

export type Toast = { session: Session; count: number };

export const toastShows = (mode: LayoutMode) => mode === "focus";

// The first line of a session's open question, else of the step it waits on, such as a permission's.
export function questionLine(session: Pick<Session, "questions" | "last_step">): string {
  const open = session.questions.find((q) => q.answer === "open");
  return (open?.questions[0]?.question ?? session.last_step?.detail ?? "").split("\n")[0] ?? "";
}

// Whether `session` is a block the toast is about: it needs Daniel and is not the `focused` one.
const toasted = (session: Session, focused: string | null) => session.state === "needs_you" && session.id !== focused;

// The toast for the sessions that need Daniel, less the `focused` one and those he dismissed.
export function toastFor(listed: Session[], focused: string | null, dismissed: Set<string>): Toast | null {
  const blocked = listed.filter((s) => toasted(s, focused) && !dismissed.has(s.id));
  return blocked[0] ? { session: blocked[0], count: blocked.length } : null;
}

const dismissed = new Set<string>();
let shown: Toast | null = null;

// The toasts, in the region's order: the needs-you, the note, the copy (#493) and the what's-new (#527) toasts down its top-right
// corner, the Clear toast at its bottom-left.
const TOASTS = ["toast", "note-toast", "copy-toast", "whats-new-toast", "clear-toast"] as const;
const said = new Map<string, string>();
let returnTo: HTMLElement | null = null;

// What a toast announces: its visible text, never its buttons' names, and that it has an action.
export const toastSays = (parts: string[]) => `${parts.filter(Boolean).join(". ")}. Actions available.`;

// A toast's buttons, made once so a redraw keeps the focus on them: its action, which `main` makes or finds in the toast, and its ×,
// which runs `close`.
const made = new Map<HTMLElement, { main: HTMLElement; close: HTMLElement }>();
export function toastButtons(toast: HTMLElement, main: () => HTMLElement, close: () => void, hover: string) {
  const found = made.get(toast);
  if (found) return found;
  const action = main();
  if (!toast.contains(action)) toast.replaceChildren(action);
  const button = pressable(text("button", "×", "btn quiet small"), null);
  button.setAttribute("aria-label", "Dismiss");
  button.title = hover;
  button.addEventListener("click", close);
  toast.append(button);
  const buttons = { main: action, close: button };
  made.set(toast, buttons);
  return buttons;
}

// Opens each toast that has something to say (`hidden` false) as a manual popover and closes the rest; none opens while a modal
// dialog is open. A toast newly open, or saying something new, is announced. Then the open ones are placed, and a toast that closed
// with the focus in it passes the focus on.
export function stackToasts() {
  const modal = document.querySelector("dialog:modal") !== null;
  const focused = document.activeElement;
  const toasts = TOASTS.map((id) => element(id));
  let lost = -1;
  toasts.forEach((toast, i) => {
    const id = TOASTS[i] ?? toast.id;
    const show = !toast.hidden && !modal;
    const open = toast.matches(":popover-open");
    if (show && !open) toast.showPopover();
    if (!show && open) {
      if (toast.contains(focused)) lost = i;
      toast.hidePopover();
    }
    const say = show ? (toast.dataset.say ?? "") : "";
    if (say && said.get(id) !== say) (document as Document & { ariaNotify?: (text: string) => void }).ariaNotify?.(say);
    said.set(id, say);
  });
  placeToasts(toasts);
  if (lost < 0) return;
  const open = toasts.filter((toast) => toast.matches(":popover-open"));
  const next = open.find((toast) => toasts.indexOf(toast) > lost) ?? open.at(-1);
  const target = next?.querySelector("button") ?? (returnTo?.isConnected ? returnTo : null);
  target?.focus();
}

// Top-layer popovers leave the region's layout, so each open toast is placed in the region's box by hand: the first four down its
// top-right corner, one under the other, and the Clear toast in its bottom-left corner.
// With none open, nothing is measured, so a resize or an update with no toast forces no layout; every size is read before any is set.
function placeToasts(toasts: HTMLElement[]) {
  const [needs, note, copy, whatsNew, clear] = toasts.map((toast) => (toast.matches(":popover-open") ? { toast, w: toast.offsetWidth, h: toast.offsetHeight } : null));
  if (!needs && !note && !copy && !whatsNew && !clear) return;
  const box = element("toasts").getBoundingClientRect();
  let top = 0;
  let above = 0;
  for (const open of [needs, note, copy, whatsNew]) {
    if (!open) continue;
    open.toast.style.top = `calc(${box.top + top}px + ${above} * var(--sp-3))`;
    open.toast.style.left = `${box.right - open.w}px`;
    top += open.h;
    above++;
  }
  if (!clear) return;
  clear.toast.style.top = `${box.bottom - clear.h}px`;
  clear.toast.style.left = `${box.left}px`;
}

// The toasts' press, focus and the modal dialogs; once, at start.
// What a press on the needs-you toast does: select its session (`selection.ts`, given by `main.ts`).
let select: (id: string) => unknown = () => {};

export function watchToasts(selectSession: (id: string) => unknown) {
  select = selectSession;
  const region = element("toasts");
  // A press on a toast acts and leaves the keyboard's focus where it was.
  for (const id of TOASTS) element(id).addEventListener("pointerdown", (event) => event.preventDefault());
  // Where the focus came from into the toasts, for a toast that closes with it.
  region.addEventListener("focusin", (event) => {
    const from = event.relatedTarget as HTMLElement | null;
    if (!from || !region.contains(from)) returnTo = from;
  });
  // A modal dialog opening or closing holds the toasts back or lets them out.
  document.addEventListener("toggle", (event) => event.target instanceof HTMLDialogElement && stackToasts(), true);
}

// Draws the toast from the list, the selection and the layout.
export function showToast() {
  const mode = (document.body.dataset.layout ?? "full") as LayoutMode;
  const focused = studio.selection?.session.id ?? null;
  const blocked = new Set(studio.listed.filter((s) => s.state === "needs_you").map((s) => s.id));
  // A block Daniel acted on is forgotten, so the session's next one toasts again.
  for (const id of dismissed) if (!blocked.has(id)) dismissed.delete(id);
  shown = toastShows(mode) ? toastFor(studio.listed, focused, dismissed) : null;
  const toast = element("toast");
  toast.hidden = shown === null;
  if (shown) {
    const action = () => {
      const button = pressable(text("button", "", "btn quiet toast-main"), null);
      button.addEventListener("click", jumpToNeedsYou);
      return button;
    };
    const { main } = toastButtons(toast, action, dismissToast, "Hide this toast until the session needs you again");
    const more = shown.count > 1 ? `+${shown.count - 1} more` : "";
    const name = sessionName(shown.session);
    const line = questionLine(shown.session);
    main.replaceChildren(fullTitle(text("strong", name, "name"), name), ...(more ? [text("span", more, "count")] : []), fullTitle(text("span", line, "line"), line));
    toast.dataset.say = toastSays([name, more, line]);
    toast.title = titled(studio.files.keymap, HOVERS.goToSession, "studio::NextNeedsYou");
  }
  stackToasts();
}

// The ×: every block shown is dismissed, so the toast comes back only for a new one.
function dismissToast() {
  const focused = studio.selection?.session.id ?? null;
  for (const s of studio.listed) if (toasted(s, focused)) dismissed.add(s.id);
  showToast();
}

// Jumps to the toast's session and leaves focus mode; the next-needs-you key does the same. False:
// no toast shows.
export function jumpToNeedsYou(): boolean {
  if (!shown) return false;
  // Leaving focus mode redraws the toast, which then shows none, so the session is read first.
  const { id } = shown.session;
  setFocus(false);
  window.dispatchEvent(new Event("layout"));
  void select(id);
  return true;
}

// The Clear toast (#244): "Cleared N landed", Undo and ×, for the window `clearing` holds open. Its × hides it; the clear goes on.
let undoClear = () => {};
// Its message, made with its buttons at its first showing.
let clearName: HTMLElement | null = null;
export function showClearToast(count: number, undo: () => void) {
  const toast = element("clear-toast");
  undoClear = undo;
  const message = `Cleared ${count} landed`;
  if (!clearName) {
    const name = text("strong", "", "name");
    const undoButton = pressable(text("button", "Undo", "btn small"), null);
    undoButton.title = "Bring the cleared sessions back";
    undoButton.addEventListener("click", () => undoClear());
    toast.replaceChildren(name, undoButton);
    toastButtons(toast, () => undoButton, hideClearToast, "Hide this toast; the clear goes on");
    clearName = name;
  }
  clearName.textContent = message;
  fullTitle(clearName, message);
  toast.dataset.say = toastSays([message]);
  toast.hidden = false;
  stackToasts();
}

export function hideClearToast() {
  element("clear-toast").hidden = true;
  stackToasts();
}

// The what's-new toast (#527): "Updated to X.Y.Z", a "What's new" link to that version's GitHub Release, and ×. Not in colour; it
// stays until its ×.
export function showWhatsNewToast(version: string, url: string) {
  const toast = element("whats-new-toast");
  const message = `Updated to ${version}`;
  const link = text("a", "What's new", "btn small");
  link.href = url;
  link.target = "_blank";
  link.rel = "noopener noreferrer";
  link.title = "Open this version's release notes on GitHub";
  toast.replaceChildren(text("strong", message, "name"), link);
  toastButtons(toast, () => link, hideWhatsNewToast, "Hide this toast; it does not come back for this version");
  toast.dataset.say = toastSays([message]);
  toast.hidden = false;
  stackToasts();
}

function hideWhatsNewToast() {
  element("whats-new-toast").hidden = true;
  stackToasts();
}
