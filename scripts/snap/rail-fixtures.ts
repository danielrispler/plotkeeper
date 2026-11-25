// Fixtures for the story rail (from the design session for #8, danielrispler/plotkeeper#102): one selected session,
// "#56 · Row controls", in four states (running, at your turn, needing you, idle), with realistic content: 5 turns, summaries as the writer's prompt shapes them (at most 4
// short lines, the first says what the turn did),
// subagents, checks, a permission mode, a model, an estimate, a brief and decision history, and a
// Git line with several tokens. Times are relative to the moment a fixture renders.

import type { Ask } from "../../ui/types.ts";
import type { BlockedCommand, ListedCheck, CheckStatus, Decision, OpenItem, RiskView, Session, SessionState, Subagent, Turn, TurnFile, TurnSummary } from "../../ui/types.ts";
import { base, type Fixture } from "./base.ts";

const MIN = 60e3;

const capabilities = { agent: "claude", asks: true, background_tasks: true, replies: true, permission_mode: true, durations: true, transcript_version: true, transcript_model: true, context: true, turn_trees: true };
const cwd = "/Users/danielrispler/work/plotkeeper/.claude/worktrees/issue-56-row-controls";

// What each turn's summary says; its first line is the turn's Headline (the turn-end intention is the final response's first line).
const summaries: Record<string, TurnSummary["summary"]> = {
  t4: {
    summary:
      "Fixed the two failures `npm run check` found: a clippy lint in a Rust test helper and a UI test that still expected the old row padding.\n" +
      "The row's padding now comes from the fold and delete columns (`--control`), so the test reads the token instead of a hard-coded 12px.\n" +
      "Clippy's `needless_borrow` was unrelated to the UI and is fixed with a one-line change.\n" +
      "`npm run check` passes; nobody looked at the rows in a real window.",
  },
  t3: {
    summary:
      "Made the delete button show on hover and on the selected row, and kept its column empty otherwise.\n" +
      "Keyboard users get it through `:focus-within`, so Tab reaches it.\n" +
      "Not checked: the strip, where rows have no delete.",
  },
  t1: {
    summary:
      "Added a fold toggle to each session row and moved the delete button into its own column.\n" +
      "The toggle sits at the row's start and is drawn only when subagents show under the row.\n" +
      "Both are siblings of the row button, since a button cannot hold a button.",
  },
};

const prompts = [
  "Read issue #56 and plan the row controls",
  "Implement the plan, starting with the fold toggle",
  "worker w1 returned",
  "The delete should show only on hover, and stay on the selected row",
  "Run npm run check and fix what fails",
];

const listed = (statement: string, status: CheckStatus, startedAgo: number, tookMs: number, typical_ms: number | null = null): ListedCheck => ({
  statement, status, started_at: Date.now() - startedAgo, ended_at: status === "running" ? null : Date.now() - startedAgo + tookMs, typical_ms,
});

const file = (path: string, added: number, removed: number, agent: string | null = null): TurnFile => ({ root: cwd, path, from: null, added, removed, agent });
const run = (status: CheckStatus, call: string, exit_code: number, agent: string | null = null) => ({ command: "cargo", statement: "cargo", status, exit_code, call, agent, started_at: null, ended_at: null });

const helper = (id: string, name: string, asked: string, type: string, result: string, turn: number, ago: number, running = false): Subagent => ({
  id, parent: "session", kind: "helper", name, asked, agent_type: type, status: running ? "running" : "completed",
  started_at: Date.now() - ago, ended_at: running ? null : Date.now() - ago + 3 * MIN, last_step: running ? { tool: "Grep", detail: "#[0-9a-f]{3,8} ui/style.css" } : null,
  step_span: null, result: running ? null : result, failed: false, opened: true, faded: !running, turn,
});

const final = [
  "Both failures are fixed and `npm run check` passes.",
  "",
  "**Clippy.** `needless_borrow` in `ui/tests` helper `rail_html`; I dropped the `&`.",
  "",
  "**The test.** `rows.test.mjs` expected 12px of padding on a session row. The row's padding is now `calc(var(--sp-4) + var(--control))`, so the test reads `--control` from `tokens.css`.",
  "",
  "I did not open the window; the rows are checked by their CSS and by the tests only.",
].join("\n");

