// The Map view's pure logic (spec danielrispler/plotkeeper#306, ticket #309), run on the `tsc` output. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { actorSize, autoPlace, draftPrompt, dragMove, dragStart, dragThreshold, effectiveTier, escapeMap, layerOrder, parseMap, plateSize, plateTitle, rimPoint, scrubTickets, sheetOf, sheetMarks, narrowBelow, withPanel, withSheet, pressRow, panelSession, switchProject, talksTo, linkLines, readingOrder, markPoints } from "../../dist/map-model.js";
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
