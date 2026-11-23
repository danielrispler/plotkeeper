// Studio state for `npm run snap`: each fixture maps a Tauri command to what it returns.
// A fixture is a function so that its times are relative to the moment it renders.

import type { BriefChapter, BriefFigure, BriefStep, BriefView, Ci, LogEntry, MapState, PrBrief, RatifyRow, Session, SessionState, StoredCatchUp, Subagent } from "../../ui/types.ts";
import { base, type ByArg, type Fixture } from "./base.ts";
import { railFixtures } from "./rail-fixtures.ts";

const session = (id: string, name: string, state: SessionState, extra: Partial<Session> = {}): Session => {
  const now = Date.now();
  return {
    id, capabilities: { agent: "claude", asks: true, background_tasks: true, replies: true, permission_mode: true, durations: true, transcript_version: true, transcript_model: true, context: true, turn_trees: true },
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

// The saved project list (#381): plotkeeper with its sessions, falinks with none (its header only, after it) and pokedex, whose
// folder is gone.
const savedProjects = [{ path: "/w/falinks", gone: false }, { path: "/w/plotkeeper", gone: false }, { path: "/w/pokedex", gone: true }];
const withSaved = (snapScript: string, extra: Fixture = {}): Fixture => ({
  ...list(),
  projects: savedProjects,
  remove_project: savedProjects.filter((project) => !project.gone),
  ...extra,
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    await wait(200);
    const heads = () => [...document.querySelectorAll("#groups .project-head")];
    const named = (name) => heads().find((head) => head.querySelector(".name").textContent === name);
    ${snapScript}`,
});
const openGone = `
    named("pokedex").querySelector(".project-toggle").click();
    await wait(300);
    const gone = document.querySelector("#map .map-empty.map-gone");
    if (!gone) return log("snap:fail the gone project shows no gone state: " + document.getElementById("map-body").textContent);
    if (!gone.textContent.includes("This folder is gone") || !gone.textContent.includes("/w/pokedex")) log("snap:fail the gone state lacks its words or its path: " + gone.textContent);
    const remove = [...gone.querySelectorAll("button")].find((b) => b.textContent === "Remove project");
    if (!remove) return log("snap:fail the gone state has no Remove project");`;

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
  fog: {}, ci: null, ready: [], ratify: [], runs: { paused: null, runs: [] }, stale: {}, catch_ups: {},
  ...extra,
});
// The Ready for review tray (#336) with two ready PRs, at the Map view's foot.
const mapTray = (): Fixture =>
  mapOpened(`
      const rows = [...document.querySelectorAll("#map-tray .tray-row")];
      if (rows.length !== 2) return log("snap:fail expected 2 tray rows, saw " + rows.length);
      if (rows[0].tagName !== "BUTTON" || rows[1].tagName !== "BUTTON") log("snap:fail a ready PR opens, written or not");
      if (!document.querySelector("#map-tray .side-title")?.textContent.includes("Ready for review")) log("snap:fail the tray has no title");
      if (!rows[0].textContent.includes("#334") || !rows[0].textContent.includes("brief ready")) log("snap:fail row text: " + rows[0].textContent);
      if (!rows[1].textContent.includes("written when you open it")) log("snap:fail row text: " + rows[1].textContent);
      log("tray ok");`, {
    map_state: mapState({ fog: { shell: "thick" }, ready: [{ number: 334, title: "PR briefs: the brief written for each ready PR", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false } }, { number: 336, title: "Ready for review tray", head: "a".repeat(40), brief: null }] }),
  });

// Background runs (#390) beside the map: a running run, the queue behind it, and an ended run that was denied a tool call. Cancel asks the
// server for that run.
const runAt = Date.UTC(2026, 10, 23, 12, 4);
const mapRuns = (): Fixture =>
  mapOpened(`
      const sent = [];
      const real = window.fetch;
      window.fetch = (url, init) => (sent.push(String(url) + " " + (init?.body ?? "")), real(url, init));
      const rows = [...document.querySelectorAll("#map-runs .run-row")];
      if (rows.length !== 4) return log("snap:fail expected 4 run rows, saw " + rows.length);
      if (!document.querySelector("#map-runs .side-title")?.textContent.includes("Background runs")) log("snap:fail the runs have no title");
      if (rows[0].dataset.state !== "running" || !rows[0].textContent.includes("Running since")) log("snap:fail the running row: " + rows[0].textContent);
      if (!rows[1].textContent.includes("Queued, next") || !rows[2].textContent.includes("Micro-world")) log("snap:fail the queue: " + rows[1].textContent + " / " + rows[2].textContent);
      if (!rows[3].textContent.includes("1 tool call was denied") || !rows[3].textContent.includes("git push")) log("snap:fail the denied run: " + rows[3].textContent);
      const words = (row) => [...row.querySelectorAll("button")].map((button) => button.textContent).join(",");
      if (words(rows[0]) !== "Cancel" || words(rows[1]) !== "Cancel" || words(rows[3]) !== "Copy prompt,Dismiss") log("snap:fail the buttons: " + rows.map(words).join(" / "));
      if (document.querySelector("#map-runs .runs-paused")) log("snap:fail a pause shows with no pause");
      rows[0].querySelector("button").click();
      await new Promise((done) => setTimeout(done, 50));
      if (!sent.some((call) => call.includes("/api/run_cancel") && call.includes("19a2f-1"))) log("snap:fail Cancel did not ask the server: " + sent.join(" | "));
      log("runs ok");`, {
    map_state: mapState({
      runs: {
        paused: null,
        runs: [
          { id: "19a2f-1", kind: "back-law", state: "running", at: runAt, outcome: null, line: null, denials: [], prompt: null },
          { id: "19a2f-2", kind: "back-law", state: "queued", at: runAt, outcome: null, line: null, denials: [], prompt: null },
          { id: "19a2f-3", kind: "micro-world", state: "queued", at: runAt, outcome: null, line: null, denials: [], prompt: null },
          { id: "19a2f-0", kind: "back-law", state: "ended", at: runAt, outcome: "denied", line: "Finished, but 1 tool call was denied", denials: ["Bash: git push origin HEAD"], prompt: "Write a test tagged @law: core-order." },
        ],
      },
    }),
    snapDone: "runs ok",
  });
// The queues paused by a login failure: the one indicator with Resume, over the run that failed and the one that waits.
const mapRunsPaused = (): Fixture =>
  mapOpened(`
      const paused = document.querySelectorAll("#map-runs .runs-paused");
      if (paused.length !== 1 || !paused[0].textContent.includes("Background runs are paused")) return log("snap:fail the pause's indicator: " + paused.length);
      if (paused[0].querySelector("button")?.textContent !== "Resume") log("snap:fail the indicator has no Resume");
      const rows = [...document.querySelectorAll("#map-runs .run-row")];
      if (!rows[1]?.textContent.includes("Queued, paused")) log("snap:fail the waiting run: " + rows[1]?.textContent);
      log("paused ok");`, {
    map_state: mapState({
      runs: {
        paused: "Claude is not logged in: run `claude` in a terminal and log in with /login",
        runs: [
          { id: "19a2f-1", kind: "back-law", state: "ended", at: runAt, outcome: "not-logged-in", line: "Claude is not logged in: run `claude` in a terminal and log in with /login", denials: [], prompt: "Write a test tagged @law: core-order." },
          { id: "19a2f-2", kind: "back-law", state: "queued", at: runAt, outcome: null, line: null, denials: [], prompt: null },
        ],
      },
    }),
    snapDone: "paused ok",
  });

// The PR brief on the map (#339), played from the tray as in the prototype's treatment C: a story about the PR that fogged Shell, Understanding and
// Window, with a light run along the links, a figure for the change inside Understanding, a quote, and what the checks left unconfirmed.
const step = (k: string, h: string, b: string, on: string[], extra: Partial<BriefStep> = {}): BriefStep => ({
  k, h, b, on, hops: [], law: null, covered_by: null, list: [], risks: [], fig: null, check: null, cites: [], flags: [], ...extra,
});
const figure = (kind: string): BriefFigure =>
  kind === "states"
    ? { kind, frame: "terminal t1", items: [{ label: "old · deleted", tone: null, bad: null }, { label: "now · lost", tone: "lost", bad: true }, { label: "kept · running", tone: "live", bad: null }, { label: "gone · ended", tone: null, bad: null }], before: [], after: [] }
    : kind === "sequence"
      ? { kind, frame: null, items: [{ label: "Turn ends", tone: null, bad: null }, { label: "Ask for PRs", tone: null, bad: null }, { label: "Write the brief", tone: "live", bad: null }, { label: "Show in tray", tone: null, bad: null }], before: [], after: [] }
      : { kind, frame: null, items: [], before: ["asks every 5 min", "from a 5 s loop", "one PR number"], after: ["asks at a turn's end", "and on focus", "ready state, head"] };
const brief = (steps: BriefStep[], extra: Partial<PrBrief> = {}): PrBrief => ({ title: "PR briefs on the map", overview: [], chapters: [{ behaviour: "story", parts: ["shell", "understanding", "window"], title: "PR briefs on the map", steps, flags: [] }], also: [], unseen: [], decisions: [], flags: [], ...extra });
const storySteps = (): BriefStep[] => [
  step("Where", "A ready PR now brings its own brief to the map", "When a pull request is ready for review, the Shell asks the Understanding part to write what changed, and the Window plays it on the map.", ["shell", "understanding", "window"], {
    hops: [["shell-understanding", false], ["shell-window", false]], cites: [{ from: "the PR body", quote: "writes its brief when it becomes ready" }],
  }),
  step("Before", "The map showed fog but never said what changed", "Fog said that files had changed since you last looked. Clearing it only meant \u201cI know this\u201d, which taught nothing.", ["window"]),
  step("Now", "A new head only marks the brief stale", "A push to a ready PR marks its brief stale. It is written again when you open it, or when the same head is seen twice in a row.", ["shell", "understanding"], {
    hops: [["shell-understanding", false]], fig: figure("states"), cites: [{ from: "src-tauri/src/pr_briefs.rs", quote: "A new head marks it stale" }],
  }),
  step("Must stay true", "A part is not read until its chapter is", "Reading a chapter to its end is what clears the fog of the parts it lit.", ["window"], { law: "window-quiet" }),
];
const briefOf = (steps: BriefStep[], partial = false, extra: Partial<PrBrief> = {}): BriefView => ({
  partial,
  brief: { version: 1, pr: 334, head: "b".repeat(40), written_at: 1, stale: false, model: "opus", brief: brief(steps, extra) },
});
// Opens the brief from the tray, goes `next` steps on, and checks the panel and the map.
// Chrome runs a snap for about 2 s of virtual time after the page loads and what is left unrun is silent, so a `quick` brief waits the least it can.
const mapBrief = (view: BriefView, next: number, check = "", quick = false, pause = 500): Fixture => ({
  ...mapOpened(
    `
      // Chrome's virtual time stalls animations: the brief plays as it does under reduced motion, whose end state is the one to review.
      const media = window.matchMedia.bind(window);
      window.matchMedia = (query) => (query.includes("prefers-reduced-motion") ? { ...media(query), matches: true } : media(query));
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      const tray = document.querySelector('#map-tray .tray-row[data-pr="334"]');
      if (!tray) return log("snap:fail no tray row for the brief");
      tray.click();
      await wait(${quick ? 300 : 500});
      const panel = document.getElementById("map-brief");
      if (panel.hidden || !panel.classList.contains("open")) return log("snap:fail the brief did not open");
      for (let i = 0; i < ${next}; i++) { document.getElementById("brief-next").click(); await wait(${quick ? 200 : pause}); }
      await wait(${quick ? 200 : pause * 3});
      if (!document.querySelector(".map-world.briefing")) log("snap:fail the map is not in the brief's light");
      if (!document.querySelector(".plate.lit")) log("snap:fail no part is lit");
      if (!document.querySelector(".map-world").style.transform.includes("scale")) log("snap:fail the camera did not frame the parts");
      ${check}
      log("brief ok");`,
    {
      map_state: mapState({ fog: { shell: "thick", understanding: "thin" }, ready: [{ number: 334, title: "PR briefs on the map", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false } }] }),
      pr_brief: view,
      pr_diff: { commit: "b".repeat(40), short: "bbbbbbb", label: "#334 PR briefs on the map", url: null, files: [{ path: "src-tauri/src/pr_briefs.rs", diff: "@@ -1 +1,2 @@\n+the brief" }] },
    },
  ),
  snapDone: "brief ok",
});
const escBrief = `
      const before = document.querySelector(".brief-dots i.at") && [...document.querySelectorAll(".brief-dots i")].findIndex((d) => d.classList.contains("at"));
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true }));
      await wait(200);
      if (!document.getElementById("map-brief").hidden) return log("snap:fail Esc did not close the brief");
      if (document.getElementById("map").hidden) return log("snap:fail Esc left the Map view with a brief open");
      document.querySelector('#map-tray .tray-row[data-pr="334"]').click();
      await wait(700);
      const after = [...document.querySelectorAll(".brief-dots i")].findIndex((d) => d.classList.contains("at"));
      if (after !== before) log("snap:fail the brief did not reopen at its step: " + before + " then " + after);`;
const mapBriefStart = (): Fixture => mapBrief(briefOf(storySteps()), 0, `if (!document.querySelector("#map-brief .brief-title").textContent.includes("brings its own brief")) log("snap:fail first step");`);
const mapBriefStory = (): Fixture => mapBrief(briefOf(storySteps(), false), 2, `${escBrief}
      if (!document.querySelector("#map-brief .brief-cites")) log("snap:fail the step shows no citations");
      document.querySelector("#map-brief .brief-code").click();
      await wait(250);
      if (!document.querySelector("#map-drawer .drawer-code")?.textContent.includes("+the brief")) log("snap:fail See the code does not open the PR's diff");`, true);
const mapBriefFigure = (kind: string): Fixture =>
  mapBrief(briefOf([step("Now", "What happens inside one part", "The change is inside Understanding, so a small figure shows it.", ["understanding"], { fig: figure(kind) })]), 0, `
      if (document.querySelectorAll("#map-brief .brief-fig .fig-chip").length < 3) log("snap:fail the figure draws no chips");`);
const mapBriefUnconfirmed = (): Fixture =>
  mapBrief(briefOf([step("Now", "Cards are written once per turn", "A turn is written once, whatever the order of events.", ["understanding"], { flags: ["the claim is not in the facts"] })], true), 0, `
      if (!document.querySelector("#map-brief .brief-unconfirmed")?.textContent.includes("not confirmed from the facts")) log("snap:fail no unconfirmed mark");
      if (!document.querySelector("#map-brief .brief-partial")?.textContent.includes("Written from part of the diff")) log("snap:fail no partial line");`);
// The prediction check (#340). A pick on a case inside one part: pick, take back, Check, then the right option and the way on.
const pickCheck: NonNullable<BriefStep["check"]> = {
  kind: "pick", q: "A worker's card comes from a turn that ended in Needs you. Is its Quiz stored?", opts: ["Yes: workers skip the rule", "No: the card is stored without its Quiz"], key: 1,
  yes: "Right: worker and check cards follow the same rule as turns.", no: "Not any more: review made worker and check cards lose it too.", flow: null, test: "a_workers_card_is_stored_without_a_quiz",
};
const pickStep = () => step("Your turn", "A worker's card comes from a turn that asked you something", "", ["understanding"], { check: pickCheck, cites: [{ from: "the PR body", quote: "worker cards lose their Quiz" }] });
const mapBriefPick = (option: number): Fixture =>
  mapBrief(briefOf([pickStep()]), 0, `
      const next = document.getElementById("brief-next");
      const opts = [...document.querySelectorAll("#map-brief .brief-opt")];
      if (opts.length !== 2 || !next.disabled || next.textContent !== "Check") return log("snap:fail a pick check starts with Check off");
      opts[${option}].click();
      if (next.disabled) log("snap:fail a pick did not turn Check on");
      opts[${option}].click();
      if (!next.disabled || opts[${option}].getAttribute("aria-pressed") !== "false") log("snap:fail a pick was not taken back");
      opts[${option}].click();
      next.click();
      await wait(100);
      const body = document.querySelector("#map-brief .brief-check .brief-body").textContent;
      const done = [...document.querySelectorAll("#map-brief .brief-opt")];
      if (!done[1].classList.contains("right")) log("snap:fail the right option is not shown");
      if (${option} === 0 && !done[0].classList.contains("miss")) log("snap:fail the wrong pick is not marked");
      if (done[0].classList.contains("right")) log("snap:fail the wrong option is shown as right");
      if (body !== ${JSON.stringify(option === 1 ? pickCheck.yes : pickCheck.no)}) log("snap:fail the answer's words: " + body);
      if (next.textContent !== "Done" || next.disabled) log("snap:fail the way on after Check: " + next.textContent);`, true);

// An order check on a flow change, answered by tapping parts on the map: the writer-ticket's synthetic flow change as the UI plays it, an
// overview of two steps that runs the flow's hops, closed by the order check, then the chapter. The key is Core, Shell, Understanding.
const orderCheck: NonNullable<BriefStep["check"]> = {
  kind: "order", q: "A turn just ended, and Core has noticed. Tap, in order, the three parts the work passes through before anything is stored.", opts: [], key: ["core", "shell", "understanding"],
  yes: "Right: Core, then the Shell, then Understanding.", no: "Not quite. It goes Core, then the Shell, then Understanding. Follow the light.", flow: "turn-flow", test: null,
};
const flowHops: [string, boolean][] = [["shell-core", true], ["shell-understanding", false]];
const flowSteps = (): BriefStep[] => [
  step("Where", "A turn's end now runs through three parts", "The Core notices the end of a turn, the Shell hands it on, and Understanding stores it.", ["core", "shell", "understanding"], { hops: flowHops }),
  step("Your turn", "A turn just ended, and Core has noticed. Tap, in order, the three parts the work passes through before anything is stored.", "", ["core", "shell", "understanding", "window"], { check: orderCheck }),
];
const flowBrief = (): BriefView =>
  briefOf(storySteps().slice(0, 2), false, { overview: flowSteps(), chapters: [{ behaviour: "story", parts: ["shell", "understanding"], title: "Turn writing", steps: storySteps().slice(0, 2), flags: [] }] });
const tap = (ids: string[]) => ids.map((id) => `document.querySelector('.plate[data-id="${id}"]').click(); await wait(50);`).join("\n      ");
const mapBriefOrder = (ids: string[], grade: boolean, check: string): Fixture =>
  mapBrief(flowBrief(), 1, `
      const next = document.getElementById("brief-next");
      if (next.textContent !== "Check" || !next.disabled) return log("snap:fail an order check starts with Check off");
      if (document.querySelectorAll(".plate.pick").length !== 4) log("snap:fail the parts the step lights cannot be tapped: " + document.querySelectorAll(".plate.pick").length);
      if (document.querySelectorAll("#map-brief .brief-slot").length !== 3) log("snap:fail the check has no three slots");
      ${tap(ids)}
      if (document.querySelector(".plate.open")) log("snap:fail a tap opened the part's sheet");
      if (document.querySelectorAll(".plate .ord").length !== ${ids.length}) log("snap:fail the taps carry no numbers on the map");
      if (document.querySelectorAll("#map-brief .brief-slot.full").length !== ${ids.length}) log("snap:fail the slots do not hold the taps");
      ${grade ? `if (next.disabled) return log("snap:fail Check is off with every slot full");
      next.click();
      await wait(250);` : `if (!next.disabled) log("snap:fail Check is on before every slot is full");`}
      ${check}`, true);
const orderTakeBack = `
      document.querySelector('.plate[data-id="shell"]').click();
      await wait(50);
      if (document.querySelectorAll(".plate .ord").length !== 1 || document.querySelector(".plate[data-id='core'] .ord")?.textContent !== "1") log("snap:fail a tap on a part did not take it back");
      document.querySelector('.plate[data-id="shell"]').click();
      await wait(50);`;
const mapBriefOrderSlots = (): Fixture => mapBriefOrder(["core", "shell"], false, orderTakeBack);
const mapBriefOrderWrong = (): Fixture =>
  mapBriefOrder(["understanding", "shell", "core"], true, `
      if (document.querySelector("#map-brief .brief-check .brief-body").textContent !== ${JSON.stringify(orderCheck.no)}) log("snap:fail the correction's words");
      const slots = [...document.querySelectorAll("#map-brief .brief-slot span:last-child")].map((n) => n.textContent);
      if (slots.join() !== "Core,Shell,Understanding") log("snap:fail the slots do not show the right order: " + slots);
      if (document.querySelectorAll(".map-light .light-trail").length !== 2) log("snap:fail the right path was not replayed: " + document.querySelectorAll(".map-light .light-trail").length);
      if (next.textContent !== "Next") log("snap:fail the way on after Check: " + next.textContent);`);
// Right, and on through the chapter: the brief plays end to end.
const mapBriefOrderRight = (): Fixture =>
  mapBriefOrder(["core", "shell", "understanding"], true, `
      if (document.querySelector("#map-brief .brief-check .brief-body").textContent !== ${JSON.stringify(orderCheck.yes)}) log("snap:fail the right answer's words");
      next.click();
      await wait(300);
      if (!document.querySelector("#map-brief .brief-title").textContent.includes("A ready PR now brings")) log("snap:fail the chapter did not follow the overview");`);
// The brief's ratify step (#343): a changed law and a link, and the agent's two decisions, one already accepted.
const ratifyRows: RatifyRow[] = [
  { pr: 334, kind: "law", id: "window-quiet", text: "Only needs-you has colour.", change: "changed", status: "not-yet" },
  { pr: 334, kind: "link", id: "shell-understanding", text: "Shell to Understanding", change: "added", status: "not-yet" },
  { pr: 334, kind: "decision", id: "d1", text: "A brief is stored per project, since PR numbers repeat across repositories.", change: "", status: "not-yet" },
  { pr: 334, kind: "decision", id: "d2", text: "A merged PR's brief is read as stored, never rewritten.", change: "", status: "accepted" },
];
const mapBriefRatify = (): Fixture => ({
  ...mapBrief(briefOf(storySteps()), 4, `
      const rows = [...document.querySelectorAll("#map-brief .ratify-row")];
      if (rows.length !== 4) return log("snap:fail expected 4 ratify rows, saw " + rows.length);
      if (document.querySelectorAll("#map-brief .ratify-accept").length !== 3) log("snap:fail Accept belongs on the three undecided rows");
      if (!rows[3].textContent.includes("Accepted")) log("snap:fail the decided row does not say so");
      document.querySelector("#map-brief .ratify-row .ratify-accept").click();
      await wait(300);
      if (!document.querySelector("#map-brief .ratify-row").textContent.includes("Accepted")) log("snap:fail Accept did not mark the row");
      log("ratify ok");`, false, 100),
  pr_ratify: ratifyRows,
});
// The end-of-story bar (#393) under the story's last step: 4 picked by its key, an "s" typed in the note that does not skip, the note kept by
// Enter; each is sent to the shell, and no rating is shown back once the brief is opened again. The screenshot is the bar after the note.
const mapBriefBar = (): Fixture =>
  mapBrief(briefOf(storySteps()), 3, `
      const sent = [];
      const real = window.fetch;
      window.fetch = (url, init) => (sent.push(String(url) + " " + (init?.body ?? "")), real(url, init));
      const bar = () => document.querySelector("#map-brief .story-bar");
      if (!bar() || bar().hidden) return log("snap:fail no end-of-story bar on the story's last step");
      const key = (target, k) => target.dispatchEvent(new KeyboardEvent("keydown", { key: k, code: /^[0-9]$/.test(k) ? "Digit" + k : "Key" + k.toUpperCase(), bubbles: true }));
      key(document, "4");
      await wait(50);
      const pressed = [...bar().querySelectorAll(".bar-rate")].filter((b) => b.getAttribute("aria-pressed") === "true").map((b) => b.textContent);
      if (pressed.join() !== "4") log("snap:fail the 4 key did not rate: " + pressed.join());
      const note = bar().querySelector(".bar-note");
      note.focus();
      key(note, "s");
      await wait(50);
      if (bar().hidden) return log("snap:fail an s typed in the note skipped the bar");
      note.value = "The second step lost me.";
      key(note, "Enter");
      await wait(50);
      if (!bar().querySelector(".bar-kept")) log("snap:fail Enter did not keep the note");
      const bars = sent.filter((call) => call.includes("/api/story_bar")).map((call) => JSON.parse(call.slice(call.indexOf(" ") + 1)));
      const kinds = bars.map((b) => b.kind + (b.rating ? ":" + b.rating : "") + (b.note ? ":" + b.note : "")).join();
      if (kinds !== "rating-given:4,note-given:The second step lost me.") log("snap:fail the bar sent " + kinds);
      if (bars.some((b) => b.project !== "/w/plotkeeper" || b.about.pr !== 334 || b.about.chapter !== 0)) log("snap:fail the bar's key: " + JSON.stringify(bars[0]));
      if (["of 5", "/5", "/ 5"].some((score) => bar().textContent.includes(score))) log("snap:fail the bar shows a score");`, false, 100);
// Overturning in the brief's ratify step (port decision Q2): the prompt goes to the clipboard and the step says so, or, when the clipboard
// refuses, shows the prompt in a box with a Copy button. The page's clipboard is stubbed to take or refuse the write.
const mapBriefOverturn = (clipboard: "copies" | "refuses"): Fixture => ({
  ...mapBrief(briefOf(storySteps()), 4, `
      Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: () => ${clipboard === "copies" ? "Promise.resolve()" : 'Promise.reject(new Error("blocked"))'} } });
      document.querySelector("#map-brief .ratify-overturn").click();
      await wait(400);
      const note = document.querySelector("#map-brief .ratify-note");
      if (!note) return log("snap:fail Overturn left no note");
      if (!note.textContent.includes("Prompt copied, paste it into your agent") === ${clipboard === "copies"}) log("snap:fail the note's line: " + note.textContent);
      const box = note.querySelector("textarea.ratify-prompt");
      if (${clipboard === "copies"} ? box : !box || !box.value.includes("#334") || !note.querySelector(".ratify-copy")) log("snap:fail the fallback box: " + note.textContent);
      log("overturn ok");`, false, 100),
  pr_ratify: ratifyRows,
  snapDone: "overturn ok",
});
// A brief that takes a while to write (port decision Q5): the panel and the tray row say "Writing the brief…" and the map stays usable.
const mapBriefWriting = (): Fixture => ({
  ...mapOpened(`
      document.querySelector('#map-tray .tray-row[data-pr="334"]').click();
      await wait(600);
      const panel = document.getElementById("map-brief");
      if (panel.hidden || !panel.textContent.includes("Writing the brief…")) return log("snap:fail the panel does not say the brief is being written");
      if (!document.querySelector('#map-tray .tray-row[data-pr="334"] .tray-note')?.textContent.includes("writing the brief")) log("snap:fail the tray row does not say so");
      if (document.getElementById("map").hidden) log("snap:fail the map is not usable");
      log("writing ok");`, {
    map_state: mapState({ fog: { shell: "thick" }, ready: [{ number: 334, title: "PR briefs on the map", head: "b".repeat(40), brief: null }] }),
    // The write runs in the background: the open answers at once that it is being written.
    pr_brief: { writing: true },
  }),
  snapDone: "writing ok",
});
// A part catch-up (#391): Understanding was fogged by two PRs and a direct push, so its sheet offers the part catch-up as the main button, with
// "I know this" quiet beside it; pressing it plays one story of the part in the brief's reader, with no "See the code" and no PR number.
const catchUpSteps = (): BriefStep[] => [
  step("Where", "Understanding writes what each turn and PR changed", "It turns a session's events into cards and a ready PR into its brief, with a sandboxed writer that has no tools.", ["understanding"], {
    cites: [{ from: "ENTRIES", quote: "writes the PR brief in the background" }],
  }),
  step("Before", "Opening a brief waited for its writer", "Opening a PR's brief that was not written yet held the panel until the writer answered, minutes at a time.", ["understanding", "window"], {
    hops: [["shell-understanding", false], ["shell-window", false]],
  }),
  step("Now", "A brief is written in the background and lands", "An open answers at once that it is being written; the brief lands on the map when the writer is done. A direct push trimmed the facts' budget.", ["understanding"], {
    fig: figure("sequence"), cites: [{ from: "DIFF", quote: "open_in_background" }],
  }),
  step("Must stay true", "The writer still reads only the facts", "The map's law says the writer is asked at most once per item; the claim check and its one retry keep to it.", ["understanding"], { law: "understanding-asked" }),
];
const storedCatchUp = (): StoredCatchUp => ({
  version: 1, part: "understanding", fog: ["c".repeat(40), "d".repeat(40), "e".repeat(40)], written_at: 1, model: "opus", partial: false,
  brief: { ...brief(catchUpSteps()), title: "Briefs are written in the background", chapters: [{ behaviour: "story", parts: ["understanding"], title: "Briefs written in the background", steps: catchUpSteps(), flags: [] }] },
});
const mapCatchUp = (): Fixture => ({
  ...mapOpened(`
      const media = window.matchMedia.bind(window);
      window.matchMedia = (query) => (query.includes("prefers-reduced-motion") ? { ...media(query), matches: true } : media(query));
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      document.querySelector("#map .plate[data-id=understanding]").click();
      await wait(150);
      const buttons = [...document.querySelectorAll(".sheet .sheet-fog button")].map((b) => b.textContent);
      if (buttons.join("|") !== "Part catch-up|I know this") return log("snap:fail the sheet's fog buttons: " + buttons.join("|"));
      if (!document.querySelector(".sheet .sheet-catch-up").classList.contains("primary")) log("snap:fail the part catch-up is not the main button");
      document.querySelector(".sheet .sheet-catch-up").click();
      await wait(350);
      const panel = document.getElementById("map-brief");
      if (panel.hidden || !panel.classList.contains("open")) return log("snap:fail the part catch-up did not open");
      if (!panel.querySelector(".brief-kick").textContent.includes("Part catch-up")) log("snap:fail the kick does not say part catch-up");
      if (!panel.querySelector(".brief-code").hidden) log("snap:fail a part catch-up offers a PR's code");
      document.getElementById("brief-next").click();
      await wait(200);
      document.getElementById("brief-next").click();
      await wait(450);
      if (!document.querySelector(".plate.lit[data-id=understanding]")) log("snap:fail the part is not lit");
      log("catch-up ok");`, {
    map_state: mapState({
      fog: { understanding: "thick", shell: "thin" },
      catch_ups: { understanding: { kind: "catch-up", ready: true, writing: false, failed: false }, shell: { kind: "chapter", pr: 334, chapter: 0 } },
    }),
    part_catch_up: storedCatchUp(),
  }),
  snapDone: "catch-up ok",
});
// A part catch-up being written: its button stays pressable and says "Writing…", the panel says so, and the map is not dimmed.
const mapCatchUpWriting = (): Fixture => ({
  ...mapOpened(`
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      document.querySelector("#map .plate[data-id=understanding]").click();
      await wait(300);
      if (document.querySelector(".sheet .sheet-catch-up-note")?.textContent !== "Writing…") log("snap:fail the sheet does not say it is being written");
      if (document.querySelector(".sheet .sheet-catch-up").disabled) log("snap:fail the button is not pressable while it is written");
      document.querySelector(".sheet .sheet-catch-up").click();
      await wait(600);
      const panel = document.getElementById("map-brief");
      if (panel.hidden || !panel.textContent.includes("Writing the part catch-up…")) return log("snap:fail the panel does not say the catch-up is being written");
      if (document.body.classList.contains("brief-open")) log("snap:fail the map is dimmed while it is written");
      log("catch-up writing ok");`, {
    map_state: mapState({ fog: { understanding: "thick" }, catch_ups: { understanding: { kind: "catch-up", ready: false, writing: true, failed: false } } }),
    part_catch_up: { writing: true },
  }),
  snapDone: "catch-up writing ok",
});
// The part sheet with one law not yet ratified (#343): it says so, with Accept and Overturn.
const mapSheetUnratified = (): Fixture =>
  mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const laws = [...document.querySelectorAll("#map-sheet .sheet-law")];
      const open = laws.filter((l) => l.textContent.includes("Not yet ratified"));
      if (laws.length !== 2 || open.length !== 1 || !open[0].textContent.includes("Notices wait for focus.")) return log("snap:fail laws: " + laws.map((l) => l.textContent).join("|"));
      if (!open[0].querySelector(".ratify-accept") || !open[0].querySelector(".ratify-overturn")) log("snap:fail no Accept and Overturn");
      log("sheet ok");`, {
    map_state: mapState({ ratify: [{ pr: 306, kind: "law", id: "shell-notice" }] }),
  });