// biome-ignore lint/complexity/noExcessiveLinesPerFunction: a fixture's whole session, one field a line; spec #548 Group 4 splits the snap fixtures
const session = (state: SessionState, { running = false, question = false } = {}): Session => {
  const now = Date.now();
  const ended = !running && state !== "needs_you";
  const intentions = {
    running: "Reading the delete handler to see why the fold toggle shares its column with the glyph.",
    needs_you: "I need one decision before I change the row layout.",
  };
  const stepTools = { tool: "Edit", detail: `${cwd}/ui/list.ts` };
  const turnStart = [now - 5 * 60 * MIN, now - 4 * 60 * MIN, now - 3 * 60 * MIN, now - 2 * 60 * MIN, now - 40 * MIN];
  const turns = prompts.map((prompt, i): Turn => ({
    started_at: turnStart[i],
    ended_at: i < 4 ? turnStart[i] + 20 * MIN : ended ? now - 6 * MIN : null,
    prompt,
    id: i < 4 || ended ? `t${i}` : null,
    message: "Done.",
    // The newest turns changed files (#113); the second turn ends with a failed check, which its folded line marks (#117).
    files: i >= 3 ? { files: [file("ui/style.css", 14, 3, "claude"), file("ui/list.ts", 22, 9, "claude"), file("ui/tests/rows.test.mjs", 6, 6)], merged: 0 } : null,
    // The feed lists the newest turn's check that runs or failed (#268).
    listed: i !== 4 ? [] : running
      ? [listed("npm run check", "running", 3 * MIN, 0, 5 * MIN + 40e3)]
      : [listed("cargo clippy --locked --workspace --all-targets -- -D warnings", "failed", 38 * MIN, 41e3)],
    checks: i === 4
      ? [
          { suite: "cargo clippy", worktree: "/r", foreign: false, status: "passed", retries: 1, runs: [run("failed", "k1", 101), run("passed", "k2", 0, "w1")] },
          { suite: "npm run check", worktree: "/r/.claude/worktrees/fix", foreign: true, status: "failed", retries: 0, runs: [run("failed", "k3", 1)] },
        ]
      : [{ suite: "check", worktree: "/r", foreign: false, status: i === 1 ? "failed" : "passed", retries: 0, runs: [] }],
  }));
  const transcript = {
    intention: ended ? final.split("\n")[0] : intentions[state === "needs_you" ? "needs_you" : "running"],
    steps_ago: ended || state === "needs_you" ? 0 : 7,
    responses: ended ? ["I'll run the full check first.", final] : ["I'll look at how the row lays out its controls."],
    unavailable: null,
    context: { tokens: 85000 },
    branch: "issue-56-row-controls",
    model: "claude-opus-5-5",
    effort: "high",
  };
  return {
    id: "s1", capabilities, model: null, cwd, folder_name: "issue-56-row-controls", terminal: null, status_only: false, shell: false, name: null,
    prompt_name: "Row controls", resume_id: "s1", permission_mode: "auto", started_mode: "auto", cleared: [], asks_before_closing: state !== "idle",
    landed: false, last_step: ended ? { tool: "Bash", detail: "Run the full check", command: "npm run check" } : stepTools,
    started_at: now - 5 * 60 * MIN, last_event_at: now - 30e3, turn_span: running ? { start: now - 14 * MIN, end: null } : null,
    step_span: running ? { start: now - 90e3, end: null } : null, state, agents: (running ? (all: Subagent[]) => all.filter((a) => a.id !== "a2") : (all: Subagent[]) => all)([
      helper("a2", "Write tests for the fold toggle", "Write node tests for the fold toggle in ui/list.ts", "general-purpose", "Added 4 tests; all pass.", 1, 4 * 60 * MIN - 5 * MIN),
      helper("a3", "Find where the delete is styled", "Find every rule that styles `.delete`.", "Explore", "Two rules in `ui/style.css`, none in `ui/list.ts`.", 3, 2 * 60 * MIN),
      helper("a1", "Review the diff", "Review the diff on this branch and list anything that breaks the token rules.", "Explore", "No raw colours; two spacing values bypass the tokens.", 4, 30 * MIN, running),
    ]), asker: state === "needs_you" ? { agent: null, since: now - 3 * MIN } : null,
    prompts: [], turns, blocked_commands: [], returned: null, seen: 3, since_looked: { turns: 2, files: 3, passed: 1, failed: 1, ran: 0 },
    questions: question ? [{
      tool_use_id: "toolu_q1", agent: null, turn: 4, answer: "open",
      questions: [{
        question: "Which column should hold the delete button on a row with no subagents?", header: "Delete column", multiSelect: false,
        options: [
          { label: "Keep its column", description: "Reserve the space, so names and glyphs stay aligned on every row." },
          { label: "Collapse the column", description: "Names move left by 20px on rows that cannot be deleted." },
          { label: "Show it in the strip too", description: "The strip gets a delete as well; its buttons are 52px wide." },
        ],
      }],
    }] : [],
    transcript,
    identity: { issue: 56, branch: "issue-56-row-controls", pr: 61, worktree: cwd, name: "#56 Row controls" },
    work: {
      kind: "implement", agent_ms: 50 * MIN,
      phases: [{ phase: state === "idle" ? "waiting_on_ci" : "implementing", at: now - 2 * 60 * MIN, agent_ms: 20 * MIN }],
      estimate: { range: state === "idle" ? null : [16 * MIN, 29 * MIN], depends_on_you: state === "needs_you" },
    },
    unseen: state === "your_turn", line: running ? "Edit ui/list.ts" : "Needs you · 3m",
  };
};

const others = () => {
  const now = Date.now();
  const other = (id: string, name: string, state: SessionState, extra: Partial<Session> = {}): Session => ({
    ...session("your_turn"), id, resume_id: id, prompt_name: name.slice(name.indexOf("·") + 2), identity: { issue: Number(name.slice(1, 3)), branch: null, pr: null, worktree: "", name },
    cwd: `/Users/danielrispler/work/plotkeeper/.claude/worktrees/issue-${name.slice(1, 3)}`, folder_name: `issue-${name.slice(1, 3)}`, state, agents: [], questions: [], turns: [],
    asker: null, unseen: false, line: undefined, landed: false, started_at: now - 3600e3, ...extra,
    transcript: { ...session("your_turn").transcript, context: { tokens: 120000 } },
  });
  return [
    other("s2", "#57 · Name from the prompt", "needs_you", { line: "Which name should a session take from its prompt? · 2m", asker: { agent: null, since: now - 2 * MIN }, questions: [{ tool_use_id: "q", agent: null, turn: 0, answer: "open", questions: [{ question: "Which name should a session take from its prompt?", options: [] }] }] }),
    other("s4", "#55 · Delete default", "running", { line: "Bash cargo test --locked --workspace" }),
    other("s5", "#54 · Subagent terminal", "your_turn", { unseen: true }),
    other("s6", "#48 · Agent deadline", "idle", { landed: true }),
    other("s7", "#41 · Git line in the top bar", "ended"),
  ];
};

