// The part sheet of the Map view (spec danielrispler/plotkeeper#306, ticket #310): a click on a plate opens it over the map's right, in the
// side area's `sheet` slot, in the glass the left panel shares (#385). It starts with why its plate carries its marks (#495: fog, staleness,
// a broken law; `sheetMarks`), then the catch-up card (#636), then holds the part's purpose, then the named things it holds, each with its
// key files (spec #473), its tier (changed in place), its laws and what it talks to (its links). `map-model.ts`'s `sheetOf` is what it
// shows; Esc closes it (the `sheet` layer). A fogged part's card says how many changes landed and shows the waiting catch-up's outline; its
// main button is its part catch-up (#391), or the one PR brief chapter that lights it (`fogActions`), and "I know this", which lifts the
// mist, is the quiet second. A part with no fog's card says "Caught up" and holds the Refresher (#392).
//
// "See the proof" (`.sheet-law-actions` of a law a check backs) and the CI line with its drill-down open the drawer (`map-drawer.ts`).
// A law on trust has Back a law there instead, or its run's patch (`patch-review.ts`, #398).
// `.sheet-extra` at the foot holds the history strip (`map-history.ts`, #313). While a past commit is shown the sheet is read-only:
// no tier change, no proof, no CI section, no catch-up card.
//
// The sheet's left edge resizes it (#445), and a click on the empty map closes it and its drawer (`map.ts`'s `pan`).

import { logStop } from "./stop-rules.js";
import { age, cut, element, failure, hint, invoke, pageStore, pressable, text } from "./dom.js";
import { currentMap, lawMark, onDraw, onPlateClick, reloadMap, registerLayer, syncSide, type MapView } from "./map.js";
import { openDrawer } from "./map-drawer.js";
import { drawStrip } from "./map-history.js";
import { overturnNote, ratifyActions } from "./map-ratify.js";
import { sheetLinkItem, sheetPending, statusWords } from "./ratify.js";
import { currentPlacement } from "./map-agents.js";
import { sessionHere } from "./map-live.js";
import { studio } from "./state.js";
import type { CatchUpOutline, Session } from "./types.js";
import { enterRefresher, refresherNode, staleWhy } from "./map-stale.js";
import { changesLine, fogActions } from "./map-catch-up.js";
import { chip, type Fold, fitPurpose, fold, purposeNodes } from "./map-sheet-folds.js";
import { ciDrawer, effectiveTier, type FoldKey, foldOrder, type PanelContent, type Part, proofDrawer, type Sheet, sheetCi, sheetGlances, sheetMarks, sheetOf, talksTo, type Tier } from "./map-model.js";
import { openBriefOf, openCatchUpOf } from "./map-brief.js";
import { lawAction, lawActionNodes, patchActs } from "./patch-review.js";
import { explained, explainedChoice, explainedLinks, explainLine, explanationsOn, explainToggle, onExplanations, SHEET } from "./explanations.js";

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
  fitPurpose(element("map-sheet"));
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
  hint(bar, "Drag to resize the sheet; double-click for its first width");
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

/** The part whose sheet is open on the shown map, null with none. */
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
  fitPurpose(slot);
  if (keepFocus) toggle.focus();
  drawStrip(view, part.id, extra);
  syncSide(view.slots);
}

