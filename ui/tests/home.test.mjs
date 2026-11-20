// Home (spec #53): the sessions that need Daniel, then the unseen ones, then a row per project; and
// its ways in. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { homeView } from "../../dist/home.js";
import { initial } from "../../dist/settings.js";
import { windowBinding } from "../../dist/keys.js";
import { listSpace } from "../../dist/home.js";

const key = (code, k, mods = {}) => ({ key: k, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods });

const turn = (ended_at) => ({ ended_at });
const live = (id, state, extra = {}) => ({ id, state, unseen: false, landed: false, turns: [], questions: [], transcript: {}, ...extra });
const view = (sessions, state = { expanded: new Set() }, extra = {}) => homeView(sessions, extra.retests ?? [], extra.projects ?? [], state, extra.headline);
const keys = (parts) => parts.map((p) => p.key);
const sessionIds = (part) => part.rows.map((r) => r.session.id);
const quiz = { text: "q", key: 0, options: [] };
const retest = (id, quiz_, taps = []) => ({ id, of: id, session_id: "s", added_at: 1, quiz: quiz_, taps });

test("Home's parts come in order, each absent when empty", () => {
  assert.deepEqual(keys(view([])), []);
  const all = view(
    [live("l", "idle", { landed: true }), live("u", "your_turn", { unseen: true }), live("r", "running"), live("n", "needs_you")],
    undefined,
    { retests: [retest("a", quiz)], projects: ["/w/plotkeeper"] },
  );
  assert.deepEqual(keys(all), ["needs_you", "running", "unseen", "learning", "landed", "start"]);
  assert.deepEqual(keys(view([live("r", "running")])), ["running"]);
  assert.deepEqual(keys(view([live("d", "idle", { unseen: true })])), ["unseen"]);
});

test("5 running sessions give 4 rows and +1, and expanded gives all 5", () => {
  const five = ["a", "b", "c", "d", "e"].map((id) => live(id, "running"));
  const [part] = view(five);
  assert.deepEqual([sessionIds(part), part.more], [["a", "b", "c", "d"], 1]);
  const [open] = view(five, { expanded: new Set(["running"]) });
  assert.deepEqual([sessionIds(open), open.more], [["a", "b", "c", "d", "e"], 0]);
  assert.equal(view(five.slice(0, 4))[0].more, 0);
});

test("4 unseen sessions give 3 rows and +1, newest turn first; a seen turn leaves", () => {
  const unseen = (id, at) => live(id, "your_turn", { unseen: true, turns: [turn(at)] });
  const four = [unseen("a", 10), unseen("b", 40), unseen("c", 30), unseen("d", 20)];
  const [part] = view(four);
  assert.deepEqual([sessionIds(part), part.more], [["b", "c", "d"], 1]);
  assert.deepEqual(sessionIds(view([unseen("a", 10), { ...unseen("b", 40), unseen: false }])[0]), ["a"]);
  assert.deepEqual(view([{ ...unseen("a", 10), unseen: false }]), []);
});

test("an unseen row holds its turn's Headline; a needs-you row holds its question's first line", () => {
  const seen = live("a", "your_turn", { unseen: true, turns: [turn(1)] });
  const [unseen] = view([seen], undefined, { headline: (s) => (s.id === "a" ? "Delete keeps the branch" : null) });
  assert.equal(unseen.rows[0].line, "Delete keeps the branch");
  const asking = live("n", "needs_you", { questions: [{ answer: "open", questions: [{ question: "Name from prompt?\nOr branch?" }] }] });
  assert.equal(view([asking])[0].rows[0].line, "Name from prompt?");
});

test("a running row holds its intention and how old it is", () => {
  const running = live("r", "running", { transcript: { intention: "Fix the lock", steps_ago: 3 } });
  const row = view([running])[0].rows[0];
  assert.deepEqual([row.line, row.note], ["Fix the lock", "3 steps ago"]);
  assert.equal(view([live("q", "running", { transcript: { intention: "x", steps_ago: 0 } })])[0].rows[0].note, "");
});

test("Needs you is the only part with the colour class", () => {
  const all = view(
    [live("l", "idle", { landed: true }), live("u", "your_turn", { unseen: true }), live("r", "running"), live("n", "needs_you")],
    undefined,
    { retests: [retest("a", quiz)], projects: ["/w/plotkeeper"] },
  );
  assert.deepEqual(all.filter((p) => p.colour).map((p) => p.key), ["needs_you"]);
});

test("one part per kind of landed session, which are not running or needs-you", () => {
  const parts = view([live("a", "idle", { landed: true }), live("b", "needs_you", { landed: true }), live("c", "running", { landed: true }), live("d", "ended", { landed: true })]);
  assert.deepEqual(parts.map((p) => [p.key, sessionIds(p)]), [["needs_you", ["b"]], ["running", ["c"]], ["landed", ["a", "d"]]]);
});

test("one row per project, named by its folder, in projects.json order", () => {
  const start = view([], undefined, { projects: ["/Users/d/work/plotkeeper", "/Users/d/work/api"] })[0];
  assert.deepEqual(start.projects, [
    { name: "plotkeeper", path: "/Users/d/work/plotkeeper" },
    { name: "api", path: "/Users/d/work/api" },
  ]);
});

test("Go home has a default key, which keymap.json can rebind", () => {
  assert.equal(windowBinding(initial.keymap, key("KeyH", "h", { metaKey: true, shiftKey: true }), false), "studio::GoHome");
  const rebound = { Workspace: { "cmd-g": "studio::GoHome" } };
  assert.equal(windowBinding(rebound, key("KeyG", "g", { metaKey: true }), false), "studio::GoHome");
});

test("a click on the list's empty space goes home; a click on a row, a heading or a control does not", () => {
  const target = (...inside) => ({ closest: (selector) => (inside.includes(selector) ? {} : null) });
  assert.equal(listSpace(target()), true);
  assert.equal(listSpace(target(".item")), false);
  assert.equal(listSpace(target("summary")), false);
  assert.equal(listSpace(target(".project-workspaces")), false);
  assert.equal(listSpace(target("button")), false);
});

test("Home offers the retests that have a Quiz and are not answered, and keeps one it showed answered", async () => {
  const { homeRetests } = await import("../../dist/home.js");
  const quiz = { text: "q", key: 0, options: [] };
  const retest = (id, quiz_, taps = []) => ({ id, of: id, session_id: "s", added_at: 1, quiz: quiz_, taps });
  // One rejected by the guards has no Quiz; an answered one that was never shown is done.
  assert.deepEqual(
    homeRetests([retest("a", quiz), retest("b", null), retest("c", quiz, [{ kind: "pick", option: 0, at: 2 }]), retest("d", quiz, [{ kind: "confidence", sure: true, at: 2 }])]).map((r) => r.id),
    ["a", "d"],
  );
});
