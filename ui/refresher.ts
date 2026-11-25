// Staleness bands and the refresher (spec danielrispler/plotkeeper#378, ticket #392), as data. The refresher is the part as the map has it
// now, drawn by code from the map file's text, never a writer call, so it shows at once: the part's sheet already holds its caption,
// purpose, held items, laws and "Talks to" (spec #473), and the refresher adds the flows through the part. `map-stale.ts` draws it.

import { isObject } from "./settings.js";

export type Refresher = {
  // Each flow with a hop on a link at the part, and those hops' sentences, in the flow's order; its id is what a micro-world is built from.
  flows: { id: string; name: string; hops: string[] }[];
};

// The map file's entries the refresher reads, each checked for the fields it reads; an entry without them is left out, as a missing
// list is.
type Hop = { link: string; text: string };
const named = (value: unknown, fields: string[]): value is Record<string, unknown> => isObject(value) && fields.every((field) => typeof value[field] === "string");
const isPart = (value: unknown): value is { id: string } => named(value, ["id"]);
const isLink = (value: unknown): value is { id: string; from: string; to: string } => named(value, ["id", "from", "to"]);
const isFlow = (value: unknown): value is { id: string; name: string; hops: unknown } => named(value, ["id", "name"]);
const isHop = (value: unknown): value is Hop => named(value, ["link", "text"]);
const listOf = <T>(value: unknown, is: (entry: unknown) => entry is T): T[] => (Array.isArray(value) ? value.filter(is) : []);

// The refresher of `part` from the map file's text; null when the text does not read or has no such part.
export function refresherOf(source: string, part: string): Refresher | null {
  let file: unknown;
  try {
    file = JSON.parse(source);
  } catch {
    return null;
  }
  if (!isObject(file)) return null;
  const parts = listOf(file.parts, isPart);
  if (!parts.some((each) => each.id === part)) return null;
  // The ids of the links at the part: a hop on one of them crosses it.
  const ids = new Set(listOf(file.links, isLink).flatMap((link) => (link.from === part || link.to === part ? [link.id] : [])));
  const flows = listOf(file.flows, isFlow)
    .map((flow) => ({ id: flow.id, name: flow.name, hops: listOf(flow.hops, isHop).filter((hop) => ids.has(hop.link)).map((hop) => hop.text) }))
    .filter((flow) => flow.hops.length > 0);
  return { flows };
}

// The weeks each band starts at.
const weeks: Record<number, number> = { 1: 3, 2: 6, 3: 12 };

// What a band says on its plate and sheet: how long since Daniel last engaged with the part. Never a count of anything else.
export const staleWords = (band: number): string => `You last engaged with this part ${weeks[band] ?? 3} weeks or more ago`;
