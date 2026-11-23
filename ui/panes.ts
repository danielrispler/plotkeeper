// What the centre column held (danielrispler/Falinks#467, #326), drawn now into the left panel over the map (#385, #386): History, a turn's
// whole diff and a worker's log. The column itself is gone, since the map is the page's base (#386), and Home with it (#388).

import { diffRequest, drawDiff } from "./diff.js";
import { drawHistory } from "./history.js";
import { followLog, showLog, showMissing } from "./log.js";
import { followPanelLog, panelShown, redrawPanel, showPanel } from "./panel.js";
import { select } from "./selection.js";
import { studio } from "./state.js";
import type { Session } from "./types.js";

let ready = false;

// The views are ready to draw.
export function startPanes() {
  ready = true;
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