// The sheet's card for `part` on `view`, its History fold's body (the history strip's, drawn once the card is in the page) and its `?`.
export function sheetCard(view: MapView, part: Part): { card: HTMLElement; extra: HTMLElement; toggle: HTMLElement } {
  const tier = effectiveTier(part, view.state.user.tiers);
  const sheet = sheetOf(part, view.doc.laws, tier, view.scrub ? undefined : view.state.fog?.[part.id]);
  const closer = pressable(text("button", "×", "btn quiet sheet-close"), null);
  closer.setAttribute("aria-label", "Close");
  hint(closer, "Close the sheet (Esc)");
  closer.addEventListener("click", closeSheet);
  // Explanations (#522): a line inside each fold, the selected tier's meaning under the tiers, a link kind's and "not yet ratified"'s
  // at their first link, and the fog's and the fog actions' in the catch-up card. The `?` beside the × shows or hides them all.
  const explain = explanationsOn();
  const header = text("div", "", "sheet-header");
  const toggle = explainToggle(explain);
  header.append(toggle, closer);
  // CI is of now: a past commit shows none.
  const ci = view.scrub ? null : sheetCi(view.state.ci, part.id, sheet.laws.some((law) => law.backing === "check"));
  const { folds, extra, chips } = sheetFolds(view, part, sheet, ci, explain);
  const card = text("div", "", "sheet");
  card.dataset.part = part.id;
  card.append(
    header,
    text("h2", sheet.name, "sheet-name"),
    text("p", sheet.caption, "sheet-caption"),
    ...(view.scrub ? [] : marksList(view, part.id)),
    ...(view.scrub ? [] : [catchUpCard(view, part.id, sheet, explain)]),
    // A converted version 1 map may have had an empty description: no empty paragraph for it.
    ...(sheet.purpose ? purposeNodes(sheet.purpose, purposeOpen === part.id, (whole) => {
      purposeOpen = whole ? part.id : null;
    }) : []),
    chips,
    folds,
  );
  const overturned = overturnNote(view.project, (item) =>
    item.kind === "law" ? sheet.laws.some((law) => law.id === item.id) : item.kind === "link" && view.doc.links.some((link) => link.id === item.id && (link.from === part.id || link.to === part.id)),
  );
  if (overturned) card.append(overturned);
  return { card, extra, toggle };
}

type Unratified = ReturnType<typeof sheetPending>;

// The part whose purpose shows whole (#637), kept across the sheet's draws; another part's sheet starts clamped.
let purposeOpen: string | null = null;

// The folds in their order (`foldOrder`), each with its glance and, with the explanations on, its line; the chips that open them; and the
// History fold's body, which the strip fills.
function sheetFolds(view: MapView, part: Part, sheet: Sheet, ci: ReturnType<typeof sheetCi>, explain: boolean) {
  // The laws and links at this part that landed without Daniel's say: marked "not yet ratified", with accept and overturn. Of now only.
  const unratified = view.scrub ? sheetPending([], part.id, []) : sheetPending(view.state.ratify ?? [], part.id, view.doc.links);
  const talks = talksRows(view, part.id, unratified, explain);
  const glances = sheetGlances(sheet, talks.length, view.state.ci);
  const here = text("div", "", "sheet-here");
  const extra = text("div", "", "sheet-extra");
  const make: Record<FoldKey, () => Fold> = {
    laws: () => fold("laws", glances.folds.laws, explain, lawsBody(view, part.id, sheet, unratified)),
    talks: () => fold("talks", glances.folds.talks, explain, talks.length ? [listOf("sheet-links", talks)] : [text("p", "No link at this part", "empty")]),
    here: () => fold("here", "", explain, [here]),
    holds: () => fold("holds", glances.folds.holds, explain, [heldList(sheet.holds)]),
    tier: () => fold("tier", glances.folds.tier, explain, tierChoice(view, part.id, sheet, explain)),
    ci: () => fold("ci", glances.folds.ci, explain, ci ? [ciBody(view, ci)] : [], { forced: ci?.broken ?? false, bad: ci?.broken ?? false }),
    history: () => fold("history", "", explain, [extra]),
  };
  const drawn = new Map(foldOrder(ci, sheet.holds.length > 0).map((key) => [key, make[key]()]));
  const herefold = drawn.get("here");
  if (herefold) fillHere(herefold.node, part.id, !view.scrub, explain);
  const folds = text("div", "", "sheet-folds");
  folds.append(...[...drawn.values()].map((each) => each.node));
  const chips = text("div", "", "sheet-chips");
  const chipOf = (key: FoldKey, words: string, bad = false) => {
    const target = drawn.get(key);
    if (target) chips.append(chip(words, target, bad));
  };
  chipOf("tier", glances.chips.tier);
  chipOf("laws", glances.chips.laws);
  chipOf("talks", glances.chips.talks);
  if (ci) chipOf("ci", glances.chips.ci, ci.broken);
  return { folds, extra, chips };
}

// A list of `rows` with `className`. Safari drops a list's semantics under `list-style: none`; the role brings them back.
function listOf(className: string, rows: HTMLElement[]): HTMLElement {
  const list = text("ul", "", className);
  list.setAttribute("role", "list");
  list.append(...rows);
  return list;
}

