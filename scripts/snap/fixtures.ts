// Studio state for `npm run snap`: each fixture maps a server command (`POST /api/<command>`) to what it returns.
// A fixture is a function so that its times are relative to the moment it renders.

import type { BriefChapter, BriefFigure, BriefStep, BriefView, CatchUpStage, Ci, LogEntry, MapState, PrBrief, RatifyRow, Session, SessionState, StoredCatchUp, Subagent } from "../../ui/types.ts";
import { base, type Fixture } from "./base.ts";
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

// #458: the project "⋯" menu, placed through the popover's implicit anchor as the rail's (#447): under its button and inside the window, and
// right over it when the button is near the window's bottom edge, where the flipped case pads the list until the header sits there.
const projectMenuScript = (nearBottom: boolean) => `
    const more = named("pokedex").querySelector(".project-more");
    // The list is padded until the button's bottom is 16 px over the window's, too near for the menu under it. In \`vh\`, as headless
    // Chrome's window takes its full height only at its first frame; in a style sheet, as the list's redraws reset its own style.
    ${nearBottom ? 'document.head.appendChild(document.createElement("style")).textContent = "#groups { padding-top: calc(100vh - 16px - " + more.getBoundingClientRect().bottom + "px); }"; await wait(50);' : ""}
    const menu = document.getElementById("project-actions");
    more.click();
    await wait(50);
    if (!menu.matches(":popover-open")) return log("snap:fail the menu did not open");
    const b = more.getBoundingClientRect(), m = menu.getBoundingClientRect();
    if (b.bottom > innerHeight) return log("snap:fail the button is outside the window: " + b.bottom);
    if (m.left < 0 || m.top < 0 || m.right > innerWidth || m.bottom > innerHeight) log("snap:fail the menu leaves the window: " + JSON.stringify(m));
    if (m.right < b.left || m.left > b.right) log("snap:fail the menu is not in line with its button: menu " + m.left + "–" + m.right + ", button " + b.left + "–" + b.right);
    ${nearBottom
      ? 'if (m.bottom > b.top + 0.5 || b.top - m.bottom > 16) log("snap:fail the menu is not right over its button: menu bottom " + m.bottom + ", button top " + b.top);'
      : 'if (m.top < b.bottom - 0.5 || m.top - b.bottom > 16) log("snap:fail the menu is not right under its button: menu top " + m.top + ", button bottom " + b.bottom);'}
    log("project menu at " + [m.left, m.top, m.right, m.bottom].map(Math.round) + "; button at " + [b.left, b.top, b.right, b.bottom].map(Math.round));
    // A real second click on its "⋯" closes it, as the rail's "⋯" does (#452's review).
    await __snapClick(more);
    await wait(50);
    if (menu.matches(":popover-open")) log("snap:fail a second click on its ⋯ left the menu open");
    log("project menu ok");`;

