// The map's left panel and a part's sheet beside it.

import type { Fixture } from "./base.ts";
import { briefOf, busyPlace, busySessions, mapFile, mapOpened, mapState, pairFile, panelChecks, session, storySteps } from "./fixtures-shared.ts";

/** The part sheet with one law not yet ratified (#343): it says so, with Accept and Overturn. */
export const mapSheetUnratified = (): Fixture =>
  mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const laws = [...document.querySelectorAll("#map-sheet .sheet-law")];
      const open = laws.filter((l) => l.textContent.includes("Not yet ratified"));
      if (laws.length !== 2 || open.length !== 1 || !open[0].textContent.includes("Notices wait for focus.")) return log("snap:fail laws: " + laws.map((l) => l.textContent).join("|"));
      if (!open[0].querySelector(".ratify-accept") || !open[0].querySelector(".ratify-overturn")) log("snap:fail no Accept and Overturn");
      if (!open[0].closest(".sheet-fold-laws")?.open) log("snap:fail the Laws fold kept open in this browser is folded");
      log("sheet ok");`, {
    map_state: mapState({ ratify: [{ pr: 306, kind: "law", id: "shell-notice" }] }),
    // Laws was left open in this browser (#637).
    storage: { "map-sheet-folds": JSON.stringify(["laws"]) },
  });
/**
 * The part sheet explains itself (#522): Shell's sheet, with links of three kinds and two not yet ratified, with Explanations on (the
 * default) and off, every fold left open in this browser (#637). On, each fold's summary is described by the line first in its body, the selected tier by its line, a kind's line follows its first link only and
 * "Not yet ratified"'s the first unratified link only, and the ? is pressed; off, no line is drawn and nothing points at one.
 */
export const mapSheetExplained = (on: boolean): Fixture =>
  mapOpened(`
      document.querySelector("#map .plate[data-id=shell]").click();
      await wait(300);
      const sheet = document.getElementById("map-sheet");
      const toggle = sheet.querySelector(".sheet-header .explain-toggle");
      if (!toggle || toggle.nextElementSibling !== sheet.querySelector(".sheet-header .sheet-close")) return log("snap:fail the ? is not beside the ×");
      if (toggle.getAttribute("aria-pressed") !== "${on}" || document.getElementById(toggle.getAttribute("interestfor"))?.textContent !== "${on ? "Hide" : "Show"} explanations") log("snap:fail the ?: " + toggle.getAttribute("aria-pressed") + " " + document.getElementById(toggle.getAttribute("interestfor"))?.textContent);
      const described = [...sheet.querySelectorAll("[aria-describedby]")].filter((node) => document.getElementById(node.getAttribute("aria-describedby"))?.classList.contains("explain"));
      const lines = [...sheet.querySelectorAll(".explain")];
      if (${!on}) {
        if (lines.length || described.length) log("snap:fail explanations drawn while off: " + lines.length + " lines, " + described.length + " described");
        return log("explained ok");
      }
      const summaries = [...sheet.querySelectorAll(".sheet-fold:not([hidden]) > .sheet-fold-summary")];
      if (summaries.length < 5) log("snap:fail the sheet shows " + summaries.length + " folds");
      for (const summary of summaries) {
        const line = document.getElementById(summary.getAttribute("aria-describedby"));
        if (!line?.classList.contains("explain") || summary.nextElementSibling?.firstElementChild !== line) log("snap:fail the fold's line: " + summary.textContent);
      }
      if (sheet.querySelector(".sheet-fold:not([open])")) log("snap:fail a fold left open in this browser is folded");
      // The Fog heading's line is the catch-up card's (#636), under its eyebrow.
      const eyebrow = sheet.querySelector(".catch-up-card .catch-up-eyebrow");
      const fogLine = eyebrow && document.getElementById(eyebrow.getAttribute("aria-describedby"));
      if (!fogLine?.classList.contains("explain") || eyebrow.nextElementSibling !== fogLine) log("snap:fail the catch-up card's line");
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
    storage: { "map-sheet-folds": JSON.stringify(["laws", "talks", "here", "holds", "tier", "ci", "history"]) },
    snapDone: "explained ok",
  });
