// Studio state for `npm run snap`: each fixture maps a Tauri command to what it returns.
// A fixture is a function so that its times are relative to the moment it renders.

import { existsSync, readFileSync } from "node:fs";
import { railFixtures } from "./rail-fixtures.mjs";

const session = (id, name, state, extra = {}) => {
  const now = Date.now();
  return {
    id, capabilities: { agent: "claude", asks: true, background_tasks: true, replies: true, permission_mode: true, durations: true, transcript_version: true, context: true, turn_trees: true },
    model: null, shell: false, cwd: `/w/plotkeeper/.claude/worktrees/${id}`, folder_name: id, terminal: null, status_only: false, name, prompt_name: null,
    resume_id: id, permission_mode: null, started_mode: null, cleared: [], asks_before_closing: false, landed: false, last_step: null,
    started_at: now - 3600e3, last_event_at: now - 60e3, turn_span: null, step_span: null, state, checks: [], agents: [], asker: null,
    prompts: [], turns: [], since_looked: { turns: 0, files: 0, passed: 0, failed: 0, ran: 0 }, questions: [], returned: null,
    transcript: { intention: null, steps_ago: 0, responses: [], unavailable: null, context: null, branch: null, model: null, effort: null },
    identity: { issue: null, branch: null, pr: null, worktree: "", name },
    work: { kind: null, phases: [], agent_ms: 0, estimate: { range: null, depends_on_you: false } },
    unseen: false, ...extra,
  };
};

const subagent = (id, name) => ({
  id, parent: "session", kind: "worker", name, asked: null, agent_type: null, status: "running",
  started_at: Date.now() - 600e3, ended_at: null, last_step: null, step_span: null,
});

const common = {
  settings_files: { settings: null, keymap: null },
  cards: { brief: null, goal: null, answers: [], open: [], design: [], recent: [], older: [], decisions: [] },
  workspaces: { main: { path: "/w/plotkeeper", branch: "main" }, tasks: [], hidden: 0 },
  terminal_connect: { repo: "/w/plotkeeper", outdated: false },
  terminal_lost: [],
  revival: [],
  projects: ["/w/plotkeeper"],
};

// A subagent's log pane: the subagent's line selected, its log from the stubbed `subagent_log`.
const logFixture = (status) => {
  const agent = { ...subagent("a1", "Review the diff"), agent_type: "Explore", status, ended_at: status === "running" ? null : Date.now() - 60e3 };
  const sessions = [session("s1", "#56 · Row controls", "running", { agents: [agent] })];
  const entries = [
    { kind: "prompt", text: "Review the diff on this branch and list anything that breaks the token rules.\nReport file and line." },
    { kind: "reply", text: "I'll start with the changed files, then check each style rule against the tokens." },
    { kind: "call", id: "c1", tool: "Bash", detail: "git diff --stat" },
    { kind: "result", id: "c1", text: " ui/log.ts | 30 ++++\n ui/style.css | 26 ++++\n 2 files changed", failed: false, cut: false },
    { kind: "call", id: "c2", tool: "Read", detail: "ui/missing.ts" },
    { kind: "result", id: "c2", text: "File does not exist.", failed: true, cut: false },
    { kind: "reply", text: "The diff is small. Checking the stylesheet for raw colours next." },
    { kind: "call", id: "c3", tool: "Grep", detail: "#[0-9a-f]{3,8} ui/style.css" },
    ...(status === "running" ? [] : [
      { kind: "result", id: "c3", text: Array.from({ length: 30 }, (_, i) => `ui/style.css:${i + 1}: no match`).join("\n"), failed: false, cut: false },
      { kind: "reply", text: "No raw colours: every colour in the diff is a theme token." },
    ]),
  ];
  return { ...common, sessions, selected: { session: sessions[0], agent: agent.id }, subagent_log: { entries, offset: 1 } };
};

// The session's terminal for `recordedTerminal`: the output of a scripted Claude Code session, kept in terminal-recording.txt and
// recorded by record-terminal.sh at the size below. Before the first recording the terminal is empty, which the fixture's script fails.
const recordingFile = new URL("./terminal-recording.txt", import.meta.url);
const recording = { cols: 110, rows: 32 };
const recordedTerminal = () => {
  const f = railFixtures.railYourTurn();
  const data = existsSync(recordingFile) ? readFileSync(recordingFile).toString("base64") : "";
  f.selected.session.terminal = "t1";
  return {
    ...f, terminals: [{ id: "t1", argv: ["claude"], cwd: f.selected.session.cwd, exited: null, replay: { data, ...recording } }],
    // The terminal's drawn rows, not the page's: a recording that did not replay leaves them empty.
    snapScript: `
      const rows = [...document.querySelectorAll("#terminal .xterm-rows > div")].filter((row) => row.textContent.trim() !== "");
      if (rows.length === 0) log("snap:fail the terminal shows no rows");
      else log("terminal rows: " + rows.length);`,
  };
};

const usageTiles = () => {
  const now = Date.now() / 1000;
  return [
    { agent: "claude", five_hour: { used_percentage: 62, resets_at: now + 2 * 3600 + 600 }, seven_day: { used_percentage: 80, resets_at: now + 3 * 86400 }, read_at: now - 240, source: "last Claude session", reason: null },
    { agent: "codex", five_hour: null, seven_day: null, read_at: null, source: null, reason: "Codex's usage could not be read: codex app-server did not answer" },
  ];
};

