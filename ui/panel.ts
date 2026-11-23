// The left panel (spec #380, ticket #385): while the Map view shows, everything Daniel reads beside the map opens in one panel over the
// map's left side, so he never leaves the map to read. It holds a session's story (the rail itself, moved in), History, a turn's diff (widened
// to at most 780 px) or Settings. It floats in the part sheet's glass (the `glass` class both share), never darkens the map, and slides in
// 12 px with a fade (at once under reduced motion). Esc closes it after the drawer, a scrubbed commit and the sheet (the `panel` layer).
//
// What opens where is `map-model.ts`'s (`withPanel`, `withSheet`, `pressRow`): below 1007 px only one overlay is open, and the widened diff
// closes the sheet. The camera pans the parts in focus clear of it (`revealParts`); no position is ever written.

import { element, text } from "./dom.js";
import { mapShown, onDraw, onLeave, registerLayer, revealParts } from "./map.js";
import { openAgents } from "./map-agents.js";
import { closeSheet, sheetPart } from "./map-sheet.js";
import { panelSession, type PanelContent, withPanel, withSheet } from "./map-model.js";

export type { PanelContent };
// What `startPanel` is given by `main.ts`: the content of each kind, and what closing a story does (it selects no session).
export type PanelDeps = {
  history: (box: HTMLElement) => void;
  diff: (box: HTMLElement, session: string, turn: number) => void;
  deselect: () => void;
};
let deps: PanelDeps;
let shown: PanelContent | null = null;

export const panelShown = () => (mapShown() ? shown : null);

const rail = () => document.querySelector<HTMLElement>(".rail")!;
// The story is the rail itself: it moves into the panel while the panel shows it, and back to its column otherwise.
function placeRail(inPanel: boolean) {
  const node = rail();
  if (inPanel && node.parentElement !== element("panel-body")) element("panel-body").replaceChildren(node);
  if (!inPanel && node.parentElement === element("panel-body")) element("map").before(node);
}

function draw() {
  const panel = element("map-panel");
  const content = mapShown() ? shown : null;
  placeRail(content?.kind === "story");
  panel.hidden = content === null;
  panel.classList.toggle("wide", content?.kind === "diff");
  panel.dataset.kind = content?.kind ?? "";
  document.body.classList.toggle("panel-open", content !== null);
  document.body.dataset.panel = content?.kind ?? "";
  element("settings-button").ariaPressed = String(content?.kind === "settings");
  openAgents(panelSession(content));
  const body = element("panel-body");
  if (content?.kind === "history") {
    const box = text("div", "", "panel-view history");
    body.replaceChildren(text("h2", "History", "panel-title"), box);
    deps.history(box);
  } else if (content?.kind === "diff") {
    const box = text("div", "", "panel-view");
    body.replaceChildren(box);
    deps.diff(box, content.session, content.turn);
  } else if (content?.kind === "settings") {
    // #387 moves the writer section here; until then the panel only gives Settings its place.
    body.replaceChildren(text("h2", "Settings", "panel-title"));
  } else if (content === null) {
    body.replaceChildren();
  }
}

// The parts the panel's content is about, kept clear of it and of the sheet: the sheet's part, and the parts the open session's chips are on.
function reveal() {
  const lit = [...element("map-body").querySelectorAll<HTMLElement>(".plate[data-id]:has(.agent-chip.lit)")].map((plate) => plate.dataset.id!);
  const sheet = sheetPart();
  revealParts([...(sheet === null ? [] : [sheet]), ...lit]);
}

// Opens `content` in the panel, or closes it with null; the sheet closes too when the window is narrow or the content is the widened diff.
export function showPanel(content: PanelContent | null) {
  const before = shown;
  const next = withPanel({ panel: shown, sheet: sheetPart() }, content, window.innerWidth);
  if (next.sheet === null && sheetPart() !== null) closeSheet();
  shown = next.panel;
  draw();
  if (shown) requestAnimationFrame(reveal);
  // Closing a story leaves the bare map: no session is selected.
  if (before?.kind === "story" && shown === null) deps.deselect();
}

export const closePanel = () => showPanel(null);

// A sheet is opening on part `part`: below 1007 px, or over the widened diff, it takes the panel's place.
export function besideSheet(part: string) {
  if (shown && withSheet({ panel: shown, sheet: sheetPart() }, part, window.innerWidth).panel === null) closePanel();
}

// The selection changed: a story panel follows it to another session, and closes when none is selected.
export function followSelection(session: string | null) {
  if (shown?.kind !== "story" || shown.session === session) return;
  if (session === null) {
    shown = null;
    draw();
  } else {
    shown = { kind: "story", session };
    draw();
  }
}

// The list changed under History or the diff: drawn again in place.
export function redrawPanel() {
  if (mapShown() && shown?.kind === "history") deps.history(element("panel-body").querySelector<HTMLElement>(".panel-view")!);
}

export function startPanel(given: PanelDeps) {
  deps = given;
  element("panel-close").addEventListener("click", closePanel);
  // Leaving the Map view puts the story back in its rail; the panel stays closed until it opens again.
  onLeave(() => {
    shown = null;
    draw();
  });
  // A redraw of the map draws its chips again: the open session's stay lit.
  onDraw(() => openAgents(panelSession(panelShown())));
  registerLayer("panel", { open: () => panelShown() !== null, close: closePanel });
}
