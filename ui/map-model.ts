// The Map view's pure logic (spec danielrispler/plotkeeper#306, ticket #309): the map file as the window reads it, where parts
// are placed, the drag threshold, the link's rim points, the layered Esc and the camera. `map.ts` draws and wires them.

import { isObject } from "./settings.js";
import type { AlsoGroup, AlsoKind, BriefFigure, PartialFacts, BriefStep, BriefView, CatchUpOffer, CatchUpStage, FogListEntry, Camera, Ci, CommitFiles, Fog, FogEntry, HistoricalMap, HistoryEntry, PrBrief, PrDiffArgs, Proof, ReadyPr } from "./types.js";

export type Tier = "core" | "interface" | "black-box";
export type Point = { x: number; y: number };
export type Size = { w: number; h: number };
// A part of a version 2 map (spec #473): a short purpose and the named things it holds, each with its key files. The server sends a
// version 1 map converted, so the page reads only these.
export type Held = { name: string; text: string; paths?: string[] };
export type Part = { id: string; name: string; caption: string; purpose: string; holds: Held[]; tier: Tier; paths: string[]; zone?: string };
export type Actor = { id: string; name: string };
// A link's kind (spec #473): `import`, a code dependency the check proves, or a run-time kind with a one-sentence reason. A version 1
// map's links have none.
export type LinkKind = "import" | "http" | "spawn" | "file";
export type MapLink = { id: string; from: string; to: string; kind?: LinkKind; reason?: string };
export type Law = { id: string; on: string; backing: "check" | "trust"; text: string };
export type MapDoc = { ok: true; parts: Part[]; actors: Actor[]; links: MapLink[]; laws: Law[] } | { ok: false; why: string };

// The sizes the stylesheet draws, `--plate-w` and the rest of tokens.css; a test holds them equal.
export const plateSize: Size = { w: 264, h: 76 };
export const actorSize: Size = { w: 120, h: 28 };
// How far the pointer moves before a press on a plate becomes a drag, so a click never nudges it.
export const dragThreshold = 6;
// The layers Esc peels, first to last (user story 5, spec #334 and spec #380): an overlay over the map (a micro-world, #399, then the pre-flight,
// #394; spec #378, "Layers and keys"), the patch review (#398), a drawer, a brief's "not confirmed" popover (#491), a PR brief (closing it keeps its step), a scrubbed past commit (back to now), a part's sheet, then the left panel (#385). The
// tickets that add them register a `Layer` under its name in `map.ts`; the Map view itself is left only when none is open.
export const layerOrder = ["micro-world", "pre-flight", "patch-review", "drawer", "popover", "brief", "scrubbed", "sheet", "panel"] as const;
export type LayerName = (typeof layerOrder)[number];
export type Layer = { open: () => boolean; close: () => void };

const tiers: Tier[] = ["core", "interface", "black-box"];
// The map file's lists, each entry checked for the fields that name it and tie it to the rest; the server read the file leniently, so
// the other fields are as the map's version writes them. A list that is missing or holds anything else is a broken map.
const named = (value: unknown, fields: string[]): value is Record<string, string> => isObject(value) && fields.every((field) => typeof value[field] === "string");
const isPart = (value: unknown): value is Part => named(value, ["id", "name"]);
const isActor = (value: unknown): value is Actor => named(value, ["id", "name"]);
const isLink = (value: unknown): value is MapLink => named(value, ["id", "from", "to"]);
const isLaw = (value: unknown): value is Law => named(value, ["id", "on"]);
const listOf = <T>(value: unknown, is: (entry: unknown) => entry is T): T[] | null => (Array.isArray(value) && value.every(is) ? value : null);

// The map file's text as the window draws it. The shell already read it leniently, so an unknown field is ignored here too.
export function parseMap(text: string): MapDoc {
  let file: unknown;
  try {
    file = JSON.parse(text);
  } catch (e) {
    return { ok: false, why: String(e) };
  }
  if (!isObject(file)) return { ok: false, why: "the map is not a JSON object" };
  const [parts, actors, links, laws] = [listOf(file.parts, isPart), listOf(file.actors, isActor), listOf(file.links, isLink), listOf(file.laws, isLaw)];
  if (!parts || !actors || !links || !laws) return { ok: false, why: "the map lacks its parts, actors, links or laws" };
  return { ok: true, parts, actors, links, laws };
}

// Daniel's tier for a part beats the map's own; a choice that is no tier is ignored.
export const effectiveTier = (part: Part, chosen: Record<string, string>): Tier => {
  const own = chosen[part.id] as Tier | undefined;
  return own && tiers.includes(own) ? own : part.tier;
};