const mapPanel = (check: string, extra: Fixture = {}): Fixture => {
  const sessions = busySessions().map((s) => (s.id === "s1" ? { ...s, turns: s.turns.map((t) => ({ ...t, id: "t1", message: "Chips on the parts a turn touches" })) } : s));
  const selected = Object.fromEntries(sessions.map((s) => [s.id, { session: s, agent: null }]));
  const f = mapOpened(`${panelChecks}
      await wait(300);
      checkAll("on the bare map");
      ${check}`, {
    map_state: mapState({ fog: { understanding: "thin" }, ready: [{ number: 334, title: "PR briefs on the map", head: "b".repeat(40), brief: { stale: false, writing: false, chapters: ["PR briefs on the map"], quiet: false } }] }),
    map_place: busyPlace(),
    select: { __by: "id", values: selected },
    projects: [{ path: "/w/plotkeeper", gone: false }],
    turn_file_hunk: { text: "@@ -261,7 +261,9 @@ function drawMap(\n   const saved = positions;\n-  const added = autoPlace(items, links);\n+  // Placed once and saved, so a part never moves on its own.\n+  const added = autoPlace(items, links, saved);\n", binary: false, left_out: 0, first_line: 261 },
    ...extra,
  });
  return { ...f, sessions: extra.sessions ?? sessions, selected: null, snapDone: "panel ok" };
};
// The footprint (#559): hovering #311's row brackets every part its turns of the last 7 days changed beside Understanding's thin fog, firm
// on its latest turn's parts (Core and Window, and the Shell its Worker changed) and faint on Understanding, from a turn 2 days ago; its
// 9-day-old turn adds nothing to Daemon. With no hover the open session's footprint shows; a hovered row that leaves the list (its
// session moved to History) gives it back; a session that touched no part says so in its row's hint.
export const mapFootprint = (): Fixture => {
  const day = 24 * 3600e3;
  const turn = (ago: number, paths: (string | [string, string])[]) => ({
    started_at: Date.now() - ago - 60e3, ended_at: Date.now() - ago, prompt: "", id: null, message: null, checks: [], listed: [],
    files: { files: paths.map((given) => { const [path, agent] = typeof given === "string" ? [given, null] : given; return { root: "/w/plotkeeper", path, from: null, added: 3, removed: 1, agent }; }), merged: 0 },
  });
  const sessions = busySessions().map((s) => (s.id === "s1" ? { ...s, turns: [turn(9 * day, ["daemon/old.rs"]), turn(2 * day, ["understanding/u.rs", "core/older.rs"]), ...s.turns] } : s));
  return mapPanel(`
      const marks = () => ["firm", "faint"].map((kind) => kind + ":" + [...document.querySelectorAll("#map .plate.foot-" + kind)].map((p) => p.dataset.id).sort().join()).join(" ");
      const hover = async (key, on = true) => { row(key).dispatchEvent(new PointerEvent(on ? "pointerenter" : "pointerleave")); await wait(150); };
      if (marks() !== "firm: faint:") log("snap:fail a footprint shows with no session hovered or open: " + marks());
      // A session that touched no part says so in its row's hint.
      const hintOf = (key) => document.getElementById(row(key).getAttribute("interestfor"))?.textContent ?? "";
      await hover("s3");
      if (!hintOf("s3").endsWith("\\nTouched no part in the last 7 days")) log("snap:fail #312's row does not say it touched no part: " + JSON.stringify(hintOf("s3")));
      if (hintOf("s1").includes("Touched no part")) log("snap:fail #311's row says it touched no part");
      await hover("s3", false);
      // The open session's footprint shows with no hover.
      await press(row("s2"));
      if (marks() !== "firm:hook faint:") log("snap:fail the open session's footprint: " + marks());
      // A hover replaces it, one footprint at a time; the pointer leaving gives it back.
      await hover("s1");
      if (marks() !== "firm:core,shell,window faint:understanding") log("snap:fail the hovered session's footprint: " + marks());
      await hover("s1", false);
      if (marks() !== "firm:hook faint:") log("snap:fail the open session's footprint did not come back: " + marks());
      // A hovered row that leaves the list never sticks: #312 ends, its row goes to History, and the open session's footprint comes back.
      await hover("s3");
      if (marks() !== "firm: faint:") log("snap:fail #312's empty footprint did not replace the open one: " + marks());
      __snapEmit("sessions", ${JSON.stringify(sessions)}.map((s) => (s.id === "s3" ? { ...s, state: "ended" } : s)));
      await wait(300);
      if (row("s3")) log("snap:fail #312's row stayed in the list after it ended");
      if (marks() !== "firm:hook faint:") log("snap:fail a hover whose row left the list stuck: " + marks());
      await hover("s1");
      const corner = getComputedStyle(document.querySelector("#map .plate[data-id=core]"), "::after");
      if (corner.content === "none" || corner.pointerEvents !== "none") log("snap:fail the brackets are not drawn, or take the pointer");
      log("footprint " + marks());
      log("panel ok");`, {
    sessions,
    map_place: { ...busyPlace(), "daemon/old.rs": ["daemon"], "understanding/u.rs": ["understanding"], "core/older.rs": ["core"] },
  });
};
// "This session here" (#561): #311's story open beside Core's sheet lists its two turns of the last 7 days that changed Core, newest first,
// each its number, Headline and age, under the section's explanation; Hook's says no turn changed it. With `older`, #311 also has a turn 9
// days old, so "Turns older than 7 days aren't listed" shows under the section. Pressing turn 2 unfolds it in the story, flashes its
// outline and moves the focus to it; the screenshot is that frame. With no story open the section is not there.
export const mapSheetHere = (older: boolean): Fixture => {
  const day = 24 * 3600e3;
  const turn = (ago: number, paths: string[], message: string) => ({
    started_at: Date.now() - ago - 60e3, ended_at: Date.now() - ago, prompt: "Draw it", id: null, message, checks: [], listed: [],
    files: { files: paths.map((path) => ({ root: "/w/plotkeeper", path, from: null, added: 3, removed: 1, agent: null })), merged: 0 },
  });
  const sessions = busySessions().map((s) => (s.id === "s1" ? { ...s, turns: [
    ...(older ? [turn(9 * day, ["daemon/old.rs"], "The daemon's old socket")] : []),
    turn(2 * day, ["core/older.rs", "understanding/u.rs"], "Per-path placement in core"),
    turn(5 * 3600e3, ["understanding/u.rs"], "Turn files read in understanding"),
    ...s.turns.map((t) => ({ ...t, message: "Chips on the parts a turn touches" })),
  ] } : s));
  const [newest, second] = older ? [4, 2] : [3, 1];
  const selected = Object.fromEntries(sessions.map((s) => [s.id, { session: s, agent: null }]));
  return mapPanel(`
      // The section is a fold (#637), its summary its heading.
      const here = () => document.querySelector("#map-sheet .sheet-fold-here");
      const rows = () => [...here().querySelectorAll(".sheet-here-turn")].map((b) => [...b.children].map((c) => c.textContent).join(" "));
      await plate("core");
      if (!sheetOpen()) return log("snap:fail the plate's click did not open its sheet");
      if (!here().hidden) return log("snap:fail This session here shows with no story open");
      await press(row("s1"));
      if (!panelOpen() || !sheetOpen()) return log("snap:fail the story and the sheet are not both open");
      if (here().hidden) return log("snap:fail This session here is not shown beside the open story");
      if (here().querySelector(".sheet-fold-title")?.textContent !== "This session here" || here().querySelector("h3")) log("snap:fail the section's heading");
      if (here().querySelector(".sheet-fold-body > .explain")?.textContent !== "The open session's turns that changed this part, newest first.") log("snap:fail the section carries no explanation line");
      const want = ["${newest} Chips on the parts a turn touches 10m", "${second} Per-path placement in core 2d"];
      if (JSON.stringify(rows()) !== JSON.stringify(want)) log("snap:fail the section's rows: " + JSON.stringify(rows()));
      const footer = here().querySelector(".sheet-here-older")?.textContent;
      if (${older} ? footer !== "Turns older than 7 days aren't listed" : footer !== undefined) log("snap:fail the footer: " + footer);
      await plate("hook");
      if (here().querySelector("p.empty")?.textContent !== "No turn of this session changed this part in the last 7 days" || rows().length) log("snap:fail Hook's empty section: " + here().textContent);
      await plate("core");
      here().querySelectorAll(".sheet-here-turn")[1].click();
      await wait(100);
      const feed = document.getElementById("feed");
      const target = feed.children[${newest} - ${second}];
      if (!target.classList.contains("flash") || feed.querySelectorAll(".turn.flash").length !== 1) log("snap:fail turn ${second} did not flash, alone");
      if (target.querySelector(".turn-line")?.getAttribute("aria-expanded") !== "true") log("snap:fail turn ${second} did not unfold");
      if (document.activeElement !== target.querySelector(".turn-line")) log("snap:fail the focus did not move to turn ${second}");
      // The section open and in view for the screenshot.
      here().open = true;
      await wait(50);
      here().scrollIntoView({ block: "start" });
      log("panel ok");`, {
    sessions,
    select: { __by: "id", values: selected },
    map_place: { ...busyPlace(), "daemon/old.rs": ["daemon"], "understanding/u.rs": ["understanding"], "core/older.rs": ["core"] },
  });
};
// A late placement (#549's final review): #311's story is open beside Hook's sheet when a new turn of it changes hook/late.rs, whose
// `map_place` answer is held back. While it is pending the section says nothing rather than "No turn of this session changed this part";
// once it lands the section lists the turn and the newest turn gets its "Show on map", with no other change. The shot is that frame.
export const mapLatePlace = (): Fixture => {
  const sessions = busySessions();
  const s1 = sessions[0];
  const late = { ...s1, turns: [...s1.turns, { started_at: Date.now() - 120e3, ended_at: Date.now() - 60e3, prompt: "Read the hook late", id: null, message: null, checks: [], listed: [], files: { files: [{ root: "/w/plotkeeper", path: "hook/late.rs", from: null, added: 3, removed: 1, agent: null }], merged: 0 } }] };
  return mapPanel(`
      // The section is a fold (#637), its summary its heading.
      const here = () => document.querySelector("#map-sheet .sheet-fold-here");
      const rows = () => [...here().querySelectorAll(".sheet-here-turn")].map((b) => b.querySelector(".sheet-here-number").textContent);
      const showOnMap = () => document.querySelector("#feed > li.turn .show-on-map");
      await plate("hook");
      await press(row("s1"));
      if (!panelOpen() || !sheetOpen() || here().hidden) return log("snap:fail the story and Hook's section are not both shown");
      if (!here().querySelector("p.empty")) log("snap:fail Hook's section does not say no turn changed it: " + here().textContent);
      // map_place's answer for the new path is held until released.
      let release;
      const held = new Promise((done) => (release = done));
      const fetched = window.fetch;
      window.fetch = async (url, init) => {
        if (!String(url).endsWith("/api/map_place")) return fetched(url, init);
        await held;
        return { ok: true, status: 200, json: async () => ({ ...${JSON.stringify(busyPlace())}, "hook/late.rs": ["hook"] }) };
      };
      __snapEmit("sessions", ${JSON.stringify(sessions)}.map((s) => (s.id === "s1" ? ${JSON.stringify(late)} : s)));
      __snapEmit("selected", { session: ${JSON.stringify(late)}, agent: null });
      await wait(300);
      if (here().querySelector("p.empty")) log("snap:fail a pending placement reads as no turn changed Hook");
      if (rows().length) log("snap:fail Hook's section lists rows before the placement came: " + rows());
      if (showOnMap()) log("snap:fail the newest turn has Show on map before its placement came");
      release();
      await wait(300);
      if (rows().join() !== "2") log("snap:fail the late placement did not reach Hook's section: " + JSON.stringify(rows()) + " " + here().textContent);
      if (!showOnMap()) log("snap:fail the late placement did not give the newest turn its Show on map");
      log("panel ok");`, { sessions });
};
// A story's turn on the map (#560): with #311's story open, hovering or focusing a turn lights its parts (a brief's ring, nothing dimmed,
// no camera move); leaving clears it, and so does a feed draw that swaps its node, while a draw that keeps the node keeps it (spec #661). Its 9-day-old turn says its files aren't shown and lights nothing; a turn with
// no files lights nothing; the newest has "Show on map"; under a brief nothing lights. It ends on the 2-day-old turn, hovered.
export const mapTurnLight = (): Fixture => {
  const day = 24 * 3600e3;
  const turn = (ago: number, paths: (string | [string, string])[], prompt: string) => ({
    started_at: Date.now() - ago - 60e3, ended_at: Date.now() - ago, prompt, id: null, message: null, checks: [], listed: [],
    files: { files: paths.map((given) => { const [path, agent] = typeof given === "string" ? [given, null] : given; return { root: "/w/plotkeeper", path, from: null, added: 3, removed: 1, agent }; }), merged: 0 },
  });
  const sessions = busySessions().map((s) => (s.id === "s1" ? { ...s, turns: [turn(9 * day, ["daemon/old.rs"], "Rework the daemon"), turn(2 * day, ["understanding/u.rs", "core/older.rs"], "Read the understanding"), turn(day, [], "Explain the fog"), ...s.turns] } : s));
  return mapPanel(`
      const lit = () => [...document.querySelectorAll("#map .plate.turn-lit")].map((p) => p.dataset.id).sort().join();
      const world = () => document.querySelector("#map .map-world").style.transform;
      const turns = () => [...document.querySelectorAll("#feed > li.turn")];
      const at = (i) => turns()[i];
      const hover = async (node, on = true) => { node.dispatchEvent(new PointerEvent(on ? "pointerenter" : "pointerleave")); await wait(30); };
      await press(row("s1"));
      if (!panelOpen()) return log("snap:fail #311's story did not open");
      // Its four turns, newest first, then the block of what started in no turn.
      if (turns().length < 4) return log("snap:fail #311's feed has " + turns().length + " turns, not 4");
      const camera = world();
      // The 2-day-old turn lights the parts it changed, nothing dims and the camera stays; leaving clears it.
      await hover(at(2));
      if (lit() !== "core,understanding") log("snap:fail the hovered turn lights " + JSON.stringify(lit()));
      const dimmed = [...document.querySelectorAll("#map .plate")].filter((p) => Number(getComputedStyle(p).opacity) < 1).map((p) => p.dataset.id);
      if (dimmed.length) log("snap:fail a turn's hover dimmed " + dimmed.join());
      if (world() !== camera) log("snap:fail a turn's hover moved the camera");
      const ring = getComputedStyle(document.querySelector("#map .plate[data-id=core]")).boxShadow;
      if (!ring.includes("0px 0px 0px")) log("snap:fail a lit part has no ring: " + ring);
      await hover(at(2), false);
      if (lit() !== "") log("snap:fail the light stayed after the pointer left: " + lit());
      // Keyboard focus lights it too, and moving the focus out clears it.
      at(2).querySelector(".turn-line").focus();
      await wait(30);
      if (lit() !== "core,understanding") log("snap:fail the focused turn lights " + JSON.stringify(lit()));
      at(2).querySelector(".turn-line").blur();
      await wait(30);
      if (lit() !== "") log("snap:fail the light stayed after the focus left: " + lit());
      // The newest turn lights its parts, its Worker's Shell included, and has "Show on map"; a turn with no files lights nothing.
      await hover(at(0));
      if (lit() !== "core,shell,window") log("snap:fail the newest turn lights " + JSON.stringify(lit()));
      await hover(at(0), false);
      if (!at(0).querySelector(".show-on-map")) log("snap:fail the newest turn has no Show on map");
      await hover(at(1));
      if (lit() !== "") log("snap:fail a turn with no files lights " + lit());
      await hover(at(1), false);
      // The 9-day-old turn, unfolded, says its files aren't shown and lights nothing.
      at(3).querySelector(".turn-line").click();
      await wait(100);
      const old = at(3).querySelector(".turn-old");
      if (old?.textContent !== "Files older than 7 days aren't shown on the map") log("snap:fail the old turn's line: " + JSON.stringify(old?.textContent));
      if (at(2).querySelector(".turn-old") || at(0).querySelector(".turn-old")) log("snap:fail a turn within 7 days says its files aren't shown");
      await hover(at(3));
      if (lit() !== "") log("snap:fail the old turn lights " + lit());
      await hover(at(3), false);
      at(3).querySelector(".turn-line").click();
      await wait(100);
      // A feed draw that keeps the hovered turn keeps its light; one that swaps it, as a new reply does, clears it.
      await hover(at(2));
      const hovered = at(2);
      __snapEmit("selected", { session: ${JSON.stringify(sessions[0])}, agent: null });
      await wait(50);
      if (at(2) !== hovered) log("snap:fail a draw of the same story swapped the hovered turn");
      if (lit() !== "core,understanding") log("snap:fail the light went with a draw that kept its turn: " + JSON.stringify(lit()));
      __snapEmit("selected", { session: ${JSON.stringify({ ...sessions[0], turns: sessions[0].turns.map((t, i) => (i === 1 ? { ...t, message: "Read the understanding again" } : t)) })}, agent: null });
      await wait(50);
      if (at(2) === hovered) log("snap:fail a draw after the turn's reply changed kept its node");
      if (lit() !== "") log("snap:fail the light stayed after its turn's node was swapped: " + lit());
      // Under a brief, a turn's hover lights nothing.
      document.querySelector('#map-tray .tray-row[data-pr="334"]').click();
      await wait(400);
      if (document.getElementById("map-brief").hidden) return log("snap:fail the brief did not open");
      await hover(at(2));
      if (lit() !== "") log("snap:fail a turn lit " + lit() + " under a brief");
      await hover(at(2), false);
      await esc();
      if (!document.getElementById("map-brief").hidden) return log("snap:fail Esc did not close the brief");
      await hover(at(2));
      log("turn lit " + lit());
      log("panel ok");`, {
    sessions,
    select: { __by: "id", values: Object.fromEntries(sessions.map((s) => [s.id, { session: s, agent: null }])) },
    pr_brief: briefOf(storySteps()),
    map_place: { ...busyPlace(), "daemon/old.rs": ["daemon"], "understanding/u.rs": ["understanding"], "core/older.rs": ["core"] },
  });
};
/**
 * The story patched in place (spec #661): with #311's story at your turn in the panel, its Headline's Ask box open and the question box
 * focused, a live update (a new step, a later event) keeps every turn, fold and button the node it was, the focus in the question box
 * and the panel's scroll; a real press on "What did this turn change?" right after it lands on that suggestion and asks it.
 */