// The Map view (#309) on a map of plotkeeper's own shape, opened by its key. Positions are saved, as the prototype's Treatment C draws them.
const mapFile = JSON.stringify({
  version: 2,
  parts: [
    { id: "plugins", name: "Agent plugins", caption: "Which hooks each agent runs", purpose: "", holds: [], tier: "interface", paths: ["plugin/"] },
    { id: "hook", name: "Hook", caption: "Records every agent event", purpose: "", holds: [], tier: "core", paths: ["hook/"] },
    { id: "core", name: "Core", caption: "Turns events into session state", purpose: "", holds: [], tier: "core", paths: ["core/"] },
    { id: "shell", name: "Shell", caption: "Reads the log, feeds the core, acts", purpose: "", holds: [], tier: "core", paths: ["src-tauri/"] },
    { id: "understanding", name: "Understanding", caption: "Writes what you read, and keeps it", purpose: "", holds: [], tier: "core", paths: ["understanding/"] },
    { id: "window", name: "Window", caption: "Everything you see and press", purpose: "", holds: [], tier: "core", paths: ["ui/"] },
    { id: "daemon", name: "Terminal daemon", caption: "Owns the terminals", purpose: "", holds: [], tier: "core", paths: ["terminal-daemon/"] },
    { id: "workbench", name: "Workbench", caption: "Git and worktrees", purpose: "", holds: [], tier: "black-box", paths: ["workbench/"] },
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
  fog: {}, ci: null, ready: [], ratify: [], runs: { paused: null, runs: [] }, stale: {}, stale_why: {}, catch_ups: {},
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

// After a push (#477, part B): one PR's read carried over a pure rebase, another rewritten after a partial read; each row says why. A
// third has a "Since you read" story of two chapters (#497): a row for each, then the full brief's.
const mapTrayAfterPush = (): Fixture =>
  mapOpened(`
      const notes = [...document.querySelectorAll("#map-tray .tray-row .tray-note")].map((n) => n.textContent);
      if (notes.length !== 5) return log("snap:fail expected 5 tray rows, saw " + notes.length);
      if (notes[0] !== "#334 · Rebased onto main, nothing new in the PR") log("snap:fail the carried row: " + notes[0]);
      if (notes[1] !== "#336 · Rewritten after a new push; you had read 1 of 2") log("snap:fail the rewritten row: " + notes[1]);
      if (notes[3] !== "#337 The push since you read · since you read · 2 of 2") log("snap:fail the story's second row: " + notes[3]);
      log("tray notes ok");`, {
    map_state: mapState({
      ready: [
        { number: 334, title: "PR briefs: the brief written for each ready PR", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false, read: true, note: "Rebased onto main, nothing new in the PR" } },
        { number: 336, title: "Ready for review tray", head: "a".repeat(40), brief: { stale: false, writing: false, chapters: ["The tray"], quiet: false, note: "Rewritten after a new push; you had read 1 of 2" } },
        { number: 337, title: "Since you read", head: "c".repeat(40), brief: { stale: false, writing: false, chapters: ["The story"], quiet: false, since: "The push since you read", since_chapters: ["The read carries", "The fog clears"] } },
      ],
    }),
  });

// The confirm dialog (#459) a destructive button opens: modal, named by its heading, Cancel focused, the confirm button saying `verb`, and
// naming each of `lost`. Nothing reaches the server's `api` until it is confirmed: Esc and Cancel send nothing. `press` is the script that
// presses the button, found anew each time, since a refresh redraws it.
const confirmFirst = (press: string, api: string, verb: string, lost: string[]) => `
      {
        const pause = () => new Promise((done) => setTimeout(done, 100));
        const sent = [];
        const real = window.fetch;
        window.fetch = (url, init) => (sent.push(String(url)), real(url, init));
        const asked = () => sent.filter((call) => call.includes("/api/${api}")).length;
        const dialog = document.getElementById("confirm-dialog");
        // A step that closes the dialog, then lets the answer land. Snap's Chrome rarely draws a frame inside its time budget, so the
        // dialog's close event mostly never fires here, and under load it can land after the next press reopened the dialog; the answer
        // comes from the form's submit and the dialog's cancel, which fire at once.
        const closing = async (close) => {
          close();
          await pause();
        };
        ${press};
        await pause();
        if (!dialog?.matches(":modal")) log("snap:fail ${api}: no modal confirm opened");
        else {
          if (!document.getElementById(dialog.getAttribute("aria-labelledby") ?? "")?.textContent) log("snap:fail ${api}: the confirm has no name");
          if (document.activeElement?.textContent !== "Cancel") log("snap:fail ${api}: Cancel is not focused but " + document.activeElement?.textContent);
          if (document.getElementById("confirm-yes")?.textContent !== ${JSON.stringify(verb)}) log("snap:fail ${api}: the confirm button says " + document.getElementById("confirm-yes")?.textContent);
          for (const word of ${JSON.stringify(lost)}) if (!dialog.textContent.includes(word)) log("snap:fail ${api}: the confirm does not name " + word + ": " + dialog.textContent);
          if (asked()) log("snap:fail ${api} reached the server before the confirm");
          await closing(() => dialog.requestClose());
          if (dialog.open || asked()) log("snap:fail ${api}: Esc did not close the confirm, or sent it");
          ${press};
          await pause();
          await closing(() => [...dialog.querySelectorAll("button")].find((b) => b.textContent === "Cancel").click());
          if (dialog.open || asked()) log("snap:fail ${api}: Cancel did not close the confirm, or sent it");
          ${press};
          await pause();
          await closing(() => document.getElementById("confirm-yes").click());
          if (asked() !== 1) log("snap:fail ${api}: confirmed, it reached the server " + asked() + " times");
        }
        window.fetch = real;
      }`;
// Background runs (#390) beside the map: a running run, the queue behind it, and an ended run that was denied a tool call. Cancel on the
// running run asks first (#459), naming its worktree and changes; Cancel on a queued run and Dismiss on the ended one ask the server at once.
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
      const press = (id, words) => [...document.querySelectorAll(\`#map-runs .run-row[data-run="\${id}"] button\`)].find((b) => b.textContent === words).click();
      press("19a2f-2", "Cancel");
      press("19a2f-0", "Dismiss");
      await new Promise((done) => setTimeout(done, 50));
      if (document.getElementById("confirm-dialog").open) log("snap:fail a queued run's Cancel or an ended run's Dismiss asked first");
      if (!sent.some((call) => call.includes("/api/run_cancel") && call.includes("19a2f-2"))) log("snap:fail Cancel on a queued run did not ask the server at once: " + sent.join(" | "));
      if (!sent.some((call) => call.includes("/api/run_cancel") && call.includes("19a2f-0"))) log("snap:fail Dismiss did not ask the server at once: " + sent.join(" | "));
      ${confirmFirst(`press("19a2f-1", "Cancel")`, "run_cancel", "Cancel run", ["worktree", "changes"])}
      if (!sent.some((call) => call.includes("/api/run_cancel") && call.includes("19a2f-1"))) log("snap:fail confirmed, Cancel did not ask the server for the running run: " + sent.join(" | "));
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
// Pre-flight (#394) of spec #378: Core and Understanding framed on the map as predicted, and the overlay listing each with its laws on trust,
// its fog and its staleness band, opened from the map's Pre-flight button.
const mapPreFlight = (): Fixture =>
  mapOpened(`
      const predicted = [...document.querySelectorAll(".plate.predicted")].map((plate) => plate.dataset.id);
      if (predicted.join(",") !== "core,understanding") return log("snap:fail the predicted plates: " + predicted.join(","));
      if (!document.querySelector('.plate[data-id="core"] .plate-predicted')?.textContent.includes("predicted")) log("snap:fail the plate has no predicted chip");
      document.getElementById("pre-flight-button").click();
      // Past the glass's rise, so the overlay is drawn whole.
      await wait(600);
      const box = document.getElementById("map-pre-flight");
      if (box.hidden) return log("snap:fail the pre-flight did not open");
      if (!box.textContent.includes("#378 Studio: Spec: the modules")) log("snap:fail the issue: " + box.textContent);
      const rows = [...box.querySelectorAll(".pre-flight-row")];
      if (rows.length !== 2) return log("snap:fail expected 2 rows, saw " + rows.length);
      if (!rows[0].textContent.includes("On trust: Events apply in order.") || rows[0].textContent.includes("The core reads no file")) log("snap:fail the core's laws: " + rows[0].textContent);
      if (!rows[1].textContent.includes("fog: laws, links or flows changed")) log("snap:fail the understanding's fog: " + rows[1].textContent);
      if (!rows[0].textContent.includes("weeks or more ago")) log("snap:fail the core's staleness band: " + rows[0].textContent);
      log("pre-flight ok");`, {
    map_state: mapState({
      fog: { understanding: "thick" },
      stale: { core: 1 },
      pre_flight: { issue: 378, title: "Studio: Spec: the modules", parts: ["core", "understanding"], predicted: true, at: runAt },
    }),
    snapDone: "pre-flight ok",
  });
// #456: the pre-flight's reading, predicting and failed states are announced politely (role="status").
const mapPreFlightStatus = (): Fixture =>
  mapOpened(`
      const box = document.getElementById("map-pre-flight");
      // Each status shows as a line and is said in the overlay's one status region, the same node from the first (#452's review).
      let first = null;
      const status = (words, why) => {
        const node = [...box.querySelectorAll("p:not([role=status])")].find((each) => each.textContent.includes(words));
        if (!node) return log("snap:fail " + why + ": no line with " + JSON.stringify(words) + " in " + JSON.stringify(box.textContent));
        const live = [...box.querySelectorAll("[role=status], [aria-live]")];
        first ??= live[0];
        if (live.length !== 1 || live[0] !== first || !live[0].isConnected) log("snap:fail " + why + ": the overlay's status region is not the one it had");
        if (!live[0]?.textContent.includes(words)) log("snap:fail " + why + " is not said: the region holds " + JSON.stringify(live[0]?.textContent));
      };
      document.getElementById("pre-flight-button").click();
      // A region new to the overlay takes its first words a moment after it is drawn.
      await wait(200);
      status("Reading the open issues", "the issues' reading");
      await wait(400);
      const issue = box.querySelector(".pre-flight-issue");
      if (!issue) return log("snap:fail the open issues never came");
      issue.click();
      await wait(50);
      status("Asking the writer", "the prediction's wait");
      await wait(500);
      status("No prediction", "the failed prediction");
      log("pre-flight status ok");`, {
    map_state: mapState(),
    pre_flight_issues: { __delay: 300, value: base.pre_flight_issues } as never,
    pre_flight_predict: { __delay: 200, value: { __throw: "the writer is not set" } } as never,
    snapDone: "pre-flight status ok",
  });
// Back a law (#398): Hook's law on trust offers Back a law; Core's law on trust has a proven patch, and Review opens the patch review over
// the map with the test patch, the output with the test alone and with the plant, and Discard.
const lawPatchDiff = [
  "diff --git a/core/tests/order.rs b/core/tests/order.rs",
  "new file mode 100644",
  "--- /dev/null",
  "+++ b/core/tests/order.rs",
  "@@ -0,0 +1,6 @@",
  "+// @law: core-order",
  "+#[test]",
  "+fn events_apply_in_order() {",
  "+    let state = apply(&[started(), ended()]);",
  "+    assert_eq!(state.last(), Some(Ended));",
  "+}",
  "diff --git a/system-map.json b/system-map.json",
  "--- a/system-map.json",
  "+++ b/system-map.json",
  "@@ -4,1 +4,1 @@",
  '-    { "id": "core-order", "on": "core", "backing": "trust", "text": "Events apply in order." },',
  '+    { "id": "core-order", "on": "core", "backing": "check", "text": "Events apply in order." },',
].join("\n");
const mapBackLaw = (): Fixture =>
  mapOpened(`
      document.querySelector("#map .plate[data-id=hook]").click();
      await wait(300);
      const held = document.querySelector('#map-sheet .sheet-law[data-law="hook-held"]');
      if (![...(held?.querySelectorAll("button") ?? [])].some((b) => b.textContent === "Back a law")) return log("snap:fail Hook's law on trust has no Back a law");
      document.querySelector("#map .plate[data-id=core]").click();
      await wait(300);
      const pure = document.querySelector('#map-sheet .sheet-law[data-law="core-pure"]');
      if ([...pure.querySelectorAll("button")].some((b) => b.textContent === "Back a law")) log("snap:fail a law a check backs offers Back a law");
      const review = [...document.querySelectorAll('#map-sheet .sheet-law[data-law="core-order"] button')].find((b) => b.textContent === "Review the patch");
      if (!review) return log("snap:fail the proven patch has no Review");
      review.click();
      await wait(300);
      const box = document.getElementById("map-patch-review");
      if (box.hidden) return log("snap:fail the patch review did not open");
      if (!box.textContent.includes("Events apply in order.") || !box.textContent.includes("events_apply_in_order in core/tests/order.rs")) log("snap:fail the law or the test: " + box.textContent);
      if (box.querySelectorAll(".patch-diff .diff-line.add").length !== 7) log("snap:fail the test patch's added lines: " + box.querySelectorAll(".patch-diff .diff-line.add").length);
      const outputs = [...box.querySelectorAll(".patch-output")].map((pre) => pre.textContent);
      if (outputs.length !== 2 || !outputs[0].includes("PASS") || !outputs[1].includes("FAIL")) log("snap:fail the outputs: " + outputs.join(" / "));
      if ([...box.querySelectorAll("button")].map((b) => b.textContent).join(",") !== "Open draft PR,Discard,Close") log("snap:fail the review's buttons");
      // A real Esc in the confirm closes the confirm alone: the window's Esc, which peels the map's layers, never sees it.
      [...box.querySelectorAll("button")].find((b) => b.textContent === "Discard").click();
      await wait(100);
      await __snapKey("Escape");
      await wait(100);
      if (document.getElementById("confirm-dialog").open) log("snap:fail a real Esc did not close the confirm");
      if (box.hidden) log("snap:fail a real Esc in the confirm also closed the patch review under it");
      ${confirmFirst(`[...document.querySelectorAll("#map-patch-review button")].find((b) => b.textContent === "Discard").click()`, "law_patch_discard", "Discard patch", ["patch"])}
      log("back a law ok");`, {
    map_state: mapState({
      law_patches: [
        {
          id: "19a2f-5", project: "/w/plotkeeper", law: "core-order", state: "proven", line: null, at: runAt, diff: lawPatchDiff,
          proof: {
            test: "events_apply_in_order", file: "core/tests/order.rs",
            passed: "$ cargo nextest run -p studio-core\n        PASS [   0.004s] studio-core::it order::events_apply_in_order\n     Summary [   0.912s] 214 tests run: 214 passed, 0 skipped",
            failed: "$ cargo nextest run -p studio-core\n        FAIL [   0.004s] studio-core::it order::events_apply_in_order\n     Summary [   0.917s] 214 tests run: 213 passed, 1 failed, 0 skipped",
          },
        },
      ],
    }),
    realKeys: true,
    snapDone: "back a law ok",
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
// #465: the pause is said in the runs area's one status region, which a redraw (Resume reads the map back) keeps in place; a copied
// prompt is said once in the page's action region, drawn empty before its words, and the row's note itself is not live.
const mapRunsAnnounced = (): Fixture => ({
  ...mapRunsPaused(),
  snapScript: mapOpened(`
      const runs = document.getElementById("map-runs");
      const live = [...runs.children].filter((node) => node.getAttribute("role") === "status");
      if (live.length !== 1 || !live[0].classList.contains("visually-hidden")) return log("snap:fail the runs area has " + live.length + " status regions, not one hidden");
      if (!live[0].textContent.includes("Background runs are paused. Claude is not logged in")) log("snap:fail the runs' region says " + JSON.stringify(live[0].textContent));
      if (runs.querySelectorAll("[role=status], [aria-live]").length !== 1) log("snap:fail another line of the runs area is live");
      let left = false;
      new MutationObserver((changes) => (left ||= changes.some((change) => [...change.removedNodes].includes(live[0])))).observe(runs, { childList: true });
      const said = [];
      const action = () => [...document.body.children].find((node) => node.getAttribute("role") === "status");
      new MutationObserver(() => {
        const now = action()?.textContent;
        if (now !== undefined && said.at(-1) !== now) said.push(now);
      }).observe(document.body, { childList: true, subtree: true, characterData: true });
      if (action()) log("snap:fail the action region is there before any action");
      Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: () => Promise.resolve() } });
      [...runs.querySelectorAll(".run-row button")].find((b) => b.textContent === "Copy prompt").click();
      await wait(400);
      const line = runs.querySelector(".run-copied");
      if (!line) return log("snap:fail the copy left no note");
      if (line.hasAttribute("role") || line.hasAttribute("aria-live")) log("snap:fail the copy's note is live itself");
      if (said.length !== 2 || said[0] !== "" || said[1] !== "Prompt copied, paste it into your agent") log("snap:fail the action region said " + JSON.stringify(said) + ", not empty then the copy's line");
      const region = action();
      [...runs.querySelectorAll(".runs-paused button")].find((b) => b.textContent === "Resume").click();
      await wait(400);
      const now = [...runs.children].filter((node) => node.getAttribute("role") === "status");
      if (now.length !== 1 || now[0] !== live[0] || left) log("snap:fail the runs' region did not stay in the area across the redraw");
      if (action() !== region || said.length !== 2) log("snap:fail the redraw touched the action region: " + JSON.stringify(said));
      log("announced ok");`).snapScript,
  snapDone: "announced ok",
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
// #456: a real Next and Back leave only the new step's title in the brief's one live region, the step itself is not live, and the dots
// say "Step N of M".
const mapBriefAnnounce = (): Fixture => ({
  ...mapBrief(briefOf(storySteps()), 0, `
      const box = document.getElementById("map-brief");
      const count = ${storySteps().length};
      const heard = (at, why) => {
        const live = [...box.querySelectorAll("[aria-live], [role=status], [role=alert]")];
        if (live.length !== 1 || live[0].getAttribute("aria-live") !== "polite") return log("snap:fail " + why + ": the brief has " + live.length + " live regions, not one polite");
        const title = box.querySelector(".brief-title").textContent;
        if (live[0].textContent !== title) log("snap:fail " + why + ": the live region holds " + JSON.stringify(live[0].textContent) + ", not the title " + JSON.stringify(title));
        if (live[0].checkVisibility() && live[0].offsetWidth > 1) log("snap:fail " + why + ": the live region is drawn");
        const dots = box.querySelector(".brief-dots");
        if (dots.getAttribute("role") !== "img" || __snapName(dots) !== "Step " + at + " of " + count) log("snap:fail " + why + ": the dots read " + JSON.stringify(__snapName(dots)));
      };
      await __snapClick(document.getElementById("brief-next"));
      await wait(500);
      heard(2, "after Next");
      await __snapClick(box.querySelector(".brief-back"));
      await wait(500);
      heard(1, "after Back");`, true),
  realPointer: true,
});
const mapBriefStart = (): Fixture => mapBrief(briefOf(storySteps()), 0, `if (!document.querySelector("#map-brief .brief-title").textContent.includes("brings its own brief")) log("snap:fail first step");`);
const mapBriefStory = (): Fixture => mapBrief(briefOf(storySteps(), false), 2, `${escBrief}
      if (!document.querySelector("#map-brief .brief-cites")) log("snap:fail the step shows no citations");
      document.querySelector("#map-brief .brief-code").click();
      await wait(250);
      if (!document.querySelector("#map-drawer .drawer-code")?.textContent.includes("+the brief")) log("snap:fail See the code does not open the PR's diff");`, true);
// #490: a step's own See the code asks for the files of the parts it lights ("Now" lights the Shell and Understanding), not the PR's every file.
const prFile = (path: string) => ({ path, diff: `@@ -1 +1,2 @@\n+${path}` });
const mapBriefStepCode = (): Fixture => ({
  ...mapBrief(briefOf(storySteps(), false), 2, `
      const button = document.querySelector("#map-brief .brief-txt .brief-code");
      if (!button) return log("snap:fail the step has no See the code of its own");
      if (document.querySelector("#map-brief .brief-foot .brief-code")) log("snap:fail the foot still holds a See the code");
      button.click();
      await wait(250);
      const shown = [...document.querySelectorAll("#map-drawer .drawer-label")].map((label) => label.textContent);
      if (shown.join("|") !== "src-tauri/src/pr_briefs.rs|understanding/src/pr_brief.rs") log("snap:fail the step's drawer shows " + shown.join("|"));`, true),
  pr_diff: {
    __by: "parts",
    values: {
      "shell,understanding": { commit: "b".repeat(40), short: "bbbbbbb", label: "#334 PR briefs on the map", url: null, files: [prFile("src-tauri/src/pr_briefs.rs"), prFile("understanding/src/pr_brief.rs")] },
      undefined: { commit: "b".repeat(40), short: "bbbbbbb", label: "#334 PR briefs on the map", url: null, files: [prFile("src-tauri/src/pr_briefs.rs"), prFile("understanding/src/pr_brief.rs"), prFile("ui/map-brief.ts"), prFile("docs/studio/behaviour/pr-briefs.md")] },
    },
  },
});
// #492: the last step ends with Also touched in every group, by kind; a group's See the code asks for only its files, by path.
const alsoGroups: PrBrief["also_groups"] = [
  { kind: "no-part", files: [{ path: "loose/notes.rs", lines: 12 }], lines: 12 },
  { kind: "tests", files: [{ path: "scripts/snap/snap.test.mjs", lines: 6, ignored: true }, { path: "server/tests/wire.rs", lines: 40, part: "shell", laws: ["wire-types-agree"] }], lines: 46 },
  { kind: "config", files: [{ path: ".github/workflows/studio.yml", lines: 3, ignored: true }, { path: "Cargo.lock", lines: 18, ignored: true }], lines: 21 },
  { kind: "docs", files: [{ path: "docs/studio/behaviour/pr-briefs.md", lines: 9, ignored: true }], lines: 9 },
  { kind: "ignored", files: [{ path: "scripts/release.sh", lines: 2, ignored: true }], lines: 2 },
  { kind: "other", files: [{ path: "hook/src/main.rs", lines: 1, part: "hook" }], lines: 1 },
];
const mapBriefAlsoGroups = (): Fixture => ({
  ...mapBrief(briefOf(storySteps(), false, { also_groups: alsoGroups }), 3, `
      const groups = [...document.querySelectorAll("#map-brief .brief-also-group")];
      const names = groups.map((group) => group.querySelector(".brief-also-name").textContent + " " + group.querySelector(".brief-also-counts").textContent);
      if (names.join("|") !== "No part 1 file · 12 lines|Tests 2 files · 46 lines|Config/build 2 files · 21 lines|Docs 1 file · 9 lines|Ignored 1 file · 2 lines|Other 1 file · 1 line") log("snap:fail the groups show " + names.join("|"));
      if (groups[1].querySelectorAll(".brief-also-files li")[1]?.querySelector(".brief-also-law")?.textContent !== "@law: wire-types-agree") log("snap:fail the Tests group's tagged file does not name its law");
      if (groups[1].querySelectorAll(".brief-tag").length !== 1 || groups[4].querySelector(".brief-tag")) log("snap:fail the ignored tag shows outside tests, config/build and docs, or not in them");
      groups[1].querySelector(".brief-code").click();
      await wait(250);
      const shown = [...document.querySelectorAll("#map-drawer .drawer-label")].map((label) => label.textContent);
      if (shown.join("|") !== "scripts/snap/snap.test.mjs|server/tests/wire.rs") log("snap:fail the Tests group's drawer shows " + shown.join("|"));
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true }));
      await wait(250);
      if (document.querySelector("#map-drawer .drawer-label")?.checkVisibility()) log("snap:fail Esc did not close the drawer");`, true),
  pr_diff: {
    __by: "paths",
    values: {
      "scripts/snap/snap.test.mjs,server/tests/wire.rs": { commit: "b".repeat(40), short: "bbbbbbb", label: "#334 PR briefs on the map", url: null, files: [prFile("scripts/snap/snap.test.mjs"), prFile("server/tests/wire.rs")] },
    },
  },
});
// #493: a Could break step's copy icon, beside its See the code, puts a prompt for the agent on the clipboard with the changed files of
// the parts it lights, and the copy toast shows its first 3 lines; with no clipboard the toast holds the whole prompt to copy by hand.
const copySteps = (): BriefStep[] => [
  storySteps()[0],
  step("Could break", "A push could mark a read brief stale", "A push that lands while you read <b>marks the brief stale</b>, so the next open writes it again.", ["shell", "understanding"], {
    risks: [{ part: "shell", trigger: "A push lands while the brief is open.", check: "Push while reading and open the brief again." }],
    cites: [{ from: "DIFF", quote: "A new head marks it stale" }],
  }),
];
const mapBriefCopy = (clipboard: boolean): Fixture => ({
  ...mapBrief(briefOf(copySteps(), false), 1, `
      Object.defineProperty(navigator, "clipboard", { configurable: true, value: ${clipboard ? "{ writeText: async (words) => void (window.__copied = words) }" : "undefined"} });
      const copy = document.querySelector("#map-brief .brief-step-code .brief-copy");
      if (!copy) return log("snap:fail the Could break step has no copy icon beside its See the code");
      if (!copy.getAttribute("aria-label")) log("snap:fail the copy icon has no name");
      copy.click();
      await wait(150);
      const toast = document.getElementById("copy-toast");
      if (!toast.matches(":popover-open") || !document.getElementById("toasts").contains(toast)) return log("snap:fail the copy toast is not an open popover in the region");
      ${clipboard
        ? `if (!toast.textContent.includes("Prompt copied")) log("snap:fail the toast does not say the prompt was copied");
      if (!(window.__copied ?? "").includes("Changed files of the parts it lights:\\n- src-tauri/src/pr_briefs.rs\\n- understanding/src/pr_brief.rs")) log("snap:fail the prompt lacks the step's files: " + window.__copied);
      if (toast.querySelector(".copy-lines").textContent.split("\\n").length !== 3) log("snap:fail the toast does not show the prompt's first 3 lines");`
        : `const area = toast.querySelector("textarea");
      if (!area || !area.value.startsWith("In PR #334, the brief warns this could break.")) log("snap:fail the toast does not hold the prompt to copy by hand");
      if (!toast.textContent.includes("Copy this prompt")) log("snap:fail the toast does not say to copy it by hand");`}`, true),
  pr_diff: {
    __by: "parts",
    values: {
      "shell,understanding": { commit: "b".repeat(40), short: "bbbbbbb", label: "#334 PR briefs on the map", url: null, files: [prFile("src-tauri/src/pr_briefs.rs"), prFile("understanding/src/pr_brief.rs")] },
    },
  },
});
// #497: a "Since you read" story, which the tray opens first: its head line with the full brief one click away, its own Partial facts, the
// Could break copy and the not-confirmed popover. Full brief opens the whole PR's brief at the same head.
const sinceView = (): BriefView => {
  const story = briefOf([{ ...copySteps()[1], flags: whyFlags }, storySteps()[1]]);
  story.brief.brief.title = "The push since you read";
  story.brief.partial_facts = { claim_error: "the claim check timed out after 120 s", no_extractor: [] };
  return { ...story, since: "a".repeat(40) };
};
const mapBriefSince = (): Fixture => ({
  ...mapBrief(sinceView(), 0, `
      const line = panel.querySelector(".brief-since");
      if (!line) return log("snap:fail the story has no Since you read line");
      if (!line.textContent.startsWith("Since you read aaaaaaa: only what this push changed.")) log("snap:fail the since line reads " + JSON.stringify(line.textContent));
      if (line.compareDocumentPosition(panel.querySelector(".brief-facts")) !== Node.DOCUMENT_POSITION_FOLLOWING) log("snap:fail the since line is not first under the head");
      if (panel.querySelector(".brief-facts summary")?.textContent !== "Partial facts · 1") log("snap:fail the story's own Partial facts are not shown");
      if (!panel.querySelector(".brief-step-code .brief-copy")) log("snap:fail the story's Could break step has no copy icon");
      ${openWhy("#map-brief .brief-step")}
      why.hidePopover();
      const rows = [...document.querySelectorAll('#map-tray .tray-row[data-pr="334"]')].map((row) => row.querySelector(".tray-title").textContent);
      if (rows.join("|") !== "The push since you read|PR briefs on the map") log("snap:fail the tray does not offer the story first: " + rows.join("|"));
      const full = line.querySelector(".brief-since-full");
      if (!full || full.getAttribute("aria-label") !== "Full brief of PR #334") return log("snap:fail no named Full brief button");
      full.click();
      await wait(400);
      if (panel.querySelector(".brief-since")) return log("snap:fail Full brief still plays the story");
      if (!panel.querySelector(".brief-step")?.textContent.includes("A ready PR now brings its own brief to the map")) log("snap:fail Full brief did not open the full brief");
      panel.querySelector(".brief-leave").click();
      await wait(100);
      document.querySelector('#map-tray .tray-row[data-pr="334"]').click();
      await wait(400);
      if (!panel.querySelector(".brief-since")) log("snap:fail the tray's first row no longer opens the story");`, true),
  map_state: mapState({ fog: { shell: "thick", understanding: "thin" }, ready: [{ number: 334, title: "PR briefs on the map", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false, since: "The push since you read" } }] }),
  pr_brief: { __by: "full", values: { false: sinceView(), true: briefOf(storySteps()) } },
  pr_diff: {
    __by: "parts",
    values: {
      "shell,understanding": { commit: "b".repeat(40), short: "bbbbbbb", label: "#334 PR briefs on the map", url: null, files: [prFile("src-tauri/src/pr_briefs.rs"), prFile("understanding/src/pr_brief.rs")] },
    },
  },
});
const mapBriefCopied = (): Fixture => mapBriefCopy(true);
const mapBriefCopyByHand = (): Fixture => mapBriefCopy(false);
// Each figure kind filled to the limits `figureShapes` draws (#449): six states, five steps, four lines a side, each label long.
const fullFigure = (kind: string): BriefFigure => {
  const label = (i: number) => `a long label ${i + 1} here`;
  return kind === "states"
    ? { kind, frame: "a terminal with a long name", items: [0, 1, 2, 3, 4, 5].map((i) => ({ label: label(i), tone: i === 1 ? "lost" : i === 2 ? "live" : null, bad: i === 1 ? true : null })), before: [], after: [] }
    : kind === "sequence"
      ? { kind, frame: null, items: [0, 1, 2, 3, 4].map((i) => ({ label: label(i), tone: i === 2 ? "live" : null, bad: null })), before: [], after: [] }
      : { kind, frame: null, items: [], before: [0, 1, 2, 3].map(label), after: [4, 5, 6, 7].map(label) };
};
// Each figure kind's accessible name, from `fullFigure`'s items (#456).
const figureNames: Record<string, string> = {
  states: "States of a terminal with a long name: a long label 1 here, a long label 2 here, a long label 3 here, a long label 4 here, a long label 5 here, a long label 6 here",
  sequence: "Sequence: a long label 1 here, then a long label 2 here, then a long label 3 here, then a long label 4 here, then a long label 5 here",
  compare: "Before: a long label 1 here, a long label 2 here, a long label 3 here, a long label 4 here. After: a long label 5 here, a long label 6 here, a long label 7 here, a long label 8 here",
};
// The card's geometry, read in the page: the card, its scrolling step, the step's words and figure, and Back and Next.
const cardBox = `
      const box = (node) => node.getBoundingClientRect();
      const card = () => document.getElementById("map-brief");`;
// The figure sits under the step's words, inside the card, never in a column beside them (#449).
const mapBriefFigure = (kind: string): Fixture =>
  mapBrief(briefOf([step("Now", "What happens inside one part", "The change is inside Understanding, so a small figure shows it.", ["understanding"], { fig: fullFigure(kind) })]), 0, `${cardBox}
      if (document.querySelectorAll("#map-brief .brief-fig .fig-chip").length < 3) log("snap:fail the figure draws no chips");
      const words = box(document.querySelector("#map-brief .brief-txt"));
      const figure = box(document.querySelector("#map-brief .brief-figure"));
      const cols = document.querySelector("#map-brief .brief-cols");
      const inside = box(cols);
      if (figure.top < words.bottom - 1) log("snap:fail the figure is not under the step's words: " + figure.top + " above " + words.bottom);
      if (figure.left < inside.left - 1 || figure.right > inside.right + 1) log("snap:fail the figure overflows the card: " + figure.left + "-" + figure.right + " in " + inside.left + "-" + inside.right);
      if (cols.scrollWidth > cols.clientWidth) log("snap:fail the step scrolls sideways: " + cols.scrollWidth + " in " + cols.clientWidth);
      if (figure.width < 200) log("snap:fail the figure is cramped: " + figure.width);
      // #456: the figure is named by its kind and items.
      const named = __snapName(document.querySelector("#map-brief .brief-fig"));
      if (named !== ${JSON.stringify(figureNames[kind])}) log("snap:fail the figure is named " + JSON.stringify(named));`);
// Each step is as tall as its own words, the card keeps one narrow width, and Back and Next keep their place (#449).
const mapBriefHeights = (): Fixture =>
  mapBrief(briefOf(storySteps()), 0, `${cardBox}
      const seen = [];
      for (let i = 0; i < 4; i++) {
        if (i) { document.getElementById("brief-next").click(); await wait(50); }
        seen.push({ card: box(card()), back: box(document.querySelector("#map-brief .brief-back")), next: box(document.getElementById("brief-next")) });
      }
      const [where, before] = seen;
      if (!(before.card.height < where.card.height - 20)) log("snap:fail the short step's card is not shorter: " + before.card.height + " against " + where.card.height);
      for (const at of seen) {
        if (Math.abs(at.card.width - where.card.width) > 0.5) log("snap:fail the card's width changes: " + at.card.width + " against " + where.card.width);
        for (const key of ["back", "next"]) if (Math.abs(at[key].top - where[key].top) > 0.5 || Math.abs(at[key].left - where[key].left) > 0.5) log("snap:fail " + key + " moved: " + JSON.stringify(at[key]) + " against " + JSON.stringify(where[key]));
      }
      if (!(where.card.width < 820)) log("snap:fail the card is not narrow: " + where.card.width);
      if (card().style.minHeight) log("snap:fail the card's height is set in script: " + card().style.minHeight);`, true);
// A step taller than the card's maximum scrolls inside the card, with Back and Next outside the scroll and in view (#449).
const tallStep = (): BriefStep =>
  step("Now", "A step with a great deal to say", "Every line below is one more thing the step says, so the step is taller than the card may be.", ["understanding"], {
    list: Array.from({ length: 30 }, (_, i) => `Line ${i + 1} of what changed, long enough to read as a sentence.`),
  });
const mapBriefTall = (): Fixture =>
  mapBrief(briefOf([tallStep(), storySteps()[1]]), 0, `${cardBox}
      const cols = document.querySelector("#map-brief .brief-cols");
      const map = box(document.querySelector(".map-stage"));
      const next = document.getElementById("brief-next");
      const back = document.querySelector("#map-brief .brief-back");
      if (!(cols.scrollHeight > cols.clientHeight + 20)) log("snap:fail the tall step does not scroll inside the card: " + cols.scrollHeight + " in " + cols.clientHeight);
      if (getComputedStyle(cols).overscrollBehaviorY !== "contain") log("snap:fail the step's scroll chains to the map: " + getComputedStyle(cols).overscrollBehaviorY);
      if (box(card()).top < map.top) log("snap:fail the card runs above the map: " + box(card()).top + " above " + map.top);
      if (cols.contains(next) || cols.contains(back)) log("snap:fail Back and Next scroll with the step");
      for (const button of [back, next]) {
        const at = box(button);
        if (at.bottom > innerHeight || at.top < map.top || document.elementFromPoint(at.left + at.width / 2, at.top + at.height / 2) !== button) log("snap:fail " + button.textContent + " is not in view");
      }
      const place = box(next);
      cols.scrollTop = cols.scrollHeight;
      await wait(100);
      if (box(next).top !== place.top) log("snap:fail Next moved with the scroll");`);
// With motion on, a step's words come in after a short fade: the camera frames once they are in, so a taller step's card never covers the
// parts it lights (#443 review). The brief opens on a short step and Next goes to a tall one.
const motionOn = `
      window.matchMedia = (query) => (query.includes("prefers-reduced-motion") ? { ...media(query), matches: false } : media(query));`;
const mapBriefTaller = (): Fixture =>
  mapBrief(briefOf([storySteps()[1], { ...tallStep(), on: ["core", "daemon"] }]), 0, `${cardBox}${motionOn}
      document.getElementById("brief-next").click();
      await wait(400);
      const top = box(card()).top;
      const lit = [...document.querySelectorAll(".plate.lit")];
      if (!lit.length) log("snap:fail the tall step lights no part");
      for (const plate of lit) if (box(plate).bottom > top + 0.5) log("snap:fail the card covers " + plate.dataset.id + ": its foot at " + box(plate).bottom + ", the card's top at " + top);`, true);
// Next from a tall step scrolled to its end opens the next tall step at its top (#443 review).
const mapBriefTallScroll = (): Fixture =>
  mapBrief(briefOf([tallStep(), tallStep()]), 0, `
      const cols = document.querySelector("#map-brief .brief-cols");
      cols.scrollTop = cols.scrollHeight;
      await wait(50);
      if (!(cols.scrollTop > 0)) return log("snap:fail the first tall step did not scroll: " + cols.scrollTop);
      document.getElementById("brief-next").click();
      await wait(200);
      if (document.querySelectorAll(".brief-dots i")[1]?.classList.contains("at") !== true) return log("snap:fail Next did not go to the second step");
      if (cols.scrollTop !== 0) log("snap:fail the next step opens scrolled: " + cols.scrollTop);`, true);
const mapBriefUnconfirmed = (): Fixture =>
  mapBrief(briefOf([step("Now", "Cards are written once per turn", "A turn is written once, whatever the order of events.", ["understanding"], { flags: ["the claim is not in the facts"] })], true), 0, `
      if (!document.querySelector("#map-brief .brief-unconfirmed")?.textContent.includes("not confirmed from the facts")) log("snap:fail no unconfirmed mark");
      // A brief from before its partial facts were stored knows only that its diff was cut (#491).
      const chip = document.querySelector("#map-brief .brief-facts");
      if (chip?.querySelector("summary").textContent !== "Partial facts · 1") return log("snap:fail an older partial brief has no chip of one: " + chip?.textContent);
      if ([...chip.querySelectorAll(".brief-facts-list > li")].map((li) => li.textContent).join("|") !== "The diff was cut") log("snap:fail the older brief's line: " + chip.textContent);`);
// The Partial facts chip (#491): a brief written from a cut diff, without its claim check, over a language no extractor reads. Closed, it says
// how many inputs it lacks; `open` opens it, and a step on it stays open, since it belongs to the panel, not the step.
const partialView = (): BriefView => {
  const view = briefOf(storySteps(), true);
  view.brief.partial_facts = {
    cut: { files_cut: ["ui/map-tray.ts", "ui/map-brief.ts"], files_partial: ["server/src/pr_briefs.rs"], bytes_sent: 65_536, bytes_cut: 20_480 },
    claim_error: "the claim check timed out after 120 s",
    no_extractor: [".py"],
  };
  return view;
};
const mapBriefPartialFacts = (open: boolean): Fixture =>
  mapBrief(partialView(), 0, `
      const chip = document.querySelector("#map-brief .brief-facts");
      if (!chip) return log("snap:fail no Partial facts chip");
      if (chip.querySelector("summary").textContent !== "Partial facts · 3") log("snap:fail the chip's count: " + chip.querySelector("summary").textContent);
      if (document.getElementById("map-brief").textContent.includes("Written from part of the diff")) log("snap:fail the old partial line is still shown");
      if (chip.compareDocumentPosition(document.querySelector("#map-brief .brief-cols")) !== Node.DOCUMENT_POSITION_FOLLOWING) log("snap:fail the chip is not above the step");
      ${open ? `chip.querySelector("summary").click();
      await wait(50);
      const lines = [...chip.querySelectorAll(".brief-facts-list > li")];
      if (lines.length !== 3) return log("snap:fail the chip lists " + lines.length + " inputs, not 3");
      if (!lines[0].textContent.startsWith("The diff was cut: 64\\u00a0KiB sent, 20\\u00a0KiB not") || lines[0].querySelectorAll(".brief-facts-files li").length !== 3) log("snap:fail the cut line: " + lines[0].textContent);
      if (!lines[0].textContent.includes("server/src/pr_briefs.rs (in part)")) log("snap:fail the file sent in part is not marked");
      if (lines[1].textContent !== "The claim check failed: the claim check timed out after 120 s") log("snap:fail the claim line: " + lines[1].textContent);
      if (!lines[2].textContent.startsWith("No extractor reads .py files")) log("snap:fail the language line: " + lines[2].textContent);
      document.getElementById("brief-next").click();
      await wait(300);
      if (document.querySelector("#map-brief .brief-facts") !== chip || !chip.open) log("snap:fail a step on reset the chip");
      document.querySelector("#map-brief .brief-back").click();
      await wait(300);` : `if (chip.open) log("snap:fail the chip opens closed");`}`, true);
const mapBriefNoFacts = (): Fixture =>
  mapBrief(briefOf(storySteps()), 0, `if (document.querySelector("#map-brief .brief-facts")) log("snap:fail a chip shows with nothing missing");`, true);
// The stale-base line (#494): main moved since the PR's merge base on what the brief relies on. It sits under the panel's head, above
// the step, and names each kind once; past three names the rest are counted. With no `stale_base` there is no line.
const mapBriefStaleBase = (): Fixture =>
  mapBrief(
    {
      ...briefOf(storySteps()),
      stale_base: { laws: ["wire-types-agree"], links: ["server-window"], flows: [], paths: [], files: ["Cargo.toml", "server/src/map.rs", "server/src/pr_briefs.rs", "ui/types.ts"] },
    },
    0,
    `
      const line = document.querySelector("#map-brief .brief-stale");
      if (!line) return log("snap:fail no stale-base line");
      const want = "Main moved since this PR's base: law wire-types-agree · link server-window · files Cargo.toml, server/src/map.rs, server/src/pr_briefs.rs +1 more";
      if (line.textContent !== want) log("snap:fail the stale-base line reads " + JSON.stringify(line.textContent));
      if (line.compareDocumentPosition(document.querySelector("#map-brief .brief-cols")) !== Node.DOCUMENT_POSITION_FOLLOWING) log("snap:fail the line is not above the step");
      if ([...line.querySelectorAll(".brief-stale-names")].some((names) => names.getAttribute("translate") !== "no")) log("snap:fail a name would be translated");`,
    true,
  );
const mapBriefNoStaleBase = (): Fixture =>
  mapBrief(briefOf(storySteps()), 0, `if (document.querySelector("#map-brief .brief-stale")) log("snap:fail a stale-base line shows with nothing moved");`, true);
// The not-confirmed popover (#491): the mark on a step, or on a flagged decision in the ratify step, opens a popover under it listing its
// reasons without the mark's own words. \`esc\` checks that Esc closes only the popover, and that a step on closes it.
const whyFlags = ["claim check: no fact says a turn is written once", "claim check: the order of events is not in the diff", "not confirmed from the facts"];
const whyStep = () => step("Now", "Cards are written once per turn", "A turn is written once, whatever the order of events.", ["understanding"], { flags: whyFlags });
const openWhy = (scope: string) => `
      const mark = document.querySelector("${scope} .brief-why-mark");
      if (!mark) return log("snap:fail no not-confirmed mark in ${scope}");
      mark.click();
      await wait(100);
      const why = document.getElementById(mark.getAttribute("aria-controls"));
      if (!why?.matches(":popover-open") || mark.getAttribute("aria-expanded") !== "true") return log("snap:fail the mark did not open its popover");
      const reasons = [...why.querySelectorAll(".brief-why-list li")].map((li) => li.textContent);
      if (reasons.join("|") !== ${JSON.stringify(whyFlags.slice(0, 2).join("|"))}) log("snap:fail the popover's reasons: " + reasons.join("|"));
      const [m, w] = [mark.getBoundingClientRect(), why.getBoundingClientRect()];
      if (!(Math.abs(w.top - m.bottom) < 16 || Math.abs(w.bottom - m.top) < 16) || w.right < m.left || w.left > m.right) log("snap:fail the popover is not anchored to its mark: " + JSON.stringify([m, w]));`;
const mapBriefWhy = (esc: boolean): Fixture =>
  mapBrief(briefOf([whyStep(), storySteps()[1]]), 0, `${openWhy("#map-brief .brief-step")}
      ${esc ? `document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true }));
      await wait(100);
      if (why.matches(":popover-open")) return log("snap:fail Esc did not close the popover");
      if (document.getElementById("map-brief").hidden) return log("snap:fail Esc closed the brief with the popover");
      document.querySelector("#map-brief .brief-why-mark").click();
      await wait(100);
      if (!why.matches(":popover-open")) return log("snap:fail the mark did not open its popover again");
      document.getElementById("brief-next").click();
      await wait(300);
      if (document.querySelector(".brief-why:popover-open")) log("snap:fail a step on left the popover open");` : `await wait(800);
      if (!why.matches(":popover-open")) log("snap:fail the popover closed by itself");`}`, true);
const mapBriefWhyDecision = (): Fixture => ({
  ...mapBrief(briefOf(storySteps(), false, { decisions: [{ id: "d1", text: "A brief is stored per project, since PR numbers repeat across repositories.", flags: whyFlags }] }), 4, `${openWhy("#map-brief .ratify-row[data-id=d1]")}
      if (document.querySelectorAll("#map-brief .ratify-row .brief-why-mark").length !== 1) log("snap:fail only the flagged decision carries the mark");`, false, 100),
  pr_ratify: ratifyRows,
});
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
// The same taps from the keyboard (#460): real Enter on each part's open button.
const keyTap = (ids: string[]) =>
  ids.map((id) => `if (!document.querySelector('.plate[data-id="${id}"] .plate-open')) return log("snap:fail ${id} has no open button");
      document.querySelector('.plate[data-id="${id}"] .plate-open').focus(); await __snapKey("Enter"); await wait(50);`).join("\n      ");
const mapBriefOrder = (ids: string[], grade: boolean, check: string, keys = false): Fixture => ({
  ...mapBrief(flowBrief(), 1, `
      const next = document.getElementById("brief-next");
      if (next.textContent !== "Check" || !next.disabled) return log("snap:fail an order check starts with Check off");
      if (document.querySelectorAll(".plate.pick").length !== 4) log("snap:fail the parts the step lights cannot be tapped: " + document.querySelectorAll(".plate.pick").length);
      if (document.querySelectorAll("#map-brief .brief-slot").length !== 3) log("snap:fail the check has no three slots");
      ${(keys ? keyTap : tap)(ids)}
      if (document.querySelector(".plate.open")) log("snap:fail a tap opened the part's sheet");
      if (document.querySelectorAll(".plate .ord").length !== ${ids.length}) log("snap:fail the taps carry no numbers on the map");
      if (document.querySelectorAll("#map-brief .brief-slot.full").length !== ${ids.length}) log("snap:fail the slots do not hold the taps");
      ${grade ? `if (next.disabled) return log("snap:fail Check is off with every slot full");
      next.click();
      await wait(250);` : `if (!next.disabled) log("snap:fail Check is on before every slot is full");`}
      ${check}`, true),
  ...(keys && { realKeys: true }),
});
const orderTakeBack = `
      document.querySelector('.plate[data-id="shell"]').click();
      await wait(50);
      if (document.querySelectorAll(".plate .ord").length !== 1 || document.querySelector(".plate[data-id='core'] .ord")?.textContent !== "1") log("snap:fail a tap on a part did not take it back");
      document.querySelector('.plate[data-id="shell"]').click();
      await wait(50);`;
// The parts are tapped with real Enter on their open buttons (#460).
const mapBriefOrderSlots = (): Fixture => mapBriefOrder(["core", "shell"], false, orderTakeBack, true);
const mapBriefOrderWrong = (): Fixture =>
  mapBriefOrder(["understanding", "shell", "core"], true, `
      if (document.querySelector("#map-brief .brief-check .brief-body").textContent !== ${JSON.stringify(orderCheck.no)}) log("snap:fail the correction's words");
      const slots = [...document.querySelectorAll("#map-brief .brief-slot span:last-child")].map((n) => n.textContent);
      if (slots.join() !== "Core,Shell,Understanding") log("snap:fail the slots do not show the right order: " + slots);
      if (document.querySelectorAll(".map-light .light-trail").length !== 2) log("snap:fail the right path was not replayed: " + document.querySelectorAll(".map-light .light-trail").length);
      if (next.textContent !== "Next") log("snap:fail the way on after Check: " + next.textContent);`);
// Right, and on through the chapter: the brief plays end to end, its parts tapped with real Enter (#460).
const mapBriefOrderRight = (): Fixture =>
  mapBriefOrder(["core", "shell", "understanding"], true, `
      if (document.querySelector("#map-brief .brief-check .brief-body").textContent !== ${JSON.stringify(orderCheck.yes)}) log("snap:fail the right answer's words");
      next.click();
      await wait(300);
      if (!document.querySelector("#map-brief .brief-title").textContent.includes("A ready PR now brings")) log("snap:fail the chapter did not follow the overview");`, true);
// The brief's ratify step (#343): a changed law and a link, and the agent's two decisions, one already accepted.
const ratifyRows: RatifyRow[] = [
  { pr: 334, kind: "law", id: "window-quiet", text: "Only needs-you has colour.", change: "changed", status: "not-yet", detail: null },
  { pr: 334, kind: "link", id: "shell-understanding", text: "Shell to Understanding", change: "added", status: "not-yet", detail: null },
  { pr: 334, kind: "decision", id: "d1", text: "A brief is stored per project, since PR numbers repeat across repositories.", change: "", status: "not-yet", detail: null },
  { pr: 334, kind: "decision", id: "d2", text: "A merged PR's brief is read as stored, never rewritten.", change: "", status: "accepted", detail: null },
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
// The brief's reader explains itself (#524), with Explanations on (the default): the ? beside Leave, pressed; the step's title described by
// its kind's line, directly under it. On the Ratify step (`ratify`), the title's line, and "Not yet ratified"'s under the first undecided
// item only. `off` then presses the ?: the ? stays, keeps the focus and is no longer pressed, and no line is drawn or pointed at.
const mapBriefExplained = (at: "where" | "ratify" | "off"): Fixture => ({
  ...mapBrief(briefOf(storySteps()), at === "ratify" ? 4 : 0, `
      const box = document.getElementById("map-brief");
      const toggle = box.querySelector(".brief-header .explain-toggle");
      if (!toggle || toggle.nextElementSibling !== box.querySelector(".brief-header .brief-leave")) return log("snap:fail the ? is not beside Leave");
      if (toggle.getAttribute("aria-pressed") !== "true" || toggle.title !== "Hide explanations") log("snap:fail the ?: " + toggle.getAttribute("aria-pressed") + " " + toggle.title);
      const title = box.querySelector(".brief-title");
      const line = document.getElementById(title.getAttribute("aria-describedby"));
      if (!line?.classList.contains("explain") || title.nextElementSibling !== line) log("snap:fail the title's line");
      const want = ${JSON.stringify(at === "ratify" ? "Changes this PR made to your map; say whether each stands." : "Where this change sits on your map.")};
      if (line?.textContent !== want) log("snap:fail the title's line says " + JSON.stringify(line?.textContent));
      if (${at === "ratify"}) {
        const notes = [...box.querySelectorAll(".ratify-row .explain-link")].map((note) => note.closest(".ratify-row").dataset.id + "=" + note.textContent.split(":")[0]);
        if (notes.join("|") !== "window-quiet=Not yet ratified") log("snap:fail not yet ratified's line: " + notes.join("|"));
      }
      if (${at === "off"}) {
        toggle.focus();
        toggle.click();
        await wait(300);
        const kept = box.querySelector(".brief-header .explain-toggle");
        if (kept !== toggle || document.activeElement !== toggle) log("snap:fail the ? did not keep the focus");
        if (toggle.getAttribute("aria-pressed") !== "false" || toggle.title !== "Show explanations") log("snap:fail the ? after a press: " + toggle.getAttribute("aria-pressed") + " " + toggle.title);
        const lines = box.querySelectorAll(".explain").length;
        const described = box.querySelectorAll("[aria-describedby]").length;
        if (lines || described) log("snap:fail explanations drawn while off: " + lines + " lines, " + described + " described");
      }`, false, 100),
  ...(at === "ratify" ? { pr_ratify: ratifyRows } : {}),
  ...(at === "off" ? { set_explanations: '{ "explanations": false }' } : {}),
});
// Old beside new in the brief's ratify step (#485): a changed part (its purpose, a path, a held item reworded, one added, one removed), an
// added part with nothing before, and a link whose kind and reason changed.
const ratifyDetailRows: RatifyRow[] = [
  {
    pr: 334, kind: "part", id: "server", text: "Server", change: "changed", status: "not-yet",
    detail: {
      of: "part",
      name: { before: "Server", after: "Server" },
      purpose: { before: "Serves the page, keeps the state, installs the hook and runs the agents.", after: "Serves the page and keeps the state it shows." },
      holds: [
        { name: "Fog", text: { before: "Greys a part.", after: "Greys a part a PR changed until its brief is read." }, paths_added: ["server/src/fog.rs"], paths_removed: [] },
        { name: "Settings", text: { before: "Kept per user.", after: "Kept per user." }, paths_added: [], paths_removed: [] },
        { name: "Locks", text: { before: null, after: "One writer per file at a time." }, paths_added: [], paths_removed: [] },
        { name: "Install", text: { before: "Puts the hook in place.", after: null }, paths_added: [], paths_removed: [] },
      ],
      paths_added: [],
      paths_removed: ["server/src/install.rs"],
    },
  },
  {
    pr: 334, kind: "part", id: "install", text: "Install", change: "added", status: "not-yet",
    detail: {
      of: "part",
      name: { before: null, after: "Install" },
      purpose: { before: null, after: "Puts plotkeeper's hook into each agent's settings, and takes it out again." },
      holds: [{ name: "Hook copies", text: { before: null, after: "The hook file each agent runs." }, paths_added: ["server/src/hook_copies.rs"], paths_removed: [] }],
      paths_added: ["server/src/install.rs", "server/src/hook_copies.rs"],
      paths_removed: [],
    },
  },
  {
    pr: 334, kind: "link", id: "shell-server", text: "Shell to Server", change: "changed", status: "not-yet",
    detail: { of: "link", kind: { before: "import", after: "http" }, reason: { before: "", after: "The page calls the server over HTTP on 127.0.0.1." } },
  },
];
const mapBriefRatifyDetail = (): Fixture => ({
  ...mapBrief(briefOf(storySteps()), 4, `
      const rows = [...document.querySelectorAll("#map-brief .ratify-row")];
      if (rows.length !== 3) return log("snap:fail expected 3 ratify rows, saw " + rows.length);
      const [changed, added, link] = rows.map((row) => row.querySelector(".ratify-detail"));
      if (!changed || !added || !link) return log("snap:fail a part or link row has no detail");
      const terms = (detail) => [...detail.querySelectorAll("dt")].map((dt) => dt.textContent);
      if (terms(changed).join("|") !== "Purpose|Paths|Holds Fog|Fog: paths|Holds Locks|Holds Install") log("snap:fail the changed part's lines: " + terms(changed).join("|"));
      if (!changed.querySelector("del")?.textContent.includes("installs the hook") || !changed.querySelector("ins")?.textContent.includes("keeps the state")) log("snap:fail the purpose is not old beside new");
      if (added.querySelector("del")) log("snap:fail an added part has something before");
      if (terms(added)[0] !== "Name" || !added.textContent.includes("Hook copies")) log("snap:fail the added part does not show its name and held items");
      if (terms(link).join("|") !== "Kind|Reason" || link.querySelector("del").textContent !== "import" || !link.textContent.includes("none")) log("snap:fail the link's kind and reason: " + link.textContent);
      log("ratify detail ok");`, false, 100),
  pr_ratify: ratifyDetailRows,
});
// Link evidence in the brief's ratify step (#499): an added link's second line, one import site then "(+N imports across M files)"; a
// removed link with none. `line` shows the second line, `list` presses the count (every site in the drawer), `site` presses a site of the
// list (its code from the PR's head, numbered, its line marked).
const evidenceRows: RatifyRow[] = [
  {
    pr: 334, kind: "link", id: "shell-understanding", text: "Shell to Understanding", change: "added", status: "not-yet",
    detail: { of: "link", kind: { before: null, after: "import" }, reason: { before: null, after: "" } },
    evidence: {
      head: "b".repeat(40),
      sites: [
        { file: "ui/map-brief.ts", line: 17 },
        { file: "ui/map-brief.ts", line: 24 },
        { file: "ui/map-ratify.ts", line: 9 },
        { file: "ui/map-tray.ts", line: 12 },
      ],
    },
  },
  { pr: 334, kind: "link", id: "hook-shell", text: "Hook to Shell", change: "removed", status: "not-yet", detail: null },
];
const evidenceSnippet = {
  file: "ui/map-ratify.ts",
  line: 9,
  first: 5,
  lines: [
    "// too, then copies a prompt that quotes the item to the clipboard for him to paste into his agent (port decision Q2); when the clipboard",
    "// refuses, the prompt shows in a small box with a Copy button.",
    "",
    "import { failure, invoke, promptBox, sayAction, text } from \"./dom.js\";",
    "import { currentMap } from \"./map.js\";",
    "import { openDrawer } from \"./map-drawer.js\";",
    "import { copyOverturn, decideWith, detailLines, evidenceLine, noteFor } from \"./ratify.js\";",
    "import { logStop } from \"./stop-rules.js\";",
    "import type { ImportSite, RatifyRow } from \"./types.js\";",
  ],
};
const mapBriefRatifyEvidence = (at: "line" | "list" | "site"): Fixture => ({
  ...mapBrief(briefOf(storySteps()), 4, `
      const rows = [...document.querySelectorAll("#map-brief .ratify-row")];
      if (rows.length !== 2) return log("snap:fail expected 2 ratify rows, saw " + rows.length);
      const line = rows[0].querySelector(".ratify-evidence");
      if (!line) return log("snap:fail the added link has no second line");
      const site = line.querySelector(".ratify-site"), more = line.querySelector(".ratify-more");
      if (site?.textContent !== "ui/map-brief.ts:17" || more?.textContent !== "(+3 imports across 3 files)") log("snap:fail the second line says " + line.textContent);
      if (rows[1].querySelector(".ratify-evidence")) log("snap:fail a removed link shows evidence");
      if (${at !== "line"}) {
        more.click();
        await wait(300);
        const picks = [...document.querySelectorAll("#map-drawer .drawer-pick")].map((pick) => pick.textContent);
        if (picks.join("|") !== "ui/map-brief.ts:17|ui/map-brief.ts:24|ui/map-ratify.ts:9|ui/map-tray.ts:12") log("snap:fail the drawer lists " + picks.join("|"));
        if (document.querySelector("#map-drawer .drawer-title")?.textContent !== "Imports behind Shell to Understanding") log("snap:fail the list's title");
      }
      if (${at === "site"}) {
        document.querySelectorAll("#map-drawer .drawer-pick")[2].click();
        await wait(300);
        const marked = document.querySelector("#map-drawer mark.drawer-line");
        if (marked?.querySelector(".drawer-n")?.textContent !== "9") log("snap:fail the site's line is not marked: " + marked?.textContent);
        if (document.querySelector("#map-drawer .drawer-line .drawer-n")?.textContent !== "5") log("snap:fail the snippet is not numbered from its first line");
        if (document.querySelector("#map-drawer .drawer-title")?.textContent !== "ui/map-ratify.ts:9") log("snap:fail the snippet's title");
      }
      log("ratify evidence ok");`, false, 100),
  pr_ratify: evidenceRows,
  ratify_site: evidenceSnippet,
});
// Guard with a law (#401): a bug fix's brief, with nothing to ratify, ends with the guard's row: the on-trust laws of Core and Shell, the
// parts it touched, with Back this law, a new law of an earlier run being proven, and Add a new law. `confirm` presses Add a new law and
// shows the confirm step with the writer's draft.
const guardedBrief = (): BriefView => {
  const view = briefOf(storySteps());
  return { ...view, brief: { ...view.brief, guard: { parts: ["core", "shell"] } } };
};
const mapBriefGuard = (confirm: boolean): Fixture => ({
  ...mapBrief(guardedBrief(), 4, `
      const row = document.querySelector("#map-brief .guard-row");
      if (!row) return log("snap:fail the brief has no Guard with a law row");
      if (document.querySelector("#map-brief .brief-title").textContent !== "Guard with a law") log("snap:fail the step's heading: " + document.querySelector("#map-brief .brief-title").textContent);
      const laws = [...row.querySelectorAll(".guard-law")].map((li) => li.dataset.law).join(",");
      if (laws !== "core-order,shell-notice,shell-ordered") log("snap:fail the guard's laws: " + laws);
      if (row.querySelectorAll(".guard-law button").length !== 2) log("snap:fail Back this law belongs on the two laws on trust");
      if (!row.textContent.includes("Proving the patch")) log("snap:fail the new law's run does not say where it stands");
      const add = [...row.querySelectorAll("button")].find((b) => b.textContent === "Add a new law");
      if (!add) return log("snap:fail no Add a new law");
      if (${confirm}) {
        add.click();
        await wait(300);
        const sentence = document.querySelector("#map-brief .guard-sentence");
        if (!sentence || sentence.value !== "The shell applies events in the order they were made.") return log("snap:fail the drafted sentence is not shown to confirm: " + sentence?.value);
        if (document.querySelector("#map-brief .guard-id").value !== "shell-keeps-order") log("snap:fail the drafted id");
        if (document.querySelector("#map-brief .guard-part").value !== "shell") log("snap:fail the drafted part");
        if (![...document.querySelectorAll("#map-brief .guard-confirm button")].some((b) => b.textContent === "Start the run")) log("snap:fail no Start the run");
      }
      log("guard ok");`, false, 100),
  map_state: mapState({
    fog: { shell: "thick", understanding: "thin" },
    ready: [{ number: 334, title: "PR briefs on the map", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false } }],
    law_patches: [{ id: "19a2f-11", project: "/w/plotkeeper", law: "shell-ordered", state: "proving", line: null, at: runAt, diff: null, proof: null, text: "The shell reads the log in order.", guards: 334 }],
  }),
  snapDone: "guard ok",
});
// The end-of-story bar (#393, restyled by #448) under the story's last step: on a story with no gist it holds only Skip, with no rating
// and no note (#443). The S key skips it, closing it and sending nothing to the shell. The screenshot is the story before the skip.
const mapBriefBar = (): Fixture =>
  mapBrief(briefOf(storySteps()), 3, `
      const sent = [];
      const real = window.fetch;
      window.fetch = (url, init) => (sent.push(String(url) + " " + (init?.body ?? "")), real(url, init));
      const bar = () => document.querySelector("#map-brief .story-bar");
      if (!bar() || bar().hidden) return log("snap:fail no end-of-story bar on the story's last step");
      if (bar().querySelector(".bar-rate, .bar-note, .bar-scale")) log("snap:fail the bar still has a rating or a note");
      if (/clear|rate|note/i.test(bar().textContent)) log("snap:fail the bar still asks for a rating or a note: " + bar().textContent);
      if (bar().querySelector(".bar-explain")) log("snap:fail explain-back on a story with no gist");
      const skip = bar().querySelector(".bar-skip");
      if (skip?.textContent !== "Skip") log("snap:fail no Skip on the bar");
      const key = (target, k) => target.dispatchEvent(new KeyboardEvent("keydown", { key: k, code: /^[0-9]$/.test(k) ? "Digit" + k : "Key" + k.toUpperCase(), bubbles: true }));
      key(document, "4");
      await wait(50);
      if (bar().hidden) log("snap:fail a digit closed the bar");
      key(document, "s");
      await wait(50);
      if (!bar().hidden) log("snap:fail the S key did not skip the bar");
      const bars = sent.filter((call) => call.includes("/api/story_bar"));
      if (bars.length) log("snap:fail the bar sent " + bars.join());
      bar().hidden = false;`, false, 100);
// Explain-back (#397, restyled by #448): the same story, its chapter carrying a gist and lighting Core parts, ends with the one-line field
// in the bar's first row: autocomplete off, a placeholder that shows an example and ends with "…". Before submit, an "s" and a "2" typed
// in the line do not skip, the gist is not shown yet, and an Enter while an input method is composing does not submit. After submit
// (Enter), the line shows with the gist under it and the three marks as radios in a fieldset with a legend, each radio inside its label;
// a click on Partly's label checks it. Each is sent to the shell; no number shows. With the picked mark focused, S skips the bar.
const explainGist = "A ready PR's brief is written in the background and played on the map, and reading a chapter clears its parts' fog.";
const mapBriefExplainBack = (submit: boolean): Fixture =>
  mapBrief(briefOf(storySteps(), false, { chapters: [{ ...brief(storySteps()).chapters[0], gist: explainGist }] }), 3, `
      const sent = [];
      const real = window.fetch;
      window.fetch = (url, init) => (sent.push(String(url) + " " + (init?.body ?? "")), real(url, init));
      const bar = () => document.querySelector("#map-brief .story-bar");
      const line = bar()?.querySelector(".bar-slot .bar-explain .bar-line");
      if (!line) return log("snap:fail no explain-back line on a Core story with a gist");
      if (bar().firstElementChild.className !== "bar-slot") log("snap:fail explain-back is not the bar's first row");
      if (bar().querySelector(".bar-rate, .bar-note, .bar-scale")) log("snap:fail the bar still has a rating or a note");
      if (bar().querySelector(".bar-skip")?.textContent !== "Skip") log("snap:fail no Skip on the bar");
      if (line.getAttribute("autocomplete") !== "off") log("snap:fail the line's autocomplete is not off");
      if (!line.placeholder.endsWith("…") || !/e\\.g\\./.test(line.placeholder)) log("snap:fail the placeholder shows no example ending in …: " + line.placeholder);
      if (bar().textContent.includes(${JSON.stringify(explainGist)})) log("snap:fail the gist shows before the line is given");
      const key = (target, k, init = {}) => target.dispatchEvent(new KeyboardEvent("keydown", { key: k, code: /^[0-9]$/.test(k) ? "Digit" + k : "Key" + k.toUpperCase(), bubbles: true, cancelable: true, ...init }));
      line.focus();
      key(line, "s");
      key(line, "2");
      await wait(50);
      if (bar().hidden) return log("snap:fail an s typed in the line skipped the bar");
      line.value = "Briefs get written in the background and reading one clears the fog.";
      key(line, "Enter", { isComposing: true });
      await wait(50);
      if (bar().querySelector(".bar-said") || sent.some((call) => call.includes("/api/story_bar"))) log("snap:fail an Enter while composing submitted the line");
      ${submit ? `key(line, "Enter");
      await wait(50);
      const said = bar().querySelector(".bar-said");
      if (said?.textContent !== line.value) log("snap:fail the line is not shown after submit: " + said?.textContent);
      if (!bar().querySelector(".bar-gist")?.textContent.includes(${JSON.stringify(explainGist)})) log("snap:fail the gist is not shown under the line");
      const group = bar().querySelector("fieldset.bar-marks");
      if (!group?.querySelector(":scope > legend")?.textContent.trim()) log("snap:fail the marks are not a fieldset with a legend");
      const radios = [...bar().querySelectorAll('.bar-marks input[type="radio"]')];
      const labels = radios.map((r) => r.closest("label"));
      if (labels.map((l) => l?.textContent.trim()).join() !== "Got it,Partly,Missed") log("snap:fail the marks: " + labels.map((l) => l?.textContent.trim()).join());
      if (new Set(radios.map((r) => r.name)).size !== 1 || !radios[0]?.name) log("snap:fail the radios are not one group");
      labels[1]?.querySelector(".bar-mark-word")?.click();
      await wait(50);
      if (radios.filter((r) => r.checked).map((r) => r.value).join() !== "partly") log("snap:fail Partly is not checked by its label");
      if (/\\d/.test(bar().querySelector(".bar-slot").textContent)) log("snap:fail explain-back shows a number");
      const bars = sent.filter((call) => call.includes("/api/story_bar")).map((call) => JSON.parse(call.slice(call.indexOf(" ") + 1)));
      const kinds = bars.map((b) => b.kind + ":" + (b.line ?? b.mark)).join();
      if (kinds !== "explain-back-given:" + line.value + ",explain-back-marked:partly") log("snap:fail the bar sent " + kinds);
      // A picked mark is no field: S still skips the bar (#443 review). The screenshot is the bar before the skip.
      radios[1].focus();
      key(radios[1], "s");
      await wait(50);
      if (!bar().hidden) log("snap:fail the S key did not skip the bar with a mark picked");
      bar().hidden = false;` : `await wait(50);`}`, false, 100);
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
      if (!(panel.getBoundingClientRect().width < 820)) log("snap:fail the waiting card is not narrow: " + panel.getBoundingClientRect().width);
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
      if (panel.querySelector(".brief-code")) log("snap:fail a part catch-up offers a PR's code");
      // Its diff was cut (#491): its only input of the Partial facts chip.
      if (panel.querySelector(".brief-facts summary")?.textContent !== "Partial facts · 1") log("snap:fail a cut part catch-up has no chip of one");
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
    part_catch_up: { ...storedCatchUp(), partial: true },
  }),
  snapDone: "catch-up ok",
});
// A part catch-up over a long fog list (#396): its first step says it summarises the main shifts, with the full fog list behind it.
const mapCatchUpSummarising = (): Fixture => ({
  ...mapOpened(`
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      document.querySelector("#map .plate[data-id=understanding]").click();
      await wait(150);
      document.querySelector(".sheet .sheet-catch-up").click();
      await wait(350);
      const panel = document.getElementById("map-brief");
      if (panel.hidden || !panel.classList.contains("open")) return log("snap:fail the part catch-up did not open");
      if (!panel.querySelector(".brief-summarises")?.textContent.includes("summarises the main shifts of 9 fog entries")) return log("snap:fail no summarising line");
      const list = panel.querySelector(".brief-fog-list");
      list.open = true;
      if (list.querySelectorAll(".brief-cite").length !== 9) log("snap:fail the full fog list does not hold 9 entries");
      await wait(150);
      log("catch-up summarising ok");`, {
    map_state: mapState({ fog: { understanding: "thick" }, catch_ups: { understanding: { kind: "catch-up", ready: true, writing: false, failed: false } } }),
    part_catch_up: {
      ...storedCatchUp(),
      fog: [..."abcdefghi"].map((c) => c.repeat(40)),
      summary: {
        line: "This catch-up summarises the main shifts of 9 fog entries; the full list is below.",
        entries: [..."abcdefghi"].map((c, i) => (i % 3 === 2 ? { commit: c.repeat(40), pr: null, title: "fix pushed straight to main" } : { commit: c.repeat(40), pr: 380 + i, title: `Briefs: change number ${i + 1}` })),
      },
    },
  }),
  snapDone: "catch-up summarising ok",
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
      if (panel.hidden || !panel.textContent.includes("Writing…") || panel.querySelector("progress")?.getAttribute("aria-label") !== "Writing the part catch-up") return log("snap:fail the panel does not say the catch-up is being written");
      if (document.body.classList.contains("brief-open")) log("snap:fail the map is dimmed while it is written");
      log("catch-up writing ok");`, {
    map_state: mapState({ fog: { understanding: "thick" }, catch_ups: { understanding: { kind: "catch-up", ready: false, writing: true, failed: false } } }),
    part_catch_up: { writing: true },
  }),
  snapDone: "catch-up writing ok",
});
// While a part catch-up is written, the waiting card says which call runs (#450): the stage's label beside a named, indeterminate
// progress bar, both in a polite live region, at the story card's narrow width.
const mapCatchUpStage = (stage: CatchUpStage, label: string): Fixture => ({
  ...mapOpened(`
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; }";
      document.head.append(calm);
      document.querySelector("#map .plate[data-id=understanding]").click();
      await wait(300);
      document.querySelector(".sheet .sheet-catch-up").click();
      await wait(600);
      const panel = document.getElementById("map-brief");
      if (panel.hidden) return log("snap:fail the waiting card is not shown");
      const live = panel.querySelector('[aria-live="polite"]');
      if (!live) return log("snap:fail the waiting card has no polite live region");
      if (!live.textContent.includes(${JSON.stringify(label)})) log("snap:fail the stage reads: " + live.textContent);
      const bar = live.querySelector("progress");
      if (!bar) log("snap:fail no progress element in the live region");
      else {
        if (bar.hasAttribute("value")) log("snap:fail the progress is not indeterminate");
        if (!bar.getAttribute("aria-label") && !bar.labels?.length && !bar.getAttribute("aria-labelledby")) log("snap:fail the progress has no name");
      }
      const narrow = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--brief-width"));
      const width = panel.getBoundingClientRect().width;
      if (Math.abs(width - narrow) > 1) log("snap:fail the waiting card is not the story card's width: " + width + " against " + narrow);
      log("catch-up stage ok");`, {
    map_state: mapState({ fog: { understanding: "thick" }, catch_ups: { understanding: { kind: "catch-up", ready: false, writing: true, failed: false, stage } } }),
    part_catch_up: { writing: true },
  }),
  snapDone: "catch-up stage ok",
});
// A step's bold phrase (#446): the writer may wrap one phrase in <b>...</b>; the reader shows it as one bold element and never the tag.
const boldBody = "No process is recorded, so the session <b>goes idle</b> until its next event.";
const boldCheck = (done: string) => `
      const body = document.querySelector("#map-brief .brief-body");
      const bold = [...body.querySelectorAll("b")];
      if (body.textContent !== "No process is recorded, so the session goes idle until its next event.") log("snap:fail the step body reads: " + body.textContent);
      if (bold.length !== 1 || bold[0].textContent !== "goes idle") log("snap:fail bold elements: " + bold.map((b) => b.textContent).join("|"));
      if (body.children.length !== bold.length) log("snap:fail the step body holds another element");
      log("${done}");`;
