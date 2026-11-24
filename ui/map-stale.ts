// Staleness bands and the refresher on the Map view (spec danielrispler/plotkeeper#378, ticket #392). A Core part Daniel has not engaged with
// for 3, 6 or 12 weeks carries a mild band along its plate's foot (the shell works the bands out, `server/src/staleness.rs`; the map
// shows only what `MapState.stale` lists). The refresher, on a part's sheet, is the part as the map has it now (`refresher.ts`), drawn at
// once with no writer call; entering it is an activity on the part, which clears its band. A past commit's map shows no band.

import { logStop } from "./stop-rules.js";
import { invoke, text } from "./dom.js";
import { onPlate, reloadMap, type MapView } from "./map.js";
import { exploreRow } from "./micro-world.js";
import { refresherOf, staleWords } from "./refresher.js";

onPlate(({ view, part, plate }) => {
  const band = view.scrub ? undefined : view.state.stale?.[part.id];
  if (!band) return;
  plate.classList.add("stale", `stale-${band}`);
  plate.append(text("div", "", "plate-stale"));
  plate.title = `${plate.title}\n\n${staleWords(band)}`;
});

// The band's line on the sheet, or null when the part is not stale (or a past commit is shown).
export function staleLine(view: MapView, part: string): HTMLElement | null {
  const band = view.scrub ? undefined : view.state.stale?.[part];
  return band ? text("p", staleWords(band), `sheet-stale stale-${band}`) : null;
}

// Enters `part`'s refresher: logged (with "stale-entered" when the part had a band) and kept by the shell as an activity on the part.
export async function enterRefresher(view: MapView, part: string) {
  logStop("refresher-entered", view.project);
  if (view.state.stale?.[part]) logStop("stale-entered", view.project);
  await invoke("map_refresher", { project: view.project, part });
  await reloadMap();
}

// The refresher of `part` as the sheet shows it, from the map's text; null when the map does not have the part. The sheet already shows the
// part's caption, description and laws with their backing, so the refresher adds what the map says around it: the flows through it, hop
// by hop, and what it is linked with.
export function refresherNode(view: MapView, part: string): HTMLElement | null {
  const found = view.state.source ? refresherOf(view.state.source, part) : null;
  if (!found) return null;
  const node = text("section", "", "refresher");
  node.dataset.part = part;
  const flows = text("ul", "", "refresher-flows");
  for (const flow of found.flows) {
    const row = text("li", "", "refresher-flow");
    const hops = text("ol", "", "refresher-hops");
    hops.append(...flow.hops.map((hop) => text("li", hop, "refresher-hop")));
    row.append(text("span", flow.name, "refresher-flow-name"), hops);
    // "Explore this" (#399): a micro-world of the flow; a past commit's map offers none.
    if (!view.scrub) row.append(exploreRow(view.project, view.state.micro_worlds, `flow:${flow.id}`, flow.name, { kind: "flow", id: flow.id }));
    flows.append(row);
  }
  const links = text("ul", "", "refresher-links");
  links.append(...found.links.map((link) => text("li", link, "refresher-link")));
  node.append(
    text("h3", "Flows through it", "sheet-heading"),
    found.flows.length ? flows : text("p", "No flow of the map crosses this part", "empty"),
    text("h3", "Linked with", "sheet-heading"),
    found.links.length ? links : text("p", "Nothing links to this part", "empty"),
  );
  return node;
}