// The gap kept between plates, and the corner of the world they start from.
const gap = 48;
const corner: Point = { x: plateSize.w / 2 + gap, y: plateSize.h / 2 + gap };
const start: Point = { x: 640, y: 360 };
const directions: Point[] = [0, 90, 180, 270, 45, 135, 225, 315].map((degrees) => ({ x: Math.cos((degrees * Math.PI) / 180), y: Math.sin((degrees * Math.PI) / 180) }));

// Where each item without a saved position goes, once: never an item already placed. An item is put near what it links to, else near
// the others of its `zone`, else near the middle of the plates there are, on the nearest spot clear of every plate. The one with
// the most placed neighbours goes first, so a map grows outwards from what is placed; ties go in the map's order. Pure, so the
// same map and positions always give the same places.
export function autoPlace(items: { id: string; zone?: string }[], links: { from: string; to: string }[], placed: Record<string, Point>): Record<string, Point> {
  const all: Record<string, Point> = { ...placed };
  const added: Record<string, Point> = {};
  let waiting = items.filter((item) => !(item.id in placed));
  const neighbours = (id: string) => links.flatMap((link) => (link.from === id ? [link.to] : link.to === id ? [link.from] : [])).filter((other) => other in all);
  const mean = (points: Point[]): Point => ({ x: points.reduce((s, p) => s + p.x, 0) / points.length, y: points.reduce((s, p) => s + p.y, 0) / points.length });
  while (waiting.length) {
    const item = waiting.reduce((best, next) => (neighbours(next.id).length > neighbours(best.id).length ? next : best));
    waiting = waiting.filter((other) => other !== item);
    const near = neighbours(item.id);
    const zoned = items.filter((other) => other.zone !== undefined && other.zone === item.zone && other.id in all && other.id !== item.id).map((other) => other.id);
    const anchors = [...near, ...zoned].flatMap((id) => all[id] ?? []);
    const where = anchors.length ? mean(anchors) : Object.keys(all).length ? mean(Object.values(all)) : start;
    const free = (at: Point) =>
      at.x >= corner.x - gap && at.y >= corner.y - gap && Object.values(all).every((other) => Math.abs(other.x - at.x) >= plateSize.w + gap || Math.abs(other.y - at.y) >= plateSize.h + gap);
    let spot: Point | null = null;
    for (let ring = 0; spot === null; ring++) {
      const around = ring === 0 ? [{ x: 0, y: 0 }] : directions.map((d) => ({ x: d.x * ring * (plateSize.w + gap), y: d.y * ring * (plateSize.h + gap) }));
      for (const offset of around) {
        const at = { x: Math.round(where.x + offset.x), y: Math.round(where.y + offset.y) };
        if (free(at)) {
          spot = at;
          break;
        }
      }
    }
    all[item.id] = added[item.id] = spot;
  }
  return added;
}

// A press on a plate that may become a drag: the pointer where it began, the plate's centre then, and whether the pointer has
// moved the threshold's distance yet.
export type Drag = { id: string; from: Point; origin: Point; moved: boolean };
export const dragStart = (id: string, from: Point, origin: Point): Drag => ({ id, from, origin, moved: false });

// The pointer at `at`: where the plate goes, null while the press is still a click. Once past the threshold the plate follows the
// pointer exactly, even back over the start.
export function dragMove(drag: Drag, at: Point): { drag: Drag; to: Point | null } {
  const moved = drag.moved || Math.hypot(at.x - drag.from.x, at.y - drag.from.y) >= dragThreshold;
  return { drag: { ...drag, moved }, to: moved ? { x: drag.origin.x + at.x - drag.from.x, y: drag.origin.y + at.y - drag.from.y } : null };
}

// Where the line from the centre of a box of `size` towards `toward` leaves the box: a link runs rim to rim. The centre itself
// when `toward` is on it.
export function rimPoint(center: Point, size: Size, toward: Point): Point {
  const [dx, dy] = [toward.x - center.x, toward.y - center.y];
  if (dx === 0 && dy === 0) return center;
  const reach = Math.min(dx === 0 ? Infinity : size.w / 2 / Math.abs(dx), dy === 0 ? Infinity : size.h / 2 / Math.abs(dy));
  return { x: center.x + dx * reach, y: center.y + dy * reach };
}

// A project switch: closes the layers that belong to the project shown, whatever the next one draws, since it may have no map to draw.
export function switchProject(layers: Partial<Record<LayerName, Layer>>) {
  for (const name of ["micro-world", "pre-flight", "patch-review", "popover", "brief"] as const) if (layers[name]?.open()) layers[name].close();
}

// Esc: closes the first layer that is open, in `layerOrder`, and says which; null when none was: on the bare map Esc does nothing, since the
// map is the page's base (#386).
export function escapeMap(layers: Partial<Record<LayerName, Layer>>): LayerName | null {
  for (const name of layerOrder) {
    const layer = layers[name];
    if (layer?.open()) {
      layer.close();
      return name;
    }
  }
  return null;
}

