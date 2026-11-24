// `npm run snap`: the page it renders is dist's index with a stub Tauri API fed by a fixture.
// These tests need no Chrome. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { chromeFailure, consoleLines, failed, headSource, snapPage, stubSource } from "../../scripts/snap/page.mjs";
import { fixtures } from "../../scripts/snap/fixtures.ts";
import { railStates } from "../../scripts/snap/rail-fixtures.ts";
import { shard } from "../../scripts/snap/shard.mjs";

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
  // The accessibility pass runs after it on the same page (a11y.mjs); only the hover rule's lines are this test's.
  assert.deepEqual(logged.filter((line) => line.startsWith("snap:fail hover")).map((line) => line.replace(/ on .*/, "")), ["snap:fail hover: no hover", "snap:fail hover: repeats its label", "snap:fail hover: out of the pointer's reach"]);
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

// snap run end to end against a stand-in Chrome (`CHROME`): one that never ends, one that ends without writing the screenshot, and
// one slow to tear down after writing it (#413).
const runSnap = async (chromeBody, env = {}, fixture = "mapBusy") => {
  const { mkdtempSync, writeFileSync, chmodSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const { spawnSync } = await import("node:child_process");
  const dir = mkdtempSync(join(tmpdir(), "snap-fake-chrome-"));
  const chrome = join(dir, "chrome");
  writeFileSync(chrome, `#!/bin/sh\n${chromeBody}\n`);
  chmodSync(chrome, 0o755);
  const out = join(dir, "out.png");
  const run = spawnSync(process.execPath, ["scripts/snap/snap.mjs", fixture, out], {
    env: { ...process.env, CHROME: chrome, ...env },
    encoding: "utf8",
    timeout: 30000,
  });
  return { status: run.status, signal: run.signal, output: run.stdout + run.stderr, out };
};

test("a Chrome that never ends is killed after snap's time limit, and the fixture fails", async () => {
  const run = await runSnap("exec sleep 600", { SNAP_CHROME_TIMEOUT_MS: "1000" });
  assert.equal(run.signal, null, "snap itself hung");
  assert.notEqual(run.status, 0);
  assert.match(run.output, /^snap:fail .*time/m);
});

test("a Chrome that ends without writing the screenshot fails the fixture", async () => {
  const run = await runSnap("exit 0");
  assert.notEqual(run.status, 0);
  assert.match(run.output, /^snap:fail .*screenshot/m);
  assert.doesNotMatch(run.output, /^wrote /m);
});

// A stand-in Chrome that writes the screenshot, then sits in its teardown as a CPU-starved Chrome does, until its own watchdog ends
// it with status 2 (#413). `before` is what it prints first, such as a page console line.
const slowTeardown = (before = "") => `for a in "$@"; do case "$a" in --screenshot=*) out="\${a#--screenshot=}";; esac; done
${before}
printf png > "$out"
echo "3 bytes written to file $out" >&2
sleep 20
echo "Teardown watchdog expired; recording dump and terminating." >&2
exit 2`;

test("a Chrome slow to tear down after writing the screenshot passes, without snap waiting for its teardown (#413)", async () => {
  const started = Date.now();
  const run = await runSnap(slowTeardown());
  assert.equal(run.status, 0, run.output);
  assert.match(run.output, /^wrote /m);
  assert.ok(Date.now() - started < 10000, `snap waited ${Date.now() - started} ms for Chrome's teardown`);
});

test("a page that logged snap:fail still fails when Chrome is ended at its screenshot (#413)", async () => {
  const line = `echo '[1124/103501.828596:INFO:CONSOLE:61] "snap:fail the card never drew", source: http://127.0.0.1:1/index.html (61)' >&2`;
  const started = Date.now();
  const run = await runSnap(slowTeardown(line));
  assert.notEqual(run.status, 0);
  assert.match(run.output, /^snap:fail the card never drew/m);
  assert.doesNotMatch(run.output, /^wrote /m);
  assert.ok(Date.now() - started < 10000, `snap waited ${Date.now() - started} ms for Chrome's teardown`);
});

// Chrome's DevTools port opens only for a fixture that asks for real input (#454): real pointer or real keys.
test("a fixture with real keys runs Chrome with its DevTools port, and one with neither real pointer nor real keys runs without it", async () => {
  const { readFileSync } = await import("node:fs");
  // A stand-in Chrome that writes its arguments as the screenshot, and logs the demo's ok line.
  const argsAsShot = `for a in "$@"; do case "$a" in --screenshot=*) out="\${a#--screenshot=}";; esac; done
echo '[1124/103501.828596:INFO:CONSOLE:61] "keys ok", source: http://127.0.0.1:1/index.html (61)' >&2
printf '%s\\n' "$@" > "$out"
echo "1 bytes written to file $out" >&2`;
  const port = (fixture) => readFileSync(runSnapOut[fixture], "utf8").split("\n").includes("--remote-debugging-port=0");
  const runSnapOut = {};
  for (const fixture of ["mapBusy", "snapRealKeys"]) {
    const run = await runSnap(argsAsShot, {}, fixture);
    assert.equal(run.status, 0, run.output);
    runSnapOut[fixture] = run.out;
  }
  assert.equal(port("mapBusy"), false, "a fixture with neither opened the DevTools port");
  assert.equal(port("snapRealKeys"), true, "a fixture with real keys has no DevTools port");
});

// CI renders the fixtures in two jobs (#432): `snap:all --shard=i/n` takes every n-th fixture from the i-th.
test("the shards of snap:all render every fixture exactly once, and no flag renders them all (#432)", () => {
  const names = Object.keys(fixtures);
  assert.deepEqual(shard(names, undefined), names);
  const one = shard(names, "1/2");
  const two = shard(names, "2/2");
  assert.deepEqual([...one, ...two].sort(), [...names].sort());
  assert.equal(new Set([...one, ...two]).size, names.length);
  assert.deepEqual(shard(["a", "b", "c", "d", "e"], "2/2"), ["b", "d"]);
  for (const bad of ["0/2", "3/2", "a/b", "1/0", "1", "1/2/3", ""]) assert.throws(() => shard(names, bad), /--shard=<i>\/<n>/, bad);
});