const mapCatchUpBold = (): Fixture => ({
  ...mapOpened(`
      const calm = document.createElement("style");
      calm.textContent = "*, *::before, *::after { transition: none !important; animation: none !important; }";
      document.head.append(calm);
      document.querySelector("#map .plate[data-id=understanding]").click();
      await wait(150);
      document.querySelector(".sheet .sheet-catch-up").click();
      await wait(350);
      if (!document.querySelector("#map-brief .brief-kick")?.textContent.includes("Part catch-up")) return log("snap:fail the part catch-up did not open");
      ${boldCheck("catch-up bold ok")}`, {
    map_state: mapState({ fog: { understanding: "thick" }, catch_ups: { understanding: { kind: "catch-up", ready: true, writing: false, failed: false } } }),
    part_catch_up: { ...storedCatchUp(), brief: { ...storedCatchUp().brief, chapters: [{ behaviour: "story", parts: ["understanding"], title: "Idle sessions", steps: [step("Now", "Each line names its agent", boldBody, ["understanding"])], flags: [] }] } },
  }),
  snapDone: "catch-up bold ok",
});
const mapBriefBold = (): Fixture => ({
  ...mapBrief(briefOf([step("Now", "Each line names its agent", boldBody, ["shell"])]), 0, boldCheck("brief bold ok"), true),
  snapDone: "brief bold ok",
});
// An unpaired <b> and an <img onerror> string stay text: no element is made from them and nothing runs.
const mapBriefBoldUnpaired = (): Fixture => {
  const body = 'A lone <b>tag and <img src="x" onerror="window.__boldRan = 1"> stay text.';
  return {
    ...mapBrief(briefOf([step("Now", "Tags the writer did not pair", body, ["shell"])]), 0, `
      const node = document.querySelector("#map-brief .brief-body");
      if (node.textContent !== ${JSON.stringify(body)}) log("snap:fail the step body reads: " + node.textContent);
      if (node.children.length) log("snap:fail the step body made elements: " + [...node.children].map((c) => c.tagName).join("|"));
      if (window.__boldRan) log("snap:fail the onerror ran");
      log("brief unpaired ok");`, true),
    snapDone: "brief unpaired ok",
  };
};
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
// A plate's marks explain themselves (#495): the sheet opens with this case's reasons. Thick fog shows the newest thick entry with its PR
// and why, then "+N more"; thin fog an entry filed before #495, by its strength and commit; staleness the server's line with its date; a
// broken law the run's date, never the law.
const fogEntry = (commit: string, strength: "thin" | "thick", pr: number | null, why: string[]) => ({ commit: commit.repeat(40), strength, pr, why });
const mapMarks = (kind: "thick" | "thin" | "stale" | "broken"): Fixture => {
  const part = { thick: "understanding", thin: "shell", stale: "core", broken: "shell" }[kind];
  const user = mapState().user;
  const fog = (entries: ReturnType<typeof fogEntry>[]) => ({ first: null, processed: null, reads: {}, parts: { [part]: entries } });
  const states: Record<typeof kind, Partial<MapState>> = {
    thick: {
      fog: { understanding: "thick" },
      user: { ...user, fog: fog([fogEntry("1", "thick", 410, ["law brief-cites-facts", "link shell-understanding"]), fogEntry("2", "thin", 412, ["3 files"]), fogEntry("3", "thick", 418, ["paths", "2 files"])]) },
    },
    thin: { fog: { shell: "thin" }, user: { ...user, fog: fog([{ commit: "4".repeat(40), strength: "thin" } as ReturnType<typeof fogEntry>]) } },
    stale: { stale: { core: 2 }, stale_why: { core: "6 weeks or more: not engaged since tracking began on 2026-10-01" } },
    broken: { ci: { ...brokenCi, at: "2026-11-24T10:00:00Z" } },
  };
  const want = {
    thick: ["Thick fog from #418: paths, 2 files", "+2 more"],
    thin: ["Thin fog from commit 4444444"],
    stale: ["6 weeks or more: not engaged since tracking began on 2026-10-01"],
    broken: ["A check behind a law of this part failed in main's latest CI, 2026-11-24"],
  }[kind];
  return mapOpened(`
      document.querySelector("#map .plate[data-id=${part}]").click();
      await wait(300);
      const marks = document.querySelector("#map-sheet .sheet-marks");
      if (!marks) return log("snap:fail the sheet has no marks list");
      if (marks.previousElementSibling?.className !== "sheet-caption") log("snap:fail the marks are not at the sheet's top");
      const text = marks.textContent;
      for (const want of ${JSON.stringify(want)}) if (!text.includes(want)) log("snap:fail the marks lack " + want + ": " + text);
      if (text.includes("wire-types-agree")) log("snap:fail a broken law is named");
      log("marks ${kind} ok");`, { map_state: mapState(states[kind]) });
};
// The refresher (#392) on Shell's sheet, a stale part with no fog: drawn at once from the map file, the flows through it hop by hop (the
// sheet already shows the rest, its links as "Talks to", spec #473), with no writer and no "Writing…".
const mapRefresher = (): Fixture => {
  const file = JSON.parse(mapFile);
  file.flows = [{ id: "turn-writing", name: "Turn writing", hops: [
    { link: "hook-shell", text: "The hook's line reaches the Shell, which reads the log." },
    { link: "shell-understanding", text: "The Shell asks Understanding to write the turn's summary." },
    { link: "understanding-writer", text: "Understanding calls the writer." },
    { link: "shell-window", text: "The Shell tells the Window, which shows the summary." },
  ] }];
  file.parts.find((part: { id: string }) => part.id === "shell").purpose = "Reads the hook's log, feeds the core, writes the summaries through Understanding and serves the Window.";
  return mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const enter = document.querySelector("#map-sheet .sheet-refresher");
      if (!enter) return log("snap:fail no refresher button");
      enter.click();
      await wait(400);
      const node = document.querySelector("#map-sheet .refresher");
      if (!node) return log("snap:fail the refresher did not show");
      if (node.previousElementSibling?.classList.contains("sheet-links") !== true) log("snap:fail the refresher is not under Talks to");
      const words = node.textContent;
      for (const want of ["Flows through it", "Turn writing", "The Shell asks Understanding"]) {
        if (!words.includes(want)) log("snap:fail the refresher lacks " + want);
      }
      if (words.includes("Linked with") || node.querySelector(".refresher-links")) log("snap:fail the refresher still lists the links");
      if (words.includes("Understanding calls the writer.")) log("snap:fail a hop not at the part shows");
      if (document.querySelector("#map-sheet .sheet-refresher")) log("snap:fail the button stays once the refresher shows");
      if (document.body.textContent.includes("Writing")) log("snap:fail the refresher waits on a writer");
      log("refresher ok");`, {
    map_state: mapState({ source: JSON.stringify(file), stale: { shell: 2 } }),
  });
};
// The map with a pair of ends joined twice (spec #473): Shell imports Understanding, and Understanding answers Shell over HTTP, with a
// law on the run-time link. Each link has its kind, and the run-time ones their reasons.
const pairFile = (() => {
  const file = JSON.parse(mapFile);
  for (const link of file.links) link.kind = link.to === "writer" || link.to === "github" ? "http" : "import";
  file.links.find((link: { id: string }) => link.id === "shell-daemon").kind = "spawn";
  file.links.find((link: { id: string }) => link.id === "shell-daemon").reason = "The Shell starts the daemon that owns the terminals.";
  file.links.push({ id: "understanding-shell", from: "understanding", to: "shell", kind: "http", reason: "Understanding answers the Shell's summary requests." });
  file.laws.push({ id: "understanding-answers", on: "understanding-shell", backing: "trust", text: "Every request gets one answer." });
  return JSON.stringify(file);
})();
// A brief step that lights the second link of a pair (spec #473): the light runs along the pair's one line, from Understanding to Shell,
// though the line is drawn from Shell, the end of the pair's first link.
const mapBriefPairLight = (): Fixture => {
  const base = mapBrief(briefOf([step("Now", "Understanding answers the Shell", "The summary comes back over the link.", ["shell", "understanding"], { hops: [["understanding-shell", false]] })]), 0, `
      const line = document.querySelector('#map .map-links [data-link~="understanding-shell"]');
      if (!line || line.getAttribute("data-from") !== "shell") return log("snap:fail the pair's line: " + line?.outerHTML);
      const trails = [...document.querySelectorAll(".map-light .light-trail")];
      if (trails.length !== 1) return log("snap:fail the step lit " + trails.length + " trails");
      const at = (node, a, b) => node.getAttribute(a) + "," + node.getAttribute(b);
      if (at(trails[0], "x1", "y1") !== at(line, "x2", "y2") || at(trails[0], "x2", "y2") !== at(line, "x1", "y1")) log("snap:fail the light runs the wrong way along the pair");
      if (!document.querySelector('.plate[data-id="shell"].lit') || !document.querySelector('.plate[data-id="understanding"].lit')) log("snap:fail the pair's ends are not lit");
      log("pair light ok");`, true);
  return { ...base, map_state: { ...(base.map_state as MapState), source: pairFile }, snapDone: "pair light ok" };
};
// "Talks to" on Shell's sheet (spec #473): every link at either end, with its arrow, other end, kind and reason, and no other list of
// links; the unratified run-time link carries Accept and Overturn on its row. On the map the Shell and Understanding pair is one line.
const mapTalksTo = (): Fixture =>
  mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const sheet = document.getElementById("map-sheet");
      const headings = [...sheet.querySelectorAll(".sheet-heading")].map((h) => h.textContent);
      if (!headings.includes("Talks to") || headings.includes("Links") || headings.includes("Linked with")) log("snap:fail headings: " + headings.join("|"));
      if (sheet.querySelectorAll("ul.sheet-links").length !== 1) log("snap:fail not one list of links");
      const rows = [...sheet.querySelectorAll(".sheet-links .sheet-link")];
      if (rows.length !== 7) return log("snap:fail Talks to rows: " + rows.length);
      const row = (id) => rows.find((r) => r.dataset.link === id);
      const back = row("understanding-shell"), out = row("shell-understanding"), spawn = row("shell-daemon");
      if (back.querySelector(".sheet-link-dir").getAttribute("aria-label") !== "from" || out.querySelector(".sheet-link-dir").getAttribute("aria-label") !== "to") log("snap:fail the arrows' directions");
      if (back.querySelector(".sheet-link-text").textContent !== "Understanding" || back.querySelector(".sheet-link-kind").textContent !== "http") log("snap:fail the run-time row: " + back.textContent);
      if (!back.querySelector(".sheet-link-reason")?.textContent.includes("summary requests")) log("snap:fail the run-time row lacks its reason");
      if (out.querySelector(".sheet-link-kind").textContent !== "import" || out.querySelector(".sheet-link-reason")) log("snap:fail the import row: " + out.textContent);
      if (!spawn.textContent.includes("spawn") || !spawn.textContent.includes("starts the daemon")) log("snap:fail the spawn row: " + spawn.textContent);
      if (!back.classList.contains("unratified") || !back.querySelector(".ratify-accept") || !back.querySelector(".ratify-overturn")) log("snap:fail the unratified row has no Accept and Overturn");
      if (sheet.querySelectorAll(".sheet-link.unratified").length !== 1) log("snap:fail a ratified link shows as unratified");
      const lines = [...document.querySelectorAll("#map .map-links line.link")];
      if (lines.length !== 10) log("snap:fail one line per pair: " + lines.length);
      const pair = document.querySelector('#map .map-links [data-link~="understanding-shell"]');
      if (!pair || pair !== document.querySelector('#map .map-links [data-link~="shell-understanding"]')) log("snap:fail the pair is not one line");
      if (!document.querySelector("#map .map-links .link-mark.trust")) log("snap:fail the law on the pair's run-time link has no mark");
      log("talks to ok");`, {
    map_state: mapState({ source: pairFile, ratify: [{ pr: 334, kind: "link", id: "understanding-shell" }] }),
    snapDone: "talks to ok",
  });
