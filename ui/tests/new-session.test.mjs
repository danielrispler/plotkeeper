// Where a new session starts when there are several projects: a "+" on each project's header, as in Zed, T3 Code,
// Superset and cmux (docs/studio/research/new-session-project.md). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El } from "./dom.mjs";

const { projectHead } = await import("../../dist/list.js");
// The document is set after the import: times.ts starts a timer when one exists at load.
globalThis.document = {
  createElement: (tag) => Object.assign(new El(tag), { dataset: {}, style: {}, classList: { add() {}, toggle() {} }, removeAttribute() {}, toggleAttribute() {} }),
  getElementById: () => new El("div"),
  visibilityState: "hidden",
};
// An outside folder's header asks for its repository; none is found.
globalThis.window = { __TAURI__: { core: { invoke: async () => null } } };

const group = (key, name, project) => ({ key, name, project, rollup: null, states: [], workspaces: null });
const plus = (head) => head.childNodes.find((node) => node.className === "project-new");

test("a project's header holds a '+' that starts a session there; a folder outside every project has none", () => {
  const button = plus(projectHead(group("/work/falinks", "falinks", true)));
  assert.equal(button.text, "+");
  assert.equal(button.title, "New session in falinks");
  assert.equal(button.getAttribute("aria-label"), "New session in falinks");
  assert.equal(plus(projectHead(group("/tmp/loose", "loose", false))), undefined);
});
