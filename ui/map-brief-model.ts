// The PR brief view's pure logic (spec danielrispler/plotkeeper#334, ticket #339; spec #548 Group 4): what `map-brief.ts` draws, its
// prediction check, and the prompts its copy icons write. Split from `map-model.ts` by concern.

import type { Drawer, Rect } from "./map-model.js";
import type { AlsoGroup, AlsoKind, BriefFigure, BriefStep, BriefView, Camera, PartialFacts, PrBrief, PrDiffArgs, ReadyPr } from "./types.js";

// ---- The PR brief view (spec danielrispler/plotkeeper#334, ticket #339): what `map-brief.ts` draws, as pure logic.

/**
 * The steps a brief plays from one tray row: a chapter's own, with the overview in front when this is the chapter that carries it (`Playing.overview`).
 * A single story has one chapter.
 */
export function playlist(brief: PrBrief, chapter: number, overview: boolean): BriefStep[] {
  const own = brief.chapters[chapter]?.steps ?? [];
  return overview ? [...brief.overview, ...own] : own;
}

/**
 * What the brief says, without the Ratify step that ends its last chapter: reading to the end, the check and the "also touched" lines
 * all key on the story, since ratifying never blocks anything (#343).
 */
export function storySteps<T extends { k: string }>(list: T[]): T[] {
  return list.filter((step) => step.k !== "Ratify");
}

/**
 * "Also touched", which shows with the last step of the last chapter: an older brief's writer lines, a newer one's groups by kind (#492).
 */
export function alsoAt(brief: PrBrief, chapter: number, step: number, length: number): { lines: PrBrief["also"]; groups: AlsoRow[] } {
  return chapter === brief.chapters.length - 1 && step === length - 1 ? { lines: brief.also, groups: alsoRows(brief.also_groups ?? []) } : { lines: [], groups: [] };
}

// One Also touched group as the reader shows it (#492): its name, its counts, its files, each with the laws its `@law:` tags name and the
// "ignored" tag (only in the Tests, Config/build and Docs groups; the Ignored group is a plain list), and the paths its See the code asks for.
export type AlsoRow = { kind: AlsoKind; name: string; counts: string; files: { path: string; ignored: boolean; laws: string[] }[]; paths: string[] };
const alsoNames: Record<AlsoKind, string> = { "no-part": "No part", tests: "Tests", config: "Config/build", docs: "Docs", ignored: "Ignored", other: "Other" };
const tagsIgnored: ReadonlySet<AlsoKind> = new Set<AlsoKind>(["tests", "config", "docs"]);
const many = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
export function alsoRows(groups: AlsoGroup[]): AlsoRow[] {
  return groups
    .filter((group) => group.files.length)
    .map((group) => ({
      kind: group.kind,
      name: alsoNames[group.kind] ?? group.kind,
      counts: `${many(group.files.length, "file")} · ${many(group.lines, "line")}`,
      files: group.files.map((file) => ({ path: file.path, ignored: tagsIgnored.has(group.kind) && file.ignored === true, laws: file.laws ?? [] })),
      paths: group.files.map((file) => file.path),
    }));
}

// `overview`: this chapter plays the brief's overview first. It is the first chapter opened, and keeps it until it is finished, so it plays once.
// `story`: a "Since you read" story's head it runs from (#497), played apart from the full brief of the same PR; "" for a brief.
type Playing = { project: string; pr: number; chapter: number; step: number; overview: boolean; story: string };

/**
 * A chapter is complete when its last step was seen and its check answered, right or wrong, when it holds one (#342). Leaving
 * mid-chapter completes nothing. `fire` runs once, the moment both hold.
 */
export type Gate = { sawLast: () => void; answered: () => void; done: () => boolean };
export function chapterGate(list: Pick<BriefStep, "check">[], fire: () => void): Gate {
  let last = false;
  let answered = !list.some((step) => step.check);
  let fired = false;
  const look = () => {
    if (last && answered && !fired) {
      fired = true;
      fire();
    }
  };
  return {
    sawLast: () => {
      last = true;
      look();
    },
    answered: () => {
      answered = true;
      look();
    },
    done: () => fired,
  };
}

