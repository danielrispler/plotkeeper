// Fixtures for the story rail (from the design session for #8, danielrispler/plotkeeper#102): one selected session,
// "#56 · Row controls", in four states (running, at your turn, needing you, idle), with realistic content: 5 turns, summaries as the writer's prompt shapes them (at most 4
// short lines, the first says what the turn did), a card with its quiz, turn Quizzes,
// subagents, checks, a permission mode, a model, an estimate, a brief and decision history, and a
// Git line with several tokens. Times are relative to the moment a fixture renders.

const MIN = 60e3;

const capabilities = { agent: "claude", asks: true, background_tasks: true, replies: true, permission_mode: true, durations: true, transcript_version: true, turn_trees: true };
const cwd = "/Users/danielrispler/work/plotkeeper/.claude/worktrees/issue-56-row-controls";

const quiz = (text, options, key) => ({ text, options, key });

// What each turn's summary says; its first line is the turn's Headline (the turn-end intention is the final response's first line).
const summaries = {
  t4: {
    summary:
      "Fixed the two failures `npm run check` found: a clippy lint in a Rust test helper and a UI test that still expected the old row padding.\n" +
      "The row's padding now comes from the fold and delete columns (`--control`), so the test reads the token instead of a hard-coded 12px.\n" +
      "Clippy's `needless_borrow` was unrelated to the UI and is fixed with a one-line change.\n" +
      "`npm run check` passes; nobody looked at the rows in a real window.",
    quiz: quiz(
      "Why does the delete button stay in the row's layout while it is hidden?",
      [
        { text: "Its box is kept, so no column shifts", why: "Right: `visibility: hidden` hides the paint but keeps the space." },
        { text: "The browser reserves space for any hover rule", why: "No rule makes a browser reserve space for a hover rule." },
        { text: "The row's padding is rounded up to the control size", why: "Padding is a fixed token here, not rounded." },
      ],
      0,
    ),
  },
  t3: {
    summary:
      "Made the delete button show on hover and on the selected row, and kept its column empty otherwise.\n" +
      "Keyboard users get it through `:focus-within`, so Tab reaches it.\n" +
      "Not checked: the strip, where rows have no delete.",
    quiz: null,
  },
  t1: {
    summary:
      "Added a fold toggle to each session row and moved the delete button into its own column.\n" +
      "The toggle sits at the row's start and is drawn only when subagents show under the row.\n" +
      "Both are siblings of the row button, since a button cannot hold a button.",
    quiz: quiz(
      "Why are the toggle and the delete siblings of the row button, not its children?",
      [
        { text: "A button may not hold interactive content", why: "Right: the HTML content model forbids it, and the press would bubble." },
        { text: "Children of a button cannot take a hover state", why: "They can; hover does not depend on the parent being a button." },
        { text: "Siblings let the grid share the glyph's column", why: "The columns come from padding, not the grid." },
      ],
      0,
    ),
  },
};

const prompts = [
  "Read issue #56 and plan the row controls",
  "Implement the plan, starting with the fold toggle",
  "worker w1 returned",
  "The delete should show only on hover, and stay on the selected row",
  "Run npm run check and fix what fails",
];

const card = (id, answered) => ({
  request: { session_id: "s1", id, agent: null, changed_by: {}, check: null },
  card: {
    summary: "Delete shows on hover, focus and the selected row.",
    explanation:
      "The delete button of a session row is now hidden with `visibility`, so its column stays in the layout. It shows when the item is hovered, when focus is inside it, and while its row is the selected one. Rows that cannot be deleted draw no button, but keep the column, so every name starts at the same x.",
    paths: ["ui/style.css", "ui/list.ts", "ui/tests/rows.test.mjs"],
    code: "-.item .delete { display: none; }\n+.item .delete { visibility: hidden; }\n+.item:hover .delete,\n+.item:focus-within .delete,\n+.item:has(> .row[aria-pressed=\"true\"]) .delete { visibility: visible; }",
    why_it_matters: "A row that changes width on hover makes the list jitter under the pointer. Keeping the column makes the list steady.",
    not_checked: "The strip, which draws no delete, and a screen reader's reading order of the sibling controls.",
    architecture: null,
    quiz: {
      text: "What does a keyboard user get when they Tab onto a session row?",
      options: [
        { text: "The delete button shows, because `:focus-within` matches the row's item", why: "Right: focus inside the item matches the rule." },
        { text: "Nothing changes until the pointer hovers the row", why: "Hover is not needed once focus is inside." },
        { text: "The row is selected, which shows the delete", why: "Tab focuses; it does not select." },
      ],
      key: 0,
    },
  },
  guess: answered ? { option: 0, right: true, at: Date.now() - 20 * MIN } : null,
  key_wrong: false,
  more: null,
  rating: null,
  check_result: null,
});

