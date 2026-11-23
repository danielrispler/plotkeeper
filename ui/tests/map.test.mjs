// The Map view's pure logic (spec danielrispler/plotkeeper#306, ticket #309), run on the `tsc` output. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import {
  actorSize, autoPlace, draftPrompt, dragMove, dragStart, dragThreshold, effectiveTier, escapeMap, layerOrder, mapProject, parseMap, plateSize, rimPoint, scrubTickets, sheetOf, scrollToShow,
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

test("Esc closes the drawer, then the brief, then returns from a scrubbed commit to now, then closes the sheet, then leaves the Map view", () => {
  const open = { scrubbed: true, drawer: true, brief: true, sheet: true };
  const closed = [];
  const layers = Object.fromEntries(layerOrder.map((name) => [name, { open: () => open[name], close: () => { open[name] = false; closed.push(name); } }]));
  assert.deepEqual(layerOrder, ["drawer", "brief", "scrubbed", "sheet"]);
  const presses = [];
  for (let i = 0; i < 5; i++) presses.push(escapeMap(layers));
  assert.deepEqual(closed, ["drawer", "brief", "scrubbed", "sheet"]);
  assert.deepEqual(presses, ["drawer", "brief", "scrubbed", "sheet", "leave"]);
});

test("Esc skips a layer that is not open, and a layer nobody registered", () => {
  const open = { drawer: false, sheet: true };
  const layers = { drawer: { open: () => open.drawer, close: () => assert.fail("closed a layer that was not open") }, sheet: { open: () => open.sheet, close: () => (open.sheet = false) } };
  assert.equal(escapeMap(layers), "sheet");
  assert.equal(escapeMap(layers), "leave");
  assert.equal(escapeMap({}), "leave");
});

test("the map opens the selected session's project, else the first project that has a map, else the first", () => {
  const paths = ["/a", "/b", "/c"];
  assert.equal(mapProject("/c", paths, new Set(["/b"])), "/c");
  assert.equal(mapProject(null, paths, new Set(["/c", "/b"])), "/b");
  assert.equal(mapProject(null, paths, new Set()), "/a");
  assert.equal(mapProject(null, [], new Set()), null);
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

test("when the sheet narrows the map, it scrolls so the plates are not cut mid-plate, and leaves a map that cannot fit where it was", () => {
  // Plates span 100..1300 in a 1300 wide area: scrolled right by the gutter that would be cut, never past the left edge.
  assert.equal(scrollToShow(100, 1300, 0, 1300, 32), 32);
  assert.equal(scrollToShow(100, 1000, 0, 1300, 32), 0, "already inside");
  assert.equal(scrollToShow(100, 1000, 500, 1300, 32), 68, "cut at the left");
  assert.equal(scrollToShow(83, 1350, 0, 1300, 32), 66.5, "the gutter shrinks to the room there is");
  assert.equal(scrollToShow(0, 1350, 500, 1300, 32), 500, "wider than the area: left where it is");
});
