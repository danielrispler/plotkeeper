// The part sheet of the Map view (spec danielrispler/plotkeeper#306, ticket #310): a click on a plate opens it beside the map, in the
// side area's `sheet` slot. It holds the part's plain description, its tier (changed in place), its laws and its fog, with a quiet
// "I know this" that lifts the mist. `map-model.ts`'s `sheetOf` is what it shows; Esc closes it (the `sheet` layer).
//
// "See the proof" (`.sheet-law-actions` of a law a check backs) and the CI line with its drill-down open the drawer (`map-drawer.ts`).
// `.sheet-extra` at the foot holds the history strip (`map-history.ts`, #313). While a past commit is shown the sheet is read-only:
// no tier change, no proof, no CI section, no fog section.

import { failure, invoke, pressable, text } from "./dom.js";
import { currentMap, onDraw, onPlateClick, reloadMap, registerLayer, syncSide, type MapView } from "./map.js";
import { openDrawer } from "./map-drawer.js";
import { drawStrip } from "./map-history.js";
import { ciDrawer, effectiveTier, type Part, proofDrawer, sheetCi, sheetOf, type Tier } from "./map-model.js";

// The part whose sheet is open, and the project it is in; a sheet never carries over to another project's map.
let open: { project: string; part: string } | null = null;
// The part as the live map last had it, so the sheet of a part a past commit no longer has still draws.
let remembered: Part | null = null;

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
  const there = open && open.project === view.project ? view.doc.parts.find((each) => each.id === open!.part) : undefined;
  if (there && !view.scrub) remembered = there;
  const part = there ?? (view.scrub && open?.project === view.project && remembered?.id === open.part ? remembered : undefined);
  if (!part) {
    open = null;
    slot.replaceChildren();
    return syncSide(view.slots);
  }
  view.slots.canvas.querySelector(`.plate[data-id="${CSS.escape(part.id)}"]`)?.classList.add("open");
  const tier = effectiveTier(part, view.state.user.tiers);
  const sheet = sheetOf(part, view.doc.laws, tier, view.scrub ? undefined : view.state.fog?.[part.id]);

  const closer = text("button", "×", "btn quiet sheet-close");
  closer.title = "Close the sheet";
  closer.addEventListener("click", close);
  const tiers = text("div", "", "sheet-tiers");
  for (const each of sheet.tiers) {
    const button = pressable(text("button", each.label, "btn"), each.pressed, () => setTier(view, part.id, each.tier).catch(failure("map_set_tier")));
    button.title = `Track ${part.name} as ${each.label}`;
    if (view.scrub) (button as HTMLButtonElement).disabled = true;
    tiers.append(button);
  }
  const laws = sheet.laws.map((law) => {
    const mark = text("span", "", `law-mark ${law.backing}`);
    const row = text("li", "", "sheet-law");
    row.dataset.law = law.id;
    row.append(mark, text("span", law.text, "sheet-law-text"), text("span", law.label, "sheet-law-backing"), text("span", "", "sheet-law-actions"));
    row.title = `${law.label}: ${law.text}`;
    if (law.backing === "check" && !view.scrub) {
      const proof = text("button", "See the proof", "btn quiet");
      proof.title = "Show the test that stands behind this law, read from the default branch";
      proof.addEventListener("click", () => {
        invoke("map_proof", { project: view.project, law: law.id })
          .then((proofs) => openDrawer(proofDrawer(law.id, proofs)))
          .catch(failure("map_proof"));
      });
      row.querySelector(".sheet-law-actions")!.append(proof);
    }
    return row;
  });
  const fog = text("div", "", `sheet-fog${!view.scrub && view.state.fog?.[part.id] ? ` ${view.state.fog[part.id]}` : ""}`);
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
  const state = view.scrub ? null : sheetCi(view.state.ci, part.id, sheet.laws.some((law) => law.backing === "check"));
  const ci = text("div", "", `sheet-ci${state?.broken ? " broken" : ""}`);
  if (state) {
    ci.append(text("p", state.line, "sheet-ci-line"));
    if (state.drill && view.state.ci) {
      const found = view.state.ci;
      const drill = text("button", "See the CI log", "btn quiet");
      drill.title = "Show the run's status and the end of its failed log, with a link to the run on GitHub";
      drill.addEventListener("click", () => openDrawer(ciDrawer(found)));
      ci.append(drill);
    }
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
    // Fog and CI are of now: a past commit shows neither.
    ...(view.scrub ? [] : [text("h3", "Fog", "sheet-heading"), fog]),
    ...(state ? [text("h3", "CI on main", "sheet-heading"), ci] : []),
  );
  const extra = text("div", "", "sheet-extra");
  card.append(extra);
  card.querySelector(".sheet-laws")?.append(...laws);
  slot.replaceChildren(card);
  drawStrip(view, part.id, extra);
  syncSide(view.slots);
}

onPlateClick(({ view, part }) => {
  open = { project: view.project, part: part.id };
  drawSheet(view);
});
onDraw(drawSheet);
registerLayer("sheet", { open: () => open !== null && currentMap() !== null, close });
