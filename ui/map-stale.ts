// Staleness bands and the refresher on the Map view (spec danielrispler/plotkeeper#378, ticket #392). A Core part Daniel has not engaged with
// for 3, 6 or 12 weeks carries a mild band along its plate's foot (the server works the bands out, `server/src/staleness.rs`; the map
// shows only what `MapState.stale` lists). The refresher, on a part's sheet, is the part as the map has it now (`refresher.ts`), drawn at
// once with no writer call; entering it is an activity on the part, which clears its band. A past commit's map shows no band.

import { logStop } from "./stop-rules.js";
import { hint, hintOf, invoke, text } from "./dom.js";
import { onPlate, reloadMap, type MapView } from "./map.js";
import { exploreRow } from "./micro-world.js";
import { refresherOf, staleWords } from "./refresher.js";

onPlate(({ view, part, plate }) => {
  const band = view.scrub ? undefined : view.state.stale?.[part.id];
  if (!band) return;
  plate.classList.add("stale", `stale-${band}`);
  plate.append(text("div", "", "plate-stale"));
  const open = plate.querySelector<HTMLElement>(".plate-open");
  if (open) hint(open, `${hintOf(open)?.textContent ?? ""}\n\n${staleWords(band)}`);
});

/**
 * The band's line at the top of the sheet (#495): the server's, with the last engagement's date, else the band alone; null when the part
 * is not stale (or a past commit is shown).
 */
export function staleWhy(view: MapView, part: string): { line: string; band: number } | null {
  const band = view.scrub ? undefined : view.state.stale?.[part];
  return band ? { line: view.state.stale_why?.[part] ?? staleWords(band), band } : null;
}

/** Enters `part`'s refresher: logged (with "stale-entered" when the part had a band) and kept by the server as an activity on the part. */
export async function enterRefresher(view: MapView, part: string) {
  logStop("refresher-entered", view.project);
  if (view.state.stale?.[part]) logStop("stale-entered", view.project);
  await invoke("map_refresher", { project: view.project, part });
  await reloadMap();
}

/**
 * The refresher of `part` as the sheet shows it, from the map's text; null when the map does not have the part. The sheet already shows the
 * part's caption, purpose, held items, laws and links ("Talks to", spec #473), so the refresher adds the flows through it, hop by hop.
 */
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
  node.append(
    text("h3", "Flows through it", "sheet-heading"),
    found.flows.length ? flows : text("p", "No flow of the map crosses this part", "empty"),
  );
  return node;
}
