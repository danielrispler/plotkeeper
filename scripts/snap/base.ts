// What a snap fixture is, and the replies every fixture starts from (#144). A fixture maps a command of the window's
// `Commands` (ui/types.ts) to its `reply`, typed by that map, so `tsc -p scripts/snap` fails on a reply that lacks a field.
// The stub answers a command a fixture leaves out with `null`, which only a `void` command takes: `base` answers every
// other command, and adding one to the map without a reply here fails `tsc` too.

import type { Command, Reply, Selection } from "../../ui/types.ts";

// A reply that depends on one argument: { __by: "id", values: { <id>: reply } }.
type ByArg<T> = { __by: string; values: Record<string, T> };

// The commands whose reply the window reads.
type ReadCommand = { [C in Command]: Reply<C> extends void ? (void extends Reply<C> ? never : C) : C }[Command];

// Beside the replies, the harness's own keys: errors for the log and a script that runs once the UI has drawn (page.mjs). `motion`
// pins `prefers-reduced-motion` for a fixture that tests motion, since the machine's own setting differs (CI's macOS runner asks for reduce).
// `budget` is the page's virtual time in ms, for a fixture whose script runs past the default 5 s (snap.mjs).
export type Fixture = { [C in Command]?: Reply<C> | ByArg<Reply<C>> } & { appErrors?: string[]; snapScript?: string; snapDone?: string; realPointer?: boolean; realKeys?: boolean; motion?: "full" | "reduce"; contrast?: boolean; budget?: number; storage?: Record<string, string> };

const noSelection: Selection | null = null;

// The free scan of a repository with no map (#760), which `base`'s map state is: three grey parts, the computed links from App and Cli to Lib,
// two read languages and one not, two folders that look like copies (one ticked), two loose files and a loose test, and Lib split by its
// subfolders (#761).
const scanPart = (id: string, name: string, path: string) => ({ id, name, caption: "Not yet described", purpose: "Not yet described.", holds: [], tier: "interface", paths: [path] });
const scanFile = JSON.stringify({
  version: 2,
  extractors: ["typescript-imports", "go-imports"],
  parts: [scanPart("app", "App", "src/app/"), scanPart("cli", "Cli", "src/cli/"), scanPart("lib", "Lib", "src/lib/")],
  actors: [],
  links: [{ id: "app-lib", from: "app", to: "lib", kind: "import" }, { id: "cli-lib", from: "cli", to: "lib", kind: "import" }],
  ignored: [],
  laws: [],
});

export const base = {
  agent_summary: null,
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
  map_scan: {
    map: scanFile,
    files: { app: 12, cli: 1, lib: 30 },
    read_languages: ["TypeScript", "Go"],
    unread_languages: ["Ruby"],
    claude: true,
    ignore_candidates: [{ path: "reference-copy/", ticked: false }, { path: "vendor/", ticked: true }],
    loose_files: ["src/index.ts", "src/main.ts"],
    loose_tests: ["tests/app.test.ts"],
    splits: { lib: ["src/lib/core/", "src/lib/net/"] },
  },
  map_scan_edit: { map: scanFile, files: { app: 12, cli: 1, lib: 30 }, read_languages: [], unread_languages: [], claude: true, ignore_candidates: [], loose_files: [], loose_tests: [], splits: {} },
  map_draft: null,
  map_draft_start: { map: scanFile, groups: [{ parts: ["app", "cli", "lib"], state: "writing", why: null, redraft: false }], verdicts: {}, not_written: [], ready: false, path: "/w/plotkeeper/system-map.json", pr_muted: null },
  map_draft_retry: { map: scanFile, groups: [{ parts: ["app", "cli", "lib"], state: "writing", why: null, redraft: false }], verdicts: {}, not_written: [], ready: false, path: "/w/plotkeeper/system-map.json", pr_muted: null },
  map_draft_redraft: { map: scanFile, groups: [{ parts: ["app", "cli", "lib"], state: "writing", why: null, redraft: false }], verdicts: {}, not_written: [], ready: false, path: "/w/plotkeeper/system-map.json", pr_muted: null },
  map_draft_edit: { map: scanFile, groups: [{ parts: ["app", "cli", "lib"], state: "writing", why: null, redraft: false }], verdicts: {}, not_written: [], ready: false, path: "/w/plotkeeper/system-map.json", pr_muted: null },
  map_draft_accept: { kind: "written", path: "/w/plotkeeper/system-map.json" },
  map_draft_pr: { number: 7, url: "https://github.com/someone/plotkeeper/pull/7" },
  map_pending_write: { kind: "written", path: "/w/plotkeeper/system-map.json" },
  map_pending_pr: { number: 8, url: "https://github.com/someone/plotkeeper/pull/8" },
  map_state: { project: "/w/plotkeeper", status: "none", source: null, from: null, pending: null, problems: [], reference: "origin/main", commit: null, note: null, user: { positions: {}, tiers: {} }, fog: {}, ci: null, ready: [], ratify: [], runs: { paused: null, runs: [] }, stale: {}, stale_why: {}, catch_ups: {} },
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
  pr_map_review: null,
  map_review_start: null,
  pr_map_diff: null,
  ratify_site: { file: "", first: 1, lines: [] },
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
  writers: { writers: [{ kind: "claude", name: "Claude", found: true, models: ["haiku", "sonnet", "opus", "fable"], efforts: ["", "low", "medium", "high"] }, { kind: "agy", name: "agy", found: false, models: ["gemini-4-neon-low"], efforts: [] }], summaries_on: true, briefs: true, keys: [], base_url: null, key_store: "the macOS Keychain" },
  test_writer: { outcome: "passed", line: "Passed: `claude` answered. Summaries are written with it.", settings: null },
} satisfies { [C in ReadCommand]: Reply<C> };
