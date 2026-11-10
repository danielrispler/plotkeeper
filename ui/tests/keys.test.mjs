// The terminal pane's key mapping (#466), run on the `tsc` output. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { bound, doctorEntries, keyAction, keyLabel, keystroke, nextNeedsYou, paletteEntries, run, stepSession, windowBinding } from "../../dist/keys.js";
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

// The keyboard spine and the palette (#575).
const press = (code, mods = {}) => windowBinding(initial.keymap, key(code, mods), false, false, true);

test("each default key of the spine runs its action", () => {
  const ran = [];
  const actions = Object.fromEntries(
    ["studio::NextSession", "studio::PreviousSession", "studio::NextNeedsYou", "studio::SelectNext", "studio::SelectPrevious",
      "studio::Answer", "studio::NewSession", "studio::OpenSettings", "studio::Palette"].map((name) => [name, (arg) => ran.push(arg ? `${name} ${arg}` : name)]),
  );
  const cmd = { metaKey: true };
  run(actions, press("BracketRight", cmd));
  run(actions, press("BracketLeft", cmd));
  run(actions, press("KeyJ", cmd));
  run(actions, windowBinding(initial.keymap, key("ArrowDown"), false, true));
  run(actions, windowBinding(initial.keymap, key("ArrowUp"), false, true));
  for (const n of ["1", "2", "3", "4"]) run(actions, press(`Digit${n}`));
  run(actions, press("KeyN", cmd));
  run(actions, press("Comma", cmd));
  run(actions, press("KeyP", { metaKey: true, shiftKey: true }));
  assert.deepEqual(ran, [
    "studio::NextSession", "studio::PreviousSession", "studio::NextNeedsYou", "studio::SelectNext", "studio::SelectPrevious",
    "studio::Answer 1", "studio::Answer 2", "studio::Answer 3", "studio::Answer 4",
    "studio::NewSession", "studio::OpenSettings", "studio::Palette",
  ]);
});

test("↑ and ↓ move in the list only while it has focus", () => {
  assert.equal(windowBinding(initial.keymap, key("ArrowDown"), false), undefined);
  assert.equal(windowBinding(initial.keymap, key("ArrowDown"), false, true), "studio::SelectNext");
  // The list's context falls back to the window's.
  assert.equal(windowBinding(initial.keymap, key("BracketRight", { metaKey: true }), false, true), "studio::NextSession");
});

test("the digits that answer, and plain control-letters, reach the terminal while it has focus", () => {
  assert.equal(windowBinding(initial.keymap, key("Digit1", {}, "1"), true), undefined);
  assert.equal(windowBinding(initial.keymap, key("KeyJ", { ctrlKey: true }, "j"), true), undefined);
  assert.equal(keyAction(key("KeyJ", { ctrlKey: true }, "j"), false), "terminal");
});

test("the digits never answer from the terminal, and answer from the list only while the card is on screen", () => {
  // From the terminal (typing): the digit is the terminal's, even with a card on screen.
  assert.equal(windowBinding(initial.keymap, key("Digit2", {}, "2"), true, false, true), undefined);
  assert.deepEqual(windowBinding(initial.keymap, key("Digit2", {}, "2"), false, true, true), ["studio::Answer", "2"]);
  // Not while focus mode or a closed overlay hides the card.
  assert.equal(windowBinding(initial.keymap, key("Digit2", {}, "2"), false, true, false), undefined);
  assert.equal(windowBinding(initial.keymap, key("Digit5", {}, "5"), false, true, true), undefined);
});

test("a rebound spine key works and its old key does not", () => {
  const { keymap } = reload(initial, {
    settings: null,
    keymap: `[{ "bindings": { "cmd-shift-j": "studio::NextNeedsYou", "ctrl-1": ["studio::Answer", "1"] } }]`,
  });
  assert.equal(windowBinding(keymap, key("KeyJ", { metaKey: true, shiftKey: true }), false), "studio::NextNeedsYou");
  assert.equal(windowBinding(keymap, key("KeyJ", { metaKey: true }), false), undefined);
  assert.deepEqual(windowBinding(keymap, key("Digit1", { ctrlKey: true }), false, false, true), ["studio::Answer", "1"]);
  assert.equal(windowBinding(keymap, key("Digit1"), false, false, true), undefined);
  assert.deepEqual(windowBinding(keymap, key("Digit2"), false, false, true), ["studio::Answer", "2"]);
});

const sessions = [
  { id: "a", state: "needs_you" },
  { id: "b", state: "running" },
  { id: "c", state: "needs_you" },
  { id: "d", state: "idle" },
];

test("⌘] and ⌘[ step through the sessions in the list's order, wrapping", () => {
  assert.equal(stepSession(sessions, "b", 1), "c");
  assert.equal(stepSession(sessions, "d", 1), "a");
  assert.equal(stepSession(sessions, "a", -1), "d");
  assert.equal(stepSession(sessions, null, 1), "a");
  assert.equal(stepSession(sessions, null, -1), "d");
  assert.equal(stepSession([], null, 1), null);
});

