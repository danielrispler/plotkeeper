// The Map view's read-only drawer (spec danielrispler/plotkeeper#306, ticket #312): it opens in the side area's `drawer` slot, under
// the part sheet, so Daniel never loses his place on the map. It shows a `Drawer` (map-model.ts): blocks of code or text, with a jump out
// of each block and of the drawer, to the editor (`openAt`) or to GitHub. Esc closes it before the sheet and the view (the `drawer` layer).
//
// To open it with other content, build a `Drawer` and call `openDrawer`: "See the proof" and the broken-law drill-down (`map-sheet.ts`)
// do, and the history strip's commit drill-down (#313) opens its files and diff the same way. A redraw of the map keeps it open; a
// different project's map closes it.

import { logStop } from "./stop-rules.js";
import { scrollingPre, text } from "./dom.js";
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

// A jump out of the drawer: a real link, opened in a new tab, so ⌘-click and middle-click work as on any link (#462).
function jumpLink(jump: Jump) {
  const link = text("a", jump.label, "btn quiet drawer-jump");
  link.setAttribute("href", jump.to.url);
  link.setAttribute("target", "_blank");
  link.setAttribute("rel", "noopener");
  link.title = jump.title;
  return link;
}

// Code numbered from line `first`, its line `mark` drawn as a `<mark>` with a gutter arrow, so the mark rests on more than colour (#499).
function numberedPre(code: string, first: number, mark: number | undefined, name: string) {
  const pre = scrollingPre("", "drawer-code drawer-lines", name);
  pre.setAttribute("role", "region");
  const body = document.createElement("code");
  body.translate = false;
  code.split("\n").forEach((words, i) => {
    const n = first + i;
    const line = text(n === mark ? "mark" : "span", "", "drawer-line");
    line.append(text("span", String(n), "drawer-n"), words);
    body.append(line, "\n");
  });
  pre.append(body);
  return pre;
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
    const name = block.label ?? drawer.title;
    part.append(block.first ? numberedPre(block.code, block.first, block.mark, name) : scrollingPre(block.code, "drawer-code", name));
    if (block.jumps.length) part.append(...block.jumps.map(jumpLink));
    card.append(part);
  }
  if (drawer.picks?.length) {
    const list = text("ul", "", "drawer-picks");
    // Safari drops a list's semantics under `list-style: none`; the role brings them back.
    list.setAttribute("role", "list");
    for (const pick of drawer.picks) {
      const button = text("button", pick.label, "btn quiet drawer-pick");
      button.setAttribute("type", "button");
      button.translate = false;
      button.title = pick.title;
      button.addEventListener("click", pick.open);
      const item = text("li", "");
      item.append(button);
      list.append(item);
    }
    card.append(list);
  }
  if (drawer.jumps.length) card.append(...drawer.jumps.map(jumpLink));
  slot.replaceChildren(card);
  syncSide(view.slots);
}

onDraw(drawDrawer);
registerLayer("drawer", { open: drawerOpen, close: closeDrawer });
