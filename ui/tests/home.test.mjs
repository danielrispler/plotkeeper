// Home (spec #53): the sessions that need Daniel, then the unseen ones, then a row per project; and
// its ways in. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { Field, fakeDocument, tauri } from "./dom.mjs";
import { homeView } from "../../dist/home.js";
import { initial } from "../../dist/settings.js";
import { windowBinding } from "../../dist/keys.js";
import { listSpace } from "../../dist/home.js";
import { recheckSeen } from "../../dist/seen.js";

const key = (code, k, mods = {}) => ({ key: k, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods });

const turn = (ended_at) => ({ ended_at });
const live = (id, state, extra = {}) => ({ id, state, unseen: false, landed: false, turns: [], questions: [], transcript: {}, ...extra });
const fresh = () => ({ expanded: new Set(), cleared: new Set(), quiet: false, held: null });
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
  assert.deepEqual(keys(view([live("d", "your_turn", { unseen: true })])), ["unseen"]);
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

test("an unseen session that went idle stays under Unseen, and a running one is listed only under Running", () => {
  const parts = view([live("i", "idle", { unseen: true }), live("r", "running", { unseen: true })]);
  assert.deepEqual(
    parts.map((p) => [p.key, sessionIds(p)]),
    [
      ["running", ["r"]],
      ["unseen", ["i"]],
    ],
  );
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
    { name: "plotkeeper", path: "/Users/d/work/plotkeeper", fogged: false },
    { name: "api", path: "/Users/d/work/api", fogged: false },
  ]);
});

test("a project whose map shows a fogged Core part is marked, with no count", () => {
  const start = homeView([], null, ["/w/a", "/w/b"], fresh(), undefined, new Set(["/w/b"]))[0];
  assert.deepEqual(start.projects.map((p) => [p.path, p.fogged]), [["/w/a", false], ["/w/b", true]]);
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
  // The store's answer that is still the answered item (a re-ask that landed before the pick) is not another item.
  assert.equal(slotOf(retest("a", quiz), answered).next, false);
  // Unanswered, the slot shows what the store chose, and no Next.
  assert.equal(slotOf(retest("b", quiz), null).next, false);
});

test("shown is due once per item, and never for a Retest that has it", async () => {
  const { shownDue } = await import("../../dist/home.js");
  const item = retest("s1", quiz);
  assert.equal(shownDue(item), true);
  assert.equal(shownDue(null), false);
  // A fresh Quiz counts too.
  assert.equal(shownDue({ kind: "fresh", id: "t9", session_id: "s", quiz }), true);
  assert.equal(shownDue(retest("s2", quiz, [{ kind: "shown", at: 1 }])), false);
});

test("entering Home from another view starts it afresh, and drawing it again while on it keeps a reveal and the expanded groups", async () => {
  const { enterHome } = await import("../../dist/home.js");
  const state = { ...fresh(), expanded: new Set(["running"]), quiet: true, held: retest("a", quiz) };
  enterHome(true, state);
  assert.deepEqual([[...state.expanded], state.quiet, state.held?.id], [["running"], true, "a"]);
  enterHome(false, state);
  assert.deepEqual([[...state.expanded], state.quiet, state.held], [[], false, null]);
});

test("the newest request wins: a slower, older reply is dropped", async () => {
  const { newest } = await import("../../dist/dom.js");
  const take = newest();
  let first, second;
  const a = take(new Promise((r) => (first = r)));
  const b = take(new Promise((r) => (second = r)));
  second("newer");
  first("older");
  assert.deepEqual([await a, await b], [undefined, "newer"]);
});

test("building the slot writes nothing; shown is written once the card is seen, in a focused window with Home open", async () => {
  const { mock } = await import("node:test");
  const observers = [];
  globalThis.IntersectionObserver = class {
    constructor(run) {
      this.run = run;
      observers.push(this);
    }
    observe() {}
    disconnect() {}
  };
  const scrollTo = (ratio, viewport) =>
    observers.at(-1).run([{ intersectionRatio: ratio, intersectionRect: { width: 100, height: viewport * 100 }, rootBounds: { width: 100, height: 100 } }]);
  const calls = [];
  tauri(calls);
  globalThis.document = fakeDocument({ visibilityState: "visible", hasFocus: () => true }, Field);
  mock.timers.enable({ apis: ["setTimeout"] });
  const { slotNode, drawHome, watchLearningShown } = await import("../../dist/home.js");
  let home = true;
  const items = [{ ...retest("w1", quiz) }, { kind: "fresh", id: "t5", session_id: "s", quiz }];
  for (const item of items) {
    calls.length = 0;
    slotNode({ item, next: false }, () => {}, () => {});
    const box = new Field("div");
    box.querySelector = () => null;
    drawHome(box, [], [], item);
    assert.deepEqual(calls, []);
    const node = new Field("div");
    watchLearningShown(node, item, () => home);
    // Built but off screen, and then seen with Home closed or the window unfocused: nothing.
    mock.timers.tick(5000);
    assert.deepEqual(calls, []);
    scrollTo(1, 1);
    home = false;
    mock.timers.tick(5000);
    assert.deepEqual(calls, []);
    home = true;
    document.hasFocus = () => false;
    mock.timers.tick(5000);
    assert.deepEqual(calls, []);
    document.hasFocus = () => true;
    // Back in a focused window with Home open: written once, however often Home is drawn again.
    recheckSeen();
    watchLearningShown(node, item, () => home);
    mock.timers.tick(5000);
    watchLearningShown(node, item, () => home);
    mock.timers.tick(5000);
    assert.deepEqual(calls, [["quiz_tap", { id: item.id, tap: { kind: "shown" } }]], item.kind);
  }
  mock.timers.reset();
});

