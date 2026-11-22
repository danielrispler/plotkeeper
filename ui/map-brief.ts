// The PR brief on the Map view (spec danielrispler/plotkeeper#334, ticket #339), as the prototype's treatment C plays it: opening a tray row plays
// a brief's steps one at a time under the map. A light runs the step's links at one speed and a part lights as it arrives; the camera frames the
// parts the brief lights; the panel keeps its tallest step's height and crossfades between steps; a figure for a change inside one part is drawn
// by `figureShapes` from the writer's labels; citations sit in quiet type, and the panel says when the writer saw only part of the diff and which
// items are not confirmed from the facts. "See the code" opens the drawer (`map-drawer.ts`) above it. Opening a brief returns the map to now.
// Esc (`brief` layer) closes it and keeps its step. `map-model.ts` holds the logic: `briefPlayer`, `briefCamera`, `figureShapes`.

import { failure, invoke, text } from "./dom.js";
import { currentMap, currentScrub, onDraw, registerLayer, type MapView } from "./map.js";
import { closeDrawer, openDrawer } from "./map-drawer.js";
import { scrubTo } from "./map-history.js";
import { ratifyActions } from "./map-ratify.js";
import { ratifyStep, statusWords } from "./ratify.js";
import { actorSize, briefCamera, briefPlayer, citeDrawer, figureShapes, figureSize, playlist, plateSize, type Rect, type Shape, unconfirmed } from "./map-model.js";
import type { BriefStep, BriefView, RatifyRow } from "./types.js";

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
  const list = playlist(playing.brief.brief, now.chapter);
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
  root.querySelector(".map-light")?.remove();
}

const centre = (view: MapView, id: string) => view.positions[id];
const sizeOf = (view: MapView, id: string) => (view.doc.actors.some((actor) => actor.id === id) ? actorSize : plateSize);
const ends = (view: MapView, [link, backwards]: [string, boolean]): [string, string] | null => {
  const found = view.doc.links.find((l) => l.id === link);
  return found ? (backwards ? [found.to, found.from] : [found.from, found.to]) : null;
};

// What a brief frames: the parts its steps light and the ones their hops reach, with room around them.
function frameOf(view: MapView, list: BriefStep[]): Rect | null {
  const ids = new Set(list.flatMap((step) => [...step.on, ...step.hops.flatMap((hop) => ends(view, hop) ?? [])]));
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
async function runLight(view: MapView, step: BriefStep) {
  const mine = ++run;
  const root = world(view);
  const links = root?.querySelector(".map-links");
  if (!root || !links) return;
  root.querySelector(".map-light")?.remove();
  const layer = svg("svg", { class: "map-light", "aria-hidden": "true", width: links.getAttribute("width") ?? "0", height: links.getAttribute("height") ?? "0" });
  root.append(layer);
  const legs = step.hops.flatMap((hop) => {
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
  if (step.hops.length) void runLight(view, step);
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

// One step's words and figure. The panel around them stays put.
function stepNode(view: MapView, step: BriefStep, pr: number) {
  const part = (id: string) => view.doc.parts.find((p) => p.id === id)?.name ?? view.doc.actors.find((a) => a.id === id)?.name ?? id;
  const words = text("div", "", "brief-txt");
  const kick = text("div", "", "brief-kick");
  kick.append(text("span", `#${pr}`), text("span", step.k));
  const tag = lawTag(view, step);
  if (tag) kick.append(tag);
  words.append(kick, text("h3", step.h, "brief-title"), text("p", step.b, "brief-body"));
  if (step.list.length) words.append(listNode(step.list));
  if (step.risks.length) words.append(listNode(step.risks.map((r) => `${part(r.part)}: ${r.trigger} To see it: ${r.check}`)));
  if (step.covered_by) words.append(text("p", `Covered by the test ${step.covered_by}.`, "brief-quiet"));
  if (step.check) words.append(checkNode(step));
  if (step.k === "Ratify") words.append(ratifyNode(view, ratifying));
  if (unconfirmed(step)) {
    const mark = text("p", "not confirmed from the facts", "brief-quiet brief-unconfirmed");
    mark.title = step.flags.join("\n");
    words.append(mark);
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

// A check on a case inside one part: pick an answer, take it back, press Check. An order check is answered on the map in a later ticket and
// shows its question here.
function checkNode(step: BriefStep) {
  const check = step.check!;
  const box = text("div", "", "brief-check");
  box.append(text("p", check.q, "brief-body"));
  if (check.kind !== "pick") return box;
  const row = text("div", "", "brief-opts");
  const buttons = check.opts.map((opt, i) => {
    const button = text("button", opt, "btn brief-opt");
    button.title = "Pick this answer; press it again to take it back";
    button.setAttribute("type", "button");
    button.addEventListener("click", () => {
      if (box.dataset.answered) return;
      const was = button.getAttribute("aria-pressed") === "true";
      buttons.forEach((b) => b.setAttribute("aria-pressed", "false"));
      button.setAttribute("aria-pressed", String(!was));
      box.dataset.picked = was ? "" : String(i);
      const press = document.getElementById("brief-next");
      if (press) (press as HTMLButtonElement).disabled = was;
    });
    return button;
  });
  row.append(...buttons);
  box.append(row);
  return box;
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
  const fill = () => cols.replaceChildren(stepNode(view, step, pr));
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
  const next = box.querySelector<HTMLButtonElement>("#brief-next")!;
  const last = now.step === list.length - 1;
  next.textContent = last ? "Done" : "Next";
  next.disabled = step.check?.kind === "pick";
  next.dataset.check = step.check?.kind === "pick" ? "pick" : "";
  if (step.check?.kind === "pick") next.textContent = "Check";
  light(view, step);
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
  for (const step of list) {
    cols.replaceChildren(stepNode(view, step, pr));
    tallest = Math.max(tallest, box.offsetHeight);
  }
  box.style.minHeight = `${tallest}px`;
}

function move(by: number) {
  const view = currentMap();
  if (!view || !player.current()) return;
  if (player.go(by, steps().length) === "done") return closeBrief();
  show(view, false);
}

// Next on a pick check grades it first: the right answer and the other ones, then the way on.
function advance() {
  const box = panel();
  const step = steps()[player.current()?.step ?? 0];
  const check = box.querySelector<HTMLElement>(".brief-check");
  if (step?.check?.kind === "pick" && check && !check.dataset.answered) {
    const picked = Number(check.dataset.picked);
    check.dataset.answered = "yes";
    const key = Number(step.check.key);
    check.querySelectorAll(".brief-opt").forEach((opt, i) => opt.classList.add(i === key ? "right" : "miss"));
    check.querySelector(".brief-body")!.textContent = picked === key ? step.check.yes : step.check.no;
    const next = box.querySelector<HTMLButtonElement>("#brief-next")!;
    next.disabled = false;
    next.textContent = player.current()!.step === steps().length - 1 ? "Done" : "Next";
    return;
  }
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
