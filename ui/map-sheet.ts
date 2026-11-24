// The part sheet of the Map view (spec danielrispler/plotkeeper#306, ticket #310): a click on a plate opens it over the map's right, in the
// side area's `sheet` slot, in the glass the left panel shares (#385). It holds the part's plain description, its tier (changed in place), its laws and its fog, with a quiet
// "I know this" that lifts the mist. `map-model.ts`'s `sheetOf` is what it shows; Esc closes it (the `sheet` layer). A fogged part's main
// button is its part catch-up (#391), or the one PR brief chapter that lights it (`fogActions`); "I know this" is the quiet second.
//
// "See the proof" (`.sheet-law-actions` of a law a check backs) and the CI line with its drill-down open the drawer (`map-drawer.ts`).
// A law on trust has Back a law there instead, or its run's patch (`patch-review.ts`, #398).
// `.sheet-extra` at the foot holds the history strip (`map-history.ts`, #313). While a past commit is shown the sheet is read-only:
// no tier change, no proof, no CI section, no fog section.
//
// The sheet's left edge resizes it (#445), and a click on the empty map closes it and its drawer (`map.ts`'s `pan`).

import { logStop } from "./stop-rules.js";
import { element, failure, invoke, pressable, text } from "./dom.js";
import { currentMap, onDraw, onPlateClick, reloadMap, registerLayer, syncSide, type MapView } from "./map.js";
import { openDrawer } from "./map-drawer.js";
import { drawStrip } from "./map-history.js";
import { overturnNote, ratifyActions } from "./map-ratify.js";
import { sheetPending, statusWords } from "./ratify.js";
import { besideSheet } from "./panel.js";
import { enterRefresher, refresherNode, staleLine } from "./map-stale.js";
import { ciDrawer, effectiveTier, fogActions, type Part, proofDrawer, sheetCi, sheetOf, type Tier } from "./map-model.js";
import { openBriefOf, openCatchUpOf } from "./map-brief.js";
import { lawAction, lawActionNodes, patchActs } from "./patch-review.js";

// The part whose sheet is open, and the project it is in; a sheet never carries over to another project's map.
let open: { project: string; part: string } | null = null;
// The parts whose part catch-up the sheet offered, told once per window.
const offered = new Set<string>();
// The part as the live map last had it, so the sheet of a part a past commit no longer has still draws.
let remembered: Part | null = null;
// The part whose refresher is entered on its open sheet (#392); it shows until the sheet closes or another part's opens.
let refreshing: string | null = null;

// How long the lifting mist plays before the map is drawn without it: the stylesheet's `--mist-lift`, none under reduced motion.
const lifted = (node: HTMLElement) =>
  new Promise<void>((done) => {
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return done();
    node.addEventListener("animationend", () => done(), { once: true });
    setTimeout(done, 1500);
  });

