// The PR brief view's pure logic (spec danielrispler/plotkeeper#334, ticket #339), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { chapterGate, briefCamera, briefPlayer, escapeMap, figureShapes, figureSize, layerOrder, playlist, unconfirmed } from "../../dist/map-model.js";

const step = (k, extra = {}) => ({ k, h: k, b: "", on: [], hops: [], law: null, covered_by: null, list: [], risks: [], fig: null, check: null, cites: [], flags: [], ...extra });
const chapter = (title, ...kinds) => ({ behaviour: title, parts: [], title, steps: kinds.map((k) => step(k)), flags: [] });
const brief = (extra) => ({ title: "T", overview: [], chapters: [], also: [], unseen: [], decisions: [], flags: [], ...extra });

test("a single story plays its steps; a chaptered brief plays one chapter, the overview before the first", () => {
  const single = brief({ chapters: [chapter("Order", "Where", "Before", "Now")] });
  assert.deepEqual(playlist(single, 0).map((s) => s.k), ["Where", "Before", "Now"]);
  const chaptered = brief({ overview: [step("Where")], chapters: [chapter("Fog", "Before", "Now"), chapter("Laws", "Now", "Could break")] });
  assert.deepEqual(playlist(chaptered, 0).map((s) => s.k), ["Where", "Before", "Now"]);
  assert.deepEqual(playlist(chaptered, 1).map((s) => s.k), ["Now", "Could break"]);
});

test("a brief reopens at the step left, and finishing it starts it over", () => {
  const player = briefPlayer({ scrubbed: () => false, backToNow: () => assert.fail("not scrubbed") });
  player.open("/p", 7, 0);
  assert.equal(player.go(1, 4), "moved");
  assert.equal(player.go(1, 4), "moved");
  assert.equal(player.current().step, 2);
  player.close();
  assert.equal(player.current(), null);
  player.open("/p", 7, 0);
  assert.equal(player.current().step, 2, "closing keeps the step");
  player.open("/p", 7, 1);
  assert.equal(player.current().step, 0, "another chapter has its own");
  assert.equal(player.go(-1, 4), "moved");
  assert.equal(player.current().step, 0, "Back stops at the first step");
  player.open("/p", 7, 0);
  assert.equal(player.go(1, 4), "moved");
  assert.equal(player.go(1, 4), "done");
  assert.equal(player.current(), null);
  player.open("/p", 7, 0);
  assert.equal(player.current().step, 0, "a finished brief starts over");
});

test("opening a brief while the map is scrubbed returns it to now first", () => {
  let scrubbed = true;
  let backs = 0;
  const player = briefPlayer({ scrubbed: () => scrubbed, backToNow: () => (backs++, (scrubbed = false)) });
  player.open("/p", 7, 0);
  assert.equal(backs, 1);
  player.close();
  player.open("/p", 7, 0);
  assert.equal(backs, 1, "now already");
});

test("Esc closes the drawer, then the brief, then returns to now, then the sheet, then leaves", () => {
  assert.deepEqual(layerOrder, ["drawer", "brief", "scrubbed", "sheet"]);
  const player = briefPlayer({ scrubbed: () => false, backToNow: () => {} });
  player.open("/p", 7, 0);
  player.go(1, 3);
  const open = { drawer: true, scrubbed: true, sheet: true };
  const layers = {
    drawer: { open: () => open.drawer, close: () => (open.drawer = false) },
    brief: { open: () => player.current() !== null, close: () => player.close() },
    scrubbed: { open: () => open.scrubbed, close: () => (open.scrubbed = false) },
    sheet: { open: () => open.sheet, close: () => (open.sheet = false) },
  };
  assert.deepEqual([1, 2, 3, 4, 5].map(() => escapeMap(layers)), ["drawer", "brief", "scrubbed", "sheet", "leave"]);
  player.open("/p", 7, 0);
  assert.equal(player.current().step, 1, "closing with Esc kept the step");
});

test("the camera fits the lit parts into the room the panel leaves, never past the largest scale", () => {
  const area = { w: 1000, h: 800 };
  const big = briefCamera({ x0: 0, y0: 0, x1: 2000, y1: 1000 }, area, { top: 20, bottom: 300 }, 1.35);
  assert.ok(big.scale > 0 && big.scale < 0.5, `scale ${big.scale}`);
  const small = briefCamera({ x0: 400, y0: 300, x1: 500, y1: 340 }, area, { top: 20, bottom: 300 }, 1.35);
  assert.equal(small.scale, 1.35);
  // The box's centre lands in the middle of the room above the panel.
  const cx = ((400 + 500) / 2) * small.scale + small.x;
  const cy = ((300 + 340) / 2) * small.scale + small.y;
  assert.ok(Math.abs(cx - 500) < 1, `cx ${cx}`);
  assert.ok(Math.abs(cy - (20 + (800 - 20 - 300) / 2)) < 1, `cy ${cy}`);
});

test("each figure kind is drawn from the writer's labels, inside its frame", () => {
  const items = [{ label: "old · deleted", tone: null, bad: null }, { label: "now · lost", tone: "lost", bad: true }, { label: "later", tone: "live", bad: null }];
  for (const kind of ["states", "sequence", "compare"]) {
    const fig = { kind, frame: "terminal t1", items, before: ["one", "two"], after: ["three"] };
    const shapes = figureShapes(fig);
    const words = shapes.map((s) => s.text);
    const wanted = kind === "compare" ? ["one", "two", "three"] : ["old · deleted", "now · lost", "later"];
    for (const word of wanted) assert.ok(words.includes(word), `${kind} lacks ${word}`);
    for (const s of shapes) assert.ok(s.x >= 0 && s.y >= 0 && s.x + s.w <= figureSize.w && s.y + s.h <= figureSize.h, `${kind} ${s.text} outside`);
  }
  const states = figureShapes({ kind: "states", frame: "terminal t1", items, before: [], after: [] });
  assert.equal(states.find((s) => s.text === "now · lost").tone, "lost");
  assert.equal(states.find((s) => s.text === "later").tone, "live");
  assert.ok(states.some((s) => s.kind === "frame" && s.text === "terminal t1"));
  assert.deepEqual(figureShapes({ kind: "unknown", frame: null, items, before: [], after: [] }), [], "a kind code does not know draws nothing");
});

