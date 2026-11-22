// The PR brief on the Map view (spec danielrispler/plotkeeper#334, ticket #339), as the prototype's treatment C plays it: opening a tray row plays
// a brief's steps one at a time under the map. A light runs the step's links at one speed and a part lights as it arrives; the camera frames the
// parts the brief lights; the panel keeps its tallest step's height and crossfades between steps; a figure for a change inside one part is drawn
// by `figureShapes` from the writer's labels; citations sit in quiet type, and the panel says when the writer saw only part of the diff and which
// items are not confirmed from the facts. "See the code" opens the drawer (`map-drawer.ts`) above it. Opening a brief returns the map to now.
// A step's check (#340) is answered in place: a pick in the panel, an order by tapping parts on the map into numbered slots (`pickRun`, `orderRun`);
// Check grades it, a wrong answer shows the correction, and an order check replays the right path along the links.
// Esc (`brief` layer) closes it and keeps its step. `map-model.ts` holds the logic: `briefPlayer`, `briefCamera`, `figureShapes`.

import { failure, invoke, text } from "./dom.js";
import { currentMap, currentScrub, onDraw, registerLayer, type MapView } from "./map.js";
import { closeDrawer, openDrawer } from "./map-drawer.js";
import { scrubTo } from "./map-history.js";
import { ratifyActions } from "./map-ratify.js";
import { ratifyStep, statusWords } from "./ratify.js";
import { actorSize, alsoAt, briefCamera, briefPlayer, citeDrawer, figureShapes, figureSize, orderRun, type OrderRun, pickRun, type PickRun, playlist, plateSize, type Rect, type Shape, unconfirmed } from "./map-model.js";
import type { BriefCheck, BriefStep, BriefView, PrBrief, RatifyRow } from "./types.js";

const player = briefPlayer({ scrubbed: () => currentScrub() !== null, backToNow: () => void scrubTo(null).catch(failure("map_at")) });
// The brief being played, as `pr_brief` returned it.
let playing: BriefView | null = null;
// What the brief offers for accept or overturn (`ratify.rs`), read when it opens; a decision updates its row at once.
let ratifying: RatifyRow[] = [];
// What stopped a row from opening, shown in its place until the next try.
const problems = new Map<number, string>();
export const briefProblem = (pr: number) => problems.get(pr) ?? null;

