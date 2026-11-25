// The Map view (spec danielrispler/plotkeeper#306, ticket #309): a project's system map as plates on a quiet chart (Treatment C). Since #386
// it is the page's base: the page opens on the last viewed project's map, and the session list's headers switch it. The shell reads the map from
// `origin`'s default branch and keeps Daniel's positions (`server/src/map.rs`); `map-model.ts` holds the pure logic.
//
// Where the next tickets plug in, so they add to this file rather than reshape it:
// - `onPlate(fn)`: runs for each plate each time it is drawn, with the plate's slots: `fog` behind the name, `marks` at its top
//   right (law marks sit there; a broken-law mark joins them), `chips` along its foot (live agents). A ticket fills a slot.
// - `onPlateClick(fn)`: a click on a plate that was not a drag, such as one that opens the part's sheet. A click on the empty map closes
//   the `drawer` and `sheet` layers (#445).
// - `onDraw(fn)`: runs after each draw with the whole `MapView`, for what is not on a plate: the side area (`slots.notPlaced`,
//   `slots.sheet`, `slots.drawer`, `slots.runs`, shown by `syncSide`), the tray (`slots.tray`) and the banner (`slots.banner`).
// - `registerLayer(name, layer)`: Esc's layers, `drawer`, `popover` (#491), `brief`, `scrubbed`, `sheet` and `panel` (#385), peeled in that order; on the bare map Esc does nothing.
// - The left panel (`panel.ts`, #385) floats over the map's left; `revealParts` pans the camera so the parts in focus clear it and the sheet.
// - `MapState` (`types.ts`, `map.rs`) is the one payload; `currentMap()` gives the view as drawn.
// - The camera (#383): the map area pans and zooms the world rather than scrolling it. Daniel's view is saved per project (`map_save_view`);
//   a PR brief borrows the camera with `lendCamera` and `giveCameraBack` shows Daniel's view again (`cameraRig` in `map-model.ts`).

import { logStop, reviewNode, reviewOf } from "./stop-rules.js";
import { element, failure, invoke, listen, newest, pressable, svg, text } from "./dom.js";
import {
  autoPlace, brokenParts, brokenText, cameraRig, type CameraRig, draftPrompt, dragMove, dragStart, effectiveTier, escapeMap, fitCamera, panTo, type Drag,
  type Law, type Layer, type LayerName, type MapDoc, startProject, switchProject, parseMap, type Part, type Point, plateSize, plateTitle, sizeOf, linkLines, markPoints, readingOrder, revealPan, rimPoint, type Scrub, type Tier, zoomAt,
} from "./map-model.js";
import { recheckSeen } from "./seen.js";
import { knownProjects, projectGone } from "./projects.js";
import type { Camera, MapState } from "./types.js";

export type LoadedMap = Extract<MapDoc, { ok: true }>;
// What the view holds once a map loaded: its payload, the map as parsed, and every id's centre in the world.
export type MapView = {
  project: string;
  state: MapState;
  doc: LoadedMap;
  positions: Record<string, Point>;
  // The past commit this view shows read-only (no fog, no live agents, no CI), null when it shows now (ticket #313).
  scrub: Scrub | null;
  slots: { canvas: HTMLElement; side: HTMLElement; notPlaced: HTMLElement; tray: HTMLElement; runs: HTMLElement; sheet: HTMLElement; drawer: HTMLElement; banner: HTMLElement };
};
export type PlateContext = { view: MapView; part: Part; tier: Tier; plate: HTMLElement; fog: HTMLElement; marks: HTMLElement; chips: HTMLElement };

