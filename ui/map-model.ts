// The Map view's pure logic (spec danielrispler/plotkeeper#306, ticket #309): the map file as the window reads it, where parts
// are placed, the drag threshold, the link's rim points and the layered Esc. `map.ts` draws and wires them.

export type Tier = "core" | "interface" | "black-box";
export type Point = { x: number; y: number };
export type Size = { w: number; h: number };
export type Part = { id: string; name: string; caption: string; description: string; tier: Tier; paths: string[]; zone?: string };
export type Actor = { id: string; name: string };
export type MapLink = { id: string; from: string; to: string };
export type Law = { id: string; on: string; backing: "check" | "trust"; text: string };
export type MapDoc = { ok: true; parts: Part[]; actors: Actor[]; links: MapLink[]; laws: Law[] } | { ok: false; why: string };

// The sizes the stylesheet draws, `--plate-w` and the rest of tokens.css; a test holds them equal.
export const plateSize: Size = { w: 264, h: 76 };
export const actorSize: Size = { w: 120, h: 28 };
// How far the pointer moves before a press on a plate becomes a drag, so a click never nudges it.
export const dragThreshold = 6;
// The layers Esc peels, first to last (user story 5): a scrubbed past commit, a drawer, a part's sheet. The tickets that add
// them register a `Layer` under its name in `map.ts`; the Map view itself is left only when none is open.
export const layerOrder = ["scrubbed", "drawer", "sheet"] as const;
export type LayerName = (typeof layerOrder)[number];
export type Layer = { open: () => boolean; close: () => void };

const tiers: Tier[] = ["core", "interface", "black-box"];
const list = <T>(value: unknown): T[] | null => (Array.isArray(value) ? (value as T[]) : null);

// The map file's text as the window draws it. The shell already read it leniently, so an unknown field is ignored here too.
export function parseMap(text: string): MapDoc {
  let file: Record<string, unknown>;
  try {
    file = JSON.parse(text);
  } catch (e) {
    return { ok: false, why: String(e) };
  }
  const [parts, actors, links, laws] = [list<Part>(file.parts), list<Actor>(file.actors), list<MapLink>(file.links), list<Law>(file.laws)];
  if (!parts || !actors || !links || !laws) return { ok: false, why: "the map lacks its parts, actors, links or laws" };
  return { ok: true, parts, actors, links, laws };
}

// Daniel's tier for a part beats the map's own; a choice that is no tier is ignored.
export const effectiveTier = (part: Part, chosen: Record<string, string>): Tier => {
  const own = chosen[part.id] as Tier | undefined;
  return own && tiers.includes(own) ? own : part.tier;
};

// The project the Map view opens on: the selected session's, else the first project with a map, else the first project.
export const mapProject = (selected: string | null, paths: string[], withMap: Set<string>): string | null =>
  selected ?? paths.find((path) => withMap.has(path)) ?? paths[0] ?? null;

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
    const anchors = [...near, ...zoned].map((id) => all[id]);
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

// Esc: closes the first layer that is open, in `layerOrder`, and says which; "leave" when none was, so the Map view is left.
export function escapeMap(layers: Partial<Record<LayerName, Layer>>): LayerName | "leave" {
  for (const name of layerOrder) {
    const layer = layers[name];
    if (layer?.open()) {
      layer.close();
      return name;
    }
  }
  return "leave";
}

// The first prompt of "Draft a map", for a project with no `system-map.json` on its default branch.
export const draftPrompt =
  "Draft system-map.json at this repository's root, in plain words: its parts, each with the files it owns, the outside actors, the links between them, " +
  "the files that belong to no part on purpose, and the laws that must stay true. Run `system-map check` and fix what it reports until it passes.";
