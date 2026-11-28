// The Map view's pure logic (spec danielrispler/plotkeeper#306, ticket #309), run on the `tsc` output. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { actorSize, autoPlace, draftPrompt, draftProgress, draftTime, partStage, languagesLine, looseGroups, ignoredFolders, scanPrompt, dragMove, dragStart, dragThreshold, effectiveTier, escapeMap, layerOrder, parseMap, plateSize, plateTitle, rimPoint, scrubTickets, sheetOf, sheetMarks, narrowBelow, withPanel, withSheet, pressRow, panelSession, switchProject, talksTo, linkLines, readingOrder, markPoints, placement, lineTag, tagWord } from "../../dist/map-model.js";
import { cameraRig, revealPan } from "../../dist/map-camera.js";

/** @returns {import("../../dist/map-model.js").Part} */
const part = (id, extra = {}) => ({ id, name: id, caption: "", purpose: "", holds: [], tier: "core", paths: [], ...extra });
const overlaps = (a, b) => Math.abs(a.x - b.x) < plateSize.w && Math.abs(a.y - b.y) < plateSize.h;

test("auto-placement puts each part beside what it links to and clear of every other plate", () => {
  const items = [{ id: "core" }, { id: "hook" }, { id: "shell" }];
  const links = [{ from: "hook", to: "core" }, { from: "shell", to: "core" }];
  const placed = autoPlace(items, links, {});
  assert.deepEqual(Object.keys(placed).sort(), ["core", "hook", "shell"]);
  const [a, b, c] = [placed.core, placed.hook, placed.shell];
  assert.ok(!overlaps(a, b) && !overlaps(a, c) && !overlaps(b, c));
  // Linked parts sit near each other, not at the far edge of the world.
  assert.ok(Math.hypot(a.x - b.x, a.y - b.y) < 3 * plateSize.w);
});

test("auto-placement is stable: a part with a saved position never moves, and a new one is placed beside its link", () => {
  const items = [{ id: "core" }, { id: "hook" }];
  const saved = { core: { x: 500, y: 400 }, hook: { x: 100, y: 100 } };
  const links = [{ from: "new", to: "core" }];
  const added = autoPlace([...items, { id: "new" }], links, saved);
  assert.deepEqual(Object.keys(added), ["new"]);
  assert.ok(Math.hypot(added.new.x - 500, added.new.y - 400) < 2 * plateSize.w);
  // Asked again with its position saved, it places nothing, and the same input gives the same answer.
  assert.deepEqual(autoPlace([...items, { id: "new" }], links, { ...saved, ...added }), {});
  assert.deepEqual(autoPlace([...items, { id: "new" }], links, saved), added);
});

test("a part with no link goes near the parts of its zone, and never on top of a plate", () => {
  const saved = { a: { x: 300, y: 300 }, b: { x: 1200, y: 800 } };
  const items = [{ id: "a", zone: "left" }, { id: "b", zone: "right" }, { id: "n", zone: "right" }];
  const { n } = autoPlace(items, [], saved);
  assert.ok(Math.hypot(n.x - 1200, n.y - 800) < Math.hypot(n.x - 300, n.y - 300));
  assert.ok(!overlaps(n, saved.a) && !overlaps(n, saved.b));
});

test("many unlinked parts spread without overlapping and stay inside the world's corner", () => {
  const items = Array.from({ length: 12 }, (_, i) => ({ id: `p${i}` }));
  const placed = autoPlace(items, [], {});
  const all = Object.values(placed);
  assert.equal(all.length, 12);
  for (const [i, a] of all.entries()) {
    assert.ok(a.x >= plateSize.w / 2 && a.y >= plateSize.h / 2);
    for (const b of all.slice(i + 1)) assert.ok(!overlaps(a, b));
  }
});

test("a click that moves under the threshold never drags; past it the plate follows the pointer", () => {
  const start = dragStart("core", { x: 100, y: 100 }, { x: 500, y: 400 });
  const small = dragMove(start, { x: 100 + dragThreshold - 1, y: 100 });
  assert.equal(small.to, null);
  assert.equal(small.drag.moved, false);
  const big = dragMove(small.drag, { x: 100 + dragThreshold, y: 100 });
  assert.deepEqual(big.to, { x: 500 + dragThreshold, y: 400 });
  assert.equal(big.drag.moved, true);
  // Once dragging, returning near the start still moves it: the plate follows the pointer exactly.
  assert.deepEqual(dragMove(big.drag, { x: 101, y: 100 }).to, { x: 501, y: 400 });
  assert.ok(dragThreshold >= 5 && dragThreshold <= 6);
});