export const mapPanelPatched = (): Fixture => {
  const ago = (ms: number) => Date.now() - ms;
  const turn = (n: number, at: number) => ({ started_at: ago(at + 60e3), ended_at: ago(at), prompt: "Prompt " + n, id: "p" + n, message: "Reply " + n + ".\nMore of reply " + n + ".", checks: [], listed: [], files: { files: [{ root: "/w/plotkeeper", path: "core/lib.rs", from: null, added: 3, removed: 1, agent: null }], merged: 0 } });
  const sessions = busySessions().map((s) => (s.id === "s1" ? { ...s, state: "your_turn" as const, turns: [turn(1, 900e3), turn(2, 600e3), turn(3, 300e3)], transcript: { ...s.transcript, responses: ["Reply 3.\nMore of reply 3."] } } : s));
  const [s1] = sessions;
  const later = { ...s1, last_event_at: Date.now(), last_step: { tool: "Read", detail: "core/lib.rs", command: null } };
  return mapPanel(`
      await press(row("s1"));
      if (!panelOpen()) return log("snap:fail #311's story did not open");
      const box = document.querySelector("#feed .now-turn .ask-about");
      if (!box) return log("snap:fail the newest turn's Headline has no Ask box");
      box.open = true;
      await wait(200);
      const pick = [...box.querySelectorAll(".ask-suggestions button")].find((b) => b.textContent === "What did this turn change?");
      if (!pick) return log("snap:fail the Ask box offers no 'What did this turn change?'");
      const field = box.querySelector("textarea");
      field.focus();
      const scroller = document.getElementById("story");
      scroller.scrollTop = Math.min(40, scroller.scrollHeight - scroller.clientHeight);
      const top = scroller.scrollTop;
      const nodes = [...document.querySelectorAll("#feed li, #feed details, #feed button")];
      const where = JSON.stringify(pick.getBoundingClientRect());
      const asked = [];
      const fetched = window.fetch;
      window.fetch = (url, init) => (String(url).endsWith("/api/ask_anchor") && asked.push(JSON.parse(init.body).question), fetched(url, init));
      __snapEmit("selected", { session: ${JSON.stringify(later)}, agent: null });
      const now = [...document.querySelectorAll("#feed li, #feed details, #feed button")];
      const swapped = nodes.filter((node, i) => node !== now[i]).length;
      if (swapped || now.length !== nodes.length) log("snap:fail the live update swapped " + swapped + " of the feed's " + nodes.length + " turns, folds and buttons");
      if (document.activeElement !== field) log("snap:fail the live update took the focus from the question box");
      if (scroller.scrollTop !== top) log("snap:fail the live update moved the story's scroll from " + top + " to " + scroller.scrollTop);
      if (!pick.isConnected || JSON.stringify(pick.getBoundingClientRect()) !== where) log("snap:fail the suggestion moved under the pointer");
      await __snapClick(pick);
      await wait(300);
      window.fetch = fetched;
      if (JSON.stringify(asked) !== JSON.stringify(["What did this turn change?"])) log("snap:fail the press after the update asked " + JSON.stringify(asked));
      log("panel ok");`, {
    sessions,
    select: { __by: "id", values: Object.fromEntries(sessions.map((s) => [s.id, { session: s, agent: null }])) },
    ask_anchor: "It changed core/lib.rs.",
    realPointer: true,
  });
};
/**
 * A side question stays with its turn (spec #661, Q7): with #311's story at your turn, a press on the newest Headline's "What did this
 * turn change?", then a new turn starts; the turn asked about is a past row, unfolded, and the answer is on screen under it.
 */
