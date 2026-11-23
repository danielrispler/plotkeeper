// Studio state for `npm run snap`: each fixture maps a Tauri command to what it returns.
// A fixture is a function so that its times are relative to the moment it renders.

import type { BriefChapter, BriefFigure, BriefStep, BriefView, Ci, LogEntry, MapState, PrBrief, RatifyRow, Session, SessionState, Subagent } from "../../ui/types.ts";
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
  fog: {}, ci: null, ready: [], ratify: [],
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
      if (document.querySelectorAll(".plate.lit").length < 2) log("snap:fail the chapter's parts are not lit");`);
// The last chapter's last step ends with the also-touched lines; another chapter, opened second, has no overview.
const mapBriefChapterLast = (): Fixture =>
  mapChaptered(2, 3, `
      if (document.querySelectorAll("#map-brief .brief-dots i").length !== 4) log("snap:fail the last chapter opened first should carry the overview");
      if (!document.querySelector("#map-brief .brief-also")?.textContent.includes("A wording change in a comment")) log("snap:fail no also-touched line on the last step: " + document.querySelector("#map-brief .brief-title").textContent);`);
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
const dueFacts = { active_days: 20, merged_prs: 25, map_days: 1, mark_days: 5, map_on_mark_days: 0, scrubs: 0, proofs: 0, knows: 0, dismissed: false, briefs_offered: 0, briefs_opened: 0, chapters_done: 0 };
// Home's dot for a waiting PR brief (#344): no fog and no broken law, but a ready PR has a brief stored, so the shell's marks name the project.
const homeBriefWaiting = (): Fixture => ({
  ...(fixtures as Record<string, () => Fixture>).home(),
  map_marks: ["/w/plotkeeper"],
  map_state: mapState({ ready: [{ number: 334, title: "PR briefs on the map", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false } }] }),
  snapScript: `
    await new Promise((done) => setTimeout(done, 300));
    const marks = [...document.querySelectorAll(".pane.home .fog-mark")];
    if (marks.length !== 1) return log("snap:fail expected one mark for the waiting brief, saw " + marks.length);
    if (marks[0].textContent !== "" || !marks[0].title.includes("PR brief")) log("snap:fail the mark has a count or does not name the brief");
    log("home brief dot ok");`,
});
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

// The writer section (#363) open under Writer: claude and agy found, or agy alone; `check` runs after it has drawn, and a script
// that presses Test waits for its outcome.
const claudeWriter = { kind: "claude" as const, name: "Claude", found: true, models: ["haiku", "sonnet", "opus", "fable"], efforts: ["", "low", "medium", "high"] };
const agyWriter = { kind: "agy" as const, name: "agy", found: true, models: ["gemini-4-neon-low", "gemini-4-neon-medium", "gemini-4-neon-high", "gemini-4-argon-low", "gemini-4-hydrogen-low"], efforts: [] };
const anthropicWriter = { kind: "anthropic" as const, name: "Anthropic API key", found: false, models: ["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5"], efforts: ["", "low", "medium", "high"] };
const writersOpen = (extra: Partial<Fixture>, check: string, press = false): Fixture => ({
  ...list(),
  ...extra,
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    await wait(200);
    document.getElementById("writer-button").click();
    await wait(200);
    const panel = document.getElementById("writers");
    if (!panel.matches(":popover-open")) return log("snap:fail the writer section did not open");
    ${press ? `document.getElementById("writer-test").click(); await wait(300);` : ""}
    ${check}
    log("writers ok");`,
  snapDone: "writers ok",
});

