// The Map view's pure logic (spec danielrispler/plotkeeper#306, ticket #309): the map file as the window reads it, where parts
// are placed, the drag threshold, the link's rim points, the layered Esc, the part sheet and the left panel. `map.ts` draws and wires them. By concern
// (spec #548, Group 4), the camera is `map-camera.ts`, the PR brief's logic `map-brief-model.ts` and the part catch-up's `map-catch-up.ts`.

import { messageOf } from "./dom.js";
import { isObject } from "./settings.js";
import type { Ci, CommitFiles, Fog, FogEntry, HistoricalMap, HistoryEntry, MapDiff, MapTag, Proof } from "./types.js";

export type Tier = "core" | "interface" | "black-box";
export type Point = { x: number; y: number };
export type Size = { w: number; h: number };
export type Rect = { x0: number; y0: number; x1: number; y1: number };
// A part of a version 2 map (spec #473): a short purpose and the named things it holds, each with its key files. The server sends a
// version 1 map converted, so the page reads only these.
type Held = { name: string; text: string; paths?: string[] };
export type Part = { id: string; name: string; caption: string; purpose: string; holds: Held[]; tier: Tier; paths: string[]; zone?: string };
type Actor = { id: string; name: string };
/**
 * A link's kind (spec #473): `import`, a code dependency the check proves, or a run-time kind with a one-sentence reason. A version 1
 * map's links have none.
 */
export type LinkKind = "import" | "http" | "spawn" | "file";
export type MapLink = { id: string; from: string; to: string; kind?: LinkKind; reason?: string };
export type Law = { id: string; on: string; backing: "check" | "trust"; text: string };
export type MapDoc = { ok: true; parts: Part[]; actors: Actor[]; links: MapLink[]; laws: Law[] } | { ok: false; why: string };

/** The sizes the stylesheet draws, `--plate-w` and the rest of tokens.css; a test holds them equal. */
export const plateSize: Size = { w: 264, h: 76 };
export const actorSize: Size = { w: 120, h: 28 };
/** The size drawn for `id`: an actor's, else a plate's. */
export const sizeOf = (doc: { actors: { id: string }[] }, id: string): Size => (doc.actors.some((actor) => actor.id === id) ? actorSize : plateSize);
/** How far the pointer moves before a press on a plate becomes a drag, so a click never nudges it. */
export const dragThreshold = 6;
/**
 * The layers Esc peels, first to last (user story 5, spec #334 and spec #380): an overlay over the map (a micro-world, #399, then the pre-flight,
 * #394; spec #378, "Layers and keys"), the patch review (#398), a session badge's list (#562), a drawer, a brief's "not confirmed" popover (#491), a PR brief (closing it keeps its step), a scrubbed past commit (back to now), a part's sheet, then the left panel (#385). The
 * tickets that add them register a `Layer` under its name in `map.ts`; the Map view itself is left only when none is open.
 */
export const layerOrder = ["micro-world", "pre-flight", "patch-review", "badges", "drawer", "popover", "brief", "scrubbed", "sheet", "panel"] as const;
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

/** The map file's text as the window draws it. The server already read it leniently, so an unknown field is ignored here too. */
export function parseMap(text: string): MapDoc {
  let file: unknown;
  try {
    file = JSON.parse(text);
  } catch (e) {
    return { ok: false, why: messageOf(e) };
  }
  if (!isObject(file)) return { ok: false, why: "the map is not a JSON object" };
  const [parts, actors, links, laws] = [listOf(file.parts, isPart), listOf(file.actors, isActor), listOf(file.links, isLink), listOf(file.laws, isLaw)];
  if (!parts || !actors || !links || !laws) return { ok: false, why: "the map lacks its parts, actors, links or laws" };
  return { ok: true, parts, actors, links, laws };
}

/** Daniel's tier for a part beats the map's own; a choice that is no tier is ignored. */
export const effectiveTier = (part: Part, chosen: Record<string, string>): Tier => {
  const own = chosen[part.id] as Tier | undefined;
  return own && tiers.includes(own) ? own : part.tier;
};