// The first prompt of "Draft a map", for a project with no `system-map.json` on its default branch.
export const draftPrompt =
  "Draft system-map.json at this repository's root, in plain words: its parts, each with the files it owns, a purpose of one or two sentences " +
  "and the named things it holds (each with one sentence and its key files), the outside actors, the links between them, " +
  "the files that belong to no part on purpose, and the laws that must stay true. Run `system-map check` and fix what it reports until it passes.";

// The part sheet (ticket #310) as data: what `map-sheet.ts` draws for one part.
export const tierLabel: Record<Tier, string> = { core: "Core", interface: "Interface-only", "black-box": "Black box" };
export type Sheet = {
  name: string;
  caption: string;
  purpose: string;
  // Each held item with its key files, its `paths` as written; an item without paths shows none.
  holds: { name: string; text: string; files: string[] }[];
  tier: Tier;
  // The three tiers in a fixed order, the effective one pressed. Never a hint to lower one.
  tiers: { tier: Tier; label: string; pressed: boolean }[];
  laws: { id: string; text: string; backing: Law["backing"]; label: string }[];
  fogLine: string;
  // "I know this" shows only while the part shows fog.
  canClear: boolean;
};
export function sheetOf(part: Part, laws: Law[], tier: Tier, fog: Fog | undefined): Sheet {
  const fogLine = fog === "thick"
    ? "Its laws, links or flows changed since you last looked"
    : fog === "thin"
      ? "Files in this part changed since you last looked"
      : tier === "black-box"
        ? "A black box part never fogs"
        : tier === "interface"
          ? "Fogs only when its laws, links or flows change"
          : "Clear: nothing has changed since you last looked";
  return {
    name: part.name,
    caption: part.caption,
    purpose: part.purpose,
    holds: (part.holds ?? []).map((item) => ({ name: item.name, text: item.text, files: item.paths ?? [] })),
    tier,
    tiers: tiers.map((each) => ({ tier: each, label: tierLabel[each], pressed: each === tier })),
    laws: laws.filter((law) => law.on === part.id).map((law) => ({ id: law.id, text: law.text, backing: law.backing, label: law.backing === "check" ? "Backed by a check" : "On trust" })),
    fogLine,
    canClear: fog !== undefined,
  };
}

// Why a part's plate carries its marks, at the top of its sheet (#495), from the facts of this case: its newest thick fog entry (else its
// newest) with its PR and why, and how many more; its staleness band with its last engagement (the server's line); a broken law, never
// named. Null where the part has no such mark.
export type SheetMarks = { fog: { line: string; more: number } | null; stale: string | null; broken: string | null };
export function sheetMarks(part: string, fog: Fog | undefined, entries: FogEntry[], stale: string | undefined, ci: Ci | null): SheetMarks {
  // Entries are filed oldest first.
  const newest = [...entries].reverse();
  const shown = newest.find((entry) => entry.strength === "thick") ?? newest[0];
  const strength = (entry: FogEntry) => (entry.strength === "thick" ? "Thick" : "Thin");
  const from = (entry: FogEntry) => (entry.pr ? `#${entry.pr}` : `commit ${entry.commit.slice(0, 7)}`);
  const why = (entry: FogEntry) => (entry.why?.length ? `: ${entry.why.join(", ")}` : "");
  const date = ci?.at ? `, ${ci.at.slice(0, 10)}` : "";
  return {
    fog: fog && shown ? { line: `${strength(shown)} fog from ${from(shown)}${why(shown)}`, more: entries.length - 1 } : null,
    stale: stale ?? null,
    broken: brokenParts(ci).has(part) ? `A check behind a law of this part failed in main's latest CI${date}` : null,
  };
}

// The sheet's "Talks to" (spec #473): each link at either end of `part`, in the map's order, with the other end by name, whether its
// arrow leaves the part, its kind and its reason. The sheet has no other list of links.
export type TalkRow = { link: string; other: string; out: boolean; kind?: LinkKind | undefined; reason?: string | undefined };
export const talksTo = (part: string, links: MapLink[], nameOf: (id: string) => string): TalkRow[] =>
  links
    .filter((link) => link.from === part || link.to === part)
    .map((link) => ({ link: link.id, other: nameOf(link.from === part ? link.to : link.from), out: link.from === part, kind: link.kind, reason: link.reason }));

// The map's lines (spec #473): links joining the same two ends, in either direction, are one line, drawn the way its first link runs and
// holding every link's id, so a brief's light and a law's mark on any of them find it.
export type LinkLine = { ids: string[]; from: string; to: string };
export function linkLines(links: MapLink[]): LinkLine[] {
  const lines = new Map<string, LinkLine>();
  for (const link of links) {
    const key = JSON.stringify([link.from, link.to].sort());
    const line = lines.get(key);
    if (line) line.ids.push(link.id);
    else lines.set(key, { ids: [link.id], from: link.from, to: link.to });
  }
  return [...lines.values()];
}

