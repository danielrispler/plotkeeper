// The PR brief on the Map view (spec danielrispler/plotkeeper#334, ticket #339), as the prototype's treatment C plays it: opening a tray row plays
// a brief's steps one at a time under the map. A light runs the step's links at one speed and a part lights as it arrives; the camera frames the
// parts the brief lights; the panel is one narrow width, as tall as the step it shows (its CSS scrolls a taller step inside it, #449), and
// crossfades between steps; a figure for a change inside one part is drawn under the step's words by `figureShapes` from the writer's labels;
// citations sit in quiet type, a "Partial facts · N" chip lists what the brief was written without (#491), and an item not confirmed
// from the facts carries a mark whose popover lists why. A step's "See the code" opens its parts' files in the drawer (`map-drawer.ts`) above it. Opening a brief returns the map to now.
// A quiet copy icon (#493) on a Could break step, in the not-confirmed popover and on the No part group copies a prompt for the agent.
// A step's check (#340) is answered in place: a pick in the panel, an order by tapping parts on the map into numbered slots (`pickRun`, `orderRun`);
// Check grades it, a wrong answer shows the correction, and an order check replays the right path along the links.
// Esc (`brief` layer) closes it and keeps its step; an open not-confirmed popover (`popover` layer) closes first, and a new step closes it. `map-model.ts` holds the logic: `briefPlayer`, `briefCamera`, `figureShapes`.
// A part catch-up (#391) plays in the same reader (`openCatchUpOf`): one story of a fogged part, under a negative number of its own (one per
// fog list, `catchUpIds`) so the player keeps its step apart from the PR briefs and from an older story of the part; read to its end it
// clears the part's fog (`part_catch_up_done`) and completes no brief.

import { failure, invoke, text } from "./dom.js";
import { currentMap, currentScrub, giveCameraBack, lendCamera, onDraw, registerLayer, type MapView } from "./map.js";
import { closeDrawer, openDrawer } from "./map-drawer.js";
import { watchSeen } from "./seen.js";
import { logStop } from "./stop-rules.js";
import { expectedLine } from "./map-pre-flight.js";
import { exploreRow } from "./micro-world.js";
import { guardRow } from "./patch-review.js";
import { scrubTo } from "./map-history.js";
import { branchOf, overturnNote, ratifyActions, ratifyDetail, ratifyEvidence } from "./map-ratify.js";
import { copyOverturn, ratifyStep, statusWords } from "./ratify.js";
import type { Copied } from "./notes.js";
import { type Bar, explainsBack, storyBar } from "./story-bar.js";
import { actorSize, agentPrompt, alsoAt, diffArgs, type AlsoRow, effectiveTier, awaitedCatchUp, catchUpIds, awaitedWrite, briefCamera, briefPlayer, chapterGate, type Gate, citeDrawer, commitDrawer, figureMax, figureShapes, figureSize, fogListLines, latestRequest, orderRun, type OrderRun, partialFactsLines, pickRun, type PickRun, playlist, plateSize, stageLabel, storySteps, type Rect, type Shape, stepCode, unconfirmed, unconfirmedReasons } from "./map-model.js";
import type { BriefCheck, BriefFigure, BriefStep, BriefView, CatchUpOffer, CommitFiles, PrBrief, RatifyRow, ReadyPr, StaleBase, StoredCatchUp } from "./types.js";
import { BRIEF, explained, explainLine, explanationsOn, explainToggle, onExplanations, pressToggle, RATIFY, stepLine } from "./explanations.js";

const player = briefPlayer({ scrubbed: () => currentScrub() !== null, backToNow: () => void scrubTo(null).catch(failure("map_at")) });
// The brief being played, as `pr_brief` returned it.
let playing: BriefView | null = null;
// The part catch-up being played: its part and the fog list it covers; null while a PR brief plays.
let catchUp: { part: string; fog: string[]; summary?: StoredCatchUp["summary"] } | null = null;
// Each part's catch-up plays under a negative number of its own, which no PR has.
const catchUpId = catchUpIds();
// A stop event's PR: none for a part catch-up.
const prOf = (pr: number) => (pr < 0 ? null : pr);
// What the brief offers for accept or overturn (`ratify.rs`), read when it opens; a decision updates its row at once.
let ratifying: RatifyRow[] = [];
// What stopped a row from opening, shown in its place until the next try.
const problems = new Map<number, string>();
// The row's problem, until its brief arrives or is being written again.
export function briefProblem(pr: ReadyPr): string | null {
  if (pr.brief?.writing || awaitedWrite(pr) === "open") problems.delete(pr.number);
  return problems.get(pr.number) ?? null;
}

const reduced = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
// The light's speed along a link, in world pixels a second.
const SPEED = 420;
const svgNS = "http://www.w3.org/2000/svg";
const svg = (tag: string, attrs: Record<string, string> = {}) => {
  const node = document.createElementNS(svgNS, tag);
  for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
  return node;
};