const common = {
  ...base,
} satisfies Fixture;

const goal = "Give each session row a fold toggle and a delete, drawn only where a row can use them.";
const answers = [{ question: "Hide the delete with display or visibility?", reply: "visibility" }];
const open: OpenItem[] = [
  { kind: "check", text: "npm run check ended failed" },
  { kind: "open", text: "The strip draws no delete; the History rows show it always." },
  { kind: "unpushed", text: "Uncommitted or unpushed work" },
];

const design = [
  { text: "Hide the delete with visibility, because display would move the row's text.", cite: "" },
];

const decisions: Decision[] = [
  { call: "c1", agent: null, tool: "AskUserQuestion", input: { questions: [{ question: "Hide the delete with display or visibility?", options: [] }] }, answer: { "Hide the delete with display or visibility?": "visibility" }, at: Date.now() - 3 * 60 * MIN, source: "card" },
  { call: "c2", agent: null, tool: "ExitPlanMode", input: { plan: "Add a fold toggle and a delete column to each row." }, answer: "allow", at: Date.now() - 4 * 60 * MIN, source: "card" },
  { call: "c3", agent: null, tool: "Bash", input: { command: "git push -u origin issue-56-row-controls" }, answer: "allow", at: Date.now() - 90 * MIN, source: "terminal" },
];

const questionAsk: Ask = { nonce: "n1", hook_event_name: "PreToolUse", tool_name: "AskUserQuestion", tool_use_id: "toolu_q1", agent_id: null, tool_input: {} };

// The fixture for a state: its session, the list around it and its brief.
const build = (state: SessionState, { ask = null as Ask | null, withBrief = false } = {}) => {
  const running = state === "running";
  const s = session(state, { running, question: ask === questionAsk });
  const sessions = [s, ...others()].sort((a, b) => ["needs_you", "running", "your_turn", "idle", "ended"].indexOf(a.state) - ["needs_you", "running", "your_turn", "idle", "ended"].indexOf(b.state));
  const turnSummaries = { ...summaries };
  if (running) delete turnSummaries.t4;
  return {
    ...common,
    sessions,
    selected: { session: s, agent: null },
    cards: { brief: null, goal: withBrief ? goal : null, answers: withBrief ? answers : [], open: withBrief ? open : [], design: withBrief ? design.map((line) => ({ ...line, cite: "t1" })) : [], decisions: withBrief ? decisions : [], risk_notes: [], decision_notes: [] },
    watch: ask,
    ask_facts: {
      sections: [
        { label: "Daniel asked", facts: [{ at: null, text: "Implement the plan, starting with the fold toggle" }] },
        { label: "Steps that edited it", facts: [{ at: Date.now() - 40 * MIN, text: "Edit by the session's agent" }, { at: Date.now() - 38 * MIN, text: "Edit by Explore the rows" }] },
        { label: "Read or searched before the first edit", facts: [{ at: Date.now() - 45 * MIN, text: "Read ui/list.ts" }, { at: Date.now() - 44 * MIN, text: "Grep fold" }] },
      ],
    },
    check_output: { exit_code: 1, tail: "error: test failed, to rerun pass `-p studio-core --lib`\n\nfailures:\n    rows::fold_toggle_is_a_sibling" },
    turn_file_hunk: { text: ["diff --git a/ui/style.css b/ui/style.css", "--- a/ui/style.css", "+++ b/ui/style.css", "@@ -10,4 +10,6 @@", " .rail {", "-  gap: 4px;", "+  gap: var(--sp-3);", "+  padding: 0;", " }", ""].join("\n"), left_out: 0, binary: false, first_line: 11 },
    turn_summary: { __by: "id", values: Object.fromEntries(Object.entries(turnSummaries).map(([id, summary]) => [id, { summary }])) },
    turn_reply: { __by: "index", values: { 0: "Plan: a fold toggle and a delete column.\n\nThe toggle is drawn only when subagents show under the row.", 1: "Implemented the toggle and the delete." } },
    agent_summary: { __by: "agentId", values: { a1: "Reviewed the diff: no raw colours, two spacing values bypass the tokens." } },
  } satisfies Fixture;
};

// The top bar (#127, #386): no item is cut. Every item in the bar sits whole inside it; the project's name is the one that may ellipsize.
const barScript = `
  const top = document.querySelector(".top");
  const room = top.getBoundingClientRect().right;
  for (const item of top.children) {
    if (item.hidden || item.classList.contains("sp") || item.classList.contains("menu")) continue;
    const box = item.getBoundingClientRect();
    if (box.right > room + 0.5) log("snap:fail the bar cuts #" + item.id + " at " + innerWidth);
    if (item.scrollWidth > item.clientWidth + 1 && item.id !== "bar-project") log("snap:fail #" + item.id + " is clipped at " + innerWidth);
  }
  if (document.getElementById("bar-project").hidden) log("snap:fail the project's name is missing");
  log("bar at " + innerWidth + ": " + [...top.children].filter((i) => !i.hidden && i.id).map((i) => i.id).join(" "));`;

// One fixture per state of the rail. `railStates` is what `npm run snap:rail` renders.
export const railStates = {
  railRunning: () => build("running"),
  railYourTurn: () => ({ ...build("your_turn"), snapScript: barScript }),
  // The contrast pass (#455) runs here too.
  railNeeds: () => ({ ...build("needs_you", { ask: questionAsk }), snapScript: barScript, contrast: true }),
  railIdle: () => build("idle", { withBrief: true }),
};