// A plate's hover text: its name, caption and purpose, never the held items, so a glance stays a glance.
export const plateTitle = (part: Part): string => [part.name, part.caption, part.purpose].filter(Boolean).join("\n\n");

// Broken laws and the drawer (ticket #312). The drawer is read-only and holds any content a ticket gives it: blocks of code or text and
// the one-click jumps out of it. `openDrawer` (map-drawer.ts) draws a `Drawer`; the history strip's commit drill-down builds one too.
export type Jump = { label: string; title: string; to: { url: string } };
export type Block = { label?: string; code: string; jumps: Jump[] };
export type Drawer = { title: string; note?: string | undefined; blocks: Block[]; jumps: Jump[] };

// What the mark and the drill-down say for a broken law: coarse, never naming the law.
export const brokenText = "A test that backs laws here failed on main";

// What CI on the default branch said, in words. A missing red mark means only "no known failure", so each outcome says which it is.
export function ciLine(ci: Ci | null): string {
  const earlier = ci?.earlier ? " (for an earlier commit; a newer run is going)" : "";
  switch (ci?.status) {
    case "passed": return `CI on main passed${earlier}`;
    case "broken": return `${brokenText}${earlier}`;
    case "not-run": return `CI on main failed, but the law tests did not run${earlier}`;
    case "unreadable": return `CI on main failed and its log cannot be read${earlier}`;
    default: return "CI on main has not been read";
  }
}

// The parts a broken law marks: only when the run failed in a test that backs a law.
export const brokenParts = (ci: Ci | null): Set<string> => new Set(ci?.status === "broken" ? ci.parts : []);

// What the part sheet says of CI for `part`, or null when it holds no law a check backs and is not marked: its line, whether it is
// the loud one, and whether a drill-down has anything to show.
export function sheetCi(ci: Ci | null, part: string, backed: boolean): { line: string; broken: boolean; drill: boolean } | null {
  const broken = brokenParts(ci).has(part);
  if (!broken && !backed) return null;
  return { line: ciLine(ci), broken, drill: ci !== null && ci.status !== "unknown" };
}

// "See the proof" for a law: each tagged test's code from the tag on. The page has no editor to jump to.
export function proofDrawer(law: string, proofs: Proof[]): Drawer {
  const blocks: Block[] = proofs.map((proof) => ({
    label: `${proof.file}:${proof.line}${proof.test ? ` · ${proof.test}` : ""}`,
    code: proof.snippet,
    jumps: [],
  }));
  return {
    title: `The proof of ${law}`,
    blocks: blocks.length ? blocks : [{ code: "No test is tagged for this law on the default branch.", jumps: [] }],
    jumps: [],
  };
}

// The broken-law drill-down: the run's status, a note when a newer run is going, the failing log's tail, and a jump to the run on GitHub.
export function ciDrawer(ci: Ci): Drawer {
  return {
    title: ciLine({ ...ci, earlier: false }),
    note: ci.earlier ? "This is the last finished run, for an earlier commit; a newer run is going." : undefined,
    blocks: ci.tail ? [{ label: "End of the failed log", code: ci.tail, jumps: [] }] : [],
    jumps: ci.run_url ? [{ label: "Open on GitHub", title: "Open this run on GitHub", to: { url: ci.run_url } }] : [],
  };
}

// The history strip and time travel (ticket #313). A scrub is a past commit's map shown read-only.
export type Scrub = Pick<HistoricalMap, "commit" | "short" | "label"> & { source: string };
export type StripRow = HistoryEntry & { current: boolean; title: string };

// What a commit of the history strip offers of its PR's stored brief: a quiet "Brief" button, none when the PR has none.
export const briefOffer = (row: Pick<HistoryEntry, "brief" | "short">): { number: number; label: string; title: string } | null => {
  if (row.brief === null) return null;
  return { number: row.brief, label: "Brief", title: `Play the brief of #${row.brief}, which this commit merged` };
};

export const stripOf = (entries: HistoryEntry[], viewing: string | null): StripRow[] =>
  entries.map((entry) => ({
    ...entry,
    current: entry.commit === viewing,
    title: `${entry.short}${entry.fogged ? " · you have not caught up on this" : ""}\nShow the map as of this commit`,
  }));

// The quiet banner while a past commit is shown: what it says and the word of the way back.
export const scrubBanner = (scrub: Pick<Scrub, "short">): { text: string; back: string } => ({ text: `Viewing ${scrub.short}`, back: "Back to now" });

