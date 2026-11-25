// The PR brief view's pure logic (spec danielrispler/plotkeeper#334, ticket #339), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { alsoAt, awaitedWrite, chapterGate, storySteps, briefCamera, briefPlayer, latestRequest, escapeMap, figureShapes, figureSize, playlist, partialFactsLines, unconfirmed, unconfirmedReasons } from "../../dist/map-model.js";
import { trayRows } from "../../dist/map-tray.js";

const step = (k, extra = {}) => ({ k, h: k, b: "", on: [], hops: [], law: null, covered_by: null, list: [], risks: [], fig: null, check: null, cites: [], flags: [], ...extra });
const chapter = (title, ...kinds) => ({ behaviour: title, parts: [], title, steps: kinds.map((k) => step(k)), flags: [] });
const brief = (extra) => ({ title: "T", overview: [], chapters: [], also: [], unseen: [], decisions: [], flags: [], ...extra });

test("a single story plays its steps; a chaptered brief plays one chapter, the overview before the first", () => {
  const single = brief({ chapters: [chapter("Order", "Where", "Before", "Now")] });
  assert.deepEqual(playlist(single, 0, false).map((s) => s.k), ["Where", "Before", "Now"]);
  const chaptered = brief({ overview: [step("Where")], chapters: [chapter("Fog", "Before", "Now"), chapter("Laws", "Now", "Could break")] });
  assert.deepEqual(playlist(chaptered, 0, true).map((s) => s.k), ["Where", "Before", "Now"]);
  assert.deepEqual(playlist(chaptered, 1, false).map((s) => s.k), ["Now", "Could break"]);
  assert.deepEqual(playlist(chaptered, 0, false).map((s) => s.k), ["Before", "Now"], "an overview already played is not played again");
});

test("the overview plays before the first chapter opened, whichever it is, and once per brief", () => {
  const player = briefPlayer({ scrubbed: () => false, backToNow: () => {} });
  player.open("/p", 7, 1);
  assert.equal(player.current().overview, true, "the first chapter opened carries it");
  player.close();
  player.open("/p", 7, 0);
  assert.equal(player.current().overview, false, "another chapter does not");
  assert.equal(player.current().step, 0);
  player.close();
  player.open("/p", 7, 1);
  assert.equal(player.current().overview, true, "leaving mid-chapter keeps the overview in front of it");
  assert.equal(player.go(1, 5), "moved");
  assert.equal(player.go(1, 5), "moved");
  assert.equal(player.go(1, 5), "moved");
  assert.equal(player.go(1, 5), "moved");
  assert.equal(player.go(1, 5), "done");
  player.open("/p", 7, 1);
  assert.equal(player.current().overview, false, "finishing it used the overview up");
  player.open("/p", 8, 0);
  assert.equal(player.current().overview, true, "another brief has its own");
});

test("each chapter resumes at the step it was left on", () => {
  const player = briefPlayer({ scrubbed: () => false, backToNow: () => {} });
  player.open("/p", 7, 0);
  player.go(1, 4);
  player.close();
  player.open("/p", 7, 2);
  player.go(1, 4);
  player.go(1, 4);
  player.close();
  player.open("/p", 7, 0);
  assert.equal(player.current().step, 1);
  player.close();
  player.open("/p", 7, 2);
  assert.equal(player.current().step, 2);
});

test("the also-touched lines show on the last step of the last chapter only", () => {
  const also = [{ part: "hook", line: "A wording change.", flags: [] }];
  const b = brief({ also, chapters: [chapter("Fog", "Before", "Now"), chapter("Laws", "Now", "Could break")] });
  assert.deepEqual(alsoAt(b, 1, 1, 2), also);
  assert.deepEqual(alsoAt(b, 1, 0, 2), []);
  assert.deepEqual(alsoAt(b, 0, 1, 2), []);
  assert.deepEqual(alsoAt(brief({ chapters: [chapter("Fog", "Now")] }), 0, 0, 1), []);
});

test("the stored v3 brief of #317 plays as three chapters of four steps, in order, with the overview none", () => {
  const raw = JSON.parse(readFileSync("understanding/tests/fixtures/stored/pr-brief-v3/317-reply.json", "utf8")).result;
  const writer = JSON.parse(raw.replace(/^\s*```json\s*/, "").replace(/\s*```\s*$/, ""));
  const full = brief({ ...writer, chapters: writer.chapters.map((c) => ({ ...c, flags: [], steps: c.steps.map((s) => step(s.k, s)) })) });
  assert.deepEqual(full.chapters.map((c) => c.behaviour), ["Fog", "Broken laws", "Seen gate"]);
  assert.deepEqual([0, 1, 2].map((i) => playlist(full, i, i === 0).length), [4, 4, 4]);
  const rows = trayRows([{ number: 317, title: "The Map view", head: "a", brief: { stale: false, writing: false, chapters: full.chapters.map((c) => c.title) } }]);
  assert.equal(rows.length, 3);
  assert.deepEqual(rows.map((r) => r.chapter), [0, 1, 2]);
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
  assert.deepEqual([1, 2, 3, 4, 5].map(() => escapeMap(layers)), ["drawer", "brief", "scrubbed", "sheet", null]);
  player.open("/p", 7, 0);
  assert.equal(player.current().step, 1, "closing with Esc kept the step");
});