// The gap kept between plates, and the corner of the world they start from.
const gap = 48;
const corner: Point = { x: plateSize.w / 2 + gap, y: plateSize.h / 2 + gap };
const start: Point = { x: 640, y: 360 };
const directions: Point[] = [0, 90, 180, 270, 45, 135, 225, 315].map((degrees) => ({ x: Math.cos((degrees * Math.PI) / 180), y: Math.sin((degrees * Math.PI) / 180) }));

/**
 * Where each item without a saved position goes, once: never an item already placed. An item is put near what it links to, else near
 * the others of its `zone`, else near the middle of the plates there are, on the nearest spot clear of every plate. The one with
 * the most placed neighbours goes first, so a map grows outwards from what is placed; ties go in the map's order. Pure, so the
 * same map and positions always give the same places.
 */
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

/**
 * A press on a plate that may become a drag: the pointer where it began, the plate's centre then, and whether the pointer has
 * moved the threshold's distance yet.
 */
export type Drag = { id: string; from: Point; origin: Point; moved: boolean };
export const dragStart = (id: string, from: Point, origin: Point): Drag => ({ id, from, origin, moved: false });

/**
 * The pointer at `at`: where the plate goes, null while the press is still a click. Once past the threshold the plate follows the
 * pointer exactly, even back over the start.
 */
export function dragMove(drag: Drag, at: Point): { drag: Drag; to: Point | null } {
  const moved = drag.moved || Math.hypot(at.x - drag.from.x, at.y - drag.from.y) >= dragThreshold;
  return { drag: { ...drag, moved }, to: moved ? { x: drag.origin.x + at.x - drag.from.x, y: drag.origin.y + at.y - drag.from.y } : null };
}

/**
 * Where the line from the centre of a box of `size` towards `toward` leaves the box: a link runs rim to rim. The centre itself
 * when `toward` is on it.
 */
export function rimPoint(center: Point, size: Size, toward: Point): Point {
  const [dx, dy] = [toward.x - center.x, toward.y - center.y];
  if (dx === 0 && dy === 0) return center;
  const reach = Math.min(dx === 0 ? Infinity : size.w / 2 / Math.abs(dx), dy === 0 ? Infinity : size.h / 2 / Math.abs(dy));
  return { x: center.x + dx * reach, y: center.y + dy * reach };
}

/** A project switch: closes the layers that belong to the project shown, whatever the next one draws, since it may have no map to draw. */
export function switchProject(layers: Partial<Record<LayerName, Layer>>) {
  for (const name of ["micro-world", "pre-flight", "patch-review", "badges", "popover", "brief"] as const) if (layers[name]?.open()) layers[name].close();
}

/**
 * Esc: closes the first layer that is open, in `layerOrder`, and says which; null when none was: on the bare map Esc does nothing, since the
 * map is the page's base (#386).
 */
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

/** The first prompt of "Draft a map", for a project with no `system-map.json` on its default branch. */
export const draftPrompt =
  "Draft system-map.json at this repository's root, in plain words: its parts, each with the files it owns, a purpose of one or two sentences " +
  "and the named things it holds (each with one sentence and its key files), the outside actors, the links between them, " +
  "the files that belong to no part on purpose, and the laws that must stay true. Run `system-map check` and fix what it reports until it passes. " +
  "Add the extractors its `note:` lines suggest to `extractors`.";

// The part sheet (ticket #310) as data: what `map-sheet.ts` draws for one part.
const tierLabel: Record<Tier, string> = { core: "Core", interface: "Interface-only", "black-box": "Black box" };
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

/**
 * Why a part's plate carries its marks, at the top of its sheet (#495), from the facts of this case: its newest thick fog entry (else its
 * newest) with its PR and why, and how many more; its staleness band with its last engagement (the server's line); a broken law, never
 * named. Null where the part has no such mark.
 */
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

/**
 * The sheet's "Talks to" (spec #473): each link at either end of `part`, in the map's order, with the other end by name, whether its
 * arrow leaves the part, its kind and its reason. The sheet has no other list of links.
 */