test("a project's mark keeps its one watch while a redraw keeps its node, and a new node takes the watch over", async () => {
  const watchers = [];
  globalThis.IntersectionObserver = class {
    constructor() {
      this.live = true;
      watchers.push(this);
    }
    observe() {}
    disconnect() {
      this.live = false;
    }
  };
  globalThis.document = fakeDocument({ visibilityState: "visible", hasFocus: () => true }, Field);
  tauri([]);
  const { watchMark } = await import("../../dist/home.js");
  const [a, b] = [new Field("span"), new Field("span")];
  watchMark("/w/p", a);
  watchMark("/w/p", a);
  assert.equal(watchers.length, 1, "the same node keeps its watch");
  watchMark("/w/p", b);
  assert.equal(watchers.length, 2);
  assert.deepEqual(watchers.map((w) => w.live), [false, true], "the node that left the DOM is no longer watched");
  watchMark("/w/p", null);
  assert.equal(watchers[1].live, false, "a project with no mark has no watch");
});

test("the window gaining focus draws Home again, which reads the marks and the review; another view is left alone", async () => {
  const { onWindowFocus } = await import("../../dist/home.js");
  let draws = 0;
  onWindowFocus(false, () => draws++);
  assert.equal(draws, 0);
  onWindowFocus(true, () => draws++);
  assert.equal(draws, 1);
});

// The slot's card (#242): the options, then "Not sure" and "Not now" inside it; a pick writes the confidence tap first.
async function slotCard(id, extra = {}, send = null) {
  const calls = [];
  tauri(calls);
  if (send) globalThis.window.__TAURI__.core.invoke = send;
  globalThis.document = fakeDocument({ visibilityState: "visible", hasFocus: () => true }, Field);
  const { slotNode } = await import("../../dist/home.js");
  const item = retest(id, { text: "q", key: 1, options: [{ text: "A", why: "a" }, { text: "B", why: "b" }] }, []);
  const node = slotNode({ item: { ...item, ...extra }, next: false }, () => {}, () => {});
  const buttons = () => node.find("button");
  const press = (label) => buttons().find((b) => b.textContent === label).listeners.click();
  const again = () => slotNode({ item: { ...item, ...extra }, next: false }, () => {}, () => {});
  return { calls, node, again, buttons, press, labels: () => buttons().map((b) => b.textContent) };
}
const taps = (calls) => calls.map(([, args]) => args.tap);
const settle = () => new Promise((r) => setTimeout(r, 30));

test("a confident Home pick is one click: a sure confidence tap for Home, then the pick, and no why", async () => {
  const { calls, press } = await slotCard("c1");
  press("A");
  await settle();
  assert.deepEqual(taps(calls), [{ kind: "confidence", sure: true, surface: "home" }, { kind: "pick", option: 0 }]);
});

test("the pick is sent only after the confidence tap has landed", async () => {
  const events = [];
  const { press } = await slotCard("c0", {}, async (command, args) => {
    events.push(`start ${args.tap.kind}`);
    if (args.tap.kind === "confidence") await new Promise((r) => setTimeout(r, 15));
    events.push(`end ${args.tap.kind}`);
    return null;
  });
  press("A");
  await new Promise((r) => setTimeout(r, 60));
  assert.deepEqual(events, ["start confidence", "end confidence", "start pick", "end pick"]);
});

test("a pick on a card's fresh Quiz on Home records its guess, and the rail shows it answered", async () => {
  const { calls, press } = await slotCard("t7", { kind: "fresh", session_id: "s", of: undefined });
  press("B");
  await settle();
  const { quizAnswered } = await import("../../dist/quiz.js");
  assert.equal(quizAnswered("t7"), true);
  assert.deepEqual(taps(calls).at(-1), { kind: "pick", option: 1 });
});

