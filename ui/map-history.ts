// The part history strip and time travel of the Map view (spec danielrispler/plotkeeper#306, ticket #313). A part's sheet lists the
// first-parent commits on the default branch that touched it, from the map's birth; a click on one shows the map as of that commit,
// read-only: no fog, no broken-law mark, no live agents, and a quiet banner "Viewing <commit> · Back to now". A commit's "Files" opens
// its changed files and diffs in the drawer. Esc returns to now (the `scrubbed` layer), after a drawer and before the sheet.
//
// `scrubTo` is the one place a scrub happens (on the shell side, the `map_at` command): a ticket that records that Daniel looked back
// hooks there.

import { logStop } from "./stop-rules.js";
import { failure, invoke, text } from "./dom.js";
import { currentMap, currentScrub, onDraw, onUnread, registerLayer, setScrub, type MapView } from "./map.js";
import { closeDrawer, openDrawer } from "./map-drawer.js";
import { hideAgents } from "./map-agents.js";
import { openBriefOf } from "./map-brief.js";
import { explained, explanationsOn, SHEET } from "./explanations.js";
import { type Scrub, briefOffer, commitDrawer, scrubBanner, scrubTickets, stripOf } from "./map-model.js";
import type { HistoryEntry } from "./types.js";

const tickets = scrubTickets();

// Shows the map as of `commit` read-only, or now again with null.
export async function scrubTo(commit: string | null) {
  if (commit === null) {
    // Back to now works whenever a scrub is set, also when its commit's map cannot be read and no view is drawn.
    tickets.cancel();
    closeDrawer();
    return setScrub(null);
  }
  const view = currentMap();
  if (!view) return;
  const ticket = tickets.start();
  const at = await invoke("map_at", { project: view.project, commit });
  // The view may have moved on while git read, or Daniel went back to now or clicked another commit.
  if (!tickets.live(ticket) || currentMap()?.project !== view.project) return;
  logStop("scrub", view.project);
  setScrub({ commit: at.commit, short: at.short, label: at.label, source: at.source });
}

// The history of a part, as last read, so a redraw of the sheet asks nothing again; it is read again when the branch moves or the part's fog changes.
const known = new Map<string, HistoryEntry[]>();

function rows(view: MapView, list: HTMLElement, entries: HistoryEntry[]) {
  if (!entries.length) return list.replaceChildren(text("li", "No commits since the map began", "empty"));
  list.replaceChildren(
    ...stripOf(entries, view.scrub?.commit ?? null).map((row) => {
      const item = text("li", "", `hist-row${row.fogged ? " fogged" : ""}${row.current ? " current" : ""}`);
      item.dataset.commit = row.commit;
      const scrub = text("button", row.label, "btn quiet hist-commit");
      scrub.title = row.title;
      scrub.setAttribute("aria-pressed", String(row.current));
      scrub.addEventListener("click", () => scrubTo(row.current ? null : row.commit).catch(failure("map_at")));
      const files = text("button", "Files", "btn quiet hist-files");
      files.title = `Show the files and diff of ${row.short}`;
      files.addEventListener("click", () => {
        invoke("map_commit", { project: view.project, commit: row.commit })
          .then((found) => openDrawer(commitDrawer(found)))
          .catch(failure("map_commit"));
      });
      item.append(text("span", "", "hist-dot"), scrub, text("span", row.short, "hist-sha"));
      // A commit whose PR has a stored brief offers it; opening it returns the map to now and plays it.
      const actions = text("span", "", "hist-actions");
      const offer = briefOffer(row);
      if (offer) {
        const brief = text("button", offer.label, "btn quiet hist-brief");
        brief.title = offer.title;
        brief.addEventListener("click", () => void openBriefOf(view.project, offer.number, 0, true));
        actions.append(brief);
      }
      actions.append(files);
      item.append(actions);
      return item;
    }),
  );
}

// Fills `slot` (the sheet's foot) with the strip of `part`.
export function drawStrip(view: MapView, part: string, slot: HTMLElement) {
  const key = `${view.project}|${part}|${view.state.commit}|${view.state.fog?.[part] ?? ""}`;
  const list = text("ul", "", "hist");
  slot.replaceChildren(...explained(text("h3", "History", "sheet-heading"), SHEET.history, explanationsOn()), list);
  const found = known.get(key);
  if (found) return void (rows(view, list, found), (list.scrollTop = list.scrollHeight));
  list.append(text("li", "Reading the history…", "empty"));
  invoke("map_history", { project: view.project, part })
    .then((entries) => {
      known.set(key, entries);
      if (!list.isConnected) return;
      rows(view, list, entries);
      list.scrollTop = list.scrollHeight;
    })
    .catch(failure("map_history"));
}

// The banner: "Viewing <commit> · Back to now" while `scrub` is set, empty with none. A commit whose map cannot be read keeps it too.
export function showBanner(banner: HTMLElement, scrub: Scrub | null) {
  if (!scrub) {
    banner.removeAttribute("title");
    return banner.replaceChildren();
  }
  const words = scrubBanner(scrub);
  const back = text("button", words.back, "btn quiet banner-back");
  back.title = "Show the map as it is now";
  back.addEventListener("click", () => scrubTo(null).catch(failure("map_at")));
  banner.title = scrub.label;
  banner.replaceChildren(text("span", words.text, "banner-text"), " · ", back);
}

// The banner while a past commit shows, the live agents hidden for as long.
let hidden = false;
onDraw((view) => {
  // Every draw makes new plates, so what is hidden is hidden again; going back to now shows them once.
  if (view.scrub !== null || hidden) hideAgents(view.scrub !== null);
  hidden = view.scrub !== null;
  showBanner(view.slots.banner, view.scrub);
});
onUnread((banner, scrub) => showBanner(banner, scrub));

registerLayer("scrubbed", { open: () => currentScrub() !== null, close: () => void scrubTo(null) });
