// What a snap fixture is, and the replies every fixture starts from (#144). A fixture maps a command of the window's
// `Commands` (ui/types.ts) to its reply, typed by that map, so `tsc -p scripts/snap` fails on a reply that lacks a field.
// The stub answers a command a fixture leaves out with `null`, which only a `void` command takes: `base` answers every
// other command, and adding one to the map without a reply here fails `tsc` too.

import type { Command, Commands, Selection } from "../../ui/types.ts";
import type { Listed } from "../../ui/daemon.ts";

// A reply that depends on one argument: { __by: "id", values: { <id>: reply } }.
export type ByArg<T> = { __by: string; values: Record<string, T> };

// The commands whose reply the window reads.
type ReadCommand = { [C in Command]: Commands[C] extends void ? never : C }[Command];

// A daemon terminal and the screen it replays when the window attaches.
export type Terminal = Listed & { replay: { data: string; cols: number; rows: number } };

// Beside the replies, the harness's own keys: the daemon's terminals and a script that runs once the UI has drawn (page.mjs).
export type Fixture = { [C in Command]?: Commands[C] | ByArg<Commands[C]> } & { terminals?: Terminal[]; appErrors?: string[]; snapScript?: string };

const workspace = { path: "/w/plotkeeper", branch: "main" };
const noSelection: Selection | null = null;

export const base = {
  add_project: [],
  agent_argv: ["claude"],
  agent_summary: null,
  agents: ["claude"],
  agy_plugin_setup: null,
  answer_blocked_command: "hook",
  ask: "",
  ask_anchor: "",
  ask_facts: null,
  ask_why_not: { facts: { sections: [] }, reasons: null },
  branches: [],
  cards: { brief: null, goal: null, answers: [], open: [], design: [], recent: [], older: [], workers: [], decisions: [], risk_notes: [], decision_notes: [] },
  check_output: null,
  choose_folder: null,
  claude_argv: ["claude"],
  create_named_task: workspace,
  create_task: workspace,
  explain_more: null,
  learning: null,
  map_place: {},
  map_save_positions: { positions: {}, tiers: {} },
  map_set_tier: { positions: {}, tiers: {} },
  map_know: { positions: {}, tiers: {} },
  map_history: [],
  map_at: { commit: "", short: "", label: "", source: "" },
  map_commit: { commit: "", short: "", label: "", url: null, files: [] },
  map_marks: [],
  stop_log: null,
  stop_facts: { active_days: 0, merged_prs: 0, map_days: 0, mark_days: 0, map_on_mark_days: 0, scrubs: 0, proofs: 0, knows: 0, dismissed: true },
  stop_dismiss: null,
  map_proof: [],
  map_state: { project: "/w/plotkeeper", status: "none", source: null, reference: "origin/main", commit: null, note: null, user: { positions: {}, tiers: {} }, fog: {}, ci: null },
  open: noSelection,
  plugin_setup: null,
  projects: ["/w/plotkeeper"],
  remove_project: [],
  remove_task: null,
  reorder_projects: [],
  repository_of: null,
  revival: [],
  select: noSelection,
  select_terminal: noSelection,
  selected: noSelection,
  sessions: [],
  settings_files: { settings: null, keymap: null },
  skills: [],
  subagent_log: null,
  suggest_projects: [],
  terminal_connect: { repo: "/w/plotkeeper", outdated: false },
  terminal_lost: [],
  turn_file_hunk: null,
  turn_reply: null,
  turn_summary: null,
  usage: [],
  watch: null,
  workspace_git_state: { branch: "main", upstream: [0, 0], changed: 0, main_behind: 0, on_default: true, would_conflict: false, operation: null },
  workspaces: { main: workspace, tasks: [], hidden: 0 },
  write_setting: "",
} satisfies { [C in ReadCommand]: Commands[C] };