// Staleness bands (#392): Core, Shell and Understanding untouched for 3, 6 and 12 weeks carry a band along their foot, firmer with each;
// the Interface-only and Black box parts never do. Understanding's sheet says how long and offers the refresher.
const mapStale = (): Fixture =>
  mapOpened(`
      const band = (id) => [1, 2, 3].find((n) => document.querySelector("#map .plate[data-id=" + id + "]").classList.contains("stale-" + n)) ?? 0;
      const bands = ["core", "shell", "understanding", "plugins", "workbench", "window"].map(band).join(",");
      if (bands !== "1,2,3,0,0,0") return log("snap:fail bands: " + bands);
      if (document.querySelectorAll("#map .plate .plate-stale").length !== 3) log("snap:fail a band without its line");
      document.querySelector("#map .plate[data-id=understanding]").click();
      await wait(300);
      if (!document.querySelector("#map-sheet .sheet-stale")?.textContent.includes("12 weeks")) log("snap:fail the sheet does not say how long");
      if (!document.querySelector("#map-sheet .sheet-refresher")) log("snap:fail no refresher on a stale part with no fog");
      log("stale ok");`, {
    map_state: mapState({ stale: { core: 1, shell: 2, understanding: 3 } }),
  });
// The refresher (#392) on Shell's sheet, a stale part with no fog: drawn at once from the map file (its caption and description, the flows
// through it, its laws with their backing and its links; the sheet already shows the first three), with no writer and no "Writing…".
const mapRefresher = (): Fixture => {
  const file = JSON.parse(mapFile);
  file.flows = [{ id: "turn-writing", name: "Turn writing", hops: [
    { link: "hook-shell", text: "The hook's line reaches the Shell, which reads the log." },
    { link: "shell-understanding", text: "The Shell asks Understanding to write the turn's summary." },
    { link: "understanding-writer", text: "Understanding calls the writer." },
    { link: "shell-window", text: "The Shell tells the Window, which shows the summary." },
  ] }];
  file.parts.find((part: { id: string }) => part.id === "shell").description = "Reads the hook's log, feeds the core, writes the summaries through Understanding and serves the Window.";
  return mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const enter = document.querySelector("#map-sheet .sheet-refresher");
      if (!enter) return log("snap:fail no refresher button");
      enter.click();
      await wait(400);
      const node = document.querySelector("#map-sheet .refresher");
      if (!node) return log("snap:fail the refresher did not show");
      if (node.previousElementSibling?.classList.contains("sheet-laws") !== true) log("snap:fail the refresher is not under the laws");
      const words = node.textContent;
      for (const want of ["Turn writing", "The Shell asks Understanding", "Hook to Shell", "Shell to Window"]) {
        if (!words.includes(want)) log("snap:fail the refresher lacks " + want);
      }
      if (words.includes("Understanding calls the writer.")) log("snap:fail a hop not at the part shows");
      if (document.querySelector("#map-sheet .sheet-refresher")) log("snap:fail the button stays once the refresher shows");
      if (document.body.textContent.includes("Writing")) log("snap:fail the refresher waits on a writer");
      log("refresher ok");`, {
    map_state: mapState({ source: JSON.stringify(file), stale: { shell: 2 } }),
  });
};
// A chaptered brief (#341): the tray lists one row per chapter, each chapter opens on its own and the overview plays before the first one opened, once.
const chapterTitles = ["Fog clears by reading", "Broken laws on the map", "The Seen gate"];
const chapterOf = (i: number, parts: string[]): BriefChapter => ({
  behaviour: chapterTitles[i], parts, title: chapterTitles[i], flags: [],
  steps: [
    step("Before", `${chapterTitles[i]}: how it was`, "Before this change the map showed the fact but said nothing of why it changed.", [parts[0]]),
    step("Now", `${chapterTitles[i]}: what happens now`, "The Shell works it out and the Window shows it, with a light along the link.", parts, { hops: [["shell-window", false]] }),
    step("Could break", `${chapterTitles[i]}: what could break`, "A part read on the way in would look clear before it was.", [parts[parts.length - 1]], { risks: [{ part: parts[0], trigger: "A push lands while the brief is open.", check: "Open the brief again." }] }),
  ],
});
const chapteredView = (): BriefView => ({
  partial: false,
  brief: {
    version: 1, pr: 334, head: "b".repeat(40), written_at: 1, stale: false, model: "opus",
    brief: {
      title: "The Map view: fog, broken laws and Seen", overview: [step("Where", "The Map view now says what changed and clears itself", "Three behaviours are new: fog that clears by reading, broken laws and the Seen gate.", ["shell", "window", "core"], { hops: [["shell-window", false]] })],
      chapters: [chapterOf(0, ["shell", "window"]), chapterOf(1, ["core", "shell", "window"]), chapterOf(2, ["window", "shell"])],
      also: [{ part: "hook", line: "A wording change in a comment.", flags: [] }], unseen: [], decisions: [], flags: [],
    },
  },
});
const chapteredState = () =>
  mapState({ fog: { shell: "thick", window: "thin", core: "thin" }, ready: [{ number: 334, title: "The Map view", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: chapterTitles, quiet: false } }] });
const mapChaptered = (row: number, next: number, check: string): Fixture =>
  mapOpened(
    `
      const media = window.matchMedia.bind(window);
      window.matchMedia = (query) => (query.includes("prefers-reduced-motion") ? { ...media(query), matches: true } : media(query));
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      const rows = [...document.querySelectorAll('#map-tray .tray-row[data-pr="334"]')];
      if (rows.length !== 3) return log("snap:fail expected 3 chapter rows, saw " + rows.length);
      if (rows.map((r) => r.querySelector(".tray-title").textContent).join("|") !== ${JSON.stringify(chapterTitles.join("|"))}) return log("snap:fail the chapter rows' titles");
      ${row < 0 ? "" : `rows[${row}].click();
      await wait(300);
      if (document.getElementById("map-brief").hidden) return log("snap:fail the chapter did not open");
      for (let i = 0; i < ${next}; i++) { document.getElementById("brief-next").click(); await wait(150); }
      await wait(200);`}
      ${check}
      log("brief ok");`,
    { map_state: chapteredState(), pr_brief: chapteredView() },
  );
const mapBriefChapters = (): Fixture => mapChaptered(-1, 0, `if (document.querySelector("#map-tray .tray-note").textContent.includes("1 of 3") === false) log("snap:fail the first row's note");`);
// Chapter 2 opened first carries the overview: its dots are the overview's one step and its own three; Next once lands on its first step.
const mapBriefChapterMid = (): Fixture =>
  mapChaptered(1, 2, `
      if (document.querySelectorAll("#map-brief .brief-dots i").length !== 4) log("snap:fail the first chapter opened should play the overview and its own steps");
      if (!document.querySelector("#map-brief .brief-title").textContent.includes("Broken laws on the map: what happens now")) log("snap:fail mid-chapter step: " + document.querySelector("#map-brief .brief-title").textContent);
      if (document.querySelector("#map-brief .brief-also")) log("snap:fail also-touched shows before the last chapter's end");
      if (document.querySelectorAll(".plate.lit").length < 2) log("snap:fail the chapter's parts are not lit");
      if (document.querySelector("#map-brief .story-bar")) log("snap:fail the end-of-story bar shows mid-chapter");
      // This chapter is not the last, and its last step still ends with the bar (#393).
      document.getElementById("brief-next").click();
      await wait(200);
      if (!document.querySelector("#map-brief .story-bar")) log("snap:fail no end-of-story bar at a chapter's end");
      document.querySelector("#map-brief .brief-back").click();
      await wait(200);`);
// The last chapter's last step ends with the also-touched lines; another chapter, opened second, has no overview.
const mapBriefChapterLast = (): Fixture =>
  mapChaptered(2, 3, `
      if (document.querySelectorAll("#map-brief .brief-dots i").length !== 4) log("snap:fail the last chapter opened first should carry the overview");
      if (!document.querySelector("#map-brief .brief-also")?.textContent.includes("A wording change in a comment")) log("snap:fail no also-touched line on the last step: " + document.querySelector("#map-brief .brief-title").textContent);
      if (!document.querySelector("#map-brief .story-bar")) log("snap:fail no end-of-story bar at the last chapter's end");`);
// The stop-rule review (#314, #384): Studio has run 20 active days and merged 25 PRs, and the Map view was engaged with on one of them, so
// the Map view fell below its threshold, and so did the history strip and "See the proof".
const dueFacts = { active_days: 20, merged_prs: 25, engaged_days: 1, scrubs: 0, proofs: 0, knows: 0, dismissed: [], briefs_offered: 0, briefs_opened: 0, chapters_done: 0, counts: {}, began: "2026-10-01" };
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
    const agy = { agent: "agy", asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, transcript_model: false, context: false, turn_trees: false };
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
      ${jump ? `// A real click, as a mouse gives it: the plate under the chip captures the pointer on its press (#385's review).
      await __snapClick(document.querySelector("#map .plate[data-id=hook] .agent-chip"));
      await wait(400);
      if (document.getElementById("map").hidden) log("snap:fail a chip left the Map view");
      if (document.getElementById("map-panel").hidden || !document.querySelector("#panel-body > .rail")) log("snap:fail a chip did not open its session's story in the left panel");
      if (document.getElementById("session-name").textContent !== "Ship PR 286") log("snap:fail the chip opened the story of " + JSON.stringify(document.getElementById("session-name").textContent));
      if (document.getElementById("map-sheet").childElementCount) log("snap:fail the chip's click opened the plate's sheet");
      log("jump ok");` : ""}`, {
      map_state: mapState(),
      map_place: { s1: ["core", "window"], "s1/w1": ["shell"], s2: ["hook"], s6: [] },
      select: { __by: "id", values: Object.fromEntries(sessions.map((s) => [s.id, { session: s, agent: null }])) },
    });
    return { ...f, sessions, selected: { session: sessions[6], agent: null }, ...(jump && { realPointer: true, snapDone: "jump ok" }) };
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
      if (jumps.length) log("snap:fail the proof has jumps out of the page: " + jumps);
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
const strip = (n: number, label: string, fogged: boolean, brief: number | null = null) => ({ commit: `${n}`.repeat(40), short: `${n}`.repeat(7), label, fogged, brief });
// The rail's quiet row "PR brief ready" in the newest turn of the session whose PR a brief waits for (#344); pressing it opens that brief on the map.
const railBriefReady = (press: boolean): Fixture => ({
  ...(railFixtures as Record<string, () => Fixture>).railPlain(),
  briefs_waiting: { project: "/w/plotkeeper", numbers: [61] },
  map_state: mapState({ fog: { shell: "thick" }, ready: [{ number: 61, title: "Row controls", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false } }] }),
  pr_brief: briefOf(storySteps()),
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    await wait(300);
    const rows = [...document.querySelectorAll(".now-turn .brief-ready")];
    if (rows.length !== 1) return log("snap:fail expected one PR brief row in the newest turn, saw " + rows.length);
    if (rows[0].textContent !== "PR brief ready") log("snap:fail the row says: " + rows[0].textContent);
    if (document.querySelector(".turn.past .brief-ready")) log("snap:fail an earlier turn holds the row");
    document.getElementById("story").scrollTop = 0;
    if (${press}) {
      rows[0].click();
      await wait(900);
      if (document.getElementById("map").hidden) return log("snap:fail the row did not open the Map view");
      if (!document.getElementById("map-brief").classList.contains("open")) return log("snap:fail the brief did not open");
    }
    log("rail brief row ok");`,
});
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
      const offers = [...document.querySelectorAll("#map-sheet .hist-brief")];
      if (offers.length !== 1 || offers[0].closest(".hist-row") !== rows[3] || offers[0].textContent !== "Brief") return log("snap:fail only #306's row offers its brief, saw " + offers.length);
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
    map_history: [strip(1, "#101 Map first drawn", false), strip(2, "#150 Plates and links", false), strip(3, "#210 Fog engine", true), strip(4, "#306 Live agents on the map", true, 306)],
    map_at: { commit: "2".repeat(40), short: "2222222", label: "#150 Plates and links", source: JSON.stringify(past) },
    map_commit: { commit: "2".repeat(40), short: "2222222", label: "#150 Plates and links", url: "https://github.com/danielrispler/plotkeeper/commit/" + "2".repeat(40), files: [{ path: "src-tauri/src/map.rs", diff: "@@ -1,2 +1,2 @@\n-one\n+two" }, { path: "ui/map.ts", diff: "@@ -1 +1,2 @@\n+three" }] },
  });
  return { ...f, sessions: busySessions(), selected: { session: busySessions()[6], agent: null } };
};

// The session list holding what Home held (#382): second lines on needs-you (its question), running (its Intention) and unseen (its last
// Headline) rows only, and "Clear landed" at the list's end while a session has landed.
const listLines = (): Fixture => {
  const now = Date.now();
  const intention = (text: string) => ({ ...session("x", "x", "idle").transcript, intention: text });
  const sessions = [
    session("s1", "Ship PR 286", "needs_you", {
      cwd: "/w/plotkeeper", line: "Bash gh pr merge 286",
      questions: [{ tool_use_id: "q1", agent: null, answer: "open", turn: null, questions: [{ question: "Merge PR 286 now, or wait for the snap job?", options: [] }] }],
    }),
    session("s2", "Correctness pass, #285", "running", { cwd: "/w/plotkeeper", line: "Read ui/list.ts", transcript: intention("Checking each claim in the brief against the code it cites.") }),
    session("s3", "Snap fails in Chrome, #282", "running", { cwd: "/w/plotkeeper", line: "Bash npm run snap", transcript: intention("Printing Chrome's own error to the snap log so a failure says why.") }),
    session("s4", "Stored fixtures, #280", "your_turn", {
      cwd: "/w/plotkeeper", unseen: true,
      turns: [{ started_at: now - 40 * 60e3, ended_at: now - 22 * 60e3, prompt: "Load the older briefs", id: null, message: "Older stored briefs load again; the reader accepts both shapes.", checks: [], listed: [], files: null }],
    }),
    session("s5", "Archive the Tauri app, #332", "your_turn", { cwd: "/w/plotkeeper", landed: true }),
    session("s6", "Strava-style running tracker", "your_turn", { cwd: "/w/timburr", line: "Done." }),
    session("s7", "Night Out Mode", "running", { cwd: "/w/fluffy", line: "Edit probe.ts", transcript: intention("Building the probe for the venue list.") }),
    session("s8", "#48 · Agent deadline", "ended", { cwd: "/w/plotkeeper", landed: true }),
  ];
  return { ...base, sessions, selected: null };
};
const listChecks = `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const line = (id) => document.querySelector('#groups .session-row[data-key="' + id + ' "] .l2')?.textContent ?? null;
      const want = { s1: "Merge PR 286 now, or wait for the snap job?", s2: "Checking each claim in the brief against the code it cites.", s4: "Older stored briefs load again; the reader accepts both shapes.", s5: null, s6: null };
      for (const [id, text] of Object.entries(want)) if (line(id) !== text) log("snap:fail " + id + "'s second line: " + line(id));
      const second = document.querySelector('#groups .session-row[data-key="s2 "] .l2');
      if (getComputedStyle(second).fontSize !== "11.5px" || getComputedStyle(second.querySelector(".row-step")).textOverflow !== "ellipsis") log("snap:fail a second line is not 11.5px cut with an ellipsis");
      if (getComputedStyle(document.querySelector("#groups .logo")).width !== "12px") log("snap:fail the list's logo is not 12px");
      const clear = () => document.querySelector(".list .clear-landed");`;
const listSecondLines = (): Fixture => ({
  ...listLines(),
  snapScript: `${listChecks}
      if (!clear() || clear().textContent !== "Clear landed") log("snap:fail no Clear landed at the list's end");
      if (clear() && getComputedStyle(clear()).color !== "rgb(111, 108, 102)") log("snap:fail Clear landed is not faint: " + getComputedStyle(clear()).color);`,
});
// Clear landed pressed: the landed row is hidden, the toast offers Undo, and Undo brings it back; pressed again for the screenshot.
// The strip and the tab's title follow at once too: the cleared session, unseen here, leaves both.
const listClearLanded = (): Fixture => ({
  ...listLines(),
  sessions: (listLines().sessions as Session[]).map((s) => (s.id === "s5" ? { ...s, unseen: true } : s)),
  snapScript: `${listChecks}
      const title = document.title;
      if (!document.querySelector('#strip [data-key="strip s5"]')) log("snap:fail the landed session is not in the strip before Clear landed");
      clear().click();
      await wait(100);
      if (line("s5") !== null || document.querySelector('#groups .session-row[data-key="s5 "]')) log("snap:fail the landed row is still shown after Clear landed");
      if (document.querySelector('#strip [data-key="strip s5"]')) log("snap:fail the landed session is still in the strip after Clear landed");
      if (document.title === title) log("snap:fail the tab's title still counts the cleared session: " + title);
      if (clear()) log("snap:fail Clear landed shows with nothing landed left");
      const toast = document.getElementById("clear-toast");
      if (toast.hidden || !/Cleared 2 landed/.test(toast.textContent) || !/Undo/.test(toast.textContent)) log("snap:fail the toast: " + toast.textContent);
      toast.querySelector("button").click();
      await wait(100);
      if (!document.querySelector('#groups .session-row[data-key="s5 "]') || !clear()) log("snap:fail Undo did not bring the landed row back");
      clear().click();
      await wait(100);
      log("cleared ok");`,
});
// The writer section (#363) in the Settings panel (#387), opened by the top bar's gear: claude and agy found, or agy alone; `check` runs
// after it has drawn, and a script that presses Test waits for its outcome. The four the ticket names run at 1000x800 too (`…Narrow`).
const claudeWriter = { kind: "claude" as const, name: "Claude", found: true, models: ["haiku", "sonnet", "opus", "fable"], efforts: ["", "low", "medium", "high"] };
const agyWriter = { kind: "agy" as const, name: "agy", found: true, models: ["gemini-4-neon-low", "gemini-4-neon-medium", "gemini-4-neon-high", "gemini-4-argon-low", "gemini-4-hydrogen-low"], efforts: [] };
const anthropicWriter = { kind: "anthropic" as const, name: "Anthropic API key", found: false, models: ["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5"], efforts: ["", "low", "medium", "high"] };
const openaiWriter = { kind: "openai" as const, name: "OpenAI-compatible API", found: false, models: ["gpt-6-luna", "gpt-5.6-luna", "gpt-5.6-terra"], efforts: ["", "low", "medium", "high"] };
const writersOpen = (extra: Partial<Fixture>, check: string, press = false): Fixture => ({
  ...list(),
  map_state: mapState(),
  ...extra,
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    await wait(200);
    if (document.getElementById("writer-button")) log("snap:fail the top bar still has a Writer entry");
    document.getElementById("settings-button").click();
    await wait(400);
    const panel = document.querySelector("#map-panel:not([hidden]) #panel-body #writers");
    if (!panel) return log("snap:fail the writer section is not in the Settings panel");
    ${press ? `document.getElementById("writer-test").click(); await wait(300);` : ""}
    ${check}
    await wait(300);
    log("writers ok");`,
  snapDone: "writers ok",
});

