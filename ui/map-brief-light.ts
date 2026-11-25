// The PR brief's light on the map (spec #548, Group 4): the parts a step lights, the camera framing them above the panel, and the light
// that runs along the step's links. `map-brief.ts` calls it for each step it shows.

import { svg } from "./dom.js";
import { giveCameraBack, lendCamera, type MapView } from "./map.js";
import { briefCamera } from "./map-brief-model.js";
import { type Rect, sizeOf } from "./map-model.js";
import { chapterParts, panel, steps } from "./map-brief-player.js";
import type { BriefStep } from "./types.js";

export const reduced = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
// The light's speed along a link, in world pixels a second.
const SPEED = 420;

export const world = (view: MapView) => view.slots.canvas.firstElementChild as HTMLElement | null;
export const plateOf = (view: MapView, id: string) => view.slots.canvas.querySelector<HTMLElement>(`[data-id="${CSS.escape(id)}"]`);

/** The map as it was before the brief: no part lit, Daniel's camera back, no light. */
export function unlight(view: MapView) {
  const root = world(view);
  if (!root) return;
  root.classList.remove("briefing");
  giveCameraBack();
  view.slots.canvas.querySelectorAll(".lit, .arrive").forEach((node) => node.classList.remove("lit", "arrive"));
  clearOrder(view);
  root.querySelector(".map-light")?.remove();
}

const centre = (view: MapView, id: string) => view.positions[id];
const ends = (view: MapView, [link, backwards]: [string, boolean]): [string, string] | null => {
  const found = view.doc.links.find((l) => l.id === link);
  return found ? (backwards ? [found.to, found.from] : [found.from, found.to]) : null;
};

// What a brief frames: the parts its steps light and the ones their hops reach, with room around them.
function frameOf(view: MapView, list: BriefStep[]): Rect | null {
  const ids = new Set([...list.flatMap((step) => [...step.on, ...step.hops.flatMap((hop) => ends(view, hop) ?? [])]), ...chapterParts()]);
  const boxes = [...ids].flatMap((id) => {
    const at = centre(view, id);
    return at ? [{ at, size: sizeOf(view.doc, id) }] : [];
  });
  if (!boxes.length) return null;
  return {
    x0: Math.min(...boxes.map((b) => b.at.x - b.size.w / 2)) - 70,
    y0: Math.min(...boxes.map((b) => b.at.y - b.size.h / 2)) - 56,
    x1: Math.max(...boxes.map((b) => b.at.x + b.size.w / 2)) + 70,
    y1: Math.max(...boxes.map((b) => b.at.y + b.size.h / 2)) + 70,
  };
}

/** Frames the brief's parts, else the step's own, above the panel. */
export function aim(view: MapView, step: BriefStep) {
  const root = world(view);
  if (!root) return;
  const canvas = view.slots.canvas;
  const area = { w: canvas.clientWidth, h: canvas.clientHeight };
  const room = { top: 26, bottom: panel().offsetHeight + 40 };
  const all = frameOf(view, steps());
  // One framing for every step; a window too low for that frames each step's own parts.
  let box = all;
  let camera = box && briefCamera(box, area, room, 1.35);
  if (!camera || camera.scale < 0.7) {
    box = frameOf(view, [step]) ?? box;
    camera = box && briefCamera(box, area, room, 1.35);
  }
  if (camera) lendCamera(camera);
}

// The light's run: a newer run, or a stop, ends the one before at its next frame.
let run = 0;
export const stopLight = () => void run++;

function arrive(view: MapView, id: string) {
  const node = plateOf(view, id);
  if (!node) return;
  node.classList.add("lit");
  node.classList.remove("arrive");
  void node.offsetWidth;
  node.classList.add("arrive");
}