export const mapPanelAskStays = (): Fixture => {
  const ago = (ms: number) => Date.now() - ms;
  const turn = (n: number, at: number) => ({ started_at: ago(at + 60e3), ended_at: ago(at), prompt: "Prompt " + n, id: "q" + n, message: "Reply " + n + ".\nMore of reply " + n + ".", checks: [], listed: [], files: { files: [{ root: "/w/plotkeeper", path: "core/lib.rs", from: null, added: 3, removed: 1, agent: null }], merged: 0 } });
  const sessions = busySessions().map((s) => (s.id === "s1" ? { ...s, state: "your_turn" as const, turns: [turn(1, 900e3), turn(2, 600e3)], transcript: { ...s.transcript, responses: ["Reply 2.\nMore of reply 2."] } } : s));
  const [s1] = sessions;
  const next = { ...s1, state: "running" as const, last_event_at: Date.now(), turns: [...s1.turns, { ...turn(3, 0), ended_at: null, message: null }] };
  return mapPanel(`
      await press(row("s1"));
      if (!panelOpen()) return log("snap:fail #311's story did not open");
      const box = document.querySelector("#feed .now-turn .ask-about");
      if (!box) return log("snap:fail the newest turn's Headline has no Ask box");
      box.open = true;
      await wait(200);
      const pick = [...box.querySelectorAll(".ask-suggestions button")].find((b) => b.textContent === "What did this turn change?");
      if (!pick) return log("snap:fail the Ask box offers no 'What did this turn change?'");
      await __snapClick(pick);
      await wait(300);
      __snapEmit("selected", { session: ${JSON.stringify(next)}, agent: null });
      await wait(200);
      const past = document.querySelector("#feed").children[1];
      if (!past?.classList.contains("past")) return log("snap:fail the turn asked about is not a past row");
      if (past.querySelector(".turn-line")?.getAttribute("aria-expanded") !== "true") log("snap:fail the past row asked about is folded");
      const reply = past.querySelector(".ask-about .ask-reply");
      if (!reply?.textContent.includes("It changed core/lib.rs.")) return log("snap:fail the past row shows no answer: " + JSON.stringify(reply?.textContent));
      reply.scrollIntoView({ block: "nearest" });
      await wait(100);
      const [r, view] = [reply.getBoundingClientRect(), document.getElementById("story").getBoundingClientRect()];
      if (!r.height || r.top < view.top || r.bottom > view.bottom) log("snap:fail the answer is not on screen");
      log("panel ok");`, {
    sessions,
    select: { __by: "id", values: Object.fromEntries(sessions.map((s) => [s.id, { session: s, agent: null }])) },
    ask_anchor: "It changed core/lib.rs.",
    realPointer: true,
  });
};
/**
 * The rail with no session picked (#526): the story's session is picked no more while its panel shows, as when it is deleted elsewhere. The
 * rail's empty screen says what will show there, under "Pick a session in the list."; the panel closes with the selection (`followSelection`),
 * so since #385 that screen is not drawn and this checks its words, not its pixels.
 */
