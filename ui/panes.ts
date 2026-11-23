// The centre column (danielrispler/Falinks#467, #326): one view at a time in `#column`. Home shows while no session is selected;
// a selected session's story is in the rail, so the column holds History, a turn's whole diff, or a subagent's log when one is
// asked for, else a line pointing at the rail.

import { diffRequest, drawDiff, keepsDiff } from "./diff.js";
import { drawHistory } from "./history.js";
import { drawHome, enterHome, landedClear } from "./home.js";
import { element, failure, invoke, listen, newest, text } from "./dom.js";
import { knownProjects } from "./projects.js";
import { followLog, showLog, showMissing } from "./log.js";
import { leaveMap } from "./map.js";
import { select } from "./selection.js";
import { studio } from "./state.js";
import type { Learning, Session } from "./types.js";

const boxes = new Map<string, HTMLElement>();
let asked: Learning | null = null;
let shown = "";
let ready = false;

// The projects Home lists: the repositories the listed sessions ran in.
const homeProjects = () => knownProjects();

// Asks the card store for the learning item (#240), with the clock and the start of Daniel's local day, and draws Home with it; a
// failed read leaves the item as it was. Asked when Home is drawn, when a turn ends and after each tap.
// Only the newest read lands: an older reply, which may predate a tap, is dropped.
const latestLearning = newest();
export const readLearning = () => {
  const now = Date.now();
  const day = new Date(now);
  day.setHours(0, 0, 0, 0);
  return latestLearning(invoke("learning", { now, dayStart: day.getTime() })).then((found) => {
    if (found === undefined) return;
    asked = found;
    redrawHome();
  }, failure("learning"));
};

// Shows Home, and again each time the retests change the item the store chooses.
export function startPanes() {
  ready = true;
  showHome();
  listen<null>("retests", () => void readLearning());
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
  // Home's expanded groups, "Not now" and the answered item last until it is left; what the store chose before is not shown
  // again until it answers, since it may be an item set aside.
  enterHome(!entering);
  if (entering) asked = null;
  drawHome(box(HOME), studio.listed, homeProjects(), asked, redrawHome, () => void readLearning());
  void readLearning();
}

// Forgets the item the store last chose, until it answers again.
export const forgetAsked = () => void (asked = null);

// Whether Home is the view shown.
export const homeShown = () => shown === HOME;

// A card or summary was stored, or a turn ended: what the store chooses for the slot, and the Headlines of the rows, may have changed.
export function refreshHome() {
  if (shown !== HOME) return;
  void readLearning();
  redrawHome();
}

// Draws Home again when the list changed under it.
export function redrawHome() {
  landedClear.sync();
  if (shown === HOME) drawHome(box(HOME), studio.listed, homeProjects(), asked, redrawHome, () => void readLearning());
}

// History (`history.ts`), a view like Home in the terminal column: the ended sessions, searchable. Clicking a row selects
// the session, which takes the column.
const HISTORY = "history";
export function showHistory() {
  leaveMap();
  show(HISTORY);
  drawHistory(box(HISTORY), studio.listed);
}

// Draws History again when the list changed under it.
export function redrawHistory() {
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

// A turn's whole diff in the column (danielrispler/plotkeeper#131), in a view of its own.
let diffShown: { session: string } | null = null;
diffRequest.open = (id, turn) => {
  const session = studio.listed.find((s) => s.id === id);
  if (!session?.capabilities.turn_trees || !session.turns[turn]?.files) return;
  diffShown = { session: id };
  show(DIFF);
  drawDiff(box(DIFF), session, turn, () => void select(id));
};
const LOG = "subagent-log";