test("a link runs from rim to rim, not centre to centre", () => {
  const a = { x: 0, y: 0 };
  const b = { x: 1000, y: 0 };
  assert.deepEqual(rimPoint(a, plateSize, b), { x: plateSize.w / 2, y: 0 });
  assert.deepEqual(rimPoint(b, plateSize, a), { x: 1000 - plateSize.w / 2, y: 0 });
  // Straight down hits the bottom edge; a diagonal hits the nearer edge.
  assert.deepEqual(rimPoint(a, plateSize, { x: 0, y: 500 }), { x: 0, y: plateSize.h / 2 });
  const diagonal = rimPoint(a, plateSize, { x: 500, y: 500 });
  assert.ok(Math.abs(diagonal.y) <= plateSize.h / 2 + 0.001 && Math.abs(diagonal.x) <= plateSize.w / 2 + 0.001);
  assert.deepEqual(rimPoint(a, actorSize, { x: 0, y: -500 }), { x: 0, y: -actorSize.h / 2 });
  // Two plates at the same spot have nowhere to leave from; the centre is returned, not NaN.
  assert.deepEqual(rimPoint(a, plateSize, a), a);
});

test("Esc closes the overlays (a micro-world, the pre-flight, the patch review), then a badge's list, then the drawer, then the brief, then returns from a scrubbed commit to now, then closes the sheet, then the left panel, then does nothing (#386, #394, #398, #399, #562)", () => {
  const open = { scrubbed: true, drawer: true, brief: true, sheet: true, panel: true, "pre-flight": true, "patch-review": true, "micro-world": true, badges: true };
  const closed = [];
  const layers = Object.fromEntries(layerOrder.map((name) => [name, { open: () => open[name], close: () => { open[name] = false; closed.push(name); } }]));
  const presses = [];
  for (let i = 0; i < 10; i++) presses.push(escapeMap(layers));
  assert.deepEqual(closed, ["micro-world", "pre-flight", "patch-review", "badges", "drawer", "brief", "scrubbed", "sheet", "panel"]);
  assert.deepEqual(presses, ["micro-world", "pre-flight", "patch-review", "badges", "drawer", "brief", "scrubbed", "sheet", "panel", null]);
});

test("a project switch closes the PR brief and its popover, the pre-flight, the patch review, a micro-world and a badge's list, and leaves the other layers", () => {
  const open = Object.fromEntries(layerOrder.map((name) => [name, true]));
  const layers = Object.fromEntries(layerOrder.map((name) => [name, { open: () => open[name], close: () => {
    open[name] = false;
  } }]));
  switchProject(layers);
  assert.deepEqual(open, { "micro-world": false, "pre-flight": false, "patch-review": false, badges: false, drawer: true, popover: false, brief: false, scrubbed: true, sheet: true, panel: true });
});

// The left panel and the part's sheet (#385): what opens where.
/** @returns {import("../../dist/map-model.js").PanelContent} */
const story = (session) => ({ kind: "story", session });
const wide = 1900;
const narrow = 1000;

test("the window is narrow below 1007 px", () => {
  assert.equal(narrowBelow, 1007);
});

test("in a wide window the panel and the sheet stay open together", () => {
  const both = withPanel({ panel: null, sheet: "core" }, story("s1"), wide);
  assert.deepEqual(both, { panel: story("s1"), sheet: "core" });
  assert.deepEqual(withSheet({ panel: { kind: "history" }, sheet: null }, "hook", wide), { panel: { kind: "history" }, sheet: "hook" });
  assert.deepEqual(withPanel({ panel: story("s1"), sheet: "core" }, { kind: "settings" }, 1007), { panel: { kind: "settings" }, sheet: "core" });
});

test("below 1007 px only one overlay is open: the panel closes the sheet, and the sheet the panel", () => {
  assert.deepEqual(withPanel({ panel: null, sheet: "core" }, story("s1"), narrow), { panel: story("s1"), sheet: null });
  assert.deepEqual(withPanel({ panel: null, sheet: "core" }, { kind: "history" }, 1006), { panel: { kind: "history" }, sheet: null });
  assert.deepEqual(withSheet({ panel: story("s1"), sheet: null }, "core", narrow), { panel: null, sheet: "core" });
  // Closing one never opens the other.
  assert.deepEqual(withPanel({ panel: story("s1"), sheet: null }, null, narrow), { panel: null, sheet: null });
});

