// The hints of the session list and the top bar (spec #582, ticket 1): each button and link of them has its own hint of the words its
// title had, the ARIA rule applied, and no `title`. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument } from "./dom.mjs";
import { row, strip, projectHead, deleting } from "../../dist/list.js";
import { hintOf } from "../../dist/dom.js";
import { studio } from "../../dist/state.js";

const page = new Map();
globalThis.document = fakeDocument({
  getElementById: (id) => page.get(id) ?? page.set(id, document.createElement("div")).get(id),
  body: { dataset: { layout: "focus" }, classList: { toggle() {} } },
});

const session = /** @type {import("../../dist/types.js").Session} */ ({
  id: "s", name: "Fix the build", prompt_name: null, folder_name: "f", identity: { issue: null, name: "PR #9 · issue-9" },
  capabilities: { agent: "claude" }, transcript: { context: null }, state: "needs_you", line: "Bash cargo test", questions: [], agents: [],
  unseen: false, landed: false, started_at: 0, last_event_at: 0, turn_span: null, status_only: false, shell: false, pid: 1,
});
const words = (button) => hintOf(button)?.textContent;
const described = (button) => button.getAttribute("aria-describedby") === button.getAttribute("interestfor");

test("a session row's hint pairs its name with its state, as its title did, and is its description", () => {
  const button = row(session, new Map());
  assert.equal(words(button), "Fix the build · Needs you");
  assert.equal(hintOf(button)?.getAttribute("role"), "tooltip");
  assert.ok(described(button));
  assert.equal(button.getAttribute("title"), null);
  assert.equal(button.title, undefined);
});

test("the strip's buttons have hints, and the open button's hint adds to its name", () => {
  studio.files = /** @type {any} */ ({ keymap: new Map() });
  studio.selection = null;
  const [open, first] = strip([session]);
  assert.equal(words(first), "Fix the build · Needs you");
  assert.ok(words(open)?.startsWith("Open the session list"));
  assert.ok(described(open));
  for (const button of [open, first]) assert.equal(button.getAttribute("title"), null);
});

test("a session's delete and a project's header have hints and no title", () => {
  const remove = deleting(session);
  assert.equal(words(remove), "Delete this session from the list…");
  assert.equal(remove.getAttribute("title"), null);
  const head = projectHead(/** @type {any} */ ({ key: "/a/repo", name: "repo", rollup: null, states: [], live: false }));
  const [toggle, more] = /** @type {HTMLElement[]} */ (/** @type {unknown} */ (head.childNodes));
  assert.match(words(toggle), /^\/a\/repo\n/);
  assert.equal(words(more), "Project");
  for (const button of [toggle, more]) assert.equal(button.getAttribute("title"), null);
});

test("the top bar's Needs you chip has a hint of its own and no title", async () => {
  const { drawChip } = await import("../../dist/list.js");
  drawChip([session]);
  const chip = document.getElementById("needs-you");
  assert.equal(words(chip), "Go to the first session that needs you");
  assert.equal(chip.getAttribute("title"), null);
  assert.equal(chip.textContent, "1 needs you");
});
