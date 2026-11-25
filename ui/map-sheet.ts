// The part sheet of the Map view (spec danielrispler/plotkeeper#306, ticket #310): a click on a plate opens it over the map's right, in the
// side area's `sheet` slot, in the glass the left panel shares (#385). It starts with why its plate carries its marks (#495: fog, staleness,
// a broken law; `sheetMarks`), then holds the part's purpose, then the named things it holds, each with its key files (spec #473), its tier (changed in place), its laws, what it talks to (its links) and its fog, with a quiet
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
import { element, failure, invoke, pageStore, pressable, text } from "./dom.js";
import { currentMap, onDraw, onPlateClick, reloadMap, registerLayer, syncSide, type MapView } from "./map.js";
import { openDrawer } from "./map-drawer.js";
import { drawStrip } from "./map-history.js";
import { overturnNote, ratifyActions } from "./map-ratify.js";
import { sheetLinkItem, sheetPending, statusWords } from "./ratify.js";
import { enterRefresher, refresherNode, staleWhy } from "./map-stale.js";
import { fogActions } from "./map-catch-up.js";
import { ciDrawer, effectiveTier, type Part, proofDrawer, type Sheet, sheetCi, sheetMarks, sheetOf, talksTo, type Tier } from "./map-model.js";
import { openBriefOf, openCatchUpOf } from "./map-brief.js";
import { lawAction, lawActionNodes, patchActs } from "./patch-review.js";
import { explained, explainedChoice, explainedLinks, explainLine, explanationsOn, explainToggle, onExplanations, SHEET, SHEET_HEADINGS } from "./explanations.js";

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
const [firstWidth, leastWidth, widthStep] = [360, 300, 16];
const kept = pageStore("map-sheet-width");
const keptWidth = () => Number(kept.get()) || firstWidth;
const showWidth = (px: number) => element("map-side").style.setProperty("--sheet-w", `${px}px`);
// Sets the width, bounded, and keeps it; null goes back to the first width and forgets the kept one.
function resize(px: number | null) {
  const side = element("map-side");
  const most = Math.max(leastWidth, (side.parentElement?.clientWidth ?? 0) / 2);
  const width = px === null ? firstWidth : Math.round(Math.min(most, Math.max(leastWidth, px)));
  showWidth(width);
  // A browser that keeps nothing still resizes.
  if (px === null) kept.remove();
  else kept.set(String(width));
}
// The handle on the sheet's left edge, made once and kept in the sheet's slot while a sheet is open: a drag, the arrow keys while it has the focus, and a
// double-click back to the first width.
let resizeHandle: HTMLButtonElement | null = null;
function handle() {
  if (resizeHandle) return resizeHandle;
  const bar = pressable(text("button", "", "btn quiet sheet-resize"), null);
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
  resizeHandle = bar;
  return bar;
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
  const asked = open && open.project === view.project ? open.part : null;
  const there = asked === null ? undefined : view.doc.parts.find((each) => each.id === asked);
  if (there && !view.scrub) remembered = there;
  const part = there ?? (view.scrub && open?.project === view.project && remembered?.id === open.part ? remembered : undefined);
  if (!part) {
    open = null;
    slot.replaceChildren();
    return syncSide(view.slots);
  }
  view.slots.canvas.querySelector(`.plate[data-id="${CSS.escape(part.id)}"]`)?.classList.add("open");
  // The `?` keeps the focus across a draw (a click on it draws the sheet again with or without the explanations): the new one takes it.
  const focused = document.activeElement;
  const keepFocus = focused instanceof HTMLElement && focused.classList.contains("explain-toggle") && slot.contains(focused);
  const { card, extra, toggle } = sheetCard(view, part);
  showWidth(keptWidth());
  // The handle stays in the slot across draws: moving it out, even to put it back, drops its pointer capture and ends a drag, and the
  // server's `map-landed` draws the sheet again at every stage of a write (#450).
  const bar = handle();
  if (bar.parentElement === slot) [...slot.children].forEach((child) => child !== bar && child.remove());
  else slot.replaceChildren(bar);
  slot.append(card);
  if (keepFocus) toggle.focus();
  drawStrip(view, part.id, extra);
  syncSide(view.slots);
}

