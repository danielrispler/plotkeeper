// Home (spec #53): the sessions that need Daniel, then the unseen ones, then a row per project; and
// its ways in. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { homeProjects, homeSections } from "../../dist/home.js";
import { initial } from "../../dist/settings.js";
import { windowBinding } from "../../dist/keys.js";
import { listSpace } from "../../dist/home.js";

const session = (id, state, unseen = false, landed = false) => ({ id, state, unseen, landed });
const ids = (section) => section.sessions.map((s) => s.id);
const key = (code, k, mods = {}) => ({ key: k, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods });

test("Home lists the sessions that need Daniel first, then the unseen ones", () => {
  const sections = homeSections([
    session("a", "your_turn", true),
    session("b", "needs_you"),
    session("c", "running"),
    session("d", "idle", true),
    session("e", "needs_you", true),
  ]);
  assert.deepEqual(sections.map((s) => [s.title, ids(s)]), [
    ["Needs you", ["b", "e"]],
    ["Unseen", ["a", "d"]],
  ]);
});

test("a section with no session is absent", () => {
  assert.deepEqual(homeSections([session("c", "running")]), []);
  assert.deepEqual(homeSections([session("d", "idle", true)]).map((s) => s.title), ["Unseen"]);
});

test("one row per project, named by its folder, in projects.json order", () => {
  assert.deepEqual(homeProjects(["/Users/d/work/plotkeeper", "/Users/d/work/api"]), [
    { name: "plotkeeper", path: "/Users/d/work/plotkeeper" },
    { name: "api", path: "/Users/d/work/api" },
  ]);
  assert.deepEqual(homeProjects([]), []);
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
  assert.equal(listSpace(target("#workspaces")), false);
  assert.equal(listSpace(target("button")), false);
});

test("landed sessions come after needs-you and unseen, and a session that is not landed never appears there", () => {
  const sections = homeSections([
    session("a", "idle", false, true),
    session("b", "needs_you"),
    session("c", "your_turn", true, true),
    session("d", "ended", false, true),
    session("e", "idle"),
    session("f", "running", false, true),
  ]);
  assert.deepEqual(sections.map((s) => [s.title, ids(s)]), [
    ["Needs you", ["b"]],
    ["Landed", ["a", "c", "d"]],
  ]);
});
