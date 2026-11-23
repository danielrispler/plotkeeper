// What the centre column held (danielrispler/Falinks#467, #326), drawn now into the left panel over the map (#385, #386): History, a turn's
// whole diff and a worker's log. The column itself is gone, since the map is the page's base (#386); Home's drawing stays here, unreachable,
// until #388 removes it.

import { diffRequest, drawDiff } from "./diff.js";
import { drawHistory } from "./history.js";
import { drawHome, enterHome, landedClear } from "./home.js";
import { element, failure, invoke, text } from "./dom.js";
import { knownProjects } from "./projects.js";
import { drawSidebar } from "./list.js";
import { followLog, showLog, showMissing } from "./log.js";
import { followPanelLog, panelShown, redrawPanel, showPanel } from "./panel.js";
import { select } from "./selection.js";
import { studio } from "./state.js";
import type { Session } from "./types.js";

const boxes = new Map<string, HTMLElement>();
let shown = "";
let ready = false;

// The projects Home lists: the repositories the listed sessions ran in.
const homeProjects = () => knownProjects();

// The views are ready to draw.
export function startPanes() {
  ready = true;
}

// A view's box, made the first time; the column it went in is gone (#386), and Home, the one view left, is no longer shown.
function box(id: string): HTMLElement {
  let node = boxes.get(id);
  if (!node) {
    node = document.createElement("div");
    node.className = "pane";
    node.hidden = true;
    boxes.set(id, node);
  }
  return node;
}

// Shows view `id` and hides the others.
function show(id: string) {
  shown = id;
  const place = box(id);
  for (const node of boxes.values()) node.hidden = node !== place;
}

// Home (`home.ts`) takes the column while no session is selected; the login shell lives in the
// terminal panel.
const HOME = "home";
export function showHome() {
  const entering = shown !== HOME;
  show(HOME);
  // Home's expanded groups last until it is left.
  enterHome(!entering);
  drawHome(box(HOME), studio.listed, homeProjects(), redrawHome);
}

// Whether Home is the view shown.
export const homeShown = () => shown === HOME;

// A summary was stored, or a turn ended: the Headlines of the rows, Home's and the list's unseen rows (#382), may have changed.
export function refreshHome() {
  drawSidebar();
  if (shown !== HOME) return;
  redrawHome();
}

// Draws Home again when the list changed under it.
export function redrawHome() {
  landedClear.sync();
  if (shown === HOME) drawHome(box(HOME), studio.listed, homeProjects(), redrawHome);
}

// History (`history.ts`), in the left panel: the ended sessions, searchable. Clicking a row selects the session.
export const showHistory = () => showPanel({ kind: "history" });

// Draws History again when the list changed under it.
export const redrawHistory = () => redrawPanel();

// The selection the server sent: a worker's log that shows reads on when the same worker is sent again.
let selected: string | null = null;
export function showSession(session: Session | null, clicked = false, agent: string | null = null) {
  const key = session ? `${session.id} ${agent ?? ""}` : null;
  const again = key === selected && !clicked;
  selected = key;
  const panel = panelShown();
  if (ready && again && session && agent && panel?.kind === "log" && panel.session === session.id && panel.agent === agent) followPanelLog();
}

// A turn's whole diff (danielrispler/plotkeeper#131), in the left panel, widened (#385).
diffRequest.open = (id, turn) => {
  const session = studio.listed.find((s) => s.id === id);
  if (!session?.capabilities.turn_trees || !session.turns[turn]?.files) return;
  showPanel({ kind: "diff", session: id, turn });
};

// The left panel's History, diff and worker's log, drawn in the panel's box.
export const historyIn = (place: HTMLElement) => drawHistory(place, studio.listed);
export function diffIn(place: HTMLElement, id: string, turn: number) {
  const session = studio.listed.find((s) => s.id === id);
  if (session) drawDiff(place, session, turn, () => void select(id));
}
export function logIn(place: HTMLElement, id: string, agent: string, follow: boolean) {
  const session = studio.listed.find((s) => s.id === id) ?? (studio.selection?.session.id === id ? studio.selection.session : undefined);
  const subagent = session?.agents.find((a) => a.id === agent);
  const toParent = () => void select(id);
  if (!subagent) return showMissing(place, toParent);
  return follow ? followLog(place, id, subagent, toParent) : showLog(place, id, subagent, toParent);
}