/** The light travels the step's links at one speed, leaving each lit behind it; a part lights as the light reaches it. */
export async function runLight(view: MapView, hops: [string, boolean][]) {
  const mine = ++run;
  const root = world(view);
  const links = root?.querySelector(".map-links");
  if (!root || !links) return;
  root.querySelector(".map-light")?.remove();
  const layer = svg("svg", { class: "map-light", "aria-hidden": "true", width: links.getAttribute("width") ?? "0", height: links.getAttribute("height") ?? "0" });
  root.append(layer);
  const legs = hops.flatMap((hop) => {
    // A pair's links share one line (spec #473), whose `data-link` lists them all.
    const line = links.querySelector(`[data-link~="${CSS.escape(hop[0])}"]`);
    const pair = ends(view, hop);
    if (!line || !pair) return [];
    const read = (name: string) => Number(line.getAttribute(name));
    const [p, q] = [{ x: read("x1"), y: read("y1") }, { x: read("x2"), y: read("y2") }];
    // The rim points run from the line's `data-from`; the light runs from the hop's first end.
    const [from, to] = line.getAttribute("data-from") === pair[0] ? [p, q] : [q, p];
    return [{ from, to, ids: pair, len: Math.hypot(to.x - from.x, to.y - from.y) }];
  });
  const lit = (leg: (typeof legs)[number]) => svg("line", { class: "light-trail", x1: String(leg.from.x), y1: String(leg.from.y), x2: String(leg.to.x), y2: String(leg.to.y) });
  const first = legs[0];
  if (!first) return;
  if (reduced()) {
    for (const leg of legs) {
      layer.append(lit(leg));
      leg.ids.forEach((id) => plateOf(view, id)?.classList.add("lit"));
    }
    return;
  }
  const spark = svg("circle", { class: "light-spark", r: "5" });
  layer.append(spark);
  arrive(view, first.ids[0]);
  for (const leg of legs) {
    const trail = lit(leg);
    layer.insertBefore(trail, spark);
    const started = performance.now();
    const time = Math.max(160, (leg.len / SPEED) * 1000);
    await new Promise<void>((done) => {
      const tick = (now: number) => {
        if (mine !== run) return done();
        const k = Math.min(1, (now - started) / time);
        spark.setAttribute("cx", String(leg.from.x + (leg.to.x - leg.from.x) * k));
        spark.setAttribute("cy", String(leg.from.y + (leg.to.y - leg.from.y) * k));
        trail.setAttribute("x2", String(leg.from.x + (leg.to.x - leg.from.x) * k));
        trail.setAttribute("y2", String(leg.from.y + (leg.to.y - leg.from.y) * k));
        if (k < 1) requestAnimationFrame(tick);
        else done();
      };
      requestAnimationFrame(tick);
    });
    if (mine !== run) return;
    arrive(view, leg.ids[1]);
  }
  spark.remove();
}

/** Lights the step's parts on the map, and runs the light along its links. */
export function light(view: MapView, step: BriefStep) {
  const root = world(view);
  if (!root) return;
  root.classList.add("briefing");
  stopLight();
  root.querySelector(".map-light")?.remove();
  // A part a hop reaches waits for the light; the rest of the step's parts light at once.
  const waiting = new Set(reduced() ? [] : step.hops.flatMap((hop) => ends(view, hop) ?? []));
  root.querySelectorAll<HTMLElement>(".plate, .actor").forEach((node) => {
    node.classList.remove("arrive");
    node.classList.toggle("lit", step.on.includes(node.dataset.id ?? "") && !waiting.has(node.dataset.id ?? ""));
  });
  if (step.hops.length) void runLight(view, step.hops);
}

/** Takes an order check's marks off the map: the parts that could be tapped, and their numbers. */
export function clearOrder(view: MapView) {
  view.slots.canvas.querySelectorAll(".pick, .picked, .wrong").forEach((node) => node.classList.remove("pick", "picked", "wrong"));
  view.slots.canvas.querySelectorAll(".ord").forEach((node) => node.remove());
}
