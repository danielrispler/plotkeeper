// What a snap fixture is, and the replies every fixture starts from (#144). A fixture maps a command of the window's
// `Commands` (ui/types.ts) to its reply, typed by that map, so `tsc -p scripts/snap` fails on a reply that lacks a field.
// The stub answers a command a fixture leaves out with `null`, which only a `void` command takes: `base` answers every
// other command, and adding one to the map without a reply here fails `tsc` too.

import type { Command, Commands, Selection } from "../../ui/types.ts";

// A reply that depends on one argument: { __by: "id", values: { <id>: reply } }.
type ByArg<T> = { __by: string; values: Record<string, T> };

// The commands whose reply the window reads.
type ReadCommand = { [C in Command]: Commands[C] extends void ? never : C }[Command];

// Beside the replies, the harness's own keys: errors for the log and a script that runs once the UI has drawn (page.mjs).
export type Fixture = { [C in Command]?: Commands[C] | ByArg<Commands[C]> } & { appErrors?: string[]; snapScript?: string; snapDone?: string; realPointer?: boolean; realKeys?: boolean; contrast?: boolean; storage?: Record<string, string> };

const noSelection: Selection | null = null;

export const base = {
  agent_summary: null,
  answer_blocked_command: "hook",
  ask: "",
  ask_anchor: "",
  ask_facts: null,
  ask_why_not: { facts: { sections: [] }, reasons: null },
  cards: { brief: null, goal: null, answers: [], open: [], design: [], decisions: [], risk_notes: [], decision_notes: [] },
  check_output: null,
  failed_check_name: "",
  map_place: {},
  map_save_positions: { positions: {}, tiers: {} },
  map_set_tier: { positions: {}, tiers: {} },
  map_know: { positions: {}, tiers: {} },
  map_refresher: { positions: {}, tiers: {} },
  map_history: [],
  map_at: { commit: "", short: "", label: "", source: "" },
  map_commit: { commit: "", short: "", label: "", url: null, files: [] },
  map_marks: [],
  stop_log: null,
  stop_facts: { active_days: 0, merged_prs: 0, engaged_days: 0, scrubs: 0, proofs: 0, knows: 0, dismissed: [], briefs_offered: 0, briefs_opened: 0, chapters_done: 0, counts: {}, began: null, stale_since: null },
  stop_dismiss: null,
  map_proof: [],
  map_state: { project: "/w/plotkeeper", status: "none", source: null, reference: "origin/main", commit: null, note: null, user: { positions: {}, tiers: {} }, fog: {}, ci: null, ready: [], ratify: [], runs: { paused: null, runs: [] }, stale: {}, stale_why: {}, catch_ups: {} },
  open: noSelection,
  pr_brief: null,
  part_catch_up: null,
  pr_diff: { commit: "", short: "", label: "", url: null, files: [] },
  pre_flight_issues: [{ number: 394, title: "Studio: pre-flight — predicted parts, and the brief's expected-against-touched line" }],
  pre_flight_predict: { issue: 394, title: "Studio: pre-flight", parts: ["core"], predicted: true, at: 0 },
  pre_flight_pick: { issue: null, title: "", parts: ["core"], predicted: false, at: 0 },
  pre_flight_clear: null,
  micro_world_explore: { started: true, line: null, prompt: null },
  micro_world_page: "",
  briefs_waiting: null,
  pr_ratify: [],
  projects: [],
  remove_project: [],
  select: noSelection,
  selected: noSelection,
  sessions: [],
  remove_api_key: null,
  set_api_key: null,
  set_explanations: "{}",
  settings_files: { settings: null, keymap: null },
  subagent_log: null,
  back_a_law: { kind: "run", id: "19a2f-9" },
  law_patch_open: { kind: "pr", number: 412, url: "https://github.com/someone/app/pull/412" },
  new_law: { kind: "run", id: "19a2f-12" },
  guard_draft: { id: "shell-keeps-order", on: "shell", text: "The shell applies events in the order they were made." },
  turn_file_hunk: null,
  turn_reply: null,
  turn_summary: null,
  watch: null,
  wired: true,
  about: { version: "0.2.0" },
  writer_notice: null,
  writers: { writers: [{ kind: "claude", name: "Claude", found: true, models: ["haiku", "sonnet", "opus", "fable"], efforts: ["", "low", "medium", "high"] }, { kind: "agy", name: "agy", found: false, models: ["gemini-4-neon-low"], efforts: [] }], summaries_on: true, briefs: true, keys: [], base_url: null },
  test_writer: { outcome: "passed", line: "Passed: `claude` answered. Summaries are written with it.", settings: null },
} satisfies { [C in ReadCommand]: Commands[C] };