async function know(view: MapView, part: string) {
  await invoke("map_know", { project: view.project, part });
  logStop("know", view.project);
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

// The sheet's width (#445): 360 px at first, never under 300 px or over half the map, kept in this browser; it reaches the stylesheet as
// `--sheet-w` on the side area, whose width is the sheet's.
const widthKey = "map-sheet-width";
const [firstWidth, leastWidth, widthStep] = [360, 300, 16];
const keptWidth = () => {
  try {
    return Number(localStorage.getItem(widthKey)) || firstWidth;
  } catch {
    return firstWidth;
  }
};
const showWidth = (px: number) => element("map-side").style.setProperty("--sheet-w", `${px}px`);
// Sets the width, bounded, and keeps it; null goes back to the first width and forgets the kept one.
function resize(px: number | null) {
  const side = element("map-side");
  const most = Math.max(leastWidth, (side.parentElement?.clientWidth ?? 0) / 2);
  const width = px === null ? firstWidth : Math.round(Math.min(most, Math.max(leastWidth, px)));
  showWidth(width);
  try {
    if (px === null) localStorage.removeItem(widthKey);
    else localStorage.setItem(widthKey, String(width));
  } catch {
    // A browser that keeps nothing still resizes.
  }
}
// The handle on the sheet's left edge, made once and moved into each drawn sheet: a drag, the arrow keys while it has the focus, and a
// double-click back to the first width.
let resizeHandle: HTMLButtonElement | null = null;
function handle() {
  if (resizeHandle) return resizeHandle;
  const bar = text("button", "", "btn quiet sheet-resize") as HTMLButtonElement;
  bar.type = "button";
  bar.setAttribute("aria-label", "Resize the sheet: the left arrow widens it, the right arrow narrows it");
  bar.title = "Drag to resize the sheet; double-click for its first width";
  const width = () => element("map-side").getBoundingClientRect().width;
  let from = { x: 0, width: 0 };
  bar.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    from = { x: event.clientX, width: width() };
    bar.setPointerCapture(event.pointerId);
    bar.classList.add("dragging");
    document.body.classList.add("resizing");
  });
  // The edge is on the left, so a move left widens the sheet.
  bar.addEventListener("pointermove", (event) => {
    if (bar.hasPointerCapture(event.pointerId)) resize(from.width + from.x - event.clientX);
  });
  bar.addEventListener("lostpointercapture", () => {
    bar.classList.remove("dragging");
    document.body.classList.remove("resizing");
  });
  bar.addEventListener("dblclick", () => resize(null));
  bar.addEventListener("keydown", (event) => {
    const step = { ArrowLeft: widthStep, ArrowRight: -widthStep }[event.key];
    if (step === undefined) return;
    event.preventDefault();
    event.stopPropagation();
    resize(width() + step);
  });
  return (resizeHandle = bar);
}

// The part whose sheet is open on the shown map, null with none.
export const sheetPart = () => (open && currentMap()?.project === open.project ? open.part : null);