// The sheet's card for `part` on `view`, its `.sheet-extra` foot (the history strip's, drawn once the card is in the page) and its `?`.
// biome-ignore lint/complexity/noExcessiveLinesPerFunction: ratcheted at #554; split it rather than grow it
export function sheetCard(view: MapView, part: Part): { card: HTMLElement; extra: HTMLElement; toggle: HTMLElement } {
  const tier = effectiveTier(part, view.state.user.tiers);
  const sheet = sheetOf(part, view.doc.laws, tier, view.scrub ? undefined : view.state.fog?.[part.id]);

  const closer = pressable(text("button", "×", "btn quiet sheet-close"), null);
  closer.setAttribute("aria-label", "Close");
  closer.title = "Close the sheet (Esc)";
  closer.addEventListener("click", closeSheet);
  // Explanations (#522): a line under each heading, the selected tier's meaning under the tiers, a link kind's and "not yet ratified"'s
  // at their first link, and the fog actions' under them. The `?` beside the × shows or hides them all.
  const explain = explanationsOn();
  const heading = (words: string) => explained(text("h3", words, "sheet-heading"), SHEET_HEADINGS[words] ?? "", explain);
  const header = text("div", "", "sheet-header");
  const toggle = explainToggle(explain);
  header.append(toggle, closer);
  const tiers = text("div", "", "sheet-tiers");
  let selected: HTMLElement | undefined;
  for (const each of sheet.tiers) {
    const button = pressable(text("button", each.label, "btn"), each.pressed, () => setTier(view, part.id, each.tier).catch(failure("map_set_tier")));
    // Each tier's meaning on hover, so the ones not picked compare without a change.
    button.title = SHEET.tiers[each.tier];
    if (view.scrub) button.disabled = true;
    if (each.pressed) selected = button;
    tiers.append(button);
  }
  // The laws and links at this part that landed without Daniel's say: marked "not yet ratified", with accept and overturn. Of now only.
  const unratified = view.scrub ? sheetPending([], part.id, []) : sheetPending(view.state.ratify ?? [], part.id, view.doc.links);
  const ratified = () => void reloadMap().catch(failure("map_state"));
  const laws = sheet.laws.map((law) => {
    const mark = text("span", "", `law-mark ${law.backing}`);
    const row = text("li", "", "sheet-law");
    row.dataset.law = law.id;
    const acts = text("span", "", "sheet-law-actions");
    row.append(mark, text("span", law.text, "sheet-law-text"), text("span", law.label, "sheet-law-backing"), acts);
    row.title = `${law.label}: ${law.text}`;
    if (law.backing === "check" && !view.scrub) {
      const proof = pressable(text("button", "See the proof", "btn quiet"), null);
      proof.title = "Show the test that stands behind this law, read from the default branch";
      proof.addEventListener("click", () => {
        invoke("map_proof", { project: view.project, law: law.id })
          .then((proofs) => openDrawer(proofDrawer(law.id, proofs)))
          .catch(failure("map_proof"));
      });
      acts.append(proof);
    }
    // A law on trust offers Back a law, or shows where its run's patch stands (#398). Of now only.
    if (law.backing === "trust" && !view.scrub) acts.append(...lawActionNodes(lawAction(law.id, view.state.law_patches), patchActs));
    const pr = unratified.laws.get(law.id);
    if (pr !== undefined) {
      row.classList.add("unratified");
      row.append(text("span", statusWords["not-yet"], "ratify-mark"), ratifyActions(view.project, { pr, kind: "law", id: law.id, text: law.text, change: "" }, ratified));
    }
    return row;
  });
  const nameOf = (id: string) => view.doc.parts.find((p) => p.id === id)?.name ?? view.doc.actors.find((a) => a.id === id)?.name ?? id;
  // "Talks to" (spec #473): every link at either end, its arrow, other end, kind and reason; an unratified one carries its accept and overturn.
  const pending = new Map(unratified.links.map((link) => [link.id, sheetLinkItem(link, link.pr)]));
  const talks = talksTo(part.id, view.doc.links, nameOf);
  const rows = talks.map((talk) => {
    const row = text("li", "", "sheet-link");
    row.dataset.link = talk.link;
    const arrow = text("span", talk.out ? "→" : "←", "sheet-link-dir");
    arrow.setAttribute("role", "img");
    arrow.setAttribute("aria-label", talk.out ? "to" : "from");
    row.append(arrow, text("span", talk.other, "sheet-link-text"));
    // The kind is a code token: never machine-translated.
    if (talk.kind) row.append(Object.assign(text("span", talk.kind, "sheet-link-kind"), { translate: false }));
    if (talk.reason) row.append(text("span", talk.reason, "sheet-link-reason"));
    const item = pending.get(talk.link);
    if (item) {
      row.classList.add("unratified");
      row.append(text("span", statusWords["not-yet"], "ratify-mark"), ratifyActions(view.project, item, ratified));
    }
    return row;
  });
  const links = explainedLinks(rows, talks.map((talk) => ({ kind: talk.kind, unratified: pending.has(talk.link) })), explain);
  const fog = text("div", "", `sheet-fog${!view.scrub && view.state.fog?.[part.id] ? ` ${view.state.fog[part.id]}` : ""}`);
  fog.append(text("p", sheet.fogLine, "sheet-fog-line"));
  const actions = fogActions(view.scrub ? undefined : view.state.fog?.[part.id], view.state.catch_ups?.[part.id]);
  const fogButtons: HTMLElement[] = [];
  if (actions.main) {
    const { open: opens, label, title, note } = actions.main;
    const main = pressable(text("button", label, "btn primary sheet-catch-up"), null);
    main.title = title;
    main.addEventListener("click", () => {
      if (opens.kind === "chapter") return void openBriefOf(view.project, opens.pr, opens.chapter, true);
      void openCatchUpOf(view.project, part.id);
    });
    fog.append(main);
    fogButtons.push(main);
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
    const clear = pressable(text("button", actions.second, "btn quiet sheet-know"), null);
    clear.title = "Clear this part's fog: you have caught up with what changed";
    clear.addEventListener("click", () => {
      clear.disabled = true;
      know(view, part.id).catch(failure("map_know"));
    });
    fog.append(clear);
    fogButtons.push(clear);
    // The fog actions' line under the buttons shown, each button described by it.
    if (explain) {
      const kind = actions.main ? (actions.main.open.kind === "chapter" ? "chapter" : "catchUp") : "know";
      const note = explainLine(SHEET.fogActions[kind], "explain-choice");
      for (const button of fogButtons) button.setAttribute("aria-describedby", note.id);
      fog.append(note);
    }
  }
  // A part with no fog offers the refresher (#392).
  const refresher = !view.scrub && refreshing === part.id ? refresherNode(view, part.id) : null;
  if (!view.scrub && !sheet.canClear && !refresher) {
    const enter = pressable(text("button", "Refresher", "btn quiet sheet-refresher"), null);
    enter.title = "The flows through this part, hop by hop";
    // It shows at once, drawn from the map; the server records the activity behind it, and the reload drops the band.
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
      const drill = pressable(text("button", "See the CI log", "btn quiet"), null);
      drill.title = "Show the run's status and the end of its failed log, with a link to the run on GitHub";
      drill.addEventListener("click", () => openDrawer(ciDrawer(found)));
      ci.append(drill);
    }
  }
  const lawList = text("ul", "", "sheet-laws");
  const linkList = text("ul", "", "sheet-links");
  // Safari drops a list's semantics under `list-style: none`; the role brings them back.
  for (const list of [lawList, linkList]) list.setAttribute("role", "list");
  lawList.append(...laws);
  linkList.append(...links);
  const card = text("div", "", "sheet");
  card.dataset.part = part.id;
  card.append(
    header,
    text("h2", sheet.name, "sheet-name"),
    text("p", sheet.caption, "sheet-caption"),
    ...(view.scrub ? [] : marksList(view, part.id)),
    // A converted version 1 map may have had an empty description: no empty paragraph for it.
    ...(sheet.purpose ? [text("p", sheet.purpose, "sheet-purpose")] : []),
    ...(sheet.holds.length ? [...heading("Holds"), heldList(sheet.holds)] : []),
    ...heading("Tier"),
    ...explainedChoice(tiers, selected, SHEET.tiers[tier], explain),
    ...heading("Laws"),
    ...(laws.length ? [lawList] : [text("p", "No laws on this part", "empty")]),
    ...heading("Talks to"),
    ...(links.length ? [linkList] : [text("p", "No link at this part", "empty")]),
    // Fog and CI are of now: a past commit shows neither.
    ...(refresher ? [refresher] : []),
    ...(view.scrub ? [] : [...heading("Fog"), fog]),
    ...(state ? [...heading("CI on main"), ci] : []),
  );
  const extra = text("div", "", "sheet-extra");
  card.append(extra);
  const overturned = overturnNote(view.project, (item) =>
    item.kind === "law" ? sheet.laws.some((law) => law.id === item.id) : item.kind === "link" && view.doc.links.some((link) => link.id === item.id && (link.from === part.id || link.to === part.id)),
  );
  if (overturned) card.append(overturned);
  return { card, extra, toggle };
}

