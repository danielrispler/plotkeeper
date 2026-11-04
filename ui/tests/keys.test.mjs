// The terminal pane's key mapping (#466), run on the `tsc` output. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { keyAction } from "../../dist/keys.js";

const key = (code, mods = {}, key = "") => ({
  key, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods,
});

test("Cmd+V with an image on the clipboard sends Ctrl+V, so Claude Code reads the clipboard itself", () => {
  assert.deepEqual(keyAction(key("KeyV", { metaKey: true }), true), { send: "\x16" });
});

test("Cmd+V with text leaves the paste to the webview, which xterm turns into a bracketed paste", () => {
  assert.equal(keyAction(key("KeyV", { metaKey: true }), false), "native");
});

test("Cmd+C copies through the webview and Ctrl+C stays the terminal's interrupt", () => {
  assert.equal(keyAction(key("KeyC", { metaKey: true }), false), "native");
  assert.equal(keyAction(key("KeyC", { ctrlKey: true }, "c"), false), "terminal");
});

test("Ctrl+V reaches the terminal as is", () => {
  assert.equal(keyAction(key("KeyV", { ctrlKey: true }, "v"), true), "terminal");
});

test("Shift+Enter sends ESC CR, a newline Claude Code accepts", () => {
  assert.deepEqual(keyAction(key("Enter", { shiftKey: true }, "Enter"), false), { send: "\x1b\r" });
  assert.equal(keyAction(key("Enter", {}, "Enter"), false), "terminal");
});

test("Option acts as Meta, by the physical key, whatever the layout types", () => {
  assert.deepEqual(keyAction(key("KeyB", { altKey: true }, "∫"), false), { send: "\x1bb" });
  assert.deepEqual(keyAction(key("KeyB", { altKey: true, shiftKey: true }, "ı"), false), { send: "\x1bB" });
  assert.deepEqual(keyAction(key("Digit3", { altKey: true }, "£"), false), { send: "\x1b3" });
  assert.deepEqual(keyAction(key("ArrowLeft", { altKey: true }), false), { send: "\x1bb" });
  assert.deepEqual(keyAction(key("ArrowRight", { altKey: true }), false), { send: "\x1bf" });
  assert.deepEqual(keyAction(key("Backspace", { altKey: true }), false), { send: "\x1b\x7f" });
});

test("Esc and Shift+Tab are the terminal's own", () => {
  assert.equal(keyAction(key("Escape", {}, "Escape"), false), "terminal");
  assert.equal(keyAction(key("Tab", { shiftKey: true }, "Tab"), false), "terminal");
});

test("the terminal's Cmd shortcuts", () => {
  const cmd = (code) => keyAction(key(code, { metaKey: true }), false);
  assert.deepEqual(cmd("KeyA"), { command: "selectAll" });
  assert.deepEqual(cmd("KeyF"), { command: "find" });
  assert.deepEqual(cmd("KeyK"), { command: "clear" });
  assert.deepEqual(cmd("ArrowLeft"), { send: "\x01" });
  assert.deepEqual(cmd("ArrowRight"), { send: "\x05" });
  assert.deepEqual(cmd("Backspace"), { send: "\x15" });
});

test("every other Cmd shortcut goes to the app", () => {
  for (const code of ["Digit1", "Digit2", "KeyQ", "KeyW", "KeyH", "KeyM", "Comma", "KeyT"]) {
    assert.equal(keyAction(key(code, { metaKey: true }), false), "app", code);
  }
  assert.equal(keyAction(key("KeyC", { metaKey: true, shiftKey: true }), false), "app");
});
