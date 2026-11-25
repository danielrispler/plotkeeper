// Staleness bands and the refresher (spec danielrispler/plotkeeper#378, ticket #392), as data. The refresher is the part as the map has it
// now, drawn by code from the map file's text, never a writer call, so it shows at once: the part's sheet already holds its caption,
// purpose, held items and laws with their backing, and the refresher adds the flows through the part and its links. `map-stale.ts` draws it.

export type Refresher = {
  // Each flow with a hop on a link at the part, and those hops' sentences, in the flow's order; its id is what a micro-world is built from.
  flows: { id: string; name: string; hops: string[] }[];
  // "From to To", by name.
  links: string[];
};

type Named = { id: string; name: string };
type Flow = { id: string; name: string; hops: { link: string; text: string }[] };
const list = <T>(value: unknown): T[] => (Array.isArray(value) ? (value as T[]) : []);

// The refresher of `part` from the map file's text; null when the text does not read or has no such part.
export function refresherOf(source: string, part: string): Refresher | null {
  let file: Record<string, unknown>;
  try {
    file = JSON.parse(source);
  } catch {
    return null;
  }
  const parts = list<Named>(file.parts);
  if (!parts.some((each) => each.id === part)) return null;
  const names = new Map([...parts, ...list<Named>(file.actors)].map((each) => [each.id, each.name]));
  const at = list<{ id: string; from: string; to: string }>(file.links).filter((link) => link.from === part || link.to === part);
  const ids = new Set(at.map((link) => link.id));
  const flows = list<Flow>(file.flows)
    .map((flow) => ({ id: flow.id, name: flow.name, hops: list<Flow["hops"][number]>(flow.hops).filter((hop) => ids.has(hop.link)).map((hop) => hop.text) }))
    .filter((flow) => flow.hops.length > 0);
  return {
    flows,
    links: at.map((link) => `${names.get(link.from) ?? link.from} to ${names.get(link.to) ?? link.to}`),
  };
}

// The weeks each band starts at.
const weeks: Record<number, number> = { 1: 3, 2: 6, 3: 12 };

// What a band says on its plate and sheet: how long since Daniel last engaged with the part. Never a count of anything else.
export const staleWords = (band: number): string => `You last engaged with this part ${weeks[band] ?? 3} weeks or more ago`;