// #444: the rail's header in two lines, at a narrow rail. Line 1 the logo, then the name on one line with an ellipsis, its full text
// in the page and on hover; line 2 the verdict, the mode and the phase, the needs-you verdict in its colour.
const longName = "Check that every snap fixture of the rail renders in…";
const headerScript = (phase: boolean) => `
  document.querySelector(".map-stage").style.setProperty("--panel-w", "320px");
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const name = document.getElementById("session-name");
  const full = "#56 · ${longName}";
  const box = (id) => document.getElementById(id).getBoundingClientRect();
  const nameBox = name.getBoundingClientRect();
  if (name.textContent !== full) log("snap:fail the name's text is " + JSON.stringify(name.textContent));
  if (name.title !== full) log("snap:fail the name's hover text is " + JSON.stringify(name.title));
  if (nameBox.height >= 2 * parseFloat(getComputedStyle(name).fontSize)) log("snap:fail the name's line is " + nameBox.height + "px high, more than one line");
  if (name.scrollWidth <= name.clientWidth) log("snap:fail the name does not overflow its line: " + name.scrollWidth + " <= " + name.clientWidth);
  const mark = box("mark");
  if (!(document.getElementById("mark").compareDocumentPosition(name) & Node.DOCUMENT_POSITION_FOLLOWING) || mark.right > nameBox.left + 0.5
      || mark.bottom <= nameBox.top || mark.top >= nameBox.bottom) log("snap:fail the logo does not precede the name on its line");
  for (const id of ["state", "mode"${phase ? ', "phase"' : ""}]) {
    const item = box(id);
    if (!document.getElementById(id).textContent) log("snap:fail line 2 has no " + id);
    if (item.top < nameBox.bottom - 0.5) log("snap:fail #" + id + " is not on the line under the name");
  }
  const [state, mode] = [box("state"), box("mode")];
  if (mode.top >= state.bottom || mode.bottom <= state.top) log("snap:fail the verdict and the mode are not on one line");
  ${phase ? 'const p = box("phase"); if (p.top >= state.bottom || p.bottom <= state.top) log("snap:fail the phase is not on the verdict\'s line");' : `
  const probe = document.createElement("span");
  probe.style.color = "var(--needs-you)";
  document.body.append(probe);
  const want = getComputedStyle(probe).color;
  probe.remove();
  const got = getComputedStyle(document.getElementById("state")).color;
  if (got !== want) log("snap:fail the needs-you verdict is " + got + ", not " + want);`}
  log("header: " + [...document.querySelectorAll("#headline p")].map((p) => p.textContent.trim()).join(" | "));`;
const withHeader = (fixture: ReturnType<typeof build>, phase: boolean) => {
  fixture.selected.session.prompt_name = longName;
  return { ...fixture, snapScript: headerScript(phase) };
};

// #447: the "⋯" menu opens at its button, under it and inside the window, and flips over it when the button is near the window's
// bottom edge; it lists only Rename, Mark unseen and Delete, as they apply. The rail's header does not scroll in the panel, so the
// flipped case moves the panel down until the button is near the bottom edge, as a rail scrolled there would put it.
// Near the bottom, the panel's slide-in is turned off too: a render that starts late under load would otherwise animate its transform
// up from 0 while the menu is measured.
const menuScript = (want: string[], nearBottom: boolean) => `
  const frame = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  ${nearBottom ? 'Object.assign(document.getElementById("map-panel").style, { top: "auto", bottom: "auto", height: "120px", animation: "none", transform: "translateY(" + (innerHeight - 90) + "px)" }); await frame();' : ""}
  const more = document.getElementById("rail-more");
  const menu = document.getElementById("rail-actions");
  more.click();
  await frame();
  if (!menu.matches(":popover-open")) return log("snap:fail the menu did not open");
  const b = more.getBoundingClientRect(), m = menu.getBoundingClientRect();
  const labels = [...menu.querySelectorAll("button")].filter((x) => x.checkVisibility()).map((x) => x.textContent);
  if (labels.join() !== ${JSON.stringify(want.join())}) log("snap:fail the menu lists " + labels.join(", "));
  if (m.left < 0 || m.top < 0 || m.right > innerWidth || m.bottom > innerHeight) log("snap:fail the menu leaves the window: " + JSON.stringify(m));
  if (m.right < b.left || m.left > b.right) log("snap:fail the menu is not in line with its button: menu " + m.left + "–" + m.right + ", button " + b.left + "–" + b.right);
  ${nearBottom
    ? 'if (m.bottom > b.top + 0.5 || b.top - m.bottom > 16) log("snap:fail the menu is not right over its button: menu bottom " + m.bottom + ", button top " + b.top);'
    : 'if (m.top < b.bottom - 0.5 || m.top - b.bottom > 16) log("snap:fail the menu is not right under its button: menu top " + m.top + ", button bottom " + b.bottom);'}
  log("menu " + labels.join(", ") + " at " + [m.left, m.top, m.right, m.bottom].map(Math.round) + "; button at " + [b.left, b.top, b.right, b.bottom].map(Math.round));`;

// A session at Daniel's turn that he has already seen, so Mark unseen applies.
const seen = (fixture: ReturnType<typeof build>) => {
  fixture.selected.session.unseen = false;
  return fixture;
};

// #124: the banner opens an idle session, right above the newest turn, with the Brief inside it and not below the feed.
const bannerScript = `
  const banner = document.getElementById("banner");
  if (!banner) log("snap:fail the banner is missing");
  else {
    const next = banner.parentElement.nextElementSibling;
    if (next?.id !== "feed") log("snap:fail the newest turn is not right under the banner");
    if (!banner.querySelector(".banner-brief .slot-title")) log("snap:fail the Brief is not inside the banner");
    if (!document.getElementById("brief-fold").hidden) log("snap:fail the Brief shows twice");
    log("banner: " + [...banner.querySelectorAll("p")].map((p) => p.textContent).join(" | "));
  }`;

