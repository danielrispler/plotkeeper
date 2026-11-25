// The PR brief's player state (spec #548, Group 4): the brief or part catch-up being played, what it offers to ratify, the step it is
// at (`briefPlayer`), and its chapters' reading gates. `map-brief.ts` opens and draws it, `map-brief-light.ts` lights the map for its
// step and `map-brief-check.ts` answers its step's check.

import { element, failure, invoke } from "./dom.js";
import { currentScrub } from "./map.js";
import { scrubTo } from "./map-history.js";
import { watchSeen } from "./seen.js";
import { logStop } from "./stop-rules.js";
import { ratifyStep } from "./ratify.js";
import { briefPlayer, chapterGate, type Gate, playlist, storySteps } from "./map-brief-model.js";
import { catchUpIds } from "./map-catch-up.js";
import type { BriefStep, BriefView, RatifyRow, StoredCatchUp } from "./types.js";

export const player = briefPlayer({ scrubbed: () => currentScrub() !== null, backToNow: () => void scrubTo(null).catch(failure("map_at")) });
// The brief being played, as `pr_brief` returned it.
let playing: BriefView | null = null;
// The part catch-up being played: its part and the fog list it covers; null while a PR brief plays.
type CatchUp = { part: string; fog: string[]; summary?: StoredCatchUp["summary"] };
let catchUp: CatchUp | null = null;
// What the brief offers for accept or overturn (`ratify.rs`), read when it opens; a decision updates its row at once.
let ratifying: RatifyRow[] = [];

/** The brief being played, null with none. */
export const playingBrief = () => playing;
/** The part catch-up being played, null while a PR brief plays or none does. */
export const playingCatchUp = () => catchUp;
/** The rows the brief offers to ratify. */
export const ratifyRows = () => ratifying;
/** What is played from now: a brief, or a part catch-up with its brief, and what it offers to ratify. */
export function setPlaying(brief: BriefView, caught: CatchUp | null, rows: RatifyRow[]) {
  playing = brief;
  catchUp = caught;
  ratifying = rows;
}
/** Nothing is played. */
export function clearPlaying() {
  playing = null;
  catchUp = null;
  ratifying = [];
}

/** Each part's catch-up plays under a negative number of its own, which no PR has. */
export const catchUpId = catchUpIds();
/** A stop event's PR: none for a part catch-up. */
export const prOf = (pr: number) => (pr < 0 ? null : pr);

export const panel = () => element("map-brief");
export const briefOpen = () => player.current() !== null;

/** The steps the chapter played shows: its own, the overview in front where it carries it, and the ratify step at the brief's end. */
export const steps = (): BriefStep[] => {
  const now = player.current();
  if (!playing || !now) return [];
  const list = playlist(playing.brief.brief, now.chapter, now.overview);
  // The ratify step ends the last chapter (the only one of a single story), and holds Guard with a law's row when the brief offers it (#401).
  const ratify = now.chapter >= Math.max(0, playing.brief.brief.chapters.length - 1) ? ratifyStep(ratifying.filter((row) => row.status !== "ratified"), [...new Set(list.flatMap((step) => step.on))], !!playing.brief.guard) : null;
  return ratify ? [...list, ratify] : list;
};

/** The parts the playing chapter's behaviour crosses, which its frame holds even where no step names them. */
export const chapterParts = (): string[] => {
  const now = player.current();
  return playing && now ? (playing.brief.brief.chapters[now.chapter]?.parts ?? []) : [];
};

// Reading a chapter to its end (#342): one gate per chapter of a brief, kept while the window runs; it tells the server once.
const gates = new Map<string, Gate>();
// The chapters of each "Since you read" story read to their end while the window runs (#497).
const storyDone = new Map<string, Set<number>>();
// The last step's words, watched until seen.
let watching: (() => void) | null = null;
export const stopWatching = () => {
  watching?.();
  watching = null;
};
/** The reading gate of the chapter played, made at its first ask. */
export function gateOf(): Gate | null {
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
/** Watches the last step's words: once they have been seen, the chapter's reading counts. */
export function watchLast(node: Element, index: number) {
  stopWatching();
  const gate = gateOf();
  if (!gate || gate.done() || index !== storySteps(steps()).length - 1) return;
  watching = watchSeen(node, { active: () => player.current()?.step === index && panel().classList.contains("open"), onSeen: () => gate.sawLast() });
}
