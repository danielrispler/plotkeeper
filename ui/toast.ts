// The needs-you toast (danielrispler/Falinks#565, decision 3). While the rail's dock cannot show a block, in focus mode or
// while the rail is an overlay, the toast floats over the terminal's top-right corner; the shell plays
// the needs-you sound (danielrispler/Falinks#556). It never resizes anything and never takes keyboard focus. Several blocks share it,
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

const dismissed = new Set<string>();
let shown: Toast | null = null;

// Draws the toast and the slim header from the list, the selection and the layout.
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

// The start-failure toast (danielrispler/Falinks#558, herdr's pattern): `message` names the session, else the program, and
// the code; "Show output" runs `open`, which shows the failed pane. Like the needs-you toast it never
// takes focus or space, and it stays until Daniel acts on it or dismisses it. Each failure is its own
// row, stacked under the earlier ones, so a second never hides the first.
export function showStartToast(message: string, open: () => void) {
  const toast = element("start-toast");
  const row: HTMLElement[] = [];
  const button = (label: string, kind: string, click: () => void) => {
    const node = text("button", label, `btn ${kind}`);
    node.tabIndex = -1;
    node.addEventListener("pointerdown", (event) => event.preventDefault());
    node.addEventListener("click", () => {
      for (const node of row) node.remove();
      toast.hidden = toast.childElementCount === 0;
      click();
    });
    return node;
  };
  const close = button("×", "quiet small", () => {});
  close.setAttribute("aria-label", "Dismiss");
  row.push(text("strong", message, "name"), button("Show output", "small", open), close);
  toast.append(...row);
  toast.hidden = false;
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
