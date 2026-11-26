// The Map view: its states, fog and marks, the camera, broken laws, the scrubbed history, the base and its projects.

import type { Ci, MapState } from "../../ui/types.ts";
import { base, type Fixture } from "./base.ts";
import { at, briefOf, busyPlace, busySessions, mapBrief, mapFile, mapOpened, mapState, pairFile, panelChecks, session, step, storySteps, withSaved } from "./fixtures-shared.ts";

const openGone = `
    named("pokedex").querySelector(".project-toggle").click();
    await wait(300);
    const gone = document.querySelector("#map .map-empty.map-gone");
    if (!gone) return log("snap:fail the gone project shows no gone state: " + document.getElementById("map-body").textContent);
    if (!gone.textContent.includes("This folder is gone") || !gone.textContent.includes("/w/pokedex")) log("snap:fail the gone state lacks its words or its path: " + gone.textContent);
    const remove = [...gone.querySelectorAll("button")].find((b) => b.textContent === "Remove project");
    if (!remove) return log("snap:fail the gone state has no Remove project");`;

/**
 * Staleness bands (#392): Core, Shell and Understanding untouched for 3, 6 and 12 weeks carry a band along their foot, firmer with each;
 * the Interface-only and Black box parts never do. Understanding's sheet says how long and offers the refresher.
 */
export const mapStale = (): Fixture =>
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
export const mapMarks = (kind: "thick" | "thin" | "stale" | "broken"): Fixture => {
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
      if (!marks.nextElementSibling?.classList.contains("catch-up-card")) log("snap:fail the catch-up card does not follow the marks");
      const text = marks.textContent;
      for (const want of ${JSON.stringify(want)}) if (!text.includes(want)) log("snap:fail the marks lack " + want + ": " + text);
      if (text.includes("wire-types-agree")) log("snap:fail a broken law is named");
      // Under the card (#637): the chips, then the folds, all folded but CI on main, which leads, open, when a run broke a law here.
      const card = marks.nextElementSibling;
      const after = [];
      for (let node = card.nextElementSibling; node; node = node.nextElementSibling) after.push(node.className);
      const shape = after.filter((name) => !name.startsWith("sheet-purpose") && !name.includes("sheet-more")).slice(0, 2).join("|");
      if (shape !== "sheet-chips|sheet-folds") log("snap:fail the card is not followed by the purpose, the chips and the folds: " + after.join("|"));
      const folds = [...document.querySelectorAll("#map-sheet .sheet-fold:not([hidden])")];
      const open = folds.filter((f) => f.open).map((f) => f.dataset.fold).join();
      if (open !== ${JSON.stringify(kind === "broken" ? "ci" : "")} || (${kind === "broken"} && folds[0].dataset.fold !== "ci")) log("snap:fail the folds: " + folds.map((f) => f.dataset.fold + (f.open ? "+" : "")).join());
      log("marks ${kind} ok");`, { map_state: mapState(states[kind]) });
};
/**
 * The refresher (#392) on Shell's sheet, a stale part with no fog: drawn at once from the map file, the flows through it hop by hop (the
 * sheet already shows the rest, its links as "Talks to", spec #473), with no writer and no "Writing…".
 */
export const mapRefresher = (): Fixture => {
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
      if (!node.parentElement?.classList.contains("catch-up-card")) log("snap:fail the refresher is not in the catch-up card");
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
/**
 * A part with no fog (#636): Shell's catch-up card, right under its caption, says "Caught up" and holds the Refresher; no Fog section.
 */
export const mapCaughtUp = (): Fixture =>
  mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const sheet = document.querySelector("#map-sheet .sheet");
      const card = sheet.querySelector(".catch-up-card");
      if (!card) return log("snap:fail no catch-up card");
      if (card.previousElementSibling?.className !== "sheet-caption") log("snap:fail the card is not under the caption: " + card.previousElementSibling?.className);
      if (!card.classList.contains("clear")) log("snap:fail the card is not the clear one");
      if (card.querySelector(".catch-up-eyebrow")?.textContent !== "Caught up") log("snap:fail the eyebrow: " + card.querySelector(".catch-up-eyebrow")?.textContent);
      if ([...card.querySelectorAll("button")].map((b) => b.textContent).join("|") !== "Refresher") log("snap:fail the card's buttons");
      if (card.querySelector(".catch-up-steps, .sheet-know")) log("snap:fail a clear part offers a catch-up");
      if ([...sheet.querySelectorAll(".sheet-heading")].some((h) => h.textContent === "Fog")) log("snap:fail the Fog section is still drawn");
      log("caught up ok");`, { map_state: mapState(), snapDone: "caught up ok" });
/**
 * A brief step that lights the second link of a pair (spec #473): the light runs along the pair's one line, from Understanding to Shell,
 * though the line is drawn from Shell, the end of the pair's first link.
 */