// The part sheet explains itself (#522): Shell's sheet, with links of three kinds and two not yet ratified, with Explanations on (the
// default) and off. On, each heading and the selected tier are described by their line, a kind's line follows its first link only and
// "Not yet ratified"'s the first unratified link only, and the ? is pressed; off, no line is drawn and nothing points at one.
const mapSheetExplained = (on: boolean): Fixture =>
  mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const sheet = document.getElementById("map-sheet");
      const toggle = sheet.querySelector(".sheet-header .explain-toggle");
      if (!toggle || toggle.nextElementSibling !== sheet.querySelector(".sheet-header .sheet-close")) return log("snap:fail the ? is not beside the ×");
      if (toggle.getAttribute("aria-pressed") !== "${on}" || toggle.title !== "${on ? "Hide" : "Show"} explanations") log("snap:fail the ?: " + toggle.getAttribute("aria-pressed") + " " + toggle.title);
      const described = [...sheet.querySelectorAll("[aria-describedby]")];
      const lines = [...sheet.querySelectorAll(".explain")];
      if (${!on}) {
        if (lines.length || described.length) log("snap:fail explanations drawn while off: " + lines.length + " lines, " + described.length + " described");
        return log("explained ok");
      }
      for (const heading of sheet.querySelectorAll(".sheet-heading")) {
        const line = document.getElementById(heading.getAttribute("aria-describedby"));
        if (!line?.classList.contains("explain") || heading.nextElementSibling !== line) log("snap:fail the heading's line: " + heading.textContent);
      }
      const tier = sheet.querySelector('.sheet-tiers [aria-pressed="true"]');
      if (document.getElementById(tier.getAttribute("aria-describedby"))?.textContent.startsWith(tier.textContent + ":") !== true) log("snap:fail the selected tier's meaning");
      const notes = [...sheet.querySelectorAll(".sheet-link .explain-link")].map((note) => note.closest(".sheet-link").dataset.link + "=" + note.textContent.split(":")[0]);
      const rows = [...sheet.querySelectorAll(".sheet-links .sheet-link")];
      const first = (test) => rows.find(test).dataset.link;
      const want = [...new Set(rows.map((row) => row.querySelector(".sheet-link-kind")?.textContent))].map((kind) => first((row) => row.querySelector(".sheet-link-kind")?.textContent === kind) + "=" + kind);
      want.push(first((row) => row.classList.contains("unratified")) + "=Not yet ratified");
      if (notes.sort().join("|") !== want.sort().join("|")) log("snap:fail the links' lines: " + notes.join("|") + " want " + want.join("|"));
      log("explained ok");`, {
    map_state: mapState({ source: pairFile, ratify: [{ pr: 334, kind: "link", id: "understanding-shell" }, { pr: 334, kind: "link", id: "shell-daemon" }] }),
    ...(on ? {} : { settings_files: { settings: '{ "explanations": false }', keymap: null } }),
    snapDone: "explained ok",
  });
// A micro-world (#399) in its overlay: from the refresher on Shell's sheet, the Turn writing flow's world is ready but may be out of date, so its
// row offers Open and Rebuild; Open shows the checked page in a frame sandboxed to scripts alone, in the glass overlay the size of the settings dialog. The page's script is
// an inline handler: a closing script tag would end the snap page's own.
const worldPage = `<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:">
<title>Turn writing</title><style>:root{color-scheme:light dark}body{margin:0;padding:24px;font:15px system-ui;background:#f6f5f2;color:#1a1a1a}
@media (prefers-color-scheme: dark){body{background:#17181b;color:#e8e6e1}}.hops{display:flex;gap:10px;flex-wrap:wrap;margin:16px 0}
.hop{padding:8px 12px;border-radius:8px;border:1px solid #8884}.hop.on{background:#4a7dff;color:#fff;border-color:#4a7dff}</style>
<h1>Turn writing, as a model</h1><p>An illustrative model of how a turn's summary is written. Step through the hops.</p>
<div class="hops"><span class="hop on">Hook</span><span class="hop">Shell</span><span class="hop">Understanding</span><span class="hop">Writer</span><span class="hop">Window</span></div>
<button onclick="const hops=[...document.querySelectorAll('.hop')];const at=hops.findIndex((h)=>h.classList.contains('on'));hops[at].classList.remove('on');hops[(at+1)%hops.length].classList.add('on')">Next hop</button>
<p>The hook's line reaches the Shell, which reads the log.</p>`;
const mapMicroWorld = (): Fixture => {
  const file = JSON.parse(mapFile);
  file.flows = [{ id: "turn-writing", name: "Turn writing", hops: [
    { link: "hook-shell", text: "The hook's line reaches the Shell, which reads the log." },
    { link: "shell-window", text: "The Shell tells the Window, which shows the summary." },
  ] }];
  return mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      document.querySelector("#map-sheet .sheet-refresher")?.click();
      await wait(400);
      const row = document.querySelector('#map-sheet .micro-world-row[data-world="flow:turn-writing"]');
      if (!row) return log("snap:fail the flow has no micro-world row");
      const words = [...row.querySelectorAll("button")].map((button) => button.textContent).join(",");
      if (words !== "Open,Rebuild") log("snap:fail the row offers " + words);
      if (!row.textContent.includes("may be out of date")) log("snap:fail the row does not say it may be out of date");
      row.querySelector("button").click();
      await wait(600);
      const box = document.getElementById("map-micro-world");
      if (box.hidden) return log("snap:fail the micro-world did not open");
      const frame = box.querySelector("iframe");
      if (!frame) return log("snap:fail no frame");
      if (frame.getAttribute("sandbox") !== "allow-scripts") log("snap:fail the frame's sandbox: " + frame.getAttribute("sandbox"));
      if (frame.hasAttribute("src") || !frame.getAttribute("srcdoc")?.includes("Content-Security-Policy")) log("snap:fail the frame is not the checked page by srcdoc");
      if (!box.querySelector(".micro-world-title")?.textContent.includes("Turn writing")) log("snap:fail the title: " + box.textContent);
      if (box.getBoundingClientRect().width < 600) log("snap:fail the overlay is not wide: " + box.getBoundingClientRect().width);
      log("micro-world ok");`, {
    map_state: mapState({
      source: JSON.stringify(file),
      stale: { shell: 2 },
      micro_worlds: { on: true, worlds: { "flow:turn-writing": { phase: "ready", page: true, stale: true, line: null, prompt: null } } },
    }),
    micro_world_page: worldPage,
    snapDone: "micro-world ok",
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
// Pre-flight (#394) was run twice and three explain-back lines (#397) were written, so the review lists only the modules of project 2.
const dueFacts = { active_days: 20, merged_prs: 25, engaged_days: 1, scrubs: 0, proofs: 0, knows: 0, dismissed: [], briefs_offered: 0, briefs_opened: 0, chapters_done: 0, counts: { "pre-flight-run": 2, "catch-up-done": 3, "explain-back-given": 3, "micro-world-opened": 2, "back-law-started": 1 }, began: "2026-10-01", stale_since: null };
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

// The map from the keyboard (#460), at the busy fixture: each plate's open button, named by the part's name and caption, sits beside
// its chips in reading order; real Tab reaches a part and then its chips, never an actor; real Enter and Space open its sheet.
const mapPartKeys = (): Fixture => {
  const f = mapBusy(false);
  return {
    ...f,
    snapScript: f.snapScript!.replace(/log\("chips ok, not placed " \+ rest\.length\);/, `
      const press = async (key) => { await __snapKey(key); await wait(80); };
      const sheetPart = () => document.querySelector("#map-sheet .sheet")?.dataset.part;
      const plates = [...document.querySelectorAll("#map .plate[data-id]")];
      for (const plate of plates) {
        const open = plate.querySelector(":scope > button.plate-open");
        const words = [".plate-name", ".plate-caption"].map((at) => plate.querySelector(at)?.textContent ?? "").filter(Boolean);
        if (!open) { log("snap:fail " + plate.dataset.id + " has no open button"); continue; }
        if (plate.hasAttribute("role") || plate.hasAttribute("tabindex")) log("snap:fail the plate " + plate.dataset.id + " takes a role or a tabindex");
        if (open.parentElement !== plate.querySelector(".plate-chips")?.parentElement) log("snap:fail " + plate.dataset.id + "'s open button is not a sibling of its chips");
        if (open.contains(plate.querySelector(".agent-chip"))) log("snap:fail " + plate.dataset.id + "'s open button holds a chip");
        if (!words.length || words.some((word) => !open.textContent.includes(word))) log("snap:fail " + plate.dataset.id + "'s open button is not named " + words.join(", "));
      }
      // Reading order: each next button is a row lower (half a plate or more), or on the same row and to the right.
      const boxes = [...document.querySelectorAll("#map .plate-open")].map((b) => ({ id: b.closest(".plate").dataset.id, box: b.closest(".plate").getBoundingClientRect() }));
      boxes.slice(1).forEach(({ id, box }, i) => {
        const before = boxes[i].box;
        if (!(box.top - before.top >= box.height / 2 || (Math.abs(box.top - before.top) < box.height / 2 && box.left > before.left))) log("snap:fail " + id + " comes after " + boxes[i].id + " out of reading order");
      });
      // Real Tab from the first part's button walks the map: every part's button, then its chips, and no actor.
      const first = document.querySelector("#map .plate-open");
      if (!first) return log("snap:fail the map has no open button");
      first.focus();
      const walked = [first];
      while (walked.length < 40) {
        await press("Tab");
        if (!document.activeElement?.closest("#map .map-world")) break;
        walked.push(document.activeElement);
      }
      const expected = [...document.querySelectorAll("#map .plate-open, #map .plate .agent-chip")];
      const named = (node) => node.closest(".plate, .actor")?.dataset.id + (node.matches(".agent-chip") ? " chip" : "");
      if (walked.map(named).join() !== expected.map(named).join()) log("snap:fail Tab walked " + walked.map(named).join() + ", not " + expected.map(named).join());
      if (walked.some((node) => node.closest(".actor"))) log("snap:fail Tab reached an actor");
      const core = document.querySelector("#map .plate[data-id=core] .plate-open");
      const at = walked.indexOf(core);
      if (at < 0 || !walked[at + 1]?.matches("#map .plate[data-id=core] .agent-chip")) log("snap:fail Tab did not reach core's button and then its chip");
      // Real Enter, then real Space, on the focused button open the part's sheet, and the focus ring shows.
      core.focus();
      await press("Tab");
      await press("Shift+Tab");
      if (document.activeElement !== core || !core.matches(":focus-visible") || getComputedStyle(core).outlineStyle === "none") log("snap:fail the focused part shows no focus ring");
      await press("Enter");
      if (sheetPart() !== "core") log("snap:fail Enter did not open core's sheet: " + sheetPart());
      await press("Escape");
      if (sheetPart()) log("snap:fail Esc left the sheet open");
      const hook = document.querySelector("#map .plate[data-id=hook] .plate-open");
      hook.focus();
      await press("Space");
      if (sheetPart() !== "hook") log("snap:fail Space did not open hook's sheet: " + sheetPart());
      hook.focus();
      log("part keys ok");`),
    realKeys: true,
    snapDone: "part keys ok",
  };
};

// A map redraw, here at a landed push that thins Core's fog, keeps the keyboard on the same part's button (#452's review).
const mapPartRedraw = (): Fixture => {
  const f = mapBusy(false);
  return {
    ...f,
    map_state: { __by: "fetch", values: { true: mapState(), false: mapState({ fog: { core: "thin" } }) } } as never,
    snapScript: f.snapScript!.replace(/log\("chips ok, not placed " \+ rest\.length\);/, `
      const hook = document.querySelector("#map .plate[data-id=hook] > .plate-open");
      hook.focus();
      __snapEmit("map-landed", "/w/plotkeeper");
      await wait(300);
      const redrawn = document.activeElement;
      if (hook.isConnected) log("snap:fail the landed push did not draw the map again");
      if (!redrawn?.matches("#map .plate[data-id=hook] > .plate-open")) log("snap:fail after a map redraw focus is on " + (redrawn?.className || redrawn?.tagName) + ", not hook's button");
      log("part redraw ok");`),
    snapDone: "part redraw ok",
  };
};

// A map whose default branch's CI failed in a test that backs a law of the shell (#312): the red mark beside the Needs you chip, at the
// size of the busy fixture; `drawer` opens the proof of a law, `log` the broken-law drill-down.
const brokenCi: Ci = {
  status: "broken", run_url: "https://github.com/danielrispler/plotkeeper/actions/runs/9", commit: "35745b1693c5d061e5aea891e98a58515942a12d", earlier: false,
  parts: ["shell"], laws: ["wire-types-agree"], note: null, at: null,
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
      // A law on trust has Back a law instead (#398), never See the proof.
      if ([...document.querySelectorAll("#map-sheet .sheet-law button")].filter((b) => b.textContent === "See the proof").length !== 1) log("snap:fail See the proof is on a law no check backs");
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
      const probe = document.body.appendChild(document.createElement("span"));
      probe.style.color = "var(--faint)";
      const faint = getComputedStyle(probe).color;
      probe.remove();
      if (clear() && getComputedStyle(clear()).color !== faint) log("snap:fail Clear landed is not faint, " + faint + ": " + getComputedStyle(clear()).color);`,
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
// The writer section's one status region (#452's review): before an action, the region and a watch on the section that fails when the
// region leaves it; after, the same region, still in the section, holding `words`.
const writerStatusBefore = `
      const section = document.getElementById("writers");
      const live = [...section.children].filter((node) => node.getAttribute("role") === "status");
      if (live.length !== 1 || live[0].textContent !== "") log("snap:fail the writer section has " + live.length + " status regions before the action, not one empty: " + live.map((node) => node.textContent));
      let left = false;
      new MutationObserver((changes) => (left ||= changes.some((change) => [...change.removedNodes].includes(live[0])))).observe(section, { childList: true });`;
const writerStatusAfter = (words: string) => `
      const now = [...section.children].filter((node) => node.getAttribute("role") === "status");
      if (now.length !== 1 || now[0] !== live[0] || left) log("snap:fail the status region did not stay in the section across the redraw");
      if (!now[0]?.textContent.includes(${JSON.stringify(words)})) log("snap:fail the status region says " + JSON.stringify(now[0]?.textContent));
      if (section.querySelectorAll("[role=status], [aria-live]").length !== 1) log("snap:fail another line of the section is live");`;

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
// The README hero screenshot: plotkeeper's current parts and actors (with a compact subset of links and laws for legibility at 1400x800).
const readmeHeroMapFile = JSON.stringify({
  version: 1,
  parts: [
    { id: "plugins", name: "Agent plugins", caption: "Which hooks each agent runs", description: "", tier: "interface", paths: ["plugin/"] },
    { id: "hook", name: "Hook", caption: "Records every agent event", description: "", tier: "core", paths: ["hook/"] },
    { id: "core", name: "Core", caption: "Turns events into session state", description: "", tier: "core", paths: ["core/"] },
    { id: "server", name: "Server", caption: "Holds the state, answers the commands", description: "", tier: "core", paths: ["server/"] },
    { id: "understanding", name: "Understanding", caption: "Writes what you read, and keeps it", description: "", tier: "core", paths: ["understanding/"] },
    { id: "window", name: "Window", caption: "Everything you see and press", description: "", tier: "core", paths: ["ui/"] },
    { id: "workbench", name: "Workbench", caption: "Worktrees and PRs", description: "", tier: "black-box", paths: ["workbench/"] },
  ],
  actors: [{ id: "agents", name: "Agents" }, { id: "writer", name: "Writer" }, { id: "github", name: "GitHub" }],
  links: [
    { id: "agents-plugins", from: "agents", to: "plugins" }, { id: "plugins-hook", from: "plugins", to: "hook" },
    { id: "hook-server", from: "hook", to: "server" }, { id: "core-server", from: "core", to: "server" },
    { id: "server-understanding", from: "server", to: "understanding" }, { id: "understanding-writer", from: "understanding", to: "writer" },
    { id: "server-window", from: "server", to: "window" }, { id: "server-workbench", from: "server", to: "workbench" },
    { id: "workbench-github", from: "workbench", to: "github" },
  ],
  ignored: [],
  laws: [
    { id: "core-pure", on: "core", backing: "trust", text: "The core does no I/O except reading transcripts." },
    { id: "hook-fast", on: "hook", backing: "trust", text: "The hook never makes the agent wait, except for your answer." },
    { id: "server-projects", on: "server", backing: "check", text: "A saved project stays until Remove project." },
    { id: "server-install", on: "server", backing: "check", text: "Install never opens Claude Code's settings file." },
    { id: "understanding-kept", on: "understanding", backing: "check", text: "A stored file of an older format still loads." },
    { id: "understanding-order", on: "understanding", backing: "check", text: "A session's pieces are written one at a time, in order." },
    { id: "understanding-no-tools", on: "understanding", backing: "check", text: "The writer runs with no tools." },
    { id: "window-quiet", on: "window", backing: "trust", text: "Loud colour is only for what needs you and for a broken law." },
  ],
});
const readmeHero = (): Fixture =>
  mapBaseOf(`
      if (!document.querySelector("#map .map-canvas")) return log("snap:fail the page did not open on a map");
      frame("on the readme hero");
      checkColours("on the readme hero");
      log("base ok");`, {
    map_state: mapState({
      source: readmeHeroMapFile,
      fog: { understanding: "thin" },
      user: {
        tiers: {},
        positions: {
          core: at(355, 195),
          understanding: at(770, 265),
          writer: at(1025, 265),
          agents: at(65, 435),
          plugins: at(255, 435),
          hook: at(565, 435),
          server: at(885, 435),
          github: at(145, 605),
          workbench: at(385, 605),
          window: at(820, 605),
        },
      },
    }),
    map_place: { s1: ["core", "window"], "s1/w1": ["server"], s2: ["hook"], s6: [] },
  });
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
      // The teaching sentence (#526) sits under the title, whatever the Explanations switch says.
      if (fresh.querySelector(".map-empty-title + .empty")?.textContent !== "plotkeeper follows each agent session and maps what it changes.") log("snap:fail no teaching sentence under the fresh line: " + fresh.textContent);
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
      // The teaching sentence (#526) follows the "no system-map.json" line, and the draft prompt stays.
      const lines = [...document.querySelectorAll("#map .map-empty .empty")].map((p) => p.textContent);
      const at = lines.findIndex((line) => line.includes("has no system-map.json"));
      if (at < 0 || lines[at + 1] !== "A map names your parts and their rules; changes light them up.") log("snap:fail no teaching sentence after the no-map line: " + JSON.stringify(lines));
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
// The rail with no session picked (#526): the story's session is picked no more while its panel shows, as when it is deleted elsewhere. The
// rail's empty screen says what will show there, under "Pick a session in the list."; the panel closes with the selection (`followSelection`),
// so since #385 that screen is not drawn and this checks its words, not its pixels.
const mapPanelNoSession = (): Fixture =>
  mapPanel(`
      await press(row("s2"));
      if (!panelOpen() || !document.querySelector("#panel-body > .rail")) return log("snap:fail a row's press did not open its story in the left panel");
      __snapEmit("selected", null);
      await wait(300);
      const empty = document.getElementById("empty");
      if (empty.hidden || !document.getElementById("view").hidden) return log("snap:fail the rail does not show its empty screen with no session picked");
      const lines = [...empty.querySelectorAll("p.empty")].map((p) => p.textContent);
      if (JSON.stringify(lines) !== JSON.stringify(["Pick a session in the list.", "Its story shows here: what it is doing, and each finished turn."])) log("snap:fail the rail's empty screen says " + JSON.stringify(lines));
      if (!document.getElementById("view").hidden) log("snap:fail the story shows with no session picked");
      if (panelOpen()) log("snap:fail the story's panel stays open with no session picked");
      log("panel ok");`);
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

// The part sheet in the map (#445). The shell holds eight items, the most a part may, so its sheet is long; its check-backed law opens
// a long proof in the drawer.
const longSheet = (() => {
  const doc = JSON.parse(mapFile);
  const shell = doc.parts.find((p: { id: string }) => p.id === "shell");
  shell.purpose = "Reads the event log, feeds each line to the core and acts on what the core says, then serves the page and answers its commands.";
  shell.holds = Array.from({ length: 8 }, (_, i) => ({
    name: `Held item ${i + 1}`,
    text: "One sentence on a thing the part holds, long enough to wrap over two lines of the sheet at its first width of 360 px.",
    paths: [`src-tauri/src/item_${i + 1}.rs`],
  }));
  return JSON.stringify(doc);
})();
// The part sheet with its purpose and held items (#483): the purpose first, then each held item's name, sentence and key files; an item
// without paths shows none. The plate's hover text is the purpose, never the held items.
const heldSheet = (() => {
  const doc = JSON.parse(mapFile);
  const shell = doc.parts.find((p: { id: string }) => p.id === "shell");
  shell.purpose = "The program that holds the state, follows the event log and serves the page on 127.0.0.1.";
  shell.holds = [
    { name: "Serving the page", text: "The built page, a POST route per command and one event stream, behind a token, a cookie and a Host check.", paths: ["src-tauri/src/web.rs", "src-tauri/src/api.rs"] },
    { name: "The read loop", text: "Follows the event log and sends what changed to the page.", paths: ["src-tauri/src/read_loop.rs"] },
    { name: "The quiet log", text: "One line per thing you use, kept for the stop rules, with nothing shown." },
  ];
  return JSON.stringify(doc);
})();
const mapSheetHolds = (): Fixture =>
  mapOpened(`
      const plate = document.querySelector("#map .plate[data-id=shell]");
      if (!plate.title.endsWith("The program that holds the state, follows the event log and serves the page on 127.0.0.1.")) log("snap:fail the plate's hover text is not the purpose: " + plate.title);
      if (plate.title.includes("Serving the page")) log("snap:fail the plate's hover text holds the held items");
      plate.click();
      await wait(300);
      const sheet = document.querySelector("#map-sheet .sheet");
      if (!sheet) return log("snap:fail the sheet did not open");
      const purpose = sheet.querySelector(".sheet-purpose");
      const list = sheet.querySelector(".sheet-holds");
      if (!purpose || !list) return log("snap:fail the sheet lacks its purpose or its held items");
      if (!(purpose.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING)) log("snap:fail the held items come before the purpose");
      const rows = [...list.querySelectorAll(".sheet-held")];
      if (rows.length !== 3) log("snap:fail the sheet shows " + rows.length + " held items, not 3");
      const files = rows.map((row) => [...row.querySelectorAll(".sheet-held-files code")].map((code) => code.textContent));
      if (files[0].join() !== "src-tauri/src/web.rs,src-tauri/src/api.rs") log("snap:fail the first item's key files are " + files[0].join());
      if (files[2].length) log("snap:fail an item without paths shows key files");
      const notes = rows.map((row) => row.classList.contains("sheet-held-note"));
      if (notes.join() !== "false,false,true") log("snap:fail only the item without paths is a note: " + notes.join());
      if (rows[2].querySelector(".sheet-held-head")?.textContent !== "The quiet log · note") log("snap:fail the note's head is " + rows[2].querySelector(".sheet-held-head")?.textContent);
      // The glass's rise ends before the picture is taken.
      await wait(400);
      log("sheet holds ok");`, { map_state: mapState({ source: heldSheet }) });
const longProof = [{ file: "workbench/tests/wire.rs", line: 11, test: "wire_types_match", first: 11, snippet: ["// @law: wire-types-agree", "#[test]", "fn wire_types_match() {", ...Array.from({ length: 40 }, (_, i) => `    let sample_${i} = studio_test_support::sample(${i});`), "}"].join("\n") }];
const sheetHelpers = `
      const esc = async () => { document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true })); await wait(100); };
      const sheetOf = () => document.querySelector("#map-sheet .sheet");
      const drawerOf = () => document.querySelector("#map-drawer .drawer");
      const plate = (id) => document.querySelector("#map .plate[data-id=" + id + "]");
      const proof = async () => {
        [...document.querySelectorAll("#map-sheet .sheet-law button")].find((b) => b.textContent === "See the proof").click();
        await wait(100);
      };`;
// At 1300x700 (`…Short`): the side column, not the sheet or the drawer, scrolls, so the sheet's last button and the drawer's end are reachable;
// the drawer's code block keeps its own scroll (danielrispler/plotkeeper#443, issue 2a).
const mapSheetScrollShort = (): Fixture =>
  mapOpened(`${sheetHelpers}
      plate("shell").click();
      await wait(300);
      const side = document.getElementById("map-side"), sheet = document.getElementById("map-sheet");
      const inWindow = (node) => { const r = node.getBoundingClientRect(); return r.top >= 0 && r.bottom <= innerHeight; };
      side.scrollTop = 1e6;
      await wait(50);
      const last = [...sheet.querySelectorAll("button")].at(-1);
      if (!inWindow(last)) log("snap:fail the sheet's last button (" + last.textContent + ") is outside the window with the side column scrolled to its end: bottom " + Math.round(last.getBoundingClientRect().bottom) + " of " + innerHeight);
      await proof();
      const drawer = document.getElementById("map-drawer"), code = drawer.querySelector(".drawer-code");
      if (!code) return log("snap:fail the proof drawer did not open");
      side.scrollTop = 1e6;
      await wait(50);
      if (side.scrollHeight <= side.clientHeight) log("snap:fail the side column does not scroll: " + side.scrollHeight + " <= " + side.clientHeight);
      if (side.getBoundingClientRect().bottom > innerHeight) log("snap:fail the side column runs below the window");
      if (!inWindow([...drawer.querySelectorAll("button")].at(-1) ?? code)) log("snap:fail the drawer's end is outside the window with the side column scrolled to its end");
      for (const [name, box] of [["sheet", sheet], ["drawer", drawer]]) if (box.scrollHeight > box.clientHeight + 1) log("snap:fail the " + name + " is a scroll box of its own: " + box.scrollHeight + " > " + box.clientHeight);
      if (code.scrollHeight <= code.clientHeight) log("snap:fail the drawer's code block does not scroll on its own");
      // The glass's rise ends before the picture is taken.
      await wait(400);
      log("sheet scroll ok");`, { map_state: mapState({ source: longSheet }), map_proof: longProof });
// The sheet's width (#445): a real drag of its left edge, within 300 px and half the map, arrow keys on the focused handle, a double-click
// back to 360, and the width kept in this browser.
const mapSheetWidth = (): Fixture => ({
  ...mapOpened(`${sheetHelpers}
      plate("shell").click();
      await wait(300);
      // The width is the side column's, as map-sheet.ts measures it: a sheet taller than the window gets the column's scrollbar inside it.
      const sheet = document.getElementById("map-sheet"), stage = document.querySelector(".map-stage");
      const width = () => Math.round(document.getElementById("map-side").getBoundingClientRect().width);
      const near = (a, b) => Math.abs(a - b) <= 2;
      if (!near(width(), 360)) log("snap:fail the sheet opens " + width() + " px wide, not 360");
      const handle = sheet.querySelector(".sheet-resize");
      if (!handle) return log("snap:fail the sheet has no resize handle");
      await __snapDrag(handle, -100, 0);
      await wait(100);
      if (!near(width(), 460)) log("snap:fail a 100 px drag left made the sheet " + width() + " px wide, not 460");
      if (Number(localStorage.getItem("map-sheet-width")) !== width()) log("snap:fail the dragged width is not kept: " + localStorage.getItem("map-sheet-width"));
      await __snapDrag(handle, -2000, 0);
      await wait(100);
      const half = Math.floor(stage.getBoundingClientRect().width / 2);
      if (!near(width(), half)) log("snap:fail a long drag left made the sheet " + width() + " px wide, not half the map, " + half);
      await __snapDrag(handle, 2000, 0);
      await wait(100);
      if (!near(width(), 300)) log("snap:fail a long drag right made the sheet " + width() + " px wide, not 300");
      handle.focus();
      const before = width();
      handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", code: "ArrowLeft", bubbles: true }));
      await wait(50);
      if (width() <= before) log("snap:fail ArrowLeft on the handle did not widen the sheet: " + before + " to " + width());
      const wider = width();
      handle.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", code: "ArrowRight", bubbles: true }));
      await wait(50);
      if (width() >= wider) log("snap:fail ArrowRight on the handle did not narrow the sheet: " + wider + " to " + width());
      if (!sheetOf()) log("snap:fail the arrow keys closed the sheet");
      await __snapDrag(handle, -60, 0);
      await __snapDoubleClick(handle);
      await wait(100);
      if (!near(width(), 360)) log("snap:fail a double-click on the handle left the sheet " + width() + " px wide, not 360");
      log("sheet width ok");`, { map_state: mapState() }),
  realPointer: true, snapDone: "sheet width ok",
});
// A redraw in the middle of a drag (#443 review): the server's `map-landed`, which #450 sends at every stage of a write, draws the sheet
// again; the drag still resizes, since the handle stays in the page and keeps the pointer.
const mapSheetResizeRedraw = (): Fixture => ({
  ...mapOpened(`${sheetHelpers}
      plate("shell").click();
      await wait(300);
      const sheet = document.getElementById("map-sheet");
      const width = () => Math.round(document.getElementById("map-side").getBoundingClientRect().width);
      const handle = sheet.querySelector(".sheet-resize");
      if (!handle) return log("snap:fail the sheet has no resize handle");
      // The next map_state differs, so the map is drawn again.
      const stubbed = window.fetch;
      window.fetch = async (url, init) => {
        const reply = await stubbed(url, init);
        if (!String(url).endsWith("/api/map_state")) return reply;
        const state = await reply.json();
        return { ...reply, json: async () => state && { ...state, __redrawn: true } };
      };
      const before = width(), card = sheetOf();
      await __snapDrag(handle, -100, 0, async () => {
        window.__snapEmit("map-landed", "/w/plotkeeper");
        await wait(300);
        if (sheetOf() === card) log("snap:fail the map-landed did not draw the sheet again");
      });
      await wait(100);
      if (Math.abs(width() - (before + 100)) > 2) log("snap:fail a 100 px drag left with a redraw in its middle made the sheet " + width() + " px wide, not " + (before + 100));
      log("resize redraw ok");`, { map_state: mapState() }),
  realPointer: true, snapDone: "resize redraw ok",
});
// A width kept from an earlier visit opens the sheet at that width.
const mapSheetKeptWidth = (): Fixture => ({
  ...mapOpened(`${sheetHelpers}
      plate("shell").click();
      await wait(300);
      const width = Math.round(document.getElementById("map-side").getBoundingClientRect().width);
      if (Math.abs(width - 480) > 2) log("snap:fail the kept width 480 opened the sheet " + width + " px wide");
      await wait(400);
      log("kept width ok");`, { map_state: mapState() }),
  storage: { "map-sheet-width": "480" }, snapDone: "kept width ok",
});
// Closing the sheet (#445): a real click, or a press of 3 px, on the empty map closes the sheet and its drawer; a longer drag pans and keeps
// them; a click on another part switches the sheet; × (named "Close") and Esc close it.
const mapSheetClose = (): Fixture => ({
  ...mapOpened(`${sheetHelpers}
      const side = document.getElementById("map-side").getBoundingClientRect();
      // A point of the empty map, clear of the plates, the tray, the legend and the side column.
      let spot = null;
      for (let y = 120; y < innerHeight - 160 && !spot; y += 40) for (let x = 80; x < side.left - 80 && !spot; x += 40) {
        const hit = document.elementFromPoint(x, y);
        if (hit && (hit.classList.contains("map-world") || hit.classList.contains("map-canvas"))) spot = { getBoundingClientRect: () => ({ left: x - 1, top: y - 1, width: 2, height: 2 }) };
      }
      if (!spot) return log("snap:fail no empty spot of the map found");
      const opened = async () => {
        await __snapClick(plate("shell"));
        await wait(100);
        if (sheetOf()?.dataset.part !== "shell") log("snap:fail a click on the shell did not open its sheet");
        await proof();
        if (!drawerOf()) log("snap:fail the proof drawer did not open");
      };
      await opened();
      await __snapClick(spot);
      await wait(80);
      if (sheetOf() || drawerOf()) log("snap:fail a click on the empty map left the sheet " + !!sheetOf() + " or the drawer " + !!drawerOf() + " open");
      await opened();
      await __snapDrag(spot, 3, 0);
      await wait(80);
      if (sheetOf() || drawerOf()) log("snap:fail a 3 px press on the empty map left the sheet or the drawer open");
      await opened();
      const was = plate("hook").getBoundingClientRect().left;
      await __snapDrag(spot, 40, 0);
      await wait(80);
      if (Math.abs(plate("hook").getBoundingClientRect().left - was - 40) > 2) log("snap:fail a 40 px drag did not pan the map");
      if (!sheetOf() || !drawerOf()) log("snap:fail a 40 px drag on the empty map closed the sheet or the drawer");
      await __snapClick(plate("hook"));
      await wait(100);
      if (sheetOf()?.dataset.part !== "hook") log("snap:fail a click on another part did not switch the sheet: " + sheetOf()?.dataset.part);
      // The shell's proof drawer stays open over the switch; Esc peels it first.
      await esc();
      if (drawerOf() || !sheetOf()) log("snap:fail the first Esc did not close the drawer alone");
      const close = document.querySelector("#map-sheet .sheet-close");
      if (close?.getAttribute("aria-label") !== "Close") log("snap:fail × is not named Close: " + close?.getAttribute("aria-label"));
      close.click();
      await wait(100);
      if (sheetOf()) log("snap:fail × did not close the sheet");
      await __snapClick(plate("hook"));
      await wait(100);
      await esc();
      if (sheetOf()) log("snap:fail Esc did not close the sheet");
      log("sheet close ok");`, { map_state: mapState(), map_proof: longProof }),
  realPointer: true, snapDone: "sheet close ok",
});

// Real keys (#454): \`await __snapKey(key)\` presses a key as Chrome's own input, so Tab moves focus and Esc closes a native popover, which a
// synthetic KeyboardEvent does neither of. From the page's start, Tab reaches the page's first control and Shift+Tab leaves it; Enter
// and Space press a focused button; the arrows reach the page as trusted keys; Esc closes an open popover.
const snapRealKeys = (): Fixture => ({
  ...base,
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const press = async (key) => { await __snapKey(key); await wait(50); };
      const named = (node) => node?.id || node?.tagName;
      document.activeElement?.blur();
      // The page's first control is the skip link (#461).
      const first = document.querySelector("a.skip-link");
      if (!first) return log("snap:fail the page has no skip link");
      await press("Tab");
      if (document.activeElement !== first) log("snap:fail Tab from the page's start focused " + named(document.activeElement) + ", not the skip link");
      await press("Shift+Tab");
      if (document.activeElement === first) log("snap:fail Shift+Tab left focus on the skip link");
      // A button made for the check, taken out again before the hover check runs.
      const button = Object.assign(document.createElement("button"), { type: "button", textContent: "Pressed" });
      const clicks = [];
      button.addEventListener("click", (event) => clicks.push(event.isTrusted));
      document.body.append(button);
      button.focus();
      await press("Enter");
      await press("Space");
      button.remove();
      if (clicks.join() !== "true,true") log("snap:fail Enter and Space on a focused button clicked it " + clicks.join() + " (trusted per click)");
      const arrows = [];
      const heard = (event) => event.isTrusted && arrows.push(event.key);
      document.addEventListener("keydown", heard);
      await press("ArrowUp");
      await press("ArrowDown");
      document.removeEventListener("keydown", heard);
      if (arrows.join() !== "ArrowUp,ArrowDown") log("snap:fail the arrows reached the page as " + arrows.join());
      const menu = document.getElementById("project-actions");
      menu.showPopover();
      if (!menu.matches(":popover-open")) return log("snap:fail the popover did not open");
      await press("Escape");
      if (menu.matches(":popover-open")) log("snap:fail Esc left the popover open");
      log("keys ok");`,
  realKeys: true, snapDone: "keys ok",
});

// What's new (#527): a page that last showed 0.1.2 opens on 0.2.0, so one toast says "Updated to 0.2.0" with a link to that
// version's GitHub Release, and the new version is stored; its × closes it. The screenshot is the toast open.
const whatsNewToast = (): Fixture => ({
  ...list(),
  storage: { "shown-version": "0.1.2" },
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      await wait(300);
      const toast = document.getElementById("whats-new-toast");
      if (!toast.matches(":popover-open") || !document.getElementById("toasts").contains(toast)) return log("snap:fail the what's-new toast is not an open popover in the region");
      if (!toast.textContent.includes("Updated to 0.2.0")) log("snap:fail the toast says " + toast.textContent);
      const link = toast.querySelector("a");
      if (link?.textContent !== "What's new" || link.href !== "https://github.com/danielrispler/plotkeeper/releases/tag/v0.2.0") log("snap:fail the link: " + link?.outerHTML);
      if (!toast.querySelector("[aria-label=Dismiss]")) log("snap:fail the toast has no ×");
      if (localStorage.getItem("shown-version") !== "0.2.0") log("snap:fail the shown version is " + localStorage.getItem("shown-version"));
      log("whats new ok");`,
  snapDone: "whats new ok",
});
// The toasts from the keyboard (#457): the needs-you, the note and the Clear toasts open at once in the Notifications region, apart.
// A real press on Undo leaves the focus where it was. From the control before the region, a real Tab reaches each main action and
// each ×; a real Enter on a × closes its toast and puts the focus on the next toast, and the last one hands it back. A modal dialog
// holds the toasts back until it closes. A real Enter on the note and on the needs-you toast runs their action.
const toastKeys = (): Fixture => {
  const sessions = [...(list().sessions as Session[])];
  const blocked = { ...sessions[1], id: "s5" };
  return {
    ...list(),
    select: { __by: "id", values: Object.fromEntries([...sessions, blocked].map((s) => [s.id, { session: s, agent: null }])) },
    snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const press = async (key) => { await __snapKey(key); await wait(30); };
      const byId = (id) => document.getElementById(id);
      const open = (id) => byId(id).matches(":popover-open");
      const name = (node) => (node?.closest(".toast")?.id ?? node?.id ?? node?.tagName) + " " + (node?.getAttribute?.("aria-label") ?? node?.textContent);
      document.querySelector(".list .clear-landed").click();
      await wait(50);
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Enter", key: "Enter", metaKey: true, shiftKey: true, bubbles: true }));
      await wait(100);
      window.__snapEmit("finished", ["s4"]);
      await wait(50);
      const region = byId("toasts");
      if (region.tagName !== "SECTION" || region.getAttribute("aria-label") !== "Notifications") log("snap:fail the toasts' region is not named Notifications");
      for (const id of ["toast", "note-toast", "clear-toast"]) if (!open(id) || !region.contains(byId(id))) return log("snap:fail #" + id + " is not an open popover in the region");
      const boxes = ["toast", "note-toast", "clear-toast"].map((id) => byId(id).getBoundingClientRect());
      const apart = (a, b) => a.bottom <= b.top || b.bottom <= a.top || a.right <= b.left || b.right <= a.left;
      if (!apart(boxes[0], boxes[1]) || !apart(boxes[0], boxes[2]) || !apart(boxes[1], boxes[2])) log("snap:fail two toasts overlap: " + boxes.map((b) => [b.left, b.top, b.right, b.bottom].map(Math.round)).join(" | "));
      if (boxes[1].top < boxes[0].bottom) log("snap:fail the note is not under the needs-you toast");
      const before = Object.assign(document.createElement("button"), { type: "button", textContent: "Before", title: "Where the focus was" });
      region.before(before);
      // A real press on Undo acts and leaves the focus where it was; Clear landed again brings the toast back.
      before.focus();
      await __snapClick(byId("clear-toast").querySelector("button"));
      await wait(50);
      if (open("clear-toast")) log("snap:fail the press on Undo did not undo");
      if (document.activeElement !== before) log("snap:fail a press on Undo moved the focus to " + name(document.activeElement));
      document.querySelector(".list .clear-landed").click();
      await wait(50);
      if (!open("clear-toast")) return log("snap:fail the Clear toast did not show again");
      before.focus();
      const buttons = [...region.querySelectorAll("button")];
      const reached = [];
      for (let i = 0; i < 6; i++) {
        await press("Tab");
        reached.push(document.activeElement);
      }
      if (reached.some((node, i) => node !== buttons[i])) log("snap:fail Tab reached " + reached.map(name).join(", ") + ", not " + buttons.map(name).join(", "));
      // The note's × from the keyboard: the focus goes to the next toast, the Clear toast's Undo.
      byId("note-toast").querySelector("[aria-label=Dismiss]").focus();
      await press("Enter");
      if (open("note-toast")) log("snap:fail Enter on the note's × left it open");
      if (document.activeElement !== byId("clear-toast").querySelector("button")) log("snap:fail after the note's × the focus is on " + name(document.activeElement));
      byId("clear-toast").querySelector("[aria-label=Dismiss]").focus();
      await press("Enter");
      if (open("clear-toast")) log("snap:fail Enter on the Clear toast's × left it open");
      if (document.activeElement !== byId("toast").querySelector("button")) log("snap:fail after the Clear toast's × the focus is on " + name(document.activeElement));
      await press("Tab");
      await press("Enter");
      if (open("toast")) log("snap:fail Enter on the needs-you toast's × left it open");
      if (document.activeElement !== before) log("snap:fail after the last toast's × the focus is on " + name(document.activeElement) + ", not where it was");
      // A modal dialog holds a new note back until it closes.
      const dialog = byId("delete-dialog");
      dialog.showModal();
      window.__snapEmit("finished", ["s4"]);
      await wait(50);
      if (open("note-toast")) log("snap:fail the note showed over a modal dialog");
      dialog.close();
      await wait(50);
      if (!open("note-toast")) return log("snap:fail the note did not show once the dialog closed");
      // Enter on the note selects its session.
      before.focus();
      await press("Tab");
      await press("Enter");
      if (open("note-toast") || document.body.dataset.panel !== "story") log("snap:fail Enter on the note did not open its session: panel " + document.body.dataset.panel);
      // A real Esc in Delete's question closes the question alone, and the story under it stays.
      byId("delete").click();
      await wait(50);
      await press("Escape");
      await wait(50);
      if (dialog.open) log("snap:fail a real Esc did not close Delete's question");
      if (document.body.dataset.panel !== "story") log("snap:fail a real Esc in Delete's question also closed the story: panel " + document.body.dataset.panel);
      // Enter on the needs-you toast of a new block jumps to it and leaves focus mode.
      window.__snapEmit("sessions", ${JSON.stringify([...sessions, blocked])});
      await wait(50);
      if (!open("toast")) return log("snap:fail no needs-you toast for the new block");
      before.focus();
      await press("Tab");
      await press("Enter");
      if (document.body.dataset.layout === "focus") log("snap:fail Enter on the needs-you toast did not leave focus mode");
      log("toasts ok");`,
    realKeys: true,
    realPointer: true,
    snapDone: "toasts ok",
  };
};

// The palette (#458): a real Esc, a real click outside it and its close button each close it with focus back on the element that had it
// before ⌘⇧P; its filter is a combobox whose real ↓ moves `aria-activedescendant` to an option, and no row is a pressed button.
const paletteFocus = (): Fixture => ({
  ...list(),
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const palette = document.getElementById("palette");
      const before = document.getElementById("settings-button");
      const named = (node) => node?.id || node?.className || node?.tagName;
      const open = async () => {
        before.focus();
        document.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyP", key: "P", metaKey: true, shiftKey: true, bubbles: true }));
        await wait(100);
        if (!palette.matches(":popover-open")) log("snap:fail the palette did not open as a popover");
      };
      const back = (how) => {
        if (palette.checkVisibility()) log("snap:fail " + how + " left the palette open");
        if (document.activeElement !== before) log("snap:fail after " + how + " focus is on " + named(document.activeElement) + ", not the settings button");
      };
      await open();
      const field = palette.querySelector("input");
      if (field?.getAttribute("role") !== "combobox") log("snap:fail the filter's role is " + field?.getAttribute("role"));
      const list = document.getElementById(field?.getAttribute("aria-controls"));
      if (list?.getAttribute("role") !== "listbox") log("snap:fail the filter controls " + named(list) + ", not a listbox");
      if (palette.querySelector("[aria-pressed]")) log("snap:fail a palette row carries aria-pressed");
      await __snapKey("ArrowDown");
      await wait(50);
      const active = document.getElementById(field?.getAttribute("aria-activedescendant"));
      if (active?.getAttribute("role") !== "option") log("snap:fail after ↓ the active descendant is " + named(active) + ", not an option");
      if (active && active !== list?.children[1]) log("snap:fail ↓ did not move to the second row");
      await __snapKey("Escape");
      await wait(100);
      back("a real Esc");
      await open();
      await __snapClick(document.querySelector(".bar-project") ?? document.querySelector("header.top"));
      await wait(100);
      back("an outside click");
      // A real click on a focusable control outside closes it and leaves the focus on that control (#452's review).
      await open();
      const outside = Object.assign(document.createElement("input"), { name: "outside", autocomplete: "off" });
      outside.setAttribute("aria-label", "A field outside the palette");
      document.querySelector("header.top").append(outside);
      await __snapClick(outside);
      await wait(100);
      if (palette.checkVisibility()) log("snap:fail a click on a field outside left the palette open");
      if (document.activeElement !== outside) log("snap:fail after a click on a field outside focus is on " + named(document.activeElement) + ", not the field");
      outside.remove();
      await open();
      const close = palette.querySelector('button[aria-label="Close"]');
      if (!close?.checkVisibility()) return log("snap:fail the palette has no visible close button");
      await __snapClick(close);
      await wait(100);
      back("the close button");
      log("palette ok");`,
  realKeys: true, realPointer: true, snapDone: "palette ok",
});

// Small gaps (#462): a real Shift+Tab from a row reaches the row before's delete, hidden only by opacity; the fold and delete boxes are
// at least 24×24; the drawer's jumps are links to a new tab; a real Tab reaches the drawer's code block, which has a name; and the panels,
// overlays and dialogs keep their scroll to themselves.
const mapSmallGaps = (): Fixture => ({
  ...mapOpened(`
      const press = async (key) => { await __snapKey(key); await wait(50); };
      const named = (node) => (node?.className || node?.tagName) + " " + (node?.getAttribute?.("aria-label") ?? "");
      const items = [...document.querySelectorAll("#groups .item")];
      const pair = items.findIndex((item, i) => item.querySelector(":scope > .delete") && !item.querySelector(":scope > .row[aria-pressed=true]")
        && items[i + 1] === item.nextElementSibling && items[i + 1].firstElementChild?.matches(".row"));
      if (pair < 0) return log("snap:fail no unselected row with a delete before a row with no fold");
      const remove = items[pair].querySelector(":scope > .delete");
      if (getComputedStyle(remove).opacity !== "0") log("snap:fail an unselected row's delete shows before hover or focus: opacity " + getComputedStyle(remove).opacity);
      items[pair + 1].firstElementChild.focus();
      await press("Shift+Tab");
      if (document.activeElement !== remove) log("snap:fail Shift+Tab from the next row focused " + named(document.activeElement) + ", not the row before's delete");
      for (const control of document.querySelectorAll("#groups .item > .fold, #groups .item > .delete")) {
        const box = control.getBoundingClientRect();
        if (box.width < 24 || box.height < 24) { log("snap:fail a row's " + control.className + " is " + box.width + "x" + box.height + ", under 24x24"); break; }
      }
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      [...document.querySelectorAll("#map-sheet .sheet-ci button")].find((b) => b.textContent === "See the CI log")?.click();
      await wait(300);
      const jumps = [...document.querySelectorAll("#map-drawer .drawer-jump")];
      if (!jumps.length) return log("snap:fail the CI log's drawer has no jump");
      for (const jump of jumps)
        if (!jump.matches("a[href]") || jump.target !== "_blank" || !jump.relList.contains("noopener")) log("snap:fail the drawer's jump " + jump.textContent + " is not a link to a new tab: " + jump.outerHTML);
      const code = document.querySelector("#map-drawer .drawer-code");
      document.querySelector("#map-drawer .sheet-close").focus();
      await press("Tab");
      if (document.activeElement !== code) log("snap:fail Tab from the drawer's × focused " + named(document.activeElement) + ", not its code block");
      if (!code.getAttribute("aria-label")) log("snap:fail the drawer's code block has no name");
      document.body.classList.add("list-open");
      const contained = ["#panel-body", "#map-pre-flight", "#map-patch-review", "#palette", "nav.list", "nav.list .full", "#rename-dialog", "#delete-dialog"];
      const loose = contained.filter((selector) => getComputedStyle(document.querySelector(selector)).overscrollBehaviorY !== "contain");
      document.body.classList.remove("list-open");
      if (loose.length) log("snap:fail these scroll the page behind them at their ends: " + loose.join(", "));
      // A ::backdrop honours overscroll-behavior only as a scroll container, so it needs overflow hidden too (modern-web-guidance,
      // prevent-page-scroll-under-modal-dialog).
      const leaky = ["#rename-dialog", "#delete-dialog", "#confirm-dialog"].filter((selector) => {
        const backdrop = getComputedStyle(document.querySelector(selector), "::backdrop");
        return backdrop.overflowY !== "hidden" || backdrop.overscrollBehaviorY !== "contain";
      });
      if (leaky.length) log("snap:fail these dialogs' backdrops scroll the page behind them: " + leaky.join(", "));
      log("small gaps ok");`, {
    map_state: mapState({ ci: brokenCi }),
    map_place: { s1: ["core", "window"], s2: ["hook"] },
  }),
  realKeys: true, snapDone: "small gaps ok",
});

// The page's structure (#461): from the page's start the first real Tab reaches "Skip to the map", shown, and a real Enter on it puts
// the focus in the map, the page's one `main`.
const skipLink = (): Fixture => ({
  ...list(),
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      const named = (node) => node ? node.tagName + (node.id ? "#" + node.id : "") + " " + JSON.stringify((node.textContent ?? "").trim().slice(0, 30)) : "nothing";
      await __snapKey("Tab");
      await wait(50);
      const link = document.activeElement;
      if (link?.textContent !== "Skip to the map") return log("snap:fail the first Tab focused " + named(link) + ", not the skip link");
      const box = link.getBoundingClientRect();
      if (box.bottom <= 0 || box.top >= innerHeight || box.width < 24) log("snap:fail the focused skip link is not on screen: " + JSON.stringify(box));
      await __snapKey("Enter");
      await wait(50);
      const main = document.querySelector("main");
      if (!main?.contains(document.activeElement)) return log("snap:fail Enter on the skip link focused " + named(document.activeElement) + ", not the map");
      log("skip link ok");`,
  realKeys: true, snapDone: "skip link ok",
});

// At 1000x800 (`…Narrow`) with the story panel open, the empty map's prompt sits beside the panel, not under it (#462).
const listNarrow = (): Fixture => ({
  ...list(),
  snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      await wait(500);
      const panel = document.getElementById("map-panel"), draft = document.querySelector("#map .map-draft");
      if (panel.hidden || !draft) return log("snap:fail the story panel or the empty map's prompt is not shown");
      const a = panel.getBoundingClientRect(), b = draft.getBoundingClientRect();
      if (a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom) log("snap:fail the prompt (" + Math.round(b.left) + "–" + Math.round(b.right) + ") is under the left panel (" + Math.round(a.left) + "–" + Math.round(a.right) + ")");
      log("narrow ok");`,
  snapDone: "narrow ok",
});

export const fixtures = {
  ...railFixtures,
  // What's new after an update (#527).
  whatsNewToast,
  // The toasts from the keyboard (#457).
  toastKeys,
  // Real keys from a fixture (#454).
  snapRealKeys,
  // The palette as a popover listbox (#458).
  paletteFocus,
  // The small keyboard, pointer and layout gaps (#462).
  mapSmallGaps,
  listNarrow,
  // The page's structure: the skip link (#461).
  skipLink,
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
      test_writer: { outcome: "passed", line: "Passed: `agy` answered. Summaries are written with it.", settings: null },
    }, `
      if (document.getElementById("writer-pick").value !== "agy") log("snap:fail the preset is not agy, the first found");
      const outcome = document.getElementById("writer-outcome");
      if (!outcome?.classList.contains("passed") || !outcome.textContent.startsWith("Passed")) log("snap:fail no passed outcome: " + outcome?.textContent);`, true),
  // A failed Test with its reason and the writer's own fix.
  writersFailed: () =>
    writersOpen({
      writers: { writers: [claudeWriter, agyWriter], summaries_on: true, briefs: true, keys: [], base_url: null },
      test_writer: { outcome: "not_logged_in", line: "`claude` is not logged in. Run `claude` in a terminal and log in with /login.", settings: null },
    }, `${writerStatusBefore}
      document.getElementById("writer-test").click();
      await wait(300);
      const outcome = document.getElementById("writer-outcome");
      if (!outcome?.classList.contains("failed") || !outcome.textContent.includes("/login")) log("snap:fail no failed outcome with its fix: " + outcome?.textContent);${writerStatusAfter("/login")}`),
  // No API key set (#366): the writer is listed but cannot be picked, and the key row offers to add one.
  writersKeyNotSet: () =>
    writersOpen({ writers: { writers: [anthropicWriter, claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: null } }, `
      const first = document.querySelector("#writer-pick option");
      if (first?.textContent !== "Anthropic API key (not set)" || !first.disabled) log("snap:fail the unset key is not listed first and unpickable: " + first?.textContent);
      if (!document.getElementById("writer-key-state")?.textContent.includes("not set")) log("snap:fail the key row does not say not set");
      if (document.getElementById("writer-key")?.placeholder !== "Add an API key…") log("snap:fail no field to add an API key");
      if (document.getElementById("writer-key-remove")) log("snap:fail Remove shown with no key set");
      // An empty base URL's placeholder reads as an example, not as part of the URL (#452's review).
      const example = document.getElementById("writer-base-url")?.placeholder;
      if (example !== "e.g. https://api.openai.com/v1…") log("snap:fail the base URL's placeholder is " + JSON.stringify(example));`),
  // An API key set: it comes first in the preset order, and a Test of the Anthropic writer passes.
  writersKeySet: () =>
    writersOpen({
      writers: { writers: [{ ...anthropicWriter, found: true }, claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: ["anthropic"], base_url: null },
      test_writer: { outcome: "passed", line: "Passed: `anthropic` answered. Summaries are written with it.", settings: null },
    }, `
      if (document.getElementById("writer-pick").value !== "anthropic") log("snap:fail the preset is not the API key");
      if (document.getElementById("writer-model").value !== "claude-haiku-5-5") log("snap:fail the preset model is not the first");
      if (!document.getElementById("writer-key-state")?.textContent.includes(": set")) log("snap:fail the key row does not say set");
      if (document.getElementById("writer-key")?.placeholder !== "Replace the API key…" || !document.getElementById("writer-key-remove")) log("snap:fail no replace or remove");
      const outcome = document.getElementById("writer-outcome");
      if (!outcome?.classList.contains("passed")) log("snap:fail no passed outcome: " + outcome?.textContent);
      ${confirmFirst(`document.getElementById("writer-key-remove").click()`, "remove_api_key", "Remove key", ["Keychain"])}`, true),
  // A local Ollama set by its base URL alone (#367): the OpenAI-compatible API is preset before Claude, its model is typed,
  // its row shows the base URL and no key, and a Test of it passes.
  // The contrast pass (#455) runs here too: its fields' borders.
  writersOpenAI: (): Fixture => ({
    ...writersOpen({
      writers: { writers: [anthropicWriter, { ...openaiWriter, found: true }, claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: "http://localhost:11434/v1" },
      test_writer: { outcome: "passed", line: "Passed: `openai` answered. Summaries are written with it.", settings: null },
    }, `
      if (document.getElementById("writer-pick").value !== "openai") log("snap:fail the preset is not the OpenAI-compatible API");
      const model = document.getElementById("writer-model");
      if (model?.tagName !== "INPUT" || model.value !== "gpt-6-luna") log("snap:fail the model is not a typed field at the first model: " + model?.tagName);
      if (document.getElementById("writer-base-url")?.value !== "http://localhost:11434/v1") log("snap:fail the base URL field does not hold the base URL");
      if (!document.getElementById("writer-openai-state")?.textContent.includes("no key")) log("snap:fail the row does not say no key");
      if (!document.getElementById("writer-openai-remove")) log("snap:fail no Remove with a base URL set");
      const outcome = document.getElementById("writer-outcome");
      if (!outcome?.classList.contains("passed")) log("snap:fail no passed outcome: " + outcome?.textContent);
      ${confirmFirst(`document.getElementById("writer-openai-remove").click()`, "remove_api_key", "Remove", ["http://localhost:11434/v1", "Keychain"])}`, true),
    contrast: true,
  }),
  // No claude: agy writes the summaries, and the section says PR briefs need Claude.
  writersBriefsNeedClaude: () =>
    writersOpen({
      writers: { writers: [{ ...claudeWriter, found: false }, agyWriter], summaries_on: true, briefs: false, keys: [], base_url: null },
      settings_files: { settings: '{ "summaries": { "writer": "agy", "model": "gemini-4-neon-high" } }', keymap: null },
    }, `
      if (!document.getElementById("writer-briefs")?.textContent.includes("briefs need Claude")) log("snap:fail no briefs need Claude line");
      if (document.getElementById("writer-model").value !== "gemini-4-neon-high") log("snap:fail the model in effect is not shown");`),
  // #456: a key the Keychain refuses: its error is said in the section's status region, and tied to the key's field.
  writersKeyError: () =>
    writersOpen({
      writers: { writers: [anthropicWriter, claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: null },
      set_api_key: { __throw: "the Keychain refused the key" } as never,
    }, `${writerStatusBefore}
      document.getElementById("writer-key").value = "sk-ant-not-a-key";
      document.getElementById("writer-key-save").click();
      await wait(300);
      const error = document.getElementById("writer-key-error");
      if (!error?.textContent.includes("Keychain refused")) return log("snap:fail no key error: " + error?.textContent);
${writerStatusAfter("Keychain refused")}
      const field = document.getElementById("writer-key");
      if (field.getAttribute("aria-invalid") !== "true" || !(field.getAttribute("aria-describedby") ?? "").split(" ").includes("writer-key-error")) log("snap:fail the key error is not tied to its field");`),
  writersListNarrow: (): Fixture => fixtures.writersList(),
  writersPassedNarrow: (): Fixture => fixtures.writersPassed(),
  writersFailedNarrow: (): Fixture => fixtures.writersFailed(),
  writersBriefsNeedClaudeNarrow: (): Fixture => fixtures.writersBriefsNeedClaude(),
  // The contrast pass (#455) runs here too.
  mapTray: (): Fixture => ({ ...mapTray(), contrast: true }),
  mapTrayAfterPush,
  mapRuns,
  // The contrast pass (#455) runs here too.
  mapPreFlight: (): Fixture => ({ ...mapPreFlight(), contrast: true }),
  mapPreFlightStatus,
  mapMicroWorld,
  mapRunsPaused,
  mapRunsAnnounced,
  mapBackLaw,
  railBriefRow: () => railBriefReady(false),
  railBriefOpened: () => railBriefReady(true),
  mapBriefStart,
  // The contrast pass (#455) runs here too.
  mapBriefStory: (): Fixture => ({ ...mapBriefStory(), contrast: true }),
  mapBriefAnnounce,
  mapBriefStepCode,
  mapBriefAlsoGroups,
  mapBriefCopied,
  mapBriefCopyByHand,
  mapBriefSince,
  mapBriefStates: () => mapBriefFigure("states"),
  mapBriefSequence: () => mapBriefFigure("sequence"),
  mapBriefCompare: () => mapBriefFigure("compare"),
  mapBriefHeights,
  mapBriefTall,
  mapBriefTaller,
  mapBriefTallScroll,
  mapBriefUnconfirmed,
  mapBriefPartialFacts: () => mapBriefPartialFacts(false),
  mapBriefPartialFactsOpen: () => mapBriefPartialFacts(true),
  mapBriefNoFacts,
  mapBriefStaleBase,
  mapBriefNoStaleBase,
  mapBriefWhy: () => mapBriefWhy(false),
  mapBriefWhyEsc: () => mapBriefWhy(true),
  mapBriefWhyDecision,
  mapBriefPickRight: () => mapBriefPick(1),
  mapBriefPickWrong: () => mapBriefPick(0),
  mapBriefOrderSlots,
  mapBriefOrderWrong,
  mapBriefOrderRight,
  mapBriefRatify,
  mapBriefExplained: () => mapBriefExplained("where"),
  mapBriefRatifyExplained: () => mapBriefExplained("ratify"),
  mapBriefExplainedOff: () => mapBriefExplained("off"),
  mapBriefRatifyDetail,
  mapBriefRatifyEvidence: () => mapBriefRatifyEvidence("line"),
  mapBriefRatifyEvidenceList: () => mapBriefRatifyEvidence("list"),
  mapBriefRatifyEvidenceSite: () => mapBriefRatifyEvidence("site"),
  mapBriefGuard: () => mapBriefGuard(false),
  mapBriefGuardConfirm: () => mapBriefGuard(true),
  mapBriefBar,
  mapBriefExplainBack: () => mapBriefExplainBack(false),
  mapBriefExplainBackDone: () => mapBriefExplainBack(true),
  mapBriefOverturnCopied: () => mapBriefOverturn("copies"),
  mapBriefOverturnFallback: () => mapBriefOverturn("refuses"),
  mapBriefWriting,
  mapCatchUp,
  mapCatchUpWriting,
  mapCatchUpWritingStage: () => mapCatchUpStage({ kind: "writing" }, "Writing…"),
  mapCatchUpCheckingStage: () => mapCatchUpStage({ kind: "checking" }, "Checking claims…"),
  mapCatchUpFixingStage: () => mapCatchUpStage({ kind: "fixing", claims: 2 }, "Fixing 2 problems…"),
  mapCatchUpSummarising,
  mapCatchUpBold,
  mapBriefBold,
  mapBriefBoldUnpaired,
  mapSheetUnratified,
  mapStale,
  mapMarksThick: () => mapMarks("thick"),
  mapMarksThin: () => mapMarks("thin"),
  mapMarksStale: () => mapMarks("stale"),
  mapMarksBroken: () => mapMarks("broken"),
  mapRefresher,
  mapTalksTo,
  mapSheetExplained: () => mapSheetExplained(true),
  mapSheetExplainedOff: () => mapSheetExplained(false),
  mapBriefPairLight,
  mapBriefChapters,
  mapBriefChapterMid,
  mapBriefChapterLast,
  logRunning: () => logFixture("running"),
  // A call's result scrolls on its own, so it takes focus with a name (#462).
  logDone: () => ({
    ...logFixture("completed"),
    snapScript: `
      const wait = (ms) => new Promise((done) => setTimeout(done, ms));
      await wait(300);
      const results = [...document.querySelectorAll(".log-result")];
      if (!results.length) return log("snap:fail the log shows no call's result");
      for (const result of results) if (result.tabIndex !== 0 || !result.getAttribute("aria-label")) log("snap:fail a call's result takes no focus or has no name: " + result.outerHTML.slice(0, 80));
      log("log ok");`,
    snapDone: "log ok",
  }),
  // The contrast pass (#455) runs here too.
  list: (): Fixture => ({ ...list(), contrast: true }),
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
      session("s1", "#56 · Row controls", "running", { transcript: { ...session("x", "x", "idle").transcript, context: { tokens: 90000 } } }),
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
  // No harness wired (#437): a first start declined, so no sessions and no story open; the base Map view still shows the
  // not-recording line, naming `plotkeeper install`.
  notRecording: () => ({
    ...base,
    wired: false,
    snapScript: `
      await new Promise((r) => setTimeout(r, 100));
      const shown = [...document.querySelectorAll("[role=status]")].filter((el) => el.checkVisibility() && /^Not recording: .*\`plotkeeper install\`/.test(el.textContent));
      if (shown.length !== 1) log("snap:fail the not-recording line is not visible once on the base Map view: " + shown.length);`,
  }),
  // A harness wired (#437): no not-recording line anywhere on the page.
  recording: () => ({
    ...base,
    wired: true,
    snapScript: `
      await new Promise((r) => setTimeout(r, 100));
      if (document.body.textContent.includes("Not recording")) log("snap:fail a not-recording line is on the page");`,
  }),
  // Alerts: two sessions' turns ended off screen (the note toast). The contrast pass (#455) runs here too: the toast's buttons.
  alerts() {
    const context = (k: number) => ({ ...session("x", "x", "idle").transcript, context: { tokens: k * 1000 } });
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
      log("note: " + note.textContent);`;
    return { ...base, sessions, selected: { session: sessions[1], agent: null }, snapScript, contrast: true };
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
  // A header's "⋯" menu (#458) opens under its button and inside the window, and flips over it near the window's bottom edge.
  projectMenu: () => withSaved(projectMenuScript(false), { realPointer: true, snapDone: "project menu ok" }),
  projectMenuBottom: () => withSaved(projectMenuScript(true), { realPointer: true, snapDone: "project menu ok" }),
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
  // The map's camera (#383): an empty point of the map is the map itself (no layer over it), a drag there pans, a wheel and a pinch
  // zoom up to 200 % and never scroll the page, and ⌘0 fits the whole map; no part's place changes. Shown fitted.
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
      const out = camera();
      if (!(out.scale < panned.scale)) log("snap:fail a plain wheel did not zoom out: " + JSON.stringify(out));
      if (!plain.defaultPrevented || document.scrollingElement.scrollTop !== scrolled) log("snap:fail a plain wheel was not swallowed");
      canvas.dispatchEvent(wheel({ ctrlKey: true }));
      if (!(camera().scale > out.scale)) log("snap:fail a pinch did not zoom in");
      for (let i = 0; i < 40; i++) canvas.dispatchEvent(wheel({ deltaY: -300 }));
      if (camera().scale !== 2) log("snap:fail zoom is not held at 200 %: " + camera().scale);
      for (let i = 0; i < 40; i++) canvas.dispatchEvent(wheel({ deltaY: 300 }));
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
      // With the sheet open, the not placed list's title sits over no plate (#466).
      const title = document.querySelector("#map-not-placed .side-title");
      if (!title) return log("snap:fail no Not placed title");
      const at = title.getBoundingClientRect();
      for (const plate of document.querySelectorAll("#map .plate")) {
        const box = plate.getBoundingClientRect();
        if (at.left < box.right && box.left < at.right && at.top < box.bottom && box.top < at.bottom) log("snap:fail the Not placed title is over the " + plate.dataset.id + " plate");
      }
      log("thin " + thin + ", thick " + thick);`, { map_state: mapState({ fog: { shell: "thick", window: "thick", core: "thin", hook: "thin" } }), map_know: { positions: {}, tiers: {} }, contrast: true }),
  // The dark page (#455): the metas, the root's colour scheme, no ::-webkit-scrollbar rule left, and the pre-flight's hand picker,
  // whose native checkbox draws dark, sampled from Chrome's own screenshot.
  mapPreFlightPick: (): Fixture => ({
    ...mapOpened(`
      const meta = (name) => document.querySelector('meta[name="' + name + '"]')?.content;
      if (meta("color-scheme") !== "dark") log("snap:fail no color-scheme meta of dark: " + meta("color-scheme"));
      const chrome = getComputedStyle(document.documentElement).getPropertyValue("--chrome").trim();
      if (!chrome || meta("theme-color") !== chrome) log("snap:fail the theme-color meta " + meta("theme-color") + " is not --chrome, " + chrome);
      if (getComputedStyle(document.documentElement).colorScheme !== "dark") log("snap:fail the root's color-scheme is " + getComputedStyle(document.documentElement).colorScheme);
      const webkit = [...document.styleSheets].flatMap((sheet) => [...sheet.cssRules]).filter((rule) => rule.selectorText?.includes("::-webkit-scrollbar"));
      if (webkit.length) log("snap:fail ::-webkit-scrollbar rules are left: " + webkit.map((rule) => rule.selectorText).join(", "));
      document.getElementById("pre-flight-button").click();
      await wait(600);
      const byHand = [...document.querySelectorAll("#map-pre-flight button")].find((b) => b.textContent === "Pick parts by hand");
      if (!byHand) return log("snap:fail the pre-flight offers no Pick parts by hand");
      byHand.click();
      await wait(300);
      const box = document.querySelector("#map-pre-flight input[type=checkbox]");
      if (!box) return log("snap:fail the hand picker has no checkbox");
      const shot = await window.__snapShot();
      const at = box.getBoundingClientRect();
      const scale = shot.width / innerWidth;
      const i = (Math.round((at.top + at.height / 2) * scale) * shot.width + Math.round((at.left + at.width / 2) * scale)) * 4;
      const [red, green, blue] = shot.data.slice(i, i + 3);
      const channel = (c) => ((c /= 255) <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
      if (0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue) > 0.2) log("snap:fail the checkbox draws light: rgb(" + [red, green, blue] + ")");
      log("dark page ok");`, { map_state: mapState() }),
    snapDone: "dark page ok",
    contrast: true,
  }),
  // Thin and thick fog (#455) under the contrast pass, with nothing over the plates: the text on each reads at full contrast, and
  // the two still differ by density, rim and drift.
  mapFogPlates: (): Fixture => ({
    ...mapOpened(`
      await wait(200);
      const [thin, thick] = ["thin", "thick"].map((kind) => document.querySelector("#map .plate.fog-" + kind));
      if (!thin || !thick) return log("snap:fail expected a thin and a thick plate");
      const mist = (plate) => getComputedStyle(plate.querySelector(".plate-fog"));
      const alpha = (plate) => Number(/,\\s*([\\d.]+)\\)$/.exec(mist(plate).backgroundColor)?.[1] ?? 1);
      if (!(alpha(thick) > alpha(thin))) log("snap:fail thick fog is not denser than thin: " + mist(thick).backgroundColor + " against " + mist(thin).backgroundColor);
      if (getComputedStyle(thick).boxShadow === getComputedStyle(thin).boxShadow) log("snap:fail thick fog has no rim of its own");
      if (mist(thick).animationName !== "mist-drift" || mist(thin).animationName !== "none") log("snap:fail only thick fog drifts: " + mist(thick).animationName + ", " + mist(thin).animationName);
      log("fog plates ok");`, { map_state: mapState({ fog: { shell: "thick", window: "thick", core: "thin", hook: "thin", understanding: "thin" } }) }),
    snapDone: "fog plates ok",
    contrast: true,
  }),
  // The Map view busy (#311): a session on two parts, a Needs you one, a Worker with its own chip, and the not placed list (no files yet,
  // agy, status-only Codex, ignored-only edits); a Helper and a finished session never show.
  mapBusy: () => mapBusy(false),
  // An open sheet keeps still while agents join and leave the not placed list (#472): the list is one line, "N not placed", whose
  // height is kept while a sheet is open, even at none; a click opens it, and it stays open across the next change.
  mapSheetStill: (): Fixture => {
    const idle = session("s7", "#55 · Delete default", "your_turn");
    const running = (id: string, name: string, state: "running" | "needs_you" = "running") => session(id, name, state, { turns: [] });
    const one = [idle, running("s3", "#312 · CI marks")];
    const three = [...one, running("s4", "#314 · Runs"), running("s6", "#313 · Docs", "needs_you")];
    const f = mapOpened(`
      const top = () => document.getElementById("map-sheet").getBoundingClientRect().top;
      const line = () => document.querySelector("#map-not-placed summary")?.textContent ?? "";
      const show = async (sessions) => { window.__snapEmit("sessions", sessions); await wait(150); };
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      if (!document.querySelector("#map-sheet .sheet")) return log("snap:fail the sheet did not open");
      const still = top();
      for (const [sessions, want] of [[${JSON.stringify(one)}, "1 not placed"], [${JSON.stringify(three)}, "3 not placed · 1 needs you"], [${JSON.stringify([idle])}, ""]]) {
        await show(sessions);
        if (line() !== want) log("snap:fail the not placed line says " + JSON.stringify(line()) + ", not " + JSON.stringify(want));
        if (top() !== still) log("snap:fail the sheet moved from " + still + " to " + top() + " at " + JSON.stringify(want || "none"));
      }
      await show(${JSON.stringify(three)});
      const chip = document.querySelector("#map-not-placed .agent-chip");
      if (!chip || chip.checkVisibility()) log("snap:fail the closed line shows its chips");
      document.querySelector("#map-not-placed summary").click();
      await wait(100);
      await show(${JSON.stringify(one)});
      const open = [...document.querySelectorAll("#map-not-placed .agent-chip")].filter((c) => c.checkVisibility()).map((c) => c.textContent);
      if (open.join() !== "#312 · CI marks") log("snap:fail the opened list did not stay open with its chips: " + open.join());
      // Emptied and filled again, it comes back closed, so it never drops the sheet by a whole list without a click.
      await show(${JSON.stringify([idle])});
      await show(${JSON.stringify(three)});
      if (document.querySelector("#map-not-placed details")?.open) log("snap:fail the list came back open after it emptied");
      if (top() !== still) log("snap:fail the sheet moved when the list came back: " + top());
      log("sheet still ok");`, { map_state: mapState(), map_place: {} });
    return { ...f, sessions: [idle], selected: { session: idle, agent: null }, snapDone: "sheet still ok" };
  },
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
  // The map from the keyboard (#460).
  mapPartKeys,
  mapPartRedraw,
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
  mapPanelNoSession,
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
  // The contrast pass (#455) runs here too.
  mapScrubbed: (): Fixture => ({ ...mapScrubbed(), contrast: true }),
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
  readmeHero,
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
  // The part sheet in the map: one scrolling column, a resizable width, and a click on the empty map closes it (#445).
  mapSheetScrollShort,
  // The sheet's purpose, then its held items with their key files (#483).
  mapSheetHolds,
  mapSheetWidth,
  mapSheetResizeRedraw,
  mapSheetKeptWidth,
  mapSheetClose,
  mapEmpty: () =>
    mapOpened(`
      const draft = document.querySelector("#map .map-empty .map-draft");
      if (!draft) return log("snap:fail no drafting prompt");
      if (!draft.textContent.includes("system-map.json") || !draft.textContent.includes("system-map check")) log("snap:fail the drafting prompt lacks the file or the check: " + draft.textContent);
      if (document.querySelector("#map .map-empty .btn")) log("snap:fail a button starts a session from the page");
      log("drafting prompt: " + draft.textContent.slice(0, 40));`, { map_state: mapState({ status: "none", source: null }) }),
} satisfies Record<string, () => Fixture>;

