// `npm run snap`: the page it renders is dist's index with a stub Tauri API fed by a fixture.
// These tests need no Chrome. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { chromeFailure, consoleLines, failed, headSource, snapPage, stubSource } from "../../scripts/snap/page.mjs";
import { fixtures } from "../../scripts/snap/fixtures.ts";
import { railStates } from "../../scripts/snap/rail-fixtures.ts";

// The stub answers what the page's transport asks: `fetch` of `/api/<command>`, read as the transport reads it.
const stubbed = (source, extra = {}) => {
  const window = {};
  vm.runInNewContext(source, { window, Date, ...extra });
  const invoke = async (command, args) => {
    const reply = await window.fetch(`/api/${command}`, { method: "POST", body: JSON.stringify(args ?? {}) });
    const body = await reply.json();
    if (!reply.ok) throw body;
    return body;
  };
  return { window, invoke };
};
const invokeIn = (source, command) => stubbed(source).invoke(command);

test("the stub answers a command from the fixture, and an unknown one with null", async () => {
  const source = stubSource({ sessions: [{ id: "s1" }] });
  assert.equal(JSON.stringify(await invokeIn(source, "sessions")), JSON.stringify([{ id: "s1" }]));
  assert.equal(await invokeIn(source, "nope"), null);
});

test("the stub is injected before the app's module script, and the fixture is not written into dist's own file", () => {
  const index = '<head><script src="vendor/marked.umd.js"></script><script type="module" src="main.js"></script></head>';
  const page = snapPage(index, { sessions: [] });
  assert.ok(page.indexOf("__snapEmit") < page.indexOf('src="main.js"'));
  assert.ok(index.indexOf("__snapEmit") === -1);
});

test("the list fixture has a nested, a selected and a landed row", async () => {
  const f = fixtures.list();
  assert.ok(f.sessions.some((s) => s.agents.length > 0));
  assert.ok(f.sessions.some((s) => s.landed));
  assert.equal(f.selected.session.id, f.sessions[0].id);
});

// The rail's fixtures (#109): each holds what its state needs.
const selected = (name) => fixtures[name]().selected.session;

test("a rail fixture exists for each state, and all are rendered by `snap:rail`", () => {
  assert.deepEqual(Object.keys(railStates), ["railRunning", "railYourTurn", "railNeeds", "railIdle"]);
  for (const name of Object.keys(railStates)) assert.equal(fixtures[name]().selected.session.id, "s1");
});

test("railRunning: a running turn with an intention, a step and two subagents", () => {
  const s = selected("railRunning");
  assert.equal(s.state, "running");
  assert.ok(s.turn_span.end === null && s.transcript.intention);
  assert.ok(s.step_span.end === null && s.last_step.tool);
  assert.equal(s.agents.length, 2);
  assert.ok(s.agents.some((a) => a.status === "running"));
});

test("railYourTurn: a summary and a failed check, and no card", () => {
  const f = fixtures.railYourTurn();
  assert.equal(f.selected.session.state, "your_turn");
  assert.ok(f.turn_summary.values.t4.summary);
  assert.equal("recent" in f.cards || "older" in f.cards || "workers" in f.cards, false);
  const listed = f.selected.session.turns.flatMap((t) => t.listed);
  assert.ok(listed.length && listed.every((c) => c.status === "failed"));
});

test("railNeeds: the session needs you, with a question docked", () => {
  const f = fixtures.railNeeds();
  const s = f.selected.session;
  assert.equal(s.state, "needs_you");
  assert.ok(s.asker);
  assert.equal(s.questions[0].answer, "open");
  assert.equal(f.watch.tool_name, "AskUserQuestion");
});

test("railIdle: an idle session with five turns and its brief", () => {
  const f = fixtures.railIdle();
  assert.equal(f.selected.session.state, "idle");
  assert.equal(f.selected.session.turns.length, 5);
  assert.ok(f.cards.goal);
});

test("an argument-keyed reply answers by that argument", async () => {
  const { invoke } = stubbed(stubSource({ turn_summary: { __by: "id", values: { t1: { summary: "x" } } } }));
  assert.equal(JSON.stringify(await invoke("turn_summary", { id: "t1" })), '{"summary":"x"}');
  assert.equal(await invoke("turn_summary", { id: "zz" }), null);
});

// Runs a page's head script in a stand-in page: `load` fires, then the timers run.
// `nodes`: what the page's hover check finds, by selector (none by default).
const runHead = (options, nodes = { controls: [], titled: [] }) => {
  const logged = [];
  const added = [];
  const handlers = {};
  const timers = [];
  const querySelectorAll = (selector) => (selector === "[title]" ? nodes.titled : nodes.controls);
  const document = { body: { classList: { add: (...names) => added.push(...names) } }, marker: "drawn", querySelectorAll };
  vm.runInNewContext(headSource(options), {
    document, console: { log: (line) => logged.push(line) }, getComputedStyle: (node) => ({ pointerEvents: node.pointer ?? "auto" }),
    addEventListener: (name, run) => (handlers[name] = run),
    setTimeout: (run) => timers.push(run),
  });
  handlers.DOMContentLoaded?.();
  handlers.load?.();
  return Promise.all(timers.map((run) => run())).then(() => ({ logged, added }));
};

test("a fixture's script runs after load and gets log; what it logs is what snap prints", async () => {
  const { logged } = await runHead({ script: 'log("rail scrollTop", 120 + 0, document.marker);' });
  assert.deepEqual(logged, ["rail scrollTop 120 drawn"]);
});