export type TalkRow = { link: string; other: string; out: boolean; kind?: LinkKind | undefined; reason?: string | undefined };
export const talksTo = (part: string, links: MapLink[], nameOf: (id: string) => string): TalkRow[] =>
  links
    .filter((link) => link.from === part || link.to === part)
    .map((link) => ({ link: link.id, other: nameOf(link.from === part ? link.to : link.from), out: link.from === part, kind: link.kind, reason: link.reason }));

/**
 * The map's lines (spec #473): links joining the same two ends, in either direction, are one line, drawn the way its first link runs and
 * holding every link's id, so a brief's light and a law's mark on any of them find it.
 */
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

/** A plate's hover text: its name, caption and purpose, never the held items, so a glance stays a glance. */
export const plateTitle = (part: Part): string => [part.name, part.caption, part.purpose].filter(Boolean).join("\n\n");

/**
 * Broken laws and the drawer (ticket #312). The drawer is read-only and holds any content a ticket gives it: blocks of code or text and
 * the one-click jumps out of it. `openDrawer` (map-drawer.ts) draws a `Drawer`; the history strip's commit drill-down builds one too.
 */
export type Jump = { label: string; title: string; to: { url: string } };
/**
 * A block's `first` numbers its lines from that line, and `mark` is a line to mark (#499's import site); `picks`, rows that each open
 * something else in the drawer.
 */
type Block = { label?: string; code: string; jumps: Jump[]; first?: number; mark?: number | undefined };
type DrawerPick = { label: string; title: string; open: () => void };
export type Drawer = { title: string; note?: string | undefined; blocks: Block[]; jumps: Jump[]; picks?: DrawerPick[] };

/** What the mark and the drill-down say for a broken law: coarse, never naming the law. */
export const brokenText = "A test that backs laws here failed on main";

/** What CI on the default branch said, in words. A missing red mark means only "no known failure", so each outcome says which it is. */
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

/** The parts a broken law marks: only when the run failed in a test that backs a law. */
export const brokenParts = (ci: Ci | null): Set<string> => new Set(ci?.status === "broken" ? ci.parts : []);

/**
 * What the part sheet says of CI for `part`, or null when it holds no law a check backs and is not marked: its line, whether it is
 * the loud one, and whether a drill-down has anything to show.
 */
export function sheetCi(ci: Ci | null, part: string, backed: boolean): { line: string; broken: boolean; drill: boolean } | null {
  const broken = brokenParts(ci).has(part);
  if (!broken && !backed) return null;
  return { line: ciLine(ci), broken, drill: ci !== null && ci.status !== "unknown" };
}

/**
 * The part sheet's folds (spec #633, ticket #637), from most to least used: Laws, Talks to, This session here (hidden while no session's
 * story is open beside the sheet), Holds (none when the part holds nothing), Tier, CI on main (when `ci` is shown at all) and History; CI on
 * main leads when a run breaks one of the part's laws.
 */
export type FoldKey = "laws" | "talks" | "here" | "holds" | "tier" | "ci" | "history";
export const foldTitles: Record<FoldKey, string> = { laws: "Laws", talks: "Talks to", here: "This session here", holds: "Holds", tier: "Tier", ci: "CI on main", history: "History" };
export function foldOrder(ci: { broken: boolean } | null, holds: boolean): FoldKey[] {
  const order: FoldKey[] = ["laws", "talks", "here", ...(holds ? (["holds"] as const) : []), "tier", ...(ci && !ci.broken ? (["ci"] as const) : []), "history"];
  return ci?.broken ? ["ci", ...order] : order;
}

/** `n` and the word for it, `one` or `many`. */
export const counted = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
// CI on main in a word, beside its fold's title and on its chip.
const ciWord = (ci: Ci | null): string => {
  switch (ci?.status) {
    case "passed": return "passing";
    case "broken": return "failing";
    case "not-run":
    case "unreadable": return "failed";
    default: return "not read";
  }
};
/**
 * The short values beside the folds' titles that are known when the sheet is drawn (History's rows load after, so it has none), and the
 * chips' words.
 */
