// The part sheet of the Map view (spec danielrispler/plotkeeper#306, ticket #310): a click on a plate opens it beside the map, in the
// side area's `sheet` slot. It holds the part's plain description, its tier (changed in place), its laws and its fog, with a quiet
// "I know this" that lifts the mist. `map-model.ts`'s `sheetOf` is what it shows; Esc closes it (the `sheet` layer).
//
// Where later tickets plug in: `.sheet-law-actions` on each law row is the place of "See the proof" (#312); `.sheet-extra` at the
// foot is the place of the history strip (#313).

import { failure, invoke, pressable, text } from "./dom.js";
import { currentMap, onDraw, onPlateClick, reloadMap, registerLayer, syncSide, type MapView } from "./map.js";
import { effectiveTier, sheetOf, type Tier } from "./map-model.js";

// The part whose sheet is open, and the project it is in; a sheet never carries over to another project's map.
let open: { project: string; part: string } | null = null;

// How long the lifting mist plays before the map is drawn without it: the stylesheet's `--mist-lift`, none under reduced motion.
const lifted = (node: HTMLElement) =>
  new Promise<void>((done) => {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return done();
    node.addEventListener("animationend", () => done(), { once: true });
    setTimeout(done, 1500);
  });

async function know(view: MapView, part: string) {
  await invoke("map_know", { project: view.project, part });
  const mist = view.slots.canvas.querySelector<HTMLElement>(`.plate[data-id="${CSS.escape(part)}"] .plate-fog`);
  if (mist) {
    mist.classList.add("lifting");
    await lifted(mist);
  }
  await reloadMap();
}

async function setTier(view: MapView, part: string, tier: Tier) {
  await invoke("map_set_tier", { project: view.project, part, tier });
  await reloadMap();
}

function close() {
  const view = currentMap();
  open = null;
  if (view) drawSheet(view);
}

// Fills the sheet slot for the open part (or empties it) and marks its plate.
function drawSheet(view: MapView) {
  const slot = view.slots.sheet;
  view.slots.canvas.querySelectorAll(".plate.open").forEach((plate) => plate.classList.remove("open"));
  const part = open && open.project === view.project ? view.doc.parts.find((each) => each.id === open!.part) : undefined;
  if (!part) {
    open = null;
    slot.replaceChildren();
    return syncSide(view.slots);
  }
  view.slots.canvas.querySelector(`.plate[data-id="${CSS.escape(part.id)}"]`)?.classList.add("open");
  const tier = effectiveTier(part, view.state.user.tiers);
  const sheet = sheetOf(part, view.doc.laws, tier, view.state.fog?.[part.id]);

  const closer = text("button", "×", "btn quiet sheet-close");
  closer.title = "Close the sheet";
  closer.addEventListener("click", close);
  const tiers = text("div", "", "sheet-tiers");
  for (const each of sheet.tiers) {
    const button = pressable(text("button", each.label, "btn"), each.pressed, () => setTier(view, part.id, each.tier).catch(failure("map_set_tier")));
    button.title = `Track ${part.name} as ${each.label}`;
    tiers.append(button);
  }
  const laws = sheet.laws.map((law) => {
    const mark = text("span", "", `law-mark ${law.backing}`);
    const row = text("li", "", "sheet-law");
    row.dataset.law = law.id;
    row.append(mark, text("span", law.text, "sheet-law-text"), text("span", law.label, "sheet-law-backing"), text("span", "", "sheet-law-actions"));
    row.title = `${law.label}: ${law.text}`;
    return row;
  });
  const fog = text("div", "", `sheet-fog${view.state.fog?.[part.id] ? ` ${view.state.fog[part.id]}` : ""}`);
  fog.append(text("p", sheet.fogLine, "sheet-fog-line"));
  if (sheet.canClear) {
    const clear = text("button", "I know this", "btn quiet sheet-know") as HTMLButtonElement;
    clear.title = "Clear this part's fog: you have caught up with what changed";
    clear.addEventListener("click", () => {
      clear.disabled = true;
      know(view, part.id).catch(failure("map_know"));
    });
    fog.append(clear);
  }
  const card = text("div", "", "sheet");
  card.dataset.part = part.id;
  card.append(
    closer,
    text("h2", sheet.name, "sheet-name"),
    text("p", sheet.caption, "sheet-caption"),
    text("p", sheet.description, "sheet-description"),
    text("h3", "Tier", "sheet-heading"),
    tiers,
    text("h3", "Laws", "sheet-heading"),
    ...(laws.length ? [text("ul", "", "sheet-laws")] : [text("p", "No laws on this part", "empty")]),
    text("h3", "Fog", "sheet-heading"),
    fog,
    text("div", "", "sheet-extra"),
  );
  card.querySelector(".sheet-laws")?.append(...laws);
  slot.replaceChildren(card);
  syncSide(view.slots);
}

onPlateClick(({ view, part }) => {
  open = { project: view.project, part: part.id };
  drawSheet(view);
});
onDraw(drawSheet);
registerLayer("sheet", { open: () => open !== null && currentMap() !== null, close });
