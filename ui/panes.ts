// The centre column (danielrispler/Falinks#467, #326): one view at a time in `#column`. Home shows while no session is selected;
// a selected session's story is in the rail, so the column holds History, a turn's whole diff, or a subagent's log when one is
// asked for, else a line pointing at the rail.

import { diffRequest, drawDiff, keepsDiff } from "./diff.js";
import { drawHistory } from "./history.js";
import { drawHome, enterHome, landedClear } from "./home.js";
import { element, failure, invoke, text } from "./dom.js";
import { knownProjects } from "./projects.js";
import { drawSidebar } from "./list.js";
import { followLog, showLog, showMissing } from "./log.js";
import { mapShown } from "./map.js";
import { redrawPanel, showPanel } from "./panel.js";
import { select } from "./selection.js";
import { studio } from "./state.js";
import type { Session } from "./types.js";

const boxes = new Map<string, HTMLElement>();
let shown = "";
let ready = false;

// The projects Home lists: the repositories the listed sessions ran in.
const homeProjects = () => knownProjects();

// Shows Home.
export function startPanes() {
  ready = true;
  showHome();
}

// A view's box in `#column`, made the first time.
function box(id: string): HTMLElement {
  let node = boxes.get(id);
  if (!node) {
    node = document.createElement("div");
    node.className = "pane";
    node.hidden = true;
    element("column").append(node);
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

// History (`history.ts`), a view like Home in the terminal column: the ended sessions, searchable. Clicking a row selects
// the session, which takes the column. Over the map it opens in the left panel instead (#385).
const HISTORY = "history";
export function showHistory() {
  if (mapShown()) return showPanel({ kind: "history" });
  show(HISTORY);
  drawHistory(box(HISTORY), studio.listed);
}

// Draws History again when the list changed under it.
export function redrawHistory() {
  redrawPanel();
  if (shown === HISTORY) drawHistory(box(HISTORY), studio.listed);
}

// The selected session's view. Daniel's click always shows it; the server's updates of the selection switch the view only when
// the session or the subagent changed, so they never take away a diff Daniel just opened.
let selected: string | null = null;
export async function showSession(session: Session | null, clicked = false, agent: string | null = null) {
  // The diff view stays while the server re-sends its session's selection; Daniel's click, or another session, ends it.
  if (keepsDiff(diffShown, session, clicked)) return;
  diffShown = null;
  const key = session ? `${session.id} ${agent ?? ""}` : null;
  if (key === selected && !clicked) {
    // The server re-sends the selection when a hook event about the subagent changed it: read its log on.
    const subagent = agent && session?.agents.find((a) => a.id === agent);
    if (session && subagent && shown === LOG) void followLog(box(LOG), session.id, subagent, () => void select(session.id));
    return;
  }
  selected = key;
  if (!ready) return;
  const current = () => selected === key;
  if (!session) return showHome();
  if (agent) {
    const subagent = session.agents.find((a) => a.id === agent);
    const toParent = () => void select(session.id);
    show(LOG);
    if (subagent) await showLog(box(LOG), session.id, subagent, toParent);
    else showMissing(box(LOG), toParent);
    return;
  }
  if (!current()) return;
  box(STORY).replaceChildren(text("p", "The session's story is in the rail.", "empty"));
  show(STORY);
}
const STORY = "story";
const DIFF = "turn-diff";

// A turn's whole diff in the column (danielrispler/plotkeeper#131), in a view of its own; over the map, in the left panel, widened (#385).
let diffShown: { session: string } | null = null;
diffRequest.open = (id, turn) => {
  const session = studio.listed.find((s) => s.id === id);
  if (!session?.capabilities.turn_trees || !session.turns[turn]?.files) return;
  if (mapShown()) return showPanel({ kind: "diff", session: id, turn });
  diffShown = { session: id };
  show(DIFF);
  drawDiff(box(DIFF), session, turn, () => void select(id));
};
const LOG = "subagent-log";

// The left panel's History and diff (#385): the same views as the column's, drawn in the panel's box.
export const historyIn = (place: HTMLElement) => drawHistory(place, studio.listed);
export function diffIn(place: HTMLElement, id: string, turn: number) {
  const session = studio.listed.find((s) => s.id === id);
  if (session) drawDiff(place, session, turn, () => void select(id));
}