export function sheetGlances(sheet: Sheet, links: number, ci: Ci | null) {
  const tested = sheet.laws.filter((law) => law.backing === "check").length;
  const tier = sheet.tiers.find((each) => each.pressed)?.label ?? "";
  return {
    folds: {
      laws: sheet.laws.length ? `${sheet.laws.length} · ${tested} tested` : "None",
      talks: links ? counted(links, "link", "links") : "None",
      holds: counted(sheet.holds.length, "thing", "things"),
      tier,
      ci: ciWord(ci),
    },
    chips: {
      tier,
      laws: sheet.laws.length ? counted(sheet.laws.length, "law", "laws") : "No laws",
      talks: links ? counted(links, "link", "links") : "No links",
      ci: `CI ${ciWord(ci)}`,
    },
  };
}

/** "See the proof" for a law: each tagged test's code from the tag on. The page has no editor to jump to. */
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

/**
 * The broken-law drill-down: the run's status, a note when a newer run is going, the failing log's tail, and a jump to the run on GitHub.
 */
export function ciDrawer(ci: Ci): Drawer {
  return {
    title: ciLine({ ...ci, earlier: false }),
    note: ci.earlier ? "This is the last finished run, for an earlier commit; a newer run is going." : undefined,
    blocks: ci.tail ? [{ label: "End of the failed log", code: ci.tail, jumps: [] }] : [],
    jumps: ci.run_url ? [{ label: "Open on GitHub", title: "Open this run on GitHub", to: { url: ci.run_url } }] : [],
  };
}

/** The history strip and time travel (ticket #313). A scrub is a past commit's map shown read-only. */
export type Scrub = Pick<HistoricalMap, "commit" | "short" | "label"> & { source: string };
export type StripRow = HistoryEntry & { current: boolean; title: string };

/** What a commit of the history strip offers of its PR's stored brief: a quiet "Brief" button, none when the PR has none. */
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

/** The quiet banner while a past commit is shown: what it says and the word of the way back. */
export const scrubBanner = (scrub: Pick<Scrub, "short">): { text: string; back: string } => ({ text: `Viewing ${scrub.short}`, back: "Back to now" });

/** A commit's drill-down: each file it changed with its diff, and the commit on GitHub when the project is there. */
export function commitDrawer(found: CommitFiles): Drawer {
  const count = `${found.files.length} ${found.files.length === 1 ? "file" : "files"}`;
  return {
    title: found.label,
    note: `${found.short} · ${count}`,
    blocks: found.files.length ? found.files.map((file) => ({ label: file.path, code: file.diff, jumps: [] })) : [{ code: "No files changed.", jumps: [] }],
    jumps: found.url ? [{ label: "Open on GitHub", title: "Open this commit on GitHub", to: { url: found.url } }] : [],
  };
}

/**
 * Tells which of a series of scrubs is the newest. `start` takes a ticket; `cancel` (Back to now) makes every ticket out stale; a
 * scrub that reads slowly lands only while its ticket is still `live`, so it never overrides what Daniel did after it.
 */
export function scrubTickets() {
  let latest = 0;
  return {
    start: () => ++latest,
    cancel: () => void ++latest,
    live: (ticket: number) => ticket === latest,
  };
}

// ---- The left panel (spec #380, ticket #385): one slot over the map's left side for a session's story, History, a turn's diff (widened) or
/** Settings, beside the part's sheet on the right. */
export type PanelContent =
  | { kind: "story"; session: string }
  | { kind: "history" }
  | { kind: "diff"; session: string; turn: number }
  | { kind: "settings" }
  // A worker's or helper's log (#386): it had the centre column, which the map replaced.
  | { kind: "log"; session: string; agent: string };
