// Studio state for `npm run snap`: each fixture maps a Tauri command to what it returns.
// A fixture is a function so that its times are relative to the moment it renders.

import { existsSync, readFileSync } from "node:fs";
import type { AgentUsage } from "../../ui/usage.ts";
import type { GitState, LogEntry, Session, SessionState, Subagent } from "../../ui/types.ts";
import { base, type ByArg, type Fixture } from "./base.ts";
import { railFixtures } from "./rail-fixtures.ts";

const session = (id: string, name: string, state: SessionState, extra: Partial<Session> = {}): Session => {
  const now = Date.now();
  return {
    id, capabilities: { agent: "claude", asks: true, background_tasks: true, replies: true, permission_mode: true, durations: true, transcript_version: true, context: true, turn_trees: true },
    model: null, shell: false, cwd: `/w/plotkeeper/.claude/worktrees/${id}`, folder_name: id, terminal: null, status_only: false, name, prompt_name: null,
    resume_id: id, permission_mode: null, started_mode: null, cleared: [], asks_before_closing: false, landed: false, last_step: null,
    started_at: now - 3600e3, last_event_at: now - 60e3, turn_span: null, step_span: null, state, checks: [], agents: [], asker: null,
    prompts: [], turns: [], since_looked: { turns: 0, files: 0, passed: 0, failed: 0, ran: 0 }, questions: [], blocked_commands: [], returned: null,
    transcript: { intention: null, steps_ago: 0, responses: [], unavailable: null, context: null, branch: null, model: null, effort: null },
    identity: { issue: null, branch: null, pr: null, worktree: "", name },
    work: { kind: null, phases: [], agent_ms: 0, estimate: { range: null, depends_on_you: false } },
    unseen: false, seen: 0, ...extra,
  };
};

const subagent = (id: string, name: string): Subagent => ({
  id, parent: "session", kind: "worker", name, asked: null, agent_type: null, status: "running",
  started_at: Date.now() - 600e3, ended_at: null, last_step: null, step_span: null, result: null, failed: false, opened: false, faded: false, turn: null,
});


// A subagent's log pane: the subagent's line selected, its log from the stubbed `subagent_log`.
const logFixture = (status: string): Fixture => {
  const agent = { ...subagent("a1", "Review the diff"), agent_type: "Explore", status, ended_at: status === "running" ? null : Date.now() - 60e3 };
  const sessions = [session("s1", "#56 · Row controls", "running", { agents: [agent] })];
  const entries: LogEntry[] = [
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
    ] satisfies LogEntry[]),
  ];
  return { ...base, sessions, selected: { session: sessions[0], agent: agent.id }, subagent_log: { entries, offset: 1 } };
};