test("the next needs-you comes after the selected session, wrapping", () => {
  assert.equal(nextNeedsYou(sessions, "a"), "c");
  assert.equal(nextNeedsYou(sessions, "c"), "a");
  assert.equal(nextNeedsYou(sessions, "b"), "c");
  assert.equal(nextNeedsYou(sessions, null), "a");
  assert.equal(nextNeedsYou([{ id: "b", state: "running" }], "b"), null);
});

test("with no needs-you, the next unseen comes after the selected session", () => {
  const sessions = [
    { id: "a", state: "your_turn", unseen: true },
    { id: "b", state: "your_turn", unseen: false },
    { id: "c", state: "your_turn", unseen: true },
  ];
  assert.equal(nextNeedsYou(sessions, "a"), "c");
  assert.equal(nextNeedsYou(sessions, "c"), "a");
  assert.equal(nextNeedsYou([...sessions, { id: "d", state: "needs_you" }], "a"), "d");
  assert.equal(nextNeedsYou([{ id: "a", state: "your_turn", unseen: false }], null), null);
});

test("keys read as macOS menus show them", () => {
  assert.equal(keyLabel("cmd-shift-p"), "⇧⌘P");
  assert.equal(keyLabel("cmd-]"), "⌘]");
  assert.equal(keyLabel("cmd--"), "⌘-");
  assert.equal(keyLabel("down"), "↓");
  assert.equal(keyLabel("escape"), "Esc");
  assert.equal(keyLabel("ctrl-alt-1"), "⌃⌥1");
});

test("the palette lists every action the window runs, each with its current key, and runs one", () => {
  const { keymap } = reload(initial, {
    settings: null,
    keymap: `[{ "bindings": { "cmd-l": "studio::ToggleList", "cmd-2": null } }]`,
  });
  const ran = [];
  const actions = {
    "studio::ToggleList": () => ran.push("list"),
    "studio::ToggleRail": () => {},
    "studio::Answer": (n) => ran.push(`answer ${n}`),
    "studio::Doctor": () => {},
  };
  const entries = paletteEntries(keymap, Object.keys(actions));
  const find = (title) => entries.find((entry) => entry.title === title);
  assert.deepEqual(find("Toggle list").keys, ["⌘L"]);
  // Unbound, it is still listed, with no key.
  assert.deepEqual(find("Toggle rail").keys, []);
  assert.deepEqual(find("Answer 3").keys, ["3"]);
  // An action with no default key, such as a later menu, is listed too.
  assert.deepEqual(find("Doctor").keys, []);
  // Only the actions the window runs: not the terminal's, nor one the window does not have.
  assert.equal(entries.some((entry) => entry.title === "Clear" || entry.title === "Zoom in"), false);
  assert.equal(new Set(entries.map((entry) => entry.title)).size, entries.length);
  run(actions, find("Toggle list").binding);
  run(actions, find("Answer 3").binding);
  assert.deepEqual(ran, ["list", "answer 3"]);
});

test("every default window action has a title and a key in the palette", () => {
  const names = [...Object.values(initial.keymap.Workspace), ...Object.values(initial.keymap.List)].map((b) => (typeof b === "string" ? b : b[0]));
  const entries = paletteEntries(initial.keymap, names);
  for (const entry of entries) assert.ok(entry.keys.length > 0, entry.title);
  assert.deepEqual(entries.find((entry) => entry.title === "Palette").keys, ["⇧⌘P"]);
  assert.deepEqual(entries.find((entry) => entry.title === "Next session").keys, ["⌘]"]);
  assert.deepEqual(entries.find((entry) => entry.title === "Select next").keys, ["↓"]);
  assert.deepEqual(entries.find((entry) => entry.title === "Toggle focus").keys, ["⇧⌘↩"]);
  assert.deepEqual(entries.find((entry) => entry.title === "Toggle split").keys, ["⌘D"]);
});

test("the Doctor's entries are palette actions, each warned by its kind, and the defaults show with none in settings", () => {
  const { settings } = reload(initial, { settings: `{ "zoom": 1 }`, keymap: null });
  assert.deepEqual(
    doctorEntries(settings.doctor).map(({ binding, title, note }) => [binding, title, note]),
    [
      [["studio::Doctor", "claude doctor"], "Doctor: Check the installation", "no tokens"],
      [["studio::Doctor", "/doctor"], "Doctor: Full checkup", "starts a Claude session, spends tokens"],
    ],
  );
  // The warning comes from the kind, not the label or the command.
  const [own] = doctorEntries([{ label: "Audit", command: "claude doctor", kind: "session" }]);
  assert.equal(own.note, "starts a Claude session, spends tokens");
});