export type Overlays = { panel: PanelContent | null; sheet: string | null };
/** Below this window width only one overlay is open at a time (user story 52). */
export const narrowBelow = 1007;
/** Opens `panel` (null closes it). Below `narrowBelow`, or for the widened diff, the part's sheet closes (user stories 29, 52). */
export const withPanel = (now: Overlays, panel: PanelContent | null, width: number): Overlays => ({
  panel,
  sheet: panel && (width < narrowBelow || panel.kind === "diff") ? null : now.sheet,
});
/** Opens the sheet of part `sheet` (null closes it). Below `narrowBelow`, or over the widened diff, the panel closes. */
export const withSheet = (now: Overlays, sheet: string | null, width: number): Overlays => ({
  sheet,
  panel: sheet && (width < narrowBelow || now.panel?.kind === "diff") ? null : now.panel,
});
/** A press on a session's row (the list's or a badge list's): its story, or none when the panel already shows that story (a second click closes it). */
export const pressRow = (panel: PanelContent | null, session: string): PanelContent | null =>
  panel?.kind === "story" && panel.session === session ? null : { kind: "story", session };
/** The session the panel shows (its story, a turn's diff or a worker's log), which is spotlit while it is open, and whose closing selects no session. */
export const panelSession = (panel: PanelContent | null): string | null =>
  panel?.kind === "story" || panel?.kind === "diff" || panel?.kind === "log" ? panel.session : null;
/**
 * The project the page opens on (#386): the last viewed, while the saved list still holds it, else the first of those the session list
 * shows (`listed`: not a gone or temporary one, spec #659); none on a fresh install.
 */
export const startProject = (last: string | null, projects: string[], listed: string[]): string | null =>
  last !== null && projects.includes(last) ? last : (listed[0] ?? null);
/** What the page shows (#386): a project's map, and what the left panel holds over it. */
export type PageView = { project: string | null; panel: PanelContent | null };
/** Selecting a session shows its project's map, with its story, or a worker's log, in the left panel. */
export const selectedView = (session: string, project: string, agent: string | null): PageView => ({
  project,
  panel: agent === null ? { kind: "story", session } : { kind: "log", session, agent },
});
/**
 * A press on a project's name shows that project's map, the shown one's included; a session's panel of another project closes with the
 * switch. Folding the group is the header's chevron (#674).
 */
export function pressHeader(page: PageView, pressed: string, projectOf: (session: string) => string | null): PageView {
  const session = panelSession(page.panel);
  return { project: pressed, panel: session === null || projectOf(session) === pressed ? page.panel : null };
}

/**
 * The parts' tab order (#460): top to bottom, then left to right. A plate within half a plate's height of a row's first (highest) plate
 * reads on that row, so one dragged a little out of line keeps its place.
 */
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

/**
 * Where the law marks of a line from `a` to `b` sit: `count` points along it, 10px apart, centred on its middle, so several never stack.
 */
export function markPoints(a: Point, b: Point, count: number): Point[] {
  const [dx, dy] = [b.x - a.x, b.y - a.y];
  const length = Math.hypot(dx, dy) || 1;
  return Array.from({ length: count }, (_, i) => {
    const along = (i - (count - 1) / 2) * 10;
    return { x: (a.x + b.x) / 2 + (along * dx) / length, y: (a.y + b.y) / 2 + (along * dy) / length };
  });
}

/**
 * Where each item is drawn (#640): its saved spot, else one placed once by `autoPlace`, saved (`save`) so it never moves on its own. The
 * `unsaved` ids are placed but never saved: a Map diff's ghosts, and every item of a past commit's map.
 */
export function placement(items: { id: string; zone?: string }[], links: { from: string; to: string }[], saved: Record<string, Point>, unsaved: string[]): { at: Record<string, Point>; save: Record<string, Point> } {
  const added = autoPlace(items, links, saved);
  const save = Object.fromEntries(Object.entries(added).filter(([id]) => !unsaved.includes(id)));
  return { at: { ...saved, ...added }, save };
}
/** A Map diff's tag of the line between `from` and `to`, either way round; none for an untagged line or with no diff. */
export const lineTag = (diff: Pick<MapDiff, "lines"> | null, from: string, to: string): MapTag | undefined => {
  const [a, b] = [from, to].sort();
  return diff?.lines.find((line) => line.ends[0] === a && line.ends[1] === b)?.change;
};
/** The word a Map diff's tag writes on a plate's edge. */
export const tagWord = (tag: MapTag): string => ({ added: "New", removed: "Gone", edited: "Edited" })[tag];