export type Player = {
  current: () => Playing | null;
  open: (project: string, pr: number, chapter: number, story?: string) => void;
  // Moves by `by` steps within `length`; "done" past the last one, which closes the brief and starts it over next time.
  go: (by: number, length: number) => "moved" | "done";
  // Leaves the brief where it is; the next `open` of it continues from there.
  close: () => void;
};
/** What plays and where each brief was left. Opening one while the map shows a past commit returns the map to now first. */
export function briefPlayer(deps: { scrubbed: () => boolean; backToNow: () => void }): Player {
  const left = new Map<string, number>();
  // Per brief: the chapter that carries the overview (the first opened), and whether it has been played to its end.
  const carrier = new Map<string, number>();
  const played = new Set<string>();
  const briefKey = (p: { project: string; pr: number; story: string }) => `${p.project}|${p.pr}|${p.story}`;
  const key = (p: { project: string; pr: number; story: string; chapter: number }) => `${briefKey(p)}|${p.chapter}`;
  let now: Playing | null = null;
  return {
    current: () => now,
    open(project, pr, chapter, story = "") {
      if (deps.scrubbed()) deps.backToNow();
      // Opening over a brief that is open leaves that one where it is.
      if (now) left.set(key(now), now.step);
      const at = { project, pr, chapter, step: 0, story };
      const brief = briefKey(at);
      if (!played.has(brief) && !carrier.has(brief)) carrier.set(brief, chapter);
      now = { ...at, step: left.get(key(at)) ?? 0, overview: !played.has(brief) && carrier.get(brief) === chapter };
    },
    go(by, length) {
      if (!now) return "moved";
      const to = now.step + by;
      if (to >= length) {
        left.delete(key(now));
        if (now.overview) {
          played.add(briefKey(now));
          carrier.delete(briefKey(now));
        }
        now = null;
        return "done";
      }
      now = { ...now, step: Math.max(0, to) };
      return "moved";
    },
    close() {
      if (now) left.set(key(now), now.step);
      now = null;
    },
  };
}

/**
 * Which of the requests that began last is the one still wanted: `begin` returns a check that is true until another request
 * begins or `cancel` is called, so a slow earlier request does not land over a later one or over a brief closed since.
 */
export function latestRequest() {
  let n = 0;
  return {
    begin: () => {
      const mine = ++n;
      return () => mine === n;
    },
    cancel: () => void n++,
  };
}

export type { Camera };
/**
 * Where the world sits so `box` fills the visible area `area` at the largest scale up to `max`, centred in the room the panel leaves above it
 * (`room.top` clear at the top, `room.bottom` taken by the panel). A point `p` of the world lands at `p * scale + (x, y)`.
 */
export function briefCamera(box: Rect, area: { w: number; h: number }, room: { top: number; bottom: number }, max: number): Camera {
  const height = Math.max(1, area.h - room.top - room.bottom);
  const scale = Math.min(area.w / Math.max(1, box.x1 - box.x0), height / Math.max(1, box.y1 - box.y0), max);
  return { scale, x: area.w / 2 - ((box.x0 + box.x1) / 2) * scale, y: room.top + height / 2 - ((box.y0 + box.y1) / 2) * scale };
}

export type Shape = { kind: "frame" | "chip" | "arrow" | "text"; x: number; y: number; w: number; h: number; text: string; tone: "live" | "lost" | "plain" };
export const figureSize = { w: 292, h: 150 };
const shape = (kind: Shape["kind"], x: number, y: number, w: number, h: number, text = "", tone: Shape["tone"] = "plain"): Shape => ({ kind, x, y, w, h, text, tone });
const toneOf = (item: { tone: string | null; bad: boolean | null }): Shape["tone"] => (item.bad || item.tone === "lost" ? "lost" : item.tone === "live" ? "live" : "plain");

/** How many items each kind of figure draws; its label for assistive tech names the same ones. */
export const figureMax = { states: 6, sequence: 5, compare: 4 };

/** A figure drawn from the writer's labels: the layout is code's, the words the writer's. A kind code does not know draws nothing. */
export function figureShapes(fig: BriefFigure): Shape[] {
  const { w, h } = figureSize;
  if (fig.kind === "states") {
    // A frame (a part's thing, such as a terminal) holding the states it can be in, two to a row.
    const out = [shape("frame", 8, 8, w - 16, h - 16, fig.frame ?? "")];
    fig.items.slice(0, figureMax.states).forEach((item, i) => out.push(shape("chip", 18 + (i % 2) * 136, 40 + Math.floor(i / 2) * 34, 120, 26, item.label, toneOf(item))));
    return out;
  }
  if (fig.kind === "sequence") {
    // The steps one after another, each a little further along, joined by a short line.
    const items = fig.items.slice(0, figureMax.sequence);
    const gap = Math.min(30, (h - 16) / Math.max(1, items.length));
    const chip = Math.min(24, gap - 4);
    const out: Shape[] = [];
    items.forEach((item, i) => {
      const x = 12 + (i * 120) / Math.max(1, items.length - 1);
      const y = 8 + i * gap;
      out.push(shape("chip", x, y, 150, chip, item.label, toneOf(item)));
      if (i < items.length - 1) out.push(shape("arrow", x + 10, y + chip, 2, gap - chip));
    });
    return out;
  }
  if (fig.kind === "compare") {
    const out = [shape("text", 10, 6, 118, 18, "before"), shape("text", 164, 6, 118, 18, "after"), shape("arrow", 134, 70, 24, 2)];
    fig.before.slice(0, figureMax.compare).forEach((label, i) => out.push(shape("chip", 10, 30 + i * 30, 118, 24, label)));
    fig.after.slice(0, figureMax.compare).forEach((label, i) => out.push(shape("chip", 164, 30 + i * 30, 118, 24, label, "live")));
    return out;
  }
  return [];
}