const reduced = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
// The light's speed along a link, in world pixels a second.
const SPEED = 420;
const svgNS = "http://www.w3.org/2000/svg";
const svg = (tag: string, attrs: Record<string, string> = {}) => {
  const node = document.createElementNS(svgNS, tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  return node;
};

const panel = () => document.getElementById("map-brief")!;
export const briefOpen = () => player.current() !== null;

// Opens `chapter` of PR `number`'s brief: reads it (the shell rewrites a stale one first), returns the map to now and plays it.
export async function openBriefOf(project: string, number: number, chapter: number) {
  problems.delete(number);
  let got: BriefView | null;
  try {
    panel().dataset.waiting = String(number);
    got = await invoke("pr_brief", { project, number });
  } catch (e) {
    problems.set(number, String(e));
    got = null;
  }
  delete panel().dataset.waiting;
  const view = currentMap();
  if (!got || !view || view.project !== project) {
    if (!got && !problems.has(number)) problems.set(number, "the brief is not written yet");
    // The tray says why, in the row's place of its note, until the map is drawn again.
    const note = document.querySelector(`#map-tray .tray-row[data-pr="${number}"] .tray-note`);
    if (note && problems.has(number)) note.textContent = problems.get(number)!;
    return;
  }
  playing = got;
  ratifying = await invoke("pr_ratify", { project, number }).catch((e) => (failure("pr_ratify")(e), [] as RatifyRow[]));
  if (!resizing) {
    resizing = true;
    addEventListener("resize", onResize);
  }
  closeDrawer();
  player.open(project, number, chapter);
  show(view, false);
}

// Opens PR `number`'s brief once the Map view has drawn `project` (the rail's row opens the view and the brief together).
let pending: { project: string; number: number } | null = null;
export function openBriefWhenDrawn(project: string, number: number) {
  const view = currentMap();
  if (view?.project === project) return void openBriefOf(project, number, 0);
  pending = { project, number };
}
onDraw((view) => {
  if (!pending || pending.project !== view.project) return;
  const { project, number } = pending;
  pending = null;
  void openBriefOf(project, number, 0);
});

export function closeBrief() {
  player.close();
  playing = null;
  ratifying = [];
  stopLight();
  const view = currentMap();
  panel().hidden = true;
  panel().classList.remove("open");
  document.body.classList.remove("brief-open");
  if (view) unlight(view);
  closeDrawer();
}

const steps = (): BriefStep[] => {
  const now = player.current();
  if (!playing || !now) return [];
  const list = playlist(playing.brief.brief, now.chapter, now.overview);
  // The ratify step ends the last chapter (the only one of a single story).
  const ratify = now.chapter >= Math.max(0, playing.brief.brief.chapters.length - 1) ? ratifyStep(ratifying.filter((row) => row.status !== "ratified"), [...new Set(list.flatMap((step) => step.on))]) : null;
  return ratify ? [...list, ratify] : list;
};

// ---- the map: lit parts, the camera and the light.

const world = (view: MapView) => view.slots.canvas.firstElementChild as HTMLElement | null;
const plateOf = (view: MapView, id: string) => view.slots.canvas.querySelector<HTMLElement>(`[data-id="${CSS.escape(id)}"]`);

function unlight(view: MapView) {
  const root = world(view);
  if (!root) return;
  root.classList.remove("briefing");
  root.style.transform = "";
  view.slots.canvas.querySelectorAll(".lit, .arrive").forEach((node) => node.classList.remove("lit", "arrive"));
  clearOrder(view);
  root.querySelector(".map-light")?.remove();
}

const centre = (view: MapView, id: string) => view.positions[id];
const sizeOf = (view: MapView, id: string) => (view.doc.actors.some((actor) => actor.id === id) ? actorSize : plateSize);
const ends = (view: MapView, [link, backwards]: [string, boolean]): [string, string] | null => {
  const found = view.doc.links.find((l) => l.id === link);
  return found ? (backwards ? [found.to, found.from] : [found.from, found.to]) : null;
};

// The parts the playing chapter's behaviour crosses, which its frame holds even where no step names them.
const chapterParts = (): string[] => {
  const now = player.current();
  return playing && now ? (playing.brief.brief.chapters[now.chapter]?.parts ?? []) : [];
};

// What a brief frames: the parts its steps light and the ones their hops reach, with room around them.
function frameOf(view: MapView, list: BriefStep[]): Rect | null {
  const ids = new Set([...list.flatMap((step) => [...step.on, ...step.hops.flatMap((hop) => ends(view, hop) ?? [])]), ...chapterParts()]);
  const boxes = [...ids].filter((id) => centre(view, id)).map((id) => ({ at: centre(view, id), size: sizeOf(view, id) }));
  if (!boxes.length) return null;
  return {
    x0: Math.min(...boxes.map((b) => b.at.x - b.size.w / 2)) - 70,
    y0: Math.min(...boxes.map((b) => b.at.y - b.size.h / 2)) - 56,
    x1: Math.max(...boxes.map((b) => b.at.x + b.size.w / 2)) + 70,
    y1: Math.max(...boxes.map((b) => b.at.y + b.size.h / 2)) + 70,
  };
}

function aim(view: MapView, step: BriefStep) {
  const root = world(view);
  if (!root) return;
  const canvas = view.slots.canvas;
  canvas.scrollLeft = canvas.scrollTop = 0;
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
  if (!camera) return;
  root.style.transformOrigin = "0 0";
  root.style.transform = `translate(${camera.x}px, ${camera.y}px) scale(${camera.scale})`;
}

let run = 0;
const stopLight = () => void run++;

function arrive(view: MapView, id: string) {
  const node = plateOf(view, id);
  if (!node) return;
  node.classList.add("lit");
  node.classList.remove("arrive");
  void node.offsetWidth;
  node.classList.add("arrive");
}

// The light travels the step's links at one speed, leaving each lit behind it; a part lights as the light reaches it.
async function runLight(view: MapView, hops: [string, boolean][]) {
  const mine = ++run;
  const root = world(view);
  const links = root?.querySelector(".map-links");
  if (!root || !links) return;
  root.querySelector(".map-light")?.remove();
  const layer = svg("svg", { class: "map-light", "aria-hidden": "true", width: links.getAttribute("width") ?? "0", height: links.getAttribute("height") ?? "0" });
  root.append(layer);
  const legs = hops.flatMap((hop) => {
    const line = links.querySelector(`[data-link="${CSS.escape(hop[0])}"]`);
    const pair = ends(view, hop);
    if (!line || !pair) return [];
    const read = (name: string) => Number(line.getAttribute(name));
    const [p, q] = [{ x: read("x1"), y: read("y1") }, { x: read("x2"), y: read("y2") }];
    // The rim points belong to the link as drawn from -> to; a reversed hop runs them the other way.
    const [from, to] = hop[1] ? [q, p] : [p, q];
    return [{ from, to, ids: pair, len: Math.hypot(to.x - from.x, to.y - from.y) }];
  });
  const lit = (leg: (typeof legs)[number]) => svg("line", { class: "light-trail", x1: String(leg.from.x), y1: String(leg.from.y), x2: String(leg.to.x), y2: String(leg.to.y) });
  if (!legs.length) return;
  if (reduced()) {
    for (const leg of legs) {
      layer.append(lit(leg));
      leg.ids.forEach((id) => plateOf(view, id)?.classList.add("lit"));
    }
    return;
  }
  const spark = svg("circle", { class: "light-spark", r: "5" });
  layer.append(spark);
  arrive(view, legs[0].ids[0]);
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

function light(view: MapView, step: BriefStep) {
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

// ---- the panel.

function figureNode(shapes: Shape[]) {
  const box = svg("svg", { viewBox: `0 0 ${figureSize.w} ${figureSize.h}`, class: "brief-fig", role: "img" });
  for (const s of shapes) {
    if (s.kind === "frame") {
      box.append(svg("rect", { class: "fig-frame", x: String(s.x), y: String(s.y), width: String(s.w), height: String(s.h), rx: "12" }));
      if (s.text) box.append(Object.assign(svg("text", { class: "fig-title", x: String(s.x + 12), y: String(s.y + 20) }), { textContent: s.text }));
    } else if (s.kind === "chip") {
      box.append(svg("rect", { class: `fig-chip ${s.tone}`, x: String(s.x), y: String(s.y), width: String(s.w), height: String(s.h), rx: String(s.h / 2) }));
      box.append(Object.assign(svg("text", { class: `fig-text ${s.tone}`, x: String(s.x + s.w / 2), y: String(s.y + s.h / 2 + 4), "text-anchor": "middle" }), { textContent: s.text }));
    } else if (s.kind === "arrow") {
      box.append(svg("line", { class: "fig-arrow", x1: String(s.x), y1: String(s.y), x2: String(s.x + s.w), y2: String(s.y + s.h) }));
    } else {
      box.append(Object.assign(svg("text", { class: "fig-title", x: String(s.x), y: String(s.y + 14) }), { textContent: s.text }));
    }
  }
  return box;
}

const lawTag = (view: MapView, step: BriefStep) => {
  const law = step.law ? view.doc.laws.find((l) => l.id === step.law) : undefined;
  return law ? text("span", law.backing === "check" ? "law · backed by a check" : "law · on trust", "brief-tag") : null;
};

const partName = (view: MapView, id: string) => view.doc.parts.find((p) => p.id === id)?.name ?? view.doc.actors.find((a) => a.id === id)?.name ?? id;

// One step's words and figure. The panel around them stays put.
function stepNode(view: MapView, step: BriefStep, pr: number, also: PrBrief["also"] = []) {
  const part = (id: string) => partName(view, id);
  const words = text("div", "", "brief-txt");
  const kick = text("div", "", "brief-kick");
  kick.append(text("span", `#${pr}`), text("span", step.k));
  const tag = lawTag(view, step);
  if (tag) kick.append(tag);
  words.append(kick, text("h3", step.h, "brief-title"), text("p", step.b, "brief-body"));
  if (step.list.length) words.append(listNode(step.list));
  if (step.risks.length) words.append(listNode(step.risks.map((r) => `${part(r.part)}: ${r.trigger} To see it: ${r.check}`)));
  if (step.covered_by) words.append(text("p", `Covered by the test ${step.covered_by}.`, "brief-quiet"));
  if (step.check) words.append(checkNode(view, step));
  if (step.k === "Ratify") words.append(ratifyNode(view, ratifying));
  if (unconfirmed(step)) {
    const mark = text("p", "not confirmed from the facts", "brief-quiet brief-unconfirmed");
    mark.title = step.flags.join("\n");
    words.append(mark);
  }
  if (also.length) {
    const lines = text("div", "", "brief-also");
    lines.append(text("p", "Also touched", "brief-quiet"));
    for (const item of also) lines.append(text("p", `${partName(view, item.part)}: ${item.line}${item.flags.length ? " (not confirmed from the facts)" : ""}`, "brief-quiet brief-also-line"));
    words.append(lines);
  }
  if (step.cites.length) {
    const cites = text("details", "", "brief-cites");
    const summary = text("summary", `What this rests on (${step.cites.length})`);
    summary.title = "Show what the writer quoted for this step";
    cites.append(summary);
    for (const cite of step.cites) cites.append(text("p", `${cite.from}: “${cite.quote}”`, "brief-cite"));
    words.append(cites);
  }
  const cols = text("div", "", "brief-step");
  cols.append(words);
  const shapes = step.fig ? figureShapes(step.fig) : [];
  if (shapes.length) {
    const figure = text("div", "", "brief-figure");
    figure.append(figureNode(shapes));
    cols.append(figure);
  }
  return cols;
}

// The items to ratify, each with where it stands; a not yet ratified one has Accept and Overturn.
function ratifyNode(view: MapView, rows: RatifyRow[]) {
  const list = text("ul", "", "brief-ratify");
  for (const row of rows.filter((each) => each.status !== "ratified")) {
    const item = text("li", "", "ratify-row");
    item.dataset.kind = row.kind;
    item.dataset.id = row.id;
    const kind = row.kind === "decision" ? "Decided" : `${row.change ? `${row.change[0].toUpperCase()}${row.change.slice(1)} ` : ""}${row.kind}`;
    item.append(text("span", kind, "ratify-kind"), text("span", row.text || row.id, "ratify-text"));
    if (row.status === "not-yet") {
      item.append(ratifyActions(view.project, row, (state) => {
        row.status = state;
        const now = currentMap();
        if (now) show(now, true);
      }));
    } else {
      item.append(text("span", statusWords[row.status], "ratify-mark done"));
    }
    list.append(item);
  }
  return list;
}

function listNode(items: string[]) {
  const list = text("ul", "", "brief-pts");
  list.append(...items.map((item) => text("li", item)));
  return list;
}

// ---- the check: one prediction, answered in the panel (a pick) or on the map (an order).

type Run = { kind: "pick"; run: PickRun } | { kind: "order"; run: OrderRun };
// A check keeps its answer while the brief is open, so a redraw of the map or a step back shows it as it was.
const runs = new WeakMap<BriefCheck, Run>();
function runOf(view: MapView, check: BriefCheck): Run | null {
  let found = runs.get(check);
  if (!found && check.kind === "pick") found = { kind: "pick", run: pickRun(Number(check.key)) };
  if (!found && check.kind === "order") found = { kind: "order", run: orderRun(Array.isArray(check.key) ? (check.key as string[]) : [], view.doc.links) };
  if (found) runs.set(check, found);
  return found ?? null;
}
const nameOf = (view: MapView, id: string) => view.doc.parts.find((p) => p.id === id)?.name ?? id;
const keyOf = (check: BriefCheck) => (Array.isArray(check.key) ? (check.key as string[]) : []);

// What an order check lets Daniel tap: the parts the step lights, and the ones of the key.
const tappable = (view: MapView, step: BriefStep) => new Set([...step.on, ...keyOf(step.check!)].filter((id) => view.doc.parts.some((p) => p.id === id)));

// The panel's part of a check, drawn from its run: the question (the answer's words once graded), the options or the numbered slots.
function checkNode(view: MapView, step: BriefStep) {
  const check = step.check!;
  const box = text("div", "", "brief-check");
  const found = runOf(view, check);
  if (!found) return box.appendChild(text("p", check.q, "brief-body")), box;
  const done = found.run.answered();
  const right = found.kind === "pick" ? found.run.picked() === Number(check.key) : keyOf(check).every((id, i) => id === found.run.picked()[i]);
  // The question is the step's own heading when the writer put it there; then only the answer's words add to it.
  if (done || check.q !== step.h) box.append(text("p", done ? (right ? check.yes : check.no) : check.q, "brief-body"));
  if (found.kind === "pick") {
    const { run } = found;
    const row = text("div", "", "brief-opts");
    check.opts.forEach((opt, i) => {
      const button = text("button", opt, "btn brief-opt");
      button.title = "Pick this answer; press it again to take it back";
      button.setAttribute("type", "button");
      button.setAttribute("aria-pressed", String(run.picked() === i));
      if (done) button.classList.add(i === Number(check.key) ? "right" : "miss");
      button.addEventListener("click", () => {
        run.tap(i);
        redrawCheck(view, step);
      });
      row.append(button);
    });
    box.append(row);
  } else {
    // The slots show the parts as tapped; once graded they show the key, which the map replays.
    const shown = done ? keyOf(check) : found.run.picked();
    const row = text("div", "", "brief-slots");
    keyOf(check).forEach((_, i) => {
      const slot = text("span", "", `brief-slot${shown[i] ? " full" : ""}`);
      slot.append(text("b", String(i + 1)), text("span", shown[i] ? nameOf(view, shown[i]) : "tap a part"));
      row.append(slot);
    });
    box.append(row);
  }
  return box;
}

// Redraws the check where it is, the Check button and the map's badges with it.
function redrawCheck(view: MapView, step: BriefStep) {
  const old = panel().querySelector(".brief-check");
  if (old) old.replaceWith(checkNode(view, step));
  paintOrder(view, step);
  syncNext(step);
}

// The map's side of an order check: the parts that can be tapped, and a number on each part tapped.
function clearOrder(view: MapView) {
  view.slots.canvas.querySelectorAll(".pick, .picked, .wrong").forEach((node) => node.classList.remove("pick", "picked", "wrong"));
  view.slots.canvas.querySelectorAll(".ord").forEach((node) => node.remove());
}
function paintOrder(view: MapView, step: BriefStep) {
  clearOrder(view);
  const found = step.check?.kind === "order" ? runOf(view, step.check) : null;
  if (!found || found.kind !== "order") return;
  const open = tappable(view, step);
  const answered = found.run.answered();
  const shown = answered ? keyOf(step.check!) : found.run.picked();
  open.forEach((id) => plateOf(view, id)?.classList.toggle("pick", !answered));
  shown.forEach((id, i) => {
    const plate = plateOf(view, id);
    plate?.classList.add("picked");
    plate?.append(text("span", String(i + 1), "ord"));
  });
}

// A tap on a part while an order check is open picks it, instead of opening its sheet.
function wireTaps(view: MapView) {
  const canvas = view.slots.canvas;
  if (canvas.dataset.briefTaps) return;
  canvas.dataset.briefTaps = "yes";
  canvas.addEventListener(
    "click",
    (event) => {
      const now = player.current();
      const step = now ? steps()[now.step] : undefined;
      const current = currentMap();
      if (!step?.check || step.check.kind !== "order" || !current) return;
      const plate = (event.target as HTMLElement).closest<HTMLElement>(".plate");
      if (!plate) return;
      event.stopPropagation();
      const found = runOf(current, step.check);
      if (!found || found.kind !== "order" || found.run.answered() || !tappable(current, step).has(plate.dataset.id ?? "")) return;
      found.run.tap(plate.dataset.id!);
      redrawCheck(current, step);
    },
    true,
  );
}

// Check on a check: graded, its right answer shown, and an order check's right path replayed along the links.
function grade(view: MapView, step: BriefStep) {
  const found = runOf(view, step.check!);
  if (!found || !found.run.ready()) return;
  if (found.kind === "pick") {
    found.run.grade();
    return redrawCheck(view, step);
  }
  const result = found.run.grade()!;
  const wrong = result.wrong;
  redrawCheck(view, step);
  // The parts out of place nudge once; then the right path runs.
  if (wrong.length && !reduced()) {
    wrong.forEach((id) => plateOf(view, id)?.classList.add("wrong"));
    setTimeout(() => replay(view, found.run), 420);
  } else {
    replay(view, found.run);
  }
}
function replay(view: MapView, run: OrderRun) {
  if (!player.current()) return;
  view.slots.canvas.querySelectorAll(".wrong").forEach((node) => node.classList.remove("wrong"));
  stopLight();
  world(view)?.querySelector(".map-light")?.remove();
  void runLight(view, run.replay());
}

// The Next button is Check while the step's check waits for its answer.
function syncNext(step: BriefStep) {
  const next = document.getElementById("brief-next") as HTMLButtonElement | null;
  const list = steps();
  if (!next) return;
  const found = step.check ? runs.get(step.check) : undefined;
  const waiting = !!step.check && (!found || !found.run.answered());
  const last = player.current()?.step === list.length - 1;
  next.textContent = waiting ? "Check" : last ? "Done" : "Next";
  next.dataset.check = waiting ? step.check!.kind : "";
  next.disabled = waiting && !(found && found.run.ready());
}

let swap = 0;

// Shows the current step: the map lit and framed, the panel's words crossfaded in.
function show(view: MapView, same: boolean) {
  const now = player.current();
  const list = steps();
  const step = list[now?.step ?? 0];
  const box = panel();
  if (!now || !step || !playing) return closeBrief();
  const pr = now.pr;
  document.body.classList.add("brief-open");
  if (!box.classList.contains("open") || !box.firstElementChild) buildPanel(view, list, pr);
  box.hidden = false;
  // A reflow first, so the panel rises from where it was hidden.
  void box.offsetWidth;
  box.classList.add("open");
  const cols = box.querySelector<HTMLElement>(".brief-cols")!;
  const also = alsoAt(playing.brief.brief, now.chapter, now.step, list.length);
  const fill = () => cols.replaceChildren(stepNode(view, step, pr, also));
  clearTimeout(swap);
  if (same || reduced()) {
    fill();
  } else {
    cols.classList.add("swap");
    swap = window.setTimeout(() => (fill(), cols.classList.remove("swap")), 130);
  }
  box.querySelectorAll(".brief-dots i").forEach((dot, i) => dot.classList.toggle("at", i === now.step));
  const code = box.querySelector<HTMLButtonElement>(".brief-code")!;
  code.hidden = step.cites.length === 0;
  (box.querySelector(".brief-back") as HTMLButtonElement).disabled = now.step === 0;
  if (step.check) runOf(view, step.check);
  syncNext(step);
  wireTaps(view);
  light(view, step);
  paintOrder(view, step);
  aim(view, step);
}

// The panel's frame, built once per brief: it keeps the height of its tallest step, so it never jumps.
function buildPanel(view: MapView, list: BriefStep[], pr: number) {
  const box = panel();
  box.style.minHeight = "";
  const leave = text("button", "Leave", "btn quiet brief-leave");
  leave.title = "Close the brief; it opens again at this step";
  leave.addEventListener("click", closeBrief);
  const cols = text("div", "", "brief-cols");
  const dots = text("div", "", "brief-dots");
  dots.append(...list.map(() => document.createElement("i")));
  const partial = playing?.partial ? text("span", "Written from part of the diff", "brief-partial") : text("span", "", "brief-partial");
  const code = text("button", "See the code", "btn quiet brief-code");
  code.title = "Open what this step quotes, above the brief";
  code.addEventListener("click", () => {
    const step = steps()[player.current()?.step ?? 0];
    if (step) openDrawer(citeDrawer(step));
  });
  const back = text("button", "Back", "btn brief-back");
  back.title = "The step before";
  back.addEventListener("click", () => move(-1));
  const next = text("button", "Next", "btn primary brief-next");
  next.id = "brief-next";
  next.title = "The next step";
  next.addEventListener("click", () => advance());
  const foot = text("div", "", "brief-foot");
  foot.append(dots, partial, code, back, next);
  box.replaceChildren(leave, cols, foot);
  // One height for the whole brief: its tallest step's.
  box.hidden = false;
  let tallest = 0;
  const now = player.current();
  list.forEach((step, i) => {
    cols.replaceChildren(stepNode(view, step, pr, playing && now ? alsoAt(playing.brief.brief, now.chapter, i, list.length) : []));
    tallest = Math.max(tallest, box.offsetHeight);
  });
  box.style.minHeight = `${tallest}px`;
}

function move(by: number) {
  const view = currentMap();
  if (!view || !player.current()) return;
  if (player.go(by, steps().length) === "done") return closeBrief();
  show(view, false);
}

// Next on a check grades it first: the right answer and the other ones, then the way on.
function advance() {
  const view = currentMap();
  const step = steps()[player.current()?.step ?? 0];
  const found = view && step?.check ? runs.get(step.check) : undefined;
  if (view && step?.check && found && !found.run.answered()) return grade(view, step);
  move(1);
}

// A redraw of the map makes new plates: the brief lights them again, where it was. Another project's map closes it.
onDraw((view) => {
  const now = player.current();
  if (!now) return;
  if (now.project !== view.project) return closeBrief();
  show(view, true);
});
// The camera frames again when the window is resized (wired when the first brief opens, so the module loads anywhere).
let resizing = false;
const onResize = () => {
  const view = currentMap();
  if (view && player.current()) show(view, true);
};

registerLayer("brief", { open: briefOpen, close: closeBrief });
