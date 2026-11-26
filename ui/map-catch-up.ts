// The part catch-up's pure logic (spec #378, ticket #391; spec #548 Group 4). Split from `map-model.ts` by concern.

import type { CatchUpOffer, CatchUpStage, Fog, FogEntry, FogListEntry } from "./types.js";

// ---- The part catch-up (spec #378, ticket #391): what a fogged part's sheet offers, from the server's `catch_ups`.

/**
 * The catch-up card's buttons (#636), the main one first: the part catch-up (or, for a part fogged by one PR whose brief lights it, that
 * chapter), then the quiet "I know this". A ready catch-up's says how many steps its outline has. A catch-up being written stays pressable:
 * the call that runs now (`stageLabel`) never blocks the sheet or the map.
 */
export type FogAction = { label: string; title: string; note: string | null; disabled: boolean; open: { kind: "catch-up" } | { kind: "chapter"; pr: number; chapter: number } };
export function fogActions(fog: Fog | undefined, offer: CatchUpOffer | undefined): { main: FogAction | null; second: "I know this" | null } {
  if (!fog) return { main: null, second: null };
  if (!offer) return { main: null, second: "I know this" };
  const main: FogAction =
    offer.kind === "chapter"
      ? { label: `Read #${offer.pr}'s brief`, title: "Open the chapter of this PR's brief that covers this part", note: null, disabled: false, open: { kind: "chapter", pr: offer.pr, chapter: offer.chapter } }
      : {
          label: offer.outline ? `Catch up · ${offer.outline.steps.length} ${offer.outline.steps.length === 1 ? "step" : "steps"}` : "Catch up",
          title: "One story of everything that landed here since you last caught up; reading it to the end clears this part's fog",
          note: offer.writing ? stageLabel(offer.stage) : offer.failed ? "The last write failed; press to try again" : null,
          disabled: false,
          open: { kind: "catch-up" },
        };
  return { main, second: "I know this" };
}

/**
 * What a catch-up opened while it was written waits for, read from the part's offer at each map draw: "wait" while a write runs, then
 * "open" once it is stored for the part's fog, else "stop" (its write failed, the part was cleared meanwhile, or no write runs).
 */
export function awaitedCatchUp(offer: CatchUpOffer | undefined): "open" | "stop" | "wait" {
  if (offer?.kind !== "catch-up") return "stop";
  if (offer.writing) return "wait";
  return offer.ready ? "open" : "stop";
}

/** The catch-up card's eyebrow (#636): how many fog entries landed since Daniel last caught up on the part. */
export const changesLine = (n: number): string => `${n} ${n === 1 ? "change" : "changes"} since you last caught up`;

/**
 * What "Read the one you have" (#638) says the older catch-up covers, of the part's fog entries (`entries`): how many of them, then each, a PR
 * by its number and a direct push by its short commit, in the catch-up's order.
 */
export function olderLine(older: { fog: string[] }, entries: FogEntry[]): string {
  const names = older.fog.map((commit) => {
    const pr = entries.find((e) => e.commit === commit)?.pr;
    return pr == null ? commit.slice(0, 7) : `#${pr}`;
  });
  return `Covers ${older.fog.length} of ${entries.length} ${entries.length === 1 ? "change" : "changes"}: ${names.join(", ")}`;
}

/**
 * What the waiting card says of a part catch-up's write (#450): the call that runs now, "Writing…" until the server says.
 * A fixing stage's count covers every flagged item, the code checks' (a long label or body) as well as the claim check's,
 * so it says "problems", not "claims".
 */
export function stageLabel(stage: CatchUpStage | undefined): string {
  if (stage?.kind === "checking") return "Checking claims…";
  if (stage?.kind === "fixing") return `Fixing ${stage.claims} ${stage.claims === 1 ? "problem" : "problems"}…`;
  return "Writing…";
}

/**
 * The number a part catch-up plays under in `briefPlayer`: a negative one, which no PR has, one per project, part and fog list, so a
 * catch-up rewritten for a new fog list starts at its first step rather than where the old story was left.
 */
export function catchUpIds(): (project: string, part: string, fog: string[]) => number {
  const ids = new Map<string, number>();
  return (project, part, fog) => {
    const key = `${project}|${part}|${fog.join(",")}`;
    const id = ids.get(key) ?? -(ids.size + 1);
    ids.set(key, id);
    return id;
  };
}

/**
 * A long part catch-up's full fog list, one line per entry, oldest first: a PR by its number and title, a direct push by its subject and
 * short commit.
 */
export function fogListLines(entries: FogListEntry[]): string[] {
  return entries.map((e) => (e.pr === null ? `${e.title} (${e.commit.slice(0, 7)}, no PR)` : `#${e.pr} ${e.title}`));
}