test("the widened diff closes the part's sheet, and a sheet opened over it closes the diff", () => {
  /** @type {import("../../dist/map-model.js").PanelContent} */
  const diff = { kind: "diff", session: "s1", turn: 2 };
  assert.deepEqual(withPanel({ panel: story("s1"), sheet: "core" }, diff, wide), { panel: diff, sheet: null });
  assert.deepEqual(withSheet({ panel: diff, sheet: null }, "hook", wide), { panel: null, sheet: "hook" });
});

test("a row's press opens its session's story in the panel, and a second press on the same row closes it", () => {
  assert.deepEqual(pressRow(null, "s1"), story("s1"));
  assert.equal(pressRow(story("s1"), "s1"), null);
  assert.deepEqual(pressRow(story("s1"), "s2"), story("s2"));
  // From History, the diff or Settings, a row's press shows its story.
  assert.deepEqual(pressRow({ kind: "history" }, "s1"), story("s1"));
  assert.deepEqual(pressRow({ kind: "diff", session: "s1", turn: 0 }, "s1"), story("s1"));
});

test("the session spotlit while the panel is open is the one the panel shows", () => {
  assert.equal(panelSession(story("s1")), "s1");
  assert.equal(panelSession({ kind: "diff", session: "s2", turn: 0 }), "s2");
  assert.equal(panelSession({ kind: "history" }), null);
  assert.equal(panelSession(null), null);
});

test("Esc skips a layer that is not open, and a layer nobody registered", () => {
  const open = { drawer: false, sheet: true };
  const layers = { drawer: { open: () => open.drawer, close: () => assert.fail("closed a layer that was not open") }, sheet: { open: () => open.sheet, close: () => (open.sheet = false) } };
  assert.equal(escapeMap(layers), "sheet");
  assert.equal(escapeMap(layers), null);
  assert.equal(escapeMap({}), null);
});

test("Daniel's tier beats the map's, and a part with no choice keeps the map's", () => {
  assert.equal(effectiveTier(part("core"), { core: "black-box" }), "black-box");
  assert.equal(effectiveTier(part("hook", { tier: "interface" }), { core: "black-box" }), "interface");
  assert.equal(effectiveTier(part("x"), { x: "not-a-tier" }), "core");
});

test("a map that reads gives its parts, actors, links and laws; one that does not gives a reason", () => {
  const text = JSON.stringify({ version: 1, parts: [part("core")], actors: [{ id: "w", name: "Writer" }], links: [{ id: "l", from: "core", to: "w" }], ignored: [], laws: [{ id: "x", on: "core", backing: "check", text: "t" }], future: 1 });
  const map = parseMap(text);
  assert.equal(map.ok, true);
  assert.deepEqual([map.parts.length, map.actors.length, map.links.length, map.laws.length], [1, 1, 1, 1]);
  assert.equal(parseMap("{ nope").ok, false);
  assert.equal(parseMap('{"version":1}').ok, false);
  // #548: a file that is JSON but no object is a broken map with its reason, never a throw.
  for (const text of ["null", "[]", "3", '"parts"', "true"]) {
    const read = parseMap(text);
    assert.equal(read.ok, false, text);
    assert.equal(read.why, "the map is not a JSON object", text);
  }
  // #554: an entry that is not what its list holds, such as a part with no id or a link with no end, is a broken map, never drawn.
  const good = { parts: [part("core")], actors: [], links: [], laws: [] };
  for (const bad of [{ parts: [null] }, { parts: [{ name: "No id" }] }, { links: [{ id: "l", from: "core" }] }, { actors: ["w"] }, { laws: [{ id: "x" }] }]) {
    const read = parseMap(JSON.stringify({ ...good, ...bad }));
    assert.equal(read.ok, false, JSON.stringify(bad));
    assert.equal(read.why, "the map lacks its parts, actors, links or laws", JSON.stringify(bad));
  }
});