// Sample scripts for a fixture's `snapScript` (see page.mjs): the first scrolls the rail of the five-turn fixture and logs
// where it stands, the second fails on purpose.
const scrollScript = `
  const story = document.getElementById("story");
  story.scrollTop = 120;
  log("rail scrollTop " + story.scrollTop + " of " + (story.scrollHeight - story.clientHeight));`;
const failScript = `log("snap:fail the sample script fails on purpose");`;

// #117: opens the second earlier turn, as a click does, and logs what stands: one turn open of four folded.
const openTurnScript = `
  document.querySelectorAll(".turn.past .turn-line")[1].click();
  const lines = [...document.querySelectorAll(".turn.past .turn-line")];
  log("folded turns " + lines.length + ", open " + lines.filter((l) => l.getAttribute("aria-expanded") === "true").length);
  if (lines.length !== 4 || lines[1].getAttribute("aria-expanded") !== "true") log("snap:fail one turn should be open of four folded");`;

// #456: a failed row's accessible text says "failed" exactly once, and no ✕ or ✗ mark is read out beside it.
const saysFailedOnce = `
  const failedOnce = (node, what) => {
    const name = __snapName(node);
    if ((name.match(/failed/gi) ?? []).length !== 1 || /[✕✗]/.test(name)) log("snap:fail " + what + " is read as " + JSON.stringify(name));
  };`;

// #345: the evidence line names the failed check by what broke, and keeps the command behind the drill-down.
const namedFailureScript = `
  const wait = () => new Promise((resolve) => setTimeout(resolve, 300));
  await wait();
  const toggle = document.querySelector(".evidence-toggle");
  if (!toggle) log("snap:fail there is no evidence line");
  if (!/✗ Window tests failed: delete keeps a live terminal \\(\\+1 more\\)/.test(toggle.textContent)) log("snap:fail the failed check is not named by its test: " + toggle.textContent);
  toggle.click();
  await wait();
  if (!document.querySelector(".evidence-check .evidence-fail code")) log("snap:fail the command is not behind the drill-down");
  ${saysFailedOnce}
  failedOnce(toggle, "the evidence line");
  failedOnce(document.querySelector(".evidence-check .evidence-fail"), "the failed check's row");
  const folded = document.querySelector(".turn-line .check-failed")?.closest(".turn-line");
  if (!folded) log("snap:fail no folded turn is marked failed");
  else failedOnce(folded, "the folded failed turn");`;

// #113: scrolls the rail, opens the evidence line, then a failed check's output, and fails when the rail's scrollTop moved.
const evidenceScript = `
  const story = document.getElementById("story");
  story.scrollTop = Math.min(80, story.scrollHeight - story.clientHeight);
  const at = story.scrollTop;
  const wait = () => new Promise((resolve) => setTimeout(resolve, 200));
  const toggle = document.querySelector(".evidence-toggle");
  if (!toggle) log("snap:fail there is no evidence line");
  toggle.click();
  await wait();
  // #207: every file counts, an agent is named only where a call or a worker names it, and nothing is "external".
  const shown = toggle.parentElement.querySelectorAll(".evidence-rows li");
  if ([...shown].some((li) => /external/.test(li.textContent))) log("snap:fail the opened line still has an external row");
  if (toggle.parentElement.querySelectorAll(".evidence-file .evidence-agent").length !== 2) log("snap:fail the files' agents are not named");
  // A check whose last run is a worker's names the worker, as a file does.
  if (toggle.parentElement.querySelectorAll(".evidence-check .evidence-agent").length !== 1) log("snap:fail the worker's check does not name its worker");
  const row = document.querySelector(".evidence-check .evidence-fail");
  if (!row) log("snap:fail the opened line has no failed check");
  ${saysFailedOnce}
  failedOnce(row, "the failed check's row");
  row.click();
  await wait();
  const output = document.querySelector(".evidence-output");
  if (!output || output.hidden || !/exit code 1/.test(output.textContent)) log("snap:fail the failed check's output did not open");
  // Only an error is live: the output itself is not read aloud as it loads (#452's review).
  if (output?.closest("[aria-live], [role=status]")) log("snap:fail the failed check's output is in a live region");
  log("rail scrollTop " + at + " -> " + story.scrollTop);
  if (story.scrollTop !== at) log("snap:fail the rail's scrollTop moved from " + at + " to " + story.scrollTop);`;

// #456: a failed check's output and a file's hunk that cannot be read say so in the row's status region, which the row held before the
// read failed; the output's box itself is not live (#452's review).
const unreadScript = `
  const wait = () => new Promise((resolve) => setTimeout(resolve, 200));
  const toggle = document.querySelector(".evidence-toggle");
  if (!toggle) return log("snap:fail there is no evidence line");
  toggle.click();
  await wait();
  const heard = (row, shown, why) => {
    row.click();
    return wait().then(() => {
      const pre = row.parentElement.querySelector(shown);
      if (!pre?.textContent.includes("could not be read")) return log("snap:fail " + why + " shows " + JSON.stringify(pre?.textContent));
      if (pre.closest("[aria-live], [role=status]")) log("snap:fail " + why + "'s box is in a live region");
      const live = row.parentElement.querySelectorAll("[role=status]");
      if (live.length !== 1 || !live[0].textContent.includes("could not be read")) log("snap:fail " + why + "'s error is not said in the row's status region: " + JSON.stringify([...live].map((node) => node.textContent)));
    });
  };
  await heard(document.querySelector(".evidence-check .evidence-fail"), ".evidence-output", "the check's output");
  await heard(document.querySelector(".evidence-file .evidence-fail"), ".evidence-hunk", "the file's hunk");`;