/** An item the checks left flagged shows "not confirmed from the facts". */
export const unconfirmed = (step: Pick<BriefStep, "flags">): boolean => step.flags.length > 0;
// What the check adds to an item still flagged after the retry (`UNCONFIRMED`, `understanding/src/pr_brief_check.rs`): the mark's own words.
const UNCONFIRMED = "not confirmed from the facts";
/** Why an item is not confirmed, as its popover lists it (#491): its flags without the mark's own words. */
export const unconfirmedReasons = (flags: string[]): string[] => flags.filter((flag) => flag !== UNCONFIRMED);

/**
 * The "Partial facts · N" chip's lines (#491): one per input the brief was written without, the cut diff's with its files. A brief stored
 * before the facts has only `partial`, whether its diff was cut, so it says that without the list. None when nothing was missing.
 */
export type PartialLine = { line: string; files: string[] };
const kib = (bytes: number) => (bytes < 1024 ? `${bytes}\u00a0B` : `${Math.round(bytes / 1024)}\u00a0KiB`);
export function partialFactsLines(facts: PartialFacts | undefined, partial: boolean): PartialLine[] {
  const lines: PartialLine[] = [];
  const cut = facts?.cut;
  if (cut) lines.push({ line: `The diff was cut: ${kib(cut.bytes_sent)} sent, ${kib(cut.bytes_cut)} not`, files: [...cut.files_cut, ...cut.files_partial.map((path) => `${path} (in part)`)] });
  else if (partial) lines.push({ line: "The diff was cut", files: [] });
  if (facts?.claim_error) lines.push({ line: `The claim check failed: ${facts.claim_error}`, files: [] });
  if (facts?.no_extractor?.length) lines.push({ line: `No extractor reads ${facts.no_extractor.join(", ")} files, so their imports are not on the map`, files: [] });
  return lines;
}

/**
 * What every `pr_diff` of the brief played in `project` asks; a caller adds the files it wants (`parts` or `paths`). A "Since you read"
 * story's diff runs from the head it was read at (#497), the full brief's from where the PR left the default branch.
 */
export function diffArgs(project: string, view: BriefView): PrDiffArgs {
  const ask: PrDiffArgs = { project, number: view.brief.pr, head: view.brief.head, title: view.brief.brief.title };
  return view.since ? { ...ask, base: view.since } : ask;
}

/**
 * Which files a step's "See the code" asks `pr_diff` for (#490), the step at `index` of the played `list`: "all" on the Ratify step, and
 * on the last step when the list has no Ratify step; else the map's parts it lights (actors left out), or "none" when it lights no part.
 */
export function stepCode(list: Pick<BriefStep, "k" | "on">[], index: number, parts: ReadonlySet<string>): "all" | "none" | string[] {
  const last = list.some((each) => each.k === "Ratify") ? list[index]?.k === "Ratify" : index === list.length - 1;
  if (last) return "all";
  const lit = (list[index]?.on ?? []).filter((id) => parts.has(id));
  return lit.length ? lit : "none";
}

/** What "See the code" opens in the drawer for a step: what the writer quoted, each with its source. The page has no editor to jump to. */
export function citeDrawer(step: Pick<BriefStep, "h" | "cites">): Drawer {
  return {
    title: step.h,
    note: step.cites.length ? "What this step rests on, as quoted from the facts." : "This step quotes nothing.",
    blocks: step.cites.map((cite) => ({
      label: cite.from,
      code: cite.quote,
      jumps: [],
    })),
    jumps: [],
  };
}

// ---- the brief's prediction check.

/** A pick check: one answer chosen among the options, taken back by tapping it again, graded by the key when Check is pressed. */
export type PickRun = { picked: () => number | null; tap: (i: number) => void; ready: () => boolean; grade: () => { right: boolean; key: number } | null; answered: () => boolean };
export function pickRun(key: number): PickRun {
  let picked: number | null = null;
  let done = false;
  return {
    picked: () => picked,
    tap(i) {
      if (!done) picked = picked === i ? null : i;
    },
    ready: () => !done && picked !== null,
    grade() {
      if (picked === null) return null;
      done = true;
      return { right: picked === key, key };
    },
    answered: () => done,
  };
}