export const fixtures = {
  ...railFixtures,
  recordedTerminal,
  logRunning: () => logFixture("running"),
  logDone: () => logFixture("completed"),
  // The list with a nested row (a session with subagents), the selected row and a landed one.
  list() {
    const sessions = [
      session("s1", "#56 · Row controls", "running", { agents: [subagent("a1", "Review the diff"), subagent("a2", "Write tests")] }),
      session("s2", "#57 · Name from the prompt", "needs_you"),
      session("s3", "#54 · Subagent terminal", "your_turn", { landed: true }),
      session("s4", "#55 · Delete default", "your_turn", { unseen: true }),
    ];
    return { ...common, sessions, selected: { session: sessions[0], agent: null } };
  },
  // The top bar's usage glance (#163): the tightest window across the agents' tiles, here Claude's weekly 80%.
  usageGlance() {
    return { ...fixtures.list(), usage: usageTiles() };
  },
  // The usage popover open (#163): Claude's tile with its bars and age, and Codex's, whose read failed, with only its reason.
  usagePopover() {
    return { ...fixtures.list(), usage: usageTiles(), snapScript: `document.getElementById("usage-popover").showPopover();` };
  },
  // The list with each agent's logo in each state (#158): Claude, agy, status-only Codex, another program and a shell.
  logos() {
    const agy = { capabilities: { agent: "agy", asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, context: false, turn_trees: false }, model: "gemini-3.8-flash-high" };
    const only = (program, extra = {}) => ({ status_only: true, capabilities: { agent: "", asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, context: false, turn_trees: false }, ...extra, identity: { issue: null, branch: null, pr: null, worktree: "", name: `${program} · plotkeeper` } });
    const sessions = [
      session("s1", "#56 · Row controls", "running", { transcript: { ...session("x", "x", "idle").transcript, context: { tokens: 90000, share: 9, window: 1000000, left: 91 } } }),
      session("s2", "agy · Name from the prompt", "needs_you", agy),
      session("s3", "#54 · Subagent terminal", "your_turn", { unseen: true }),
      session("s4", "codex · plotkeeper", "running", only("codex")),
      session("s5", "aider · plotkeeper", "your_turn", only("aider")),
      session("s6", "plotkeeper", "idle", only("zsh", { shell: true })),
      session("s7", "#55 · Delete default", "idle", agy),
    ];
    // The idle group is folded by default: open it, so the idle rows show.
    const snapScript = `[...document.querySelectorAll("summary, button")].find((el) => /^Idle/.test(el.textContent.trim()))?.click();`;
    return { ...common, sessions, selected: { session: sessions[1], agent: null }, snapScript };
  },
  // Workspaces rows and the git line (#160): the selected session's task shows its branch once, a renamed and a detached one show
  // theirs, and the main checkout keeps its branch. Built on the rail's running fixture, whose session works in the first task.
  branchOnce() {
    const base = railFixtures.railRunning();
    const root = "/Users/danielrispler/work/plotkeeper";
    const task = (name) => `${root}/.claude/worktrees/${name}`;
    const state = (branch, extra = {}) => ({ branch, upstream: branch ? [0, 0] : null, changed: 0, main_behind: null, on_default: false, would_conflict: false, operation: null, ...extra });
    return {
      ...base,
      workspaces: {
        main: { path: root, branch: "main" },
        tasks: [
          { path: task("issue-56-row-controls"), branch: "issue-56-row-controls" },
          { path: task("issue-140-old"), branch: "issue-140-new" },
          { path: task("look"), branch: null },
        ],
        hidden: 0,
      },
      workspace_git_state: { __by: "path", values: {
        [root]: state("main", { on_default: true }),
        [task("issue-56-row-controls")]: state("issue-56-row-controls", { upstream: [2, 1], changed: 5 }),
        [task("issue-140-old")]: state("renamed-branch", { upstream: null }),
        [task("look")]: state(null, { changed: 1 }),
      } },
    };
  },
  // Home: nothing selected, with Needs you, Unseen, Landed and the project row.
  home() {
    const sessions = [
      session("s1", "#56 · Row controls", "running"),
      session("s2", "#57 · Name from the prompt", "needs_you"),
      session("s4", "#55 · Delete default", "your_turn", { unseen: true }),
      session("s3", "#54 · Subagent terminal", "idle", { landed: true }),
      session("s5", "#48 · Agent deadline", "ended", { landed: true }),
    ];
    return { ...common, sessions, selected: null };
  },
};

// Pull main (#164), on #160's rows: the main checkout is 3 commits behind origin, so Pull ↓3 shows, enabled; with uncommitted files it is disabled.
const pullMain = (extra) => () => {
  const f = fixtures.branchOnce();
  const main = f.workspaces.main.path;
  f.workspace_git_state.values[main] = { ...f.workspace_git_state.values[main], upstream: [0, 3], main_behind: 3, ...extra };
  return f;
};
fixtures.pullEnabled = pullMain({});
fixtures.pullDisabled = pullMain({ changed: 2, would_overwrite: true });