// A commit's drill-down: each file it changed with its diff, and the commit on GitHub when the project is there.
export function commitDrawer(found: CommitFiles): Drawer {
  const count = `${found.files.length} ${found.files.length === 1 ? "file" : "files"}`;
  return {
    title: found.label,
    note: `${found.short} · ${count}`,
    blocks: found.files.length ? found.files.map((file) => ({ label: file.path, code: file.diff, jumps: [] })) : [{ code: "No files changed.", jumps: [] }],
    jumps: found.url ? [{ label: "Open on GitHub", title: "Open this commit on GitHub", to: { url: found.url } }] : [],
  };
}

// Tells which of a series of scrubs is the newest. `start` takes a ticket; `cancel` (Back to now) makes every ticket out stale; a
// scrub that reads slowly lands only while its ticket is still `live`, so it never overrides what Daniel did after it.
export function scrubTickets() {
  let latest = 0;
  return {
    start: () => ++latest,
    cancel: () => void ++latest,
    live: (ticket: number) => ticket === latest,
  };
}

// ---- The PR brief view (spec danielrispler/plotkeeper#334, ticket #339): what `map-brief.ts` draws, as pure logic.

// The steps a brief plays from one tray row: a chapter's own, with the overview in front when this is the chapter that carries it (`Playing.overview`).
// A single story has one chapter.
export function playlist(brief: PrBrief, chapter: number, overview: boolean): BriefStep[] {
  const own = brief.chapters[chapter]?.steps ?? [];
  return overview ? [...brief.overview, ...own] : own;
}

// What the brief says, without the Ratify step that ends its last chapter: reading to the end, the check and the "also touched" lines
// all key on the story, since ratifying never blocks anything (#343).
export function storySteps<T extends { k: string }>(list: T[]): T[] {
  return list.filter((step) => step.k !== "Ratify");
}

// "Also touched", which shows with the last step of the last chapter: an older brief's writer lines, a newer one's groups by kind (#492).
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
export type Playing = { project: string; pr: number; chapter: number; step: number; overview: boolean; story: string };

// A chapter is complete when its last step was seen and its check answered, right or wrong, when it holds one (#342). Leaving
// mid-chapter completes nothing. `fire` runs once, the moment both hold.
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
// What plays and where each brief was left. Opening one while the map shows a past commit returns the map to now first.
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

// Which of the requests that began last is the one still wanted: `begin` returns a check that is true until another request
// begins or `cancel` is called, so a slow earlier request does not land over a later one or over a brief closed since.
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

export type Rect = { x0: number; y0: number; x1: number; y1: number };
export type { Camera };
// Where the world sits so `box` fills the visible area `area` at the largest scale up to `max`, centred in the room the panel leaves above it
// (`room.top` clear at the top, `room.bottom` taken by the panel). A point `p` of the world lands at `p * scale + (x, y)`.
export function briefCamera(box: Rect, area: { w: number; h: number }, room: { top: number; bottom: number }, max: number): Camera {
  const height = Math.max(1, area.h - room.top - room.bottom);
  const scale = Math.min(area.w / Math.max(1, box.x1 - box.x0), height / Math.max(1, box.y1 - box.y0), max);
  return { scale, x: area.w / 2 - ((box.x0 + box.x1) / 2) * scale, y: room.top + height / 2 - ((box.y0 + box.y1) / 2) * scale };
}

export type Shape = { kind: "frame" | "chip" | "arrow" | "text"; x: number; y: number; w: number; h: number; text: string; tone: "live" | "lost" | "plain" };
export const figureSize = { w: 292, h: 150 };
const shape = (kind: Shape["kind"], x: number, y: number, w: number, h: number, text = "", tone: Shape["tone"] = "plain"): Shape => ({ kind, x, y, w, h, text, tone });
const toneOf = (item: { tone: string | null; bad: boolean | null }): Shape["tone"] => (item.bad || item.tone === "lost" ? "lost" : item.tone === "live" ? "live" : "plain");

// How many items each kind of figure draws; its label for assistive tech names the same ones.
export const figureMax = { states: 6, sequence: 5, compare: 4 };

// A figure drawn from the writer's labels: the layout is code's, the words the writer's. A kind code does not know draws nothing.
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

// An item the checks left flagged shows "not confirmed from the facts".
export const unconfirmed = (step: Pick<BriefStep, "flags">): boolean => step.flags.length > 0;
// What the check adds to an item still flagged after the retry (`UNCONFIRMED`, `understanding/src/pr_brief_check.rs`): the mark's own words.
const UNCONFIRMED = "not confirmed from the facts";
// Why an item is not confirmed, as its popover lists it (#491): its flags without the mark's own words.
export const unconfirmedReasons = (flags: string[]): string[] => flags.filter((flag) => flag !== UNCONFIRMED);

