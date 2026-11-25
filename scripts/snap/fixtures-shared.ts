// The helpers more than one area's fixtures build on: sessions, the saved projects, the map file and state, the confirm dialog's check,
// a PR brief's steps and the opened map.

import type { BriefFigure, BriefStep, BriefView, MapState, PrBrief, Session, SessionState, Subagent } from "../../ui/types.ts";
import { base, type Fixture } from "./base.ts";

export const session = (id: string, name: string, state: SessionState, extra: Partial<Session> = {}): Session => {
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

export const subagent = (id: string, name: string): Subagent => ({
  id, parent: "session", kind: "worker", name, asked: null, agent_type: null, status: "running",
  started_at: Date.now() - 600e3, ended_at: null, last_step: null, step_span: null, result: null, failed: false, opened: false, faded: false, turn: null,
});


/** The list with a nested row (a session with subagents), the selected row and a landed one. */
export const list = () => {
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
export const withSaved = (snapScript: string, extra: Fixture = {}): Fixture => ({
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
/** The Map view (#309) on a map of plotkeeper's own shape, opened by its key. Positions are saved, as the prototype's Treatment C draws them. */
export const mapFile = JSON.stringify({
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
export const at = (x: number, y: number): [number, number] => [x, y];
export const mapState = (extra: Partial<MapState> = {}): MapState => ({
  project: "/w/plotkeeper", status: "loaded", source: mapFile, reference: "origin/main", commit: "35745b1693c5d061e5aea891e98a58515942a12d", note: null,
  user: { tiers: {}, positions: { core: at(620, 190), understanding: at(980, 290), plugins: at(300, 430), hook: at(580, 430), shell: at(880, 440), window: at(1180, 560), daemon: at(760, 690), workbench: at(480, 650), agents: at(110, 430), writer: at(1400, 190), github: at(250, 650) } },
  fog: {}, ci: null, ready: [], ratify: [], runs: { paused: null, runs: [] }, stale: {}, stale_why: {}, catch_ups: {},
  ...extra,
});
/**
 * The confirm dialog (#459) a destructive button opens: modal, named by its heading, Cancel focused, the confirm button saying `verb`, and
 * naming each of `lost`. Nothing reaches the server's `api` until it is confirmed: Esc and Cancel send nothing. `press` is the script that
 * presses the button, found anew each time, since a refresh redraws it.
 */
export const confirmFirst = (press: string, api: string, verb: string, lost: string[]) => `
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
/**
 * Background runs (#390) beside the map: a running run, the queue behind it, and an ended run that was denied a tool call. Cancel on the
 * running run asks first (#459), naming its worktree and changes; Cancel on a queued run and Dismiss on the ended one ask the server at once.
 */
export const runAt = Date.UTC(2026, 10, 23, 12, 4);
/**
 * The PR brief on the map (#339), played from the tray as in the prototype's treatment C: a story about the PR that fogged Shell, Understanding and
 * Window, with a light run along the links, a figure for the change inside Understanding, a quote, and what the checks left unconfirmed.
 */
export const step = (k: string, h: string, b: string, on: string[], extra: Partial<BriefStep> = {}): BriefStep => ({
  k, h, b, on, hops: [], law: null, covered_by: null, list: [], risks: [], fig: null, check: null, cites: [], flags: [], ...extra,
});
export const figure = (kind: string): BriefFigure =>
  kind === "states"
    ? { kind, frame: "terminal t1", items: [{ label: "old · deleted", tone: null, bad: null }, { label: "now · lost", tone: "lost", bad: true }, { label: "kept · running", tone: "live", bad: null }, { label: "gone · ended", tone: null, bad: null }], before: [], after: [] }
    : kind === "sequence"
      ? { kind, frame: null, items: [{ label: "Turn ends", tone: null, bad: null }, { label: "Ask for PRs", tone: null, bad: null }, { label: "Write the brief", tone: "live", bad: null }, { label: "Show in tray", tone: null, bad: null }], before: [], after: [] }
      : { kind, frame: null, items: [], before: ["asks every 5 min", "from a 5 s loop", "one PR number"], after: ["asks at a turn's end", "and on focus", "ready state, head"] };
export const brief = (steps: BriefStep[], extra: Partial<PrBrief> = {}): PrBrief => ({ title: "PR briefs on the map", overview: [], chapters: [{ behaviour: "story", parts: ["shell", "understanding", "window"], title: "PR briefs on the map", steps, flags: [] }], also: [], unseen: [], decisions: [], flags: [], ...extra });
export const storySteps = (): BriefStep[] => [
  step("Where", "A ready PR now brings its own brief to the map", "When a pull request is ready for review, the Shell asks the Understanding part to write what changed, and the Window plays it on the map.", ["shell", "understanding", "window"], {
    hops: [["shell-understanding", false], ["shell-window", false]], cites: [{ from: "the PR body", quote: "writes its brief when it becomes ready" }],
  }),
  step("Before", "The map showed fog but never said what changed", "Fog said that files had changed since you last looked. Clearing it only meant \u201cI know this\u201d, which taught nothing.", ["window"]),
  step("Now", "A new head only marks the brief stale", "A push to a ready PR marks its brief stale. It is written again when you open it, or when the same head is seen twice in a row.", ["shell", "understanding"], {
    hops: [["shell-understanding", false]], fig: figure("states"), cites: [{ from: "src-tauri/src/pr_briefs.rs", quote: "A new head marks it stale" }],
  }),
  step("Must stay true", "A part is not read until its chapter is", "Reading a chapter to its end is what clears the fog of the parts it lit.", ["window"], { law: "window-quiet" }),
];
export const briefOf = (steps: BriefStep[], partial = false, extra: Partial<PrBrief> = {}): BriefView => ({
  partial,
  brief: { version: 1, pr: 334, head: "b".repeat(40), written_at: 1, stale: false, model: "opus", brief: brief(steps, extra) },
});
/**
 * Opens the brief from the tray, goes `next` steps on, and checks the panel and the map.
 * Chrome runs a snap for about 2 s of virtual time after the page loads and what is left unrun is silent, so a `quick` brief waits the least it can.
 */
export const mapBrief = (view: BriefView, next: number, check = "", quick = false, pause = 500): Fixture => ({
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
/**
 * The map with a pair of ends joined twice (spec #473): Shell imports Understanding, and Understanding answers Shell over HTTP, with a
 * law on the run-time link. Each link has its kind, and the run-time ones their reasons.
 */
export const pairFile = (() => {
  const file = JSON.parse(mapFile);
  for (const link of file.links) link.kind = link.to === "writer" || link.to === "github" ? "http" : "import";
  file.links.find((link: { id: string }) => link.id === "shell-daemon").kind = "spawn";
  file.links.find((link: { id: string }) => link.id === "shell-daemon").reason = "The Shell starts the daemon that owns the terminals.";
  file.links.push({ id: "understanding-shell", from: "understanding", to: "shell", kind: "http", reason: "Understanding answers the Shell's summary requests." });
  file.laws.push({ id: "understanding-answers", on: "understanding-shell", backing: "trust", text: "Every request gets one answer." });
  return JSON.stringify(file);
})();
export const mapOpened = (check: string, extra: Fixture = {}): Fixture => ({
  ...list(),
  ...extra,
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    document.dispatchEvent(new KeyboardEvent("keydown", { code: "KeyM", key: "m", metaKey: true, shiftKey: true, bubbles: true }));
    await wait(400);
    if (document.getElementById("map").hidden) return log("snap:fail the map key did not open the Map view");
    ${check}`,
});

export const busySessions = () => {
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
/**
 * The left panel over the map (#385). Each script fails when a part's saved position changes as a panel opens or closes, when a colour other
 * than Needs you orange or broken-law red shows, when a card over the map lacks the shared glass class, or when a layer covers the map where
 * no card is. It runs at both sizes: below 1007 px only one overlay is open, so what it expects there differs.
 */
export const panelChecks = `
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
