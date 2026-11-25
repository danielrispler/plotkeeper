// The left panel (spec #380, ticket #385): while the Map view shows, everything Daniel reads beside the map opens in one panel over the
// map's left side, so he never leaves the map to read. It holds a session's story (the rail itself, moved in), History, a turn's diff (widened
// to at most 780 px), Settings, which holds the writer section (#387), or a worker's log, which had the centre column until the map became the
// page's base (#386). It floats in the part sheet's glass (the `glass` class both share), never darkens the map, and slides in
// 12 px with a fade (at once under reduced motion). Esc closes it after the drawer, a scrubbed commit and the sheet (the `panel` layer).
//
// What opens where is `map-model.ts`'s (`withPanel`, `withSheet`, `pressRow`): below 1007 px only one overlay is open, and the widened diff
// closes the sheet. The camera pans the parts in focus clear of it (`revealParts`); no position is ever written.

import { element, failure, text } from "./dom.js";
import { mapShown, onDraw, registerLayer, revealParts } from "./map.js";
import { openAgents } from "./map-agents.js";
import { closeSheet, sheetPart } from "./map-sheet.js";
import { panelSession, type PanelContent, withPanel, withSheet } from "./map-model.js";
import { writersIn } from "./writers.js";

export type { PanelContent };
/** What `startPanel` is given by `main.ts`: the content of each kind, and what closing a session's panel does (it selects no session). */
export type PanelDeps = {
  history: (box: HTMLElement) => void;
  diff: (box: HTMLElement, session: string, turn: number) => void;
  // A worker's or helper's log (#386); `follow` reads it on from where it was.
  log: (box: HTMLElement, session: string, agent: string, follow: boolean) => Promise<void>;
  deselect: () => void;
};
let deps: PanelDeps;
let shown: PanelContent | null = null;

export const panelShown = () => (mapShown() ? shown : null);

const rail = () => {
  const node = document.querySelector<HTMLElement>(".rail");
  if (!node) throw new Error(".rail is not on the page");
  return node;
};
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
    // The writer section (#363), moved here from the top bar (#387).
    const box = text("div", "", "panel-view settings");
    body.replaceChildren(text("h2", "Settings", "panel-title"), box);
    writersIn(box);
  } else if (content?.kind === "log") {
    const box = text("div", "", "panel-view log");
    body.replaceChildren(box);
    deps.log(box, content.session, content.agent, false).catch(failure("subagent log"));
  } else if (content === null) {
    body.replaceChildren();
  }
}

/** The selection was sent again while a log shows: the log reads on. */
export function followPanelLog() {
  const place = element("panel-body").querySelector<HTMLElement>(".panel-view.log");
  if (shown?.kind === "log" && place) deps.log(place, shown.session, shown.agent, true).catch(failure("subagent log"));
}

// The parts the panel's content is about, kept clear of it and of the sheet: the sheet's part, and the parts the open session's chips are on.
function reveal() {
  const lit = [...element("map-body").querySelectorAll<HTMLElement>(".plate[data-id]:has(.agent-chip.lit)")].flatMap((plate) => plate.dataset.id ?? []);
  const sheet = sheetPart();
  revealParts([...(sheet === null ? [] : [sheet]), ...lit]);
}

/**
 * Opens `content` in the panel, or closes it with null; the sheet closes too when the window is narrow or the content is the widened diff.
 * Closing a panel that shows a session (`panelSession`: its story, a turn's diff or a worker's log) leaves the bare map with no session
 * selected, unless `keepSelection` (⌘2, which opens the story again).
 */
export function showPanel(content: PanelContent | null, keepSelection = false) {
  const before = shown;
  const next = withPanel({ panel: shown, sheet: sheetPart() }, content, window.innerWidth);
  if (next.sheet === null && sheetPart() !== null) closeSheet();
  shown = next.panel;
  draw();
  if (shown) requestAnimationFrame(reveal);
  if (!keepSelection && panelSession(before) !== null && shown === null) deps.deselect();
}

export const closePanel = () => showPanel(null);

/** History (`history.ts`), in the left panel: the ended sessions, searchable. Clicking a row selects the session. */
export const showHistory = () => showPanel({ kind: "history" });

/** A sheet is opening on part `part`: below 1007 px, or over the widened diff, it takes the panel's place. */
export function besideSheet(part: string) {
  if (shown && withSheet({ panel: shown, sheet: sheetPart() }, part, window.innerWidth).panel === null) closePanel();
}

/** The selection changed: a panel that shows a session follows it to the new session's story, and closes when none is selected. */
export function followSelection(session: string | null) {
  const own = panelSession(shown);
  if (own === null || own === session) return;
  shown = session === null ? null : { kind: "story", session };
  draw();
}

/** The list changed under History or the diff: drawn again in place. */
export function redrawPanel() {
  if (!mapShown() || shown?.kind !== "history") return;
  const place = element("panel-body").querySelector<HTMLElement>(".panel-view");
  if (place) deps.history(place);
}

export function startPanel(given: PanelDeps) {
  deps = given;
  element("panel-close").addEventListener("click", closePanel);
  // A redraw of the map draws its chips again: the open session's stay lit.
  onDraw(() => openAgents(panelSession(panelShown())));
  registerLayer("panel", { open: () => panelShown() !== null, close: closePanel });
}