test("the script does not run at load itself, only after the UI has had time to draw", async () => {
  const handlers = {};
  const timers = [];
  vm.runInNewContext(headSource({ script: 'log("ran")' }), {
    document: { body: { classList: { add() {} } } }, console: { log() { throw new Error("ran too early"); } },
    addEventListener: (name, run) => (handlers[name] = run), setTimeout: (run) => timers.push(run),
  });
  handlers.load();
  assert.equal(timers.length, 1);
});

test("a script that throws logs a snap:fail line, and the body's classes are set from the run", async () => {
  const { logged, added } = await runHead({ script: 'throw new Error("no rail");', bodyClasses: ["rail-open"] });
  assert.ok(failed(logged));
  assert.match(logged[0], /^snap:fail the script threw: Error: no rail/);
  assert.deepEqual(added, ["rail-open"]);
});

test("a page without a script logs nothing while the hover rule holds, and has no classes by default", async () => {
  const { logged, added } = await runHead({});
  assert.deepEqual([logged, added], [[], []]);
});

// A stand-in element for the hover check (#220): its tag, label, own title and the nearest title up from it.
const control = (label, { title = null, hover = title, pointer = "auto" } = {}) => ({
  tagName: "BUTTON", id: "", className: "btn", textContent: label, title: title ?? "", pointer,
  hasAttribute: (name) => name === "title" && title !== null,
  closest: (selector) => (selector === ".xterm" ? null : hover === null ? null : { getAttribute: () => hover }),
});

test("the hover check fails a control with no hover, one whose hover repeats its label, and a titled node out of reach", async () => {
  const silent = control("×");
  const repeats = control("Dismiss", { title: "Dismiss" });
  const unreachable = control("Pull ↓2", { title: "Uncommitted changes would be overwritten", pointer: "none" });
  const fine = control("+", { title: "Start a new session (⌘N)" });
  const inherited = control("Open", { hover: "Resume this session" });
  const { logged } = await runHead({}, { controls: [silent, repeats, fine, inherited, unreachable], titled: [repeats, fine, unreachable] });
  assert.deepEqual(logged.map((line) => line.replace(/ on .*/, "")), ["snap:fail hover: no hover", "snap:fail hover: repeats its label", "snap:fail hover: out of the pointer's reach"]);
  assert.ok(failed(logged));
});

test("the hover check runs after a script that returns early or throws", async () => {
  const nodes = { controls: [control("×")], titled: [] };
  for (const script of ["return;", 'throw new Error("no rail");']) {
    const { logged } = await runHead({ script }, nodes);
    assert.ok(logged.some((line) => line.startsWith("snap:fail hover: no hover")), script);
  }
});

test("snapPage puts a fixture's script in the page, after the stub and before the app", () => {
  const index = '<head><script type="module" src="main.js"></script></head>';
  const page = snapPage(index, { snapScript: "log(1)" });
  // The fixture, with its script, is also data inside the stub, so the script that runs is the last one.
  assert.ok(page.indexOf("window.__snapEmit =") < page.lastIndexOf("log(1)") && page.lastIndexOf("log(1)") < page.indexOf('src="main.js"'));
});

test("a log line starting snap:fail fails the run; other lines do not", () => {
  assert.equal(failed(["rail scrollTop 120", "snap:fail moved"]), true);
  assert.equal(failed(["rail scrollTop 120", "a snap:fail inside"]), false);
});

// #144: an uncaught exception or an unhandled rejection in the page fails the run, as Chrome logs either: a stub reply the UI
// cannot read, such as `null` for a list, throws in a promise that nothing awaits.
test("an uncaught error or unhandled rejection in the page fails the run", () => {
  assert.equal(failed(["Uncaught TypeError: Cannot read properties of null (reading 'filter')"]), true);
  assert.equal(failed(["Uncaught (in promise) TypeError: Cannot read properties of null (reading 'filter')"]), true);
  assert.equal(failed(["the rail logged Uncaught once"]), false);
});

test("consoleLines reads the page's console lines out of Chrome's stderr", () => {
  const stderr = [
    '[1116/101010.000:INFO:CONSOLE:5] "rail scrollTop 120", source: http://127.0.0.1:1/index.html (5)',
    "[1116/101010.000:ERROR:something_else.cc:1] not console",
    '[1116/101010.000:INFO:CONSOLE:6] "snap:fail it said \\"no\\"", source: http://127.0.0.1:1/index.html (6)',
  ].join("\n");
  assert.deepEqual(consoleLines(stderr), ["rail scrollTop 120", 'snap:fail it said "no"']);
});

test("a non-zero Chrome status reports the status and the last lines of its stderr; a zero status reports nothing", () => {
  const stderr = Array.from({ length: 40 }, (_, i) => `line ${i + 1}`).join("\n") + "\n";
  const report = chromeFailure(2, stderr);
  assert.match(report, /exit status 2/);
  assert.ok(report.includes("line 40") && report.includes("line 31"));
  assert.ok(!report.includes("line 1\n") && !report.includes("line 20"));
  assert.match(chromeFailure(null, ""), /exit status null/);
  assert.equal(chromeFailure(0, stderr), "");
});

test("a map brief fixture that never printed its ok line fails: Chrome's time budget cut the script off before its assertions ran", () => {
  assert.equal(fixtures.mapBriefStart().snapDone, "brief ok");
  assert.equal(failed(["something else"], "brief ok"), true);
  assert.equal(failed(["brief ok"], "brief ok"), false);
  assert.equal(failed(["snap:fail x", "brief ok"], "brief ok"), true);
  assert.equal(failed([]), false, "a fixture with no ok line is as before");
});
