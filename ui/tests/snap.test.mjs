// `npm run snap`: the page it renders is dist's index with a stub Tauri API fed by a fixture.
// These tests need no Chrome. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { snapPage, stubSource } from "../../scripts/snap/page.mjs";
import { fixtures } from "../../scripts/snap/fixtures.mjs";

const invokeIn = (source, command) => {
  const window = {};
  vm.runInNewContext(source, { window, Date });
  return window.__TAURI__.core.invoke(command);
};

test("the stub answers a command from the fixture, and an unknown one with null", async () => {
  const source = stubSource({ sessions: [{ id: "s1" }] });
  assert.equal(JSON.stringify(await invokeIn(source, "sessions")), JSON.stringify([{ id: "s1" }]));
  assert.equal(await invokeIn(source, "nope"), null);
});

test("the stub is injected before the app's module script, and the fixture is not written into dist's own file", () => {
  const index = '<head><script src="vendor/marked.umd.js"></script><script type="module" src="main.js"></script></head>';
  const page = snapPage(index, { sessions: [] });
  assert.ok(page.indexOf("__TAURI__") < page.indexOf('src="main.js"'));
  assert.ok(index.indexOf("__TAURI__") === -1);
});

test("the list fixture has a nested, a selected and a landed row", async () => {
  const f = fixtures.list();
  assert.ok(f.sessions.some((s) => s.agents.length > 0));
  assert.ok(f.sessions.some((s) => s.landed));
  assert.equal(f.selected.session.id, f.sessions[0].id);
});

test("the home fixture has every section: needs you, unseen, landed, and a project", () => {
  const f = fixtures.home();
  assert.equal(f.selected, null);
  assert.ok(f.sessions.some((s) => s.state === "needs_you"));
  assert.ok(f.sessions.some((s) => s.unseen && s.state !== "needs_you" && !s.landed));
  assert.ok(f.sessions.some((s) => s.landed && s.state !== "running"));
  assert.ok(f.terminal_connect.repo);
});
