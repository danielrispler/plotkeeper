// The page's transport (danielrispler/plotkeeper#325): commands over `fetch`, events over one `EventSource`, log lines posted.
import { test } from "node:test";
import assert from "node:assert/strict";

const requests = [];
let reply = { ok: true, body: null };
// biome-ignore lint/suspicious/useAwait: a stand-in for an async API, so a throw in it rejects as the real one's would
globalThis.fetch = async (url, init) => {
  requests.push([url, init.method, JSON.parse(init.body)]);
  return { ok: reply.ok, status: reply.ok ? 200 : 500, json: async () => reply.body };
};
const sources = [];
globalThis.EventSource = class {
  handlers = {};
  constructor(url) {
    this.url = url;
    sources.push(this);
  }
  addEventListener(name, handler) {
    this.handlers[name] = [...(this.handlers[name] ?? []), handler];
  }
  removeEventListener(name, handler) {
    this.handlers[name] = (this.handlers[name] ?? []).filter((h) => h !== handler);
  }
};
globalThis.document = { visibilityState: "visible", hasFocus: () => true, addEventListener() {} };
globalThis.addEventListener = () => {};
globalThis.window = {};
await import("../../dist/transport.js");
const seam = () => window.__TAURI__;

test("a command is a POST to /api/<command> with its arguments as JSON, and its reply is the JSON answer", async () => {
  reply = { ok: true, body: [{ id: "s1" }] };
  assert.deepEqual(await seam().core.invoke("select", { id: "s1" }), [{ id: "s1" }]);
  assert.deepEqual(requests.at(-1), ["/api/select", "POST", { id: "s1" }]);
  await seam().core.invoke("sessions");
  assert.deepEqual(requests.at(-1), ["/api/sessions", "POST", {}]);
});

// A refusal is an `Error` (#554): its message is the server's reason, and it names the command and the HTTP status.
test("a refused command rejects with a CommandError carrying the server's reason", async () => {
  const { CommandError } = await import("../../dist/transport.js");
  reply = { ok: false, body: "not a session id" };
  await assert.rejects(seam().core.invoke("rename", { id: "x", name: "y" }), (e) =>
    e instanceof CommandError && e instanceof Error && e.message === "not a session id" && e.command === "rename" && e.status === 500);
  reply = { ok: false, body: null };
  await assert.rejects(seam().core.invoke("rename", { id: "x", name: "y" }), { message: "rename: 500" });
});

test("one event stream carries every event to the listeners of its name, until one is removed", async () => {
  const seen = [];
  const off = await seam().event.listen("cards", (event) => seen.push(["a", event.payload]));
  await seam().event.listen("cards", (event) => seen.push(["b", event.payload]));
  await seam().event.listen("notice", (event) => seen.push(["notice", event.payload]));
  assert.equal(sources.length, 1);
  assert.equal(sources[0].url, "/events");
  for (const h of sources[0].handlers.cards) h({ data: '{"n":1}' });
  for (const h of sources[0].handlers.notice) h({ data: '"hi"' });
  assert.deepEqual(seen, [["a", { n: 1 }], ["b", { n: 1 }], ["notice", "hi"]]);
  off();
  for (const h of sources[0].handlers.cards) h({ data: "2" });
  assert.deepEqual(seen.at(-1), ["b", 2]);
});

test("a log line is posted at its level", async () => {
  reply = { ok: true, body: null };
  await seam().log.warn("select: boom");
  assert.deepEqual(requests.at(-1), ["/api/log", "POST", { level: "warn", line: "select: boom" }]);
  await seam().log.error("x");
  assert.deepEqual(requests.at(-1)[2], { level: "error", line: "x" });
});

test("the page tells the server whether it is visible and focused when the stream opens", async () => {
  for (const h of sources[0].handlers.open) h();
  await new Promise((done) => setTimeout(done));
  assert.deepEqual(requests.at(-1), ["/api/page_focus", "POST", { focused: true }]);
});
