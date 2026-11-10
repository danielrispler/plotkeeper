// The selection: only Daniel's click, or a session he starts (`startClaude`), changes the selected
// session, and the subagent of it whose view shows, if any. The core decides; this draws the one
// selection it returns in the list, the top bar, the rail and the terminal (#567).

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
  // The cards come first, so the rail draws the new session once, with its cards (#567).
  const cards = await invoke<SessionCards>("cards");
  document.body.classList.remove("list-open");
  show(selected);
  showCards(cards);
  showSession(selected.session, true);
}

export function show(selected: Selection | null) {
  studio.selection = selected;
  showSessions(studio.listed);
  showView(selected);
  showSession(selected?.session ?? null);
}