const plateHooks: ((context: PlateContext) => void)[] = [];
const clickHooks: ((context: PlateContext) => void)[] = [];
const drawHooks: ((view: MapView) => void)[] = [];
const layers: Partial<Record<LayerName, Layer>> = {};
export const onPlate = (hook: (context: PlateContext) => void) => void plateHooks.push(hook);
export const onPlateClick = (hook: (context: PlateContext) => void) => void clickHooks.push(hook);
export const onDraw = (hook: (view: MapView) => void) => void drawHooks.push(hook);
// Runs when a draw has no map to show (none yet, or a past commit's cannot be read), with the banner and the scrub still set.
const unreadHooks: ((banner: HTMLElement, scrub: Scrub | null) => void)[] = [];
export const onUnread = (hook: (banner: HTMLElement, scrub: Scrub | null) => void) => void unreadHooks.push(hook);
export const registerLayer = (name: LayerName, layer: Layer) => {
  layers[name] = layer;
};

// What `startMap` is given by `main.ts`, which owns what the Map view may not import.
export type MapDeps = {
  // The project of the selected session, null with none.
  selectedProject: () => string | null;
  // Remove project (#381): takes a project off the saved list.
  removeProject: (path: string) => void;
  // Another project's map shows (#386): the list marks its header.
  shown: () => void;
};
let deps: MapDeps;
let project: string | null = null;
let loaded: MapState | null = null;
let view: MapView | null = null;
let drawn = "";
// The past commit shown instead of the default branch's map, in `project`; `openMap` of another project drops it.
let scrub: Scrub | null = null;
// The camera of `project`, made by the first draw of its map from its saved view; `openMap` of another project drops it.
let rig: CameraRig | null = null;
// The sheet's part the camera last revealed, so a later draw does not pan Daniel's view back to it.
let revealed: string | null = null;

// The map is the page's base (#386): the body carries `map-open` from the start and never loses it.
export const mapShown = () => document.body?.classList.contains("map-open") === true;
// The project whose map shows; null before the saved list arrives, and on a fresh install.
export const shownProject = () => project;
export const currentMap = (): MapView | null => view;
export const currentScrub = (): Scrub | null => scrub;

// Shows the map as of `to` read-only, or now again with null (the one setter of a scrub: `scrubTo` in `map-history.ts` calls it).
export function setScrub(to: Scrub | null) {
  scrub = to;
  draw();
}

const latest = newest();

// Reads the project's map; `fetch` asks `origin` first, within the shell's cooldown. A reply for a project no longer shown, or older than the
// newest, is dropped; one that changes nothing draws nothing.
async function load(path: string, fetch: boolean) {
  const state = await latest(invoke("map_state", { project: path, fetch }));
  if (!state || path !== project || !mapShown()) return;
  const key = JSON.stringify(state);
  if (key === drawn && view) return;
  drawn = key;
  loaded = state;
  draw();
}

// The last viewed project, which the page opens on (#386): a per-browser convenience, so a store that throws only loses it.
const LAST = "project";
const lastViewed = (): string | null => {
  try {
    return localStorage.getItem(LAST);
  } catch {
    return null;
  }
};

// Shows `path`'s map.
export function openMap(path: string | null) {
  if (path !== project) {
    // A PR brief and the pre-flight belong to the project they opened on; the next may have no map to draw, so they close here, not by a draw.
    switchProject(layers);
    [scrub, rig, revealed] = [null, null, null];
  }
  project = path;
  loaded = null;
  view = null;
  drawn = "";
  try {
    if (path !== null) localStorage.setItem(LAST, path);
  } catch {
    // The page opens on the first project instead.
  }
  // The top bar names the project shown; Treatment C's large title on the map is not drawn, so the name is said once.
  element("bar-project").textContent = path === null ? "plotkeeper" : name(path);
  element("bar-project").title = path ?? "";
  deps.shown();
  recheckSeen();
  draw();
  // A gone folder has no map to read: its empty state says so, and nothing asks the server about it (#381).
  if (path !== null && !projectGone(path)) {
    logStop("map-open", path);
    showReview(path).catch(failure("stop_facts"));
    load(path, true).catch(failure("map"));
  }
}

