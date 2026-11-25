// The part catch-up's pure logic (spec #378, ticket #391; spec #548 Group 4). Split from `map-model.ts` by concern.

import type { CatchUpOffer, CatchUpStage, Fog, FogListEntry } from "./types.js";

// ---- The part catch-up (spec #378, ticket #391): what a fogged part's sheet offers, from the server's `catch_ups`.

/**
 * The sheet's fog buttons, the main one first: the part catch-up (or, for a part fogged by one PR whose brief lights it, that chapter), then
 * the quiet "I know this". A catch-up being written stays pressable: "Writing…" never blocks the sheet or the map.
 */
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

/**
 * What a catch-up opened while it was written waits for, read from the part's offer at each map draw: "wait" while a write runs, then
 * "open" once it is stored for the part's fog, else "stop" (its write failed, the part was cleared meanwhile, or no write runs).
 */
export function awaitedCatchUp(offer: CatchUpOffer | undefined): "open" | "stop" | "wait" {
  if (offer?.kind !== "catch-up") return "stop";
  if (offer.writing) return "wait";
  return offer.ready ? "open" : "stop";
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
