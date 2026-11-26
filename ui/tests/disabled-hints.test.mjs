// The controls that explain why they are off (spec #582, ticket 2): `aria-disabled="true"`, not `disabled`, so they stay focusable
// and show their reason as a hint; their activation does nothing. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { El, fakeDocument } from "./dom.mjs";

globalThis.marked = createRequire(import.meta.url)("marked");
const page = new Map();
globalThis.document = fakeDocument({
  body: new El("body"),
  getElementById: (id) => page.get(id) ?? page.set(id, document.createElement("div")).get(id),
  visibilityState: "visible",
  addEventListener() {},
});
const invoked = [];
let release = () => {};
const asked = { nonce: "n1", hook_event_name: "PreToolUse", tool_name: "AskUserQuestion", tool_use_id: "t1", tool_input: {}, agent_id: null };
globalThis.window = /** @type {any} */ ({
  __TAURI__: {
    core: {
      invoke: (command, args) => {
        invoked.push([command, args]);
        if (command === "watch") return Promise.resolve(asked);
        if (command === "ask_anchor") return new Promise((done) => (release = () => done("An answer.")));
        return Promise.resolve(null);
      },
    },
    event: { listen: () => Promise.resolve(() => {}) },
  },
});
globalThis.setInterval = /** @type {any} */ (() => 0);

const { studio } = await import("../../dist/state.js");
const { startWatching } = await import("../../dist/answer.js");
const { questionElement, focusSend } = await import("../../dist/questions.js");
const { askAnchor } = await import("../../dist/ask.js");
const { exploreNode } = await import("../../dist/micro-world.js");
const { hintOf } = await import("../../dist/dom.js");

const disabled = (node) => node.getAttribute("aria-disabled") === "true";

test("the Send of a question card is aria-disabled with its reason as a hint until every question has a choice", async () => {
  studio.selection = /** @type {any} */ ({ session: { id: "s", resume_id: "s" }, agent: null });
  startWatching(() => {});
  await new Promise((done) => setTimeout(done, 5));
  const question = {
    tool_use_id: "t1",
    answer: "open",
    questions: [
      { question: "A?", header: "a", multiSelect: false, options: [{ label: "x", description: null }] },
      { question: "B?", header: "b", multiSelect: false, options: [{ label: "y", description: null }] },
    ],
  };
  const card = /** @type {any} */ (questionElement(/** @type {any} */ (question)));
  const [send] = card.find("button").filter((button) => button.className.includes("send"));
  assert.ok(send, "the card has a Send");
  assert.ok(disabled(send));
  assert.equal(send.getAttribute("disabled"), null);
  assert.equal(hintOf(send)?.textContent, "Choose an answer to every question first");
  assert.equal(send.getAttribute("title"), null);
});

test("a card's Send takes the answer keys' focus only once every question has a choice", () => {
  const question = {
    tool_use_id: "t1",
    answer: "open",
    questions: [
      { question: "C?", header: "c", multiSelect: false, options: [{ label: "x", description: null }] },
      { question: "D?", header: "d", multiSelect: false, options: [{ label: "y", description: null }] },
    ],
  };
  const card = /** @type {any} */ (questionElement(/** @type {any} */ (question)));
  const [send] = card.find("button").filter((button) => button.className.includes("send"));
  let focused = 0;
  send.focus = () => focused++;
  const [first, second] = card.find("button").filter((button) => !button.className.includes("send") && button.text !== "Answer in the terminal");
  first.listeners.click();
  focusSend(card);
  assert.ok(disabled(send), "one question still waits for a choice");
  assert.equal(focused, 0, "an unavailable Send does not take focus");
  second.listeners.click();
  focusSend(card);
  assert.ok(!disabled(send));
  assert.equal(focused, 1);
});

test("Ask on an anchor is aria-disabled while it waits, ignores a second ask, and its hint says so", async () => {
  invoked.length = 0;
  const box = /** @type {any} */ (askAnchor("s", { kind: "headline", turn: 0 }, "this turn"));
  const [field] = box.find("textarea");
  const send = box.find("button").find((button) => button.text === "Ask");
  assert.ok(!disabled(send));
  assert.match(hintOf(send).textContent, /^Ask about this/);
  field.value = "Why?";
  send.listeners.click();
  assert.ok(disabled(send));
  assert.equal(send.getAttribute("disabled"), null);
  assert.equal(hintOf(send).textContent, "Waiting for the answer to your question");
  send.listeners.click();
  field.listeners.keydown({ metaKey: true, key: "Enter" });
  assert.equal(invoked.filter(([command]) => command === "ask_anchor").length, 1);
  release();
  await new Promise((done) => setTimeout(done, 5));
  assert.ok(!disabled(send));
  assert.match(hintOf(send).textContent, /^Ask about this/);
});

test("the waiting 'Building…' offer is aria-disabled with its reason as a hint, and a click changes nothing", () => {
  const calls = [];
  const building = { on: true, worlds: { k: { phase: "building", page: false, stale: false, line: null, prompt: null } } };
  const node = /** @type {any} */ (exploreNode(/** @type {any} */ (building), "k", { explore: () => calls.push("explore"), open: () => calls.push("open"), copy() {} }));
  const [wait] = node.find("button");
  assert.ok(disabled(wait));
  assert.equal(wait.getAttribute("disabled"), null);
  assert.equal(hintOf(wait)?.textContent, "A background run is building it");
  wait.listeners.click?.();
  assert.deepEqual(calls, []);
});