export function closeSheet() {
  const view = currentMap();
  open = null;
  refreshing = null;
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
  closer.setAttribute("aria-label", "Close");
  closer.title = "Close the sheet (Esc)";
  closer.addEventListener("click", closeSheet);
  const tiers = text("div", "", "sheet-tiers");
  for (const each of sheet.tiers) {
    const button = pressable(text("button", each.label, "btn"), each.pressed, () => setTier(view, part.id, each.tier).catch(failure("map_set_tier")));
    button.title = `Track ${part.name} as ${each.label}`;
    if (view.scrub) (button as HTMLButtonElement).disabled = true;
    tiers.append(button);
  }
  // The laws and links at this part that landed without Daniel's say: marked "not yet ratified", with accept and overturn. Of now only.
  const unratified = view.scrub ? sheetPending([], part.id, []) : sheetPending(view.state.ratify ?? [], part.id, view.doc.links);
  const ratified = () => void reloadMap();
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
    // A law on trust offers Back a law, or shows where its run's patch stands (#398). Of now only.
    if (law.backing === "trust" && !view.scrub) row.querySelector(".sheet-law-actions")!.append(...lawActionNodes(lawAction(law.id, view.state.law_patches), patchActs));
    const pr = unratified.laws.get(law.id);
    if (pr !== undefined) {
      row.classList.add("unratified");
      row.append(text("span", statusWords["not-yet"], "ratify-mark"), ratifyActions(view.project, { pr, kind: "law", id: law.id, text: law.text, change: "" }, ratified));
    }
    return row;
  });
  const nameOf = (id: string) => view.doc.parts.find((p) => p.id === id)?.name ?? view.doc.actors.find((a) => a.id === id)?.name ?? id;
  const links = unratified.links.map((link) => {
    const row = text("li", "", "sheet-link unratified");
    row.dataset.link = link.id;
    const words = `${nameOf(link.from)} to ${nameOf(link.to)}`;
    row.append(text("span", words, "sheet-link-text"), text("span", statusWords["not-yet"], "ratify-mark"), ratifyActions(view.project, { pr: link.pr, kind: "link", id: link.id, text: words, change: "" }, ratified));
    return row;
  });
  const fog = text("div", "", `sheet-fog${!view.scrub && view.state.fog?.[part.id] ? ` ${view.state.fog[part.id]}` : ""}`);
  fog.append(text("p", sheet.fogLine, "sheet-fog-line"));
  const actions = fogActions(view.scrub ? undefined : view.state.fog?.[part.id], view.state.catch_ups?.[part.id]);
  if (actions.main) {
    const { open: opens, label, title, note } = actions.main;
    const main = text("button", label, "btn primary sheet-catch-up") as HTMLButtonElement;
    main.title = title;
    main.addEventListener("click", () => {
      if (opens.kind === "chapter") return void openBriefOf(view.project, opens.pr, opens.chapter);
      void openCatchUpOf(view.project, part.id);
    });
    fog.append(main);
    if (note) fog.append(text("span", note, "sheet-catch-up-note"));
    if (opens.kind === "catch-up") {
      const key = `${view.project}|${part.id}`;
      if (!offered.has(key)) {
        offered.add(key);
        logStop("catch-up-offered", view.project);
      }
    }
  }
  if (sheet.canClear && actions.second) {
    const clear = text("button", actions.second, "btn quiet sheet-know") as HTMLButtonElement;
    clear.title = "Clear this part's fog: you have caught up with what changed";
    clear.addEventListener("click", () => {
      clear.disabled = true;
      know(view, part.id).catch(failure("map_know"));
    });
    fog.append(clear);
  }
  // A Core part Daniel has not engaged with for weeks says so, and a part with no fog offers the refresher (#392).
  const stale = staleLine(view, part.id);
  if (stale) fog.append(stale);
  const refresher = !view.scrub && refreshing === part.id ? refresherNode(view, part.id) : null;
  if (!view.scrub && !sheet.canClear && !refresher) {
    const enter = text("button", "Refresher", "btn quiet sheet-refresher") as HTMLButtonElement;
    enter.title = "The part as the map has it now: its flows, laws and links";
    // It shows at once, drawn from the map; the shell records the activity behind it, and the reload drops the band.
    enter.addEventListener("click", () => {
      refreshing = part.id;
      drawSheet(view);
      enterRefresher(view, part.id).catch(failure("map_refresher"));
    });
    fog.append(enter);
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
    ...(links.length ? [text("h3", "Links", "sheet-heading"), text("ul", "", "sheet-links")] : []),
    ...(refresher ? [refresher] : []),
    ...(view.scrub ? [] : [text("h3", "Fog", "sheet-heading"), fog]),
    ...(state ? [text("h3", "CI on main", "sheet-heading"), ci] : []),
  );
  const extra = text("div", "", "sheet-extra");
  card.append(extra);
  card.querySelector(".sheet-laws")?.append(...laws);
  card.querySelector(".sheet-links")?.append(...links);
  const overturned = overturnNote(view.project, (item) =>
    item.kind === "law" ? sheet.laws.some((law) => law.id === item.id) : item.kind === "link" && view.doc.links.some((link) => link.id === item.id && (link.from === part.id || link.to === part.id)),
  );
  if (overturned) card.append(overturned);
  showWidth(keptWidth());
  slot.replaceChildren(handle(), card);
  drawStrip(view, part.id, extra);
  syncSide(view.slots);
}

onPlateClick(({ view, part }) => {
  if (open?.part !== part.id) refreshing = null;
  open = { project: view.project, part: part.id };
  // Below 1007 px, or over the widened diff, the sheet takes the left panel's place (#385).
  besideSheet(part.id);
  logStop("map-action", view.project);
  drawSheet(view);
});
onDraw(drawSheet);
registerLayer("sheet", { open: () => open !== null && currentMap() !== null, close: closeSheet });