// The "Partial facts · N" chip's lines (#491): one per input the brief was written without, the cut diff's with its files. A brief stored
// before the facts has only `partial`, whether its diff was cut, so it says that without the list. None when nothing was missing.
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

// What every `pr_diff` of the brief played in `project` asks; a caller adds the files it wants (`parts` or `paths`). A "Since you read"
// story's diff runs from the head it was read at (#497), the full brief's from where the PR left the default branch.
export function diffArgs(project: string, view: BriefView): PrDiffArgs {
  const ask: PrDiffArgs = { project, number: view.brief.pr, head: view.brief.head, title: view.brief.brief.title };
  return view.since ? { ...ask, base: view.since } : ask;
}

// Which files a step's "See the code" asks `pr_diff` for (#490), the step at `index` of the played `list`: "all" on the Ratify step, and
// on the last step when the list has no Ratify step; else the map's parts it lights (actors left out), or "none" when it lights no part.
export function stepCode(list: Pick<BriefStep, "k" | "on">[], index: number, parts: ReadonlySet<string>): "all" | "none" | string[] {
  const last = list.some((each) => each.k === "Ratify") ? list[index]?.k === "Ratify" : index === list.length - 1;
  if (last) return "all";
  const lit = (list[index]?.on ?? []).filter((id) => parts.has(id));
  return lit.length ? lit : "none";
}

// What "See the code" opens in the drawer for a step: what the writer quoted, each with its source. The page has no editor to jump to.
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

// A pick check: one answer chosen among the options, taken back by tapping it again, graded by the key when Check is pressed.
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

// An order check: the parts tapped in order into numbered slots, one tap more takes a part back, and Check grades them against the key
// (the changed flow's hops, set by code). The replay is the key's path along the map's links.
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

// What an open waiting on its brief's write does when the map draws `pr` (the ready PR, gone when undefined): "open" once the brief of
// its head is stored, "stop" when the write failed, the PR is quiet or no longer ready, and "wait" while it is written or the map still
// shows how it stood before the write started.
export function awaitedWrite(pr: ReadyPr | undefined, full = false): "open" | "stop" | "wait" {
  const brief = pr?.brief;
  if (!pr || brief?.quiet || (brief?.failed && !brief.writing)) return "stop";
  // A "Since you read" story (#497) opens once stored, whatever head the full brief is of and while it is written after the story;
  // asking for the full brief waits for that.
  if (!full && brief?.since) return "open";
  return brief && !brief.writing && !brief.stale && brief.chapters.length > 0 ? "open" : "wait";
}

// ---- The map's camera (spec #380, ticket #383): pan and zoom applied to the world, in place of a scrolling box. A point `p` of the world
// lands at `p * scale + (x, y)` in the map area, as with `briefCamera`.

export const zoomLimits = { min: 0.4, max: 2 };
// Zooms `camera` by `factor` around the area's point (px, py), which keeps the world point under it; the scale is held to `zoomLimits`.
export function zoomAt(camera: Camera, factor: number, px: number, py: number): Camera {
  const scale = Math.min(zoomLimits.max, Math.max(zoomLimits.min, camera.scale * factor));
  const r = scale / camera.scale;
  return { scale, x: px - (px - camera.x) * r, y: py - (py - camera.y) * r };
}
// The camera a drag of the empty background gives: `start` moved by the pointer's travel from `from` to `at`, at the same scale.
export const panTo = (start: Camera, from: Point, at: Point): Camera => ({ scale: start.scale, x: start.x + at.x - from.x, y: start.y + at.y - from.y });
// The camera that fits `box` (the whole map) into an area `area` large with `gutter` kept around it, centred; never above 125 % (a small map is
// not blown up) nor outside `zoomLimits`.
export function fitCamera(box: Rect, area: { w: number; h: number }, gutter: number): Camera {
  const room = { w: Math.max(1, area.w - 2 * gutter), h: Math.max(1, area.h - 2 * gutter) };
  const wanted = Math.min(room.w / Math.max(1, box.x1 - box.x0), room.h / Math.max(1, box.y1 - box.y0), 1.25);
  const scale = Math.min(zoomLimits.max, Math.max(zoomLimits.min, wanted));
  return { scale, x: area.w / 2 - ((box.x0 + box.x1) / 2) * scale, y: area.h / 2 - ((box.y0 + box.y1) / 2) * scale };
}
// Reveals plates spanning `left` to `right` of the world by panning just enough to bring them, with `wanted` gutter, inside the visible band of
// the area (from the panel's right edge to the sheet's left edge, in area coordinates). The gutter shrinks to the room the band has; a span
// wider than the band is left where it is. Only the camera moves: no position is written.
export function revealPan(camera: Camera, left: number, right: number, band: { left: number; right: number }, wanted: number): Camera {
  const [lo, hi] = [left * camera.scale + camera.x, right * camera.scale + camera.x];
  const gutter = Math.min(wanted, (band.right - band.left - (hi - lo)) / 2);
  if (gutter < 0) return camera;
  if (hi + gutter > band.right) return { ...camera, x: camera.x + band.right - (hi + gutter) };
  if (lo - gutter < band.left) return { ...camera, x: camera.x + band.left - (lo - gutter) };
  return camera;
}
// The map's one camera, shared by Daniel's view and a PR brief's: a brief borrows it with `lend` while it plays (each step may lend again),
// and `giveBack` shows Daniel's view as the brief found it. While lent, Daniel's moves are refused (`move` says false), so his view is never
// the brief's.
export type CameraRig = { shown: () => Camera; user: () => Camera; lent: () => boolean; move: (to: Camera) => boolean; lend: (to: Camera) => void; giveBack: () => Camera };
export function cameraRig(start: Camera): CameraRig {
  let user = start;
  let borrowed: Camera | null = null;
  return {
    shown: () => borrowed ?? user,
    user: () => user,
    lent: () => borrowed !== null,
    move: (to) => {
      if (borrowed) return false;
      user = to;
      return true;
    },
    lend: (to) => {
      borrowed = to;
    },
    giveBack: () => {
      borrowed = null;
      return user;
    },
  };
}