// The quiet one-time review, under the head, when it is due for `path` and not dismissed (read when the view opens, never on a timer).
async function showReview(path: string) {
  const box = element("map-review");
  box.hidden = true;
  box.replaceChildren();
  const modules = await reviewOf(path);
  if (!modules?.length || path !== project || !mapShown()) return;
  box.replaceChildren(reviewNode(modules, [path], () => (box.hidden = true)));
  box.hidden = false;
}

// The saved project list changed (#381): its first arrival opens the last viewed project, else the first, unless a selected session's
// already shows (#386); a fresh install's first
// project opens when it is added; a shown project that was removed gives way to the first saved one, and one whose folder went or came
// back is drawn again.
let started = false;
export function projectsChanged() {
  const first = !started;
  started = true;
  // A session selected before the list arrived has already shown its project's map.
  if (project === null) {
    const opening = startProject(lastViewed(), knownProjects());
    if (first || opening !== null) openMap(opening);
    return;
  }
  if (!first && !knownProjects().includes(project)) return openMap(knownProjects()[0] ?? null);
  if (projectGone(project) !== (element("map-body").querySelector(".map-gone") !== null)) openMap(project);
}

// Reads the map again, for a change the view itself made (a tier, "I know this"); the payload decides whether it draws.
export const reloadMap = () => (project === null ? Promise.resolve() : load(project, false));

// The gutter around the plates when the camera reveals or fits them: `--map-gutter` in tokens.css.
const mapGutter = 32;
// The view before Daniel first moves it: the world's corner at the area's, at 100 %.
const unmoved: Camera = { scale: 1, x: 0, y: 0 };

// Puts the world where the camera shows it; `glide` eases there (a brief's framing, a fit, a reveal), a drag or a zoom follows at once.
function applyCamera(glide: boolean) {
  const world = view?.slots.canvas.firstElementChild as HTMLElement | null;
  if (!view || !rig || !world) return;
  const { scale, x, y } = rig.shown();
  world.classList.toggle("glide", glide);
  world.style.transformOrigin = "0 0";
  world.style.transform = `translate(${x}px, ${y}px) scale(${scale})`;
  // The dot grid lies on the area and moves with the world.
  view.slots.canvas.style.setProperty("--cam-x", `${x}px`);
  view.slots.canvas.style.setProperty("--cam-y", `${y}px`);
  view.slots.canvas.style.setProperty("--cam-scale", String(scale));
}

// Each project's view waiting to be saved, so a move on another project never cancels it.
const saving = new Map<string, ReturnType<typeof setTimeout>>();
// Moves Daniel's view, unless a brief holds the camera, and saves it a moment after the last move (never for a past commit's map).
function moveCamera(to: Camera, glide: boolean) {
  if (!rig?.move(to)) return;
  applyCamera(glide);
  const path = project;
  if (path === null || view?.scrub) return;
  clearTimeout(saving.get(path));
  saving.set(path, setTimeout(() => {
    saving.delete(path);
    invoke("map_save_view", { project: path, view: to }).catch(failure("map_save_view"));
  }, 400));
}

// A PR brief's camera, shown while it plays (`map-brief.ts`); Daniel's view is kept as it was.
export function lendCamera(camera: Camera) {
  rig?.lend(camera);
  applyCamera(true);
}
// Shows Daniel's view again, as the brief found it.
export function giveCameraBack() {
  rig?.giveBack();
  applyCamera(true);
}

// Where every plate and label of the view lies in the world, edges included.
const worldBox = (current: MapView) => {
  const edges = Object.entries(current.positions).map(([id, at]) => ({ at, size: sizeOf(current.doc, id) }));
  return {
    x0: Math.min(...edges.map(({ at, size }) => at.x - size.w / 2)), y0: Math.min(...edges.map(({ at, size }) => at.y - size.h / 2)),
    x1: Math.max(...edges.map(({ at, size }) => at.x + size.w / 2)), y1: Math.max(...edges.map(({ at, size }) => at.y + size.h / 2)),
  };
};

