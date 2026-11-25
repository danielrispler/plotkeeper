// Live agents on the Map view's parts and its "not placed" list (spec danielrispler/plotkeeper#306, ticket #311), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { arrange, litAgents, liveAgents, notPlacedLine } from "../../dist/map-agents.js";

const file = (path, agent = null) => ({ root: "/w/p", path, from: null, added: 1, removed: 0, agent });
const turn = (files) => ({ started_at: 1, ended_at: null, prompt: "", id: null, message: null, checks: [], listed: [], files: files === null ? null : { files, merged: 0 } });
const worker = (id, extra = {}) => ({ id, parent: "session", kind: "worker", name: `worker ${id}`, status: "running", ...extra });
const session = (id, state, extra = {}) => ({
  id, state, name: `session ${id}`, cwd: "/w/p", shell: false, status_only: false, asker: null, agents: [], turns: [turn([])], last_step: null,
  capabilities: { turn_trees: true }, identity: { name: `identity ${id}` }, ...extra,
});
const here = () => true;
const keys = (live) => live.map((l) => l.key);

test("a running session is on every part its turn touched, a Needs you one loudly", () => {
  const sessions = [
    session("a", "running", { turns: [turn([file("core/x.rs"), file("ui/y.ts")])] }),
    session("b", "needs_you", { turns: [turn([file("core/z.rs")])] }),
  ];
  const live = liveAgents(sessions, here);
  assert.deepEqual(live.map((l) => [l.key, l.needsYou, l.label]), [["a", false, "session a"], ["b", true, "session b"]]);
  const placed = { a: ["core", "window"], b: ["core"] };
  const { byPart, notPlaced } = arrange(live, placed);
  assert.deepEqual(byPart.core.map((l) => l.key), ["a", "b"]);
  assert.deepEqual(byPart.window.map((l) => l.key), ["a"]);
  assert.deepEqual(notPlaced, []);
});

test("only Running and Needs you sessions are live; finished ones, shells and other projects' sessions leave the map", () => {
  const files = [file("core/x.rs")];
  const sessions = ["your_turn", "idle", "ended"].map((state) => session(state, state, { turns: [turn(files)] }));
  sessions.push(session("shell", "running", { shell: true }), session("away", "running", { cwd: "/elsewhere" }), session("on", "running"));
  assert.deepEqual(keys(liveAgents(sessions, (cwd) => cwd === "/w/p")), ["on"]);
});

test("a Worker has its own chip on the parts of its files; a Helper and a finished Worker have none", () => {
  const w = worker("w1");
  const sessions = [session("a", "running", {
    agents: [w, { ...worker("h1"), kind: "helper" }, worker("w2", { status: "completed" })],
    turns: [turn([file("core/own.rs"), file("ui/w.ts", "w1"), file("hook/gone.rs", "w2"), file("hook/h.rs", "h1")])],
  })];
  const live = liveAgents(sessions, here);
  assert.deepEqual(keys(live), ["a", "a/w1"]);
  assert.deepEqual(live[0].paths, ["core/own.rs", "hook/gone.rs", "hook/h.rs"]);
  assert.deepEqual(live[1].paths, ["ui/w.ts"]);
  assert.equal(live[1].label, "worker w1");
  assert.equal(live[1].agent, "w1");
});

test("a Worker the session waits on is the one that needs you", () => {
  const sessions = [session("a", "needs_you", { agents: [worker("w1")], asker: { agent: "w1", since: 1 } })];
  assert.deepEqual(liveAgents(sessions, here).map((l) => [l.key, l.needsYou]), [["a", false], ["a/w1", true]]);
});

test("an in-flight Edit or Write counts before the turn's files are diffed", () => {
  const edit = (tool, detail) => session("a", "running", { turns: [turn(null)], last_step: { tool, detail } });
  assert.deepEqual(liveAgents([edit("Edit", "/w/p/core/a.rs")], here)[0].paths, ["core/a.rs"]);
  assert.deepEqual(liveAgents([edit("Write", "ui/b.ts")], here)[0].paths, ["ui/b.ts"]);
  assert.deepEqual(liveAgents([edit("Bash", "ls")], here)[0].paths, []);
});

test("a session with no changed files, no turn trees, no turns or only ignored edits is in the not placed list", () => {
  const sessions = [
    session("none", "running"),
    session("agy", "running", { capabilities: { turn_trees: false }, turns: [turn(null)] }),
    session("codex", "running", { status_only: true, turns: [] }),
    session("ignored", "needs_you", { turns: [turn([file("docs/a.md")])] }),
    session("placed", "running", { turns: [turn([file("core/a.rs")])] }),
  ];
  const live = liveAgents(sessions, here);
  const { byPart, notPlaced } = arrange(live, { ignored: [], placed: ["core"] });
  assert.deepEqual(keys(notPlaced), ["none", "agy", "codex", "ignored"]);
  assert.deepEqual(byPart.core.map((l) => l.key), ["placed"]);
});

test("a path moved to another part moves the chip with it", () => {
  const live = liveAgents([session("a", "running", { turns: [turn([file("a")])] })], here);
  assert.deepEqual(Object.keys(arrange(live, { a: ["one"] }).byPart), ["one"]);
  assert.deepEqual(Object.keys(arrange(live, { a: ["two"] }).byPart), ["two"]);
});

// Hovering a row in the session list lights that session's chips on the map (#382): its own and its Workers', no other session's.
test("a hovered row lights its session's chips, its Workers' too, and nothing else", () => {
  const sessions = [
    session("a", "running", { agents: [worker("w")], turns: [turn([file("core/x.rs"), file("ui/y.ts", "w")])] }),
    session("b", "needs_you", { turns: [turn([file("core/z.rs")])] }),
  ];
  const live = liveAgents(sessions, here);
  assert.deepEqual(litAgents(live, "a"), ["a", "a/w"]);
  assert.deepEqual(litAgents(live, "b"), ["b"]);
  // A hovered session with no chip, and no hover, light nothing.
  assert.deepEqual(litAgents(live, "c"), []);
  assert.deepEqual(litAgents(live, null), []);
});

// The session open in the left panel keeps its chips lit while it is open (#385), beside a hovered row's.
test("the session open in the panel keeps its chips lit, with a hovered row's", () => {
  const sessions = [
    session("a", "running", { agents: [worker("w")], turns: [turn([file("core/x.rs"), file("ui/y.ts", "w")])] }),
    session("b", "needs_you", { turns: [turn([file("core/z.rs")])] }),
  ];
  const live = liveAgents(sessions, here);
  assert.deepEqual(litAgents(live, null, "a"), ["a", "a/w"]);
  assert.deepEqual(litAgents(live, "b", "a"), ["a", "a/w", "b"]);
  assert.deepEqual(litAgents(live, null, null), []);
});

test("the not placed line counts the agents no part holds, and says how many need you (#472)", () => {
  const agent = (key, needsYou = false) => ({ key, session: key, agent: null, label: key, needsYou, paths: [] });
  assert.deepEqual(notPlacedLine([]), { count: "0 not placed", needsYou: null });
  assert.deepEqual(notPlacedLine([agent("a")]), { count: "1 not placed", needsYou: null });
  assert.deepEqual(notPlacedLine([agent("a"), agent("b", true), agent("c", true)]), { count: "3 not placed", needsYou: "2 need you" });
  assert.deepEqual(notPlacedLine([agent("a", true)]), { count: "1 not placed", needsYou: "1 needs you" });
});