// Reading a chapter to its end (#342): one gate per chapter of a brief, kept while the window runs; it tells the shell once.
const gates = new Map<string, Gate>();
// The chapters of each "Since you read" story read to their end while the window runs (#497).
const storyDone = new Map<string, Set<number>>();
let watching: (() => void) | null = null;
// The "not confirmed from the facts" marks shown, watched until seen or the brief closes (#346).
let marks: (() => void)[] = [];
const stopWatching = () => (watching?.(), (watching = null));
// The not-confirmed popover (#491): one manual popover of the panel, so Esc reaches the map's layers, which close it before the brief. It
// shows the reasons of the mark that opened it, anchored to that mark (`showPopover`'s source); a redraw of the same step anchors it to the
// mark's new copy, and a new step closes it.
let why: HTMLElement | null = null;
// The open popover's mark, by its key (`step`, or `decision:<id>`).
let whyOf: string | null = null;
const popoverOpen = () => !!why?.isConnected && why.matches(":popover-open");
const markOf = (key: string) => panel().querySelector<HTMLElement>(`.brief-why-mark[data-why="${CSS.escape(key)}"]`);
function closePopover() {
  if (popoverOpen()) why!.hidePopover();
  if (whyOf) markOf(whyOf)?.setAttribute("aria-expanded", "false");
  whyOf = null;
}
function openPopover(mark: HTMLElement, reasons: string[], copy: Prompt | null) {
  if (!why) {
    why = text("div", "", "brief-why");
    why.id = "brief-why";
    why.setAttribute("popover", "manual");
  }
  if (why.parentElement !== panel()) panel().append(why);
  const list = reasons.length ? text("ul", "", "brief-why-list") : text("p", "The checks gave no reason.", "brief-why-none");
  list.append(...reasons.map((reason) => text("li", reason)));
  why.replaceChildren(text("p", "Why it is not confirmed", "brief-why-title"), list);
  if (copy) {
    const row = text("div", "", "brief-why-copy");
    row.append(copyButton("Copy for your agent: why it is not confirmed", "Copy a prompt for your agent: the item and why it is not confirmed", copy));
    why.append(row);
  }
  if (popoverOpen()) why.hidePopover();
  why.showPopover({ source: mark });
  whyOf = mark.dataset.why ?? null;
  mark.setAttribute("aria-expanded", "true");
}
// After a redraw of the same step: the open popover follows its mark's new copy, or closes when the mark is gone.
function reanchorPopover() {
  if (!whyOf || !popoverOpen()) return;
  const mark = markOf(whyOf);
  if (!mark) return closePopover();
  why!.hidePopover();
  why!.showPopover({ source: mark });
  mark.setAttribute("aria-expanded", "true");
}
// The "not confirmed from the facts" mark of a step or a flagged decision: a button whose popover lists the item's reasons, the mark's
// own words left out. Seeing the mark counts for the stop rules (#346).
function unconfirmedMark(view: MapView, pr: number, flags: string[], key: string, copy: Prompt | null): HTMLElement {
  const wrap = text("p", "", "brief-quiet brief-unconfirmed");
  const mark = text("button", "not confirmed from the facts", "btn quiet brief-why-mark");
  mark.setAttribute("type", "button");
  mark.title = "Show why the checks could not confirm this";
  mark.dataset.why = key;
  mark.setAttribute("aria-controls", "brief-why");
  mark.setAttribute("aria-expanded", String(whyOf === key && popoverOpen()));
  mark.addEventListener("click", () => (whyOf === key && popoverOpen() ? closePopover() : openPopover(mark, unconfirmedReasons(flags), copy)));
  marks.push(watchSeen(mark, { active: () => mark.isConnected && panel().classList.contains("open"), onSeen: () => logStop("unconfirmed-seen", view.project, prOf(pr)) }));
  wrap.append(mark);
  return wrap;
}
// Copy for the agent (#493): a quiet copy icon puts a prompt that stands alone for any agent on the clipboard (`agentPrompt`), through
// Overturn's path (`copyOverturn`), and the copy toast shows it, or holds it to copy by hand when the clipboard refuses: the reader says so
// with a `copy-for-agent` event, which `main.ts` hands to the toast, so the reader loads none of the shell's modules. A prompt is a
// string, or waits for the changed files of a step's lit parts; those are asked for when the step is drawn, so a click usually has them
// and the clipboard is written inside it.
type Prompt = () => string | Promise<string>;
const COPY_ICON = '<svg viewBox="0 0 14 14" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="4.5" y="4.5" width="7" height="7" rx="1.5"/><path d="M9.5 2.5h-6a1 1 0 0 0-1 1v6"/></svg>';
function copyButton(label: string, hover: string, prompt: Prompt): HTMLElement {
  const button = text("button", "", "btn quiet brief-copy");
  button.setAttribute("type", "button");
  button.setAttribute("aria-label", label);
  button.title = hover;
  button.innerHTML = COPY_ICON;
  button.addEventListener("click", () => {
    const write = navigator.clipboard?.writeText?.bind(navigator.clipboard);
    const copy = (made: string) =>
      void copyOverturn(made, write).then((outcome) => window.dispatchEvent(new CustomEvent<Copied>("copy-for-agent", { detail: { outcome, prompt: made } })));
    const made = prompt();
    if (typeof made === "string") copy(made);
    else void made.then(copy);
  });
  return button;
}
// The changed files of the parts `step` lights (actors left out), asked of `pr_diff` once per brief and parts; none when git cannot give them.
type Files = { done: string[] | null; wait: Promise<string[]> };
const litFiles = new Map<string, Files>();
function filesOf(view: MapView, step: BriefStep): Files {
  const parts = step.on.filter((id) => view.doc.parts.some((part) => part.id === id));
  if (!playing || !parts.length) return { done: [], wait: Promise.resolve([]) };
  const { pr, head } = playing.brief;
  // A since story shares its full brief's head, but not its diff.
  const key = `${view.project}|${pr}|${playing.since ?? ""}|${head}|${parts.join(",")}`;
  let found = litFiles.get(key);
  if (!found) {
    const files: Files = { done: null, wait: invoke("pr_diff", { ...diffArgs(view.project, playing), parts }).then((got) => got.files.map((file) => file.path), () => []) };
    void files.wait.then((paths) => (files.done = paths));
    litFiles.set(key, (found = files));
  }
  return found;
}
// A step's prompt, with its lit parts' files: at once when they have come, else once they come.
function stepPrompt(view: MapView, step: BriefStep, pr: number): Prompt {
  const files = filesOf(view, step);
  const make = (paths: string[]) => agentPrompt(pr, branchOf(pr), { of: "step", step, files: paths });
  return () => (files.done ? make(files.done) : files.wait.then(make));
}