// ⌘0 and the palette's Fit map: the whole map in the area.
export function fitMap() {
  if (!mapShown() || !view || !Object.keys(view.positions).length) return;
  const { canvas } = view.slots;
  moveCamera(fitCamera(worldBox(view), { w: canvas.clientWidth, h: canvas.clientHeight }, mapGutter), true);
}

// Esc in the Map view: the first open layer (a drawer, a brief, a scrubbed commit, the sheet, the left panel); on the bare map, nothing.
export const mapEscape = () => void escapeMap(layers);

// The side area shows while any of its slots holds something; the tray floats on its own (#385).
export const syncSide = (slots: Omit<MapView["slots"], "canvas">) => {
  slots.side.hidden = ![slots.notPlaced, slots.runs, slots.sheet, slots.drawer].some((slot) => slot.childElementCount > 0);
  // When a part's sheet opens, the camera pans just enough to show that part whole beside it, once per part, so a later draw never pans
  // Daniel's view back; positions are never written here.
  const shown = view && view.slots === slots ? view : null;
  const focus = shown ? (slots.sheet.querySelector<HTMLElement>("[data-part]")?.dataset.part ?? null) : null;
  if (focus === revealed) return;
  revealed = focus;
  if (focus !== null) revealParts([focus]);
};

// The band of the map area no card covers, in the area's coordinates: from the left panel's right edge to the sheet's (or the drawer's) left edge.
function clearBand(canvas: HTMLElement) {
  const area = canvas.getBoundingClientRect();
  const panel = element("map-panel");
  const card = [element("map-sheet"), element("map-drawer")].find((slot) => slot.childElementCount > 0);
  const left = panel.hidden ? 0 : Math.max(0, panel.getBoundingClientRect().right - area.left);
  const right = card ? Math.min(area.width, card.getBoundingClientRect().left - area.left) : area.width;
  return { left, right };
}

// Pans just enough to show the parts `ids` whole in the band the left panel and the sheet leave (#383, #385). Only the camera moves: no position
// is written, and a brief's camera is never moved.
export function revealParts(ids: string[]) {
  const shown = view;
  if (!shown || !rig || rig.lent() || !shown.slots.canvas.clientWidth) return;
  const spans = ids.flatMap((id) => {
    const at = shown.positions[id];
    return at ? [at.x - sizeOf(shown.doc, id).w / 2, at.x + sizeOf(shown.doc, id).w / 2] : [];
  });
  if (!spans.length) return;
  const to = revealPan(rig.user(), Math.min(...spans), Math.max(...spans), clearBand(shown.slots.canvas), mapGutter);
  if (to !== rig.user()) moveCamera(to, true);
}

const name = (path: string) => path.split("/").filter(Boolean).pop() ?? path;

// The head: the ref the map was read from; the project's name is in the top bar (#386).
function drawHead(state: MapState | null) {
  const ref = element("map-ref");
  ref.textContent = state?.reference ? `${state.reference}${state.commit ? ` · ${state.commit.slice(0, 7)}` : ""}` : "";
  ref.title = state?.note ?? "The branch this map was read from, as last fetched";
}

const lawMark = (law: Law) => {
  const mark = text("span", "", `law-mark ${law.backing}`);
  mark.title = `${law.backing === "check" ? "Backed by a check" : "On trust"}: ${law.text}`;
  return mark;
};

// Where `plotkeeper install` is explained.
const installDocs = "https://github.com/danielrispler/plotkeeper/blob/main/docs/studio/plugin.md";

