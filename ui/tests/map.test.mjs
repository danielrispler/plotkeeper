// The Map view's pure logic (spec danielrispler/plotkeeper#306, ticket #309), run on the `tsc` output. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import {
  actorSize, autoPlace, cameraRig, draftPrompt, fitCamera, panTo, zoomAt, zoomLimits, dragMove, dragStart, dragThreshold, effectiveTier, escapeMap, layerOrder, parseMap, plateSize, rimPoint, revealPan, scrubTickets, sheetOf,
  narrowBelow, withPanel, withSheet, pressRow, panelSession,
} from "../../dist/map-model.js";

const part = (id, extra = {}) => ({ id, name: id, caption: "", description: "", tier: "core", paths: [], ...extra });
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

test("Esc closes the drawer, then the brief, then returns from a scrubbed commit to now, then closes the sheet, then the left panel, then does nothing (#386)", () => {
  const open = { scrubbed: true, drawer: true, brief: true, sheet: true, panel: true };
  const closed = [];
  const layers = Object.fromEntries(layerOrder.map((name) => [name, { open: () => open[name], close: () => { open[name] = false; closed.push(name); } }]));
  const presses = [];
  for (let i = 0; i < 6; i++) presses.push(escapeMap(layers));
  assert.deepEqual(closed, ["drawer", "brief", "scrubbed", "sheet", "panel"]);
  assert.deepEqual(presses, ["drawer", "brief", "scrubbed", "sheet", "panel", null]);
});

// The left panel and the part's sheet (#385): what opens where.
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

test("the session whose chips stay lit is the one the panel shows", () => {
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
});

test("a part's sheet names it, lists its laws by backing, and offers the three tiers with the effective one pressed", () => {
  const laws = [
    { id: "l1", on: "core", backing: "check", text: "Reads no file." },
    { id: "l2", on: "core", backing: "trust", text: "Applies in order." },
    { id: "l3", on: "hook", backing: "check", text: "Not this part's." },
  ];
  const sheet = sheetOf(part("core", { name: "Core", caption: "Events to state", description: "Plain Rust." }), laws, "interface", undefined);
  assert.deepEqual([sheet.name, sheet.caption, sheet.description], ["Core", "Events to state", "Plain Rust."]);
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