// The end-of-story bar of each chapter played since the brief opened (#393), kept so a redraw keeps what is typed; a reopened brief starts
// them empty.
const bars = new Map<string, Bar>();
function gateOf(): Gate | null {
  const now = player.current();
  if (!now || !playing) return null;
  const story = `${now.project}|${now.pr}|${playing.brief.head}|${now.story}`;
  const key = `${story}|${now.chapter}`;
  let gate = gates.get(key);
  if (!gate) {
    const { project, pr, chapter } = now;
    const head = playing.brief.head;
    const caught = catchUp;
    const chapters = playing.since ? playing.brief.brief.chapters.length : 0;
    gate = chapterGate(storySteps(steps()), () => {
      if (chapters) {
        // A "Since you read" story (#497) read to its end, every chapter of it: the read carries over to its head. Its chapters are not
        // the full brief's, so none is marked read on its own.
        const done = storyDone.get(story) ?? new Set<number>();
        storyDone.set(story, done.add(chapter));
        if (done.size < chapters) return;
        logStop("chapter-done", project, pr);
        void invoke("brief_since_done", { project, number: pr, head }).catch(failure("brief_since_done"));
        return;
      }
      if (caught) {
        logStop("catch-up-done", project);
        void invoke("part_catch_up_done", { project, part: caught.part, fog: caught.fog }).catch(failure("part_catch_up_done"));
        return;
      }
      logStop("chapter-done", project, pr);
      void invoke("brief_chapter_done", { project, number: pr, chapter, head }).catch(failure("brief_chapter_done"));
    });
    gates.set(key, gate);
  }
  return gate;
}
// Watches the last step's words: once they have been seen, the chapter's reading counts.
function watchLast(node: Element, index: number) {
  stopWatching();
  const gate = gateOf();
  if (!gate || gate.done() || index !== storySteps(steps()).length - 1) return;
  watching = watchSeen(node, { active: () => player.current()?.step === index && panel().classList.contains("open"), onSeen: () => gate.sawLast() });
}

const panel = () => document.getElementById("map-brief")!;
export const briefOpen = () => player.current() !== null;

// Opens `chapter` of PR `number`'s brief: reads it, returns the map to now and plays it. A stale or unwritten brief is written in the
// background (port decision Q5): the panel and the PR's tray row say "Writing the brief…", the map stays usable, and the brief opens
// when it lands (`map-landed` draws the map again), unless another open or a close came since.
const latest = latestRequest();
let awaiting: { project: string; number: number; chapter: number; full: boolean; wanted: () => boolean } | null = null;
// `full` opens the full brief past the head's "Since you read" story (#497), which opens first otherwise.
export async function openBriefOf(project: string, number: number, chapter: number, full = false) {
  problems.delete(number);
  awaiting = null;
  awaitingCatchUp = null;
  const wanted = latest.begin();
  let got: BriefView | { writing: true } | null;
  // An answer that takes a moment shows "Writing the brief…" too.
  const slow = window.setTimeout(() => wanted() && showWriting(number), WRITING_AFTER_MS);
  try {
    panel().dataset.waiting = String(number);
    got = await invoke("pr_brief", { project, number, full });
  } catch (e) {
    // A superseded open's failure is not this PR's problem: only a wanted answer may set it.
    if (wanted()) problems.set(number, String(e));
    got = null;
  }
  clearTimeout(slow);
  // A later open, or a close, since: this one's answer is not wanted, and the panel's waiting mark is that one's.
  if (!wanted()) return;
  delete panel().dataset.waiting;
  if (got && "writing" in got) {
    awaiting = { project, number, chapter, full, wanted };
    showWriting(number);
    return;
  }
  if (!briefOpen()) hideWriting();
  const view = currentMap();
  if (!got || !view || view.project !== project) {
    if (!got && !problems.has(number)) problems.set(number, "the brief is not written yet");
    // The tray says why, in the row's place of its note, until the map is drawn again.
    const note = number === null ? null : document.querySelector(`#map-tray .tray-row[data-pr="${number}"] .tray-note`);
    if (note && problems.has(number)) note.textContent = problems.get(number)!;
    return;
  }
  const rows = await invoke("pr_ratify", { project, number }).catch((e) => (failure("pr_ratify")(e), [] as RatifyRow[]));
  if (!wanted()) return;
  // Another brief or chapter open: leave it (it keeps its step) so the panel is built afresh for this one.
  if (briefOpen()) closeBrief();
  playing = got;
  ratifying = rows;
  logStop("brief-opened", project, number);
  if (!resizing) {
    resizing = true;
    addEventListener("resize", onResize);
  }
  closeDrawer();
  player.open(project, number, chapter, got.since ?? "");
  show(view, false);
}