// The session's terminal for `recordedTerminal`: the output of a scripted Claude Code session, kept in terminal-recording.txt and
// recorded by record-terminal.sh at the size below. Before the first recording the terminal is empty, which the fixture's script fails.
const recordingFile = new URL("./terminal-recording.txt", import.meta.url);
const recording = { cols: 110, rows: 32 };
const recordedTerminal = (): Fixture => {
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

const usageTiles = (): AgentUsage[] => {
  const now = Date.now() / 1000;
  const window = (label: string, used: number, ahead: number) => ({ label, used_percentage: used, resets_at: now + ahead });
  return [
    { agent: "claude", groups: [{ name: "", windows: [window("5h", 62, 2 * 3600 + 600), window("week", 80, 3 * 86400)] }], read_at: now - 240, source: "last Claude session", reason: null },
    { agent: "codex", groups: [], read_at: null, source: null, reason: "Codex's usage could not be read: it did not answer in time" },
    {
      agent: "agy",
      groups: [
        { name: "Gemini Models", windows: [window("5h", 1, 5400), window("week", 3, 4 * 86400)] },
        { name: "Claude and GPT models", windows: [window("5h", 0, 4 * 3600), window("week", 0, 7 * 86400)] },
        { name: "Gemini 4 Argon", windows: [window("week", 78, 5 * 86400)] },
      ],
      read_at: now - 5,
      source: "agy /usage",
      reason: null,
    },
  ];
};

// The list with a nested row (a session with subagents), the selected row and a landed one.
const list = () => {
  const sessions = [
    session("s1", "#56 · Row controls", "running", { agents: [subagent("a1", "Review the diff"), subagent("a2", "Write tests")] }),
    session("s2", "#57 · Name from the prompt", "needs_you"),
    session("s3", "#54 · Subagent terminal", "your_turn", { landed: true }),
    session("s4", "#55 · Delete default", "your_turn", { unseen: true }),
  ];
  return { ...base, sessions, selected: { session: sessions[0], agent: null } } satisfies Fixture;
};

// Workspaces rows and the git line (#160): the selected session's task shows its branch once, a renamed and a detached one show
// theirs, and the main checkout keeps its branch. Built on the rail's running fixture, whose session works in the first task.
const branchOnce = () => {
  const running = railFixtures.railRunning();
  const root = "/Users/danielrispler/work/plotkeeper";
  const task = (name: string) => `${root}/.claude/worktrees/${name}`;
  const state = (branch: string | null, extra: Partial<GitState> = {}): GitState => ({ branch, upstream: branch ? [0, 0] : null, changed: 0, main_behind: null, on_default: false, would_conflict: false, operation: null, ...extra });
  return {
    ...running,
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
  } satisfies Fixture;
};

// Pull main (#164), on #160's rows: the main checkout is 3 commits behind origin, so Pull ↓3 shows, enabled; when the incoming commits would overwrite changed files it is disabled.
const pullScript = (disabled: boolean) => `
  const pull = document.querySelector(".workspace .pull");
  if (!pull) log("snap:fail Pull is missing");
  else if (pull.disabled !== ${disabled}) log("snap:fail Pull should be ${disabled ? "disabled" : "enabled"}");
  else log("Pull is " + (pull.disabled ? "disabled: " + pull.title : "enabled"));
`;
const pullMain = (extra: Partial<GitState>, disabled: boolean) => (): Fixture => {
  const f = branchOnce();
  const main = f.workspaces.main.path;
  // The other sessions work in tasks of their own, so none of them blocks Pull for working in the main checkout.
  for (const { cwd } of f.sessions) if (!f.workspaces.tasks.some(({ path }) => path === cwd)) f.workspaces.tasks.push({ path: cwd, branch: cwd.split("/").pop() ?? null });
  f.workspace_git_state.values[main] = { ...f.workspace_git_state.values[main], upstream: [0, 3], main_behind: 3, ...extra };
  return { ...f, snapScript: pullScript(disabled) };
};

// The settings page (#195), opened with the top bar's gear on the list. `settings` is the text of `settings.json`; `expect` is a script
// of checks that logs `snap:fail` for what the page does not show.
const settingsPage = (settings: string | null, expect: string, extra: Fixture = {}): Fixture => ({
  ...list(),
  settings_files: { settings, keymap: null },
  ...extra,
  snapScript: `
    document.getElementById("settings-gear").click();
    const dialog = document.getElementById("settings-dialog");
    if (!dialog.open) log("snap:fail the dialog did not open");
    const titles = [...dialog.querySelectorAll(".setting-title")].map((t) => t.textContent.trim());
    log("rows: " + titles.join(" | "));
    const undo = () => dialog.querySelectorAll(".reset").length;
    const broken = () => dialog.querySelector(".broken")?.textContent ?? "";
    const toggles = () => [...dialog.querySelectorAll(".switch")].map((t) => t.getAttribute("aria-checked") + (t.disabled ? " disabled" : ""));
    ${expect}`,
});

// The page's steppers and dropdowns (#196) with one value changed, the zoom: its stepper and undo icon, the font size's stylesheet value, and the
// dropdowns at Haiku and Default.
const settingsControls = (): Fixture =>
  settingsPage(`{ "zoom": 1.2 }`, `
      const values = () => [...dialog.querySelectorAll(".stepper-value")].map((v) => v.textContent);
      const selects = () => [...dialog.querySelectorAll("select")].map((s) => s.options[s.selectedIndex].textContent);
      if (values().join() !== "1.2,12.5") log("snap:fail the steppers read " + values());
      if (selects().join() !== "Haiku,Default") log("snap:fail the dropdowns read " + selects());
      if (undo() !== 1) log("snap:fail expected one undo icon, got " + undo());
      if (dialog.querySelector(".setting-title").textContent.trim() !== "Default zoom") log("snap:fail the first row is not Default zoom");
      log("headings: " + [...dialog.querySelectorAll("h2")].map((h) => h.textContent).join(" | "));
      log("scroll: " + dialog.querySelector(".settings-body").scrollHeight + " of " + dialog.querySelector(".settings-body").clientHeight);`);

export const fixtures = {
  ...railFixtures,
  recordedTerminal,
  logRunning: () => logFixture("running"),
  logDone: () => logFixture("completed"),
  list,
  // The top bar's usage glance (#163): the tightest window across the agents' tiles, here Claude's weekly 80%.
  usageGlance() {
    return { ...list(), usage: usageTiles() };
  },
  // The usage popover open (#163): Claude's tile with its bars and age, Codex's, whose read failed, with only its reason, and agy's three groups, most used first.
  usagePopover() {
    return { ...list(), usage: usageTiles(), snapScript: `document.getElementById("usage-popover").showPopover();` };
  },
  // The list with each agent's logo in each state (#158): Claude, agy, status-only Codex, another program and a shell.
  logos() {
    const agy = { capabilities: { agent: "agy", asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, context: false, turn_trees: false }, model: "gemini-3.8-flash-high" };
    const only = (program: string, extra: Partial<Session> = {}) => ({ status_only: true, capabilities: { agent: "", asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, context: false, turn_trees: false }, ...extra, identity: { issue: null, branch: null, pr: null, worktree: "", name: `${program} · plotkeeper` } });
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
    return { ...base, sessions, selected: { session: sessions[1], agent: null }, snapScript };
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
    return { ...base, sessions, selected: null };
  },
  // The settings page at its defaults: no undo icon, both toggles at their defaults, the file's two rows.
  settingsDefaults: () =>
    settingsPage(null, `
      if (undo() !== 0) log("snap:fail an undo icon shows at the defaults");
      if (toggles().join() !== "true,false,false") log("snap:fail the toggles read " + toggles());
      if (broken()) log("snap:fail the broken-file line shows");`),
  // Two values changed in the file: each row shows its undo icon.
  settingsChanged: () =>
    settingsPage(`{\n  // Quiet.\n  "sounds": false,\n  "notifications": { "while_focused": true },\n}\n`, `
      if (undo() !== 2) log("snap:fail expected 2 undo icons, got " + undo());
      if (toggles().join() !== "false,true,false") log("snap:fail the toggles read " + toggles());
      if (dialog.querySelector(".reset").title !== "Reset to default") log("snap:fail the undo icon's tooltip is wrong");
      if (document.getElementById("settings-close").title !== "Close (Esc)") log("snap:fail the X's tooltip is " + document.getElementById("settings-close").title);`),
  // The page's steppers and dropdowns (#196) with one value changed, the zoom: its stepper and undo icon, the font size's stylesheet
  // value, and the dropdowns at Haiku and Default.
  settingsControls,
  // The same page scrolled to its end, to see Summaries and Files.
  settingsControlsEnd: () => {
    const f = settingsControls();
    return { ...f, snapScript: `${f.snapScript}\ndialog.querySelector(".settings-body").scrollTop = 9999;` };
  },
  // A model the list lacks is its own entry, selected; the page never overwrites it.
  settingsModel: () =>
    settingsPage(`{ "summaries": { "model": "claude-opus-4-1" } }`, `
      const model = dialog.querySelector('select[data-key="summaries.model"]');
      if (model.options[model.selectedIndex].textContent !== "claude-opus-4-1") log("snap:fail the unlisted model is not selected");
      if (model.options.length !== 5) log("snap:fail the model's entries are " + model.options.length);
      if (undo() !== 1) log("snap:fail expected one undo icon, got " + undo());`),
  // Pressing a stepper sends the write: the zoom's text it returns shows its value and undo icon.
  settingsStepper: () =>
    settingsPage(null, `
      dialog.querySelector('button[data-action="up"]').click();
      await new Promise((done) => setTimeout(done, 200));
      if (dialog.querySelector(".stepper-value").textContent !== "1.1") log("snap:fail the zoom did not step to 1.1");
      if (undo() !== 1) log("snap:fail the change did not show its undo icon");`, { write_setting: '{ "zoom": 1.1 }' }),
  // A stepper at its bound has that button off.
  settingsBound: () =>
    settingsPage(`{ "zoom": 2 }`, `
      const zoom = dialog.querySelector(".stepper");
      if (!zoom.querySelector('[data-action="up"]').disabled) log("snap:fail Increase is enabled at 2");
      if (zoom.querySelector('[data-action="down"]').disabled) log("snap:fail Decrease is off at 2");`),
  // Choosing a model in the dropdown sends the write, and the page shows it as the one selected, with its undo icon.
  settingsSelect: () =>
    settingsPage(null, `
      const model = dialog.querySelector('select[data-key="summaries.model"]');
      model.value = "opus";
      model.dispatchEvent(new Event("change", { bubbles: true }));
      await new Promise((done) => setTimeout(done, 200));
      if (model.options[model.selectedIndex].textContent !== "Opus") log("snap:fail the model did not show Opus");
      if (undo() !== 1) log("snap:fail the change did not show its undo icon");`, { write_setting: '{ "summaries": { "model": "opus" } }' }),
  // A file that does not parse: the line, the button, and every control off.
  settingsBroken: () =>
    settingsPage(`{ "sounds": false, `, `
      if (!broken().startsWith("settings.json has errors: fix it first")) log("snap:fail the line reads: " + broken());
      if (!toggles().every((t) => t.endsWith("disabled"))) log("snap:fail a toggle is enabled: " + toggles());
      if ([...dialog.querySelectorAll("select, .stepper .btn")].some((c) => !c.disabled)) log("snap:fail a stepper or dropdown is enabled");
      if ([...dialog.querySelectorAll("button.open")].some((b) => !b.disabled)) log("snap:fail an Open button is enabled");`),
  // The undo icon of a changed dropdown row sends the write that removes its key (a click on a dropdown's own control does not).
  settingsReset: () =>
    settingsPage(`{ "summaries": { "model": "opus" } }`, `
      const sent = [];
      const invoke = window.__TAURI__.core.invoke;
      window.__TAURI__.core.invoke = (command, args) => (command === "write_setting" && sent.push(args), invoke(command, args));
      dialog.querySelector('select[data-key="summaries.model"]').click();
      dialog.querySelector('button[data-key="summaries.model"][data-action="reset"]').click();
      await new Promise((done) => setTimeout(done, 200));
      if (sent.length !== 1) log("snap:fail expected one write, got " + sent.length);
      else if (sent[0].value !== null || sent[0].path.join(".") !== "summaries.model") log("snap:fail the write was " + JSON.stringify(sent[0]));
      if (undo() !== 0) log("snap:fail the undo icon still shows");`, { write_setting: "{}" }),
  // A change saves at once: pressing Sounds sends the write, and the text it returns shows with its undo icon.
  settingsWrite: () =>
    settingsPage(null, `
      dialog.querySelector(".switch").click();
      await new Promise((done) => setTimeout(done, 200));
      if (undo() !== 1) log("snap:fail the change did not show its undo icon");
      if (toggles()[0] !== "false") log("snap:fail Sounds did not turn off: " + toggles());`, { write_setting: '{ "sounds": false }' }),
  // A refused write shows its reason in the page, also once the files read again after it bring the newer text, and the line coming
  // leaves the rows' nodes as they were (a focused control stays focused).
  settingsRefused: () =>
    settingsPage(null, `
      const invoke = window.__TAURI__.core.invoke;
      window.__TAURI__.core.invoke = (command, args) =>
        command === "settings_files" ? Promise.resolve({ settings: '{ "zoom": 1.1 }', keymap: null }) : invoke(command, args);
      const sounds = dialog.querySelector(".switch");
      sounds.click();
      await new Promise((done) => setTimeout(done, 200));
      if (dialog.querySelector(".switch") !== sounds) log("snap:fail the refusal's line rebuilt the rows");
      if (!dialog.querySelector(".refused")?.textContent.includes("changed since")) log("snap:fail the refusal's reason is not shown");`, { write_setting: { __throw: "settings.json changed since the page read it: try again" } as never }),
  branchOnce,
  pullEnabled: pullMain({}, false),
  pullDisabled: pullMain({ changed: 2, would_overwrite: true }, true),
} satisfies Record<string, () => Fixture>;

