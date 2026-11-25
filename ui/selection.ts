// The selection: only Daniel's click, or a session he starts (`startClaude`), changes the selected
// session, and the subagent of it whose view shows, if any. The core decides; this draws the one
// selection it returns in the list, the rail and the terminal (danielrispler/Falinks#567).

import { rewatch } from "./answer.js";
import { showCards } from "./cards.js";
import { failure, invoke } from "./dom.js";
import { openMap, shownProject } from "./map.js";
import { pressRow, selectedView } from "./map-model.js";
import { closePanel, followSelection, panelShown, showPanel } from "./panel.js";
import { showSessions } from "./list.js";
import { showSession } from "./panes.js";
import { projectOfCwd } from "./projects.js";
import { studio } from "./state.js";
import { showView } from "./story.js";
import { showToast } from "./toast.js";
import type { Selection } from "./types.js";

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
  // The map is the page's base (#386): the session's project's map shows, and its story opens in the left panel (#385) at once; a worker's
  // log opens there once the server has opened it.
  const session = studio.listed.find((s) => s.id === id);
  if (session) {
    const page = selectedView(id, projectOfCwd(session.cwd), agent);
    if (page.project !== shownProject()) openMap(page.project);
    if (agent === null) showPanel(page.panel);
  }
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
      if (agent !== null && selected?.agent === agent) showPanel(selectedView(id, projectOfCwd(selected.session.cwd), agent).panel);
    }
  } catch (e) {
    if (studio.pending === pressed) {
      studio.pending = null;
      showSessions(studio.listed);
    }
    failure("select", "error")(e);
  }
  await catchUp().catch(failure("catch up"));
}

// A press on a session's row in the list or its chip on the map: a second press on the session the panel shows closes the panel (#385);
// otherwise it selects.
export function pressSession(id: string, agent: string | null = null) {
  if (agent === null && pressRow(panelShown(), id) === null) return closePanel();
  return select(id, agent);
}

// Selects no session, as closing the left panel's story does (#385): the core clears the selection and what is viewed, and the bare map stays.
export async function deselect() {
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
    failure("select none", "error")(e);
  }
}

// Draws the selection and cards that events sent while a press waited; fetched again, since an
// event held back may predate the press's reply. A read that fails leaves them missed, for the next press's catch-up.
async function catchUp() {
  if (studio.pending || !studio.missed) return;
  const [selected, cards] = await Promise.all([invoke("selected"), invoke("cards")]);
  if (studio.pending) return; // a newer press draws its own reply, and catches up after it
  studio.missed = false;
  show(selected);
  showCards(cards);
}

export function show(selected: Selection | null) {
  studio.selection = selected;
  showSessions(studio.listed);
  showToast();
  showView(selected);
  showSession(selected?.session ?? null, false, selected?.agent ?? null);
  followSelection(selected?.session.id ?? null);
  rewatch();
}
