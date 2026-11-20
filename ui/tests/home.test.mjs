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
const fresh = () => ({ expanded: new Set(), quiet: false, held: null });
const view = (sessions, state = fresh(), extra = {}) => homeView(sessions, extra.learning ?? null, extra.projects ?? [], state, extra.headline);
const keys = (parts) => parts.map((p) => p.key);
const sessionIds = (part) => part.rows.map((r) => r.session.id);
const quiz = { text: "q", key: 0, options: [] };
const retest = (id, quiz_, taps = []) => ({ kind: "retest", id, of: id, session_id: "s", added_at: 1, quiz: quiz_, taps });

test("Home's parts come in order, each absent when empty", () => {
  assert.deepEqual(keys(view([])), []);
  const all = view(
    [live("l", "idle", { landed: true }), live("u", "your_turn", { unseen: true }), live("r", "running"), live("n", "needs_you")],
    undefined,
    { learning: retest("a", quiz), projects: ["/w/plotkeeper"] },
  );
  assert.deepEqual(keys(all), ["needs_you", "running", "unseen", "learning", "landed", "start"]);
  assert.deepEqual(keys(view([live("r", "running")])), ["running"]);
  assert.deepEqual(keys(view([live("d", "idle", { unseen: true })])), ["unseen"]);
});