// The tier buttons, the selected one's meaning under them with the explanations on. Of now only: a past commit's are disabled.
function tierChoice(view: MapView, part: string, sheet: Sheet, explain: boolean): HTMLElement[] {
  const tiers = text("div", "", "sheet-tiers");
  let selected: HTMLElement | undefined;
  for (const each of sheet.tiers) {
    const button = pressable(text("button", each.label, "btn"), each.pressed, () => setTier(view, part, each.tier).catch(failure("map_set_tier")));
    // Each tier's meaning on hover, so the ones not picked compare without a change.
    hint(button, SHEET.tiers[each.tier]);
    if (view.scrub) button.disabled = true;
    if (each.pressed) selected = button;
    tiers.append(button);
  }
  return explainedChoice(tiers, selected, SHEET.tiers[sheet.tier], explain);
}

// The part's laws, each its mark, text and backing, "See the proof" for one a check backs and Back a law for one on trust, and accept and
// overturn for one not yet ratified.
function lawsBody(view: MapView, part: string, sheet: Sheet, unratified: Unratified): HTMLElement[] {
  const ratified = () => void reloadMap().catch(failure("map_state"));
  const rows = sheet.laws.map((law) => {
    // The law's mark says its backing and text on the pointer, as the row's title did; keyed apart from the plate's own mark of it.
    const mark = lawMark({ id: law.id, on: part, backing: law.backing, text: law.text }, `sheet-${part}-${law.id}`);
    const row = text("li", "", "sheet-law");
    row.dataset.law = law.id;
    const acts = text("span", "", "sheet-law-actions");
    row.append(mark, text("span", law.text, "sheet-law-text"), text("span", law.label, "sheet-law-backing"), acts);
    if (law.backing === "check" && !view.scrub) {
      const proof = pressable(text("button", "See the proof", "btn quiet"), null);
      hint(proof, "Show the test that stands behind this law, read from the default branch");
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
  return rows.length ? [listOf("sheet-laws", rows)] : [text("p", "No laws on this part", "empty")];
}

// "Talks to" (spec #473): every link at either end, its arrow, other end, kind and reason; an unratified one carries its accept and overturn.
function talksRows(view: MapView, part: string, unratified: Unratified, explain: boolean): HTMLElement[] {
  const ratified = () => void reloadMap().catch(failure("map_state"));
  const nameOf = (id: string) => view.doc.parts.find((p) => p.id === id)?.name ?? view.doc.actors.find((a) => a.id === id)?.name ?? id;
  const pending = new Map(unratified.links.map((link) => [link.id, sheetLinkItem(link, link.pr)]));
  const talks = talksTo(part, view.doc.links, nameOf);
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
  return explainedLinks(rows, talks.map((talk) => ({ kind: talk.kind, unratified: pending.has(talk.link) })), explain);
}

// CI on main's line and, for a run there is something to show of, "See the CI log", which opens the drawer.
function ciBody(view: MapView, state: NonNullable<ReturnType<typeof sheetCi>>): HTMLElement {
  const ci = text("div", "", `sheet-ci${state.broken ? " broken" : ""}`);
  ci.append(text("p", state.line, "sheet-ci-line"));
  const found = view.state.ci;
  if (state.drill && found) {
    const drill = pressable(text("button", "See the CI log", "btn quiet"), null);
    hint(drill, "Show the run's status and the end of its failed log, with a link to the run on GitHub");
    drill.addEventListener("click", () => openDrawer(ciDrawer(found)));
    ci.append(drill);
  }
  return ci;
}

// The catch-up card (spec #633, ticket #636), of now only. A fogged part's says how many changes landed since Daniel last caught up and, once
// the catch-up is stored, its outline: its title and each step's kind and headline, never its gist, which stays behind Explain-back. Under
// them the main button and "I know this", with the write's stage or failure. A part with no fog's says "Caught up" and holds the Refresher,
// which opens inside it.
function catchUpCard(view: MapView, part: string, sheet: Sheet, explain: boolean): HTMLElement {
  const shown = view.state.fog?.[part];
  const card = text("article", "", `catch-up-card ${shown ?? "clear"}`);
  const changes = view.state.user.fog?.parts[part]?.length ?? 0;
  const eyebrow = text("p", shown ? changesLine(Math.max(1, changes)) : "Caught up", "catch-up-eyebrow");
  card.append(...explained(eyebrow, SHEET.fog, explain));
  if (!shown) {
    caughtUp(view, part, sheet, card);
    return card;
  }
  const offer = view.state.catch_ups?.[part];
  const outline = offer?.kind === "catch-up" ? offer.outline : undefined;
  if (outline) card.append(...outlineNodes(outline));
  card.append(...catchUpActions(view, part, fogActions(shown, offer), explain));
  return card;
}

// A part with no fog's card: a part that never fogs, or fogs only on its laws, links or flows, says so; then the Refresher, or the
// refresher once entered.
function caughtUp(view: MapView, part: string, sheet: Sheet, card: HTMLElement) {
  if (sheet.tier !== "core") card.append(text("p", sheet.fogLine, "catch-up-line"));
  const refresher = refreshing === part ? refresherNode(view, part) : null;
  if (refresher) return void card.append(refresher);
  const enter = pressable(text("button", "Refresher", "btn quiet sheet-refresher"), null);
  hint(enter, "The flows through this part, hop by hop");
  // It shows at once in the button's place, drawn from the map; the server records the activity behind it, and the reload, which draws
  // the sheet again with it open, drops the band.
  enter.addEventListener("click", () => {
    refreshing = part;
    const opened = refresherNode(view, part);
    if (opened) card.replaceChild(opened, enter);
    enterRefresher(view, part).catch(failure("map_refresher"));
  });
  card.append(enter);
}

// A stored catch-up's outline: its title, then one row per step, its kind and headline.
function outlineNodes(outline: CatchUpOutline): HTMLElement[] {
  const steps = text("ol", "", "catch-up-steps");
  // Safari drops a list's semantics under `list-style: none`; the role brings them back.
  steps.setAttribute("role", "list");
  for (const step of outline.steps) {
    const row = text("li", "", "catch-up-step");
    row.append(text("span", step.k, "catch-up-kind"), text("span", step.h, "catch-up-headline"));
    steps.append(row);
  }
  return [text("h3", outline.title, "catch-up-title"), steps];
}

// The card's buttons in a row, the write's stage or failure under them, and, with the explanations on, the fog actions' line, which each
// button is described by.
function catchUpActions(view: MapView, part: string, actions: ReturnType<typeof fogActions>, explain: boolean): HTMLElement[] {
  const buttons: HTMLElement[] = [];
  if (actions.main) {
    const { open: opens, label, title } = actions.main;
    const main = pressable(text("button", label, "btn primary sheet-catch-up"), null);
    hint(main, title);
    main.addEventListener("click", () => {
      if (opens.kind === "chapter") return void openBriefOf(view.project, opens.pr, opens.chapter, true);
      void openCatchUpOf(view.project, part);
    });
    buttons.push(main);
    const key = `${view.project}|${part}`;
    if (opens.kind === "catch-up" && !offered.has(key)) {
      offered.add(key);
      logStop("catch-up-offered", view.project);
    }
  }
  if (actions.second) {
    const clear = pressable(text("button", actions.second, "btn quiet sheet-know"), null);
    hint(clear, "Clear this part's fog: you have caught up with what changed");
    clear.addEventListener("click", () => {
      clear.disabled = true;
      know(view, part).catch(failure("map_know"));
    });
    buttons.push(clear);
  }
  const row = text("div", "", "catch-up-actions");
  row.append(...buttons);
  const nodes: HTMLElement[] = [row, ...(actions.main?.note ? [text("p", actions.main.note, "sheet-catch-up-note")] : [])];
  if (explain) {
    const kind = actions.main ? (actions.main.open.kind === "chapter" ? "chapter" : "catchUp") : "know";
    const line = explainLine(SHEET.fogActions[kind], "explain-choice");
    for (const button of buttons) button.setAttribute("aria-describedby", line.id);
    nodes.push(line);
  }
  return nodes;
}

// "This session here" (#561): while a session's story is open in the left panel beside the sheet, its turns of the last 7 days that changed
// the part, newest first, each its number, Headline and age; a row's press goes to the turn in the story (`showTurn`). Of now only. Below
// 1007 px the sheet takes the panel's place, so the section never shows there, and nothing is kept for it.
const hereEmpty = "No turn of this session changed this part in the last 7 days";
const hereOlder = "Turns older than 7 days aren't listed";
const day = 24 * 60 * 60 * 1000;
const hereAge = (from: number, now: number) => (now - from >= day ? `${Math.floor((now - from) / day)}d` : age(from, now));

/**
 * What the sheet asks of the page, given by `main.ts` to `startMapSheet`: the left panel's `besideSheet` and `panelShown` (`panel.ts`, which
 * imports this module; below 1007 px, or over the widened diff, the sheet takes the panel's place, #385), and the story's turn Headline and
 * the press of a "This session here" row (`story.ts`, so the sheet never imports the story and its clocks).
 */
export type SheetDeps = {
  besideSheet: (part: string) => void;
  panelShown: () => PanelContent | null;
  headline: (session: Session, index: number) => string | null;
  show: (index: number) => void;
};
let deps: SheetDeps | null = null;

// The session whose story is open beside the sheet, none for a subagent's view.
function storyBeside() {
  const shown = deps?.panelShown() ?? null;
  const selected = studio.selection;
  if (shown?.kind !== "story" || selected === null || selected.agent !== null || selected.session.id !== shown.session) return null;
  return selected.session;
}

// Fills the "This session here" fold for `part`: hidden with no session's story beside the sheet, else its turns in its body and how
// many beside its title; drawn again only when what it shows changed, so a focused row keeps the focus. The fold's summary is its heading.
function fillHere(fold: HTMLElement, part: string, live: boolean, explain: boolean) {
  const box = fold.querySelector<HTMLElement>(".sheet-here");
  const value = fold.querySelector<HTMLElement>(".sheet-fold-value");
  if (!box || !value) return;
  const given = deps;
  const session = live && given ? storyBeside() : null;
  const now = Date.now();
  const found = session ? sessionHere(session, part, currentPlacement(), now) : null;
  const rows = (session && found ? found.rows : []).map(({ turn, index, number }) => ({
    index,
    number: String(number),
    headline: (session ? given?.headline(session, index) : null) ?? (turn.prompt.trim().split("\n")[0] || "No Headline"),
    age: hereAge(turn.ended_at ?? turn.started_at, now),
  }));
  const drawn = JSON.stringify([session?.id ?? null, rows, found?.older, found?.pending, explain]);
  if (box.dataset.drawn === drawn) return;
  box.dataset.drawn = drawn;
  const focused = box.contains(document.activeElement) ? (document.activeElement as HTMLElement).dataset.turn : undefined;
  fold.hidden = found === null;
  if (found === null) {
    box.replaceChildren();
    return;
  }
  value.textContent = rows.length ? `${rows.length} ${rows.length === 1 ? "turn" : "turns"}` : found.pending ? "" : "None";
  const list = text("ul", "", "sheet-here-turns");
  // Safari drops a list's semantics under `list-style: none`; the role brings them back.
  list.setAttribute("role", "list");
  for (const row of rows) {
    const button = pressable(text("button", "", "sheet-here-turn"), null, () => deps?.show(row.index));
    button.dataset.turn = String(row.index);
    hint(button, "Show this turn in the story");
    button.append(text("span", row.number, "sheet-here-number"), cut(text("span", row.headline, "sheet-here-headline")), text("span", row.age, "sheet-here-age"));
    const item = text("li", "");
    item.append(button);
    list.append(item);
  }
  box.replaceChildren(
    // While `map_place` has not answered for one of the turns' files, an empty list says nothing yet.
    ...(rows.length ? [list] : found.pending ? [] : [text("p", hereEmpty, "empty")]),
    ...(found.older ? [text("p", hereOlder, "sheet-here-older")] : []),
  );
  if (focused !== undefined) box.querySelector<HTMLElement>(`[data-turn="${focused}"]`)?.focus();
}

// The story or its panel changed: the open sheet's "This session here" follows.
export function refreshHere() {
  const view = currentMap();
  const fold = view?.slots.sheet.querySelector<HTMLElement>(".sheet-fold-here");
  const part = fold?.closest<HTMLElement>(".sheet")?.dataset.part;
  if (view && fold && part) fillHere(fold, part, !view.scrub, explanationsOn());
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

/**
 * The things a part holds, each its name, its sentence and its key files. An item with no key files is a note (#525): its name in italics,
 * then "· note", so it never reads as a section heading.
 */
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

/** Hooks the sheet into the map's plate clicks and draws, the explanations switch and Esc's `sheet` layer, with what `given` holds. */
export function startMapSheet(given: SheetDeps) {
  deps = given;
  onPlateClick(({ view, part }) => {
    if (open?.part !== part.id) refreshing = null;
    open = { project: view.project, part: part.id };
    given.besideSheet(part.id);
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