test("with the not sure toggle on, the Home pick writes sure false first", async () => {
  const { calls, press } = await slotCard("c2");
  press("Not sure");
  assert.deepEqual(calls, []);
  press("B");
  await settle();
  assert.deepEqual(taps(calls), [{ kind: "confidence", sure: false, surface: "home" }, { kind: "pick", option: 1 }]);
});

test("Home's card holds the options, then Not sure and Not now, with no Sure row and no Show the rest anyway", async () => {
  const { labels } = await slotCard("c3");
  assert.deepEqual(labels(), ["A", "B", "Not sure", "Not now"]);
});

test("an item that arrives already answered still offers a way on: Next sets it aside", async () => {
  const { calls, press, labels } = await slotCard("c5", { taps: [{ kind: "skip", at: 1 }] });
  assert.ok(labels().includes("Next"), labels().join());
  press("Next");
  await settle();
  assert.deepEqual(taps(calls), [{ kind: "not_now" }]);
});

test("the reveal shows the turn's date and Key wrong", async () => {
  const { node, press, labels } = await slotCard("c4", { of_at: Date.UTC(2026, 10, 13, 12) });
  press("A");
  assert.ok(node.text.includes("13 November"), node.text);
  assert.ok(labels().includes("Key wrong"));
  assert.ok(!labels().includes("Not now"));
});

test("a fresh Quiz's reveal shows the turn's date from `at`", async () => {
  const { node, press } = await slotCard("c6", { kind: "fresh", at: Date.UTC(2026, 10, 19, 12) });
  press("A");
  assert.ok(node.text.includes("19 November"), node.text);
});

const stored = ["--- ui/settings.rs", "@@ -1,2 +1,2 @@", "-old line", "+new line", "--- core/lib.rs", "@@ -5 +5 @@", "+other", ""].join("\n");

test("a pick draws the item's stored hunk in place, a caption and code per file, with no git read and no hunk_opened", async () => {
  const { calls, again, press } = await slotCard("c7", { hunk: stored });
  press("A");
  await settle();
  // Home redraws the slot after the pick.
  const node = again();
  const pres = node.find("pre");
  assert.equal(pres.length, 2);
  assert.ok(pres[0].textContent.includes("+new line") && !pres[0].textContent.includes("other"), pres[0].textContent);
  assert.ok(node.text.includes("ui/settings.rs") && node.text.includes("core/lib.rs"), node.text);
  assert.deepEqual(calls.map(([c]) => c).filter((c) => c === "turn_file_hunk" || c === "hunk_opened"), []);
});

test("an item with no stored hunk shows no hunk block", async () => {
  const { again, press } = await slotCard("c8", { hunk: undefined });
  press("A");
  assert.equal(again().find("pre").length, 0);
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
  const written = [];
  const commits = [];
  const m = clearing(time, {
    landed: (id) => (live.get(id)?.landed ? live.get(id) : undefined),
    commit: (s) => (deleted.push(s.id), new Promise((resolve, reject) => commits.push({ id: s.id, resolve, reject }))),
    changed: (list, writing = []) => (pending.push(list.map((s) => s.id)), written.push(writing.map((s) => s.id))),
  });
  return { m, time, live, deleted, pending, written, commits };
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
  time.tick(CLEAR_WINDOW_MS * 3);
  assert.deepEqual(deleted, ["a", "b"]);
});

test("committed rows stay hidden until they leave the list, and a Clear before then does not commit them again", () => {
  const { m, time, live: sessions, deleted } = clearMachine([landed("a"), landed("b")]);
  m.clear([landed("a")]);
  time.tick(CLEAR_WINDOW_MS);
  assert.deepEqual([...m.hidden], ["a"]);
  // Home offers only the rows still shown, which leave "a" out.
  m.clear([landed("b")]);
  time.tick(CLEAR_WINDOW_MS);
  assert.deepEqual(deleted, ["a", "b"]);
  // The delete lands: the sessions leave the list and their rows stop being hidden.
  sessions.clear();
  m.sync();
  assert.deepEqual([...m.hidden], []);
});