test("the plate's size here is the stylesheet's, so rim points land on the drawn edge", () => {
  const tokens = fs.readFileSync(new URL("../tokens.css", import.meta.url), "utf8");
  for (const [name, value] of [["--plate-w", plateSize.w], ["--plate-h", plateSize.h], ["--actor-w", actorSize.w], ["--actor-h", actorSize.h]]) {
    assert.match(tokens, new RegExp(`${name}: ${value}px`));
  }
  assert.match(tokens, /--mist: #c4ccd6/);
});

test("Draft a map's prompt names the map file and the command that checks it", () => {
  assert.match(draftPrompt, /system-map\.json/);
  assert.match(draftPrompt, /system-map check/);
  assert.match(draftPrompt, /purpose/);
  assert.match(draftPrompt, /holds/);
  assert.match(draftPrompt, /Add the extractors its `note:` lines suggest to `extractors`\./);
});

test("a part's sheet gives its purpose, then each held item with its key files, and its plate's hover text the purpose", () => {
  const holds = [{ name: "Adapters", text: "One per agent.", paths: ["core/src/claude.rs", "core/src/agy.rs"] }, { name: "Rules", text: "What must hold." }];
  const core = part("core", { name: "Core", caption: "Events to state", purpose: "Plain Rust.", holds });
  const sheet = sheetOf(core, [], "core", undefined);
  assert.equal(sheet.purpose, "Plain Rust.");
  assert.deepEqual(sheet.holds, [
    { name: "Adapters", text: "One per agent.", files: ["core/src/claude.rs", "core/src/agy.rs"] },
    { name: "Rules", text: "What must hold.", files: [] },
  ]);
  assert.equal(plateTitle(core), "Core\n\nEvents to state\n\nPlain Rust.");
});

test("a part's sheet names it, lists its laws by backing, and offers the three tiers with the effective one pressed", () => {
  /** @type {import("../../dist/map-model.js").Law[]} */
  const laws = [
    { id: "l1", on: "core", backing: "check", text: "Reads no file." },
    { id: "l2", on: "core", backing: "trust", text: "Applies in order." },
    { id: "l3", on: "hook", backing: "check", text: "Not this part's." },
  ];
  const sheet = sheetOf(part("core", { name: "Core", caption: "Events to state", purpose: "Plain Rust." }), laws, "interface", undefined);
  assert.deepEqual([sheet.name, sheet.caption, sheet.purpose], ["Core", "Events to state", "Plain Rust."]);
  assert.deepEqual(sheet.tiers, [
    { tier: "core", label: "Core", pressed: false },
    { tier: "interface", label: "Interface-only", pressed: true },
    { tier: "black-box", label: "Black box", pressed: false },
  ]);
  assert.deepEqual(sheet.laws.map((l) => [l.id, l.label]), [["l1", "Backed by a check"], ["l2", "On trust"]]);
});

test("a sheet says how the part's fog stands and offers I know this only while it shows fog", () => {
  const core = part("core");
  const line = (tier, fog) => sheetOf(core, [], tier, fog);
  assert.equal(line("core", undefined).canClear, false);
  assert.match(line("core", undefined).fogLine, /^Clear/);
  assert.equal(line("core", "thin").canClear, true);
  assert.match(line("core", "thin").fogLine, /files/i);
  assert.match(line("core", "thick").fogLine, /laws, links or flows/);
  assert.notEqual(line("core", "thin").fogLine, line("core", "thick").fogLine);
  assert.match(line("black-box", undefined).fogLine, /never/i);
  assert.match(line("interface", undefined).fogLine, /only when/i);
});

test("a sheet's marks explain themselves: the newest thick fog entry with its PR and why, staleness by the server's line, a broken law unnamed", () => {
  const entry = (commit, strength, pr, why) => ({ commit: commit.repeat(40), strength, pr, why });
  const entries = [entry("1", "thick", 410, ["law wire-types-agree", "paths"]), entry("2", "thin", 412, ["3 files"]), entry("3", "thin", null, ["1 file"])];
  assert.deepEqual(sheetMarks("core", "thick", entries, undefined, null), {
    fog: { line: "Thick fog from #410: law wire-types-agree, paths", more: 2 },
    stale: null,
    broken: null,
  });
  // Thin fog: the newest entry; one with no PR names its commit.
  assert.deepEqual(sheetMarks("core", "thin", entries.slice(1), undefined, null).fog, { line: "Thin fog from commit 3333333: 1 file", more: 1 });
  // An entry filed before #495 has neither PR nor why: its strength and commit.
  // @ts-expect-error an entry of an older shape, without the fields #495 added
  assert.deepEqual(sheetMarks("core", "thin", [{ commit: "4".repeat(40), strength: "thin" }], undefined, null).fog, { line: "Thin fog from commit 4444444", more: 0 });
  // A part whose fog does not show at its tier says nothing of it.
  assert.equal(sheetMarks("core", undefined, entries, undefined, null).fog, null);
  const line = "3 weeks or more: not engaged since tracking began on 2026-10-01";
  assert.equal(sheetMarks("core", undefined, [], line, null).stale, line);
  /** @type {import("../../dist/types.js").Ci} */
  const ci = { status: "broken", run_url: null, commit: null, earlier: false, parts: ["core"], laws: ["wire-types-agree"], tail: "", note: null, at: "2026-11-24T10:00:00Z" };
  const broken = sheetMarks("core", undefined, [], undefined, ci).broken;
  assert.equal(broken, "A check behind a law of this part failed in main's latest CI, 2026-11-24");
  assert.doesNotMatch(broken, /wire-types-agree/);
  assert.equal(sheetMarks("core", undefined, [], undefined, { ...ci, at: null }).broken, "A check behind a law of this part failed in main's latest CI");
  assert.equal(sheetMarks("shell", undefined, [], undefined, ci).broken, null);
  assert.equal(sheetMarks("core", undefined, [], undefined, { ...ci, status: "passed" }).broken, null);
});

test("a scrub that reads slowly is dropped after Back to now or a later click", () => {
  const tickets = scrubTickets();
  const first = tickets.start();
  const second = tickets.start();
  assert.equal(tickets.live(first), false);
  assert.equal(tickets.live(second), true);
  tickets.cancel();
  assert.equal(tickets.live(second), false);
  assert.equal(tickets.live(tickets.start()), true);
});

// Revealing never moves a part: `revealPan` is given no positions and returns only a camera.
test("revealing plates pans the camera just enough to bring them into the visible band", () => {
  const at = (x, scale = 1) => ({ scale, x, y: 40 });
  const band = { left: 0, right: 1300 };
  // Plates span 100..1300 in a 1300 wide band: panned left by the gutter that would be cut.
  assert.deepEqual(revealPan(at(0), 100, 1300, band, 32), at(-32));
  assert.deepEqual(revealPan(at(0), 100, 1000, band, 32), at(0), "already inside");
  assert.deepEqual(revealPan(at(-500), 100, 1000, band, 32), at(-68), "cut at the left");
  assert.deepEqual(revealPan(at(0), 83, 1350, band, 32), at(-66.5), "the gutter shrinks to the room there is");
  assert.deepEqual(revealPan(at(-500), 0, 1350, band, 32), at(-500), "wider than the band: left where it is");
  // Zoomed out to half, a span of 1000..2000 shows at 500..1000, cut by a sheet at 800.
  assert.deepEqual(revealPan(at(0, 0.5), 1000, 2000, { left: 0, right: 800 }, 32), at(-232, 0.5));
  // A panel over the left of the area: the part is panned right, clear of its edge.
  assert.deepEqual(revealPan(at(0), 100, 500, { left: 300, right: 1300 }, 32), at(232));
});

test("a PR brief takes the camera while it plays and closes back to the view it found", () => {
  const mine = { scale: 0.8, x: -120, y: 30 };
  const rig = cameraRig(mine);
  assert.equal(rig.move({ scale: 1.2, x: 10, y: 10 }), true, "Daniel moves his view");
  const found = rig.user();
  rig.lend({ scale: 1.35, x: 400, y: -90 });
  rig.lend({ scale: 1.1, x: 200, y: -40 });
  assert.deepEqual(rig.shown(), { scale: 1.1, x: 200, y: -40 }, "the brief's newest framing shows");
  assert.equal(rig.lent(), true);
  assert.equal(rig.move({ scale: 2, x: 0, y: 0 }), false, "a move during the brief is refused");
  assert.deepEqual(rig.giveBack(), found);
  assert.deepEqual(rig.shown(), { scale: 1.2, x: 10, y: 10 });
  assert.equal(rig.lent(), false);
});

test("the parts' tab order reads the map top to bottom, then left to right, a row being plates within half a plate's height", () => {
  const at = {
    right: { x: 700, y: 210 },
    lower: { x: 100, y: 400 },
    left: { x: 100, y: 200 },
    // Dragged a little lower than "left", still on its row.
    middle: { x: 400, y: 200 + plateSize.h / 2 - 1 },
    top: { x: 900, y: 50 },
  };
  assert.deepEqual(readingOrder(at), ["top", "left", "middle", "right", "lower"]);
  // A plate a full half height lower starts the next row.
  assert.deepEqual(readingOrder({ a: { x: 500, y: 100 }, b: { x: 100, y: 100 + plateSize.h / 2 } }), ["a", "b"]);
  assert.deepEqual(readingOrder({}), []);
});

// Spec #473, "The sheet's links and the drawing": one list of links on the sheet, one line per pair on the map.
/** @type {import("../../dist/map-model.js").MapLink[]} */
const pairLinks = [
  { id: "hook-server", from: "hook", to: "server", kind: "import" },
  { id: "server-hook", from: "server", to: "hook", kind: "http", reason: "The server asks the hook for its log." },
  { id: "server-window", from: "server", to: "window", kind: "file", reason: "The page reads the built bundle." },
  { id: "agents-hook", from: "agents", to: "hook" },
];

test("Talks to lists every link at either end of the part: the other end, the arrow's direction, the kind and the reason", () => {
  const names = { hook: "Hook", server: "Server", window: "Window", agents: "Agents" };
  assert.deepEqual(talksTo("server", pairLinks, (id) => names[id]), [
    { link: "hook-server", other: "Hook", out: false, kind: "import", reason: undefined },
    { link: "server-hook", other: "Hook", out: true, kind: "http", reason: "The server asks the hook for its log." },
    { link: "server-window", other: "Window", out: true, kind: "file", reason: "The page reads the built bundle." },
  ]);
  // A version 1 link has no kind; an actor is named like a part.
  assert.deepEqual(talksTo("hook", pairLinks, (id) => names[id]).at(-1), { link: "agents-hook", other: "Agents", out: false, kind: undefined, reason: undefined });
  assert.deepEqual(talksTo("gone", pairLinks, (id) => id), []);
});

test("a pair with an import link and a run-time link, in either direction, draws as one line holding both ids", () => {
  assert.deepEqual(linkLines(pairLinks), [
    { ids: ["hook-server", "server-hook"], from: "hook", to: "server" },
    { ids: ["server-window"], from: "server", to: "window" },
    { ids: ["agents-hook"], from: "agents", to: "hook" },
  ]);
  assert.deepEqual(linkLines([]), []);
});

test("the law marks on one line sit apart along it, 10px from one another, centred on its middle", () => {
  assert.deepEqual(markPoints({ x: 0, y: 0 }, { x: 100, y: 0 }, 1), [{ x: 50, y: 0 }]);
  assert.deepEqual(markPoints({ x: 0, y: 0 }, { x: 100, y: 0 }, 3), [{ x: 40, y: 0 }, { x: 50, y: 0 }, { x: 60, y: 0 }]);
  assert.deepEqual(markPoints({ x: 0, y: 100 }, { x: 0, y: 0 }, 2), [{ x: 0, y: 55 }, { x: 0, y: 45 }], "along the line's own direction");
  assert.deepEqual(markPoints({ x: 5, y: 5 }, { x: 5, y: 5 }, 2), [{ x: 5, y: 5 }, { x: 5, y: 5 }], "a line of no length keeps them at its point");
});

// The Map diff (#640): an added part is placed once and its spot saved, so it does not move after the merge; a ghost keeps the spot it had, and one
// that never had a spot is placed beside its links without saving it.
test("in a Map diff an added part's spot is saved on first draw, and a ghost uses its kept spot or an unsaved one", () => {
  const items = [{ id: "core" }, { id: "new" }, { id: "gone" }, { id: "never" }];
  const links = [{ from: "new", to: "core" }, { from: "never", to: "core" }];
  const saved = { core: { x: 500, y: 400 }, gone: { x: 900, y: 400 } };
  const { at, save } = placement(items, links, saved, ["gone", "never"]);
  assert.deepEqual(Object.keys(at).sort(), ["core", "gone", "never", "new"]);
  assert.deepEqual(at.gone, { x: 900, y: 400 }, "the ghost's kept spot");
  assert.deepEqual(Object.keys(save), ["new"], "only the added part is saved");
  assert.deepEqual(save.new, at.new);
  assert.ok(!overlaps(at.new, at.never) && !overlaps(at.never, at.core));
  // Drawn again with the added spot saved, nothing more is saved and the ghost lands where it did.
  const again = placement(items, links, { ...saved, ...save }, ["gone", "never"]);
  assert.deepEqual(again.save, {});
  assert.deepEqual(again.at.never, at.never);
});

test("a Map diff's line is found by its two ends either way round, and its parts' tags read as New, Gone and Edited", () => {
  /** @type {Pick<import("../../dist/types.js").MapDiff, "lines">} */
  const diff = { lines: [{ ends: ["core", "server"], change: "removed" }, { ends: ["a", "b"], change: "added" }] };
  assert.equal(lineTag(diff, "server", "core"), "removed");
  assert.equal(lineTag(diff, "a", "b"), "added");
  assert.equal(lineTag(diff, "a", "core"), undefined);
  assert.equal(lineTag(null, "a", "b"), undefined);
  assert.deepEqual(/** @type {const} */ (["added", "removed", "edited"]).map(tagWord), ["New", "Gone", "Edited"]);
});

// The scan screen's words (#753, "The free scan"; ticket #760).
test("the languages line names the languages a reader reads, then the ones none reads yet", () => {
  /** @type {[string[], string[], string | null][]} (read, unread, line) */
  const cases = [
    [["TypeScript", "Go"], ["Ruby"], "TypeScript and Go: links computed. Ruby: no reader yet"],
    [["Rust"], [], "Rust: links computed"],
    [["Go", "Python", "TypeScript"], ["Kotlin", "Ruby"], "Go, Python and TypeScript: links computed. Kotlin and Ruby: no reader yet"],
    [[], ["Ruby"], "Ruby: no reader yet, so no links are drawn"],
    [[], [], null],
  ];
  for (const [read, unread, line] of cases) assert.equal(languagesLine(read, unread), line, JSON.stringify([read, unread]));
});

test("Draft the map's time hint grows with the scan's part count", () => {
  // The trial drafted 15 parts in about 3.6 minutes (#753): about a quarter of a minute a part, a minute at least.
  /** @type {[number, string][]} */
  const cases = [[1, "About 1 minute"], [4, "About 1 minute"], [5, "About 2 minutes"], [15, "About 4 minutes"]];
  for (const [parts, hint] of cases) assert.equal(draftTime(parts), hint, String(parts));
});

test("a Map draft's part stands where its latest group does, and the progress line counts the parts", () => {
  const group = (parts, state, redraft = false) => ({ parts, state, why: null, redraft });
  const verdict = (id, flag) => ({ id: `part:${id}/purpose`, verdict: flag, why: "", cites: [], flagged: flag !== "supported" });
  // (groups, verdicts, each part's stage, the progress line): a redraft is the part's latest group, so it stands as that group does.
  /** @type {[ReturnType<typeof group>[], Record<string, ReturnType<typeof verdict>>, string[], string][]} */
  const cases = [
    [[group(["a", "b"], "writing"), group(["c"], "writing")], {}, ["writing", "writing", "writing"], "0 of 3 parts checked · 3 being written"],
    [[group(["a"], "done"), group(["b"], "checking"), group(["c"], "writer-failed")], { a: verdict("a", "supported") }, ["checked", "checking", "writer-failed"], "1 of 3 parts checked · 1 being checked · 1 failed"],
    [[group(["a", "b"], "done"), group(["c"], "reviewer-failed")], { a: verdict("a", "overreach"), b: verdict("b", "supported") }, ["flagged", "checked", "reviewer-failed"], "2 of 3 parts checked · 1 failed"],
    [[group(["a", "b", "c"], "done"), group(["b"], "writing", true)], { a: verdict("a", "supported"), c: verdict("c", "supported") }, ["checked", "redrafting", "checked"], "2 of 3 parts checked · 1 being written"],
    [[group(["a", "b", "c"], "done")], { a: verdict("a", "supported"), b: verdict("b", "supported"), c: verdict("c", "contradicted") }, ["checked", "checked", "flagged"], "3 of 3 parts checked"],
  ];
  for (const [groups, verdicts, stages, line] of cases) {
    const draft = { map: "", groups, verdicts, not_written: [], ready: false, path: "/w/app/system-map.json", pr_muted: null };
    assert.deepEqual(["a", "b", "c"].map((id) => partStage(draft, id)), stages, JSON.stringify(groups));
    assert.equal(draftProgress(draft, ["a", "b", "c"]), line, JSON.stringify(groups));
  }
});

test("the loose files are offered by folder, each folder with the names of its files and Ignore muted where it would take a part's files", () => {
  // (files, parts, rows as [folder, words, why Ignore is muted]): the root's own files say so, and a long folder names three and counts the rest.
  const root = "Ignoring the root would leave out the whole repository";
  const parts = [{ name: "App", paths: ["src/app/"] }, { name: "Lib", paths: ["src/lib/"] }, { name: "Docs", paths: ["docs/guide.md"] }];
  /** @type {[string[], {name: string, paths: string[]}[], [string, string, string | null][]][]} */
  const cases = [
    [[], [], []],
    [["src/index.ts"], [], [["src/", "index.ts", null]]],
    [["src/index.ts", "main.go", "src/cli.ts", "example/demo/run.ts"], [], [["The root", "main.go", root], ["example/demo/", "run.ts", null], ["src/", "cli.ts and index.ts", null]]],
    [["t/a.ts", "t/b.ts", "t/c.ts", "t/d.ts", "t/e.ts"], [], [["t/", "a.ts, b.ts, c.ts and 2 more", null]]],
    // Files that are not code, hidden ones too, are rows like any other (#753: the check asks a home for every tracked file).
    [["README.md", ".claude/settings.json", "docs/guide.md"], [], [["The root", "README.md", root], [".claude/", "settings.json", null], ["docs/", "guide.md", null]]],
    // Ignore would leave out the files of a part with a path inside the folder, a folder's or a file's: the server refuses it too.
    [["src/index.ts", "docs/api.md", "wiki/a.md"], parts, [["docs/", "api.md", "Ignoring docs/ would leave out the files of Docs"], ["src/", "index.ts", "Ignoring src/ would leave out the files of App and Lib"], ["wiki/", "a.md", null]]],
    // A folder holding two or more loose rows and no files of its own is a parent row above them, at every level; one child row gets none.
    [[...["a", "b"].map((n) => `.claude/skills/${n}/s.md`), ".claude/agents/x/a.md", ".claude/agents/y/a.md"], [], [
      [".claude/", "2 folders", null], [".claude/agents/", "2 folders", null], [".claude/agents/x/", "a.md", null], [".claude/agents/y/", "a.md", null],
      [".claude/skills/", "2 folders", null], [".claude/skills/a/", "s.md", null], [".claude/skills/b/", "s.md", null],
    ]],
    [["a/b/f.md"], [], [["a/b/", "f.md", null]]],
    // A folder with files of its own keeps its row, and its subfolders' rows follow it; a part's files in a parent mute its Ignore only.
    [["d/x.md", "d/e/y.md", "d/f/z.md", "k/p/1.md", "k/q/2.md"], [{ name: "Kit", paths: ["k/q/other.md"] }], [
      ["d/", "x.md", null], ["d/e/", "y.md", null], ["d/f/", "z.md", null],
      ["k/", "2 folders", "Ignoring k/ would leave out the files of Kit"], ["k/p/", "1.md", null], ["k/q/", "2.md", "Ignoring k/q/ would leave out the files of Kit"],
    ]],
  ];
  for (const [files, held, rows] of cases) {
    const groups = looseGroups(files, held);
    assert.deepEqual(groups.map((g) => [g.label, g.names, g.muted]), rows, JSON.stringify(files));
    assert.deepEqual(groups.flatMap((g) => g.files).sort(), [...files].sort(), "every file is in one row");
    assert.ok(groups.every((g) => !g.parent || !g.files.length), "a parent row holds no files to give");
  }
  // A row's depth is how many parent rows it sits under: the example's .claude/skills/a/ sits under .claude/skills/ and .claude/.
  {
    const deep = looseGroups([".claude/skills/a/s.md", ".claude/skills/b/s.md", ".claude/agents/x/a.md", ".claude/agents/y/a.md"]);
    assert.deepEqual(deep.map((g) => [g.folder, g.parent, g.depth]), [
      [".claude/", true, 0], [".claude/agents/", true, 1], [".claude/agents/x/", false, 2], [".claude/agents/y/", false, 2],
      [".claude/skills/", true, 1], [".claude/skills/a/", false, 2], [".claude/skills/b/", false, 2],
    ]);
  }
  // The folders Ignore left out are the skeleton's ignored ones that are not the copies the scan asks about, which keep their checkboxes.
  const scan = { map: JSON.stringify({ ignored: ["vendor/", "wiki/", ".claude/"] }), ignore_candidates: [{ path: "vendor/", ticked: true }, { path: "copy/", ticked: false }] };
  assert.deepEqual(ignoredFolders(scan), [".claude/", "wiki/"]);
  assert.deepEqual(ignoredFolders({ map: JSON.stringify({ parts: [] }), ignore_candidates: [] }), []);
});

test("the copy-prompt carries the skeleton and says where to write the map and which check to pass", () => {
  const skeleton = JSON.stringify({ version: 2, parts: [{ id: "app", paths: ["src/app/"] }], links: [] });
  const prompt = scanPrompt(skeleton);
  assert.ok(prompt.includes(JSON.stringify(JSON.parse(skeleton), null, 2)), prompt);
  assert.match(prompt, /system-map\.json at the root of this repository's main checkout/);
  assert.match(prompt, /`npx plotkeeper map check`[^.]*until it passes/);
});