// The empty states: a fresh install, a gone folder, a project with no map on its default branch, one whose file does not read, and the wait for the first read.
function drawEmpty(body: HTMLElement, state: MapState | null) {
  const box = text("div", "", "map-empty");
  if (project !== null && projectGone(project)) {
    // A saved project whose folder was moved or deleted (#381): never an error, and Remove project is here.
    const gone = project;
    box.classList.add("map-gone");
    const remove = pressable(text("button", "Remove project", "btn"), null, () => deps.removeProject(gone));
    remove.title = "Take this project off the list; its sessions stay in History";
    box.append(text("p", "This folder is gone", "map-empty-title"), text("p", `${gone} was moved or deleted. Its ended sessions stay in History.`, "empty"), remove);
  } else if (project === null) {
    // A fresh install (#386): one calm line, and the way to record the agents started in a terminal.
    const install = text("a", "How to install plotkeeper", "map-install");
    install.setAttribute("href", installDocs);
    install.setAttribute("target", "_blank");
    install.setAttribute("rel", "noreferrer");
    install.title = "How to record the agents you start in your terminals: plotkeeper install";
    box.classList.add("map-fresh");
    // Each empty screen says what will appear there (#526), whatever the Explanations switch says.
    box.append(
      text("p", "Run your agent in a repository; it appears here", "map-empty-title"),
      text("p", "plotkeeper follows each agent session and maps what it changes.", "empty"),
      install,
    );
  } else if (state === null) {
    box.append(text("p", "Reading the map…", "empty"));
  } else if (state.status === "unreadable") {
    box.append(text("p", "This map does not read", "map-empty-title"), text("p", state.note ?? "", "empty"));
  } else {
    const where = state.reference ?? "the default branch";
    box.append(
      text("p", "No map yet", "map-empty-title"),
      text("p", `${name(state.project)} has no system-map.json on ${where}.`, "empty"),
      text("p", "A map names your parts and their rules; changes light them up.", "empty"),
    );
    // The page starts no session: the prompt is shown to give to one in the project.
    box.append(text("p", "To draft one, give this to a session in the project:", "empty"), text("pre", draftPrompt, "map-draft"));
  }
  body.replaceChildren(box);
}


// The links as lines from rim to rim, one per pair of ends (spec #473), with the laws on any of its links as marks about its middle, 10px apart. A line's
// `data-link` holds all its links' ids, space-separated, and `data-from` the end it is drawn from.
function drawLinks(layer: Element, view: MapView) {
  const nodes: Element[] = [];
  for (const line of linkLines(view.doc.links)) {
    const [from, to] = [view.positions[line.from], view.positions[line.to]];
    if (!from || !to) continue;
    const a = rimPoint(from, sizeOf(view.doc, line.from), to);
    const b = rimPoint(to, sizeOf(view.doc, line.to), from);
    nodes.push(svg("line", { class: "link", "data-link": line.ids.join(" "), "data-from": line.from, x1: String(a.x), y1: String(a.y), x2: String(b.x), y2: String(b.y) }));
    const laws = view.doc.laws.filter((law) => line.ids.includes(law.on));
    const points = markPoints(a, b, laws.length);
    laws.forEach((law, i) => {
      const at = points[i];
      if (!at) return;
      const mark = svg("rect", { class: `link-mark ${law.backing}`, width: "7", height: "7", x: "-3.5", y: "-3.5", transform: `translate(${at.x} ${at.y}) rotate(45)` });
      const title = svg("title", {});
      mark.append(title);
      title.textContent = `${law.backing === "check" ? "Backed by a check" : "On trust"}: ${law.text}`;
      nodes.push(mark);
    });
  }
  layer.replaceChildren(...nodes);
}

const place = (node: HTMLElement, at: Point) => {
  node.style.left = `${at.x}px`;
  node.style.top = `${at.y}px`;
};

// The positions of the parts and actors: the saved ones, and the rest placed once. A past commit's map is read-only: what it places is
// not saved.
function placeAll(state: MapState, doc: LoadedMap, past: Scrub | null) {
  const saved = Object.fromEntries(Object.entries(state.user.positions).map(([id, [x, y]]) => [id, { x, y }]));
  const items = [...doc.parts, ...doc.actors];
  const added = autoPlace(items, doc.links, saved);
  if (Object.keys(added).length && !past) {
    // Saved the first time they are placed, so they never move again on their own.
    const stored = Object.fromEntries(Object.entries(added).map(([id, at]) => [id, [at.x, at.y] as [number, number]]));
    state.user.positions = { ...state.user.positions, ...stored };
    invoke("map_save_positions", { project: state.project, positions: stored }).catch(failure("map_save_positions"));
  }
  return { ...saved, ...added };
}