// #208: opens the evidence line of a turn that merged a branch, and fails unless the line counts 3 files and "+2 merged",
// and the opened rows end their files with a "+2 merged" row that is no file.
const mergedScript = `
  const wait = () => new Promise((resolve) => setTimeout(resolve, 200));
  const toggle = document.querySelector(".evidence-toggle");
  if (!toggle) log("snap:fail there is no evidence line");
  if (!/^3 files · \\+2 merged/.test(toggle.textContent)) log("snap:fail the line reads " + toggle.textContent);
  toggle.click();
  await wait();
  const merged = toggle.parentElement.querySelectorAll(".evidence-merged");
  if (merged.length !== 1 || merged[0].textContent !== "+2 merged") log("snap:fail the opened line has no '+2 merged' row");
  if (toggle.parentElement.querySelectorAll(".evidence-file").length !== 3) log("snap:fail the merged files are listed as files");`;

// #114: scrolls the rail, opens the evidence line, then a file's hunk, and fails when the rail's scrollTop moved.
const hunkScript = `
  const story = document.getElementById("story");
  story.scrollTop = Math.min(80, story.scrollHeight - story.clientHeight);
  const at = story.scrollTop;
  const wait = () => new Promise((resolve) => setTimeout(resolve, 200));
  const toggle = document.querySelector(".evidence-toggle");
  if (!toggle) log("snap:fail there is no evidence line");
  toggle.click();
  await wait();
  const row = document.querySelector(".evidence-file .evidence-fail");
  if (!row) log("snap:fail the opened line has no file row");
  row.click();
  await wait();
  const hunk = document.querySelector(".evidence-hunk");
  if (!hunk || hunk.hidden || !/gap: var/.test(hunk.textContent)) log("snap:fail the file's hunk did not open");
  // Only an error is live: the hunk itself is not read aloud as it loads (#452's review).
  if (hunk?.closest("[aria-live], [role=status]")) log("snap:fail the file's hunk is in a live region");
  log("rail scrollTop " + at + " -> " + story.scrollTop);
  if (story.scrollTop !== at) log("snap:fail the rail's scrollTop moved from " + at + " to " + story.scrollTop);`;

// #131: opens the evidence line, then "Open full diff", and fails when the diff pane does not list a file with its hunk.
const diffScript = `
  const wait = () => new Promise((resolve) => setTimeout(resolve, 300));
  document.querySelector(".evidence-toggle").click();
  await wait();
  const open = [...document.querySelectorAll(".evidence-edit")].find((b) => b.textContent === "Open full diff");
  if (!open) log("snap:fail the opened line has no Open full diff");
  open.click();
  await wait();
  const pane = document.querySelector(".turn-diff");
  if (!pane || pane.hidden) log("snap:fail the diff pane did not open");
  log("diff files " + pane.querySelectorAll(".diff-file").length + ", lines " + pane.querySelectorAll(".diff-line").length);
  if (!pane.querySelector(".diff-line.add") || !pane.querySelector(".diff-line.del")) log("snap:fail the diff has no added and removed lines");`;

// #125: opens the evidence line, a file's Ask, then a failed check's Ask, and fails when the rail's scrollTop moved or no
// facts show.
const askScript = (selectors: string[]) => `
  const story = document.getElementById("story");
  story.scrollTop = Math.min(80, story.scrollHeight - story.clientHeight);
  const at = story.scrollTop;
  const wait = () => new Promise((resolve) => setTimeout(resolve, 200));
  document.querySelector(".evidence-toggle").click();
  await wait();
  const open = async (selector) => {
    const box = document.querySelector(selector + " .ask-about");
    if (!box) return log("snap:fail there is no Ask on " + selector);
    box.open = true;
    await wait();
    if (!box.querySelector(".ask-facts li")) log("snap:fail Ask on " + selector + " shows no facts");
  };
  for (const selector of ${JSON.stringify(selectors)}) await open(selector);
  log("rail scrollTop " + at + " -> " + story.scrollTop);
  if (story.scrollTop !== at) log("snap:fail the rail's scrollTop moved from " + at + " to " + story.scrollTop);`;

// #461: in a file's Ask, Enter that confirms an input method's word in the why-not field asks nothing; a plain Enter then asks once.
const whyNotImeScript = `
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  document.querySelector(".evidence-toggle").click();
  await wait(200);
  const box = document.querySelector(".evidence-file .ask-about");
  if (!box) return log("snap:fail there is no Ask on a file");
  box.open = true;
  await wait(200);
  const field = box.querySelector(".ask-why input");
  if (!field) return log("snap:fail the file's Ask has no why-not field");
  const asked = [];
  const fetched = window.fetch;
  window.fetch = (url, init) => (String(url).endsWith("/api/ask_why_not") && asked.push(url), fetched(url, init));
  field.focus();
  field.value = "main.ts";
  await __snapKey("Composing Enter");
  await wait(200);
  if (asked.length) log("snap:fail Enter mid-composition asked " + asked.length + " time(s)");
  await __snapKey("Enter");
  await wait(200);
  window.fetch = fetched;
  if (asked.length !== 1) log("snap:fail a plain Enter after the composition asked " + asked.length + " time(s), not once");
  log("why-not ime ok");`;