// Opens `part`'s catch-up: the stored one for its current fog, or, while it is written, the panel says which call of the write runs, the map
// stays usable, and it opens when it lands (`map-landed` draws the map again), unless another open or a close came since.
let awaitingCatchUp: { project: string; part: string; wanted: () => boolean } | null = null;
export async function openCatchUpOf(project: string, part: string) {
  awaiting = null;
  awaitingCatchUp = null;
  const wanted = latest.begin();
  // The card says which call of the write runs (#450), as the map's last state has it.
  const writing = () => stageOf(currentMap()?.state.catch_ups?.[part]);
  // A catch-up has no tray row for the wait to mark.
  const slow = window.setTimeout(() => wanted() && showWriting(null, writing(), CATCH_UP_WAIT), WRITING_AFTER_MS);
  let got: StoredCatchUp | { writing: true } | null;
  try {
    got = await invoke("part_catch_up", { project, part });
  } catch (e) {
    if (wanted()) failure("part_catch_up")(e);
    got = null;
  }
  clearTimeout(slow);
  if (!wanted()) return;
  if (got && "writing" in got) {
    awaitingCatchUp = { project, part, wanted };
    showWriting(null, writing(), CATCH_UP_WAIT);
    return;
  }
  if (!briefOpen()) hideWriting();
  const view = currentMap();
  if (!got || !view || view.project !== project) return;
  if (briefOpen()) closeBrief();
  // A catch-up rewritten for a new fog list is another story, with a number of its own, so it starts at its first step.
  const id = catchUpId(project, part, got.fog);
  // The reader plays it as a single story; there is nothing to ratify.
  playing = { brief: { version: got.version, pr: id, head: got.fog.join(","), written_at: got.written_at, stale: false, model: got.model, brief: got.brief }, partial: got.partial };
  catchUp = { part, fog: got.fog, summary: got.summary };
  ratifying = [];
  logStop("catch-up-opened", project);
  if (view.state.stale?.[part]) logStop("stale-entered", project);
  if (!resizing) {
    resizing = true;
    addEventListener("resize", onResize);
  }
  closeDrawer();
  player.open(project, id, 0);
  show(view, false);
}
onDraw((view) => {
  if (!awaitingCatchUp || view.scrub) return;
  if (!awaitingCatchUp.wanted() || view.project !== awaitingCatchUp.project) {
    awaitingCatchUp = null;
    if (!briefOpen()) hideWriting();
    return;
  }
  const { project, part } = awaitingCatchUp;
  const next = awaitedCatchUp(view.state.catch_ups?.[part]);
  if (next === "wait") return showWriting(null, stageOf(view.state.catch_ups?.[part]), CATCH_UP_WAIT);
  awaitingCatchUp = null;
  if (next === "open") void openCatchUpOf(project, part);
  else if (!briefOpen()) hideWriting();
});

// The panel says so while a brief is written for an open: after this long, so a stored brief never flashes it.
const WRITING_AFTER_MS = 300;
const CATCH_UP_WAIT = "Writing the part catch-up";
const stageOf = (offer: CatchUpOffer | undefined) => stageLabel(offer?.kind === "catch-up" ? offer.stage : undefined);
// The waiting card (#450): at the story card's width, a named indeterminate progress bar and what the write is doing, in a polite live
// region so a screen reader hears each new stage. A new stage changes the words in place, so the region announces it.
function showWriting(number: number | null, words = "Writing the brief…", name = "Writing the brief") {
  if (briefOpen()) return;
  const box = panel();
  let stage = box.querySelector<HTMLElement>(".brief-writing-stage");
  if (!stage || box.querySelector("progress")?.getAttribute("aria-label") !== name) {
    const wait = document.createElement("div");
    wait.className = "brief-writing";
    wait.setAttribute("role", "status");
    wait.setAttribute("aria-live", "polite");
    const bar = document.createElement("progress");
    bar.className = "brief-wait";
    bar.setAttribute("aria-label", name);
    stage = text("p", words, "brief-writing-stage");
    wait.append(bar, stage);
    box.replaceChildren(wait);
  } else if (stage.textContent !== words) stage.textContent = words;
  box.classList.add("open");
  box.hidden = false;
  const note = number === null ? null : document.querySelector(`#map-tray .tray-row[data-pr="${number}"] .tray-note`);
  if (note) note.textContent = `#${number} · writing the brief…`;
}
function hideWriting() {
  const box = panel();
  if (box.querySelector(".brief-writing")) {
    box.replaceChildren();
    box.classList.remove("open");
    box.hidden = true;
  }
}