// A part's plate: its tier and fog, its laws' marks and a broken law's, and its button. A past commit's parts get no fog or broken-law
// mark.
function plateNode(part: Part, current: MapView, links: SVGElement, fit: () => void): HTMLElement {
  const { state, doc, scrub: past } = current;
  const tier = effectiveTier(part, state.user.tiers);
  // Fog under Daniel's effective tier, as the server worked it out (`fog.rs`); a part not listed is clear.
  const mist = past ? undefined : state.fog?.[part.id];
  const plate = text("div", "", `plate tier-${tier}${mist ? ` fog-${mist}` : ""}`);
  plate.dataset.id = part.id;
  const [fog, marks, chips] = [text("div", "", "plate-fog"), text("div", "", "plate-marks"), text("div", "", "plate-chips")];
  marks.append(...doc.laws.filter((law) => law.on === part.id).map(lawMark));
  // A test that backs a law failed on main: one coarse mark, never naming the law. Red, beside Needs you the only loud colour.
  if (!past && brokenParts(state.ci).has(part.id)) {
    const broken = text("span", "", "law-broken");
    broken.title = brokenText;
    marks.append(broken);
    plate.classList.add("broken");
  }
  // The keyboard's way in (#460): a real button over the text area, named by the name and caption, beside the chips rather than around
  // them. Its click reaches the plate's own, so Enter and Space open the sheet, or answer a brief's order question, as a tap does.
  const open = text("button", "", "btn plate-open");
  open.append(text("span", part.name, "plate-name"), text("span", part.caption, "plate-caption"));
  plate.append(fog, open, marks, chips);
  plate.title = plateTitle(part);
  const context: PlateContext = { view: current, part, tier, plate, fog, marks, chips };
  plateHooks.forEach((hook) => hook(context));
  grab(plate, part.id, current, links, fit, () => clickHooks.forEach((hook) => hook(context)));
  return plate;
}

// Draws the parts as plates, the actors as labels and the links between them, placing what has no saved position once.
function drawMap(body: HTMLElement, state: MapState, doc: LoadedMap, past: Scrub | null) {
  const positions = placeAll(state, doc, past);
  const canvas = text("div", "", "map-canvas");
  const world = text("div", "", "map-world");
  const links = svg("svg", { class: "map-links", "aria-hidden": "true" });
  world.append(links);
  const slots = { canvas, side: element("map-side"), notPlaced: element("map-not-placed"), tray: element("map-tray"), runs: element("map-runs"), sheet: element("map-sheet"), drawer: element("map-drawer"), banner: element("map-banner") };
  const current: MapView = { project: state.project, state, doc, positions, scrub: past, slots };

  const fit = () => {
    const reach = Object.entries(positions).map(([id, at]) => ({ x: at.x + sizeOf(current.doc, id).w / 2, y: at.y + sizeOf(current.doc, id).h / 2 }));
    world.style.width = `${Math.max(canvas.clientWidth, ...reach.map((p) => p.x + plateSize.w / 2))}px`;
    world.style.height = `${Math.max(canvas.clientHeight, ...reach.map((p) => p.y + plateSize.h / 2))}px`;
    links.setAttribute("width", world.style.width.replace("px", ""));
    links.setAttribute("height", world.style.height.replace("px", ""));
  };
  const nodes = new Map<string, HTMLElement>();
  const elements: HTMLElement[] = [];

  // In reading order, so Tab walks the parts top to bottom, then left to right.
  const parts = new Map(doc.parts.map((part) => [part.id, part]));
  const byId = Object.fromEntries(doc.parts.flatMap((part) => {
    const at = positions[part.id];
    return at ? [[part.id, at]] : [];
  }));
  for (const part of readingOrder(byId).flatMap((id) => parts.get(id) ?? [])) {
    const plate = plateNode(part, current, links, fit);
    nodes.set(part.id, plate);
    elements.push(plate);
  }
  for (const actor of doc.actors) {
    const label = text("div", actor.name, "actor");
    label.dataset.id = actor.id;
    grab(label, actor.id, current, links, fit, () => {});
    nodes.set(actor.id, label);
    elements.push(label);
  }
  for (const [id, node] of nodes) {
    const at = positions[id];
    if (at) place(node, at);
  }
  world.append(...elements);
  canvas.append(world);
  // The keyboard on a part's button stays on that part's new button (#452's review).
  const focused = document.activeElement;
  const kept = focused?.matches(".plate-open") && body.contains(focused) ? focused.closest<HTMLElement>(".plate")?.dataset.id : undefined;
  body.replaceChildren(canvas);
  if (kept) nodes.get(kept)?.querySelector<HTMLElement>(".plate-open")?.focus({ preventScroll: true });
  fit();
  drawLinks(links, current);
  view = current;
  rig ??= cameraRig(state.user.view ?? unmoved);
  applyCamera(false);
  pan(canvas);
  drawHooks.forEach((hook) => hook(current));
  syncSide(slots);
}