test("a session stays told to the shell until its delete is written, and a failed delete brings the row back", async () => {
  const { m, time, written, commits } = clearMachine([landed("a"), landed("b")]);
  m.clear([landed("a"), landed("b")]);
  time.tick(CLEAR_WINDOW_MS);
  // The window closed with both deletes in flight: the shell still hears both, so a quit now commits them.
  assert.deepEqual(written.at(-1), ["a", "b"]);
  commits[0].resolve();
  await new Promise((r) => setImmediate(r));
  assert.deepEqual(written.at(-1), ["b"]);
  commits[1].reject(new Error("no"));
  await new Promise((r) => setImmediate(r));
  assert.deepEqual(written.at(-1), []);
  assert.deepEqual([...m.hidden], ["a"]);
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

// One rule for Clear and Delete (#270): an ended session keeps its terminal, which a later session may now run in.
// @law: delete-keeps-live-terminal
test("a terminal is in use while a session not leaving, and not ended, runs in it", async () => {
  const { terminalInUse } = await import("../../dist/home.js");
  const old = live("old", "ended", { terminal: "t1" });
  const now = live("now", "running", { terminal: "t1" });
  const leaving = (ids) => (id) => ids.includes(id);
  assert.equal(terminalInUse("t1", [old, now], leaving(["old"])), true);
  assert.equal(terminalInUse("t1", [old, now], leaving(["old", "now"])), false);
  assert.equal(terminalInUse("t1", [old, live("gone", "ended", { terminal: "t1" })], leaving(["old"])), false);
  assert.equal(terminalInUse("t1", [old, live("other", "running", { terminal: "t2" })], leaving(["old"])), false);
});

test("Home hides the cleared rows, and shows the Landed part again after Undo", () => {
  const sessions = [landed("a"), landed("b")];
  const hidden = { expanded: new Set(), cleared: new Set(["a", "b"]) };
  assert.deepEqual(keys(view(sessions, hidden)), []);
  assert.deepEqual(sessionIds(view(sessions, { expanded: new Set(), cleared: new Set(["a"]) })[0]), ["b"]);
  assert.deepEqual(sessionIds(view(sessions, { expanded: new Set(), cleared: new Set() })[0]), ["a", "b"]);
});

test("the card's label names the Retest's age and the session of its first Quiz; a fresh Quiz names its session; done has none", async () => {
  const { slotLabel } = await import("../../dist/home.js");
  const now = Date.UTC(2026, 10, 20);
  const day = 864e5;
  const session = { name: null, identity: { issue: 44, name: "" }, folder_name: "plotkeeper" };
  const aged = { ...retest("r", quiz), of_at: now - 8 * day };
  assert.equal(slotLabel(aged, session, now), "Retest · 8 days after #44");
  assert.equal(slotLabel({ ...aged, of_at: now - 7 * day }, session, now), "Retest · 7 days after #44");
  // The session is gone: only the age.
  assert.equal(slotLabel(aged, undefined, now), "Retest · 8 days ago");
  assert.equal(slotLabel(retest("r", quiz), undefined, now), "Retest");
  assert.equal(slotLabel({ kind: "fresh", id: "t1", session_id: "s", quiz }, session, now), "Quiz · #44");
  assert.equal(slotLabel({ kind: "fresh", id: "t1", session_id: "s", quiz }, undefined, now), "Quiz");
  assert.equal(slotLabel({ kind: "done" }, session, now), "");
  // A session with no issue number is named as the list names it.
  assert.equal(slotLabel({ kind: "fresh", id: "t1", session_id: "s", quiz }, { name: "Row controls", identity: { issue: null, name: "" }, folder_name: "x" }, now), "Quiz · Row controls");
});

test("the learning part has no heading of its own, as the card carries its label", () => {
  assert.equal(view([], undefined, { learning: retest("a", quiz) }).find((p) => p.key === "learning").title, "");
});

test("a mark that was seen is watched again by the next redraw, so a later day is logged while Studio stays open", async () => {
  const observers = [];
  globalThis.IntersectionObserver = class {
    constructor(run) {
      this.run = run;
      observers.push(this);
    }
    observe() {}
    disconnect() {}
  };
  globalThis.document = fakeDocument({ visibilityState: "visible", hasFocus: () => true }, Field);
  globalThis.window = { ...globalThis.window, innerWidth: 100, innerHeight: 100 };
  const { watchMark } = await import("../../dist/home.js");
  const { showHome, homeShown } = await import("../../dist/panes.js");
  tauri([]);
  // `show` records Home as the view shown before it draws; the rest of drawing Home is not what this test reads.
  await Promise.resolve(showHome()).catch(() => {});
  assert.equal(homeShown(), true);
  const node = new Field("span");
  watchMark("/w/seen", node);
  observers[0].run([{ intersectionRatio: 1, intersectionRect: { width: 1, height: 1 }, rootBounds: { width: 100, height: 100 } }]);
  await new Promise((done) => setTimeout(done, 650));
  watchMark("/w/seen", node);
  assert.equal(observers.length, 2, "the redraw that keeps the node watches it again once it fired");
  watchMark("/w/seen", null);
});