// Opens PR `number`'s brief once the Map view has drawn `project` (the rail's row opens the view and the brief together).
let pending: { project: string; number: number } | null = null;
export function openBriefWhenDrawn(project: string, number: number) {
  const view = currentMap();
  if (view?.project === project) return void openBriefOf(project, number, 0);
  pending = { project, number };
}
onDraw((view) => {
  if (!awaiting || view.scrub) return;
  if (!awaiting.wanted() || view.project !== awaiting.project) {
    awaiting = null;
    if (!briefOpen()) hideWriting();
    return;
  }
  const { project, number, chapter, full } = awaiting;
  const next = awaitedWrite((view.state.ready ?? []).find((pr) => pr.number === number), full);
  if (next === "wait") return;
  awaiting = null;
  if (next === "open") void openBriefOf(project, number, chapter, full);
  else if (!briefOpen()) hideWriting();
});
onDraw((view) => {
  if (!pending || pending.project !== view.project) return;
  const { project, number } = pending;
  pending = null;
  void openBriefOf(project, number, 0);
});

export function closeBrief() {
  latest.cancel();
  delete panel().dataset.waiting;
  hideWriting();
  stopWatching();
  marks.forEach((stop) => stop());
  marks = [];
  closePopover();
  litFiles.clear();
  player.close();
  playing = null;
  catchUp = null;
  ratifying = [];
  bars.clear();
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
  // The ratify step ends the last chapter (the only one of a single story), and holds Guard with a law's row when the brief offers it (#401).
  const ratify = now.chapter >= Math.max(0, playing.brief.brief.chapters.length - 1) ? ratifyStep(ratifying.filter((row) => row.status !== "ratified"), [...new Set(list.flatMap((step) => step.on))], !!playing.brief.guard) : null;
  return ratify ? [...list, ratify] : list;
};

// ---- the map: lit parts, the camera and the light.

const world = (view: MapView) => view.slots.canvas.firstElementChild as HTMLElement | null;
const plateOf = (view: MapView, id: string) => view.slots.canvas.querySelector<HTMLElement>(`[data-id="${CSS.escape(id)}"]`);