// How far, in screen pixels, a press on the empty map may move and still be a click (#445, tldraw's threshold).
const clickSlop = 4;

// The map area's own input: a drag of the empty background pans Daniel's view, and a wheel, a two-finger scroll or a pinch (a wheel
// with Ctrl held, as Chrome reports it) zooms around the pointer and never scrolls the page.
// A press that never moved past `clickSlop` before its release is a click on the empty map: it closes the part's sheet and its drawer (#445).
function pan(canvas: HTMLElement) {
  canvas.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || !rig || rig.lent() || (event.target as Element).closest(".plate, .actor")) return;
    const [start, from] = [rig.user(), { x: event.clientX, y: event.clientY }];
    let far = 0;
    canvas.setPointerCapture(event.pointerId);
    canvas.classList.add("panning");
    const move = (moved: PointerEvent) => {
      far = Math.max(far, Math.hypot(moved.clientX - from.x, moved.clientY - from.y));
      moveCamera(panTo(start, from, { x: moved.clientX, y: moved.clientY }), false);
    };
    const end = (ended: PointerEvent) => {
      canvas.classList.remove("panning");
      canvas.removeEventListener("pointermove", move);
      canvas.removeEventListener("pointerup", end);
      canvas.removeEventListener("pointercancel", end);
      if (ended.type !== "pointerup" || Math.max(far, Math.hypot(ended.clientX - from.x, ended.clientY - from.y)) > clickSlop) return;
      for (const name of ["drawer", "sheet"] as const) {
        const layer = layers[name];
        if (layer?.open()) layer.close();
      }
    };
    canvas.addEventListener("pointermove", move);
    canvas.addEventListener("pointerup", end);
    canvas.addEventListener("pointercancel", end);
  });
  canvas.addEventListener("wheel", (event) => {
    event.preventDefault();
    if (!rig || rig.lent()) return;
    const box = canvas.getBoundingClientRect();
    // A pinch sends small steps, a mouse wheel large ones: the prototype's rates.
    const factor = Math.exp(-event.deltaY * (event.ctrlKey ? 0.012 : 0.004));
    moveCamera(zoomAt(rig.user(), factor, event.clientX - box.left, event.clientY - box.top), false);
  }, { passive: false });
}

