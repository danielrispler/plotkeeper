// Wait tips (#523, spec #520): the pure chooser and the hand-written list. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { initial } from "../../dist/settings.js";
import { choose, cooldown, LINE, MAX_SHOWS, stretches, tipText, TIPS } from "../../dist/tips.js";

const fresh = () => ({ stretch: 0, tips: {} });
const all = { map: true, brief: true, trees: true };
const none = { map: false, brief: false, trees: false };
const tip = (id, when) => ({ id, text: `Tip ${id}`, ...(when ? { when } : {}) });

// Runs `n` Running stretches with tips on, returning the id shown in each (null for none).
const run = (tips, n, context = all, shows = fresh()) => {
  const seen = [];
  for (let i = 0; i < n; i++) {
    const picked = choose({ tips, shows, context, running: true, on: true });
    seen.push(picked.tip?.id ?? null);
    shows = picked.shows;
  }
  return { seen, shows };
};

test("a tip shows only when its context holds", () => {
  const tips = [tip("map", "map"), tip("trees", "trees")];
  assert.deepEqual(run(tips, 1, { ...none, trees: true }).seen, ["trees"]);
  assert.deepEqual(run(tips, 1, none).seen, [null]);
});

test("the cooldown is max(1, min(N, eligible - 1)), counted in Running stretches", () => {
  assert.equal(cooldown(1), 1);
  assert.equal(cooldown(2), 1);
  assert.equal(cooldown(3), 2);
  assert.equal(cooldown(12), 4);
  // Three tips: each waits two stretches after it shows, so they take turns.
  assert.deepEqual(run([tip("a"), tip("b"), tip("c")], 6).seen, ["a", "b", "c", "a", "b", "c"]);
});

test("a stretch with nothing shown still moves the counter", () => {
  const { shows } = run([tip("map", "map")], 3, none);
  assert.equal(shows.stretch, 3);
});

test("never the same tip twice in a row; a lone tip shows every other stretch until its cap", () => {
  const { seen } = run([tip("a")], 12);
  assert.deepEqual(seen, ["a", null, "a", null, "a", null, "a", null, "a", null, null, null]);
  assert.equal(seen.filter(Boolean).length, MAX_SHOWS);
  const pair = run([tip("a"), tip("b")], 8).seen;
  for (let i = 1; i < pair.length; i++) assert.notEqual(pair[i], pair[i - 1]);
});

test("the tip shown longest ago comes first, a never-shown one before any", () => {
  const shows = { stretch: 20, tips: { a: { count: 1, last: 10 }, b: { count: 1, last: 5 } } };
  assert.equal(choose({ tips: [tip("a"), tip("b"), tip("c")], shows, context: all, running: true, on: true }).tip.id, "c");
  assert.equal(choose({ tips: [tip("a"), tip("b")], shows, context: all, running: true, on: true }).tip.id, "b");
});

test("none when not Running and none with tips off; neither moves the counter", () => {
  for (const args of [{ running: false, on: true }, { running: true, on: false }]) {
    const picked = choose({ tips: [tip("a")], shows: fresh(), context: all, ...args });
    assert.equal(picked.tip, null);
    assert.equal(picked.shows.stretch, 0);
  }
});

// A store in memory, as `localStorage` would hold it.
const memory = () => {
  let text = null;
  return { get: () => text, set: (value) => void (text = value) };
};

test("one tip per Running stretch: it stays while the session runs, and the next stretch picks again", () => {
  const tips = [tip("a"), tip("b"), tip("c")];
  const at = stretches(memory(), tips);
  const running = { id: "s1", state: "running" };
  const first = at(running, [running], all, true);
  assert.equal(first.id, "a");
  assert.equal(at(running, [running], all, true), first);
  assert.equal(at({ ...running, state: "your_turn" }, [{ ...running, state: "your_turn" }], all, true), null);
  assert.equal(at(running, [running], all, true).id, "b");
  // A session that stopped running while not selected starts a new stretch when it runs again.
  at({ id: "s2", state: "running" }, [running, { id: "s2", state: "running" }], all, true);
  assert.equal(at(running, [{ ...running, state: "needs_you" }], all, true).id, "a");
});

test("show history lives in the store, so a reload keeps it", () => {
  const store = memory();
  const running = { id: "s1", state: "running" };
  stretches(store, [tip("a"), tip("b")])(running, [running], all, true);
  assert.equal(stretches(store, [tip("a"), tip("b")])(running, [running], all, true).id, "b");
});

// The palette's actions: the keys of `main.ts`'s action table.
const actions = new Set([...readFileSync(new URL("../main.ts", import.meta.url), "utf8").matchAll(/^\s*"(studio::\w+)":/gm)].map((m) => m[1]));

test("the list: real palette actions, one line each, written to you, unique ids", () => {
  assert.ok(TIPS.length >= 10);
  assert.ok(actions.has("studio::Palette"));
  for (const t of TIPS) {
    if (t.action) assert.ok(actions.has(t.action), `${t.id}: ${t.action} is not a palette action`);
    assert.equal(t.text.includes("{key}"), t.action !== undefined, `${t.id}: {key} marks its action's key`);
    const line = tipText(t, initial.keymap);
    assert.ok(line.length <= LINE, `${t.id}: "${line}" is longer than ${LINE} characters`);
    assert.ok(!/Daniel/.test(t.text), `${t.id} names Daniel`);
  }
  assert.equal(new Set(TIPS.map((t) => t.id)).size, TIPS.length);
});

test("a tip's key is the live binding; an unbound action says where to find it", () => {
  const t = { id: "fit", text: "Fit map ({key}) puts the whole map in view.", action: "studio::FitMap" };
  assert.equal(tipText(t, initial.keymap), "Tip: Fit map (⌘0) puts the whole map in view.");
  const rebound = { ...initial.keymap, Workspace: { "cmd-9": "studio::FitMap" } };
  assert.equal(tipText(t, rebound), "Tip: Fit map (⌘9) puts the whole map in view.");
  assert.equal(tipText(t, { Workspace: {} }), "Tip: Fit map (in the palette) puts the whole map in view.");
});

test("the tip is the Intention box's last line, under the step", () => {
  const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const box = html.slice(html.indexOf('<div id="intention"'), html.indexOf("</div>", html.indexOf('<div id="intention"')));
  assert.ok(box.indexOf('id="step"') > 0 && box.indexOf('id="tip"') > box.indexOf('id="step"'));
  assert.match(box, /<p id="tip" class="tip" hidden><\/p>\s*$/);
});