test("a step with a flag left after the rewrite is unconfirmed", () => {
  assert.equal(unconfirmed(step("Now", { flags: ["a quote is not in the facts"] })), true);
  assert.equal(unconfirmed(step("Now")), false);
});

test("See the code opens what the step quoted, with a jump only for a file", async () => {
  const { citeDrawer } = await import("../../dist/map-model.js");
  const drawer = citeDrawer(step("Now", { h: "Delete keeps a live terminal", cites: [{ from: "ui/map.ts", quote: "const a = 1;" }, { from: "the PR body", quote: "Closes #5" }] }));
  assert.equal(drawer.title, "Delete keeps a live terminal");
  assert.deepEqual(drawer.blocks.map((b) => [b.label, b.code, b.jumps.length]), [["ui/map.ts", "const a = 1;", 1], ["the PR body", "Closes #5", 0]]);
  assert.deepEqual(drawer.blocks[0].jumps[0].to, { file: "ui/map.ts", line: 1 });
});

test("a pick is chosen, taken back by tapping it again, and graded only when Check is pressed", async () => {
  const { pickRun } = await import("../../dist/map-model.js");
  const run = pickRun(1);
  assert.equal(run.ready(), false, "nothing picked: Check is off");
  run.tap(0);
  assert.equal(run.picked(), 0);
  assert.equal(run.ready(), true);
  run.tap(1);
  assert.equal(run.picked(), 1, "another option replaces the pick");
  run.tap(1);
  assert.equal(run.picked(), null, "tapping it again takes it back");
  assert.equal(run.ready(), false);
  assert.equal(run.grade(), null, "no answer without a pick");
  run.tap(0);
  assert.deepEqual(run.grade(), { right: false, key: 1 });
  assert.equal(run.answered(), true);
  run.tap(1);
  assert.equal(run.picked(), 0, "an answered check is final");
  const right = pickRun(1);
  right.tap(1);
  assert.deepEqual(right.grade(), { right: true, key: 1 });
});

const links = [{ id: "a-b", from: "a", to: "b" }, { id: "c-b", from: "c", to: "b" }, { id: "a-x", from: "a", to: "x" }];

test("the order slots fill by tap in order, a tap on a filled part takes it back, and Check waits for every slot", async () => {
  const { orderRun } = await import("../../dist/map-model.js");
  const run = orderRun(["a", "b", "c"], links);
  run.tap("b");
  run.tap("a");
  assert.deepEqual(run.picked(), ["b", "a"]);
  assert.equal(run.ready(), false);
  run.tap("b");
  assert.deepEqual(run.picked(), ["a"], "taking back leaves the others in their order");
  run.tap("b");
  run.tap("c");
  assert.deepEqual(run.picked(), ["a", "b", "c"]);
  assert.equal(run.ready(), true);
  run.tap("x");
  assert.deepEqual(run.picked(), ["a", "b", "c"], "no slot is left for a fourth");
});

test("a wrong order names the parts out of place; the right one replays the key along the links, a link backwards when it runs against the key", async () => {
  const { orderRun } = await import("../../dist/map-model.js");
  const wrong = orderRun(["a", "b", "c"], links);
  assert.equal(wrong.grade(), null, "not graded before every slot is full");
  for (const id of ["a", "c", "b"]) wrong.tap(id);
  assert.deepEqual(wrong.grade(), { right: false, wrong: ["c", "b"] });
  assert.equal(wrong.answered(), true);
  wrong.tap("a");
  assert.deepEqual(wrong.picked(), ["a", "c", "b"], "an answered check is final");
  assert.deepEqual(wrong.replay(), [["a-b", false], ["c-b", true]]);
  const right = orderRun(["a", "b", "c"], links);
  for (const id of ["a", "b", "c"]) right.tap(id);
  assert.deepEqual(right.grade(), { right: true, wrong: [] });
});

test("a chapter is complete when its last step was seen and its check answered, right or wrong", () => {
  let fired = 0;
  const gate = chapterGate([step("Where"), step("Your turn", { check: { kind: "pick" } })], () => fired++);
  gate.sawLast();
  assert.equal(fired, 0, "the check is not answered yet");
  assert.equal(gate.done(), false);
  gate.answered();
  assert.equal(fired, 1);
  gate.answered();
  gate.sawLast();
  assert.equal(fired, 1, "it tells once");
  // Answered first, then seen: the same.
  let again = 0;
  const other = chapterGate([step("Your turn", { check: { kind: "order" } })], () => again++);
  other.answered();
  assert.equal(again, 0, "the last step was not seen");
  other.sawLast();
  assert.equal(again, 1);
});

test("a chapter with no check is complete once its last step is seen, and leaving earlier completes nothing", () => {
  let fired = 0;
  const gate = chapterGate([step("Where"), step("Now")], () => fired++);
  assert.equal(fired, 0);
  assert.equal(gate.done(), false);
  gate.sawLast();
  assert.equal(fired, 1);
  assert.equal(gate.done(), true);
});