// The left panel over the map (#385). Each script fails when a part's saved position changes as a panel opens or closes, when a colour other
// than Needs you orange or broken-law red shows, when a card over the map lacks the shared glass class, or when a layer covers the map where
// no card is. It runs at both sizes: below 1007 px only one overlay is open, so what it expects there differs.
const panelChecks = `
      // Chrome's virtual time stalls animations: the cards are drawn as under reduced motion, whose end state is the one to review.
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      const narrow = innerWidth < 1007;
      const panel = document.getElementById("map-panel");
      const sheet = document.getElementById("map-sheet");
      const panelOpen = () => !panel.hidden;
      const sheetOpen = () => sheet.childElementCount > 0;
      const places = () => [...document.querySelectorAll("#map .plate, #map .actor")].map((n) => n.dataset.id + n.style.left + n.style.top).join();
      const before = places();
      const checkPlaces = (when) => { if (places() !== before) log("snap:fail a part's position changed " + when); };
      const row = (key) => document.querySelector('#groups .session-row[data-key="' + key + ' "]');
      const press = async (node) => { node.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0, buttons: 1 })); await wait(300); };
      const plate = async (id) => { document.querySelector("#map .plate[data-id=" + id + "]").click(); await wait(300); };
      const esc = async () => { document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true })); await wait(300); };
      const palette = async (title) => {
        document.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyP", key: "p", metaKey: true, shiftKey: true, bubbles: true }));
        await wait(100);
        const entry = [...document.querySelectorAll("#palette button.row")].find((b) => b.querySelector(".name").textContent === title);
        if (!entry) return log("snap:fail no palette entry " + title);
        entry.click();
        await wait(400);
      };
      const rgb = (value) => { const m = /rgba?\\(([\\d.]+), ([\\d.]+), ([\\d.]+)(?:, ([\\d.]+))?\\)/.exec(value); return m ? [+m[1], +m[2], +m[3], m[4] === undefined ? 1 : +m[4]] : null; };
      const probe = document.createElement("i");
      document.body.append(probe);
      const allowed = ["--needs-you", "--needs-you-bg", "--needs-you-line", "--conflict"].map((token) => { probe.style.color = "var(" + token + ")"; return rgb(getComputedStyle(probe).color).slice(0, 3).join(); });
      probe.remove();
      // A colour is a fill or a line whose channels differ by more than the cool mist's and the brief's warm light do.
      const checkColours = (when) => {
        const seen = new Set();
        for (const node of document.querySelectorAll("body *")) {
          if (!node.getClientRects().length) continue;
          const style = getComputedStyle(node);
          if (style.visibility !== "visible" || style.display === "none") continue;
          for (const prop of ["color", "backgroundColor", "borderTopColor", "fill", "stroke"]) {
            const c = rgb(style[prop]);
            if (!c || c[3] === 0 || (prop === "color" && !node.textContent.trim())) continue;
            if ((Math.max(c[0], c[1], c[2]) - Math.min(c[0], c[1], c[2])) * c[3] <= 24 || allowed.includes(c.slice(0, 3).join())) continue;
            seen.add(prop + " " + style[prop] + " on " + node.tagName + "." + [...node.classList].join("."));
          }
        }
        if (seen.size) log("snap:fail a colour other than Needs you or broken-law red " + when + ": " + [...seen].slice(0, 4).join(" | "));
      };
      // Each card over the map wears the one glass: the same class, so the same blur, corner and shadow.
      const checkGlass = () => {
        for (const card of [panel, sheet, document.getElementById("map-drawer")]) if (!card.classList.contains("glass")) log("snap:fail #" + card.id + " lacks the shared glass class");
        const look = (node) => ["backdropFilter", "borderTopLeftRadius", "boxShadow", "backgroundColor"].map((prop) => getComputedStyle(node)[prop]).join(" / ");
        if (look(panel) !== look(sheet)) log("snap:fail the panel's glass is not the sheet's: " + look(panel) + " vs " + look(sheet));
        if (getComputedStyle(panel).backdropFilter === "none") log("snap:fail the panel has no backdrop blur");
      };
      // Where no card is, the map itself is under the pointer: a hidden layer (a closed panel, the rail's column) never covers it.
      const checkHit = (when) => {
        const area = document.querySelector("#map .map-canvas").getBoundingClientRect();
        const left = panelOpen() ? panel.getBoundingClientRect().right + 8 : area.left + 8;
        const side = document.getElementById("map-side");
        const right = side.hidden ? area.right - 8 : side.getBoundingClientRect().left - 8;
        // A widened diff in a narrow window can reach under the side area: no band of bare map is left to test there.
        for (const x of right > left ? [left, (left + right) / 2, right] : []) {
          for (const y of [area.top + 8, area.top + area.height / 2]) {
            const hit = document.elementFromPoint(x, y);
            if (!hit || (!hit.closest(".map-canvas") && !hit.closest("#map-tray"))) log("snap:fail " + (hit ? hit.tagName + "." + hit.className : "nothing") + " covers the map at " + Math.round(x) + "," + Math.round(y) + " " + when);
          }
        }
        if (panelOpen()) {
          const box = panel.getBoundingClientRect();
          const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
          if (!hit || !hit.closest("#map-panel")) log("snap:fail the panel is not on top at its middle " + when + ": " + (hit ? hit.tagName + "." + hit.className : "nothing"));
        }
      };
      const checkAll = (when) => { checkPlaces(when); checkColours(when); checkGlass(); checkHit(when); };`;