// A press on a plate or label, which becomes a drag only past the threshold: a plate then follows the pointer and its place is
// saved when it is let go; a press that never moved is a click, which `click` handles, and one that did swallows its click.
function grab(node: HTMLElement, id: string, current: MapView, links: Element, fit: () => void, click: () => void) {
  let drag: Drag | null = null;
  let swallow = false;
  // The drag runs on the screen, so its threshold is in screen pixels whatever the zoom; the plate's place is that over the scale.
  let scale = 1;
  node.addEventListener("pointerdown", (event) => {
    // A press on a control on the plate, such as an agent's chip, is the control's: capturing it here would give its click to the plate.
    // The part's own open button is the plate's, so a press on its text still drags (#460).
    if (event.button !== 0 || (event.target as Element).closest("button:not(.plate-open)")) return;
    scale = rig?.shown().scale ?? 1;
    const at = current.positions[id];
    if (!at) return;
    drag = dragStart(id, { x: event.clientX, y: event.clientY }, { x: at.x * scale, y: at.y * scale });
    node.setPointerCapture(event.pointerId);
  });
  node.addEventListener("pointermove", (event) => {
    if (!drag || !node.hasPointerCapture(event.pointerId)) return;
    const step = dragMove(drag, { x: event.clientX, y: event.clientY });
    drag = step.drag;
    if (!step.to) return;
    const half = sizeOf(current.doc, id);
    const to = { x: Math.max(half.w / 2, step.to.x / scale), y: Math.max(half.h / 2, step.to.y / scale) };
    current.positions[id] = to;
    node.classList.add("dragging");
    place(node, to);
    fit();
    drawLinks(links, current);
  });
  const end = () => {
    const done = drag;
    drag = null;
    node.classList.remove("dragging");
    if (!done?.moved) return;
    swallow = true;
    if (current.scrub) return;
    const at = current.positions[id];
    if (!at) return;
    current.state.user.positions = { ...current.state.user.positions, [id]: [at.x, at.y] };
    invoke("map_save_positions", { project: current.project, positions: { [id]: [at.x, at.y] } }).catch(failure("map_save_positions"));
  };
  node.addEventListener("pointerup", end);
  node.addEventListener("pointercancel", end);
  node.addEventListener("click", () => {
    if (swallow) swallow = false;
    else click();
  });
}

function draw() {
  drawHead(loaded);
  element("map-legend").hidden = loaded?.status !== "loaded";
  const body = element("map-body");
  const past = loaded?.status === "loaded" ? scrub : null;
  const doc = loaded?.status === "loaded" && loaded.source !== null ? parseMap(past?.source ?? loaded.source) : null;
  if (loaded && doc?.ok) return drawMap(body, loaded, doc, past);
  view = null;
  const unreadable = loaded && doc && !doc.ok ? { ...loaded, status: "unreadable" as const, note: doc.why } : loaded;
  drawEmpty(body, unreadable);
  const slots = { side: element("map-side"), notPlaced: element("map-not-placed"), tray: element("map-tray"), runs: element("map-runs"), sheet: element("map-sheet"), drawer: element("map-drawer"), banner: element("map-banner") };
  // No map is drawn, so nothing of the last one may stay: its banner, sheet and drawer would belong to another project.
  [slots.notPlaced, slots.tray, slots.runs, slots.sheet, slots.drawer, slots.banner].forEach((slot) => slot.replaceChildren());
  syncSide(slots);
  // A scrubbed commit whose map cannot be read still shows the way back to now.
  unreadHooks.forEach((hook) => hook(slots.banner, scrub));
}

// Wires the view: the project menu, the window's focus and a merge the shell saw.
export function startMap(given: MapDeps) {
  deps = given;
  element("map-legend").append(
    lawMark({ id: "", on: "", backing: "check", text: "a test stands behind it" }), " law, backed by a check ",
    lawMark({ id: "", on: "", backing: "trust", text: "nothing checks it" }), " law, on trust",
  );
  element("map-legend").append(
    " ", text("span", "", "law-broken"), " a test that backs a law failed on main ",
    text("span", "", "mist-swatch thin"), " fog, files changed ", text("span", "", "mist-swatch thick"), " fog, laws, links or flows changed",
  );
  // Never on a timer: the focus and a merge.
  window.addEventListener("focus", () => {
    if (mapShown() && project !== null && !projectGone(project)) load(project, true).catch(failure("map"));
  });
  listen<string>("map-landed", (landed) => {
    if (mapShown() && landed === project) load(landed, false).catch(failure("map"));
  });
}