// ---- The left panel (spec #380, ticket #385): one slot over the map's left side for a session's story, History, a turn's diff (widened) or
// Settings, beside the part's sheet on the right.
export type PanelContent =
  | { kind: "story"; session: string }
  | { kind: "history" }
  | { kind: "diff"; session: string; turn: number }
  | { kind: "settings" }
  // A worker's or helper's log (#386): it had the centre column, which the map replaced.
  | { kind: "log"; session: string; agent: string };
export type Overlays = { panel: PanelContent | null; sheet: string | null };
// Below this window width only one overlay is open at a time (user story 52).
export const narrowBelow = 1007;
// Opens `panel` (null closes it). Below `narrowBelow`, or for the widened diff, the part's sheet closes (user stories 29, 52).
export const withPanel = (now: Overlays, panel: PanelContent | null, width: number): Overlays => ({
  panel,
  sheet: panel && (width < narrowBelow || panel.kind === "diff") ? null : now.sheet,
});
// Opens the sheet of part `sheet` (null closes it). Below `narrowBelow`, or over the widened diff, the panel closes.
export const withSheet = (now: Overlays, sheet: string | null, width: number): Overlays => ({
  sheet,
  panel: sheet && (width < narrowBelow || now.panel?.kind === "diff") ? null : now.panel,
});
// A press on a session's row or chip: its story, or none when the panel already shows that story (a second click closes it).
export const pressRow = (panel: PanelContent | null, session: string): PanelContent | null =>
  panel?.kind === "story" && panel.session === session ? null : { kind: "story", session };
// The session the panel shows (its story, a turn's diff or a worker's log), whose chips stay lit while it is open, and whose closing selects no session.
export const panelSession = (panel: PanelContent | null): string | null =>
  panel?.kind === "story" || panel?.kind === "diff" || panel?.kind === "log" ? panel.session : null;
// The project the page opens on (#386): the last viewed, while the list still holds it, else the first; none on a fresh install.
export const startProject = (last: string | null, projects: string[]): string | null =>
  last !== null && projects.includes(last) ? last : (projects[0] ?? null);
// What the page shows (#386): a project's map, and what the left panel holds over it.
export type PageView = { project: string | null; panel: PanelContent | null };
// Selecting a session shows its project's map, with its story, or a worker's log, in the left panel.
export const selectedView = (session: string, project: string, agent: string | null): PageView => ({
  project,
  panel: agent === null ? { kind: "story", session } : { kind: "log", session, agent },
});
// A press on a project header shows that project's map; a session's panel of another project closes with the switch. A press on the shown
// project's header folds its group, as the header did before it switched projects (#97).
export function pressHeader(page: PageView, pressed: string, projectOf: (session: string) => string | null): PageView | "fold" {
  if (pressed === page.project) return "fold";
  const session = panelSession(page.panel);
  return { project: pressed, panel: session === null || projectOf(session) === pressed ? page.panel : null };
}

// ---- The part catch-up (spec #378, ticket #391): what a fogged part's sheet offers, from the server's `catch_ups`.

