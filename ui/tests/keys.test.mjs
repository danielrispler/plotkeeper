// The terminal pane's key mapping (#466), run on the `tsc` output. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { bound, keyAction, keystroke, windowBinding } from "../../dist/keys.js";
import { initial, reload } from "../../dist/settings.js";

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

test("keystrokes read as Zed writes them, by the physical key", () => {
  assert.equal(keystroke(key("Digit1", { metaKey: true }, "1")), "cmd-1");
  assert.equal(keystroke(key("Escape", {}, "Escape")), "escape");
  assert.equal(keystroke(key("Equal", { metaKey: true }, "=")), "cmd-=");
  assert.equal(keystroke(key("Minus", { metaKey: true }, "-")), "cmd--");
  assert.equal(keystroke(key("KeyK", { metaKey: true, shiftKey: true, ctrlKey: true, altKey: true }, "˚")), "ctrl-alt-shift-cmd-k");
  assert.equal(keystroke(key("ArrowLeft", { metaKey: true })), "cmd-left");
});

test("today's window shortcuts are bound by default", () => {
  const { keymap } = initial;
  assert.equal(bound(keymap, "Workspace", key("Digit1", { metaKey: true }, "1")), "studio::ToggleList");
  assert.equal(bound(keymap, "Workspace", key("Digit2", { metaKey: true }, "2")), "studio::ToggleRail");
  assert.equal(bound(keymap, "Workspace", key("Escape", {}, "Escape")), "studio::Dismiss");
  assert.equal(bound(keymap, "Workspace", key("Equal", { metaKey: true }, "=")), "studio::ZoomIn");
});

test("a rebound shortcut triggers its action, and the old key no longer does", () => {
  const { keymap } = reload(initial, {
    settings: null,
    keymap: `[
      { "context": "Workspace", "bindings": { "cmd-l": "studio::ToggleList" } },
      { "context": "Terminal", "bindings": { "cmd-shift-k": "terminal::Clear", "cmd-left": null } },
    ]`,
  });
  assert.equal(bound(keymap, "Workspace", key("KeyL", { metaKey: true }, "l")), "studio::ToggleList");
  assert.equal(bound(keymap, "Workspace", key("Digit1", { metaKey: true }, "1")), undefined);
  assert.equal(bound(keymap, "Workspace", key("Digit2", { metaKey: true }, "2")), "studio::ToggleRail");
  assert.deepEqual(keyAction(key("KeyK", { metaKey: true, shiftKey: true }), false, keymap), { command: "clear" });
  assert.equal(keyAction(key("KeyK", { metaKey: true }), false, keymap), "app");
  assert.equal(keyAction(key("ArrowLeft", { metaKey: true }), false, keymap), "app");
  assert.deepEqual(keyAction(key("ArrowRight", { metaKey: true }), false, keymap), { send: "\x05" });
});

test("while typing, a window shortcut without Cmd leaves the key to the field, except Esc's dismiss", () => {
  const { keymap } = reload(initial, {
    settings: null,
    keymap: `[{ "bindings": { "ctrl-l": "studio::ToggleList", "alt-1": "studio::ToggleRail" } }]`,
  });
  assert.equal(windowBinding(keymap, key("KeyL", { ctrlKey: true }, "l"), true), undefined);
  assert.equal(windowBinding(keymap, key("Digit1", { altKey: true }, "1"), true), undefined);
  assert.equal(windowBinding(keymap, key("KeyL", { ctrlKey: true }, "l"), false), "studio::ToggleList");
  assert.equal(windowBinding(keymap, key("Escape", {}, "Escape"), true), "studio::Dismiss");
  assert.equal(windowBinding(keymap, key("Equal", { metaKey: true }, "="), true), "studio::ZoomIn");
});
