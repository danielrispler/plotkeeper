// Always-on stop-rule tracking (spec danielrispler/plotkeeper#306, ticket #314): the window tells the shell what Daniel uses, the shell
// keeps the log in the data folder (`server/src/stop_rules.rs`) and counts it, and this file decides, with no number ever shown, whether
// the one-time review is due and which modules fell below their threshold. Nothing here runs on a timer: the events are the ones that
// already happen (a session list, the Map view opening, an entered action on the map, a scrub, a drawer, "I know this") and the evaluation
// runs when the Map view opens. The review shows on the map only (spec #380, "Stop rules").

import { failure, invoke, text } from "./dom.js";
import type { StopFacts, StopKind } from "./types.js";

export type Facts = StopFacts;
// A module of the review: a row of `table` (or of a later ticket's rows). "skips" is a note, not a module: "I know this" cleared more fog
// than finished brief chapters did.
export type Module = string;

// The review is due once both hold, whichever comes last.
export const dueAfter = { days: 14, prs: 20 };

// One module's row: its words in the review, its "kept if" bar over the shell's counts, and an optional exception before which it is not
// judged at all: a calendar wait of `weeks` from its rollout (`from`, a local day, or read from the project's facts, null while it has
// none; a project whose log began later waits from its own start), or `least` offers of `kind` logged for the project. A note row
// (`note`) has its own sentence instead of a name in the list.
export type Row = {
  module: Module;
  name: string;
  kept: (facts: Facts) => boolean;
  note?: string;
  wait?: { from: string | ((facts: Facts) => string | null); weeks: number };
  offers?: { kind: StopKind; least: number };
};

// Where the refresher's and staleness bands' calendar wait starts (#392): the local day of the project's staleness baseline, its first
// map shown with that build (`server/src/staleness.rs`); none before it.
const staleRollout = (f: Facts) => (f.stale_since === null ? null : localDay(new Date(f.stale_since * 1000)));

// The modules and their bars, in the order the review lists them. Map view: engaged days, active days with at least one entered action on
// the map (the shell counts them; a day the map was only open on is not one). History strip: scrubs. Proof: opens. PR briefs: opened for at
// least a quarter of the merged PRs that had one; with none offered there is nothing to judge, so it is kept.
export const table: Row[] = [
  { module: "map", name: "the Map view", kept: (f) => f.engaged_days >= 4 },
  { module: "history", name: "the history strip", kept: (f) => f.scrubs >= 2 },
  { module: "proof", name: "See the proof", kept: (f) => f.proofs >= 2 },
  { module: "briefs", name: "PR briefs", kept: (f) => f.briefs_offered === 0 || f.briefs_opened >= f.briefs_offered * 0.25 },
  // Pre-flight (#394, spec #378 "Stop rules"): run at least twice, from an issue, from text or by hand.
  { module: "pre-flight", name: "Pre-flight", kept: (f) => (f.counts["pre-flight-run"] ?? 0) >= 2 },
  {
    module: "skips",
    name: "",
    note: "“I know this” cleared more fog than finishing a brief's chapters did.",
    kept: (f) => f.briefs_offered === 0 || f.knows <= f.chapters_done,
  },
  // Part catch-ups (#391): completed at least 3 times; "I know this" is counted apart (`know`).
  { module: "catch-ups", name: "part catch-ups", kept: (f) => (f.counts["catch-up-done"] ?? 0) >= 3 },
  // The refresher and staleness bands (#392): a part turns stale 3 weeks after rollout at the earliest, so neither is judged before 5
  // calendar weeks. "stale-entered" is a refresher or a part catch-up entered from a part with a band.
  { module: "refresher", name: "the refresher", kept: (f) => (f.counts["refresher-entered"] ?? 0) >= 2, wait: { from: staleRollout, weeks: 5 } },
  { module: "staleness", name: "staleness bands", kept: (f) => (f.counts["stale-entered"] ?? 0) >= 1, wait: { from: staleRollout, weeks: 5 } },
  // Explain-back (#397): at least 3 lines said back in the end-of-story bar; the marks, the rating and the note have no threshold.
  { module: "explain-back", name: "explain-back", kept: (f) => (f.counts["explain-back-given"] ?? 0) >= 3 },
  // Micro-worlds (#399): a checked page opened in its overlay at least twice.
  { module: "micro-worlds", name: "micro-worlds", kept: (f) => (f.counts["micro-world-opened"] ?? 0) >= 2 },
];

// `day` plus `n` days, both local days (`2026-11-03`).
const addDays = (day: string, n: number) => {
  const [y, m, d] = day.split("-").map(Number);
  return localDay(new Date(y, m - 1, d + n));
};

// Whether `row` may be judged on `today`: past its calendar wait and its offer count, if it has them.
function judgeable(row: Row, facts: Facts, today: string): boolean {
  if (row.wait) {
    const rollout = typeof row.wait.from === "string" ? row.wait.from : row.wait.from(facts);
    if (rollout === null) return false;
    const from = facts.began !== null && facts.began > rollout ? facts.began : rollout;
    if (today < addDays(from, row.wait.weeks * 7)) return false;
  }
  return !row.offers || (facts.counts[row.offers.kind] ?? 0) >= row.offers.least;
}

// The modules of `rows` below their bar once the review is due, in table order, leaving out each one dismissed and each one still in its
// exception; null while the review is not due. An empty list means nothing to show (the log goes on).
export function judge(rows: Row[], facts: Facts, today: string): Module[] | null {
  if (facts.active_days < dueAfter.days || facts.merged_prs < dueAfter.prs) return null;
  return rows.filter((row) => !facts.dismissed.includes(row.module) && judgeable(row, facts, today) && !row.kept(facts)).map((row) => row.module);
}

// `judge` over this build's table, today.
export const unused = (facts: Facts, today: string = localDay()) => judge(table, facts, today);

// The review's words: the unused modules, then each note that applies. Never a score or a count.
export function reviewWords(modules: Module[], rows: Row[] = table): string {
  const shown = rows.filter((row) => modules.includes(row.module));
  const list = shown.filter((row) => !row.note).map((row) => row.name);
  const joined = list.length > 1 ? `${list.slice(0, -1).join(", ")} and ${list.at(-1)}` : list[0];
  const used = list.length ? `Little used so far: ${joined}. Worth pruning?` : "";
  return [used, ...shown.map((row) => row.note ?? "")].filter(Boolean).join(" ");
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

// What the shell counted for `project`, and the modules it is short on (null: not due).
export const reviewOf = async (project: string) => unused(await invoke("stop_facts", { project }));

// Dismisses the review of `modules` in each project for good; a module not named is still reviewed when it falls due.
export const dismissReview = (projects: string[], modules: Module[]) => invoke("stop_dismiss", { projects, modules, day: localDay() });

// The review's line with its one button, for the Map view. Pressing "Dismiss" dismisses the modules shown and calls `dismissed` after the
// shell has it.
export function reviewNode(modules: Module[], projects: string[], dismissed: () => void): HTMLElement {
  const node = text("div", "", "review");
  const button = text("button", "Dismiss", "btn quiet small");
  button.title = "Do not show this again";
  button.addEventListener("click", () => {
    dismissReview(projects, modules).then(dismissed).catch(failure("stop_dismiss"));
  });
  node.append(text("span", reviewWords(modules), "review-text"), button);
  return node;
}

