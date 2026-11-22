// The Map view (spec danielrispler/plotkeeper#306, ticket #309): a project's system map as plates on a quiet chart (Treatment C),
// over the story rail and the terminal column, with the session list and the top bar left live. The shell reads the map from
// `origin`'s default branch and keeps Daniel's positions (`src-tauri/src/map.rs`); `map-model.ts` holds the pure logic.
//
// Where the next tickets plug in, so they add to this file rather than reshape it:
// - `onPlate(fn)`: runs for each plate each time it is drawn, with the plate's slots: `fog` behind the name, `marks` at its top
//   right (law marks sit there; a broken-law mark joins them), `chips` along its foot (live agents). A ticket fills a slot.
// - `onPlateClick(fn)`: a click on a plate that was not a drag, such as one that opens the part's sheet.
// - `onDraw(fn)`: runs after each draw with the whole `MapView`, for what is not on a plate: the side area (`slots.notPlaced`,
//   `slots.sheet`, `slots.drawer`, shown by `syncSide`) and the banner (`slots.banner`).
// - `registerLayer(name, layer)`: Esc's layers, `scrubbed`, `drawer` and `sheet`, peeled in that order before the view is left.
// - `MapState` (`types.ts`, `map.rs`) is the one payload; `currentMap()` gives the view as drawn.

import { logStop, reviewNode, reviewOf } from "./stop-rules.js";
import { element, failure, invoke, listen, newest, pressable, text } from "./dom.js";
import {
  actorSize, autoPlace, brokenParts, brokenText, draftPrompt, dragMove, dragStart, effectiveTier, escapeMap, type Actor, type Drag, type Law, type Layer, type LayerName,
  type MapDoc, mapProject, type MapLink, parseMap, type Part, type Point, plateSize, rimPoint, scrollToShow, type Scrub, type Tier,
} from "./map-model.js";
import { recheckSeen } from "./seen.js";
import { knownProjects, projects } from "./projects.js";
import type { MapState } from "./types.js";

