// Home (spec #53): the sessions that need Daniel, then the unseen ones, then a row per project; and
// its ways in. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { homeView, markTitle } from "../../dist/home.js";
import { initial } from "../../dist/settings.js";
import { windowBinding } from "../../dist/keys.js";
import { listSpace } from "../../dist/home.js";
import { recheckSeen } from "../../dist/seen.js";

const key = (code, k, mods = {}) => ({ key: k, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods });

const turn = (ended_at) => ({ ended_at });
const live = (id, state, extra = {}) => ({ id, state, unseen: false, landed: false, turns: [], questions: [], transcript: {}, ...extra });
const fresh = () => ({ expanded: new Set(), cleared: new Set() });
const view = (sessions, state = fresh(), extra = {}) => homeView(sessions, extra.projects ?? [], state, extra.headline);
const keys = (parts) => parts.map((p) => p.key);
const sessionIds = (part) => part.rows.map((r) => r.session.id);

test("Home's parts come in order, each absent when empty", () => {
  assert.deepEqual(keys(view([])), []);
  const all = view(
    [live("l", "idle", { landed: true }), live("u", "your_turn", { unseen: true }), live("r", "running"), live("n", "needs_you")],
    undefined,
    { projects: ["/w/plotkeeper"] },
  );
  assert.deepEqual(keys(all), ["needs_you", "running", "unseen", "landed", "projects"]);
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
    { projects: ["/w/plotkeeper"] },
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
  const start = homeView([], ["/w/a", "/w/b"], fresh(), undefined, new Set(["/w/b"]))[0];
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
  assert.equal(listSpace(target("button")), false);
});

test("entering Home from another view collapses the expanded groups, and drawing it again while on it keeps them", async () => {
  const { enterHome } = await import("../../dist/home.js");
  const state = { ...fresh(), expanded: new Set(["running"]) };
  enterHome(true, state);
  assert.deepEqual([...state.expanded], ["running"]);
  enterHome(false, state);
  assert.deepEqual([...state.expanded], []);
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

test("the window gaining focus draws Home again, which reads the marks; another view is left alone", async () => {
  const { onWindowFocus } = await import("../../dist/home.js");
  let draws = 0;
  onWindowFocus(false, () => draws++);
  assert.equal(draws, 0);
  onWindowFocus(true, () => draws++);
  assert.equal(draws, 1);
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

test("Home hides the cleared rows, and shows the Landed part again after Undo", () => {
  const sessions = [landed("a"), landed("b")];
  const hidden = { expanded: new Set(), cleared: new Set(["a", "b"]) };
  assert.deepEqual(keys(view(sessions, hidden)), []);
  assert.deepEqual(sessionIds(view(sessions, { expanded: new Set(), cleared: new Set(["a"]) })[0]), ["b"]);
  assert.deepEqual(sessionIds(view(sessions, { expanded: new Set(), cleared: new Set() })[0]), ["a", "b"]);
});

test("a project's dot also lights for a waiting PR brief, and says so on hover", () => {
  // The shell's `map_marks` names the project (a ready PR with a stored brief); Home marks it as it marks fog, with no count.
  const start = homeView([], ["/w/a", "/w/b"], fresh(), undefined, new Set(["/w/a"]))[0];
  assert.deepEqual(start.projects.map((p) => [p.path, p.fogged]), [["/w/a", true], ["/w/b", false]]);
  assert.match(markTitle, /PR brief is waiting/);
});
