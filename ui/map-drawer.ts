// The Map view's read-only drawer (spec danielrispler/plotkeeper#306, ticket #312): it opens in the side area's `drawer` slot, under
// the part sheet, so Daniel never loses his place on the map. It shows a `Drawer` (map-model.ts): blocks of code or text, with a jump out
// of each block and of the drawer, to the editor (`openAt`) or to GitHub. Esc closes it before the sheet and the view (the `drawer` layer).
//
// To open it with other content, build a `Drawer` and call `openDrawer`: "See the proof" and the broken-law drill-down (`map-sheet.ts`)
// do, and the history strip's commit drill-down (#313) opens its files and diff the same way. A redraw of the map keeps it open; a
// different project's map closes it.

import { logStop } from "./stop-rules.js";
import { text } from "./dom.js";
import { currentMap, onDraw, registerLayer, syncSide, type MapView } from "./map.js";
import type { Drawer, Jump } from "./map-model.js";

let open: { project: string; drawer: Drawer } | null = null;

// Opens the drawer on the current map's project with `drawer`, replacing what it held.
export function openDrawer(drawer: Drawer) {
  const view = currentMap();
  if (!view) return;
  open = { project: view.project, drawer };
  logStop("proof", view.project);
  drawDrawer(view);
}

export function closeDrawer() {
  const view = currentMap();
  open = null;
  if (view) drawDrawer(view);
}

export const drawerOpen = () => open !== null && currentMap()?.project === open.project;

// A jump out of the drawer: a link, opened in a new tab.
const jumpTo = (jump: Jump) => window.open(jump.to.url, "_blank", "noopener");

function jumpButton(jump: Jump) {
  const button = text("button", jump.label, "btn quiet drawer-jump");
  button.title = jump.title;
  button.addEventListener("click", () => jumpTo(jump));
  return button;
}

function drawDrawer(view: MapView) {
  const slot = view.slots.drawer;
  if (!open || open.project !== view.project) {
    open = null;
    slot.replaceChildren();
    return syncSide(view.slots);
  }
  const { drawer } = open;
  const closer = text("button", "×", "btn quiet sheet-close");
  closer.title = "Close the drawer";
  closer.setAttribute("aria-label", "Close");
  closer.addEventListener("click", closeDrawer);
  const card = text("div", "", "drawer");
  card.append(closer, text("h2", drawer.title, "drawer-title"));
  if (drawer.note) card.append(text("p", drawer.note, "drawer-note"));
  for (const block of drawer.blocks) {
    const part = text("div", "", "drawer-block");
    if (block.label) part.append(text("p", block.label, "drawer-label"));
    part.append(text("pre", block.code, "drawer-code"));
    if (block.jumps.length) part.append(...block.jumps.map(jumpButton));
    card.append(part);
  }
  if (drawer.jumps.length) card.append(...drawer.jumps.map(jumpButton));
  slot.replaceChildren(card);
  syncSide(view.slots);
}

onDraw(drawDrawer);
registerLayer("drawer", { open: drawerOpen, close: closeDrawer });