// #491: a "not confirmed from the facts" popover is its own Esc layer, ahead of the brief, so Esc closes it and leaves the brief open.
test("Esc with a not-confirmed popover open closes only the popover, then the brief", () => {
  const player = briefPlayer({ scrubbed: () => false, backToNow: () => {} });
  player.open("/p", 7, 0);
  let popover = true;
  const layers = {
    popover: { open: () => popover, close: () => (popover = false) },
    brief: { open: () => player.current() !== null, close: () => player.close() },
  };
  assert.equal(escapeMap(layers), "popover");
  assert.equal(popover, false);
  assert.notEqual(player.current(), null, "the brief stays open");
  assert.equal(escapeMap(layers), "brief");
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

// #491: the popover lists why an item is not confirmed; the mark's own words, the sentinel the check adds, are not a reason.
test("a not-confirmed item's reasons are its flags without the sentinel", () => {
  assert.deepEqual(unconfirmedReasons(["claim check: the quote is not in the diff", "not confirmed from the facts"]), ["claim check: the quote is not in the diff"]);
  assert.deepEqual(unconfirmedReasons(["not confirmed from the facts"]), []);
});

// #491: the Partial facts chip has one line per missing input, from the facts stored with the brief; a brief from before them knows only
// that its diff was cut.
test("the Partial facts lines come from the stored facts, one per input, and none when nothing is missing", () => {
  assert.deepEqual(partialFactsLines(undefined, false), [], "nothing missing: no chip");
  assert.deepEqual(partialFactsLines({}, false), []);
  const cut = { files_cut: ["ui/a.ts", "ui/b.ts"], files_partial: ["server/src/c.rs"], bytes_sent: 65_536, bytes_cut: 20_480 };
  assert.deepEqual(partialFactsLines({ cut, claim_error: "the check timed out", no_extractor: [".go", ".py"] }, true), [
    { line: "The diff was cut: 64\u00a0KiB sent, 20\u00a0KiB not", files: ["ui/a.ts", "ui/b.ts", "server/src/c.rs (in part)"] },
    { line: "The claim check failed: the check timed out", files: [] },
    { line: "No extractor reads .go, .py files, so their imports are not on the map", files: [] },
  ]);
  assert.deepEqual(partialFactsLines(undefined, true), [{ line: "The diff was cut", files: [] }], "an older brief has only its flag");
  assert.deepEqual(partialFactsLines({ claim_error: "no answer" }, false), [{ line: "The claim check failed: no answer", files: [] }]);
});

test("See the code opens what the step quoted, with its source and no jump, as the page has no editor", async () => {
  const { citeDrawer } = await import("../../dist/map-model.js");
  const drawer = citeDrawer(step("Now", { h: "Delete keeps a live terminal", cites: [{ from: "ui/map.ts", quote: "const a = 1;" }, { from: "the PR body", quote: "Closes #5" }] }));
  assert.equal(drawer.title, "Delete keeps a live terminal");
  assert.deepEqual(drawer.blocks.map((b) => [b.label, b.code, b.jumps.length]), [["ui/map.ts", "const a = 1;", 0], ["the PR body", "Closes #5", 0]]);
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

test("the Ratify step that ends the last chapter is no part of the story: the gate and the also-touched lines key on the step before it", () => {
  const also = [{ part: "hook", line: "A wording change.", flags: [] }];
  const b = brief({ also, chapters: [chapter("Fog", "Before", "Now")] });
  const played = [...playlist(b, 0, false), step("Ratify")];
  const story = storySteps(played);
  assert.deepEqual(story.map((s) => s.k), ["Before", "Now"]);
  assert.deepEqual(alsoAt(b, 0, 1, story.length), also, "the last story step carries them");
  assert.deepEqual(alsoAt(b, 0, 2, story.length), [], "the Ratify step does not");
  // Reading the last story step completes the chapter; the Ratify step is never needed.
  let fired = 0;
  const gate = chapterGate(story, () => fired++);
  gate.sawLast();
  assert.equal(fired, 1);
});

test("opening another chapter over an open one leaves the open one's step where it was", () => {
  const player = briefPlayer({ scrubbed: () => false, backToNow: () => {} });
  player.open("/p", 7, 0);
  player.go(1, 5);
  player.open("/p", 7, 1);
  assert.equal(player.current().step, 0);
  player.open("/p", 7, 0);
  assert.equal(player.current().step, 1, "the first chapter continues where it was left");
});

test("only the latest request is wanted: a later one, or a cancel, drops the earlier", () => {
  const request = latestRequest();
  const first = request.begin();
  assert.equal(first(), true);
  const second = request.begin();
  assert.equal(first(), false, "a later request supersedes it");
  assert.equal(second(), true);
  request.cancel();
  assert.equal(second(), false, "a close drops the one in flight");
});

test("an open waiting on its write opens the brief once it is stored, stops on a failure, a quiet PR or one gone, and waits otherwise", () => {
  const pr = (brief) => ({ number: 7, title: "Fog", head: "aaa", brief });
  const info = (extra) => ({ stale: false, writing: false, chapters: [], quiet: false, ...extra });
  assert.equal(awaitedWrite(pr(info({ chapters: ["Fog"] }))), "open");
  assert.equal(awaitedWrite(pr(info({ writing: true }))), "wait");
  assert.equal(awaitedWrite(pr(info({ chapters: ["Fog"], writing: true }))), "wait", "rewritten");
  // The map drawn from before the write started: an older brief, or none yet.
  assert.equal(awaitedWrite(pr(info({ chapters: ["Fog"], stale: true }))), "wait");
  assert.equal(awaitedWrite(pr(null)), "wait");
  assert.equal(awaitedWrite(pr(info({ failed: true }))), "stop");
  assert.equal(awaitedWrite(pr(info({ chapters: ["Fog"], stale: true, failed: true }))), "stop");
  assert.equal(awaitedWrite(pr(info({ quiet: true }))), "stop");
  assert.equal(awaitedWrite(undefined), "stop", "no longer ready");
});