// `npm run snap:readme` (scripts/snap/readme.mjs): the README's rail crops. Each script scrolls the rail to its top, finds
// its target (a missing one is a `snap:fail`) and logs `snap:crop x y width height`, the box in page pixels that the crop keeps.
const cropScript = (setup: string, from: string, to: string) => `
  const story = document.getElementById("story");
  const wait = () => new Promise((resolve) => setTimeout(resolve, 200));
  ${setup}
  story.scrollTop = 0;
  await wait();
  const top = document.querySelector(${JSON.stringify(from)});
  const bottom = document.querySelector(${JSON.stringify(to)});
  if (!top || !bottom) return log("snap:fail the crop's target is missing: " + (top ? ${JSON.stringify(to)} : ${JSON.stringify(from)}));
  const rail = story.getBoundingClientRect();
  const start = top.getBoundingClientRect().top;
  const end = bottom.getBoundingClientRect().bottom;
  if (end - start < 40) return log("snap:fail the crop's target is empty");
  log("snap:crop " + [rail.left, start - 8, story.clientWidth, end - start + 16].map(Math.round).join(" "));`;

const readmeTurn = () => ({ ...build("your_turn"), snapScript: cropScript("", ".now-turn", ".now-turn .evidence-toggle") });
const readmeWhere = () => ({ ...build("idle", { withBrief: true }), snapScript: cropScript("", "#banner", "#banner") });

// #297: a blocked command's card sits in the dock only while it has a button; every other card is neutral in its turn.
const blockedCommand = (status: BlockedCommand["status"], at: number, extra: Partial<BlockedCommand> = {}): BlockedCommand => ({
  call: "toolu_b1", command: "git push origin issue-56-row-controls", cwd, reason: "[Unauthorized Access]", agent: null, at, status, ...extra,
});
const withBlocked = (state: SessionState, card: (turnStart: number) => BlockedCommand, snapScript: string) => {
  const fixture = build(state);
  const turn = fixture.selected.session.turns.at(-1);
  if (!turn) throw new Error("the rail fixture has a turn");
  fixture.selected.session.blocked_commands = [card(turn.started_at + 30e3)];
  return { ...fixture, snapScript };
};
// The card has Allow and Decline: it is in the dock, once its request stands, and not in the feed.
const dockScript = `
  window.__snapEmit("blocked", [{ nonce: "n1", tool_use_id: "toolu_b1", waiting: true }]);
  await new Promise((resolve) => setTimeout(resolve, 200));
  const dock = document.getElementById("dock");
  const labels = [...dock.querySelectorAll(".blocked-command .answer-buttons button")].map((b) => b.textContent);
  if (dock.hidden || labels.join() !== "Allow,Decline") log("snap:fail the dock lacks the card's buttons: " + labels);
  if (document.querySelector("#feed .blocked-command")) log("snap:fail the card is in the feed too");
  log("dock buttons: " + labels.join(" "));`;
// #298: a docked card's risk note, on its way, arrived and failed. The place keeps two lines, so Allow and Decline sit at the same place in
// the three: each run logs where the buttons are in the dock, and the screenshots are compared.
const riskNote = (note: RiskView | null) => {
  const fixture = withBlocked("your_turn", (at) => blockedCommand("blocked", at), riskScript);
  return { ...fixture, cards: { ...fixture.cards, risk_notes: note ? [note] : [], decision_notes: [] } };
};
const riskScript = `
  window.__snapEmit("blocked", [{ nonce: "n1", tool_use_id: "toolu_b1", waiting: true }]);
  await new Promise((resolve) => setTimeout(resolve, 200));
  const dock = document.getElementById("dock");
  const place = dock.querySelector(".blocked-command .risk-note");
  if (!place || place.getAttribute("aria-live") !== "polite") return log("snap:fail the docked card has no note place");
  const buttons = [...dock.querySelectorAll(".blocked-command .answer-buttons button")];
  if (buttons.map((b) => b.textContent).join() !== "Allow,Decline") return log("snap:fail the buttons are " + buttons.map((b) => b.textContent));
  const at = (e) => { const r = e.getBoundingClientRect(), d = dock.getBoundingClientRect(); return Math.round(r.left - d.left) + "," + Math.round(r.top - d.top); };
  const tall = Math.round(place.getBoundingClientRect().height);
  log("note: " + place.textContent);
  log("note height " + tall + "; Allow at " + at(buttons[0]) + "; Decline at " + at(buttons[1]));`;
// A note that arrives while the card is docked fills its place where it stands: the place and the buttons stay the same nodes, so a screen
// reader hears the live region fill and keyboard focus stays on Allow. The script holds the nodes, sends the cards with a note, and compares.
const riskFilled = () => {
  const fixture = riskNote({ call: "toolu_b1", state: "reading" });
  const note = { call: "toolu_b1", state: "arrived", level: "high", line: "Pushes the branch issue-56-row-controls to origin", at: Date.now() };
  const next = { ...fixture.cards, risk_notes: [note] };
  const snapScript = `
    window.__snapEmit("blocked", [{ nonce: "n1", tool_use_id: "toolu_b1", waiting: true }]);
    await new Promise((resolve) => setTimeout(resolve, 200));
    const dock = document.getElementById("dock");
    const card = dock.querySelector(".blocked-command");
    const place = card.querySelector(".risk-note");
    const buttons = [...card.querySelectorAll(".answer-buttons button")];
    const before = place.textContent;
    if (before !== "Reading the command…") return log("snap:fail the place reads '" + before + "' before the note");
    buttons[0].focus();
    window.__snapEmit("cards", ${JSON.stringify(next)});
    await new Promise((resolve) => setTimeout(resolve, 200));
    const same = dock.querySelector(".blocked-command") === card && card.querySelector(".risk-note") === place;
    const kept = [...card.querySelectorAll(".answer-buttons button")].every((b, i) => b === buttons[i]);
    if (!same || !kept) return log("snap:fail the dock's card, its place or a button is a new node");
    if (place.textContent === before || !place.textContent.startsWith("Risk: High")) return log("snap:fail the place did not fill: " + place.textContent);
    if (document.activeElement !== buttons[0]) return log("snap:fail focus left Allow");
    log("filled in place: " + place.textContent);`;
  return { ...fixture, snapScript };
};
// An allowed card has no button: the dock is empty and the card is neutral, with its state line, in the newest turn.
const fedScript = `
  const card = document.querySelector("#feed .now-turn .blocked-command");
  if (!card) return log("snap:fail the allowed card is not in the newest turn");
  if (!document.getElementById("dock").hidden) log("snap:fail the dock still shows");
  if (card.querySelector("button")) log("snap:fail the allowed card has a button");
  const state = card.querySelector(".state");
  if (state.textContent !== "Allowed" || state.dataset.state) log("snap:fail the state line reads '" + state.textContent + "' or is coloured");
  card.scrollIntoView({ block: "center" });
  log("fed card: " + state.textContent);`;

