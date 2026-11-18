// The UI's failures reach the Studio log (#190): a failure call at the agreed level, an uncaught page error at `error`, and nothing beyond the message. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";

const lines = [];
const listeners = {};
globalThis.window = {
  __TAURI__: { log: { warn: async (m) => void lines.push(["warn", m]), error: async (m) => void lines.push(["error", m]) } },
  addEventListener: (type, handler) => void (listeners[type] = handler),
};
const { failure, logUncaught } = await import("../../dist/dom.js");
const last = () => lines[lines.length - 1];

test("a failed call is logged at warn with the command's name and the error's message", () => {
  failure("rail_shown")(new Error("no window"));
  assert.deepEqual(last(), ["warn", "rail_shown: no window"]);
});

test("a rejection with a string, as the app's commands give, is logged as it is", () => {
  failure("open_link")("no such app");
  assert.deepEqual(last(), ["warn", "open_link: no such app"]);
});

test("a failed action Daniel asked for is logged at error", () => {
  failure("rename", "error")(new Error("disk full"));
  assert.deepEqual(last(), ["error", "rename: disk full"]);
});

test("only the message is kept: not the stack, the cause or other fields", () => {
  const e = Object.assign(new Error("boom"), { cause: "card text", content: "a prompt" });
  failure("answer", "error")(e);
  assert.deepEqual(last(), ["error", "answer: boom"]);
});

test("a log call that itself fails does not throw", async () => {
  window.__TAURI__.log.warn = async () => {
    throw new Error("log gone");
  };
  assert.doesNotThrow(() => failure("x")("y"));
  await new Promise((r) => setTimeout(r, 0));
});

test("an uncaught page error and an unhandled rejection are logged at error", () => {
  logUncaught(window);
  listeners.error({ message: "x is not a function", error: new Error("x is not a function") });
  assert.deepEqual(last(), ["error", "uncaught: x is not a function"]);
  listeners.unhandledrejection({ reason: new Error("lost") });
  assert.deepEqual(last(), ["error", "unhandled rejection: lost"]);
});
