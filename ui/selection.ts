// The selection: only Daniel's click changes the selected session, and the subagent of it whose view
// shows, if any. The core decides; this draws the selection it returns, in the list, the top bar and
// the rail.

import { showCards } from "./cards.js";
import { invoke } from "./dom.js";
import { showSessions } from "./list.js";
import { showSession } from "./panes.js";
import { studio } from "./state.js";
import { showView } from "./story.js";
import type { Selection, Session, SessionCards } from "./types.js";

// Asks the shell for the selected session's cards and draws them.
export async function refreshCards() {
  showCards(await invoke<SessionCards>("cards"));
}

// Opening a subagent's line also fades a failed helper. The core asks for it, and the window draws
// the selection the core returns.
export async function select(id: string, agent: string | null = null) {
  let selected = await invoke<Selection>("select", { id });
  if (agent !== null) {
    selected = await invoke<Selection>("open", { agentId: agent });
    studio.listed = await invoke<Session[]>("sessions");
  }
  document.body.classList.remove("list-open");
  show(selected);
  showSession(selected.session, true);
  await refreshCards();
}

export function show(selected: Selection | null) {
  studio.selection = selected;
  showSessions(studio.listed);
  showView(selected);
  showSession(selected?.session ?? null);
}