// #523: a Wait tip while running, the Intention box's last line under the step, set apart by a hair line, muted, one line ending in
// "…" when cut, its full text the hover. The first tip of a fresh history teaches the palette, with its key from the keymap.
const tipScript = `
  const tip = document.getElementById("tip");
  if (tip.hidden || !tip.checkVisibility()) return log("snap:fail no tip shows while running");
  if (tip.parentElement.id !== "intention" || tip.previousElementSibling?.id !== "step") log("snap:fail the tip is not under the step in the Intention box");
  if (tip.textContent !== "Tip: Every action is in the palette (⇧⌘P).") log("snap:fail the tip reads " + JSON.stringify(tip.textContent));
  if (tip.title !== tip.textContent) log("snap:fail the tip's hover text is " + JSON.stringify(tip.title));
  const style = getComputedStyle(tip);
  if (style.borderTopStyle !== "solid" || style.whiteSpace !== "nowrap" || style.textOverflow !== "ellipsis") log("snap:fail the tip is not one cut line under a hair line");
  if (tip.getBoundingClientRect().height >= 2 * parseFloat(style.fontSize) + parseFloat(style.paddingTop)) log("snap:fail the tip takes more than one line");
  log("tip: " + tip.textContent);`;

export const railFixtures = {
  railBlockedDock: () => withBlocked("your_turn", (at) => blockedCommand("blocked", at), dockScript),
  railRiskReading: () => riskNote({ call: "toolu_b1", state: "reading" }),
  railRiskArrived: () => riskNote({ call: "toolu_b1", state: "arrived", level: "high", line: "Pushes the branch issue-56-row-controls to origin, so everyone who fetches sees it", at: Date.now() }),
  railRiskFilled: riskFilled,
  railRiskFailed: () => riskNote({ call: "toolu_b1", state: "failed" }),
  railBlockedAllowed: () => withBlocked("your_turn", (at) => blockedCommand("allowed", at), fedScript),
  readmeTurn,
  readmeWhere,
  railPlain: () => build("your_turn"),
  railDiff: () => ({ ...build("your_turn"), snapScript: diffScript }),
  railAsk: () => ({ ...build("your_turn"), snapScript: askScript([".evidence-file", ".evidence-check"]) }),
  railAskCheck: () => ({ ...build("your_turn"), snapScript: askScript([".evidence-check"]) }),
  railAskWhyNotIme: () => ({ ...build("your_turn"), snapScript: whyNotImeScript, realKeys: true, snapDone: "why-not ime ok" }),
  railHunk: () => ({ ...build("your_turn"), snapScript: hunkScript }),
  railMerged: () => {
    const fixture = build("your_turn");
    const files = fixture.selected.session.turns[4]?.files;
    if (!files) throw new Error("the rail fixture's fifth turn has files");
    files.merged = 2;
    return { ...fixture, snapScript: mergedScript };
  },
  railNamedFailure: () => ({ ...build("your_turn"), failed_check_name: "Window tests failed: delete keeps a live terminal (+1 more)", snapScript: namedFailureScript }),
  railEvidence: () => ({ ...build("your_turn"), snapScript: evidenceScript }),
  railEvidenceUnread: () => ({ ...build("your_turn"), check_output: { __throw: "the transcript is gone" } as never, turn_file_hunk: { __throw: "the tree is gone" } as never, snapScript: unreadScript }),
  ...railStates,
  railHeaderNeeds: () => withHeader(build("needs_you", { ask: questionAsk }), false),
  railHeaderRunning: () => withHeader(build("running"), true),
  railTip: () => ({ ...build("running"), snapScript: tipScript }),
  railMenu: () => ({ ...seen(build("your_turn")), snapScript: menuScript(["Rename", "Mark unseen", "Delete"], false) }),
  railMenuRunning: () => ({ ...build("running"), snapScript: menuScript(["Rename", "Delete"], false) }),
  railMenuFlip: () => ({ ...seen(build("your_turn")), snapScript: menuScript(["Rename", "Mark unseen", "Delete"], true) }),
  railBanner: () => ({ ...build("idle", { withBrief: true }), snapScript: bannerScript }),
  railIdleScroll: () => ({ ...build("idle", { withBrief: true }), snapScript: scrollScript }),
  railIdleOpened: () => ({ ...build("idle", { withBrief: true }), snapScript: openTurnScript }),
  railIdleFail: () => ({ ...build("idle", { withBrief: true }), snapScript: failScript }),
} satisfies Record<string, () => Fixture>;
