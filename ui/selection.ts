// The selection: only Daniel's click, or a session he starts (`startClaude`), changes the selected
// session, and the subagent of it whose view shows, if any. The core decides; this draws the one
// selection it returns in the list, the rail and the terminal (#567).

import { rewatch } from "./answer.js";
import { showCards } from "./cards.js";
import { invoke } from "./dom.js";
import { showSessions } from "./list.js";
import { showSession } from "./panes.js";
import { studio } from "./state.js";
import { showView } from "./story.js";
import { showToast } from "./toast.js";
import type { Selection, Session, SessionCards } from "./types.js";

// Asks the shell for the selected session's cards and draws them.
export async function refreshCards() {
  showCards(await invoke<SessionCards>("cards"));
}

// Whether row `session`, or its subagent `agent`'s line, shows selected: the one Daniel just pressed
// until the shell replies, else the shell's selection, so a reply that disagrees moves the mark back
// (#570). With no `agent`, any of the session's lines counts, as the strip shows.
export function isSelected(session: string, agent?: string | null): boolean {
  const shown = studio.pending ?? (studio.selection && { id: studio.selection.session.id, agent: studio.selection.agent });
  return shown?.id === session && (agent === undefined || shown.agent === agent);
}

// The press shows at once in the list; then the core selects, and the window draws the selection it
// returns. Opening a subagent's line also fades a failed helper. A later press wins over this one's
// reply.
export async function select(id: string, agent: string | null = null) {
  const pressed = { id, agent };
  studio.pending = pressed;
  showSessions(studio.listed);
  try {
    let selected = await invoke<Selection>("select", { id });
    if (agent !== null) {
      selected = await invoke<Selection>("open", { agentId: agent });
      studio.listed = await invoke<Session[]>("sessions");
    }
    // The cards come first, so the rail draws the new session once, with its cards (#567).
    const cards = await invoke<SessionCards>("cards");
    if (studio.pending === pressed) {
      studio.pending = null;
      document.body.classList.remove("list-open");
      show(selected);
      showCards(cards);
      showSession(selected.session, true);
    }
  } catch (e) {
    if (studio.pending === pressed) {
      studio.pending = null;
      showSessions(studio.listed);
    }
    console.warn(e);
  }
  await catchUp();
}

// Draws the selection and cards that events sent while a press waited; fetched again, since an
// event held back may predate the press's reply.
async function catchUp() {
  if (studio.pending || !studio.missed) return;
  studio.missed = false;
  const [selected, cards] = await Promise.all([invoke<Selection | null>("selected"), invoke<SessionCards>("cards")]);
  if (studio.pending) return; // a newer press draws its own reply
  show(selected);
  showCards(cards);
}

export function show(selected: Selection | null) {
  studio.selection = selected;
  showSessions(studio.listed);
  showToast();
  showView(selected);
  showSession(selected?.session ?? null);
  rewatch();
}