export const mapPanelNoSession = (): Fixture =>
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
/**
 * The story of Ship PR 286 in the left panel with Understanding's sheet open (both, in a wide window; below 1007 px the sheet takes the
 * panel's place, and the row's press takes it back).
 */
export const mapPanelStory = (): Fixture =>
  mapPanel(`
      await press(row("s2"));
      if (!panelOpen() || !document.querySelector("#panel-body > .rail")) return log("snap:fail a row's press did not open its story in the left panel");
      if (document.getElementById("map").hidden) return log("snap:fail the row's press left the Map view");
      if (document.querySelector("#map .plate[data-id=hook] .spot-chip") === null) log("snap:fail the open session's chip is not on its part");
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
      if (!narrow && document.querySelector("#map .plate[data-id=hook] .spot-chip") === null) log("snap:fail the open session's chip went out");
      log("panel ok");`);
/** Focus mode below 1007 px (#380's review): the open panel hides the legend and the Ready-for-review tray there as in the strip layout. */
export const mapPanelFocus = (): Fixture =>
  mapPanel(`
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Enter", key: "Enter", metaKey: true, shiftKey: true, bubbles: true }));
      await wait(300);
      if (document.body.dataset.layout !== "focus") return log("snap:fail ⌘⇧↩ did not enter focus mode: " + document.body.dataset.layout);
      if (!document.getElementById("map-tray").getClientRects().length) return log("snap:fail the tray is hidden with no panel open");
      await press(row("s2"));
      if (!panelOpen()) return log("snap:fail a row's press did not open its story in focus mode");
      for (const id of ["map-legend", "map-tray"]) if (narrow && document.getElementById(id).getClientRects().length) log("snap:fail #" + id + " shows beside the open panel below 1007 px in focus mode");
      log("panel ok");`);
