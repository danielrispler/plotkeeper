// Always-on stop-rule tracking (spec danielrispler/plotkeeper#306, ticket #314): the window tells the shell what Daniel uses, the shell
// keeps the log in the data folder (`server/src/stop_rules.rs`) and counts it, and this file decides, with no number ever shown, whether
// the one-time review is due and which modules fell below their threshold. Nothing here runs on a timer: the events are the ones that
// already happen (a session list, the Map view opening, Home's mark seen, a scrub, a drawer, "I know this") and the evaluation runs when the
// Map view opens and when Home is entered.

import { failure, invoke, text } from "./dom.js";
import type { StopFacts, StopKind } from "./types.js";

export type Facts = StopFacts;
export type Module = "map" | "history" | "proof";

// The review is due once both hold, whichever comes last.
export const dueAfter = { days: 14, prs: 20 };
// Map view: opened on this many active days, or on this share of the days Home's mark was seen. History strip: scrubs. Proof: opens.
export const thresholds = { mapDays: 4, markShare: 0.2, scrubs: 2, proofs: 2 };

// The modules below their threshold once the review is due, in the order they are listed; null while it is not due or after it was
// dismissed (the log goes on). An empty list means every module passed.
export function unused(facts: Facts): Module[] | null {
  if (facts.dismissed || facts.active_days < dueAfter.days || facts.merged_prs < dueAfter.prs) return null;
  const mapped = facts.map_days >= thresholds.mapDays || (facts.mark_days > 0 && facts.map_on_mark_days / facts.mark_days >= thresholds.markShare);
  const below: Module[] = [];
  if (!mapped) below.push("map");
  if (facts.scrubs < thresholds.scrubs) below.push("history");
  if (facts.proofs < thresholds.proofs) below.push("proof");
  return below;
}

const names: Record<Module, string> = { map: "the Map view", history: "the history strip", proof: "See the proof" };

// The review's one sentence: the unused modules only, never a score or a count.
export function reviewWords(modules: Module[]): string {
  const list = modules.map((m) => names[m]);
  const joined = list.length > 1 ? `${list.slice(0, -1).join(", ")} and ${list.at(-1)}` : list[0];
  return `Little used so far: ${joined}. Worth pruning?`;
}

// The projects whose review Home shows too: the ones whose Map view itself fell below its threshold, so an unused map still reaches Daniel.
export const onHome = (found: Record<string, Module[] | null>): string[] =>
  Object.entries(found).filter(([, modules]) => modules?.includes("map")).map(([project]) => project);

// A local calendar day, `2026-11-03`.
export const localDay = (at: Date = new Date()) => {
  const two = (n: number) => String(n).padStart(2, "0");
  return `${at.getFullYear()}-${two(at.getMonth() + 1)}-${two(at.getDate())}`;
};

// A kind that counts days is told once a day per project (the shell counts days, so more would only fill the log).
const dayKinds = new Set<StopKind>(["session", "map-open", "mark-seen"]);
const told = new Set<string>();

// Tells the shell that Daniel used something. Fire and forget: a failure is logged quietly, never shown.
export function logStop(kind: StopKind, project: string | null = null) {
  const day = localDay();
  if (dayKinds.has(kind)) {
    const key = `${day}|${kind}|${project}`;
    if (told.has(key)) return;
    told.add(key);
  }
  invoke("stop_log", { kind, project, day }).catch(failure("stop_log"));
}

// What the shell counted for `project`, and the modules it is short on (null: not due, or dismissed).
export const reviewOf = async (project: string) => unused(await invoke("stop_facts", { project }));

// Dismisses the review of each project for good.
export const dismissReview = (projects: string[]) => invoke("stop_dismiss", { projects, day: localDay() });

// The review's line with its one button, for the Map view and for Home. Pressing "Dismiss" calls `dismissed` after the shell has it.
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