// The busy map with a session selected by a press, as the shell answers it.
// The map as the page's base (#386), each drawn at 1900x1180 and, as `…Narrow`, at 1000x800, to review beside the prototype's scenes `bare`,
// `zero`, `nomap`, `fresh` and `narrow`. Each script fails when a colour other than Needs you orange or broken-law red shows, or when a layer
// covers a map that is present (`panelChecks`), and checks Treatment C's frame: a 34 px top bar, the list at 252 px (its strip below 1007 px)
// on the darkest tone, the map on `--bg` with a rounded corner, no border line between them, and no Home, centre column or unseen chip.
const baseProjects = [{ path: "/w/plotkeeper", gone: false }, { path: "/w/falinks", gone: false }, { path: "/w/timburr", gone: false }];
const baseChecks = `${panelChecks}
      const frame = (when) => {
        const bar = document.querySelector(".top").getBoundingClientRect();
        if (Math.round(bar.height) !== 34) log("snap:fail the top bar is " + bar.height + " px tall " + when);
        const list = document.querySelector(".list").getBoundingClientRect();
        if (Math.round(list.width) !== (narrow ? 56 : 252)) log("snap:fail the list is " + list.width + " px wide " + when);
        const map = document.getElementById("map");
        const look = getComputedStyle(map);
        if (map.hidden || look.display === "none") return log("snap:fail the map is not the page's base " + when);
        if (Math.abs(map.getBoundingClientRect().right - list.left) > 1) log("snap:fail the map does not meet the list " + when);
        const tone = (token) => { probe.style.color = "var(" + token + ")"; document.body.append(probe); const c = getComputedStyle(probe).color; probe.remove(); return c; };
        const probe = document.createElement("i");
        if (look.backgroundColor !== tone("--bg")) log("snap:fail the map is not on --bg: " + look.backgroundColor);
        if (parseFloat(look.borderTopLeftRadius) <= 0) log("snap:fail the map has no rounded corner");
        if (getComputedStyle(document.body).backgroundColor !== tone("--chrome")) log("snap:fail the frame is not on --chrome");
        for (const node of [map, document.querySelector(".list"), document.querySelector(".top")]) {
          const s = getComputedStyle(node);
          if (["Top", "Right", "Bottom", "Left"].some((side) => parseFloat(s["border" + side + "Width"]) > 0)) log("snap:fail a border line on " + node.className);
        }
        for (const gone of ["#centre", "#column", "#unseen", "#session-header", "#spine", "#map-project", ".pane.home"]) if (document.querySelector(gone)) log("snap:fail " + gone + " is still on the page");
      };`;