const check = (command, passed, startedAgo, tookMs, turn) => ({
  command, passed, started_at: Date.now() - startedAgo, ended_at: Date.now() - startedAgo + tookMs, typical_ms: tookMs, turn,
});

const file = (path, added, removed) => ({ root: cwd, path, from: null, added, removed });
const run = (status, call, exit_code) => ({ command: "cargo", status, exit_code, call, agent: null });

const helper = (id, name, asked, type, result, turn, ago, running = false) => ({
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

const session = (state, { running = false, question = false } = {}) => {
  const now = Date.now();
  const ended = !running && state !== "needs_you";
  const intentions = {
    running: "Reading the delete handler to see why the fold toggle shares its column with the glyph.",
    needs_you: "I need one decision before I change the row layout.",
  };
  const stepTools = { tool: "Edit", detail: `${cwd}/ui/list.ts` };
  const turnStart = [now - 5 * 60 * MIN, now - 4 * 60 * MIN, now - 3 * 60 * MIN, now - 2 * 60 * MIN, now - 40 * MIN];
  const turns = prompts.map((prompt, i) => ({
    started_at: turnStart[i],
    ended_at: i < 4 ? turnStart[i] + 20 * MIN : ended ? now - 6 * MIN : null,
    prompt,
    id: i < 4 || ended ? `t${i}` : null,
    message: "Done.",
    // The newest turns changed files (#113); the second turn ends with a failed check, which its folded line marks (#117).
    files: i >= 3 ? { files: [file("ui/style.css", 14, 3), file("ui/list.ts", 22, 9), file("ui/tests/rows.test.mjs", 6, 6)], external: i === 4 ? 2 : 0 } : null,
    checks: i === 4
      ? [
          { suite: "cargo clippy", status: "passed", retries: 1, runs: [run("failed", "k1", 101), run("passed", "k2", 0)] },
          { suite: "npm run check", status: "failed", retries: 0, runs: [run("failed", "k3", 1)] },
        ]
      : [{ suite: "check", status: i === 1 ? "failed" : "passed", retries: 0, runs: [] }],
  }));
  const transcript = {
    intention: ended ? final.split("\n")[0] : intentions[state === "needs_you" ? "needs_you" : "running"],
    steps_ago: ended || state === "needs_you" ? 0 : 7,
    responses: ended ? ["I'll run the full check first.", final] : ["I'll look at how the row lays out its controls."],
    unavailable: null,
    context: { tokens: 85000, window: 1000000, share: 9, left: 91 },
    branch: "issue-56-row-controls",
    model: "claude-opus-5-5",
    effort: "high",
  };
  return {
    id: "s1", capabilities, model: null, cwd, folder_name: "issue-56-row-controls", terminal: null, status_only: false, name: null,
    prompt_name: "Row controls", resume_id: "s1", permission_mode: "auto", started_mode: "auto", cleared: [], asks_before_closing: state !== "idle",
    landed: false, last_step: ended ? { tool: "Bash", detail: "Run the full check", command: "npm run check" } : stepTools,
    started_at: now - 5 * 60 * MIN, last_event_at: now - 30e3, turn_span: running ? { start: now - 14 * MIN, end: null } : null,
    step_span: running ? { start: now - 90e3, end: null } : null, state, checks: [
      check("cargo clippy --locked --workspace --all-targets -- -D warnings", false, 38 * MIN, 41e3, 4),
      check("cargo clippy --locked --workspace --all-targets -- -D warnings", true, 22 * MIN, 38e3, 4),
      check("npm run check", true, 20 * MIN, 5 * MIN + 40e3, 4),
    ].slice(0, running ? 1 : 3), agents: (running ? (all) => all.filter((a) => a.id !== "a2") : (all) => all)([
      helper("a2", "Write tests for the fold toggle", "Write node tests for the fold toggle in ui/list.ts", "general-purpose", "Added 4 tests; all pass.", 1, 4 * 60 * MIN - 5 * MIN),
      helper("a3", "Find where the delete is styled", "Find every rule that styles `.delete`.", "Explore", "Two rules in `ui/style.css`, none in `ui/list.ts`.", 3, 2 * 60 * MIN),
      helper("a1", "Review the diff", "Review the diff on this branch and list anything that breaks the token rules.", "Explore", "No raw colours; two spacing values bypass the tokens.", 4, 30 * MIN, running),
    ]), asker: state === "needs_you" ? { agent: null, since: now - 3 * MIN } : null,
    prompts: [], turns, returned: null,
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
  const base = (id, name, state, extra) => ({
    ...session("your_turn"), id, resume_id: id, prompt_name: name.slice(name.indexOf("·") + 2), identity: { issue: Number(name.slice(1, 3)), branch: null, pr: null, worktree: "", name },
    cwd: `/Users/danielrispler/work/plotkeeper/.claude/worktrees/issue-${name.slice(1, 3)}`, folder_name: `issue-${name.slice(1, 3)}`, state, checks: [], agents: [], questions: [], turns: [],
    asker: null, unseen: false, line: undefined, landed: false, started_at: now - 3600e3, ...extra,
    transcript: { ...session("your_turn").transcript, context: { tokens: 120000, window: 1000000, share: 12, left: 88 } },
  });
  return [
    base("s2", "#57 · Name from the prompt", "needs_you", { line: "Which name should a session take from its prompt? · 2m", asker: { agent: null, since: now - 2 * MIN }, questions: [{ tool_use_id: "q", agent: null, turn: 0, answer: "open", questions: [{ question: "Which name should a session take from its prompt?", options: [] }] }] }),
    base("s4", "#55 · Delete default", "running", { line: "Bash cargo test --locked --workspace" }),
    base("s5", "#54 · Subagent terminal", "your_turn", { unseen: true }),
    base("s6", "#48 · Agent deadline", "idle", { landed: true }),
    base("s7", "#41 · Git line in the top bar", "ended"),
  ];
};

const common = {
  projects: ["/Users/danielrispler/work/plotkeeper"],
  settings_files: { settings: null, keymap: null },
  workspaces: {
    main: { path: "/Users/danielrispler/work/plotkeeper", branch: "main" },
    tasks: [{ path: cwd, branch: "issue-56-row-controls" }],
    hidden: 0,
  },
  workspace_git_state: { branch: "issue-56-row-controls", upstream: [2, 1], changed: 5, main_behind: 7, on_default: false, would_conflict: true, operation: null },
  terminal_connect: { repo: "/Users/danielrispler/work/plotkeeper", outdated: false },
  terminal_lost: [],
  revival: [],
  usage: { five_hour: { used_percentage: 42, resets_at: Date.now() / 1000 + 3600 }, seven_day: { used_percentage: 18, resets_at: Date.now() / 1000 + 86400 } },
};

const brief =
  "Row controls (#56): each session row gets a fold toggle at its start and a delete at its end.\n" +
  "Done: the toggle, the delete's hover and focus rule, tests for both.\n" +
  "Open: the strip draws no delete; the Home rows show it always (their own rule).\n" +
  "Next: look at the rows in a real window, then open the PR.";

const decisions = [
  { call: "c1", agent: null, tool: "AskUserQuestion", input: { questions: [{ question: "Hide the delete with display or visibility?", options: [] }] }, answer: { "Hide the delete with display or visibility?": "visibility" }, at: Date.now() - 3 * 60 * MIN, source: "card" },
  { call: "c2", agent: null, tool: "ExitPlanMode", input: { plan: "Add a fold toggle and a delete column to each row." }, answer: "allow", at: Date.now() - 4 * 60 * MIN, source: "card" },
  { call: "c3", agent: null, tool: "Bash", input: { command: "git push -u origin issue-56-row-controls" }, answer: "allow", at: Date.now() - 90 * MIN, source: "terminal" },
];

const questionAsk = { nonce: "n1", hook_event_name: "PreToolUse", tool_name: "AskUserQuestion", tool_use_id: "toolu_q1", agent_id: null, tool_input: {} };

// The fixture for a state: its session, the list around it and the cards it has.
const build = (state, { ask = null, answered = false, withBrief = false } = {}) => {
  const running = state === "running";
  const s = session(state, { running, question: ask === questionAsk });
  const sessions = [s, ...others()].sort((a, b) => ["needs_you", "running", "your_turn", "idle", "ended"].indexOf(a.state) - ["needs_you", "running", "your_turn", "idle", "ended"].indexOf(b.state));
  const stored = card(`s1@${Date.now() - 110 * MIN}`, answered);
  const turnSummaries = { ...summaries };
  if (running) delete turnSummaries.t4;
  return {
    ...common,
    sessions,
    selected: { session: s, agent: null },
    cards: { brief: withBrief ? brief : null, recent: [], older: [stored], decisions: withBrief ? decisions : [] },
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
  };
};

// The top bar (#127): no item is cut. Every item in the bar sits whole inside it; the session's header is the one that may
// ellipsize its name, and only past its own cap.
const barScript = `
  const top = document.querySelector(".top");
  const room = top.getBoundingClientRect().right;
  for (const item of top.children) {
    if (item.hidden || item.classList.contains("sp") || item.classList.contains("menu")) continue;
    const box = item.getBoundingClientRect();
    if (box.right > room + 0.5) log("snap:fail the bar cuts #" + item.id + " at " + innerWidth);
    if (item.scrollWidth > item.clientWidth + 1 && item.id !== "session-header") log("snap:fail #" + item.id + " is clipped at " + innerWidth);
  }
  if (document.getElementById("session-header").hidden) log("snap:fail the session header is missing");
  log("bar at " + innerWidth + ": " + [...top.children].filter((i) => !i.hidden && i.id).map((i) => i.id).join(" "));`;

// One fixture per state of the rail. `railStates` is what `npm run snap:rail` renders.
export const railStates = {
  railRunning: () => build("running"),
  railYourTurn: () => ({ ...build("your_turn"), snapScript: barScript }),
  railNeeds: () => ({ ...build("needs_you", { ask: questionAsk }), snapScript: barScript }),
  railIdle: () => build("idle", { answered: true, withBrief: true }),
};

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
  const row = document.querySelector(".evidence-fail");
  if (!row) log("snap:fail the opened line has no failed check");
  row.click();
  await wait();
  const output = document.querySelector(".evidence-output");
  if (!output || output.hidden || !/exit code 1/.test(output.textContent)) log("snap:fail the failed check's output did not open");
  log("rail scrollTop " + at + " -> " + story.scrollTop);
  if (story.scrollTop !== at) log("snap:fail the rail's scrollTop moved from " + at + " to " + story.scrollTop);`;

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
const askScript = (selectors) => `
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

// #120: scrolls the rail, answers the turn's Quiz (a pick, or "Show the rest anyway"), and fails when the rail's scrollTop moved
// or the reasons and the held Body lines did not show.
const quizScript = (answer) => `
  const story = document.getElementById("story");
  story.scrollTop = Math.min(80, story.scrollHeight - story.clientHeight);
  const at = story.scrollTop;
  const wait = () => new Promise((resolve) => setTimeout(resolve, 200));
  const region = document.querySelector(".now-turn .quiz-region");
  if (!region) return log("snap:fail the newest turn has no open Quiz");
  if (region.querySelector(".options li p")) log("snap:fail the reasons show before an answer");
  ${answer === "pick" ? `region.querySelector(".options .btn").click();` : `region.querySelector(".quiz-skip").click();`}
  await wait();
  if (!region.querySelector(".options li p")) log("snap:fail the reasons did not show");
  if (!region.querySelector(".quiz-chips")) log("snap:fail the chips did not show");
  log("rail scrollTop " + at + " -> " + story.scrollTop);
  if (story.scrollTop !== at) log("snap:fail the rail's scrollTop moved from " + at + " to " + story.scrollTop);`;

export const railFixtures = {
  railQuiz: () => build("your_turn"),
  railQuizPick: () => ({ ...build("your_turn"), snapScript: quizScript("pick") }),
  railQuizSkip: () => ({ ...build("your_turn"), snapScript: quizScript("skip") }),
  railDiff: () => ({ ...build("your_turn"), snapScript: diffScript }),
  railAsk: () => ({ ...build("your_turn"), snapScript: askScript([".evidence-file", ".evidence-check"]) }),
  railAskCheck: () => ({ ...build("your_turn"), snapScript: askScript([".evidence-check"]) }),
  railHunk: () => ({ ...build("your_turn"), snapScript: hunkScript }),
  railEvidence: () => ({ ...build("your_turn"), snapScript: evidenceScript }),
  ...railStates,
  railIdleScroll: () => ({ ...build("idle", { answered: true, withBrief: true }), snapScript: scrollScript }),
  railIdleOpened: () => ({ ...build("idle", { answered: true, withBrief: true }), snapScript: openTurnScript }),
  railIdleFail: () => ({ ...build("idle", { answered: true, withBrief: true }), snapScript: failScript }),
};
