// The selection: only Daniel's click, or a session he starts (`startClaude`), changes the selected
// session, and the subagent of it whose view shows, if any. The core decides; this draws the one
// selection it returns in the list, the rail and the terminal (danielrispler/Falinks#567).

import { rewatch } from "./answer.js";
import { showCards } from "./cards.js";
import { element, failure, invoke } from "./dom.js";
import { leaveMap } from "./map.js";
import { showSessions } from "./list.js";
import { homeShown, showSession } from "./panes.js";
import { studio } from "./state.js";
import { showView } from "./story.js";
import { showToast } from "./toast.js";
import type { Selection } from "./types.js";

// Asks the shell for the selected session's cards and draws them.
export async function refreshCards() {
  showCards(await invoke("cards"));
}

// Whether row `session`, or its subagent `agent`'s line, shows selected: the one Daniel just pressed
// until the shell replies, else the shell's selection, so a reply that disagrees moves the mark back
// (danielrispler/Falinks#570). With no `agent`, any of the session's lines counts, as the strip shows.
export function isSelected(session: string, agent?: string | null): boolean {
  const shown = studio.pending ?? (studio.selection && { id: studio.selection.session.id, agent: studio.selection.agent });
  return shown?.id === session && (agent === undefined || shown.agent === agent);
}

// The press shows at once in the list; then the core selects, and the window draws the selection it
// returns. Opening a subagent's line also fades a failed helper. A later press wins over this one's
// reply.
export async function select(id: string, agent: string | null = null) {
  leaveMap(); // a session pressed anywhere leaves the Map view over the rail and the terminal
  const pressed = { id, agent };
  studio.pending = pressed;
  showSessions(studio.listed);
  try {
    let selected = await invoke("select", { id });
    if (agent !== null) {
      selected = await invoke("open", { agentId: agent });
      studio.listed = await invoke("sessions");
    }
    // The cards come first, so the rail draws the new session once, with its cards (danielrispler/Falinks#567).
    const cards = await invoke("cards");
    if (studio.pending === pressed) {
      studio.pending = null;
      document.body.classList.remove("list-open");
      show(selected);
      showCards(cards);
      showSession(selected?.session ?? null, true, selected?.agent ?? null);
    }
  } catch (e) {
    if (studio.pending === pressed) {
      studio.pending = null;
      showSessions(studio.listed);
    }
    failure("select", "error")(e);
  }
  await catchUp();
}

// What the Home key does (#239, like tmux `last-window`): from any view but Home (a session, History, a full diff, a new terminal) it shows
// Home, remembering the session when one is selected; on Home it selects the remembered session if the list still holds it, else does
// nothing.
export function homeToggle(current: string | null, remembered: string | null, listed: { id: string }[], home = current === null): { show: string | null } | { select: string } | null {
  if (!home) return { show: current };
  return remembered !== null && listed.some((s) => s.id === remembered) ? { select: remembered } : null;
}

// The session Daniel came from when he last went Home.
let cameFrom: string | null = null;

// The session left behind when the selection goes from `before` to `after`: the one he left when it ends on none, by any route (the
// Home key, a click on the list's empty space, a Delete), else what was remembered.
export function leftSession(before: string | null, after: string | null, remembered: string | null): string | null {
  return before !== null && after === null ? before : remembered;
}

// The Home key's action.
export async function toggleHome() {
  const step = homeToggle(studio.selection?.session.id ?? null, cameFrom, studio.listed, homeShown());
  if (!step) return;
  // Leaving a selected session for none remembers it (`show`); from a view with none selected, what was remembered stays.
  if ("show" in step) await goHome();
  else await select(step.select);
}

// Selects no session, which shows Home: the core clears the selection and what is viewed.
export async function goHome() {
  leaveMap();
  studio.pending = null;
  try {
    const selected = await invoke("select", { id: null });
    const cards = await invoke("cards");
    if (studio.pending) return; // a press since wins
    document.body.classList.remove("list-open");
    show(selected);
    showCards(cards);
    showSession(null, true);
  } catch (e) {
    failure("select home", "error")(e);
  }
}

// Draws the selection and cards that events sent while a press waited; fetched again, since an
// event held back may predate the press's reply.
async function catchUp() {
  if (studio.pending || !studio.missed) return;
  studio.missed = false;
  const [selected, cards] = await Promise.all([invoke("selected"), invoke("cards")]);
  if (studio.pending) return; // a newer press draws its own reply
  show(selected);
  showCards(cards);
}

export function show(selected: Selection | null) {
  cameFrom = leftSession(studio.selection?.session.id ?? null, selected?.session.id ?? null, cameFrom);
  studio.selection = selected;
  showSessions(studio.listed);
  showToast();
  showView(selected);
  showSession(selected?.session ?? null, false, selected?.agent ?? null);
  rewatch();
}
