// Home's waiting room, after the outside review of #253. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { Field, fakeDocument, tauri } from "./dom.mjs";

const calls = [];
tauri(calls);
const { drawHome, resetHome, homeState } = await import("../../dist/home.js");
globalThis.document = fakeDocument({ visibilityState: "visible", hasFocus: () => true }, Field);

const quiz = { text: "q", key: 1, options: [{ text: "A", why: "a" }, { text: "B", why: "b" }] };
const retest = (id) => ({ kind: "retest", id, of: id, session_id: "s", added_at: 1, quiz, taps: [] });
const live = (id, state, extra = {}) => ({ id, state, unseen: false, landed: false, turns: [], questions: [], transcript: {}, identity: { issue: null, name: "" }, folder_name: "p", cwd: "/w/p", capabilities: { agent: "claude" }, name: id, ...extra });
const settle = () => new Promise((r) => setTimeout(r, 30));

test("turn ends are tracked per session: a quick follow-on prompt hides none, and the watermark never moves back (C13)", async () => {
  const { endTracker } = await import("../../dist/home.js");
  const ended = endTracker();
  const s = (id, ...ends) => ({ id, turns: ends.map((e) => ({ ended_at: e })) });
  assert.equal(ended([s("a", 10), s("b", 5)]), false);
  // a's turn at 20 ended and a queued prompt started the next turn at once: the last turn has not ended, the one before did.
  assert.equal(ended([s("a", 10, 20, null), s("b", 5)]), true);
  // The session holding the latest end starts a new turn: nothing ended, and 20 stays the watermark.
  assert.equal(ended([s("a", 10, 20, null), s("b", 5)]), false);
  // An older turn ending later than before is still a turn that ended, but not an older one than the watermark.
  assert.equal(ended([s("a", 10, 20, null), s("b", 5, 15)]), true);
  assert.equal(ended([s("a", 10, 20, null), s("b", 5, 15)]), false);
});

test("a turn ending on a quiet Home does not mark the stale item set aside before it shown (C8)", async () => {
  calls.length = 0;
  resetHome();
  const box = new Field("div");
  const r3 = retest("r3");
  drawHome(box, [live("a", "running", { turns: [{ ended_at: 1 }] })], [], null);
  homeState.quiet = true;
  // The turn of `a` ends while Home is visible, and the old reply is still the next Retest.
  drawHome(box, [live("a", "your_turn", { turns: [{ ended_at: 2 }] })], [], r3);
  await settle();
  assert.deepEqual(calls.filter(([c, a]) => c === "quiz_tap" && a.id === "r3"), []);
  assert.equal(homeState.quiet, false);
});

test("a pick on Home leaves no why waiting in the rail, even under the trial (C9)", async () => {
  calls.length = 0;
  const { slotNode } = await import("../../dist/home.js");
  const { whyWaiting } = await import("../../dist/quiz.js");
  const node = slotNode({ item: retest("w9"), next: false }, () => {}, () => {});
  node.find("button").find((b) => b.textContent === "A").listeners.click();
  assert.equal(whyWaiting("w9"), false);
});

test("a running row's elapsed time and an unseen row's age count up like the list's (C12)", () => {
  resetHome();
  const box = new Field("div");
  const now = Date.now();
  drawHome(box, [live("r", "running", { turn_span: { start: now - 60_000 } }), live("u", "your_turn", { unseen: true, turns: [{ ended_at: now - 60_000 }] })], [], null);
  const clocks = box.find("span").filter((n) => n.className === "clock");
  assert.equal(clocks.length, 2);
  for (const c of clocks) assert.match(String(c.dataset.key), /^count /);
});

test("an unseen row has two lines and the status shape beside the name (S6)", () => {
  resetHome();
  const box = new Field("div");
  drawHome(box, [live("u", "your_turn", { unseen: true, turns: [{ ended_at: Date.now() }] })], [], null);
  const row = box.find("button").find((b) => b.dataset.key === "unseen u");
  assert.ok(row.find("span").some((n) => n.dataset.glyph === "filled"));
});

test("patching by key keeps the slot's card when rows above it change (C11)", async () => {
  const { patch } = await import("../../dist/dom.js");
  const keyed = (tag, key, text) => Object.assign(new Field(tag, {}, text ? [text] : []), { attrs: new Map([["data-key", key]]) });
  const parent = new Field("div");
  const [a, b, slot] = [keyed("button", "a", "a"), keyed("button", "b", "b"), keyed("div", "slot", "card")];
  parent.childNodes = [a, b, slot];
  // A row above is dropped.
  patch(parent, [keyed("button", "a", "a"), keyed("div", "slot", "card")]);
  assert.deepEqual(parent.childNodes.map((n) => n.getAttribute("data-key")), ["a", "slot"]);
  assert.equal(parent.childNodes[1], slot);
  // A row is added above.
  patch(parent, [keyed("button", "a", "a"), keyed("button", "c", "c"), keyed("div", "slot", "card")]);
  assert.deepEqual(parent.childNodes.map((n) => n.getAttribute("data-key")), ["a", "c", "slot"]);
  assert.equal(parent.childNodes[2], slot);
  // Unkeyed nodes still match by place.
  const t = new Field("div", {}, ["x"]);
  const p2 = new Field("div", {}, []);
  p2.childNodes = [t];
  patch(p2, [new Field("div", {}, ["y"])]);
  assert.equal(p2.childNodes[0], t);
});