// The sheet's fog buttons, the main one first: the part catch-up (or, for a part fogged by one PR whose brief lights it, that chapter), then
// the quiet "I know this". A catch-up being written stays pressable: "Writing…" never blocks the sheet or the map.
export type FogAction = { label: string; title: string; note: string | null; disabled: boolean; open: { kind: "catch-up" } | { kind: "chapter"; pr: number; chapter: number } };
export function fogActions(fog: Fog | undefined, offer: CatchUpOffer | undefined): { main: FogAction | null; second: "I know this" | null } {
  if (!fog) return { main: null, second: null };
  if (!offer) return { main: null, second: "I know this" };
  const main: FogAction =
    offer.kind === "chapter"
      ? { label: `Read #${offer.pr}'s brief`, title: "Open the chapter of this PR's brief that covers this part", note: null, disabled: false, open: { kind: "chapter", pr: offer.pr, chapter: offer.chapter } }
      : {
          label: "Part catch-up",
          title: "One story of everything that landed here since you last caught up; reading it to the end clears this part's fog",
          note: offer.writing ? "Writing…" : offer.failed ? "The last write failed; press to try again" : null,
          disabled: false,
          open: { kind: "catch-up" },
        };
  return { main, second: "I know this" };
}

// What a catch-up opened while it was written waits for, read from the part's offer at each map draw: "wait" while a write runs, then
// "open" once it is stored for the part's fog, else "stop" (its write failed, the part was cleared meanwhile, or no write runs).
export function awaitedCatchUp(offer: CatchUpOffer | undefined): "open" | "stop" | "wait" {
  if (offer?.kind !== "catch-up") return "stop";
  if (offer.writing) return "wait";
  return offer.ready ? "open" : "stop";
}

// What the waiting card says of a part catch-up's write (#450): the call that runs now, "Writing…" until the server says.
// A fixing stage's count covers every flagged item, the code checks' (a long label or body) as well as the claim check's,
// so it says "problems", not "claims".
export function stageLabel(stage: CatchUpStage | undefined): string {
  if (stage?.kind === "checking") return "Checking claims…";
  if (stage?.kind === "fixing") return `Fixing ${stage.claims} ${stage.claims === 1 ? "problem" : "problems"}…`;
  return "Writing…";
}

// The number a part catch-up plays under in `briefPlayer`: a negative one, which no PR has, one per project, part and fog list, so a
// catch-up rewritten for a new fog list starts at its first step rather than where the old story was left.
export function catchUpIds(): (project: string, part: string, fog: string[]) => number {
  const ids = new Map<string, number>();
  return (project, part, fog) => {
    const key = `${project}|${part}|${fog.join(",")}`;
    const id = ids.get(key) ?? -(ids.size + 1);
    ids.set(key, id);
    return id;
  };
}

// A long part catch-up's full fog list, one line per entry, oldest first: a PR by its number and title, a direct push by its subject and
// short commit.
export function fogListLines(entries: FogListEntry[]): string[] {
  return entries.map((e) => (e.pr === null ? `${e.title} (${e.commit.slice(0, 7)}, no PR)` : `#${e.pr} ${e.title}`));
}

// The parts' tab order (#460): top to bottom, then left to right. A plate within half a plate's height of a row's first (highest) plate
// reads on that row, so one dragged a little out of line keeps its place.
export function readingOrder(positions: Record<string, Point>): string[] {
  const rows: [string, Point][][] = [];
  for (const entry of Object.entries(positions).sort(([, a], [, b]) => a.y - b.y || a.x - b.x)) {
    const row = rows.at(-1);
    const first = row?.[0];
    if (row && first && entry[1].y - first[1].y < plateSize.h / 2) row.push(entry);
    else rows.push([entry]);
  }
  return rows.flatMap((row) => row.sort(([, a], [, b]) => a.x - b.x).map(([id]) => id));
}

// Where the law marks of a line from `a` to `b` sit: `count` points along it, 10px apart, centred on its middle, so several never stack.
export function markPoints(a: Point, b: Point, count: number): Point[] {
  const [dx, dy] = [b.x - a.x, b.y - a.y];
  const length = Math.hypot(dx, dy) || 1;
  return Array.from({ length: count }, (_, i) => {
    const along = (i - (count - 1) / 2) * 10;
    return { x: (a.x + b.x) / 2 + (along * dx) / length, y: (a.y + b.y) / 2 + (along * dy) / length };
  });
}

// ---- Copy for the agent (#493): a prompt code writes, with no model, that stands alone for any agent.

// What is copied: a step (a Could break step, or one not confirmed) with the changed files of the parts it lights; a not-confirmed
// decision; the No part group's files.
export type AgentItem =
  | { of: "step"; step: Pick<BriefStep, "k" | "h" | "b" | "risks" | "cites" | "flags">; files: string[] }
  | { of: "decision"; text: string; flags: string[] }
  | { of: "no-part"; files: string[] };

// The prompt for `item` of PR `pr`, naming its branch while it is open: a line that says what to do, the item's words, its risks, why
// it is not confirmed (the mark's own words left out), its citations as `from: "quote"`, and its files as paths. Citations carry no
// lines, so neither does the prompt. An empty section is left out.
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