/**
 * An order check: the parts tapped in order into numbered slots, one tap more takes a part back, and Check grades them against the key
 * (the changed flow's hops, set by code). The replay is the key's path along the map's links.
 */
export type OrderRun = {
  picked: () => string[];
  tap: (id: string) => void;
  ready: () => boolean;
  grade: () => { right: boolean; wrong: string[] } | null;
  answered: () => boolean;
  replay: () => [string, boolean][];
};
export function orderRun(key: string[], links: { id: string; from: string; to: string }[]): OrderRun {
  const picked: string[] = [];
  let done = false;
  return {
    picked: () => [...picked],
    tap(id) {
      if (done) return;
      const at = picked.indexOf(id);
      if (at >= 0) picked.splice(at, 1);
      else if (picked.length < key.length) picked.push(id);
    },
    ready: () => !done && picked.length === key.length,
    grade() {
      if (picked.length < key.length) return null;
      done = true;
      const wrong = picked.filter((id, i) => id !== key[i]);
      return { right: wrong.length === 0, wrong };
    },
    answered: () => done,
    replay() {
      const hops: [string, boolean][] = [];
      key.slice(1).forEach((to, i) => {
        const from = key[i];
        const forward = links.find((l) => l.from === from && l.to === to);
        const back = links.find((l) => l.from === to && l.to === from);
        if (forward) hops.push([forward.id, false]);
        else if (back) hops.push([back.id, true]);
      });
      return hops;
    },
  };
}

/**
 * What an open waiting on its brief's write does when the map draws `pr` (the ready PR, gone when undefined): "open" once the brief of
 * its head is stored, "stop" when the write failed, the PR is quiet or no longer ready, and "wait" while it is written or the map still
 * shows how it stood before the write started.
 */
export function awaitedWrite(pr: ReadyPr | undefined, full = false): "open" | "stop" | "wait" {
  const brief = pr?.brief;
  if (!pr || brief?.quiet || (brief?.failed && !brief.writing)) return "stop";
  // A "Since you read" story (#497) opens once stored, whatever head the full brief is of and while it is written after the story;
  // asking for the full brief waits for that.
  if (!full && brief?.since) return "open";
  return brief && !brief.writing && !brief.stale && brief.chapters.length > 0 ? "open" : "wait";
}

// ---- Copy for the agent (#493): a prompt code writes, with no model, that stands alone for any agent.

/**
 * What is copied: a step (a Could break step, or one not confirmed) with the changed files of the parts it lights; a not-confirmed
 * decision; the No part group's files.
 */
export type AgentItem =
  | { of: "step"; step: Pick<BriefStep, "k" | "h" | "b" | "risks" | "cites" | "flags">; files: string[] }
  | { of: "decision"; text: string; flags: string[] }
  | { of: "no-part"; files: string[] };

/**
 * The prompt for `item` of PR `pr`, naming its branch while it is open: a line that says what to do, the item's words, its risks, why
 * it is not confirmed (the mark's own words left out), its citations as `from: "quote"`, and its files as paths. Citations carry no
 * lines, so neither does the prompt. An empty section is left out.
 */
export function agentPrompt(pr: number, branch: string | null, item: AgentItem): string {
  const where = branch ? `In PR #${pr} (branch ${branch})` : `In PR #${pr}`;
  const list = (title: string | null, items: string[]) => (items.length ? [...(title ? [title] : []), ...items.map((each) => `- ${each}`)] : []);
  const reasons = (flags: string[]) => list("Why it is not confirmed:", unconfirmedReasons(flags));
  if (item.of === "no-part") {
    return [`${where}, these changed files belong to no part of the map (system-map.json). Assign these files to a part, or ignore them, and tell me what you changed.`, ...list(null, item.files)].join("\n");
  }
  if (item.of === "decision") {
    return [`${where}, you declared a decision the checks could not confirm from the facts. Check it against the code, correct it or say why it holds, and tell me what you changed.`, `Decided: ${item.text}`, ...reasons(item.flags)].join("\n");
  }
  const { step } = item;
  const lead =
    step.k === "Could break"
      ? `${where}, the brief warns this could break. Check whether it does, fix it if so, and tell me what you changed.`
      : `${where}, the brief says this, but its checks could not confirm it from the facts. Check it against the code, fix what is wrong, and tell me what you changed.`;
  const body = step.b.replace(/<b>([\s\S]*?)<\/b>/g, "$1");
  return [
    lead,
    `${step.k}: ${step.h}`,
    ...(body ? [body] : []),
    ...list(null, step.risks.map((risk) => `${risk.part}: ${risk.trigger} To see it: ${risk.check}`)),
    ...reasons(step.flags),
    ...list("It rests on:", step.cites.map((cite) => `${cite.from}: "${cite.quote}"`)),
    ...list("Changed files of the parts it lights:", item.files),
  ].join("\n");
}
