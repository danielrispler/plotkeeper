// What the centre column held (danielrispler/Falinks#467, #326), drawn now into the left panel over the map (#385, #386): History, a turn's
// whole diff and a worker's log. The column itself is gone, since the map is the page's base (#386), and Home with it (#388).

import { diffRequest, drawDiff } from "./diff.js";
import { drawHistory } from "./history.js";
import { followLog, showLog, showMissing } from "./log.js";
import { followPanelLog, panelShown, showPanel } from "./panel.js";
import { studio } from "./state.js";
import type { Session } from "./types.js";

let ready = false;
// What a diff's or a log's way back does: select the session (`selection.ts`, given by `main.ts`).
let select: (id: string) => unknown = () => {};

// The views are ready to draw.
export function startPanes(selectSession: (id: string) => unknown) {
  select = selectSession;
  ready = true;
}

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
export async function logIn(place: HTMLElement, id: string, agent: string, follow: boolean): Promise<void> {
  const session = studio.listed.find((s) => s.id === id) ?? (studio.selection?.session.id === id ? studio.selection.session : undefined);
  const subagent = session?.agents.find((a) => a.id === agent);
  const toParent = () => void select(id);
  if (!subagent) return showMissing(place, toParent);
  await (follow ? followLog(place, id, subagent, toParent) : showLog(place, id, subagent, toParent));
}
