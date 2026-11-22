// Studio state for `npm run snap`: each fixture maps a Tauri command to what it returns.
// A fixture is a function so that its times are relative to the moment it renders.

import { existsSync, readFileSync } from "node:fs";
import type { AgentUsage } from "../../ui/usage.ts";
import type { Ci, GitState, LogEntry, MapState, Session, SessionState, Subagent } from "../../ui/types.ts";
import { base, type ByArg, type Fixture } from "./base.ts";
import { railFixtures } from "./rail-fixtures.ts";

const session = (id: string, name: string, state: SessionState, extra: Partial<Session> = {}): Session => {
  const now = Date.now();
  return {
    id, capabilities: { agent: "claude", asks: true, background_tasks: true, replies: true, permission_mode: true, durations: true, transcript_version: true, context: true, turn_trees: true },
    model: null, shell: false, cwd: `/w/plotkeeper/.claude/worktrees/${id}`, folder_name: id, terminal: null, status_only: false, name, prompt_name: null,
    resume_id: id, permission_mode: null, started_mode: null, cleared: [], asks_before_closing: false, landed: false, last_step: null,
    started_at: now - 3600e3, last_event_at: now - 60e3, turn_span: null, step_span: null, state, agents: [], asker: null,
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

// The page's steppers and dropdowns (#196) with one value changed, the zoom: its stepper and undo icon, the font size's stylesheet value, the context warning at 30, and the
// dropdowns at Haiku and Default.
const settingsControls = (): Fixture =>
  settingsPage(`{ "zoom": 1.2 }`, `
      const values = () => [...dialog.querySelectorAll(".stepper-value")].map((v) => v.textContent);
      const selects = () => [...dialog.querySelectorAll("select")].map((s) => s.options[s.selectedIndex].textContent);
      if (values().join() !== "1.2,12.5,30") log("snap:fail the steppers read " + values());
      if (selects().join() !== "Haiku,Default") log("snap:fail the dropdowns read " + selects());
      if (undo() !== 1) log("snap:fail expected one undo icon, got " + undo());
      if (dialog.querySelector(".setting-title").textContent.trim() !== "Default zoom") log("snap:fail the first row is not Default zoom");
      log("headings: " + [...dialog.querySelectorAll("h2")].map((h) => h.textContent).join(" | "));
      log("scroll: " + dialog.querySelector(".settings-body").scrollHeight + " of " + dialog.querySelector(".settings-body").clientHeight);`);

// Home's answered Retest of session s3, whose older turn (index 0) holds the Quiz: the turn's own (`ofCard` false), or a worker's card placed in
// that turn. The script clicks "See the code" and checks what the rail shows. Run narrower than 1007px, so the rail is an overlay.
const seeCode = (ofCard: boolean): Fixture => {
  const f = fixtures.home();
  const now = Date.now();
  const file = { root: "/w/plotkeeper", path: "ui/settings.rs", from: null, added: 1, removed: 1, agent: null };
  const turn = (i: number, id: string | null, ago: number, files: boolean) => ({ started_at: now - ago * 864e5 + i * 6e4, ended_at: now - ago * 864e5 + 600e3, prompt: `Turn ${i}`, id, message: `Turn ${i} done.`, checks: [], listed: [], files: files ? { files: [file], merged: 0 } : null });
  // Newer turns enough to push the older one below the fold, so only a scroll brings it into view.
  const turns = [turn(0, "r1", 8, true), ...Array.from({ length: 30 }, (_, i) => turn(i + 1, `t${i + 1}`, 3, false))];
  const s3 = { ...f.sessions.find((s) => s.id === "s3")!, turns };
  const sessions = f.sessions.map((s) => (s.id === "s3" ? s3 : s));
  const cardId = `s3@${turns[0].started_at + 60e3}`;
  const worker = { request: { session_id: "s3", id: cardId, agent: "w1", changed_by: {}, check: null }, card: { summary: "The worker fixed the write.", explanation: "It copies the state out.", paths: ["ui/settings.rs"], code: "", why_it_matters: "No lock across the read.", not_checked: "Nothing.", architecture: null, quiz: { text: "Why copy the state out?", key: 0, options: [{ text: "To hold no lock across the read", why: "Right." }, { text: "To save memory", why: "It copies." }] } }, guess: { option: 0, right: true, at: now }, key_wrong: false, more: null, rating: null, check_result: null, shown_at: now };
  const hunk = ["--- ui/settings.rs", "@@ -41,3 +41,3 @@", "-    let text = self.read_file(&self.state.lock());", "+    let text = { self.state.lock().file.clone() };", ""].join("\n");
  const learning = { ...f.learning, of: ofCard ? cardId : "r1", hunk, taps: [{ kind: "pick" as const, option: 0, at: now }] };
  const cards = { ...base.cards, workers: ofCard ? [{ card: worker, turn: 0 }] : [] };
  const snapScript = `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    const link = document.querySelector(".pane.home .slot-card .quiz-code-link");
    if (!link) return log("snap:fail the reveal has no See the code link");
    link.click();
    await wait(300);
    if (document.body.dataset.layout !== "overlay") log("snap:fail the window is not in the overlay layout: " + document.body.dataset.layout);
    if (document.querySelector(".pane.home:not([hidden])")) log("snap:fail Home is still shown after See the code");
    const lines = [...document.querySelectorAll("#feed .turn.past .turn-line")];
    const first = lines.find((l) => l.textContent.includes("Turn 0"));
    if (!first) return log("snap:fail the rail does not list the session's older turn: the session is not selected");
    if (first.getAttribute("aria-expanded") !== "true") log("snap:fail the Quiz's turn is not open");
    if (lines.some((l) => l !== first && l.getAttribute("aria-expanded") === "true")) log("snap:fail another folded turn is open");
    const hunk = document.querySelector("#feed .evidence-hunk");
    if (!hunk || hunk.hidden || !hunk.textContent.includes("self.state.lock().file.clone()")) log("snap:fail the turn's hunk is not open in the rail: " + hunk?.textContent);
    if (${ofCard} && !document.querySelector("#feed details.answered-card[open] article.card")?.textContent.includes("It copies the state out.")) log("snap:fail the card of the Quiz is not open");
    if (!document.body.classList.contains("rail-open")) log("snap:fail the overlay rail did not open");
    const story = document.getElementById("story");
    const turnTop = first.closest(".turn").getBoundingClientRect().top - story.getBoundingClientRect().top;
    if (story.scrollTop <= 0 || turnTop < -2 || turnTop > story.clientHeight - 60) log("snap:fail the turn was not scrolled into view: scrollTop " + story.scrollTop + ", top " + turnTop);`;
  return { ...f, sessions, learning, cards, select: { session: s3, agent: null }, turn_file_hunk: { text: hunk, left_out: 0, binary: false, first_line: 41 }, snapScript };
};

// The Map view (#309) on a map of plotkeeper's own shape, opened by its key. Positions are saved, as the prototype's Treatment C draws them.
const mapFile = JSON.stringify({
  version: 1,
  parts: [
    { id: "plugins", name: "Agent plugins", caption: "Which hooks each agent runs", description: "", tier: "interface", paths: ["plugin/"] },
    { id: "hook", name: "Hook", caption: "Records every agent event", description: "", tier: "core", paths: ["hook/"] },
    { id: "core", name: "Core", caption: "Turns events into session state", description: "", tier: "core", paths: ["core/"] },
    { id: "shell", name: "Shell", caption: "Reads the log, feeds the core, acts", description: "", tier: "core", paths: ["src-tauri/"] },
    { id: "understanding", name: "Understanding", caption: "Writes what you read, and keeps it", description: "", tier: "core", paths: ["understanding/"] },
    { id: "window", name: "Window", caption: "Everything you see and press", description: "", tier: "core", paths: ["ui/"] },
    { id: "daemon", name: "Terminal daemon", caption: "Owns the terminals", description: "", tier: "core", paths: ["terminal-daemon/"] },
    { id: "workbench", name: "Workbench", caption: "Git and worktrees", description: "", tier: "black-box", paths: ["workbench/"] },
  ],
  actors: [{ id: "agents", name: "Agents" }, { id: "writer", name: "Writer" }, { id: "github", name: "GitHub" }],
  links: [
    { id: "agents-plugins", from: "agents", to: "plugins" }, { id: "plugins-hook", from: "plugins", to: "hook" }, { id: "hook-shell", from: "hook", to: "shell" },
    { id: "shell-core", from: "shell", to: "core" }, { id: "shell-understanding", from: "shell", to: "understanding" }, { id: "understanding-writer", from: "understanding", to: "writer" },
    { id: "shell-window", from: "shell", to: "window" }, { id: "shell-daemon", from: "shell", to: "daemon" }, { id: "shell-workbench", from: "shell", to: "workbench" }, { id: "workbench-github", from: "workbench", to: "github" },
  ],
  ignored: [],
  laws: [
    { id: "core-pure", on: "core", backing: "check", text: "The core reads no file." }, { id: "core-order", on: "core", backing: "trust", text: "Events apply in order." },
    { id: "shell-log", on: "shell", backing: "check", text: "The log is only appended." }, { id: "shell-notice", on: "shell", backing: "trust", text: "Notices wait for focus." },
    { id: "hook-held", on: "hook", backing: "trust", text: "A held question blocks the agent." },
    { id: "understanding-kept", on: "understanding", backing: "check", text: "Cards are kept." }, { id: "understanding-once", on: "understanding", backing: "check", text: "A turn is written once." }, { id: "understanding-asked", on: "understanding", backing: "trust", text: "The writer is asked at most once." },
    { id: "window-quiet", on: "window", backing: "check", text: "Only needs-you has colour." }, { id: "window-key", on: "window", backing: "trust", text: "Every key is rebindable." },
    { id: "daemon-live", on: "daemon", backing: "check", text: "A terminal outlives the window." },
  ],
});
const at = (x: number, y: number): [number, number] => [x, y];
const mapState = (extra: Partial<MapState> = {}): MapState => ({
  project: "/w/plotkeeper", status: "loaded", source: mapFile, reference: "origin/main", commit: "35745b1693c5d061e5aea891e98a58515942a12d", note: null,
  user: { tiers: {}, positions: { core: at(620, 190), understanding: at(980, 290), plugins: at(300, 430), hook: at(580, 430), shell: at(880, 440), window: at(1180, 560), daemon: at(760, 690), workbench: at(480, 650), agents: at(110, 430), writer: at(1400, 190), github: at(250, 650) } },
  fog: {}, ci: null, ready: [],
  ...extra,
});
// The Ready for review tray (#336) with two ready PRs, at the Map view's foot.
const mapTray = (): Fixture =>
  mapOpened(`
      const rows = [...document.querySelectorAll("#map-tray .tray-row")];
      if (rows.length !== 2) return log("snap:fail expected 2 tray rows, saw " + rows.length);
      if (!document.querySelector("#map-tray .side-title")?.textContent.includes("Ready for review")) log("snap:fail the tray has no title");
      if (!rows[0].textContent.includes("#334") || !rows[0].textContent.includes("not written yet")) log("snap:fail row text: " + rows[0].textContent);
      log("tray ok");`, {
    map_state: mapState({ fog: { shell: "thick" }, ready: [{ number: 334, title: "PR briefs: the brief written for each ready PR", head: "b".repeat(40) }, { number: 336, title: "Ready for review tray", head: "a".repeat(40) }] }),
  });
const homeMarked = (press: boolean): Fixture => ({
  ...(fixtures as Record<string, () => Fixture>).home(),
  map_marks: ["/w/plotkeeper"],
  map_state: mapState({ fog: { shell: "thick" } }),
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    await wait(300);
    const marks = [...document.querySelectorAll(".pane.home .fog-mark")];
    if (marks.length !== 1) return log("snap:fail expected one fog mark, saw " + marks.length);
    if (marks[0].textContent !== "") log("snap:fail the mark carries text, a count?");
    if (${press}) {
      marks[0].click();
      await wait(400);
      if (document.getElementById("map").hidden) return log("snap:fail the mark did not open the Map view");
    }`,
});
// The stop-rule review (#314): Studio has run 20 active days and merged 25 PRs, and the Map view was opened on one of them and Home's mark
// never led to it, so the Map view fell below its threshold, and so did the history strip and "See the proof".
const dueFacts = { active_days: 20, merged_prs: 25, map_days: 1, mark_days: 5, map_on_mark_days: 0, scrubs: 0, proofs: 0, knows: 0, dismissed: false };
const homeReviewed = (): Fixture => ({
  ...(fixtures as Record<string, () => Fixture>).home(),
  map_marks: ["/w/plotkeeper"],
  stop_facts: dueFacts,
  snapScript: `
    await new Promise((done) => setTimeout(done, 400));
    const review = document.querySelector(".pane.home .review");
    if (!review) return log("snap:fail Home shows no review");
    if (!review.textContent.includes("Little used so far")) log("snap:fail the review's words: " + review.textContent);
    log("review ok");`,
});
const mapOpened = (check: string, extra: Fixture = {}): Fixture => ({
  ...list(),
  ...extra,
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    document.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyM", key: "m", metaKey: true, shiftKey: true, bubbles: true }));
    await wait(400);
    if (document.getElementById("map").hidden) return log("snap:fail the map key did not open the Map view");
    ${check}`,
});

const busySessions = () => {
    const touched = (...paths: string[]) => [{ started_at: Date.now() - 600e3, ended_at: null, prompt: "", id: null, message: null, checks: [], listed: [], files: { files: paths.map((path) => ({ root: "/w/plotkeeper", path, from: null, added: 3, removed: 1, agent: null })), merged: 0 } }];
    const agy = { agent: "agy", asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, context: false, turn_trees: false };
    const w = { ...subagent("w1", "Map chips"), kind: "worker" as const };
    const h = { ...subagent("h1", "Read the log"), kind: "helper" as const };
    const sessions = [
      session("s1", "#311 · Live agents", "running", { turns: touched("core/lib.rs", "ui/map.ts"), agents: [w, h] }),
      session("s2", "Ship PR 286", "needs_you", { turns: touched("hook/src/main.rs") }),
      session("s3", "#312 · CI marks", "running"),
      session("s4", "agy · Name from the prompt", "running", { capabilities: agy }),
      session("s5", "codex · plotkeeper", "running", { status_only: true, turns: [] }),
      session("s6", "#313 · Docs", "needs_you", { turns: touched("docs/notes.md") }),
      session("s7", "#55 · Delete default", "your_turn"),
    ];
    return sessions;
  };
const mapBusy = (jump: boolean): Fixture => {
    const sessions = busySessions();
    const f = mapOpened(`
      const chips = (id) => [...document.querySelectorAll("#map .plate[data-id=" + id + "] .agent-chip")].map((c) => c.textContent);
      await wait(400);
      if (chips("core").join() !== "#311 · Live agents") log("snap:fail core chips: " + chips("core").join());
      if (chips("window").join() !== "#311 · Live agents") log("snap:fail window chips: " + chips("window").join());
      if (chips("hook").join() !== "Ship PR 286 needs you") log("snap:fail hook chips: " + chips("hook").join());
      if (chips("shell").join() !== "Map chips") log("snap:fail shell chips: " + chips("shell").join());
      if (!document.querySelector("#map .plate[data-id=hook] .agent-chip.needs-you")) log("snap:fail the needs-you chip is not loud");
      const rest = [...document.querySelectorAll("#map-not-placed .agent-chip")].map((c) => c.textContent);
      if (rest.join("|") !== "#312 · CI marks|agy · Name from the prompt|codex · plotkeeper|#313 · Docs needs you") log("snap:fail not placed: " + rest.join("|"));
      if (document.querySelector("#map .agent-chip[title*=Helper], #map .agent-chip[title*='Read the log']")) log("snap:fail a Helper is on the map");
      log("chips ok, not placed " + rest.length);
      ${jump ? `document.querySelector("#map .plate[data-id=hook] .agent-chip").click();
      await wait(400);
      if (!document.getElementById("map").hidden) log("snap:fail a chip did not leave the Map view");` : ""}`, {
      map_state: mapState(),
      map_place: { s1: ["core", "window"], "s1/w1": ["shell"], s2: ["hook"], s6: [] },
    });
    return { ...f, sessions, selected: { session: sessions[6], agent: null } };
  };

// A map whose default branch's CI failed in a test that backs a law of the shell (#312): the red mark beside the Needs you chip, at the
// size of the busy fixture; `drawer` opens the proof of a law, `log` the broken-law drill-down.
const brokenCi: Ci = {
  status: "broken", run_url: "https://github.com/danielrispler/plotkeeper/actions/runs/9", commit: "35745b1693c5d061e5aea891e98a58515942a12d", earlier: false,
  parts: ["shell"], laws: ["wire-types-agree"], note: null,
  tail: "        FAIL [   0.013s] (4/4) studio-workbench::wire wire_types_match\n  stderr ───\n\n    thread 'wire_types_match' panicked at workbench/tests/wire.rs:11:25:\n    assertion `left == right` failed: wire drifted\n      left: 1\n     right: 2\n\n────────────\n     Summary [   0.014s] 4 tests run: 3 passed, 1 failed, 0 skipped",
};
const mapBroken = (open: "none" | "proof" | "log"): Fixture => {
  const sessions = busySessions();
  const opened = {
    none: "",
    proof: `document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const see = [...document.querySelectorAll("#map-sheet .sheet-law button")].find((b) => b.textContent === "See the proof");
      if (!see) return log("snap:fail no See the proof on a check-backed law");
      if (document.querySelectorAll("#map-sheet .sheet-law button").length !== 1) log("snap:fail See the proof is on a law no check backs");
      see.click();
      await wait(300);
      const code = document.querySelector("#map-drawer .drawer-code");
      if (!code || !code.textContent.includes("@law: wire-types-agree")) return log("snap:fail the proof drawer does not show the tag");
      const jumps = [...document.querySelectorAll("#map-drawer .drawer-jump")].map((b) => b.textContent);
      if (jumps.join() !== "Open in editor") log("snap:fail the proof's jumps are " + jumps);
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true }));
      await wait(100);
      if (document.querySelector("#map-drawer .drawer") || !document.querySelector("#map-sheet .sheet")) log("snap:fail the first Esc did not close the drawer alone");
      see.click();
      await wait(300);`,
    log: `document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const drill = [...document.querySelectorAll("#map-sheet .sheet-ci button")].find((b) => b.textContent === "See the CI log");
      if (!drill) return log("snap:fail no broken-law drill-down");
      drill.click();
      await wait(300);
      if (!document.querySelector("#map-drawer .drawer-code")?.textContent.includes("wire_types_match")) return log("snap:fail the log tail is not shown");
      const jumps = [...document.querySelectorAll("#map-drawer .drawer-jump")].map((b) => b.textContent);
      if (jumps.join() !== "Open on GitHub") log("snap:fail the log's jumps are " + jumps);`,
  }[open];
  const f = mapOpened(`
      await wait(400);
      const marked = [...document.querySelectorAll("#map .plate .law-broken")].map((m) => m.closest(".plate").dataset.id);
      if (marked.join() !== "shell") log("snap:fail the broken-law mark is on " + marked);
      if (!document.querySelector("#map .agent-chip.needs-you")) log("snap:fail no needs-you chip beside the mark");
      const red = getComputedStyle(document.querySelector("#map .plate .law-broken")).backgroundColor;
      const orange = getComputedStyle(document.querySelector("#map .agent-chip.needs-you")).color;
      if (red === orange) log("snap:fail the broken mark and Needs you are one colour");
      ${opened}
      log("broken " + marked.join());`, {
    map_state: mapState({ ci: brokenCi }),
    map_place: { s1: ["core", "window"], "s1/w1": ["shell"], s2: ["hook"], s6: [] },
    map_proof: [{ file: "workbench/tests/wire.rs", line: 11, test: "wire_types_match", first: 11, snippet: "// @law: wire-types-agree\n#[test]\nfn wire_types_match() {\n    studio_test_support::assert_wire_types_agree();\n}" }],
  });
  return { ...f, sessions, selected: { session: sessions[6], agent: null } };
};

// A part's history strip and a past commit (#313): the shell's sheet open on a part with four commits, the newest two not caught up on;
// scrubbing to an older one shows that commit's map (without the workbench) read-only, with no fog, broken-law mark or live agent and a
// banner; "Files" opens the commit's diff in the drawer; Esc then peels the drawer, the past commit, the sheet and the view, in that order.
const past = JSON.parse(mapFile);
past.parts = past.parts.filter((part: { id: string }) => part.id !== "workbench");
past.links = past.links.filter((link: { from: string; to: string }) => link.from !== "workbench" && link.to !== "workbench");
const strip = (n: number, label: string, fogged: boolean) => ({ commit: `${n}`.repeat(40), short: `${n}`.repeat(7), label, fogged });
const mapScrubbed = (): Fixture => {
  const esc = `document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true }));
      await wait(100);`;
  const f = mapOpened(`
      await wait(200);
      if (!document.querySelector("#map .plate .law-broken") || !document.querySelector("#map .agent-chip")) return log("snap:fail the live map lacks its mark or chips");
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(200);
      const rows = [...document.querySelectorAll("#map-sheet .hist-row")];
      if (rows.length !== 4) return log("snap:fail expected 4 strip rows, saw " + rows.length);
      if (rows.filter((r) => r.classList.contains("fogged")).length !== 2 || !rows[3].classList.contains("fogged") || rows[0].classList.contains("fogged")) log("snap:fail fogged rows");
      rows[1].querySelector(".hist-commit").click();
      await wait(200);
      const banner = document.getElementById("map-banner");
      if (!banner.textContent.includes("Viewing 2222222") || !banner.textContent.includes("Back to now")) return log("snap:fail banner: " + banner.textContent);
      if (document.querySelector("#map .agent-chip, #map-not-placed .agent-chip")) log("snap:fail a live agent shows in the past");
      if (document.querySelector("#map .plate .law-broken")) log("snap:fail a broken-law mark shows in the past");
      if (document.querySelector("#map .plate.fog-thin, #map .plate.fog-thick")) log("snap:fail fog shows in the past");
      if (document.querySelector("#map .plate[data-id=workbench]")) log("snap:fail the workbench was not yet there");
      if (!document.querySelector("#map-sheet .sheet")) log("snap:fail the sheet closed on a scrub");
      if (!document.querySelector("#map-sheet .hist-row.current")) log("snap:fail no current row");
      rows[1].querySelector(".hist-files").click();
      await wait(150);
      if (!document.querySelector("#map-drawer .drawer-code")?.textContent.includes("+two")) return log("snap:fail the diff is not in the drawer");
      if ([...document.querySelectorAll("#map-drawer .drawer-jump")].map((b) => b.textContent).join() !== "Open on GitHub") log("snap:fail the commit's jump");
      ${esc}
      if (document.querySelector("#map-drawer .drawer") || !banner.textContent.includes("Viewing")) return log("snap:fail the first Esc did not close the drawer alone");
      document.querySelectorAll("#map-sheet .hist-row")[1].querySelector(".hist-files").click();
      await wait(150);
      ${esc}
      ${esc}
      if (banner.textContent !== "" || !document.querySelector("#map .plate .law-broken") || !document.querySelector("#map-sheet .sheet")) return log("snap:fail the second Esc did not return to now alone: " + banner.textContent);
      document.querySelectorAll("#map-sheet .hist-row")[1].querySelector(".hist-commit").click();
      await wait(200);
      if (!document.getElementById("map-banner").textContent.includes("Viewing")) return log("snap:fail could not scrub again");
      log("scrubbed ok");`, {
    map_state: mapState({ ci: brokenCi, fog: { shell: "thick", core: "thin" } }),
    map_place: { s1: ["core", "window"], "s1/w1": ["shell"], s2: ["hook"], s6: [] },
    map_history: [strip(1, "#101 Map first drawn", false), strip(2, "#150 Plates and links", false), strip(3, "#210 Fog engine", true), strip(4, "#306 Live agents on the map", true)],
    map_at: { commit: "2".repeat(40), short: "2222222", label: "#150 Plates and links", source: JSON.stringify(past) },
    map_commit: { commit: "2".repeat(40), short: "2222222", label: "#150 Plates and links", url: "https://github.com/danielrispler/plotkeeper/commit/" + "2".repeat(40), files: [{ path: "src-tauri/src/map.rs", diff: "@@ -1,2 +1,2 @@\n-one\n+two" }, { path: "ui/map.ts", diff: "@@ -1 +1,2 @@\n+three" }] },
  });
  return { ...f, sessions: busySessions(), selected: { session: busySessions()[6], agent: null } };
};

export const fixtures = {
  ...railFixtures,
  mapTray,
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
  // The New session picker open (#220), so every page's hover check reaches its agent choice, quick start and worktree rows.
  pickerOpen() {
    const f = branchOnce();
    return { ...f, agents: ["claude", "agy", "codex"], snapScript: `document.getElementById("new-session").click(); await new Promise((done) => setTimeout(done, 500));` };
  },
  // The palette open (#220), so the hover check reaches its rows.
  paletteOpen() {
    const snapScript = `document.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyP", key: "P", metaKey: true, shiftKey: true, bubbles: true }));
      await new Promise((done) => setTimeout(done, 300));
      if (!document.querySelector("#picker .row")) log("snap:fail the palette did not open");`;
    return { ...list(), snapScript };
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
  // Home, the waiting room (#238), at 1400x900: 4 running, 1 needs you, 1 unseen, the learning slot with a Retest, 2 landed and
  // 2 projects. The script checks the Retest sits fully above the fold.
  home() {
    const now = Date.now();
    const running = (id: string, name: string, minutes: number, intention: string, steps: number) =>
      session(id, name, "running", { cwd: "/w/plotkeeper", turn_span: { start: now - minutes * 60e3, end: null }, transcript: { ...session("x", "x", "idle").transcript, intention, steps_ago: steps } });
    const asking = session("s2", "#57 · Name from the prompt", "needs_you", {
      cwd: "/w/plotkeeper",
      questions: [{ tool_use_id: "q1", agent: null, answer: "open", turn: null, questions: [{ question: "Should the name come from the first prompt or from the branch?", options: [] }] }],
    });
    const sessions = [
      running("s1", "#56 · Row controls", 14, "Make the row's controls reachable by keyboard.", 2),
      asking,
      running("s6", "#58 · Retest card", 9, "Draw the quiz region with its options and Not now.", 0),
      running("s7", "#59 · Agent deadline", 6, "Stop an agent that outlives its turn.", 7),
      running("s8", "#61 · Sounds setting", 2, "Add the sounds toggle to the settings page.", 1),
      session("s4", "#55 · Delete default", "your_turn", {
        cwd: "/w/plotkeeper", unseen: true, last_event_at: now - 22 * 60e3,
        turns: [{ started_at: now - 40 * 60e3, ended_at: now - 22 * 60e3, prompt: "Make delete keep the branch", id: null, message: "Delete now removes the worktree and keeps the branch.", checks: [], listed: [], files: null }],
      }),
      session("s3", "#54 · Subagent terminal", "idle", { cwd: "/w/plotkeeper", landed: true, identity: { issue: 54, branch: null, pr: null, worktree: "", name: "#54 · Subagent terminal" } }),
      session("s5", "#48 · Agent deadline", "ended", { cwd: "/w/plotkeeper", landed: true }),
    ];
    const learning = { kind: "retest" as const, id: "r1", of: "r1", session_id: "s3", added_at: now - 7 * 864e5, of_at: now - 8 * 864e5, taps: [], quiz: { text: "The settings write held a lock across a read. Why was it safe?", key: 1, options: [
      { text: "Keep the lock across the await so nobody reads half a write", why: "A lock across the await deadlocks." },
      { text: "Release it first and re-read the state after", why: "The read can wait on the writer." },
      { text: "Copy the state out and work on the copy", why: "It still races." },
    ] } };
    const snapScript = `
      const learning = document.querySelector(".pane.home .slot-card");
      if (!learning) log("snap:fail Home has no learning card");
      else if (!/^Retest · \\d+ days after #54/.test(learning.querySelector(".label")?.textContent ?? "")) log("snap:fail the card has no Retest label");
      else if ([...document.querySelectorAll(".pane.home h2")].some((h) => h.textContent === "Retest")) log("snap:fail Home still has a Retest heading");
      // The window is 900 high; the page reports less while this script runs (headless Chrome counts its frame), so the fold is fixed.
      else if (learning.getBoundingClientRect().bottom > 900 - 6) log("snap:fail the Retest is below the fold: it ends at " + learning.getBoundingClientRect().bottom);`;
    return { ...base, sessions, learning, projects: ["/w/plotkeeper", "/w/falinks"], selected: null, snapScript };
  },
  // Home's reveal (#242): the script picks the second option of the Retest; the card then shows the turn's date and Key wrong, and no Not now.
  homeReveal(): Fixture {
    const snapScript = `
      const option = document.querySelectorAll(".pane.home .quiz-region .options button")[1];
      if (!option) log("snap:fail Home's Retest has no options");
      else {
        option.click();
        await new Promise((r) => setTimeout(r, 100));
        const card = document.querySelector(".pane.home .quiz-region")?.textContent ?? "";
        if (!/The turn of \\d+ \\w+/.test(card)) log("snap:fail the reveal shows no date of the turn");
        if (!/Key wrong/.test(card)) log("snap:fail the reveal has no Key wrong");
        if (/Not now|Show the rest anyway|why\\?/.test(card)) log("snap:fail the reveal shows a rail control");
      }`;
    return { ...fixtures.home(), snapScript };
  },
  // Home with a Retest answered (#243): the reveal, the link to the code, and the stored hunk in place. The script checks both show.
  homeReveal243(): Fixture {
    const f = fixtures.home();
    const now = Date.now();
    const file = { root: "/w/plotkeeper", path: "ui/settings.rs", from: null, added: 1, removed: 1, agent: null };
    const sessions = f.sessions.map((s) =>
      s.id === "s3" ? { ...s, turns: [{ started_at: now - 8 * 864e5, ended_at: now - 8 * 864e5 + 600e3, prompt: "Fix the settings write", id: "r1", message: null, checks: [], listed: [], files: { files: [file], merged: 0 } }] } : s,
    );
    // The hunk is the item's stored text, drawn with no git read (the fixture has no turn_file_hunk).
    const hunk = ["--- ui/settings.rs", "@@ -41,3 +41,3 @@", "-    let text = self.read_file(&self.state.lock());", "+    let text = { self.state.lock().file.clone() };", "     self.write_file(merge(text, key, value));", ""].join("\n");
    const learning = { ...f.learning, hunk, taps: [{ kind: "pick" as const, option: 0, at: now }] };
    const snapScript = `
      await new Promise((done) => setTimeout(done, 300));
      const card = document.querySelector(".pane.home .slot-card");
      if (!card?.querySelector(".quiz-code-link")) log("snap:fail the reveal has no link to the code inside the card");
      if (!card?.querySelector(".evidence-hunk")?.textContent.includes("self.state.lock().file.clone()")) log("snap:fail the stored hunk is not drawn in place inside the card");
      if (card?.querySelector(".evidence-caption")?.textContent !== "ui/settings.rs") log("snap:fail the hunk has no file caption");
      for (const control of card?.querySelectorAll(".evidence-edit, .ask-about") ?? []) if (getComputedStyle(control).display !== "none") log("snap:fail the compact hunk shows " + control.className);
      // The card sits below the fold of a full page: scroll it into view to see the reveal whole.
      card?.scrollIntoView({ block: "start" });`;
    return { ...f, sessions, learning, snapScript };
  },
  // Home after Clear (#244): the landed rows are hidden, the Landed part is gone, and the toast offers Undo. The script clicks Clear on the heading.
  homeClearToast(): Fixture {
    const snapScript = `
      const clear = document.querySelector(".pane.home h2 .clear");
      if (!clear) log("snap:fail the Landed heading has no Clear");
      else {
        clear.click();
        await new Promise((r) => setTimeout(r, 100));
        const toast = document.getElementById("clear-toast");
        if (toast.hidden || !/Undo/.test(toast.textContent)) log("snap:fail Clear shows no toast with Undo");
        if (document.querySelector(".pane.home h2 .clear")) log("snap:fail the landed rows are still shown after Clear");
      }`;
    return { ...fixtures.home(), snapScript };
  },
  // Home refreshes on a stored card or summary (#226): the summary of an Unseen row's turn resolves after the first draw and the row's Headline
  // changes; the slot answers nothing, then a fresh Quiz, and the `cards` event (a card stored) makes Home show it.
  homeRefresh(): Fixture {
    const f = fixtures.home();
    const now = Date.now();
    const sessions = f.sessions.map((s) => (s.id === "s4" ? { ...s, turns: s.turns.map((t) => ({ ...t, id: "t4" })) } : s));
    const summary = { summary: { summary: "Delete keeps the branch.", headline: "Delete keeps the branch (summary).", lines: [], quiz: null } };
    const fresh = { kind: "fresh", id: "f1", session_id: "s3", at: now, hunk: "", quiz: { text: "Why does delete keep the branch?", key: 0, options: [{ text: "The branch holds the work", why: "Right." }, { text: "Git forbids deleting it", why: "It does not." }] } };
    const snapScript = `
      // The summary lands 3 s after the first draw (the script runs at 2.5 s), after the learning slot's reply: only a refresh draws its Headline.
      const headline = () => document.querySelector('.pane.home [data-key="unseen s4"] .row-step')?.textContent;
      if (headline() !== "Delete now removes the worktree and keeps the branch.") log("snap:fail the row does not show the turn's message before its summary: " + headline());
      await new Promise((done) => setTimeout(done, 1000));
      if (headline() !== "Delete keeps the branch (summary).") log("snap:fail an Unseen row's Headline did not update once its summary arrived: " + headline());
      if (document.querySelector(".pane.home .slot-card")) log("snap:fail the slot shows an item before the store holds one");
      const invoke = window.__TAURI__.core.invoke;
      window.__TAURI__.core.invoke = (command, args) => (command === "learning" ? Promise.resolve(${JSON.stringify(fresh)}) : invoke(command, args));
      window.__TAURI__.__emit("cards", await invoke("cards"));
      await new Promise((done) => setTimeout(done, 200));
      if (!document.querySelector(".pane.home .slot-card")?.textContent.includes("Why does delete keep the branch?")) log("snap:fail Home does not show the Quiz a stored card made");`;
    return { ...f, sessions, learning: null, turn_summary: { __by: "id", values: { t4: { __delay: 3000, value: summary } as never } }, snapScript };
  },
  // "See the code" (#243): from Home's reveal of a Retest whose session is listed, the link selects the session, opens the Quiz's turn (an older
  // one) and its hunk in the rail, opens the card when the Quiz is a card's, scrolls the turn into view and, in the overlay layout, opens the rail.
  homeSeeCode: () => seeCode(false),
  homeSeeCodeCard: () => seeCode(true),
  // Errors in the notice line (#191): two errors and the dismiss control, beside a settings error.
  noticeErrors() {
    const snapScript = `
      await new Promise((r) => setTimeout(r, 100));
      const notice = document.getElementById("notice");
      const box = notice.getBoundingClientRect();
      const button = document.getElementById("notice-dismiss").getBoundingClientRect();
      if (notice.hidden || notice.textContent.split("\\n").length !== 3) log("snap:fail the notice line does not hold the settings error and the two errors");
      if (button.right > innerWidth || box.right > button.left) log("snap:fail the notice line or its dismiss control is clipped");`;
    const appErrors = [
      "save_cards: the card store did not save (No space left on device)",
      "delete_session: could not remove the worktree /w/plotkeeper/.claude/worktrees/issue-191-notice-errors-with-a-very-long-name",
    ];
    return { ...list(), settings_files: { settings: "{ bad", keymap: null }, appErrors, snapScript };
  },
  // The settings page at its defaults: no undo icon, both toggles at their defaults, the file's two rows.
  settingsDefaults: () =>
    settingsPage(null, `
      if (undo() !== 0) log("snap:fail an undo icon shows at the defaults");
      if (toggles().join() !== "true,false") log("snap:fail the toggles read " + toggles());
      if (broken()) log("snap:fail the broken-file line shows");`),
  // Two values changed in the file: each row shows its undo icon.
  settingsChanged: () =>
    settingsPage(`{\n  // Quiet.\n  "sounds": false,\n  "context_warning": 20,\n}\n`, `
      if (undo() !== 2) log("snap:fail expected 2 undo icons, got " + undo());
      if (toggles().join() !== "false,false") log("snap:fail the toggles read " + toggles());
      const warning = [...dialog.querySelectorAll(".stepper-value")].map((v) => v.textContent)[2];
      if (warning !== "20") log("snap:fail the context warning reads " + warning);
      if (dialog.querySelector(".reset").title !== "Reset to default") log("snap:fail the undo icon's tooltip is wrong");
      if (document.getElementById("settings-close").title !== "Close settings (Esc)") log("snap:fail the X's tooltip is " + document.getElementById("settings-close").title);`),
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
  // A file that does not parse: the line, the button, and every setting's control off; the Open buttons stay on (#221).
  settingsBroken: () =>
    settingsPage(`{ "sounds": false, `, `
      if (!broken().startsWith("settings.json has errors: fix it first")) log("snap:fail the line reads: " + broken());
      if (!toggles().every((t) => t.endsWith("disabled"))) log("snap:fail a toggle is enabled: " + toggles());
      if ([...dialog.querySelectorAll("select, .stepper .btn")].some((c) => !c.disabled)) log("snap:fail a stepper or dropdown is enabled");
      if ([...dialog.querySelectorAll("button.open")].some((b) => b.disabled)) log("snap:fail an Open button is disabled");`),
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
  // Alerts: two sessions' turns ended off screen (the note toast and the unseen chip), and one session past the 30% context
  // warning, whose row mark reads at full strength.
  alerts() {
    const context = (share: number) => ({ ...session("x", "x", "idle").transcript, context: { tokens: share * 10000, share, window: 1000000, left: 100 - share } });
    const sessions = [
      session("s1", "#56 · Row controls", "running", { transcript: context(34) }),
      session("s2", "#57 · Name from the prompt", "running", { transcript: context(12) }),
      session("s3", "#54 · Subagent terminal", "your_turn", { unseen: true, transcript: { ...context(20), intention: "The subagent's terminal opens beside its parent's" } }),
      session("s4", "#55 · Delete default", "your_turn", { unseen: true }),
    ];
    const snapScript = `
      window.__TAURI__.__emit("finished", ["s3", "s4"]);
      await new Promise((done) => setTimeout(done, 50));
      const note = document.getElementById("note-toast");
      if (note.hidden || !note.textContent.includes("2 finished")) log("snap:fail the note toast should say 2 finished");
      const unseen = document.getElementById("unseen");
      if (unseen.hidden || unseen.textContent !== "2 unseen") log("snap:fail the unseen chip should say 2 unseen");
      const past = [...document.querySelectorAll(".row .id.past")].map((mark) => mark.textContent);
      if (past.join() !== "context 34%") log("snap:fail only the 34% mark should be past the warning: " + past.join());
      log("note: " + note.textContent + " · chip: " + unseen.textContent + " · past: " + past.join());`;
    return { ...base, sessions, selected: { session: sessions[1], agent: null }, snapScript };
  },
  // The calm Map view: plates, law marks of both kinds, a black box part, actors, and the session list live beside it.
  mapCalm: () =>
    mapOpened(`
      const plates = document.querySelectorAll("#map .plate").length;
      const checks = document.querySelectorAll("#map .plate .law-mark.check").length;
      const trusts = document.querySelectorAll("#map .plate .law-mark.trust").length;
      if (plates !== 8) log("snap:fail expected 8 plates, saw " + plates);
      if (checks !== 6 || trusts !== 5) log("snap:fail law marks: " + checks + " check, " + trusts + " trust");
      if (document.querySelectorAll("#map .link").length !== 10) log("snap:fail expected 10 links");
      if (document.querySelectorAll("#map .actor").length !== 3) log("snap:fail expected 3 actors");
      if (document.querySelector("#map .plate[data-id=workbench]").className.indexOf("tier-black-box") < 0) log("snap:fail workbench is not a black box plate");
      const list = document.querySelector(".list").getBoundingClientRect();
      const map = document.getElementById("map").getBoundingClientRect();
      if (map.right > list.left + 1) log("snap:fail the map overlaps the session list");
      log("plates " + plates + ", marks " + checks + "/" + trusts);`, { map_state: mapState() }),
  // Home's quiet mark (#310): one row of two carries it, with no count.
  homeFogMark: (): Fixture => homeMarked(false),
  // Pressing the mark opens that project's Map view, not the new session its row starts.
  homeFogMarkOpens: (): Fixture => homeMarked(true),
  // The Map view with fog (#310): thin and thick fog on Core and interface parts, and the part sheet of a thick one open.
  mapFog: () =>
    mapOpened(`
      const thin = document.querySelectorAll("#map .plate.fog-thin").length;
      const thick = document.querySelectorAll("#map .plate.fog-thick").length;
      if (thin !== 2 || thick !== 2) log("snap:fail expected 2 thin and 2 thick plates, saw " + thin + " and " + thick);
      const name = document.querySelector("#map .plate.fog-thick .plate-name");
      if (!name || getComputedStyle(name).opacity !== "1") log("snap:fail a fogged part's name is not at full strength");
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const sheet = document.querySelector("#map-sheet .sheet");
      if (!sheet) return log("snap:fail the sheet did not open");
      const pressed = [...sheet.querySelectorAll(".sheet-tiers [aria-pressed=true]")].map((b) => b.textContent);
      if (pressed.join() !== "Core") log("snap:fail the tier pressed is " + pressed);
      if (sheet.querySelectorAll(".sheet-law").length !== 2) log("snap:fail expected 2 laws on the sheet");
      if (![...sheet.querySelectorAll("button")].some((b) => b.textContent === "I know this")) log("snap:fail no I know this");
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true }));
      await wait(100);
      if (document.querySelector("#map-sheet .sheet")) log("snap:fail the first Esc did not close the sheet");
      if (document.getElementById("map").hidden) log("snap:fail the first Esc left the Map view");
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      log("thin " + thin + ", thick " + thick);`, { map_state: mapState({ fog: { shell: "thick", window: "thick", core: "thin", hook: "thin" } }), map_know: { positions: {}, tiers: {} } }),
  // The Map view busy (#311): a session on two parts, a Needs you one, a Worker with its own chip, and the not placed list (no files yet,
  // agy, status-only Codex, ignored-only edits); a Helper and a finished session never show.
  mapBusy: () => mapBusy(false),
  mapChipJump: () => mapBusy(true),
  // CI on the default branch broke a law (#312): the red mark on the shell next to a Needs you chip; then with "See the proof" open, and
  // with the broken-law drill-down open.
  mapBroken: () => mapBroken("none"),
  mapProof: () => mapBroken("proof"),
  mapBrokenLog: () => mapBroken("log"),
  // A part's history strip scrubbed to an older commit, with the sheet open (#313).
  mapScrubbed,
  // The one-time review under the Map view's head, listing the modules below their thresholds (#314).
  mapReview: () =>
    mapOpened(`
      const review = document.querySelector("#map-review .review");
      if (!review || document.getElementById("map-review").hidden) return log("snap:fail the review is not shown");
      if (!review.textContent.includes("Little used so far: the Map view, the history strip and See the proof")) log("snap:fail the review's words: " + review.textContent);
      if (/\\d/.test(review.querySelector(".review-text").textContent)) log("snap:fail the review shows a count");
      log("review ok");`, { map_state: mapState(), stop_facts: dueFacts }),
  // The same review on Home, when the Map view itself fell below its threshold (#314).
  homeReview: homeReviewed,
  // No map on the default branch: the empty state, and "Draft a map" opens New session with the drafting prompt.
  mapEmpty: () =>
    mapOpened(`
      const button = [...document.querySelectorAll("#map .map-empty .btn")].find((b) => b.textContent === "Draft a map");
      if (!button) return log("snap:fail no Draft a map button");
      button.click();
      await wait(500);
      const field = document.querySelector("#picker input");
      if (document.getElementById("picker").hidden || !field) return log("snap:fail New session did not open");
      if (!field.value.includes("system-map.json") || !field.value.includes("system-map check")) log("snap:fail the quick start lacks the drafting prompt: " + field.value);
      log("drafting prompt: " + field.value.slice(0, 40));`, { map_state: mapState({ status: "none", source: null }) }),
  branchOnce,
  pullEnabled: pullMain({}, false),
  pullDisabled: pullMain({ changed: 2, would_overwrite: true }, true),
} satisfies Record<string, () => Fixture>;