// Why the part's plate carries its marks (#495), first on its sheet: its fog, its staleness band and a broken law, each from this case's
// facts; nothing when it carries none.
function marksList(view: MapView, part: string): HTMLElement[] {
  const shown = view.state.fog?.[part];
  const stale = staleWhy(view, part);
  const marks = sheetMarks(part, shown, view.state.user.fog?.parts[part] ?? [], stale?.line, view.state.ci);
  const rows: HTMLElement[] = [];
  if (marks.fog) {
    const row = text("li", "", `sheet-mark sheet-mark-fog ${shown}`);
    // The line names ids (laws, links, flows, a commit): code tokens, never machine-translated.
    row.append(Object.assign(text("span", marks.fog.line), { translate: false }));
    if (marks.fog.more) row.append(text("span", `+${marks.fog.more} more`, "sheet-mark-more"));
    rows.push(row);
  }
  if (marks.stale && stale) rows.push(text("li", marks.stale, `sheet-mark sheet-stale stale-${stale.band}`));
  if (marks.broken) rows.push(text("li", marks.broken, "sheet-mark sheet-mark-broken"));
  if (!rows.length) return [];
  const list = text("ul", "", "sheet-marks");
  // Safari drops a list's semantics under `list-style: none`; the role brings them back.
  list.setAttribute("role", "list");
  list.append(...rows);
  return [list];
}