function unlight(view: MapView) {
  const root = world(view);
  if (!root) return;
  root.classList.remove("briefing");
  giveCameraBack();
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

// A figure's name for assistive tech (#456): its kind and the items it draws, the ones `figureShapes` draws (`figureMax`).
function figureLabel(fig: BriefFigure): string {
  const labels = (items: { label: string }[]) => items.map((item) => item.label);
  if (fig.kind === "states") return `States${fig.frame ? ` of ${fig.frame}` : ""}: ${labels(fig.items.slice(0, figureMax.states)).join(", ")}`;
  if (fig.kind === "sequence") return `Sequence: ${labels(fig.items.slice(0, figureMax.sequence)).join(", then ")}`;
  return `Before: ${fig.before.slice(0, figureMax.compare).join(", ")}. After: ${fig.after.slice(0, figureMax.compare).join(", ")}`;
}

function figureNode(shapes: Shape[], fig: BriefFigure) {
  const box = svg("svg", { viewBox: `0 0 ${figureSize.w} ${figureSize.h}`, class: "brief-fig", role: "img", "aria-label": figureLabel(fig) });
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

// A step's body (#446): the writer may wrap a phrase in <b>...</b>. Each paired span becomes a bold element and every other run plain
// text, so an unpaired tag shows as written. The body is never parsed as HTML.
function bodyNode(body: string) {
  const node = text("p", "", "brief-body");
  const runs = body.split(/(<\/?b>)/);
  for (let i = 0; i < runs.length; i++) {
    if (runs[i] === "<b>" && runs[i + 2] === "</b>") {
      node.append(text("b", runs[i + 1]!));
      i += 2;
    } else if (runs[i]) node.append(runs[i]!);
  }
  return node;
}

// One step's words and figure. The panel around them stays put.
function stepNode(view: MapView, step: BriefStep, pr: number, also: PrBrief["also"] = []) {
  const part = (id: string) => partName(view, id);
  const words = text("div", "", "brief-txt");
  const kick = text("div", "", "brief-kick");
  kick.append(text("span", pr < 0 ? "Part catch-up" : `#${pr}`), text("span", step.k));
  const tag = lawTag(view, step);
  if (tag) kick.append(tag);
  // Explanations (#524): a line under the title saying what this kind of step is for; on Ratify, what its title asks.
  const line = explanationsOn() ? stepLine(step) : undefined;
  words.append(kick, ...explained(text("h3", step.h, "brief-title"), line ?? "", line !== undefined), bodyNode(step.b));
  if (step.list.length) words.append(listNode(step.list));
  if (step.risks.length) words.append(listNode(step.risks.map((r) => `${part(r.part)}: ${r.trigger} To see it: ${r.check}`)));
  if (step.covered_by) words.append(text("p", `Covered by the test ${step.covered_by}.`, "brief-quiet"));
  if (step.check) words.append(checkNode(view, step));
  if (step.k === "Ratify") {
    words.append(ratifyNode(view, ratifying));
    const guard = playing?.brief.guard;
    if (guard && pr > 0 && !view.scrub) words.append(guardRow(view, pr, guard, () => {
      const now = currentMap();
      if (now) show(now, true);
    }));
  }
  if (unconfirmed(step)) words.append(unconfirmedMark(view, pr, step.flags, "step", pr > 0 ? stepPrompt(view, step, pr) : null));
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
  if (step.fig && shapes.length) {
    const figure = text("div", "", "brief-figure");
    figure.append(figureNode(shapes, step.fig));
    cols.append(figure);
  }
  return cols;
}

// "Explore this" under a PR brief's figure (#399): a micro-world of the parts its step lights, kept by the step's place in its chapter. A part
// catch-up's figure (no PR) and a step that is not its chapter's own offer none.
function figureExplore(view: MapView, node: HTMLElement, step: BriefStep, pr: number, chapter: number) {
  const figure = node.querySelector(".brief-figure");
  const at = playing?.brief.brief.chapters[chapter]?.steps.indexOf(step) ?? -1;
  if (!figure || pr <= 0 || at < 0 || view.scrub) return;
  const name = step.on.map((id) => partName(view, id)).join(", ") || `#${pr}`;
  figure.append(exploreRow(view.project, view.state.micro_worlds, `figure:${pr}:${chapter}:${at}`, name, { kind: "figure", pr, chapter, step: at }));
}

// A long part catch-up's first step says it summarises the main shifts, with the full fog list behind it (#396).
// A step's own "See the code" (#490), under its words: the files of the parts it lights; every file on the Ratify step, or on the last
// step when there is none; every file with a note on a step that lights no changed file. A part catch-up has no one PR's code to show.
function codeInto(view: MapView, node: HTMLElement, list: BriefStep[], index: number) {
  const step = list[index];
  const words = node.querySelector(".brief-txt");
  if (!playing || catchUp || !step || !words) return;
  const want = stepCode(list, index, new Set(view.doc.parts.map((part) => part.id)));
  const ask = diffArgs(view.project, playing);
  const diff = (parts?: string[]) => invoke("pr_diff", parts ? { ...ask, parts } : ask);
  const unlit = (found: CommitFiles) => {
    const drawer = commitDrawer(found);
    return { ...drawer, note: `This step lights no changed files · ${drawer.note}` };
  };
  const button = text("button", "See the code", "btn quiet brief-code");
  button.title = want === "all" ? "Open every file the PR changed, above the brief" : "Open the changed files of the parts this step lights, above the brief";
  button.addEventListener("click", () => {
    // The PR's diff (the files the writer read); when git cannot give it, what the step quoted.
    const drawer = want === "all" ? diff().then(commitDrawer) : want === "none" ? diff().then(unlit) : diff(want).then((found) => (found.files.length ? commitDrawer(found) : diff().then(unlit)));
    drawer.then(openDrawer).catch((e) => (failure("pr_diff")(e), openDrawer(citeDrawer(step))));
  });
  const row = text("div", "", "brief-step-code");
  row.append(button);
  // A Could break step's copy icon (#493), one per step.
  if (step.k === "Could break") row.append(copyButton("Copy for your agent: this risk", "Copy a prompt for your agent: this step, its risks and its parts' changed files", stepPrompt(view, step, playing.brief.pr)));
  words.append(row);
}

// Also touched by kind (#492), on the last step of the last chapter: each group its name and counts, its files, each with the laws its
// `@law:` tags name and the "ignored" tag where `alsoRows` keeps it, and a See the code of only its files.
function alsoInto(view: MapView, node: HTMLElement, groups: AlsoRow[]) {
  const words = node.querySelector(".brief-txt");
  if (!playing || catchUp || !groups.length || !words) return;
  const ask = diffArgs(view.project, playing);
  const box = text("section", "", "brief-also");
  box.setAttribute("aria-label", "Also touched");
  box.append(...explained(text("h4", "Also touched", "brief-quiet brief-also-title"), BRIEF.also, explanationsOn()));
  for (const group of groups) {
    const row = text("div", "", "brief-also-group");
    const head = text("div", "", "brief-also-head");
    head.append(text("h5", group.name, "brief-also-name"), text("span", group.counts, "brief-quiet brief-also-counts"));
    const button = text("button", "See the code", "btn quiet brief-code");
    button.title = `Open the ${group.name} files, above the brief`;
    button.setAttribute("aria-label", `See the code: ${group.name}`);
    button.addEventListener("click", () => {
      invoke("pr_diff", { ...ask, paths: group.paths }).then((found) => openDrawer(commitDrawer(found))).catch(failure("pr_diff"));
    });
    head.append(button);
    if (group.kind === "no-part") {
      const pr = playing.brief.pr;
      head.append(copyButton("Copy for your agent: assign these files to a part, or ignore them", "Copy a prompt for your agent: assign these files to a part, or ignore them", () => agentPrompt(pr, branchOf(pr), { of: "no-part", files: group.paths })));
    }
    row.append(head);
    const files = text("ul", "", "brief-also-files");
    files.setAttribute("role", "list");
    for (const file of group.files) {
      const item = text("li", file.path);
      if (file.laws.length) item.append(" ", text("span", `@law: ${file.laws.join(", ")}`, "brief-also-law"));
      if (file.ignored) item.append(" ", text("span", "ignored", "brief-tag"));
      files.append(item);
    }
    row.append(files);
    box.append(row);
  }
  words.append(box);
}

function summaryInto(node: HTMLElement) {
  const summary = catchUp?.summary;
  const words = node.querySelector(".brief-txt");
  if (!summary || !words) return;
  const list = text("details", "", "brief-cites brief-fog-list");
  const open = text("summary", `The full fog list (${summary.entries.length})`);
  open.title = "Show every PR and push this catch-up covers";
  list.append(open);
  for (const line of fogListLines(summary.entries)) list.append(text("p", line, "brief-cite"));
  words.append(text("p", summary.line, "brief-quiet brief-summarises"), list);
}

// The items to ratify, each with where it stands; a not yet ratified one has Accept and Overturn.
function ratifyNode(view: MapView, rows: RatifyRow[]) {
  const list = text("ul", "", "brief-ratify");
  // Safari drops a list's semantics under `list-style: none` and grid; the role brings them back.
  list.setAttribute("role", "list");
  // "Not yet ratified"'s line (#524), worded as on the sheet, under the first item still undecided only.
  let told = !explanationsOn();
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
    if (row.status === "not-yet" && !told) {
      told = true;
      item.append(explainLine(RATIFY.notYet, "explain-link"));
    }
    const evidence = ratifyEvidence(view.project, row);
    if (evidence) item.append(evidence);
    const detail = ratifyDetail(row);
    if (detail) item.append(detail);
    const decided = row.kind === "decision" ? playing?.brief.brief.decisions.find((each) => each.id === row.id) : undefined;
    if (decided && unconfirmed(decided)) {
      const copy = () => agentPrompt(row.pr, branchOf(row.pr), { of: "decision", text: decided.text, flags: decided.flags });
      item.append(unconfirmedMark(view, row.pr, decided.flags, `decision:${row.id}`, copy));
    }
    list.append(item);
  }
  const note = overturnNote(view.project, (item) => rows.some((row) => row.pr === item.pr && row.kind === item.kind && row.id === item.id));
  if (note) list.append(note);
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
  if (step.check && runs.get(step.check)?.run.answered()) gateOf()?.answered();
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

// A check was graded, right or wrong.
const answeredCheck = () => {
  const now = player.current();
  if (now) logStop("check-answered", now.project, prOf(now.pr));
};

// Check on a check: graded, its right answer shown, and an order check's right path replayed along the links.
function grade(view: MapView, step: BriefStep) {
  const found = runOf(view, step.check!);
  if (!found || !found.run.ready()) return;
  if (found.kind === "pick") {
    found.run.grade();
    answeredCheck();
    return redrawCheck(view, step);
  }
  const result = found.run.grade()!;
  answeredCheck();
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
  if (!box.classList.contains("open") || !box.firstElementChild) buildPanel(view, list);
  box.hidden = false;
  // A reflow first, so the panel rises from where it was hidden.
  void box.offsetWidth;
  box.classList.add("open");
  const cols = box.querySelector<HTMLElement>(".brief-cols")!;
  const also = alsoAt(playing.brief.brief, now.chapter, now.step, storySteps(list).length);
  // A new step closes the popover of the one before.
  if (!same) closePopover();
  const fill = () => {
    const node = stepNode(view, step, pr, also.lines);
    figureExplore(view, node, step, pr, now.chapter);
    codeInto(view, node, list, now.step);
    alsoInto(view, node, also.groups);
    if (now.step === 0) summaryInto(node);
    const bar = endBar(node, now.step, list.length);
    cols.replaceChildren(node);
    // A new step opens at its top, and the camera frames once the step's words set the card's height (#443 review).
    if (!same) cols.scrollTop = 0;
    bar?.offer();
    watchLast(node, now.step);
    reanchorPopover();
    aim(view, step);
  };
  clearTimeout(swap);
  if (same || reduced()) {
    fill();
  } else {
    cols.classList.add("swap");
    swap = window.setTimeout(() => (fill(), cols.classList.remove("swap")), 130);
  }
  box.querySelectorAll(".brief-dots i").forEach((dot, i) => dot.classList.toggle("at", i === now.step));
  box.querySelector(".brief-dots")!.setAttribute("aria-label", `Step ${now.step + 1} of ${list.length}`);
  (box.querySelector(".brief-back") as HTMLButtonElement).disabled = now.step === 0;
  if (step.check) runOf(view, step.check);
  syncNext(step);
  wireTaps(view);
  light(view, step);
  paintOrder(view, step);
}

// The end-of-story bar ends the played list's last step: a single story's end, or a chapter's (#393). It goes under the step's words and figure.
function endBar(node: HTMLElement, index: number, count: number): Bar | null {
  const now = player.current();
  if (!now || index !== count - 1) return null;
  const key = `${now.project}|${now.pr}|${now.story}|${now.chapter}`;
  let bar = bars.get(key);
  // A part catch-up's bar is keyed by its part (#391).
  const about = catchUp ? { project: now.project, part: catchUp.part } : { project: now.project, pr: now.pr, chapter: now.chapter };
  if (!bar) bars.set(key, (bar = storyBar(about, gistOf(now.chapter))));
  node.append(bar.node);
  return bar;
}

// The gist the bar's explain-back compares with (#397): the chapter's, or the part catch-up's one story's, when one of its parts is Core
// under Daniel's tiers now; none otherwise, so only a Core part's story offers explain-back.
function gistOf(chapter: number): string | undefined {
  const view = currentMap();
  const story = playing?.brief.brief.chapters[chapter];
  if (!view || !story) return undefined;
  const parts = catchUp ? [catchUp.part] : [...story.parts, ...story.steps.flatMap((step) => step.on)];
  const tierOf = (id: string) => {
    const part = view.doc.parts.find((each) => each.id === id);
    return part ? effectiveTier(part, view.state.user.tiers) : "";
  };
  return explainsBack(story.gist, parts, tierOf) ? story.gist : undefined;
}

// The panel's frame, built once per brief. Its height follows the step it shows, and Back and Next end it (#449).
function buildPanel(view: MapView, list: BriefStep[]) {
  const box = panel();
  const leave = text("button", "Leave", "btn quiet brief-leave");
  leave.title = "Close the brief; it opens again at this step";
  leave.addEventListener("click", closeBrief);
  // The explanations' `?` beside Leave (#524), as beside the sheet's ×; kept for the brief, its state set again when they switch.
  const header = text("div", "", "brief-header");
  header.append(explainToggle(explanationsOn()), leave);
  const cols = text("div", "", "brief-cols");
  const dots = text("div", "", "brief-dots");
  dots.setAttribute("role", "img");
  dots.append(...list.map(() => document.createElement("i")));
  // The brief's one live region (#456): Back and Next put the new step's title in it, and nothing else; the step itself is not live.
  const live = text("p", "", "visually-hidden brief-live");
  live.setAttribute("aria-live", "polite");
  const back = text("button", "Back", "btn brief-back");
  back.title = "The step before";
  back.addEventListener("click", () => move(-1));
  const next = text("button", "Next", "btn primary brief-next");
  next.id = "brief-next";
  next.title = "The next step";
  next.addEventListener("click", () => advance());
  const foot = text("div", "", "brief-foot");
  foot.append(dots, back, next);
  // A PR that closes the pre-flighted issue, or a ticket under it (#394): what Daniel expected against what it touched.
  const line = expectedLine(playing?.expected, view.doc);
  const facts = factsChip();
  const stale = staleBaseLine(playing?.stale_base);
  const since = sinceLine();
  box.replaceChildren(header, ...(since ? [since] : []), ...(facts ? [facts] : []), ...(stale ? [stale] : []), ...(line ? [text("p", line, "brief-expected")] : []), cols, foot, live);
}

// A "Since you read" story's head (#497): what it tells, from which head, and the full brief one click away, at the chapter played.
function sinceLine(): HTMLElement | null {
  const now = player.current();
  if (!playing?.since || !now) return null;
  const line = text("p", "Since you read ", "brief-since");
  const from = text("code", playing.since.slice(0, 7), "brief-since-head");
  from.setAttribute("translate", "no");
  line.append(from, ": only what this push changed. ");
  const full = text("button", "Full brief", "btn quiet brief-since-full");
  full.setAttribute("type", "button");
  full.setAttribute("aria-label", `Full brief of PR #${now.pr}`);
  full.title = "Open the whole PR's brief at this head";
  const { project, pr } = now;
  full.addEventListener("click", () => void openBriefOf(project, pr, 0, true));
  line.append(full);
  return line;
}

// The stale-base line (#494): what main changed since an open PR's merge base that the brief relies on, under the panel's head. The
// server sends it for open PRs only, and only when something moved. Each kind is named once, its names left untranslated.
function staleBaseLine(stale: StaleBase | undefined): HTMLElement | null {
  if (!stale) return null;
  const groups: [string, string, string[]][] = [
    ["law", "laws", stale.laws],
    ["link", "links", stale.links],
    ["flow", "flows", stale.flows],
    ["paths of", "paths of", stale.paths],
    ["file", "files", stale.files],
  ];
  const shown = groups.filter(([, , names]) => names.length);
  if (!shown.length) return null;
  const line = text("p", "Main moved since this PR's base: ", "brief-stale");
  shown.forEach(([one, many, names], n) => {
    if (n) line.append(" · ");
    line.append(`${names.length === 1 ? one : many} `);
    // A kind shows its first three names; the rest are counted.
    const list = text("span", names.slice(0, 3).join(", "), "brief-stale-names");
    list.setAttribute("translate", "no");
    line.append(list);
    if (names.length > 3) line.append(` +${names.length - 3} more`);
  });
  line.title = "The brief compares against the merge base; check these against main";
  return line;
}

// "Partial facts · N" (#491): what the brief was written without, one line per input, opening below the panel's head. Built with the
// panel, so it stays open across steps. None when nothing was missing.
function factsChip(): HTMLElement | null {
  const lines = partialFactsLines(playing?.brief.partial_facts, playing?.partial ?? false);
  if (!lines.length) return null;
  const chip = text("details", "", "brief-facts");
  const summary = text("summary", `Partial facts · ${lines.length}`);
  summary.title = "What this brief was written without";
  const list = text("ul", "", "brief-facts-list");
  for (const { line, files } of lines) {
    const item = text("li", line);
    if (files.length) {
      const paths = text("ul", "", "brief-facts-files");
      paths.setAttribute("translate", "no");
      paths.append(...files.map((path) => text("li", path)));
      item.append(paths);
    }
    list.append(item);
  }
  chip.append(summary, list);
  return chip;
}

function move(by: number) {
  const view = currentMap();
  if (!view || !player.current()) return;
  if (player.go(by, steps().length) === "done") return closeBrief();
  show(view, false);
  const live = panel().querySelector(".brief-live");
  if (live) live.textContent = steps()[player.current()?.step ?? 0]?.h ?? "";
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

// The explanations switched on or off (#524): the open brief's `?` says so, and its step draws again with or without its lines.
onExplanations(() => {
  const toggle = panel().querySelector<HTMLElement>(".brief-header .explain-toggle");
  if (toggle) pressToggle(toggle, explanationsOn());
  const view = currentMap();
  if (view && player.current()) show(view, true);
});

registerLayer("popover", { open: popoverOpen, close: closePopover });
registerLayer("brief", { open: briefOpen, close: closeBrief });