test("5 running sessions give 4 rows and +1, and expanded gives all 5", () => {
  const five = ["a", "b", "c", "d", "e"].map((id) => live(id, "running"));
  const [part] = view(five);
  assert.deepEqual([sessionIds(part), part.more], [["a", "b", "c", "d"], 1]);
  const [open] = view(five, { ...fresh(), expanded: new Set(["running"]) });
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
    { learning: retest("a", quiz), projects: ["/w/plotkeeper"] },
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

test("quiet hides the learning slot until the Home state is reset", async () => {
  const { resetHome } = await import("../../dist/home.js");
  const state = { ...fresh(), quiet: true };
  assert.deepEqual(keys(view([], state, { learning: retest("a", quiz) })), []);
  resetHome(state);
  assert.deepEqual(keys(view([], state, { learning: retest("a", quiz) })), ["learning"]);
});

test("done for today shows only in its case", () => {
  const slotOf = (learning) => view([], undefined, { learning }).find((p) => p.key === "learning")?.slot;
  assert.equal(slotOf({ kind: "done" }).item.kind, "done");
  assert.equal(slotOf(retest("a", quiz)).item.kind, "retest");
  assert.equal(slotOf(null), undefined);
});

test("a fresh Quiz fills the slot as the store chose it", () => {
  const slotOf = (learning) => view([], undefined, { learning }).find((p) => p.key === "learning")?.slot;
  assert.equal(slotOf({ kind: "fresh", id: "t1", session_id: "s1", quiz }).item.kind, "fresh");
});

test("an item answered here stays with its reveal, and Next shows only when another item is due", () => {
  const answered = retest("a", quiz, [{ kind: "pick", option: 0, at: 2 }]);
  const slotOf = (learning, held) => view([], { ...fresh(), held }, { learning }).find((p) => p.key === "learning")?.slot;
  assert.deepEqual([slotOf(retest("b", quiz), answered).item.id, slotOf(retest("b", quiz), answered).next], ["a", true]);
  assert.equal(slotOf({ kind: "done" }, answered).next, false);
  assert.equal(slotOf(null, answered).next, false);
  // Unanswered, the slot shows what the store chose, and no Next.
  assert.equal(slotOf(retest("b", quiz), null).next, false);
});

test("shown is due only with Home active, the window visible and focused, and the Retest on screen", async () => {
  const { shownDue } = await import("../../dist/home.js");
  const item = retest("s1", quiz);
  assert.equal(shownDue(item, true, true, true), true);
  assert.equal(shownDue(item, false, true, true), false);
  assert.equal(shownDue(item, true, false, true), false);
  assert.equal(shownDue(item, true, true, false), false);
  assert.equal(shownDue(null, true, true, true), false);
  assert.equal(shownDue({ kind: "done" }, true, true, true), false);
  assert.equal(shownDue(retest("s2", quiz, [{ kind: "shown", at: 1 }]), true, true, true), false);
});

test("building the slot writes nothing; shown is written once, when its conditions hold", async () => {
  const { El: Base } = await import("./dom.mjs");
  class El extends Base {
    dataset = {};
    title = "";
    textContent = "";
    disabled = false;
  }
  const calls = [];
  globalThis.window = { __TAURI__: { core: { invoke: async (command, args) => (calls.push([command, args]), null) } } };
  globalThis.document = { createElement: (tag) => new El(tag), visibilityState: "hidden", hasFocus: () => true, getElementById: () => Object.assign(new El("div"), { replaceChildren() {} }) };
  const { slotNode, drawHome, markLearningShown } = await import("../../dist/home.js");
  const item = { ...retest("w1", { text: "q", key: 0, options: [{ text: "A", why: "a" }] }) };
  slotNode({ item, next: false }, () => {}, () => {});
  assert.deepEqual(calls, []);
  const box = new El("div");
  box.dataset = {};
  drawHome(box, [], [], item);
  // The window is hidden: the background mount wrote nothing.
  assert.deepEqual(calls, []);
  markLearningShown(true);
  assert.deepEqual(calls, []);
  document.visibilityState = "visible";
  markLearningShown(false);
  assert.deepEqual(calls, []);
  markLearningShown(true);
  markLearningShown(true);
  assert.deepEqual(calls, [["quiz_tap", { id: "w1", tap: { kind: "shown" } }]]);
});

// The slot's card (#242): the options, then "Not sure" and "Not now" inside it; a pick writes the confidence tap first.
async function slotCard(id, extra = {}) {
  const { El: Base } = await import("./dom.mjs");
  class El extends Base {
    dataset = {};
    title = "";
    disabled = false;
    dir = "";
  }
  const calls = [];
  globalThis.window = { __TAURI__: { core: { invoke: async (command, args) => (calls.push([command, args]), null) } } };
  globalThis.document = { createElement: (tag) => new El(tag), visibilityState: "visible", hasFocus: () => true, getElementById: () => Object.assign(new El("div"), { replaceChildren() {} }) };
  const { slotNode } = await import("../../dist/home.js");
  const item = retest(id, { text: "q", key: 1, options: [{ text: "A", why: "a" }, { text: "B", why: "b" }] }, []);
  const node = slotNode({ item: { ...item, ...extra }, next: false }, () => {}, () => {});
  const buttons = () => node.find("button");
  const press = (label) => buttons().find((b) => b.textContent === label).listeners.click();
  return { calls, node, buttons, press, labels: () => buttons().map((b) => b.textContent) };
}
const taps = (calls) => calls.map(([, args]) => args.tap);

test("a confident Home pick is one click: a sure confidence tap for Home, then the pick, and no why", async () => {
  const { calls, press } = await slotCard("c1");
  press("A");
  assert.deepEqual(taps(calls), [{ kind: "confidence", sure: true, surface: "home" }, { kind: "pick", option: 0 }]);
});

test("with the not sure toggle on, the Home pick writes sure false first", async () => {
  const { calls, press } = await slotCard("c2");
  press("Not sure");
  assert.deepEqual(calls, []);
  press("B");
  assert.deepEqual(taps(calls), [{ kind: "confidence", sure: false, surface: "home" }, { kind: "pick", option: 1 }]);
});

test("Home's card holds the options, then Not sure and Not now, with no Sure row and no Show the rest anyway", async () => {
  const { labels } = await slotCard("c3");
  assert.deepEqual(labels(), ["A", "B", "Not sure", "Not now"]);
});

test("the reveal shows the turn's date and Key wrong", async () => {
  const { node, press, labels } = await slotCard("c4", { of_at: Date.UTC(2026, 10, 13, 12) });
  press("A");
  assert.ok(node.text.includes("13 November"), node.text);
  assert.ok(labels().includes("Key wrong"));
  assert.ok(!labels().includes("Not now"));
});

// Clear landed (#244), a state machine on an injected clock (prior art hover.test.mjs).
import { clearing, CLEAR_WINDOW_MS } from "../../dist/home.js";

function clearMachine(sessions) {
  let now = 0;
  const timers = new Map();
  let next = 1;
  const time = {
    set: (run, ms) => (timers.set(next, { at: now + ms, run }), next++),
    clear: (id) => timers.delete(id),
    tick(ms) {
      now += ms;
      for (const [id, t] of [...timers]) if (t.at <= now) (timers.delete(id), t.run());
    },
  };
  const live = new Map(sessions.map((s) => [s.id, s]));
  const deleted = [];
  const pending = [];
  const m = clearing(time, {
    landed: (id) => (live.get(id)?.landed ? live.get(id) : undefined),
    commit: (s) => deleted.push(s.id),
    changed: (list) => pending.push(list.map((s) => s.id)),
  });
  return { m, time, live, deleted, pending };
}
const landed = (id) => live(id, "idle", { landed: true });

test("Clear hides every landed row, and the window's end deletes each once", () => {
  const { m, time, deleted } = clearMachine([landed("a"), landed("b")]);
  m.clear([landed("a"), landed("b")]);
  assert.deepEqual([...m.hidden], ["a", "b"]);
  time.tick(CLEAR_WINDOW_MS - 1);
  assert.deepEqual(deleted, []);
  time.tick(1);
  assert.deepEqual(deleted, ["a", "b"]);
  assert.deepEqual([...m.hidden], []);
  time.tick(CLEAR_WINDOW_MS * 3);
  assert.deepEqual(deleted, ["a", "b"]);
});

test("Undo restores the rows and nothing is ever deleted", () => {
  const { m, time, deleted, pending } = clearMachine([landed("a")]);
  m.clear([landed("a")]);
  m.undo();
  assert.deepEqual([...m.hidden], []);
  time.tick(CLEAR_WINDOW_MS * 3);
  assert.deepEqual(deleted, []);
  assert.deepEqual(pending, [["a"], []]);
});

test("a session no longer landed when the window closes is spared", () => {
  const { m, time, live: sessions, deleted } = clearMachine([landed("a"), landed("b")]);
  m.clear([landed("a"), landed("b")]);
  sessions.set("b", live("b", "running", { landed: false }));
  sessions.delete("a");
  time.tick(CLEAR_WINDOW_MS);
  assert.deepEqual(deleted, []);
});

test("sync forgets a session that stopped being landed, so the shell commits only the rest", () => {
  const { m, live: sessions, pending } = clearMachine([landed("a"), landed("b")]);
  m.clear([landed("a"), landed("b")]);
  sessions.delete("a");
  m.sync();
  assert.deepEqual([...m.hidden], ["b"]);
  assert.deepEqual(pending.at(-1), ["b"]);
});

test("a second Clear inside the window restarts it and keeps the first rows hidden", () => {
  const { m, time, deleted } = clearMachine([landed("a"), landed("b")]);
  m.clear([landed("a")]);
  time.tick(CLEAR_WINDOW_MS - 1);
  m.clear([landed("b")]);
  time.tick(CLEAR_WINDOW_MS - 1);
  assert.deepEqual(deleted, []);
  time.tick(1);
  assert.deepEqual(deleted, ["a", "b"]);
});

test("Home hides the cleared rows, and shows the Landed part again after Undo", () => {
  const sessions = [landed("a"), landed("b")];
  const hidden = { expanded: new Set(), cleared: new Set(["a", "b"]) };
  assert.deepEqual(keys(view(sessions, hidden)), []);
  assert.deepEqual(sessionIds(view(sessions, { expanded: new Set(), cleared: new Set(["a"]) })[0]), ["b"]);
  assert.deepEqual(sessionIds(view(sessions, { expanded: new Set(), cleared: new Set() })[0]), ["a", "b"]);
});