// The things a part holds, each its name, its sentence and its key files. An item with no key files is a note (#525): its name in italics,
// then "· note", so it never reads as a section heading.
export function heldList(holds: Sheet["holds"]): HTMLElement {
  const list = text("ul", "", "sheet-holds");
  // Safari drops a list's semantics under `list-style: none` and flex; the role brings them back.
  list.setAttribute("role", "list");
  for (const item of holds) {
    const note = !item.files.length;
    const row = text("li", "", note ? "sheet-held sheet-held-note" : "sheet-held");
    const name = text("span", item.name, "sheet-held-name");
    if (note) {
      const head = text("span", "", "sheet-held-head");
      head.append(name, " ", text("span", "· note", "sheet-held-kind"));
      row.append(head);
    } else row.append(name);
    row.append(text("span", item.text, "sheet-held-text"));
    if (!note) {
      const files = text("span", "", "sheet-held-files");
      // A path is a code token: never machine-translated.
      files.append(...item.files.map((file) => Object.assign(text("code", file), { translate: false })));
      row.append(files);
    }
    list.append(row);
  }
  return list;
}

/**
 * Hooks the sheet into the map's plate clicks and draws, the explanations switch and Esc's `sheet` layer. `besideSheet` is the left
 * panel's (`panel.ts`), which imports this module: below 1007 px, or over the widened diff, the sheet takes the panel's place (#385).
 */
export function startMapSheet(besideSheet: (part: string) => void) {
  onPlateClick(({ view, part }) => {
    if (open?.part !== part.id) refreshing = null;
    open = { project: view.project, part: part.id };
    besideSheet(part.id);
    logStop("map-action", view.project);
    drawSheet(view);
  });
  onDraw(drawSheet);
  // The explanations switched on or off (#522): the open sheet draws again with or without them.
  onExplanations(() => {
    const view = currentMap();
    if (view && open) drawSheet(view);
  });
  registerLayer("sheet", { open: () => open !== null && currentMap() !== null, close: closeSheet });
}