export const fixtures = {
  ...railFixtures,
  // Unconfigured, with both writers found: preset to Claude, summaries off until a Test passes.
  writersList: () =>
    writersOpen({ writers: { writers: [claudeWriter, agyWriter], summaries_on: false, briefs: true } }, `
      const picks = [...document.querySelectorAll("#writer-pick option")].map((o) => o.textContent + (o.disabled ? "!" : ""));
      if (picks.join() !== "Claude,agy") log("snap:fail the writers listed: " + picks.join());
      if (document.getElementById("writer-pick").value !== "claude") log("snap:fail the preset is not Claude");
      if (document.getElementById("writer-model").value !== "haiku") log("snap:fail the preset model is not haiku");
      if (!document.getElementById("notice").textContent.startsWith("Summaries are off: ")) log("snap:fail the gate's notice is not shown");`),
  // A passed Test of agy, the only writer found.
  writersPassed: () =>
    writersOpen({
      writers: { writers: [{ ...claudeWriter, found: false }, agyWriter], summaries_on: false, briefs: true },
      test_writer: { outcome: "passed", line: "Passed: \`agy\` answered. Summaries are written with it.", settings: null },
    }, `
      if (document.getElementById("writer-pick").value !== "agy") log("snap:fail the preset is not agy, the first found");
      const outcome = document.getElementById("writer-outcome");
      if (!outcome?.classList.contains("passed") || !outcome.textContent.startsWith("Passed")) log("snap:fail no passed outcome: " + outcome?.textContent);`, true),
  // A failed Test with its reason and the writer's own fix.
  writersFailed: () =>
    writersOpen({
      writers: { writers: [claudeWriter, agyWriter], summaries_on: true, briefs: true },
      test_writer: { outcome: "not_logged_in", line: "\`claude\` is not logged in. Run \`claude\` in a terminal and log in with /login.", settings: null },
    }, `
      const outcome = document.getElementById("writer-outcome");
      if (!outcome?.classList.contains("failed") || !outcome.textContent.includes("/login")) log("snap:fail no failed outcome with its fix: " + outcome?.textContent);`, true),
  // No API key set (#366): the writer is listed but cannot be picked, and the key row offers to add one.
  writersKeyNotSet: () =>
    writersOpen({ writers: { writers: [anthropicWriter, claudeWriter, agyWriter], summaries_on: false, briefs: true } }, `
      const first = document.querySelector("#writer-pick option");
      if (first?.textContent !== "Anthropic API key (not set)" || !first.disabled) log("snap:fail the unset key is not listed first and unpickable: " + first?.textContent);
      if (!document.getElementById("writer-key-state")?.textContent.includes("not set")) log("snap:fail the key row does not say not set");
      if (document.getElementById("writer-key")?.placeholder !== "Add an API key") log("snap:fail no field to add an API key");
      if (document.getElementById("writer-key-remove")) log("snap:fail Remove shown with no key set");`),
  // An API key set: it comes first in the preset order, and a Test of the Anthropic writer passes.
  writersKeySet: () =>
    writersOpen({
      writers: { writers: [{ ...anthropicWriter, found: true }, claudeWriter, agyWriter], summaries_on: false, briefs: true },
      test_writer: { outcome: "passed", line: "Passed: \`anthropic\` answered. Summaries are written with it.", settings: null },
    }, `
      if (document.getElementById("writer-pick").value !== "anthropic") log("snap:fail the preset is not the API key");
      if (document.getElementById("writer-model").value !== "claude-haiku-5-5") log("snap:fail the preset model is not the first");
      if (!document.getElementById("writer-key-state")?.textContent.includes(": set")) log("snap:fail the key row does not say set");
      if (document.getElementById("writer-key")?.placeholder !== "Replace the API key" || !document.getElementById("writer-key-remove")) log("snap:fail no replace or remove");
      const outcome = document.getElementById("writer-outcome");
      if (!outcome?.classList.contains("passed")) log("snap:fail no passed outcome: " + outcome?.textContent);`, true),
  // No claude: agy writes the summaries, and the section says PR briefs need Claude.
  writersBriefsNeedClaude: () =>
    writersOpen({
      writers: { writers: [{ ...claudeWriter, found: false }, agyWriter], summaries_on: true, briefs: false },
      settings_files: { settings: '{ "summaries": { "writer": "agy", "model": "gemini-4-neon-high" } }', keymap: null },
    }, `
      if (!document.getElementById("writer-briefs")?.textContent.includes("briefs need Claude")) log("snap:fail no briefs need Claude line");
      if (document.getElementById("writer-model").value !== "gemini-4-neon-high") log("snap:fail the model in effect is not shown");`),
  mapTray,
  railBriefRow: () => railBriefReady(false),
  railBriefOpened: () => railBriefReady(true),
  homeBriefWaiting,
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
  mapBriefOverturnCopied: () => mapBriefOverturn("copies"),
  mapBriefOverturnFallback: () => mapBriefOverturn("refuses"),
  mapBriefWriting,
  mapSheetUnratified,
  mapBriefChapters,
  mapBriefChapterMid,
  mapBriefChapterLast,
  logRunning: () => logFixture("running"),
  logDone: () => logFixture("completed"),
  list,
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
  // Home, the waiting room (#238), at 1400x900: 4 running, 1 needs you, 1 unseen, 2 landed and 2 projects. It has no Quiz and no
  // Retest (#347); the script checks none shows.
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
      running("s6", "#58 · Sounds row", 9, "Draw the sounds row with its toggle.", 0),
      running("s7", "#59 · Agent deadline", 6, "Stop an agent that outlives its turn.", 7),
      running("s8", "#61 · Sounds setting", 2, "Add the sounds toggle to the settings page.", 1),
      session("s4", "#55 · Delete default", "your_turn", {
        cwd: "/w/plotkeeper", unseen: true, last_event_at: now - 22 * 60e3,
        turns: [{ started_at: now - 40 * 60e3, ended_at: now - 22 * 60e3, prompt: "Make delete keep the branch", id: null, message: "Delete now removes the worktree and keeps the branch.", checks: [], listed: [], files: null }],
      }),
      session("s3", "#54 · Subagent terminal", "idle", { cwd: "/w/plotkeeper", landed: true, identity: { issue: 54, branch: null, pr: null, worktree: "", name: "#54 · Subagent terminal" } }),
      session("s5", "#48 · Agent deadline", "ended", { cwd: "/w/plotkeeper", landed: true }),
    ];
    const snapScript = `
      if (document.querySelector(".pane.home .slot-card, .pane.home .quiz-region")) log("snap:fail Home shows a Quiz or a Retest");
      if ([...document.querySelectorAll(".pane.home h2")].some((h) => h.textContent === "Retest")) log("snap:fail Home still has a Retest heading");`;
    return { ...base, sessions, projects: ["/w/plotkeeper", "/w/falinks"], selected: null, snapScript };
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
  // changes once the summary arrives.
  homeRefresh(): Fixture {
    const f = fixtures.home();
    const sessions = f.sessions.map((s) => (s.id === "s4" ? { ...s, turns: s.turns.map((t) => ({ ...t, id: "t4" })) } : s));
    const summary = { summary: { summary: "Delete keeps the branch.", headline: "Delete keeps the branch (summary).", lines: [] } };
    const snapScript = `
      // The summary lands 3 s after the first draw (the script runs at 2.5 s): only a refresh draws its Headline.
      const headline = () => document.querySelector('.pane.home [data-key="unseen s4"] .row-step')?.textContent;
      if (headline() !== "Delete now removes the worktree and keeps the branch.") log("snap:fail the row does not show the turn's message before its summary: " + headline());
      await new Promise((done) => setTimeout(done, 1000));
      if (headline() !== "Delete keeps the branch (summary).") log("snap:fail an Unseen row's Headline did not update once its summary arrived: " + headline());`;
    return { ...f, sessions, turn_summary: { __by: "id", values: { t4: { __delay: 3000, value: summary } as never } }, snapScript };
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
      window.__snapEmit("finished", ["s3", "s4"]);
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
  // No map on the default branch: the empty state shows the drafting prompt to give to a session.
  mapEmpty: () =>
    mapOpened(`
      const draft = document.querySelector("#map .map-empty .map-draft");
      if (!draft) return log("snap:fail no drafting prompt");
      if (!draft.textContent.includes("system-map.json") || !draft.textContent.includes("system-map check")) log("snap:fail the drafting prompt lacks the file or the check: " + draft.textContent);
      if (document.querySelector("#map .map-empty .btn")) log("snap:fail a button starts a session from the page");
      log("drafting prompt: " + draft.textContent.slice(0, 40));`, { map_state: mapState({ status: "none", source: null }) }),
} satisfies Record<string, () => Fixture>;