export const mapBriefPairLight = (): Fixture => {
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
/**
 * "Talks to" on Shell's sheet (spec #473): every link at either end, with its arrow, other end, kind and reason, and no other list of
 * links; the unratified run-time link carries Accept and Overturn on its row. On the map the Shell and Understanding pair is one line.
 */
export const mapTalksTo = (): Fixture =>
  mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const sheet = document.getElementById("map-sheet");
      // The sections are folds (#637): a summary's title is its heading.
      const headings = [...sheet.querySelectorAll(".sheet-fold-title")].map((h) => h.textContent);
      if (!headings.includes("Talks to") || headings.includes("Links") || headings.includes("Linked with")) log("snap:fail headings: " + headings.join("|"));
      if (sheet.querySelectorAll("ul.sheet-links").length !== 1) log("snap:fail not one list of links");
      const rows = [...sheet.querySelectorAll(".sheet-links .sheet-link")];
      if (rows.length !== 7) return log("snap:fail Talks to rows: " + rows.length);
      // Folded at first (#637); its chip opens it and brings it into view.
      const talks = sheet.querySelector(".sheet-fold-talks");
      if (talks.open) log("snap:fail Talks to is open before its chip is pressed");
      const chip = [...sheet.querySelectorAll(".sheet-chip")].find((b) => b.textContent === "7 links");
      if (!chip) return log("snap:fail no links chip: " + [...sheet.querySelectorAll(".sheet-chip")].map((b) => b.textContent).join("|"));
      chip.click();
      await wait(600);
      const side = document.getElementById("map-side").getBoundingClientRect(), at = talks.getBoundingClientRect();
      if (!talks.open) log("snap:fail the chip did not open Talks to");
      if (at.top < side.top || at.top > side.bottom) log("snap:fail Talks to is not in view: " + at.top + " in " + side.top + "–" + side.bottom);
      if (document.activeElement !== talks.querySelector("summary")) log("snap:fail the focus is not on Talks to's summary");
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
// The stop-rule review (#314, #384): Studio has run 20 active days and merged 25 PRs, and the Map view was engaged with on one of them, so
// the Map view fell below its threshold, and so did the history strip and "See the proof".
// Pre-flight (#394) was run twice and three explain-back lines (#397) were written, so the review lists only the modules of project 2.
const dueFacts = { active_days: 20, merged_prs: 25, engaged_days: 1, scrubs: 0, proofs: 0, knows: 0, dismissed: [], briefs_offered: 0, briefs_opened: 0, chapters_done: 0, counts: { "pre-flight-run": 2, "catch-up-done": 3, "explain-back-given": 3, "micro-world-opened": 2, "back-law-started": 1 }, began: "2026-10-01", stale_since: null };
export const mapBusy = (): Fixture => {
    const sessions = busySessions();
    const f = mapOpened(`
      const badge = (id) => document.querySelector("#map .plate[data-id=" + id + "] .session-badge");
      await wait(400);
      const counts = [...document.querySelectorAll("#map .plate .session-badge")].map((b) => b.closest(".plate").dataset.id + ":" + b.textContent).sort().join();
      // #311 counts once on each of its parts, its Worker's Shell included; Ship PR 286 needs you on Hook.
      if (counts !== "core:1,hook:1,shell:1,window:1") log("snap:fail the badges: " + counts);
      if (!badge("hook")?.matches(".needs-you") || badge("core")?.matches(".needs-you")) log("snap:fail only Hook's badge carries the Needs-you accent");
      if (document.querySelector("#map .spot-chip")) log("snap:fail a chip shows with no session spotlit");
      const rest = [...document.querySelectorAll("#map-not-placed .session-pick:not(.worker) .session-pick-name")].map((c) => c.textContent);
      if (rest.join("|") !== "#313 · Docs|#312 · CI marks|agy · Name from the prompt|codex · plotkeeper") log("snap:fail not placed: " + rest.join("|"));
      if (document.querySelector("#map-not-placed .session-pick.worker")) log("snap:fail the not placed list shows a Worker row");
      if ([...document.querySelectorAll("#map [interestfor], #map-not-placed [interestfor]")].some((node) => document.getElementById(node.getAttribute("interestfor"))?.textContent.includes("Read the log"))) log("snap:fail a Helper is on the map");
      log("badges ok, not placed " + rest.length);`, {
      map_state: mapState(),
      map_place: busyPlace(),
      select: { __by: "id", values: Object.fromEntries(sessions.map((s) => [s.id, { session: s, agent: null }])) },
    });
    return { ...f, sessions, selected: { session: sessions[6], agent: null } };
  };

// The badge fixtures' sessions (#562): the busy ones, with Ship PR 286 also on Core, #311 telling its Intention, and an unseen session on
// Core and Understanding, so Core's badge counts three and carries the accent.
const badgePlace = () => ({ ...busyPlace(), "understanding/u.rs": ["understanding"] });
const badgeSessions = () => {
  const touched = (message: string | null, ...paths: (string | [string, string])[]) => [{ started_at: Date.now() - 600e3, ended_at: Date.now() - 60e3, prompt: "", id: null, message, checks: [], listed: [], files: { files: paths.map((given) => { const [path, agent] = typeof given === "string" ? [given, null] : given; return { root: "/w/plotkeeper", path, from: null, added: 3, removed: 1, agent }; }), merged: 0 } }];
  const sessions = busySessions().map((s) => {
    if (s.id === "s1") return { ...s, transcript: { ...s.transcript, intention: "Drawing the session badges on the plates" } };
    if (s.id === "s2") return { ...s, turns: touched(null, "hook/src/main.rs", "core/lib.rs") };
    return s;
  });
  return [...sessions, session("s8", "#410 · Reading review", "your_turn", { unseen: true, turns: touched("Reviewed how the core reads a turn", "core/lib.rs", "understanding/u.rs") })];
};
const badgeFixture = (check: string, done: string): Fixture => {
  const sessions = badgeSessions();
  const f = mapOpened(`
      const badge = (id) => document.querySelector("#map .plate[data-id=" + id + "] .session-badge");
      const list = document.getElementById("map-badge-list");
      const listOpen = () => list.matches(":popover-open");
      const names = () => [...list.querySelectorAll(".session-pick")].map((r) => (r.matches(".worker") ? "↳ " : "") + r.querySelector(".session-pick-name").textContent);
      const sheetOpen = () => document.getElementById("map-sheet").childElementCount > 0;
      await wait(200);
      ${check}
      log(${JSON.stringify(done)});`, {
    map_state: mapState(),
    map_place: badgePlace(),
    select: { __by: "id", values: Object.fromEntries(sessions.map((s) => [s.id, { session: s, agent: null }])) },
  });
  return { ...f, sessions, selected: { session: sessions[6], agent: null }, realPointer: true, realKeys: true, snapDone: done };
};

// A badge with the Needs-you accent and its open list (#562): real presses, as a mouse and keys give them. The badge's press opens the list
// beside it and never the sheet; a row's press opens its story; Esc, a click outside and a zoom close it; the shot ends with Core's list open.
export const mapBadgeList = (): Fixture =>
  badgeFixture(`
      const sent = [];
      const fetched = window.fetch;
      window.fetch = (url, init) => {
        if (String(url).endsWith("/stop_log")) sent.push(JSON.parse(init.body).kind);
        return fetched(url, init);
      };
      if (badge("core")?.textContent !== "3" || !badge("core").matches(".needs-you")) return log("snap:fail Core's badge: " + badge("core")?.outerHTML);
      if (!document.getElementById(badge("core").getAttribute("interestfor"))?.textContent.includes("Ship PR 286") || badge("core").getAttribute("aria-label") !== "3 sessions on this part, 1 needs you") log("snap:fail Core's badge names: " + document.getElementById(badge("core").getAttribute("interestfor"))?.textContent + " / " + badge("core").getAttribute("aria-label"));
      await __snapClick(badge("core"));
      await wait(60);
      if (!listOpen()) return log("snap:fail the badge's press did not open its list");
      if (sheetOpen()) log("snap:fail the badge's press opened the plate's sheet");
      if (!sent.includes("map-action")) log("snap:fail the badge's press logged no map-action: " + sent);
      if (badge("core").getAttribute("aria-expanded") !== "true") log("snap:fail the open badge is not expanded");
      // Needs you first, then the session list's order.
      if (names().join("|") !== "Ship PR 286|#311 · Live agents|#410 · Reading review") log("snap:fail Core's list: " + names().join("|"));
      const states = [...list.querySelectorAll(".session-pick-state")].map((r) => r.textContent).join("|");
      if (states !== "Needs you|Running|Your turn, not seen") log("snap:fail the list's states: " + states);
      const lines = [...list.querySelectorAll(".session-pick-line")].map((r) => r.textContent).join("|");
      if (lines !== "Drawing the session badges on the plates|Reviewed how the core reads a turn") log("snap:fail the list's lines: " + lines);
      if (list.querySelectorAll(".session-pick .logo").length !== 3) log("snap:fail a row lacks its logo");
      const [b, l] = [badge("core").getBoundingClientRect(), list.getBoundingClientRect()];
      if (Math.abs(l.right - b.right) > 1 || l.top < b.bottom || l.top - b.bottom > 12) log("snap:fail the list is not under its badge: " + JSON.stringify([b, l]));
      // Esc closes the list alone, and the keyboard is back on the badge.
      await __snapKey("Escape");
      await wait(60);
      if (listOpen()) log("snap:fail Esc did not close the list");
      if (document.activeElement !== badge("core")) log("snap:fail after Esc the focus is on " + document.activeElement?.className);
      // A badge of one opens its list too, with its session's Worker on the part under it.
      await __snapClick(badge("shell"));
      await wait(60);
      if (names().join("|") !== "#311 · Live agents|↳ Map chips") log("snap:fail Shell's list: " + names().join("|"));
      // A second press closes it.
      await __snapClick(badge("shell"));
      await wait(60);
      if (listOpen()) log("snap:fail a second press on the badge left its list open");
      // A click outside closes it, and so does a zoom.
      await __snapClick(badge("core"));
      await wait(60);
      await __snapClick(document.querySelector("#map .actor"));
      await wait(60);
      if (listOpen()) log("snap:fail a click outside left the list open");
      await __snapClick(badge("core"));
      await wait(60);
      document.querySelector("#map .map-canvas").dispatchEvent(new WheelEvent("wheel", { deltaY: 40, clientX: 400, clientY: 300, bubbles: true, cancelable: true }));
      await wait(60);
      if (listOpen()) log("snap:fail a zoom left the list open");
      // A press on the open list's badge that is dragged off and let go elsewhere closes the list; the badge's next press, by the keyboard
      // too, opens it again.
      await __snapClick(badge("core"));
      await wait(60);
      await __snapDrag(badge("core"), 80, 0);
      await wait(100);
      if (listOpen()) log("snap:fail a press let go off the badge left the list open");
      badge("core").focus();
      await __snapKey("Enter");
      await wait(60);
      if (!listOpen()) log("snap:fail after a press let go off the badge, its next press did not open its list");
      await __snapClick(badge("core"));
      await wait(60);
      // Opening another badge's list from the keyboard while one is open leaves the first badge collapsed (outside review).
      await __snapClick(badge("core"));
      await wait(60);
      badge("shell").focus();
      await __snapKey("Enter");
      await wait(60);
      if (badge("core").getAttribute("aria-expanded") !== "false" || badge("shell").getAttribute("aria-expanded") !== "true") log("snap:fail a list opened over another left expanded: core " + badge("core").getAttribute("aria-expanded") + ", shell " + badge("shell").getAttribute("aria-expanded"));
      await __snapKey("Escape");
      await wait(60);
      // A row's press opens its story and closes the list, never the sheet.
      await __snapClick(badge("core"));
      await wait(60);
      await __snapClick(list.querySelector(".session-pick"));
      await wait(250);
      if (listOpen()) log("snap:fail the row's press left the list open");
      if (document.getElementById("session-name").textContent !== "Ship PR 286") log("snap:fail the row opened the story of " + JSON.stringify(document.getElementById("session-name").textContent));
      if (sheetOpen()) log("snap:fail the row's press opened the plate's sheet");
      document.getElementById("panel-close").click();
      await wait(150);
      await __snapClick(badge("core"));
      await wait(60);
      if (!listOpen()) log("snap:fail the list did not open again");`, "badge list ok");

// The spotlight's view (#562): hovering #311's row spotlights it. Its chip replaces the badge on Core, Window and Shell, with its line on Core alone (a tie of
// one file each goes to reading order); Understanding's badge dims and Hook's Needs-you one stays full. The shot keeps the hover.
export const mapSpotlight = (): Fixture =>
  badgeFixture(`
      const chips = () => [...document.querySelectorAll("#map .spot-chip")].map((c) => c.closest(".plate").dataset.id + ":" + c.querySelector(".spot-chip-name").textContent + (c.querySelector(".spot-chip-line") ? " · " + c.querySelector(".spot-chip-line").textContent : "")).sort().join("|");
      const row = document.querySelector('#groups .session-row[data-key="s1 "]');
      row.dispatchEvent(new PointerEvent("pointerenter"));
      await wait(150);
      if (chips() !== "core:#311 · Live agents · Drawing the session badges on the plates|shell:#311 · Live agents|window:#311 · Live agents") log("snap:fail the spotlit chips: " + chips());
      if (badge("core") || badge("window") || badge("shell")) log("snap:fail a badge shows beside the spotlit chip");
      if (!badge("understanding")?.matches(".dim")) log("snap:fail Understanding's badge is not dimmed");
      if (badge("hook")?.matches(".dim") || getComputedStyle(badge("hook")).opacity !== "1") log("snap:fail Hook's Needs-you badge dimmed");
      if (!(Number(getComputedStyle(badge("understanding")).opacity) < 0.5)) log("snap:fail the dimmed badge's opacity is " + getComputedStyle(badge("understanding")).opacity);
      // The hover ends: the badges come back undimmed.
      row.dispatchEvent(new PointerEvent("pointerleave"));
      await wait(150);
      if (chips() !== "" || badge("core")?.textContent !== "3" || document.querySelector("#map .session-badge.dim")) log("snap:fail the badges did not come back: " + chips());
      // Hovering a badge spotlights nothing.
      badge("core").dispatchEvent(new PointerEvent("pointerenter", { bubbles: true }));
      await wait(100);
      if (chips() !== "") log("snap:fail hovering a badge spotlit " + chips());
      // A badge list's row spotlights its session while hovered.
      await __snapClick(badge("understanding"));
      await wait(150);
      list.querySelector(".session-pick").dispatchEvent(new PointerEvent("pointerenter"));
      await wait(150);
      if (!chips().startsWith("core:#410 · Reading review")) log("snap:fail the badge list's row did not spotlight its session: " + chips());
      if (!listOpen()) log("snap:fail spotlighting from the list closed it");
      // Esc closes the list under the pointer: the hover ends with it.
      await __snapKey("Escape");
      await wait(100);
      if (chips() !== "") log("snap:fail the closed list's row stayed spotlit: " + chips());
      // A row's press from the keyboard opens its story, whose spotlit chip takes the badge's place: the keyboard lands on Core's button.
      badge("core").focus();
      await __snapKey("Enter");
      await wait(60);
      await __snapKey("Enter");
      await wait(300);
      if (document.getElementById("session-name").textContent !== "Ship PR 286") log("snap:fail Enter on the row opened the story of " + JSON.stringify(document.getElementById("session-name").textContent));
      if (!document.activeElement?.matches("#map .plate[data-id=core] .plate-open")) log("snap:fail after a row's Enter the focus is on " + (document.activeElement?.className || document.activeElement?.tagName) + ", not Core's button");
      document.getElementById("panel-close").click();
      await wait(100);
      document.activeElement?.blur();
      row.dispatchEvent(new PointerEvent("pointerenter"));
      await wait(150);`, "spotlight ok");

/**
 * The map from the keyboard (#460), at the busy fixture: each plate's open button, named by the part's name and caption, sits beside
 * its session badge in reading order; real Tab reaches a part and then its badge, never an actor; real Enter and Space open its sheet.
 */
export const mapPartKeys = (): Fixture => {
  const f = mapBusy();
  return {
    ...f,
    snapScript: (f.snapScript ?? "").replace(/log\("badges ok, not placed " \+ rest\.length\);/, `
      const press = async (key) => { await __snapKey(key); await wait(80); };
      const sheetPart = () => document.querySelector("#map-sheet .sheet")?.dataset.part;
      const plates = [...document.querySelectorAll("#map .plate[data-id]")];
      for (const plate of plates) {
        const open = plate.querySelector(":scope > button.plate-open");
        const words = [".plate-name", ".plate-caption"].map((at) => plate.querySelector(at)?.textContent ?? "").filter(Boolean);
        if (!open) { log("snap:fail " + plate.dataset.id + " has no open button"); continue; }
        if (plate.hasAttribute("role") || plate.hasAttribute("tabindex")) log("snap:fail the plate " + plate.dataset.id + " takes a role or a tabindex");
        if (open.parentElement !== plate.querySelector(".plate-chips")?.parentElement) log("snap:fail " + plate.dataset.id + "'s open button is not a sibling of its badge's slot");
        if (open.contains(plate.querySelector(".session-badge"))) log("snap:fail " + plate.dataset.id + "'s open button holds a badge");
        if (!words.length || words.some((word) => !open.textContent.includes(word))) log("snap:fail " + plate.dataset.id + "'s open button is not named " + words.join(", "));
      }
      // Reading order: each next button is a row lower (half a plate or more), or on the same row and to the right.
      const boxes = [...document.querySelectorAll("#map .plate-open")].map((b) => ({ id: b.closest(".plate").dataset.id, box: b.closest(".plate").getBoundingClientRect() }));
      boxes.slice(1).forEach(({ id, box }, i) => {
        const before = boxes[i].box;
        if (!(box.top - before.top >= box.height / 2 || (Math.abs(box.top - before.top) < box.height / 2 && box.left > before.left))) log("snap:fail " + id + " comes after " + boxes[i].id + " out of reading order");
      });
      // Real Tab from the first part's button walks the map: every part's button, then its session badge, and no actor.
      const first = document.querySelector("#map .plate-open");
      if (!first) return log("snap:fail the map has no open button");
      first.focus();
      const walked = [first];
      while (walked.length < 40) {
        await press("Tab");
        if (!document.activeElement?.closest("#map .map-world")) break;
        walked.push(document.activeElement);
      }
      const expected = [...document.querySelectorAll("#map .plate-open, #map .plate .session-badge")];
      const named = (node) => node.closest(".plate, .actor")?.dataset.id + (node.matches(".session-badge") ? " badge" : "");
      if (walked.map(named).join() !== expected.map(named).join()) log("snap:fail Tab walked " + walked.map(named).join() + ", not " + expected.map(named).join());
      if (walked.some((node) => node.closest(".actor"))) log("snap:fail Tab reached an actor");
      const core = document.querySelector("#map .plate[data-id=core] .plate-open");
      const at = walked.indexOf(core);
      if (at < 0 || !walked[at + 1]?.matches("#map .plate[data-id=core] .session-badge")) log("snap:fail Tab did not reach core's button and then its badge");
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

/**
 * A map redraw, here at a landed push that thins Core's fog, keeps the keyboard on the same part's button (#452's review), and then at
 * the window's focus on the same part's session badge (#562).
 */
export const mapPartRedraw = (): Fixture => {
  const f = mapBusy();
  return {
    ...f,
    map_state: { __by: "fetch", values: { true: mapState(), false: mapState({ fog: { core: "thin" } }) } } as never,
    snapScript: (f.snapScript ?? "").replace(/log\("badges ok, not placed " \+ rest\.length\);/, `
      const hook = document.querySelector("#map .plate[data-id=hook] > .plate-open");
      hook.focus();
      __snapEmit("map-landed", "/w/plotkeeper");
      await wait(300);
      const redrawn = document.activeElement;
      if (hook.isConnected) log("snap:fail the landed push did not draw the map again");
      if (!redrawn?.matches("#map .plate[data-id=hook] > .plate-open")) log("snap:fail after a map redraw focus is on " + (redrawn?.className || redrawn?.tagName) + ", not hook's button");
      // The keyboard on a part's session badge stays on that part's new badge (#562).
      const hookBadge = document.querySelector("#map .plate[data-id=hook] .session-badge");
      hookBadge.focus();
      // The window's focus reads the map again, which has Core's fog back.
      window.dispatchEvent(new Event("focus"));
      await wait(300);
      if (hookBadge.isConnected) log("snap:fail the window's focus did not draw the map again");
      if (!document.activeElement?.matches("#map .plate[data-id=hook] .session-badge")) log("snap:fail after a map redraw focus is on " + (document.activeElement?.className || document.activeElement?.tagName) + ", not hook's badge");
      // A redraw at a new commit asks map_place again, so the badges come after the map: the badge gets the keyboard back when they do.
      const fetched = window.fetch;
      window.fetch = async (url, init) => {
        const reply = await fetched(url, init);
        if (String(url).endsWith("/api/map_place")) await wait(150);
        if (!String(url).endsWith("/api/map_state")) return reply;
        const state = await reply.json();
        return { ...reply, json: async () => ({ ...state, commit: "f".repeat(40) }) };
      };
      const before = document.activeElement;
      window.dispatchEvent(new Event("focus"));
      await wait(500);
      if (before.isConnected) log("snap:fail the new commit did not draw the map again");
      if (!document.activeElement?.matches("#map .plate[data-id=hook] .session-badge")) log("snap:fail after a new commit's redraw focus is on " + (document.activeElement?.className || document.activeElement?.tagName) + ", not hook's badge");
      log("part redraw ok");`),
    snapDone: "part redraw ok",
  };
};

// A map whose default branch's CI failed in a test that backs a law of the shell (#312): the red mark beside a Needs-you badge, at the
// size of the busy fixture; `drawer` opens the proof of a law, `log` the broken-law drill-down.
const brokenCi: Ci = {
  status: "broken", run_url: "https://github.com/danielrispler/plotkeeper/actions/runs/9", commit: "35745b1693c5d061e5aea891e98a58515942a12d", earlier: false,
  parts: ["shell"], laws: ["wire-types-agree"], note: null, at: null,
  tail: "        FAIL [   0.013s] (4/4) studio-workbench::wire wire_types_match\n  stderr ───\n\n    thread 'wire_types_match' panicked at workbench/tests/wire.rs:11:25:\n    assertion `left == right` failed: wire drifted\n      left: 1\n     right: 2\n\n────────────\n     Summary [   0.014s] 4 tests run: 3 passed, 1 failed, 0 skipped",
};
export const mapBroken = (open: "none" | "sheet" | "proof" | "log"): Fixture => {
  const sessions = busySessions();
  const opened = {
    none: "",
    // The shell's sheet (#637): CI on main leads its folds, open, its value and its chip in the broken tone; the others stay folded.
    sheet: `document.querySelector("#map .plate[data-id=shell]").click();
      await wait(400);
      const folds = [...document.querySelectorAll("#map-sheet .sheet-fold:not([hidden])")];
      const order = folds.map((f) => f.querySelector(".sheet-fold-title").textContent + (f.open ? "+" : "")).join("|");
      if (order !== "CI on main+|Laws|Talks to|Tier|History") log("snap:fail the folds: " + order);
      const ci = folds[0];
      if (ci.querySelector(".sheet-fold-value").textContent !== "failing" || !ci.querySelector(".sheet-fold-value").classList.contains("bad")) log("snap:fail CI on main's value");
      if (!ci.querySelector(".sheet-ci.broken .sheet-ci-line")?.textContent.includes("A test that backs laws here failed on main")) log("snap:fail CI on main's line");
      if (![...ci.querySelectorAll("button")].some((b) => b.textContent === "See the CI log")) log("snap:fail no See the CI log");
      const chip = [...document.querySelectorAll("#map-sheet .sheet-chip")].find((b) => b.textContent === "CI failing");
      if (!chip?.classList.contains("bad")) log("snap:fail no CI failing chip in the broken tone");
      if (getComputedStyle(ci.querySelector(".sheet-fold-value")).color === getComputedStyle(folds[1].querySelector(".sheet-fold-value")).color) log("snap:fail the failing value reads as the others");
      // The glass's rise ends before the picture is taken.
      await wait(400);`,
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
      if (!document.querySelector("#map .session-badge.needs-you")) log("snap:fail no needs-you badge beside the mark");
      const red = getComputedStyle(document.querySelector("#map .plate .law-broken")).backgroundColor;
      const orange = getComputedStyle(document.querySelector("#map .session-badge.needs-you")).color;
      if (red === orange) log("snap:fail the broken mark and Needs you are one colour");
      ${opened}
      log("broken " + marked.join());`, {
    map_state: mapState({ ci: brokenCi }),
    map_place: busyPlace(),
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
export const mapScrubbed = (): Fixture => {
  const esc = `document.dispatchEvent(new KeyboardEvent("keydown", { code: "Escape", key: "Escape", bubbles: true }));
      await wait(100);`;
  const f = mapOpened(`
      await wait(200);
      if (!document.querySelector("#map .plate .law-broken") || !document.querySelector("#map .session-badge")) return log("snap:fail the live map lacks its mark or badges");
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(200);
      if (!document.querySelector("#map-sheet .catch-up-card")) return log("snap:fail the live sheet has no catch-up card");
      const rows = [...document.querySelectorAll("#map-sheet .hist-row")];
      if (rows.length !== 4) return log("snap:fail expected 4 strip rows, saw " + rows.length);
      if (rows.filter((r) => r.classList.contains("fogged")).length !== 2 || !rows[3].classList.contains("fogged") || rows[0].classList.contains("fogged")) log("snap:fail fogged rows");
      const offers = [...document.querySelectorAll("#map-sheet .hist-brief")];
      if (offers.length !== 1 || offers[0].closest(".hist-row") !== rows[3] || offers[0].textContent !== "Brief") return log("snap:fail only #306's row offers its brief, saw " + offers.length);
      rows[1].querySelector(".hist-commit").click();
      await wait(200);
      const banner = document.getElementById("map-banner");
      if (!banner.textContent.includes("Viewing 2222222") || !banner.textContent.includes("Back to now")) return log("snap:fail banner: " + banner.textContent);
      if (document.querySelector("#map .session-badge, #map .spot-chip, #map-not-placed .session-pick")) log("snap:fail a live session shows in the past");
      if (document.querySelector("#map .plate .law-broken")) log("snap:fail a broken-law mark shows in the past");
      if (document.querySelector("#map .plate.fog-thin, #map .plate.fog-thick")) log("snap:fail fog shows in the past");
      if (document.querySelector("#map .plate[data-id=workbench]")) log("snap:fail the workbench was not yet there");
      if (!document.querySelector("#map-sheet .sheet")) log("snap:fail the sheet closed on a scrub");
      if (document.querySelector("#map-sheet .catch-up-card")) log("snap:fail a past commit's sheet has a catch-up card");
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
    map_place: busyPlace(),
    map_history: [strip(1, "#101 Map first drawn", false), strip(2, "#150 Plates and links", false), strip(3, "#210 Fog engine", true), strip(4, "#306 Live agents on the map", true, 306)],
    map_at: { commit: "2".repeat(40), short: "2222222", label: "#150 Plates and links", source: JSON.stringify(past) },
    // History was left open in this browser (#637), so the strip shows.
    storage: { "map-sheet-folds": JSON.stringify(["history"]) },
    map_commit: { commit: "2".repeat(40), short: "2222222", label: "#150 Plates and links", url: "https://github.com/danielrispler/plotkeeper/commit/" + "2".repeat(40), files: [{ path: "src-tauri/src/map.rs", diff: "@@ -1,2 +1,2 @@\n-one\n+two" }, { path: "ui/map.ts", diff: "@@ -1 +1,2 @@\n+three" }] },
  });
  return { ...f, sessions: busySessions(), selected: { session: busySessions()[6], agent: null } };
};

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
  map_place: busyPlace(),
  ...extra,
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    await wait(500);
    ${baseChecks}
    ${check}`,
  snapDone: "base ok",
});
/** The bare map with the list: the page opens on the last viewed project's map, its name in the top bar, its header marked; Esc does nothing. */
export const mapBase = (): Fixture =>
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
export const readmeHero = (): Fixture =>
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
    map_place: busyPlace("server"),
  });
/**
 * A project with no sessions keeps its header, with the quiet dot drawn as the map's mist; its hover names what lit it. A click on the header
 * shows that project's map.
 */
export const mapHeaderDot = (): Fixture =>
  mapBaseOf(`
      const head = document.querySelector('#groups .project-head[data-project="/w/falinks"]');
      if (!head) return log("snap:fail falinks has no header");
      const dot = head.querySelector(".project-dot");
      if (!narrow) {
        if (!dot) return log("snap:fail falinks's header has no dot");
        const words = document.getElementById(dot.getAttribute("data-hint"))?.textContent;
        if (words !== "A Core part you track has changed\\nA PR brief is waiting" || dot.getAttribute("aria-label") !== words || dot.hasAttribute("title")) log("snap:fail the dot's hint says " + JSON.stringify(words));
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
/** A fresh install: no project, so the map's place holds one calm line and the install docs, and nothing covers the page. */
export const mapFresh = (): Fixture =>
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
/** A project without system-map.json keeps today's draft prompt in the map's place. */
export const mapNoMap = (): Fixture =>
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
/** A view moved on one project and, within the save's moment, on another: both are saved. */
export const mapSwitchSavesViews = (): Fixture =>
  switching(`
      const zoom = () => document.querySelector("#map .map-canvas").dispatchEvent(new WheelEvent("wheel", { deltaY: -100, ctrlKey: true, clientX: 700, clientY: 400, bubbles: true, cancelable: true }));
      zoom();
      await open("/w/falinks");
      zoom();
      await wait(700);
      const saved = calls.filter(([command]) => command === "map_save_view").map(([, args]) => args.project).sort().join();
      if (saved !== "/w/falinks,/w/plotkeeper") log("snap:fail the views saved were of " + JSON.stringify(saved));`);
/** A gone folder's empty state: the window's focus asks nothing about it. */
export const mapGoneFocus = (): Fixture =>
  switching(`
      await open("/w/pokedex");
      if (!document.querySelector("#map .map-gone")) return log("snap:fail pokedex shows no gone state");
      calls.length = 0;
      window.dispatchEvent(new Event("focus"));
      await wait(300);
      const asked = calls.filter(([command, args]) => command === "map_state" && args.project === "/w/pokedex");
      if (asked.length) log("snap:fail the focus read the gone folder's map " + asked.length + " times");`);
/** A PR brief open on one project's map closes when a project with no map shows. */
export const mapBriefSwitch = (): Fixture =>
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
export const mapDotsRetry = (): Fixture =>
  dots(`
      if (dot()) return log("snap:fail a dot shows though the read failed");
      answer(() => falinks);
      // A press on the shown project's header folds it, which draws the list again.
      document.querySelector('#groups .project-head[data-project="/w/plotkeeper"] .project-toggle').click();
      await wait(200);
      if (!dot()) log("snap:fail the failed read was never tried again");`, { map_marks: { __throw: "the server is busy" } as never });
export const mapDotsOrder = (): Fixture =>
  dots(`
      answer((n) => (n === 0 ? new Promise((done) => setTimeout(() => done(falinks), 400)) : []));
      window.dispatchEvent(new Event("focus"));
      await wait(50);
      window.dispatchEvent(new Event("focus"));
      await wait(700);
      if (dot()) log("snap:fail the older read's answer landed over the newer one's");`, { map_marks: [] });
/**
 * Small gaps (#462): a real Shift+Tab from a row reaches the row before's delete, hidden only by opacity; the fold and delete boxes are
 * at least 24×24; the drawer's jumps are links to a new tab; a real Tab reaches the drawer's code block, which has a name; and the panels,
 * overlays and dialogs keep their scroll to themselves.
 */
export const mapSmallGaps = (): Fixture => ({
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
    map_place: busyPlace(),
  }),
  realKeys: true, snapDone: "small gaps ok",
});

/** A saved project whose folder is gone, in the map's place: its words, its path and Remove project, never an error (#381). */
export const mapProjectGone = () => withSaved(openGone + `
  log("gone: " + gone.textContent);`);
/** Remove project on the gone state: its header leaves the list and the map gives way to another project. */
export const mapProjectRemoved = () => withSaved(openGone + `
  remove.click();
  await wait(300);
  if (named("pokedex")) log("snap:fail the removed project keeps its header");
  if (document.querySelector("#map .map-gone")) log("snap:fail the map still shows the removed project");
  log("removed; headers " + heads().map((head) => head.querySelector(".name").textContent).join());`);
/** The calm Map view: plates, law marks of both kinds, a black box part, actors, and the session list live beside it. */
export const mapCalm = () =>
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
    log("plates " + plates + ", marks " + checks + "/" + trusts);`, { map_state: mapState() });
/**
 * The map's camera (#383): an empty point of the map is the map itself (no layer over it), a drag there pans, a wheel and a pinch
 * zoom up to 200 % and never scroll the page, and ⌘0 fits the whole map; no part's place changes. Shown fitted.
 */
export const mapCamera = () =>
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
    log("camera ok: fitted at " + Math.round(fitted.scale * 100) + "% over " + shown.length + " items");`, { map_state: mapState() });
/** The Map view with fog (#310): thin and thick fog on Core and interface parts, and the part sheet of a thick one open. */
export const mapFog = () =>
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
    log("thin " + thin + ", thick " + thick);`, { map_state: mapState({ fog: { shell: "thick", window: "thick", core: "thin", hook: "thin" } }), map_know: { positions: {}, tiers: {} }, contrast: true });
/**
 * Thin and thick fog (#455) under the contrast pass, with nothing over the plates: the text on each reads at full contrast, and
 * the two still differ by density, rim and drift.
 */
export const mapFogPlates = (): Fixture => ({
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
});
/** Story 34: every card over the map, the left panel and the sheet and drawer alike, arrives by a 12 px slide with a fade. */
export const mapGlassMotion = () =>
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
    log("motion ok");`, { map_state: mapState(), snapDone: "motion ok" });
/** The one-time review under the Map view's head, listing the modules below their thresholds (#314). */
export const mapReview = () =>
  mapOpened(`
    const review = document.querySelector("#map-review .review");
    if (!review || document.getElementById("map-review").hidden) return log("snap:fail the review is not shown");
    if (!review.textContent.includes("Little used so far: the Map view, the history strip and See the proof")) log("snap:fail the review's words: " + review.textContent);
    if (/\\d/.test(review.querySelector(".review-text").textContent)) log("snap:fail the review shows a count");
    log("review ok");`, { map_state: mapState(), stop_facts: dueFacts });
export const mapEmpty = () =>
  mapOpened(`
    const draft = document.querySelector("#map .map-empty .map-draft");
    if (!draft) return log("snap:fail no drafting prompt");
    if (!draft.textContent.includes("system-map.json") || !draft.textContent.includes("system-map check")) log("snap:fail the drafting prompt lacks the file or the check: " + draft.textContent);
    if (document.querySelector("#map .map-empty .btn")) log("snap:fail a button starts a session from the page");
    log("drafting prompt: " + draft.textContent.slice(0, 40));`, { map_state: mapState({ status: "none", source: null }) });

/**
 * The map's marks show their hint to the real pointer (spec #582, ticket 4): a plate's law mark, a broken-law mark and a link's mark on
 * its line (an SVG shape) each open a hint beside them after the delay, and none is in the Tab order, or has a title.
 */
export const mapMarkHints = (): Fixture => {
  // A law on a link, which draws the mark on its line.
  const file = JSON.parse(mapFile);
  file.laws.push({ id: "shell-core-fed", on: "shell-core", backing: "check", text: "The shell tells the core only what the log says." });
  return {
  ...mapOpened(
    `
      const hintOf = (node) => document.getElementById(node.getAttribute("data-hint"));
      const beside = (hint, trigger) => {
        const [h, t] = [hint.getBoundingClientRect(), trigger.getBoundingClientRect()];
        const apart = Math.max(h.top - t.bottom, t.top - h.bottom);
        return apart >= -1 && apart <= 24 && h.right > t.left && h.left < t.right;
      };
      for (const [name, selector] of [["a law mark", "#map .plate .law-mark"], ["the broken-law mark", "#map .plate .law-broken"], ["a link's mark", "#map .map-links .link-mark"]]) {
        const mark = document.querySelector(selector);
        if (!mark) return log("snap:fail the map has no " + name);
        if (mark.getAttribute("role") !== "img" || !mark.getAttribute("aria-label")) log("snap:fail " + name + " is no role img with an aria-label");
        if (mark.hasAttribute("tabindex") || mark.hasAttribute("title")) log("snap:fail " + name + " takes the Tab order or has a title");
        const hint = hintOf(mark);
        if (!hint) return log("snap:fail " + name + " has no hint");
        await __snapMove(mark);
        await wait(570);
        if (!hint.matches(":popover-open")) return log("snap:fail " + name + "'s hint did not open on the pointer");
        if (hint.textContent !== mark.getAttribute("aria-label")) log("snap:fail " + name + "'s hint says " + JSON.stringify(hint.textContent));
        if (!beside(hint, mark)) log("snap:fail " + name + "'s hint is not beside it " + JSON.stringify([hint.getBoundingClientRect(), mark.getBoundingClientRect()]));
      }
      log("mark hints ok");`,
    { map_state: mapState({ source: JSON.stringify(file), ci: { ...brokenCi, at: "2026-11-24T10:00:00Z" } }) },
  ),
  realPointer: true,
  snapDone: "mark hints ok",
  };
};
