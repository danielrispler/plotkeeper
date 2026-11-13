// The terminal panel (danielrispler/Falinks#542): a login shell in the folder of the session shown, in a band under the
// terminal column, below its pane or both panes of a split; `ctrl-\`` shows and hides it. The band is
// at most 40 % of the column's height and costs no width (danielrispler/Falinks#565, decision 5). One shell per folder,
// kept in the daemon under a fixed id, so switching folders and back, or restarting the app, finds
// the same shell with its scrollback; a dead daemon's shell revives like any other (`panes.ts`).

import type * as Xterm from "@xterm/xterm";
import { element } from "./dom.js";
import { panelHeight } from "./layout.js";
import { openPane } from "./terminal.js";

const PANEL = "panel-";
export const isPanel = (id: string) => id.startsWith(PANEL);

// The panel's shell for the session shown, by its folder; with none, the configured repository's.
export function panelShell(session: { cwd: string } | null, repo: string): { id: string; cwd: string } {
  const cwd = session?.cwd ?? repo;
  return { id: `${PANEL}${cwd}`, cwd };
}

const shells = new Map<string, { box: HTMLElement; term: Xterm.Terminal }>();
let current: { id: string; cwd: string } | null = null;
let open = false;
// The height its divider last left, remembered; read at first use.
let dragged: number | null = null;

// The panel's terminal `id` when it has a pane, so a revived daemon reattaches it.
export const panelTerm = (id: string) => shells.get(id)?.term;

// Follows the session shown; the folder's shell draws once the panel is open.
export function showPanel(session: { cwd: string } | null, repo: string) {
  current = panelShell(session, repo);
  if (open) draw(false);
}

// Shows or hides the panel; shown, its shell takes the keyboard, hidden, the pane above it does.
export function togglePanel(focusAbove: () => void) {
  open = !open;
  element("panel").hidden = !open;
  placePanel();
  if (open) draw(true);
  else focusAbove();
}

// Opens the panel on its shell `id`, as a start failure's "Show output" asks (danielrispler/Falinks#558).
export function showPanelShell(id: string, focusAbove: () => void) {
  current = { id, cwd: id.slice(PANEL.length) };
  if (open) draw(true);
  else togglePanel(focusAbove);
}

// The panel's height in the terminal column, as the width rule's layout or a drag changes it.
export function placePanel() {
  dragged ??= Number(localStorage.getItem("panel")) || 240;
  element("panel").style.height = `${panelHeight(dragged, element("terminal").clientHeight)}px`;
}

// Shows the current folder's shell, opening it the first time; the others stay, hidden.
function draw(focus: boolean) {
  if (!current) return;
  let shell = shells.get(current.id);
  if (!shell) {
    const before = document.activeElement as HTMLElement | null;
    const box = document.createElement("div");
    box.className = "pane";
    element("panel").append(box);
    shell = { box, term: openPane(box, current.id, current.cwd) };
    shells.set(current.id, shell);
    if (!focus) before?.focus(); // `openPane` focuses; a switch of session keeps the keyboard where it was
  }
  for (const other of shells.values()) other.box.hidden = other !== shell;
  if (focus) shell.term.focus();
}

// Dragging the panel's top edge resizes it, within `panelHeight`, and the height is remembered.
export function dragPanel() {
  const handle = element("panel-divider");
  handle.addEventListener("pointerdown", (event) => {
    event.preventDefault(); // no text selection while dragging
    handle.setPointerCapture(event.pointerId);
    handle.classList.add("dragging");
  });
  handle.addEventListener("pointermove", (event) => {
    if (!handle.hasPointerCapture(event.pointerId)) return;
    const column = element("terminal").getBoundingClientRect();
    dragged = panelHeight(column.bottom - event.clientY, column.height);
    placePanel();
  });
  handle.addEventListener("lostpointercapture", () => {
    handle.classList.remove("dragging");
    localStorage.setItem("panel", String(dragged));
  });
}