/** Esc peels the sheet, then the panel, then nothing of the panel is left; a second press on the open row closes its story. */
export const mapPanelPeel = (): Fixture =>
  mapPanel(`
      await press(row("s2"));
      await press(row("s2"));
      if (panelOpen()) return log("snap:fail a second press on the same row did not close its story");
      if (document.querySelector("#map .spot-chip")) log("snap:fail the chip stays after the story closed");
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
/** `]` closes the selected session's story and opens it again: the session stays selected while it is closed. */
export const mapPanelToggle = (): Fixture =>
  mapPanel(`
      await press(row("s2"));
      const bracket = async () => { document.dispatchEvent(new KeyboardEvent("keydown", { code: "BracketRight", key: "]", bubbles: true })); await wait(300); };
      if (!panelOpen()) return log("snap:fail the row's press did not open its story before ]");
      await bracket();
      if (panelOpen()) log("snap:fail ] did not close the story");
      if (row("s2").getAttribute("aria-pressed") !== "true") log("snap:fail ] deselected the session");
      await bracket();
      if (!panelOpen() || document.getElementById("session-name").textContent !== "Ship PR 286") log("snap:fail ] did not open the story again");
      log("panel ok");`);
/**
 * `[` in a wide window (spec #658, user stories 29–31): the full list gives way to its 56 px strip and the map takes the width; a press on
 * the strip's ☰ opens the full list over the map; `[` again brings the full list back in its column, with no overlay class left and the
 * full list never fixed over the map; behind a modal dialog `[` does nothing. Shown with the list back. `mapListFoldNarrow`: below 1007 px `[` opens the list over the map and
 * closes it again, as ⌘1 did.
 */
export const mapListFold = (): Fixture =>
  mapPanel(`
      const bracket = async () => { document.dispatchEvent(new KeyboardEvent("keydown", { code: "BracketLeft", key: "[", bubbles: true })); await wait(300); };
      const full = document.querySelector("nav.list .full");
      const stripNode = document.getElementById("strip");
      const shows = (node) => node.getClientRects().length > 0;
      if (narrow) {
        if (document.body.dataset.layout !== "strip" || shows(full)) return log("snap:fail below 1007 px the list is not its strip");
        await bracket();
        if (!document.body.classList.contains("list-open") || !shows(full) || getComputedStyle(full).position !== "fixed") log("snap:fail below 1007 px [ did not open the list over the map");
        if (document.body.dataset.layout !== "strip") log("snap:fail below 1007 px [ changed the layout: " + document.body.dataset.layout);
        if (Math.round(full.getBoundingClientRect().width) !== 252) log("snap:fail the list over the map is " + full.getBoundingClientRect().width + " px wide, not 252");
        await bracket();
        if (document.body.classList.contains("list-open") || shows(full)) log("snap:fail below 1007 px [ again did not close the list");
        return log("panel ok");
      }
      const mapBox = () => document.getElementById("map").getBoundingClientRect();
      const wide = mapBox().width;
      if (document.body.dataset.layout !== "full" || !shows(full) || shows(stripNode)) return log("snap:fail the wide window does not open on the full list");
      // Behind a modal dialog, such as Delete's, a key without Cmd does nothing (spec #658's review).
      const dialog = document.getElementById("confirm-dialog");
      dialog.showModal();
      await bracket();
      if (document.body.dataset.layout !== "full") log("snap:fail [ behind a modal dialog folded the list");
      dialog.close();
      await wait(50);
      await bracket();
      if (document.body.dataset.layout !== "strip") log("snap:fail [ did not fold the list to its strip: " + document.body.dataset.layout);
      if (shows(full) || !shows(stripNode)) log("snap:fail [ left the full list or hid the strip");
      if (Math.round(document.querySelector("nav.list").getBoundingClientRect().width) !== 56) log("snap:fail the folded list is " + document.querySelector("nav.list").getBoundingClientRect().width + " px wide, not 56");
      if (!(mapBox().width > wide)) log("snap:fail the map did not take the list's width");
      const opened = mapBox();
      stripNode.querySelector(".open").click();
      await wait(300);
      if (!shows(full) || getComputedStyle(full).position !== "fixed") log("snap:fail the strip's press did not open the full list over the map");
      if (mapBox().width !== opened.width) log("snap:fail the list open over the map moved the map");
      if (Math.round(full.getBoundingClientRect().width) !== 252) log("snap:fail the list over the map is " + full.getBoundingClientRect().width + " px wide, not 252");
      await bracket();
      if (document.body.dataset.layout !== "full") log("snap:fail [ again did not bring the full list back: " + document.body.dataset.layout);
      const left = ["list-open", "list-peek"].filter((name) => document.body.classList.contains(name));
      if (left.length) log("snap:fail [ again left the overlay's classes: " + left.join());
      if (!shows(full) || shows(stripNode) || getComputedStyle(full).position === "fixed") log("snap:fail the full list is not back in its column");
      if (Math.round(mapBox().width) !== Math.round(wide)) log("snap:fail the map is " + mapBox().width + " px wide with the list back, not " + wide);
      // An overlay class left from a narrow window never fixes the full list over the map.
      document.body.classList.add("list-open");
      if (getComputedStyle(full).position === "fixed") log("snap:fail list-open fixes the full list over the map in the full layout");
      document.body.classList.remove("list-open");
      checkPlaces("after [");
      log("panel ok");`);
/** A turn's whole diff, widened, from the palette: it closes the part's sheet; its Back shows the story again. */
export const mapPanelDiff = (): Fixture =>
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
/** History in the left panel, from the list's link. */
export const mapPanelHistory = (): Fixture =>
  mapPanel(`
      document.querySelector("#history-link .history-link").click();
      await wait(300);
      if (!panelOpen() || panel.querySelector(".panel-title")?.textContent !== "History") return log("snap:fail the History link did not open History in the left panel");
      if (panel.querySelectorAll(".history .row").length !== 2) log("snap:fail History lists " + panel.querySelectorAll(".history .row").length + " ended sessions");
      // The tray hides while a left panel is open, at every width (#666), and comes back when it closes.
      const trayShown = () => document.getElementById("map-tray").getClientRects().length > 0;
      if (trayShown()) log("snap:fail the tray shows beside the open panel");
      checkAll("with History open");
      // Hovering an ended session's row spotlights its footprint; Esc closes History under the pointer, and the hover ends with its row.
      const firm = () => [...document.querySelectorAll("#map .plate.foot-firm")].map((p) => p.dataset.id).join();
      panel.querySelector('.history .row[data-key="e1 "]').dispatchEvent(new PointerEvent("pointerenter"));
      await wait(150);
      if (firm() !== "hook") log("snap:fail the History row's footprint: " + firm());
      await esc();
      if (panelOpen()) log("snap:fail Esc did not close History");
      if (!trayShown()) log("snap:fail the tray did not come back when History closed");
      if (firm() !== "") log("snap:fail the removed History row's spotlight stuck: " + firm());
      document.querySelector("#history-link .history-link").click();
      await wait(300);
      log("panel ok");`, {
  sessions: [...busySessions(), session("e1", "Map view drawer, #312", "ended", { last_event_at: Date.now() - 2 * 86400e3, turns: [{ started_at: Date.now() - 600e3, ended_at: Date.now() - 300e3, prompt: "", id: null, message: null, checks: [], listed: [], files: { files: [{ root: "/w/plotkeeper", path: "hook/src/main.rs", from: null, added: 3, removed: 1, agent: null }], merged: 0 } }] }), session("e2", "Wire check covers MapState, #333", "ended", { last_event_at: Date.now() - 3 * 86400e3 })],
});
/** Settings in the left panel from the top bar's gear, holding the writer section (#387); the gear again closes it. */
export const mapPanelSettings = (): Fixture =>
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
/**
 * The hints of fields and fold-outs (spec #582, ticket 2), with real input: the pointer on Settings' writer picker (a select) opens its
 * hint after the delay, beside the picker, and Esc closes it; Tab, then focus on the picker, opens it by keyboard; the pointer on the
 * story's "Steps only"-style fold-out header, a summary, opens its hint too, and a click on it still folds it.
 */
export const mapPanelFieldHints = (): Fixture => ({
  ...mapPanel(`
      const open = (hint) => hint.matches(":popover-open");
      const hintOf = (node) => document.getElementById(node.getAttribute("data-hint"));
      const beside = (hint, trigger) => {
        const [h, t] = [hint.getBoundingClientRect(), trigger.getBoundingClientRect()];
        const apart = Math.max(h.top - t.bottom, t.top - h.bottom);
        return apart >= 0 && apart <= 24 && h.right > t.left && h.left < t.right;
      };
      document.getElementById("settings-button").click();
      await wait(200);
      const pick = () => document.getElementById("writer-pick");
      if (!pick()) return log("snap:fail Settings has no writer picker");
      const pickHint = hintOf(pick());
      if (!pickHint?.matches('[popover="hint"]')) return log("snap:fail the picker has no hint");
      if (pick().hasAttribute("title")) log("snap:fail the picker kept its title");
      await __snapMove(pick());
      await wait(100);
      if (open(pickHint)) log("snap:fail the picker's hint was open before its delay");
      await wait(450);
      if (!open(pickHint)) return log("snap:fail the picker's hint did not open on hover");
      if (!beside(pickHint, pick())) log("snap:fail the picker's hint is not beside it " + JSON.stringify([pickHint.getBoundingClientRect(), pick().getBoundingClientRect()]));
      await __snapKey("Escape");
      await wait(50);
      if (open(pickHint)) log("snap:fail Esc left the picker's hint open");
      // Esc closes the open hint only: the map's layers stay as they were (spec #582, user story 5).
      if (!panelOpen()) return log("snap:fail Esc on an open hint also closed Settings");
      await __snapMove(document.body);
      await wait(150);
      await __snapKey("Tab");
      pick().focus();
      await wait(600);
      if (!open(pickHint)) log("snap:fail focus on the picker did not open its hint");
      if (document.activeElement !== pick()) log("snap:fail the hint moved the focus from the picker");
      await __snapKey("Escape");
      await wait(150);
      if (open(pickHint)) log("snap:fail Esc left the picker's hint open after focus");
      log("field hints ok");`),
  realKeys: true,
  realPointer: true,
  snapDone: "field hints ok",
});
/**
 * The hint of a fold-out header (spec #582, ticket 2), with real input: Tab, then focus on the map's "not placed"
 * line (a summary), opens its hint beside it without moving focus and Esc closes it; the pointer on it opens it too, and a click on the line
 * still folds it out.
 */
export const mapNotPlacedHint = (): Fixture => {
  const running = (id: string, name: string) => session(id, name, "running", { turns: [] });
  const sessions = [running("s3", "#312 · CI marks"), running("s4", "#314 · Runs")];
  const f = mapOpened(`
    const open = (hint) => hint.matches(":popover-open");
    const line = () => document.querySelector("#map-not-placed summary");
    window.__snapEmit("sessions", ${JSON.stringify(sessions)});
    await wait(150);
    const hintOf = (node) => document.getElementById(node.getAttribute("data-hint"));
    if (!line()) return log("snap:fail the map has no not placed line");
    const shown = hintOf(line());
    if (!shown?.matches('[popover="hint"]')) return log("snap:fail the not placed line has no hint");
    if (line().hasAttribute("title")) log("snap:fail the not placed line kept its title");
    await __snapKey("Tab");
    line().focus();
    await wait(600);
    if (!open(shown)) log("snap:fail focus on the not placed line did not open its hint");
    if (document.activeElement !== line()) log("snap:fail the hint moved the focus from the line");
    await __snapKey("Escape");
    await wait(150);
    if (open(shown)) log("snap:fail Esc left the hint open");
    line().blur();
    await __snapMove(line());
    await wait(600);
    if (!open(shown)) log("snap:fail the pointer on the not placed line did not open its hint");
    await __snapClick(line());
    await wait(100);
    if (!document.querySelector("#map-not-placed details")?.open) log("snap:fail a click on the line did not fold it out");
    log("not placed hint ok");`, { map_state: mapState(), map_place: {} });
  return { ...f, sessions: [], realKeys: true, realPointer: true, snapDone: "not placed hint ok" };
};
/**
 * The not placed list on the glass (#666): over Understanding's sheet, the closed line is one bare line, with no glass; opened, the list
 * has the sheet's glass, so no plate under it shows through; closed again, the sheet under it is where it was. The screenshot is it open.
 */
export const mapNotPlacedGlass = (): Fixture =>
  mapPanel(`
      const slot = document.getElementById("map-not-placed");
      const details = slot.querySelector("details");
      if (!details) return log("snap:fail the map has no not placed list");
      await plate("understanding");
      if (!sheetOpen()) return log("snap:fail the plate's click did not open its sheet");
      const look = (node) => { const style = getComputedStyle(node); return style.backgroundColor + " " + style.backdropFilter; };
      const glass = look(sheet);
      const sheetTop = sheet.getBoundingClientRect().top;
      if (getComputedStyle(slot).backdropFilter !== "none" || rgb(getComputedStyle(slot).backgroundColor)[3] !== 0) log("snap:fail the closed not placed line has the glass: " + look(slot));
      const line = details.querySelector("summary").getBoundingClientRect().height;
      if (slot.getBoundingClientRect().height !== line) log("snap:fail the closed not placed line is " + slot.getBoundingClientRect().height + " px tall, not its one line's " + line);
      details.open = true;
      await wait(200);
      if (look(slot) !== glass) log("snap:fail the open not placed list is not on the sheet's glass: " + look(slot) + ", not " + glass);
      checkAll("with the not placed list open");
      details.open = false;
      await wait(200);
      if (sheet.getBoundingClientRect().top !== sheetTop) log("snap:fail the sheet moved after the list closed: " + sheet.getBoundingClientRect().top + ", not " + sheetTop);
      details.open = true;
      await wait(200);
      log("panel ok");`);
/**
 * The toasts beside the side area (#666), in focus mode: the needs-you toast stacks against the side area's left edge, so it covers neither
 * the not placed line nor, once a sheet opens, the sheet's ×; widening the sheet moves it; with no side area it keeps the map's top-right
 * corner. The Clear toast keeps the bottom-left corner. The screenshot is the toast beside the widened sheet.
 */
export const mapToastBeside = (): Fixture => {
  const sessions = busySessions().map((s) => (s.id === "s4" ? { ...s, state: "your_turn" as const, landed: true } : s));
  return mapPanel(`
      const toast = document.getElementById("toast");
      const side = document.getElementById("map-side");
      const region = () => document.getElementById("toasts").getBoundingClientRect();
      const box = (node) => node.getBoundingClientRect();
      const over = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
      const round = (b) => [b.left, b.top, b.right, b.bottom].map(Math.round).join();
      const beside = (when) => {
        const [t, s] = [box(toast), box(side)];
        if (t.right > s.left || s.left - t.right > 40) log("snap:fail the toast is not against the side area's left edge " + when + ": " + round(t) + " beside " + round(s));
        if (Math.round(t.top) !== Math.round(region().top)) log("snap:fail the toast left the map's top " + when + ": " + round(t));
      };
      document.dispatchEvent(new KeyboardEvent("keydown", { code: "Enter", key: "Enter", metaKey: true, shiftKey: true, bubbles: true }));
      await wait(200);
      if (!toast.matches(":popover-open")) return log("snap:fail no needs-you toast in focus mode");
      const line = document.querySelector("#map-not-placed summary");
      if (!line || side.hidden) return log("snap:fail the side area does not show the not placed line");
      if (over(box(toast), box(line))) log("snap:fail the toast covers the not placed line: " + round(box(toast)) + " over " + round(box(line)));
      beside("with the not placed line");
      await plate("understanding");
      const close = sheet.querySelector(".sheet-close");
      if (!close) return log("snap:fail the plate's click did not open its sheet");
      if (over(box(toast), box(close))) log("snap:fail the toast covers the sheet's ×: " + round(box(toast)) + " over " + round(box(close)));
      beside("with the sheet");
      const was = box(toast).left;
      for (let i = 0; i < 6; i++) sheet.querySelector(".sheet-resize").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowLeft", bubbles: true }));
      await wait(100);
      if (box(toast).left >= was) log("snap:fail widening the sheet did not move the toast: " + was + " then " + box(toast).left);
      beside("with the sheet widened");
      // The Clear toast keeps the bottom-left corner.
      document.querySelector(".list .clear-landed").click();
      await wait(100);
      const clear = document.getElementById("clear-toast");
      if (!clear.matches(":popover-open")) return log("snap:fail Clear landed showed no toast");
      if (Math.round(box(clear).left) !== Math.round(region().left) || Math.round(box(clear).bottom) !== Math.round(region().bottom)) log("snap:fail the Clear toast left its corner: " + round(box(clear)) + " in " + round(region()));
      // With no side area, the toast keeps the map's top-right corner.
      __snapEmit("sessions", ${JSON.stringify(sessions)}.filter((s) => ["s1", "s2"].includes(s.id)));
      await esc();
      await wait(100);
      if (!side.hidden) return log("snap:fail the side area still shows");
      if (Math.round(box(toast).right) !== Math.round(region().right) || Math.round(box(toast).top) !== Math.round(region().top)) log("snap:fail with no side area the toast is not in the map's top-right corner: " + round(box(toast)) + " in " + round(region()));
      // The side area back, the toast moves out of its way again, for the screenshot.
      __snapEmit("sessions", ${JSON.stringify(sessions)});
      await plate("understanding");
      await wait(100);
      beside("with the side area back");
      log("panel ok");`, { sessions });
};
/** A PR brief over the story and the sheet: the one thing that dims the map, and both panels with it. */
export const mapPanelBrief = (): Fixture =>
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
export const mapSheetHolds = (): Fixture =>
  mapOpened(`
      const plate = document.querySelector("#map .plate[data-id=shell]");
      const plateHint = document.getElementById(plate.querySelector(".plate-open").getAttribute("interestfor"))?.textContent ?? "";
      if (!plateHint.endsWith("The program that holds the state, follows the event log and serves the page on 127.0.0.1.")) log("snap:fail the plate's hint is not the purpose: " + plateHint);
      if (plateHint.includes("Serving the page")) log("snap:fail the plate's hint holds the held items");
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
      if (list.closest(".sheet-fold")?.querySelector(".sheet-fold-value")?.textContent !== "3 things") log("snap:fail the Holds fold's value");
      // The glass's rise ends before the picture is taken.
      await wait(400);
      log("sheet holds ok");`, { map_state: mapState({ source: heldSheet }), storage: { "map-sheet-folds": JSON.stringify(["holds"]) } });
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
/**
 * At 1300x700 (`…Short`): the side column, not the sheet or the drawer, scrolls, so the sheet's last button and the drawer's end are reachable;
 * the drawer's code block keeps its own scroll (danielrispler/plotkeeper#443, issue 2a).
 */
export const mapSheetScrollShort = (): Fixture =>
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
      log("sheet scroll ok");`, { map_state: mapState({ source: longSheet }), map_proof: longProof, storage: { "map-sheet-folds": JSON.stringify(["laws", "talks", "here", "holds", "tier", "ci", "history"]) } });
/**
 * The sheet's width (#445): a real drag of its left edge, within 300 px and half the map, arrow keys on the focused handle, a double-click
 * back to 360, and the width kept in this browser.
 */
export const mapSheetWidth = (): Fixture => ({
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
/**
 * A redraw in the middle of a drag (#443 review): the server's `map-landed`, which #450 sends at every stage of a write, draws the sheet
 * again; the drag still resizes, since the handle stays in the page and keeps the pointer.
 */
export const mapSheetResizeRedraw = (): Fixture => ({
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
/** A width kept from an earlier visit opens the sheet at that width. */
export const mapSheetKeptWidth = (): Fixture => ({
  ...mapOpened(`${sheetHelpers}
      plate("shell").click();
      await wait(300);
      const width = Math.round(document.getElementById("map-side").getBoundingClientRect().width);
      if (Math.abs(width - 480) > 2) log("snap:fail the kept width 480 opened the sheet " + width + " px wide");
      await wait(400);
      log("kept width ok");`, { map_state: mapState() }),
  storage: { "map-sheet-width": "480" }, snapDone: "kept width ok",
});
/**
 * Closing the sheet (#445): a real click, or a press of 3 px, on the empty map closes the sheet and its drawer; a longer drag pans and keeps
 * them; a click on another part switches the sheet; × (named "Close") and Esc close it.
 */
export const mapSheetClose = (): Fixture => ({
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

/**
 * An open sheet keeps still while sessions join and leave the not placed list (#472): the list is one line, "N not placed", whose
 * height is kept while a sheet is open, even at none; a click opens it, and it stays open across the next change.
 */
export const mapSheetStill = (): Fixture => {
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
    const pick = document.querySelector("#map-not-placed .session-pick");
    if (!pick || pick.checkVisibility()) log("snap:fail the closed line shows its rows");
    document.querySelector("#map-not-placed summary").click();
    await wait(100);
    await show(${JSON.stringify(one)});
    const open = [...document.querySelectorAll("#map-not-placed .session-pick-name")].filter((c) => c.checkVisibility()).map((c) => c.textContent);
    if (open.join() !== "#312 · CI marks") log("snap:fail the opened list did not stay open with its rows: " + open.join());
    // Emptied and filled again, it comes back closed, so it never drops the sheet by a whole list without a click.
    await show(${JSON.stringify([idle])});
    await show(${JSON.stringify(three)});
    if (document.querySelector("#map-not-placed details")?.open) log("snap:fail the list came back open after it emptied");
    if (top() !== still) log("snap:fail the sheet moved when the list came back: " + top());
    log("sheet still ok");`, { map_state: mapState(), map_place: {} });
  return { ...f, sessions: [idle], selected: { session: idle, agent: null }, snapDone: "sheet still ok" };
};
/**
 * #571: the panel's close is ×, named Close, on its views' header line, within 1 px of the story's "⋯" and of the History, Settings and
 * diff headers' centre; the Ready-for-review tray beside it is on the sheet's glass, since the map pans under it.
 */
export const mapPanelHead = (): Fixture =>
  mapPanel(`
      const close = document.getElementById("panel-close");
      // The centre of a node's content box, so a title's padding does not count.
      const mid = (node) => {
        const box = node.getBoundingClientRect(), style = getComputedStyle(node);
        const top = box.top + parseFloat(style.paddingTop), bottom = box.bottom - parseFloat(style.paddingBottom);
        return (top + bottom) / 2;
      };
      const level = (node, view) => {
        if (!node) return log("snap:fail no header to line the × up with in " + view);
        const off = Math.round(mid(close) - mid(node));
        log(view + " off by " + off + " px");
        if (Math.abs(off) > 1) log("snap:fail the × is " + off + " px off the header line in " + view);
      };
      await press(row("s2"));
      if (close.textContent !== "×" || close.getAttribute("aria-label") !== "Close") log("snap:fail the panel's close is " + JSON.stringify(close.textContent) + " named " + close.getAttribute("aria-label"));
      level(document.getElementById("rail-more"), "the story");
      const tray = document.getElementById("map-tray");
      const style = getComputedStyle(tray);
      if (!style.backdropFilter.includes("blur") || style.backgroundColor !== getComputedStyle(panel).backgroundColor)
        log("snap:fail the tray is not on the glass: " + style.backgroundColor + " " + style.backdropFilter);
      document.querySelector("#history-link .history-link").click();
      await wait(300);
      level(panel.querySelector(".panel-title"), "History");
      await press(row("s1"));
      await palette("Open full diff");
      level(panel.querySelector(".turn-diff header > *"), "the diff");
      document.getElementById("settings-button").click();
      await wait(300);
      level(panel.querySelector(".panel-title"), "Settings");
      log("panel ok");`, {
  // An ended session, so the list has its History link; #311's turn has its id, as in mapPanel, for its diff.
  sessions: [...busySessions().map((s) => (s.id === "s1" ? { ...s, turns: s.turns.map((t) => ({ ...t, id: "t1", message: "Chips on the parts a turn touches" })) } : s)), session("e1", "Map view drawer, #312", "ended", { last_event_at: Date.now() - 2 * 86400e3 })],
});
/**
 * The story header's line 2 (spec #658): only the items a session has, one "·" between them. A running session with no phase, one that needs
 * you with no block kind, and one with neither nor a mode show no empty item, so never "Running ·" or "Running · · auto mode"; a session whose
 * estimate's lower bound has passed reads "est. up to 12m left", and the header's hover starts with that phase and estimate.
 */
export const mapPanelLine2 = (): Fixture => {
  const min = 60e3;
  const implementing = { kind: null, phases: [{ phase: "implementing" as const, at: Date.now() - 5 * min, agent_ms: 0 }], agent_ms: 0, estimate: { range: [0, 12 * min] as [number, number], depends_on_you: false } };
  const sessions = [
    session("s1", "#320 · Mode only", "running", { permission_mode: "auto" }),
    session("s2", "#321 · Needs you, no block", "needs_you", { permission_mode: "auto" }),
    session("s3", "#322 · Nothing else", "running"),
    session("s4", "#323 · Estimate passed", "running", { permission_mode: "auto", work: implementing }),
  ];
  return mapPanel(`
      const line = () => {
        const items = [...document.querySelectorAll("#headline .line-2 > *")];
        const stray = items.filter((node) => node.getClientRects().length && !node.textContent.trim()).map((node) => node.id);
        if (stray.length) log("snap:fail line 2 shows empty " + stray.join());
        return items.filter((node) => node.getClientRects().length).map((node) => node.textContent).join(" · ");
      };
      const want = { s1: "Running · auto mode", s2: "Needs you · auto mode", s3: "Running", s4: "Running · auto mode · Implementing · est. up to 12m left" };
      for (const [key, words] of Object.entries(want)) {
        await press(row(key));
        if (!panelOpen()) return log("snap:fail " + key + "'s press did not open its story");
        if (line() !== words) log("snap:fail " + key + "'s line 2 reads " + JSON.stringify(line()) + ", not " + JSON.stringify(words));
        const hover = document.getElementById(document.getElementById("headline").getAttribute("data-hint"))?.textContent ?? "";
        if (key === "s4" && !hover.startsWith("Implementing · est. up to 12m left · Session ")) log("snap:fail the header's hover reads " + JSON.stringify(hover));
        if (key !== "s4" && !hover.startsWith("Session ")) log("snap:fail " + key + "'s header hover reads " + JSON.stringify(hover));
      }
      log("panel ok");`, { sessions, select: { __by: "id", values: Object.fromEntries(sessions.map((s) => [s.id, { session: s, agent: null }])) } });
};