export type LoadedMap = Extract<MapDoc, { ok: true }>;
// What the view holds once a map loaded: its payload, the map as parsed, and every id's centre in the world.
export type MapView = {
  project: string;
  state: MapState;
  doc: LoadedMap;
  positions: Record<string, Point>;
  // The past commit this view shows read-only (no fog, no live agents, no CI), null when it shows now (ticket #313).
  scrub: Scrub | null;
  slots: { canvas: HTMLElement; side: HTMLElement; notPlaced: HTMLElement; tray: HTMLElement; sheet: HTMLElement; drawer: HTMLElement; banner: HTMLElement };
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
export const registerLayer = (name: LayerName, layer: Layer) => void (layers[name] = layer);

// What `startMap` is given by `main.ts`, which owns what the Map view may not import.
export type MapDeps = {
  // Opens New session in `project` with `prompt` in its quick start.
  draft: (project: string, prompt: string) => void;
  // The project of the selected session, null with none.
  selectedProject: () => string | null;
  // Gives the keyboard back to the terminal pane the view covered.
  refocus: () => void;
};
let deps: MapDeps;
let project: string | null = null;
let loaded: MapState | null = null;
let view: MapView | null = null;
let drawn = "";
// The past commit shown instead of the default branch's map, in `project`; `openMap` of another project drops it.
let scrub: Scrub | null = null;

export const mapShown = () => document.body?.classList.contains("map-open") === true;
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

// Opens the Map view on `path`.
export function openMap(path: string | null) {
  if (path !== project) scrub = null;
  project = path;
  loaded = null;
  view = null;
  drawn = "";
  // The terminal's textarea may hold the keyboard; Esc and the map key must reach the window.
  (document.activeElement as HTMLElement | null)?.blur?.();
  document.body.classList.add("map-open");
  element("map").hidden = false;
  recheckSeen();
  draw();
  if (path !== null) {
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

// Reads the map again, for a change the view itself made (a tier, "I know this"); the payload decides whether it draws.
export const reloadMap = () => (project === null ? Promise.resolve() : load(project, false));

// Leaves the Map view; what was under it shows again. No-op when it is not showing.
export function leaveMap() {
  if (!mapShown()) return;
  document.body.classList.remove("map-open");
  element("map").hidden = true;
  recheckSeen();
  hideProjects();
  deps?.refocus();
}

// The map key: opens the Map view on the selected session's project, else the first project with a map; again, leaves it.
export async function toggleMap() {
  if (mapShown()) return leaveMap();
  const paths = await projects().catch(() => knownProjects());
  const selected = deps.selectedProject();
  const withMap = new Set<string>();
  if (selected === null) {
    for (const path of paths) {
      const state = await invoke("map_state", { project: path, fetch: false }).catch(() => null);
      if (state?.status === "loaded") {
        withMap.add(path);
        break;
      }
    }
  }
  openMap(mapProject(selected, paths, withMap));
}

const hideProjects = () => {
  const menu = element("map-projects");
  if (menu.matches(":popover-open")) menu.hidePopover();
};

// The gutter around the plates when the map scrolls them into view: `--map-gutter` in tokens.css.
const mapGutter = 32;

// Esc in the Map view: the project menu first, then the first open layer (a scrubbed commit, a drawer, a sheet), else the view itself.
export function mapEscape() {
  if (element("map-projects").matches(":popover-open")) return hideProjects();
  if (escapeMap(layers) === "leave") leaveMap();
}

// The side area shows while any of its slots holds something.
export const syncSide = (slots: MapView["slots"]) => {
  slots.side.hidden = ![slots.notPlaced, slots.tray, slots.sheet, slots.drawer].some((slot) => slot.childElementCount > 0);
  // The map area ends where the side area begins; a plate it now cuts is scrolled into whole view when the plates fit.
  // Only plates count: an actor's label is not worth a plate cut off, and a map too wide for the area keeps its plates whole first.
  const placed = view && view.slots === slots ? Object.entries(view.positions).filter(([id]) => !view!.doc.actors.some((actor) => actor.id === id)) : [];
  if (!placed.length || !slots.canvas?.clientWidth) return;
  const { canvas } = slots;
  const edges = placed.map(([id, at]) => [at.x - sizeOf(id, view!).w / 2, at.x + sizeOf(id, view!).w / 2]);
  canvas.scrollLeft = scrollToShow(Math.min(...edges.map(([a]) => a)), Math.max(...edges.map(([, b]) => b)), canvas.scrollLeft, canvas.clientWidth, mapGutter);
};

const name = (path: string) => path.split("/").filter(Boolean).pop() ?? path;

// The head: the project's name, which opens the list of projects, and the ref the map was read from.
function drawHead(state: MapState | null) {
  const button = element("map-project");
  button.textContent = project === null ? "Map" : `${name(project)} ▾`;
  if (project !== null) button.title = `${project}\nShow another project's map`;
  button.hidden = project === null;
  const ref = element("map-ref");
  ref.textContent = state?.reference ? `${state.reference}${state.commit ? ` · ${state.commit.slice(0, 7)}` : ""}` : "";
  ref.title = state?.note ?? "The branch this map was read from, as last fetched";
}

function drawProjects() {
  const menu = element("map-projects");
  const choose = (path: string) => {
    hideProjects();
    if (path !== project) openMap(path);
  };
  menu.replaceChildren(
    ...knownProjects().map((path) => {
      const row = pressable(text("button", name(path), "btn"), path === project, () => choose(path));
      row.title = path;
      return row;
    }),
  );
}

const lawMark = (law: Law) => {
  const mark = text("span", "", `law-mark ${law.backing}`);
  mark.title = `${law.backing === "check" ? "Backed by a check" : "On trust"}: ${law.text}`;
  return mark;
};

// The empty states: a project with no map on its default branch, one whose file does not read, and the wait for the first read.
function drawEmpty(body: HTMLElement, state: MapState | null) {
  const box = text("div", "", "map-empty");
  if (project === null) {
    box.append(text("p", "No project yet", "map-empty-title"), text("p", "Add a project and its map shows here.", "empty"));
  } else if (state === null) {
    box.append(text("p", "Reading the map…", "empty"));
  } else if (state.status === "unreadable") {
    box.append(text("p", "This map does not read", "map-empty-title"), text("p", state.note ?? "", "empty"));
  } else {
    const where = state.reference ?? "the default branch";
    box.append(
      text("p", "No map yet", "map-empty-title"),
      text("p", `${name(state.project)} has no system-map.json on ${where}.`, "empty"),
    );
    const draft = pressable(text("button", "Draft a map", "btn primary"), false, () => deps.draft(state.project, draftPrompt));
    draft.removeAttribute("aria-pressed");
    draft.title = "Start a session in this project that drafts system-map.json and checks it with system-map check";
    box.append(draft);
  }
  body.replaceChildren(box);
}

const svg = (tag: string, attrs: Record<string, string>) => {
  const node = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  return node;
};

const sizeOf = (id: string, view: MapView) => (view.doc.actors.some((actor) => actor.id === id) ? actorSize : plateSize);

// The links as lines from rim to rim, with the laws on a link as marks at its middle.
function drawLinks(layer: Element, view: MapView) {
  const nodes: Element[] = [];
  for (const link of view.doc.links) {
    const [from, to] = [view.positions[link.from], view.positions[link.to]];
    if (!from || !to) continue;
    const a = rimPoint(from, sizeOf(link.from, view), to);
    const b = rimPoint(to, sizeOf(link.to, view), from);
    nodes.push(svg("line", { class: "link", "data-link": link.id, x1: String(a.x), y1: String(a.y), x2: String(b.x), y2: String(b.y) }));
    for (const law of view.doc.laws.filter((law) => law.on === link.id)) {
      const mark = svg("rect", { class: `link-mark ${law.backing}`, width: "7", height: "7", x: "-3.5", y: "-3.5", transform: `translate(${(a.x + b.x) / 2} ${(a.y + b.y) / 2}) rotate(45)` });
      mark.append(svg("title", {}));
      mark.firstChild!.textContent = `${law.backing === "check" ? "Backed by a check" : "On trust"}: ${law.text}`;
      nodes.push(mark);
    }
  }
  layer.replaceChildren(...nodes);
}

const place = (node: HTMLElement, at: Point) => {
  node.style.left = `${at.x}px`;
  node.style.top = `${at.y}px`;
};

// Draws the parts as plates, the actors as labels and the links between them, placing what has no saved position once.
function drawMap(body: HTMLElement, state: MapState, doc: LoadedMap, past: Scrub | null) {
  const saved = Object.fromEntries(Object.entries(state.user.positions).map(([id, [x, y]]) => [id, { x, y }]));
  const items = [...doc.parts, ...doc.actors];
  const added = autoPlace(items, doc.links, saved);
  // A past commit's map is read-only: what it places is not saved, and its parts get no fog or broken-law mark.
  if (Object.keys(added).length && !past) {
    // Saved the first time they are placed, so they never move again on their own.
    const stored = Object.fromEntries(Object.entries(added).map(([id, at]) => [id, [at.x, at.y] as [number, number]]));
    state.user.positions = { ...state.user.positions, ...stored };
    invoke("map_save_positions", { project: state.project, positions: stored }).catch(failure("map_save_positions"));
  }
  const positions = { ...saved, ...added };

  const canvas = text("div", "", "map-canvas");
  const world = text("div", "", "map-world");
  const links = svg("svg", { class: "map-links", "aria-hidden": "true" });
  world.append(links);
  const slots = { canvas, side: element("map-side"), notPlaced: element("map-not-placed"), tray: element("map-tray"), sheet: element("map-sheet"), drawer: element("map-drawer"), banner: element("map-banner") };
  const current: MapView = { project: state.project, state, doc, positions, scrub: past, slots };

  const fit = () => {
    const reach = Object.entries(positions).map(([id, at]) => ({ x: at.x + sizeOf(id, current).w / 2, y: at.y + sizeOf(id, current).h / 2 }));
    world.style.width = `${Math.max(canvas.clientWidth, ...reach.map((p) => p.x + plateSize.w / 2))}px`;
    world.style.height = `${Math.max(canvas.clientHeight, ...reach.map((p) => p.y + plateSize.h / 2))}px`;
    links.setAttribute("width", world.style.width.replace("px", ""));
    links.setAttribute("height", world.style.height.replace("px", ""));
  };
  const nodes = new Map<string, HTMLElement>();
  const elements: HTMLElement[] = [];

  for (const part of doc.parts) {
    const tier = effectiveTier(part, state.user.tiers);
    // Fog under Daniel's effective tier, as the shell worked it out (`fog.rs`); a part not listed is clear.
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
    plate.append(fog, text("span", part.name, "plate-name"), text("span", part.caption, "plate-caption"), marks, chips);
    plate.title = [part.name, part.caption, part.description].filter(Boolean).join("\n\n");
    const context: PlateContext = { view: current, part, tier, plate, fog, marks, chips };
    plateHooks.forEach((hook) => hook(context));
    grab(plate, part.id, current, links, fit, () => clickHooks.forEach((hook) => hook(context)));
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
  for (const [id, node] of nodes) place(node, positions[id]);
  world.append(...elements);
  canvas.append(world);
  body.replaceChildren(canvas);
  fit();
  drawLinks(links, current);
  view = current;
  drawHooks.forEach((hook) => hook(current));
  syncSide(slots);
}

// A press on a plate or label, which becomes a drag only past the threshold: a plate then follows the pointer and its place is
// saved when it is let go; a press that never moved is a click, which `click` handles, and one that did swallows its click.
function grab(node: HTMLElement, id: string, current: MapView, links: Element, fit: () => void, click: () => void) {
  let drag: Drag | null = null;
  let swallow = false;
  node.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    drag = dragStart(id, { x: event.clientX, y: event.clientY }, current.positions[id]);
    node.setPointerCapture(event.pointerId);
  });
  node.addEventListener("pointermove", (event) => {
    if (!drag || !node.hasPointerCapture(event.pointerId)) return;
    const step = dragMove(drag, { x: event.clientX, y: event.clientY });
    drag = step.drag;
    if (!step.to) return;
    const half = sizeOf(id, current);
    const to = { x: Math.max(half.w / 2, step.to.x), y: Math.max(half.h / 2, step.to.y) };
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
  const unreadable = doc && !doc.ok ? { ...loaded!, status: "unreadable" as const, note: doc.why } : loaded;
  drawEmpty(body, unreadable);
  const slots = { side: element("map-side"), notPlaced: element("map-not-placed"), tray: element("map-tray"), sheet: element("map-sheet"), drawer: element("map-drawer"), banner: element("map-banner") };
  // No map is drawn, so nothing of the last one may stay: its banner, sheet and drawer would belong to another project.
  [slots.notPlaced, slots.tray, slots.sheet, slots.drawer, slots.banner].forEach((slot) => slot.replaceChildren());
  syncSide(slots as MapView["slots"]);
  // A scrubbed commit whose map cannot be read still shows the way back to now.
  unreadHooks.forEach((hook) => hook(slots.banner, scrub));
}

// Wires the view: the project menu, the window's focus and a merge the shell saw.
export function startMap(given: MapDeps) {
  deps = given;
  element("map-projects").addEventListener("toggle", (event) => {
    if ((event as ToggleEvent).newState === "open") drawProjects();
  });
  element("map-legend").append(
    lawMark({ id: "", on: "", backing: "check", text: "a test stands behind it" }), " law, backed by a check ",
    lawMark({ id: "", on: "", backing: "trust", text: "nothing checks it" }), " law, on trust",
  );
  element("map-legend").append(
    " ", text("span", "", "law-broken"), " a test that backs a law failed on main ",
    text("span", "", "mist-swatch thin"), " fog, files changed ", text("span", "", "mist-swatch thick"), " fog, laws, links or flows changed",
  );
  // Never on a timer: the focus and a merge, while the view is open.
  window.addEventListener("focus", () => {
    if (mapShown() && project !== null) load(project, true).catch(failure("map"));
  });
  listen<string>("map-landed", (landed) => {
    if (mapShown() && landed === project) load(landed, false).catch(failure("map"));
  });
}
