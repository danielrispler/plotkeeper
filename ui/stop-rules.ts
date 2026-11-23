// Always-on stop-rule tracking (spec danielrispler/plotkeeper#306, ticket #314): the window tells the shell what Daniel uses, the shell
// keeps the log in the data folder (`server/src/stop_rules.rs`) and counts it, and this file decides, with no number ever shown, whether
// the one-time review is due and which modules fell below their threshold. Nothing here runs on a timer: the events are the ones that
// already happen (a session list, the Map view opening, an entered action on the map, a scrub, a drawer, "I know this") and the evaluation
// runs when the Map view opens. The review shows on the map only (spec #380, "Stop rules").

import { failure, invoke, text } from "./dom.js";
import type { StopFacts, StopKind } from "./types.js";

export type Facts = StopFacts;
// "skips" is a note, not a module: "I know this" cleared more fog than finished brief chapters did.
export type Module = "map" | "history" | "proof" | "briefs" | "skips";

// The review is due once both hold, whichever comes last.
export const dueAfter = { days: 14, prs: 20 };
// Map view: engaged days, active days with at least one entered action on the map (the shell counts them; a day the map was only open on
// is not one). History strip: scrubs. Proof: opens. PR briefs: opened for at least this share of the merged PRs that had one.
export const thresholds = { engagedDays: 4, scrubs: 2, proofs: 2, briefShare: 0.25 };

// The modules below their threshold once the review is due, in the order they are listed; null while it is not due or after it was
// dismissed (the log goes on). An empty list means every module passed.
export function unused(facts: Facts): Module[] | null {
  if (facts.dismissed || facts.active_days < dueAfter.days || facts.merged_prs < dueAfter.prs) return null;
  const below: Module[] = [];
  if (facts.engaged_days < thresholds.engagedDays) below.push("map");
  if (facts.scrubs < thresholds.scrubs) below.push("history");
  if (facts.proofs < thresholds.proofs) below.push("proof");
  if (facts.briefs_offered > 0 && facts.briefs_opened < facts.briefs_offered * thresholds.briefShare) below.push("briefs");
  if (facts.briefs_offered > 0 && facts.knows > facts.chapters_done) below.push("skips");
  return below;
}

const names: Record<Exclude<Module, "skips">, string> = { map: "the Map view", history: "the history strip", proof: "See the proof", briefs: "PR briefs" };

// The review's words: the unused modules, then the note on skipping when it applies. Never a score or a count.
export function reviewWords(modules: Module[]): string {
  const list = modules.filter((m) => m !== "skips").map((m) => names[m as keyof typeof names]);
  const joined = list.length > 1 ? `${list.slice(0, -1).join(", ")} and ${list.at(-1)}` : list[0];
  const used = list.length ? `Little used so far: ${joined}. Worth pruning?` : "";
  const skips = modules.includes("skips") ? "“I know this” cleared more fog than finishing a brief's chapters did." : "";
  return [used, skips].filter(Boolean).join(" ");
}

// A local calendar day, `2026-11-03`.
export const localDay = (at: Date = new Date()) => {
  const two = (n: number) => String(n).padStart(2, "0");
  return `${at.getFullYear()}-${two(at.getMonth() + 1)}-${two(at.getDate())}`;
};

// A kind that counts days is told once a day per project (the shell counts days, so more would only fill the log). `map-action` is an
// entered action on the map with no event of its own: a part's sheet opened, an agent chip clicked.
const dayKinds = new Set<StopKind>(["session", "map-open", "map-action"]);
// A kind about a PR's brief that is told once per PR (the shell also keeps a brief's offer to once, across restarts).
const prKinds = new Set<StopKind>(["brief-offered", "unconfirmed-seen"]);
const told = new Set<string>();

// Tells the shell that Daniel used something; brief events name their PR. Fire and forget: a failure is logged quietly, never shown.
export function logStop(kind: StopKind, project: string | null = null, pr: number | null = null) {
  const day = localDay();
  const key = dayKinds.has(kind) ? `${day}|${kind}|${project}` : prKinds.has(kind) ? `${kind}|${project}|${pr}` : null;
  if (key !== null) {
    if (told.has(key)) return;
    told.add(key);
  }
  invoke("stop_log", { kind, project, day, pr }).catch(failure("stop_log"));
}

// What the shell counted for `project`, and the modules it is short on (null: not due, or dismissed).
export const reviewOf = async (project: string) => unused(await invoke("stop_facts", { project }));

// Dismisses the review of each project for good.
export const dismissReview = (projects: string[]) => invoke("stop_dismiss", { projects, day: localDay() });

// The review's line with its one button, for the Map view. Pressing "Dismiss" calls `dismissed` after the shell has it.
export function reviewNode(modules: Module[], projects: string[], dismissed: () => void): HTMLElement {
  const node = text("div", "", "review");
  const button = text("button", "Dismiss", "btn quiet small");
  button.title = "Do not show this again";
  button.addEventListener("click", () => {
    dismissReview(projects).then(dismissed).catch(failure("stop_dismiss"));
  });
  node.append(text("span", reviewWords(modules), "review-text"), button);
  return node;
}