const mapBaseOf = (check: string, extra: Fixture = {}): Fixture => ({
  ...base,
  sessions: busySessions(),
  selected: null,
  projects: baseProjects,
  map_state: mapState({ fog: { understanding: "thin" } }),
  map_place: { s1: ["core", "window"], "s1/w1": ["shell"], s2: ["hook"], s6: [] },
  ...extra,
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    await wait(500);
    ${baseChecks}
    ${check}`,
  snapDone: "base ok",
});
// The bare map with the list: the page opens on the last viewed project's map, its name in the top bar, its header marked; Esc does nothing.
const mapBase = (): Fixture =>
  mapBaseOf(`
      if (document.getElementById("bar-project").textContent !== "plotkeeper") log("snap:fail the top bar names " + document.getElementById("bar-project").textContent);
      if (!document.querySelector("#map .map-canvas")) return log("snap:fail the page did not open on a map");
      if (!narrow && !document.querySelector('#groups .project-head.current[data-project="/w/plotkeeper"]')) log("snap:fail the shown project's header is not marked");
      frame("on the bare map");
      checkColours("on the bare map");
      checkHit("on the bare map");
      await esc();
      if (!document.querySelector("#map .map-canvas") || panelOpen()) log("snap:fail Esc on the bare map changed the page");
      log("base ok");`);
// A project with no sessions keeps its header, with the quiet dot drawn as the map's mist; its hover names what lit it. A click on the header
// shows that project's map.
const mapHeaderDot = (): Fixture =>
  mapBaseOf(`
      const head = document.querySelector('#groups .project-head[data-project="/w/falinks"]');
      if (!head) return log("snap:fail falinks has no header");
      const dot = head.querySelector(".project-dot");
      if (!narrow) {
        if (!dot) return log("snap:fail falinks's header has no dot");
        if (dot.title !== "A Core part you track has changed\\nA PR brief is waiting") log("snap:fail the dot's hover says " + JSON.stringify(dot.title));
        if (dot.textContent !== "") log("snap:fail the dot carries text, a count?");
        if (document.querySelector('#groups .project-head[data-project="/w/plotkeeper"] .project-dot')) log("snap:fail an unmarked project has a dot");
      }
      frame("with the dot");
      checkColours("with the dot");
      head.querySelector(".project-toggle").click();
      await wait(400);
      if (document.getElementById("bar-project").textContent !== "falinks") log("snap:fail the header's click did not show falinks's map");
      if (!head.isConnected ? !document.querySelector('#groups .project-head.current[data-project="/w/falinks"]') : !head.classList.contains("current")) log("snap:fail falinks's header is not marked after the switch");
      frame("after the switch");
      checkColours("after the switch");
      checkHit("after the switch");
      log("base ok");`, {
    map_marks: [{ project: "/w/falinks", why: ["fog", "brief"] }],
    map_state: { __by: "project", values: { "/w/plotkeeper": mapState(), "/w/falinks": mapState({ project: "/w/falinks", fog: { understanding: "thick" } }) } },
  });
// A fresh install: no project, so the map's place holds one calm line and the install docs, and nothing covers the page.
const mapFresh = (): Fixture =>
  mapBaseOf(`
      const fresh = document.querySelector("#map .map-empty.map-fresh");
      if (!fresh) return log("snap:fail no fresh-install line: " + document.getElementById("map-body").textContent);
      if (fresh.querySelector(".map-empty-title").textContent !== "Run your agent in a repository; it appears here") log("snap:fail the fresh line says " + fresh.textContent);
      const link = fresh.querySelector("a.map-install");
      if (!link || !link.href.includes("plugin.md")) log("snap:fail no link to the install docs");
      if (document.getElementById("bar-project").textContent !== "plotkeeper") log("snap:fail the top bar names a project");
      frame("on a fresh install");
      checkColours("on a fresh install");
      log("base ok");`, { sessions: [], projects: [] });
// A project without system-map.json keeps today's draft prompt in the map's place.
const mapNoMap = (): Fixture =>
  mapBaseOf(`
      const draft = document.querySelector("#map .map-empty .map-draft");
      if (!draft) return log("snap:fail no draft prompt");
      if (document.querySelector("#map .map-empty.map-fresh")) log("snap:fail the fresh line shows for a project");
      frame("without a map");
      checkColours("without a map");
      log("base ok");`, { map_state: mapState({ status: "none", source: null }) });
// Switching projects (#386's review): what one project's map started is not lost or left over on the next. `calls` are the commands the
// page sends from here on; `open(path)` presses a project's header.
const switchProjects = [...baseProjects, { path: "/w/pokedex", gone: true }];
const switching = (check: string, extra: Fixture = {}): Fixture =>
  mapBaseOf(`
      const calls = [];
      const sent = window.fetch;
      window.fetch = (url, init) => {
        calls.push([/\\/api\\/(\\w+)$/.exec(String(url))?.[1], init?.body ? JSON.parse(init.body) : null]);
        return sent(url, init);
      };
      const open = async (path) => { document.querySelector('#groups .project-head[data-project="' + path + '"] .project-toggle').click(); await wait(150); };
      ${check}
      log("base ok");`, {
    projects: switchProjects,
    map_state: { __by: "project", values: { "/w/plotkeeper": mapState(), "/w/falinks": mapState({ project: "/w/falinks" }) } },
    ...extra,
  });
// A view moved on one project and, within the save's moment, on another: both are saved.
const mapSwitchSavesViews = (): Fixture =>
  switching(`
      const zoom = () => document.querySelector("#map .map-canvas").dispatchEvent(new WheelEvent("wheel", { deltaY: -100, ctrlKey: true, clientX: 700, clientY: 400, bubbles: true, cancelable: true }));
      zoom();
      await open("/w/falinks");
      zoom();
      await wait(700);
      const saved = calls.filter(([command]) => command === "map_save_view").map(([, args]) => args.project).sort().join();
      if (saved !== "/w/falinks,/w/plotkeeper") log("snap:fail the views saved were of " + JSON.stringify(saved));`);
// A gone folder's empty state: the window's focus asks nothing about it.
const mapGoneFocus = (): Fixture =>
  switching(`
      await open("/w/pokedex");
      if (!document.querySelector("#map .map-gone")) return log("snap:fail pokedex shows no gone state");
      calls.length = 0;
      window.dispatchEvent(new Event("focus"));
      await wait(300);
      const asked = calls.filter(([command, args]) => command === "map_state" && args.project === "/w/pokedex");
      if (asked.length) log("snap:fail the focus read the gone folder's map " + asked.length + " times");`);
// A PR brief open on one project's map closes when a project with no map shows.
const mapBriefSwitch = (): Fixture =>
  switching(`
      const media = window.matchMedia.bind(window);
      window.matchMedia = (query) => (query.includes("prefers-reduced-motion") ? { ...media(query), matches: true } : media(query));
      document.querySelector('#map-tray .tray-row[data-pr="334"]').click();
      await wait(800);
      if (document.getElementById("map-brief").hidden) return log("snap:fail the brief did not open");
      await open("/w/pokedex");
      if (!document.getElementById("map-brief").hidden || document.body.classList.contains("brief-open")) log("snap:fail the brief stayed open over the gone project");`, {
    map_state: { __by: "project", values: { "/w/plotkeeper": mapState({ ready: [{ number: 334, title: "PR briefs on the map", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false } }] }) } },
    pr_brief: briefOf(storySteps()),
  });
// The header dots' reads (#386's review): a read that failed is tried again on the next draw of the list, and an older read's answer never
// lands over a newer one's. `answer(fn)` answers \`map_marks\` from here on with \`fn(n)\` for its n-th call, a promise or a value.
const dots = (check: string, extra: Fixture = {}): Fixture =>
  mapBaseOf(`
      const dot = () => document.querySelector('#groups .project-head[data-project="/w/falinks"] .project-dot');
      const sent = window.fetch;
      const answer = (fn) => {
        let n = 0;
        window.fetch = async (url, init) => (String(url).endsWith("/map_marks") ? { ok: true, status: 200, json: async () => await fn(n++) } : sent(url, init));
      };
      const falinks = [{ project: "/w/falinks", why: ["fog"] }];
      ${check}
      log("base ok");`, extra);
const mapDotsRetry = (): Fixture =>
  dots(`
      if (dot()) return log("snap:fail a dot shows though the read failed");
      answer(() => falinks);
      // A press on the shown project's header folds it, which draws the list again.
      document.querySelector('#groups .project-head[data-project="/w/plotkeeper"] .project-toggle').click();
      await wait(200);
      if (!dot()) log("snap:fail the failed read was never tried again");`, { map_marks: { __throw: "the server is busy" } as never });
const mapDotsOrder = (): Fixture =>
  dots(`
      answer((n) => (n === 0 ? new Promise((done) => setTimeout(() => done(falinks), 400)) : []));
      window.dispatchEvent(new Event("focus"));
      await wait(50);
      window.dispatchEvent(new Event("focus"));
      await wait(700);
      if (dot()) log("snap:fail the older read's answer landed over the newer one's");`, { map_marks: [] });
const mapPanel = (check: string, extra: Fixture = {}): Fixture => {
  const sessions = busySessions().map((s) => (s.id === "s1" ? { ...s, turns: s.turns.map((t) => ({ ...t, id: "t1", message: "Chips on the parts a turn touches" })) } : s));
  const selected = Object.fromEntries(sessions.map((s) => [s.id, { session: s, agent: null }]));
  const f = mapOpened(`${panelChecks}
      await wait(300);
      checkAll("on the bare map");
      ${check}`, {
    map_state: mapState({ fog: { understanding: "thin" }, ready: [{ number: 334, title: "PR briefs on the map", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false } }] }),
    map_place: { s1: ["core", "window"], "s1/w1": ["shell"], s2: ["hook"], s6: [] },
    select: { __by: "id", values: selected },
    projects: [{ path: "/w/plotkeeper", gone: false }],
    turn_file_hunk: { text: "@@ -261,7 +261,9 @@ function drawMap(\n   const saved = positions;\n-  const added = autoPlace(items, links);\n+  // Placed once and saved, so a part never moves on its own.\n+  const added = autoPlace(items, links, saved);\n", binary: false, left_out: 0, first_line: 261 },
    ...extra,
  });
  return { ...f, sessions: extra.sessions ?? sessions, selected: null, snapDone: "panel ok" };
};
// The story of Ship PR 286 in the left panel with Understanding's sheet open (both, in a wide window; below 1007 px the sheet takes the
// panel's place, and the row's press takes it back).
const mapPanelStory = (): Fixture =>
  mapPanel(`
      await press(row("s2"));
      if (!panelOpen() || !document.querySelector("#panel-body > .rail")) return log("snap:fail a row's press did not open its story in the left panel");
      if (document.getElementById("map").hidden) return log("snap:fail the row's press left the Map view");
      if (document.querySelector("#map .plate[data-id=hook] .agent-chip.lit") === null) log("snap:fail the open session's chip is not lit");
      // Below 1007 px the open panel leaves no room for the legend (the prototype's scene narrow), and the tray never lies on a plate.
      const checkFoot = (when) => {
        if (narrow && panelOpen() && document.getElementById("map-legend").getClientRects().length) log("snap:fail the legend shows beside the open panel below 1007 px " + when);
        const tray = document.getElementById("map-tray");
        const over = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
        if (!tray.getClientRects().length) return;
        const box = tray.getBoundingClientRect();
        const under = [...document.querySelectorAll("#map .plate, #map .actor")].filter((node) => over(box, node.getBoundingClientRect())).map((node) => node.dataset.id);
        if (under.length) log("snap:fail the tray lies on " + under.join() + " " + when);
      };
      checkFoot("with the story open");
      checkAll("with the story open");
      await plate("understanding");
      if (!sheetOpen()) return log("snap:fail the plate's click did not open its sheet");
      if (panelOpen() === narrow) log("snap:fail " + (narrow ? "below 1007 px the sheet did not close the panel" : "the sheet closed the panel in a wide window"));
      if (narrow) {
        await press(row("s2"));
        if (!panelOpen() || sheetOpen()) log("snap:fail below 1007 px the panel did not take the sheet's place");
      }
      checkFoot("with the story and the sheet");
      checkAll("with the story and the sheet");
      if (!narrow && document.querySelector("#map .plate[data-id=hook] .agent-chip.lit") === null) log("snap:fail the open session's chip went out");
      log("panel ok");`);
// Focus mode below 1007 px (#380's review): the open panel hides the legend and the Ready-for-review tray there as in the strip layout.
const mapPanelFocus = (): Fixture =>
  mapPanel(`
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Enter", key: "Enter", metaKey: true, shiftKey: true, bubbles: true }));
      await wait(300);
      if (document.body.dataset.layout !== "focus") return log("snap:fail ⌘⇧↩ did not enter focus mode: " + document.body.dataset.layout);
      if (!document.getElementById("map-tray").getClientRects().length) return log("snap:fail the tray is hidden with no panel open");
      await press(row("s2"));
      if (!panelOpen()) return log("snap:fail a row's press did not open its story in focus mode");
      for (const id of ["map-legend", "map-tray"]) if (narrow && document.getElementById(id).getClientRects().length) log("snap:fail #" + id + " shows beside the open panel below 1007 px in focus mode");
      log("panel ok");`);
// Esc peels the sheet, then the panel, then nothing of the panel is left; a second press on the open row closes its story.
const mapPanelPeel = (): Fixture =>
  mapPanel(`
      await press(row("s2"));
      await press(row("s2"));
      if (panelOpen()) return log("snap:fail a second press on the same row did not close its story");
      if (document.querySelector("#map .agent-chip.lit")) log("snap:fail the chips stay lit after the story closed");
      checkAll("after the second press");
      await press(row("s2"));
      await plate("understanding");
      await esc();
      if (sheetOpen()) log("snap:fail Esc did not close the sheet first");
      if (panelOpen() === narrow) log("snap:fail Esc closed the panel " + (narrow ? "that the sheet had closed" : "before the sheet"));
      if (!narrow) {
        await esc();
        if (panelOpen()) log("snap:fail the second Esc did not close the panel");
      }
      if (document.getElementById("map").hidden) log("snap:fail Esc left the Map view while a layer was open");
      checkAll("after Esc");
      log("panel ok");`);
// ⌘2 closes the selected session's story and opens it again: the session stays selected while it is closed.
const mapPanelToggle = (): Fixture =>
  mapPanel(`
      await press(row("s2"));
      const cmd2 = async () => { document.dispatchEvent(new KeyboardEvent("keydown", { code: "Digit2", key: "2", metaKey: true, bubbles: true })); await wait(300); };
      if (!panelOpen()) return log("snap:fail the row's press did not open its story before ⌘2");
      await cmd2();
      if (panelOpen()) log("snap:fail ⌘2 did not close the story");
      if (row("s2").getAttribute("aria-pressed") !== "true") log("snap:fail ⌘2 deselected the session");
      await cmd2();
      if (!panelOpen() || document.getElementById("session-name").textContent !== "Ship PR 286") log("snap:fail ⌘2 did not open the story again");
      log("panel ok");`);
// A turn's whole diff, widened, from the palette: it closes the part's sheet; its Back shows the story again.
const mapPanelDiff = (): Fixture =>
  mapPanel(`
      await press(row("s1"));
      if (!narrow) {
        await plate("core");
        if (!sheetOpen()) return log("snap:fail the sheet did not open beside the story");
      }
      await palette("Open full diff");
      if (!panelOpen() || !panel.querySelector(".turn-diff")) return log("snap:fail the diff did not open in the left panel");
      if (sheetOpen()) log("snap:fail the widened diff did not close the part's sheet");
      const width = panel.getBoundingClientRect().width;
      const room = document.querySelector("#map .map-stage").getBoundingClientRect().width - 44;
      if (Math.round(width) !== Math.round(Math.min(780, room))) log("snap:fail the diff's panel is " + width + " px wide, not min(780, " + room + ")");
      if (!panel.querySelector(".diff-line.add")) log("snap:fail the diff shows no lines");
      checkAll("with the diff open");
      // Closing the diff, like closing its story, leaves the bare map with no session selected.
      await esc();
      if (panelOpen()) log("snap:fail Esc did not close the diff");
      if (row("s1").getAttribute("aria-pressed") === "true") log("snap:fail closing the diff left its session selected");
      log("panel ok");`);
// History in the left panel, from the list's link.
const mapPanelHistory = (): Fixture =>
  mapPanel(`
      document.querySelector("#history-link .history-link").click();
      await wait(300);
      if (!panelOpen() || panel.querySelector(".panel-title")?.textContent !== "History") return log("snap:fail the History link did not open History in the left panel");
      if (panel.querySelectorAll(".history .row").length !== 2) log("snap:fail History lists " + panel.querySelectorAll(".history .row").length + " ended sessions");
      const tray = document.getElementById("map-tray").getBoundingClientRect();
      if (tray.left < panel.getBoundingClientRect().right) log("snap:fail the tray is under the open panel");
      checkAll("with History open");
      log("panel ok");`, {
  sessions: [...busySessions(), session("e1", "Map view drawer, #312", "ended", { last_event_at: Date.now() - 2 * 86400e3 }), session("e2", "Wire check covers MapState, #333", "ended", { last_event_at: Date.now() - 3 * 86400e3 })],
});
// Settings in the left panel from the top bar's gear, holding the writer section (#387); the gear again closes it.
const mapPanelSettings = (): Fixture =>
  mapPanel(`
      document.getElementById("settings-button").click();
      await wait(300);
      if (!panelOpen() || panel.querySelector(".panel-title")?.textContent !== "Settings") return log("snap:fail the gear did not open Settings in the left panel");
      if (document.getElementById("settings-button").ariaPressed !== "true") log("snap:fail the gear does not show pressed");
      if (!panel.querySelector("#writers #writer-pick")) log("snap:fail the Settings panel does not hold the writer section");
      document.getElementById("settings-button").click();
      await wait(200);
      if (panelOpen()) log("snap:fail the gear again did not close Settings");
      await palette("Show settings");
      if (!panelOpen()) return log("snap:fail the palette did not open Settings");
      checkAll("with Settings open");
      log("panel ok");`);
// A PR brief over the story and the sheet: the one thing that dims the map, and both panels with it.
const mapPanelBrief = (): Fixture =>
  mapPanel(`
      await press(row("s2"));
      if (!narrow) await plate("understanding");
      const media = window.matchMedia.bind(window);
      window.matchMedia = (query) => (query.includes("prefers-reduced-motion") ? { ...media(query), matches: true } : media(query));
      document.querySelector('#map-tray .tray-row[data-pr="334"]').click();
      await wait(800);
      if (document.getElementById("map-brief").hidden) return log("snap:fail the brief did not open");
      // A card's glass stays whole, so the dimmed plates under it never show through; what it holds is dimmed (the prototype's scene brief).
      const dim = Number(getComputedStyle(document.documentElement).getPropertyValue("--brief-dim"));
      const cards = [panel, ...document.querySelectorAll("#map-side > .glass")].filter((card) => card.getClientRects().length);
      if (cards.length < (narrow ? 1 : 2)) log("snap:fail the brief shows " + cards.length + " glass cards");
      for (const card of cards) {
        for (let node = card; node && node.id !== "map"; node = node.parentElement) {
          if (Number(getComputedStyle(node).opacity) !== 1) log("snap:fail #" + card.id + " lets the map show through it under the brief: opacity " + getComputedStyle(node).opacity + " on " + node.tagName + "." + node.className);
        }
        for (const inner of card.children) {
          if (inner.getClientRects().length && Number(getComputedStyle(inner).opacity) !== dim) log("snap:fail the brief did not dim #" + card.id + " > " + inner.tagName + "." + inner.className + ": " + getComputedStyle(inner).opacity);
        }
      }
      if (!panelOpen()) log("snap:fail the brief closed the panel");
      if (!narrow && !sheetOpen()) log("snap:fail the brief closed the sheet");
      checkPlaces("under the brief");
      checkColours("under the brief");
      checkGlass();
      log("panel ok");`, {
  pr_brief: briefOf(storySteps()),
});

export const fixtures = {
  ...railFixtures,
  // Unconfigured, with both writers found: preset to Claude, summaries off until a Test passes.
  writersList: () =>
    writersOpen({ writers: { writers: [claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: null } }, `
      const picks = [...document.querySelectorAll("#writer-pick option")].map((o) => o.textContent + (o.disabled ? "!" : ""));
      if (picks.join() !== "Claude,agy") log("snap:fail the writers listed: " + picks.join());
      if (document.getElementById("writer-pick").value !== "claude") log("snap:fail the preset is not Claude");
      if (document.getElementById("writer-model").value !== "haiku") log("snap:fail the preset model is not haiku");
      if (!document.getElementById("notice").textContent.startsWith("Summaries are off: ")) log("snap:fail the gate's notice is not shown");`),
  // A passed Test of agy, the only writer found.
  writersPassed: () =>
    writersOpen({
      writers: { writers: [{ ...claudeWriter, found: false }, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: null },
      test_writer: { outcome: "passed", line: "Passed: \`agy\` answered. Summaries are written with it.", settings: null },
    }, `
      if (document.getElementById("writer-pick").value !== "agy") log("snap:fail the preset is not agy, the first found");
      const outcome = document.getElementById("writer-outcome");
      if (!outcome?.classList.contains("passed") || !outcome.textContent.startsWith("Passed")) log("snap:fail no passed outcome: " + outcome?.textContent);`, true),
  // A failed Test with its reason and the writer's own fix.
  writersFailed: () =>
    writersOpen({
      writers: { writers: [claudeWriter, agyWriter], summaries_on: true, briefs: true, keys: [], base_url: null },
      test_writer: { outcome: "not_logged_in", line: "\`claude\` is not logged in. Run \`claude\` in a terminal and log in with /login.", settings: null },
    }, `
      const outcome = document.getElementById("writer-outcome");
      if (!outcome?.classList.contains("failed") || !outcome.textContent.includes("/login")) log("snap:fail no failed outcome with its fix: " + outcome?.textContent);`, true),
  // No API key set (#366): the writer is listed but cannot be picked, and the key row offers to add one.
  writersKeyNotSet: () =>
    writersOpen({ writers: { writers: [anthropicWriter, claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: null } }, `
      const first = document.querySelector("#writer-pick option");
      if (first?.textContent !== "Anthropic API key (not set)" || !first.disabled) log("snap:fail the unset key is not listed first and unpickable: " + first?.textContent);
      if (!document.getElementById("writer-key-state")?.textContent.includes("not set")) log("snap:fail the key row does not say not set");
      if (document.getElementById("writer-key")?.placeholder !== "Add an API key") log("snap:fail no field to add an API key");
      if (document.getElementById("writer-key-remove")) log("snap:fail Remove shown with no key set");`),
  // An API key set: it comes first in the preset order, and a Test of the Anthropic writer passes.
  writersKeySet: () =>
    writersOpen({
      writers: { writers: [{ ...anthropicWriter, found: true }, claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: ["anthropic"], base_url: null },
      test_writer: { outcome: "passed", line: "Passed: \`anthropic\` answered. Summaries are written with it.", settings: null },
    }, `
      if (document.getElementById("writer-pick").value !== "anthropic") log("snap:fail the preset is not the API key");
      if (document.getElementById("writer-model").value !== "claude-haiku-5-5") log("snap:fail the preset model is not the first");
      if (!document.getElementById("writer-key-state")?.textContent.includes(": set")) log("snap:fail the key row does not say set");
      if (document.getElementById("writer-key")?.placeholder !== "Replace the API key" || !document.getElementById("writer-key-remove")) log("snap:fail no replace or remove");
      const outcome = document.getElementById("writer-outcome");
      if (!outcome?.classList.contains("passed")) log("snap:fail no passed outcome: " + outcome?.textContent);`, true),
  // A local Ollama set by its base URL alone (#367): the OpenAI-compatible API is preset before Claude, its model is typed,
  // its row shows the base URL and no key, and a Test of it passes.
  writersOpenAI: () =>
    writersOpen({
      writers: { writers: [anthropicWriter, { ...openaiWriter, found: true }, claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: "http://localhost:11434/v1" },
      test_writer: { outcome: "passed", line: "Passed: \`openai\` answered. Summaries are written with it.", settings: null },
    }, `
      if (document.getElementById("writer-pick").value !== "openai") log("snap:fail the preset is not the OpenAI-compatible API");
      const model = document.getElementById("writer-model");
      if (model?.tagName !== "INPUT" || model.value !== "gpt-6-luna") log("snap:fail the model is not a typed field at the first model: " + model?.tagName);
      if (document.getElementById("writer-base-url")?.value !== "http://localhost:11434/v1") log("snap:fail the base URL field does not hold the base URL");
      if (!document.getElementById("writer-openai-state")?.textContent.includes("no key")) log("snap:fail the row does not say no key");
      if (!document.getElementById("writer-openai-remove")) log("snap:fail no Remove with a base URL set");
      const outcome = document.getElementById("writer-outcome");
      if (!outcome?.classList.contains("passed")) log("snap:fail no passed outcome: " + outcome?.textContent);`, true),
  // No claude: agy writes the summaries, and the section says PR briefs need Claude.
  writersBriefsNeedClaude: () =>
    writersOpen({
      writers: { writers: [{ ...claudeWriter, found: false }, agyWriter], summaries_on: true, briefs: false, keys: [], base_url: null },
      settings_files: { settings: '{ "summaries": { "writer": "agy", "model": "gemini-4-neon-high" } }', keymap: null },
    }, `
      if (!document.getElementById("writer-briefs")?.textContent.includes("briefs need Claude")) log("snap:fail no briefs need Claude line");
      if (document.getElementById("writer-model").value !== "gemini-4-neon-high") log("snap:fail the model in effect is not shown");`),
  writersListNarrow: (): Fixture => fixtures.writersList(),
  writersPassedNarrow: (): Fixture => fixtures.writersPassed(),
  writersFailedNarrow: (): Fixture => fixtures.writersFailed(),
  writersBriefsNeedClaudeNarrow: (): Fixture => fixtures.writersBriefsNeedClaude(),
  mapTray,
  mapRuns,
  mapRunsPaused,
  railBriefRow: () => railBriefReady(false),
  railBriefOpened: () => railBriefReady(true),
  mapBriefStart,
  mapBriefStory,
  mapBriefStates: () => mapBriefFigure("states"),
  mapBriefSequence: () => mapBriefFigure("sequence"),
  mapBriefCompare: () => mapBriefFigure("compare"),
  mapBriefUnconfirmed,
  mapBriefPickRight: () => mapBriefPick(1),
  mapBriefPickWrong: () => mapBriefPick(0),
  mapBriefOrderSlots,
  mapBriefOrderWrong,
  mapBriefOrderRight,
  mapBriefRatify,
  mapBriefBar,
  mapBriefOverturnCopied: () => mapBriefOverturn("copies"),
  mapBriefOverturnFallback: () => mapBriefOverturn("refuses"),
  mapBriefWriting,
  mapCatchUp,
  mapCatchUpWriting,
  mapSheetUnratified,
  mapStale,
  mapRefresher,
  mapBriefChapters,
  mapBriefChapterMid,
  mapBriefChapterLast,
  logRunning: () => logFixture("running"),
  logDone: () => logFixture("completed"),
  list,
  listSecondLines,
  listClearLanded,
  // The palette open (#220), so the hover check reaches its rows.
  paletteOpen() {
    const snapScript = `document.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyP", key: "P", metaKey: true, shiftKey: true, bubbles: true }));
      await new Promise((done) => setTimeout(done, 300));
      if (!document.querySelector("#palette .row")) log("snap:fail the palette did not open");`;
    return { ...list(), snapScript };
  },
  // The list with each agent's logo in each state (#158): Claude, agy, status-only Codex, another program and a shell.
  logos() {
    const agy = { capabilities: { agent: "agy", asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, transcript_model: false, context: false, turn_trees: false }, model: "gemini-3.8-flash-high" };
    const only = (program: string, extra: Partial<Session> = {}) => ({ status_only: true, capabilities: { agent: "", asks: false, background_tasks: false, replies: false, permission_mode: false, durations: false, transcript_version: false, transcript_model: false, context: false, turn_trees: false }, ...extra, identity: { issue: null, branch: null, pr: null, worktree: "", name: `${program} · plotkeeper` } });
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
  // Alerts: two sessions' turns ended off screen (the note toast), and one session past the 30% context
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
      window.__snapEmit("finished", ["s3", "s4"]);
      await new Promise((done) => setTimeout(done, 50));
      const note = document.getElementById("note-toast");
      if (note.hidden || !note.textContent.includes("2 finished")) log("snap:fail the note toast should say 2 finished");
      // The top bar has no unseen chip (#386): the unseen rows carry their mark in the list.
      if (document.getElementById("unseen")) log("snap:fail the top bar still has an unseen chip");
      const past = [...document.querySelectorAll(".row .id.past")].map((mark) => mark.textContent);
      if (past.join() !== "context 34%") log("snap:fail only the 34% mark should be past the warning: " + past.join());
      log("note: " + note.textContent + " · past: " + past.join());`;
    return { ...base, sessions, selected: { session: sessions[1], agent: null }, snapScript };
  },
  // The saved project list (#381): a project with no sessions keeps its header, after the one with sessions; a gone one is
  // struck through; a header's "⋯" offers Remove project.
  projectsSaved: () =>
    withSaved(`
      const order = heads().map((head) => head.querySelector(".name").textContent).join();
      if (order !== "plotkeeper,falinks,pokedex") log("snap:fail the headers' order: " + order);
      if (named("falinks")?.closest(".project").querySelector(".row")) log("snap:fail a project with no sessions shows rows");
      if (!named("pokedex")?.classList.contains("gone")) log("snap:fail the gone project's header is not marked gone");
      // Remove project is offered only for a project with no live session: one with sessions would come back with their next event.
      if (named("plotkeeper").querySelector(".project-more")) log("snap:fail a project with live sessions offers Remove project");
      named("pokedex").querySelector(".project-more").click();
      await wait(100);
      const menu = document.getElementById("project-actions");
      if (!menu.matches(":popover-open") || menu.textContent.trim() !== "Remove project") log("snap:fail the header's menu does not offer Remove project");
      log("headers " + order);`),
  // A saved project whose folder is gone, in the map's place: its words, its path and Remove project, never an error (#381).
  mapProjectGone: () => withSaved(openGone + `
    log("gone: " + gone.textContent);`),
  // Remove project on the gone state: its header leaves the list and the map gives way to another project.
  mapProjectRemoved: () => withSaved(openGone + `
    remove.click();
    await wait(300);
    if (named("pokedex")) log("snap:fail the removed project keeps its header");
    if (document.querySelector("#map .map-gone")) log("snap:fail the map still shows the removed project");
    log("removed; headers " + heads().map((head) => head.querySelector(".name").textContent).join());`),
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
  // The map's camera (#383): an empty point of the map is the map itself (no layer over it), a drag there pans, a plain wheel does nothing,
  // ⌘+wheel and a pinch zoom up to 200 %, and ⌘0 fits the whole map; no part's place changes. Shown fitted.
  mapCamera: () =>
    mapOpened(`
      const canvas = document.querySelector("#map .map-canvas");
      const world = () => document.querySelector("#map .map-world");
      const camera = () => {
        const m = /translate\\((-?[\\d.e+-]+)px, (-?[\\d.e+-]+)px\\) scale\\(([\\d.e+-]+)\\)/.exec(world().style.transform);
        return m ? { x: +m[1], y: +m[2], scale: +m[3] } : null;
      };
      const places = () => [...document.querySelectorAll("#map .plate, #map .actor")].map((n) => n.dataset.id + n.style.left + n.style.top).join();
      const before = places();
      const box = canvas.getBoundingClientRect();
      const [px, py] = [box.left + 300, box.top + 60];
      const hit = document.elementFromPoint(px, py);
      if (!hit || !(hit === canvas || hit === world())) return log("snap:fail the element under an empty point of the map is not the map: " + (hit ? hit.tagName + "." + hit.className : "none"));
      if (getComputedStyle(hit).cursor !== "grab") log("snap:fail the empty map's cursor is " + getComputedStyle(hit).cursor);
      const start = camera();
      if (!start) return log("snap:fail the world has no camera: " + world().style.transform);
      const pointer = (type, x) => new PointerEvent(type, { bubbles: true, clientX: x, clientY: py, pointerId: 1, button: 0, buttons: 1, isPrimary: true });
      hit.dispatchEvent(pointer("pointerdown", px));
      canvas.dispatchEvent(pointer("pointermove", px + 120));
      canvas.dispatchEvent(pointer("pointerup", px + 120));
      const panned = camera();
      if (Math.round(panned.x - start.x) !== 120 || panned.y !== start.y || panned.scale !== start.scale) log("snap:fail a drag of the empty map did not pan by 120: " + JSON.stringify([start, panned]));
      const wheel = (extra) => new WheelEvent("wheel", { bubbles: true, cancelable: true, deltaY: -100, clientX: px, clientY: py, ...extra });
      const scrolled = document.scrollingElement.scrollTop;
      const plain = wheel({ deltaY: 100 });
      canvas.dispatchEvent(plain);
      if (JSON.stringify(camera()) !== JSON.stringify(panned)) log("snap:fail a plain wheel moved the view: " + JSON.stringify(camera()));
      if (!plain.defaultPrevented || document.scrollingElement.scrollTop !== scrolled) log("snap:fail a plain wheel was not swallowed");
      canvas.dispatchEvent(wheel({ ctrlKey: true }));
      if (!(camera().scale > panned.scale)) log("snap:fail a pinch did not zoom in");
      for (let i = 0; i < 40; i++) canvas.dispatchEvent(wheel({ deltaY: -300, metaKey: true }));
      if (camera().scale !== 2) log("snap:fail zoom is not held at 200 %: " + camera().scale);
      for (let i = 0; i < 40; i++) canvas.dispatchEvent(wheel({ deltaY: 300, metaKey: true }));
      if (camera().scale !== 0.4) log("snap:fail zoom is not held at 40 %: " + camera().scale);
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Digit0", key: "0", metaKey: true, bubbles: true }));
      const fitted = camera();
      if (fitted.scale === 0.4) log("snap:fail ⌘0 did not fit the map");
      // Headless Chrome's virtual time does not run the fit's glide: skip to where it ends.
      world().classList.remove("glide");
      await wait(100);
      const shown = [...document.querySelectorAll("#map .plate, #map .actor")].map((n) => n.getBoundingClientRect());
      if (shown.some((r) => r.left < box.left || r.right > box.right || r.top < box.top || r.bottom > box.bottom)) log("snap:fail ⌘0 left a part outside the map");
      if (places() !== before) log("snap:fail a part's saved position changed");
      log("camera ok: fitted at " + Math.round(fitted.scale * 100) + "% over " + shown.length + " items");`, { map_state: mapState() }),
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
  // A list row hovered lights its session's chips on the map (#382): #311's on Core and Window, and its Worker's on the Shell.
  mapListHover: (): Fixture => {
    const f = mapBusy(false);
    return {
      ...f,
      snapScript: `${f.snapScript}
      const lit = () => [...document.querySelectorAll("#map .agent-chip.lit")].map((c) => c.closest(".plate").dataset.id + ":" + c.textContent).sort().join();
      if (lit() !== "") log("snap:fail chips are lit before a hover: " + lit());
      const row = document.querySelector('#groups .session-row[data-key="s1 "]');
      row.dispatchEvent(new PointerEvent("pointerenter"));
      await wait(100);
      if (lit() !== "core:#311 · Live agents,shell:Map chips,window:#311 · Live agents") log("snap:fail the hovered row's chips: " + lit());
      row.dispatchEvent(new PointerEvent("pointerleave"));
      await wait(100);
      if (lit() !== "") log("snap:fail chips stay lit after the pointer left: " + lit());
      document.querySelector('#groups .session-row[data-key="s2 "]').dispatchEvent(new PointerEvent("pointerenter"));
      await wait(100);
      if (lit() !== "hook:Ship PR 286 needs you") log("snap:fail the needs-you row's chip: " + lit());
      row.dispatchEvent(new PointerEvent("pointerenter"));
      await wait(100);
      log("lit: " + lit());`,
    };
  },
  mapChipJump: () => mapBusy(true),
  // Story 34: every card over the map, the left panel and the sheet and drawer alike, arrives by a 12 px slide with a fade.
  mapGlassMotion: () =>
    mapOpened(`
      const start = (name) => {
        const probe = document.createElement("div");
        probe.style.animation = name + " 1s linear paused";
        document.body.append(probe);
        const look = getComputedStyle(probe);
        const found = [look.opacity, look.transform];
        probe.remove();
        return found;
      };
      const want = { "panel-in": "matrix(1, 0, 0, 1, -12, 0)", "glass-in": "matrix(1, 0, 0, 1, 0, -12)" };
      for (const [name, transform] of Object.entries(want)) {
        const [opacity, moved] = start(name);
        if (opacity !== "0" || moved !== transform) log("snap:fail " + name + " starts at opacity " + opacity + ", " + moved + ", not a 12 px slide with a fade");
      }
      log("motion ok");`, { map_state: mapState(), snapDone: "motion ok" }),
  // The left panel over the map (#385): the story with a part's sheet, Esc's peel and a second press, the widened diff, History, Settings,
  // ⌘2's toggle, and a brief over both panels. `mapPanelNarrow` is the story at 1000x800 (snap:all's size for it), where only one overlay opens.
  mapPanelStory,
  mapPanelPeel,
  mapPanelToggle,
  mapPanelDiff,
  mapPanelHistory,
  mapPanelSettings,
  mapPanelBrief,
  mapPanelNarrow: mapPanelStory,
  mapPanelFocusNarrow: mapPanelFocus,
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
  // No map on the default branch: the empty state shows the drafting prompt to give to a session.
  // The map as the page's base (#386), each also below 1007 px, where the list is its strip.
  mapBase,
  mapBaseNarrow: mapBase,
  mapHeaderDot,
  mapHeaderDotNarrow: mapHeaderDot,
  // Switching projects: both views saved, a gone folder never read on focus, a PR brief closed.
  mapSwitchSavesViews,
  mapGoneFocus,
  mapBriefSwitch,
  // The header dots: a failed read is tried again, and an older answer never lands over a newer one.
  mapDotsRetry,
  mapDotsOrder,
  mapFresh,
  mapFreshNarrow: mapFresh,
  mapNoMap,
  mapNoMapNarrow: mapNoMap,
  mapEmpty: () =>
    mapOpened(`
      const draft = document.querySelector("#map .map-empty .map-draft");
      if (!draft) return log("snap:fail no drafting prompt");
      if (!draft.textContent.includes("system-map.json") || !draft.textContent.includes("system-map check")) log("snap:fail the drafting prompt lacks the file or the check: " + draft.textContent);
      if (document.querySelector("#map .map-empty .btn")) log("snap:fail a button starts a session from the page");
      log("drafting prompt: " + draft.textContent.slice(0, 40));`, { map_state: mapState({ status: "none", source: null }) }),
} satisfies Record<string, () => Fixture>;

