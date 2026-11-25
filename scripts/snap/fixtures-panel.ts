// The map's left panel and a part's sheet beside it.

import type { Fixture } from "./base.ts";
import { briefOf, busySessions, mapFile, mapOpened, mapState, pairFile, panelChecks, session, storySteps } from "./fixtures-shared.ts";

/** The part sheet with one law not yet ratified (#343): it says so, with Accept and Overturn. */
export const mapSheetUnratified = (): Fixture =>
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
/**
 * The part sheet explains itself (#522): Shell's sheet, with links of three kinds and two not yet ratified, with Explanations on (the
 * default) and off. On, each heading and the selected tier are described by their line, a kind's line follows its first link only and
 * "Not yet ratified"'s the first unratified link only, and the ? is pressed; off, no line is drawn and nothing points at one.
 */
export const mapSheetExplained = (on: boolean): Fixture =>
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
/** ⌘2 closes the selected session's story and opens it again: the session stays selected while it is closed. */
export const mapPanelToggle = (): Fixture =>
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
      const tray = document.getElementById("map-tray").getBoundingClientRect();
      if (tray.left < panel.getBoundingClientRect().right) log("snap:fail the tray is under the open panel");
      checkAll("with History open");
      log("panel ok");`, {
  sessions: [...busySessions(), session("e1", "Map view drawer, #312", "ended", { last_event_at: Date.now() - 2 * 86400e3 }), session("e2", "Wire check covers MapState, #333", "ended", { last_event_at: Date.now() - 3 * 86400e3 })],
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
      log("sheet scroll ok");`, { map_state: mapState({ source: longSheet }), map_proof: longProof });
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
 * An open sheet keeps still while agents join and leave the not placed list (#472): the list is one line, "N not placed", whose
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
};
