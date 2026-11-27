// The page's key mapping (danielrispler/Falinks#466, #326), run on the `tsc` output. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { bound, titled, keyLabel, keystroke, newestUnseen, nextNeedsYou, stepLine, paletteEntries, run, stepSession, windowBinding } from "../../dist/keys.js";
import { defaultsFor, initial, platformOf, reload } from "../../dist/settings.js";

const key = (code, mods = {}, key = "") => ({
  key, code, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...mods,
});

test("keystrokes read as Zed writes them, by the physical key", () => {
  assert.equal(keystroke(key("Digit1", { metaKey: true }, "1")), "cmd-1");
  assert.equal(keystroke(key("Escape", {}, "Escape")), "escape");
  assert.equal(keystroke(key("Equal", { metaKey: true }, "=")), "cmd-=");
  assert.equal(keystroke(key("Minus", { metaKey: true }, "-")), "cmd--");
  assert.equal(keystroke(key("KeyK", { metaKey: true, shiftKey: true, ctrlKey: true, altKey: true }, "˚")), "ctrl-alt-shift-cmd-k");
  assert.equal(keystroke(key("ArrowLeft", { metaKey: true })), "cmd-left");
});

test("a rebound shortcut triggers its action, and the old key no longer does", () => {
  const { keymap } = reload(initial, {
    settings: null,
    keymap: `[
      { "context": "Workspace", "bindings": { "cmd-l": "studio::ToggleList" } },
      { "context": "List", "bindings": { "down": null } },
    ]`,
  });
  assert.equal(bound(keymap, "Workspace", key("KeyL", { metaKey: true }, "l")), "studio::ToggleList");
  assert.equal(bound(keymap, "Workspace", key("BracketLeft", {}, "[")), undefined);
  assert.equal(bound(keymap, "Workspace", key("BracketRight", {}, "]")), "studio::ToggleRail");
  assert.equal(windowBinding(keymap, key("ArrowDown"), false, true), undefined);
  assert.equal(windowBinding(keymap, key("ArrowUp"), false, true), "studio::SelectPrevious");
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
  assert.equal(windowBinding(keymap, key("KeyL", { metaKey: true }, "l"), true), undefined);
});

// The list, story and fit keys are single keys off Chrome's ⌘1, ⌘2 and ⌘0 (spec #658, user story 28): a field takes them while typing.
test("[, ] and 0 run their actions, and do nothing while typing in a field", () => {
  const keys = [["BracketLeft", "[", "studio::ToggleList"], ["BracketRight", "]", "studio::ToggleRail"], ["Digit0", "0", "studio::FitMap"]];
  for (const [code, char, action] of keys) {
    assert.equal(windowBinding(initial.keymap, key(code, {}, char), false), action, char);
    assert.equal(windowBinding(initial.keymap, key(code, {}, char), true), undefined, `${char} while typing`);
    // From the focused list too, whose own context does not bind them.
    assert.equal(windowBinding(initial.keymap, key(code, {}, char), false, true), action, `${char} in the list`);
  }
  for (const code of ["Digit1", "Digit2", "Digit0"]) assert.equal(windowBinding(initial.keymap, key(code, { metaKey: true }), false), undefined, `⌘${code}`);
});

// A held key without Cmd runs its action once (spec #658's review): `[` and `]` would flicker, and a held digit answer the next question
// too. The arrows still step down the list while held, and a held Cmd key repeats.
test("a held key without Cmd runs once, except the list's arrows", () => {
  const held = (code, char, mods = {}) => ({ ...key(code, mods, char), repeat: true });
  for (const [code, char] of [["BracketLeft", "["], ["BracketRight", "]"], ["Digit0", "0"], ["Digit1", "1"], ["Escape", "Escape"]]) {
    assert.equal(windowBinding(initial.keymap, held(code, char), false, false, true), undefined, `${char} held`);
  }
  assert.equal(windowBinding(initial.keymap, held("ArrowDown", ""), false, true), "studio::SelectNext");
  assert.equal(windowBinding(initial.keymap, held("BracketRight", "]", { metaKey: true }), false), "studio::NextSession");
});

// The keyboard spine and the palette (danielrispler/Falinks#575).
const press = (code, mods = {}) => windowBinding(initial.keymap, key(code, mods), false, false, true);

test("each default key of the spine runs its action", () => {
  const ran = [];
  const actions = new Map(
    ["studio::NextSession", "studio::PreviousSession", "studio::NextNeedsYou", "studio::SelectNext", "studio::SelectPrevious",
      "studio::Answer", "studio::Palette"].map((name) => [name, (arg) => ran.push(arg ? `${name} ${arg}` : name)]),
  );
  const cmd = { metaKey: true };
  run(actions, press("BracketRight", cmd));
  run(actions, press("BracketLeft", cmd));
  run(actions, press("KeyJ", cmd));
  run(actions, windowBinding(initial.keymap, key("ArrowDown"), false, true));
  run(actions, windowBinding(initial.keymap, key("ArrowUp"), false, true));
  for (const n of ["1", "2", "3", "4"]) run(actions, press(`Digit${n}`));
  run(actions, press("KeyP", { metaKey: true, shiftKey: true }));
  assert.deepEqual(ran, [
    "studio::NextSession", "studio::PreviousSession", "studio::NextNeedsYou", "studio::SelectNext", "studio::SelectPrevious",
    "studio::Answer 1", "studio::Answer 2", "studio::Answer 3", "studio::Answer 4",
    "studio::Palette",
  ]);
  // The list's context falls back to the window's, and ↑ and ↓ move only while the list has focus.
  assert.equal(windowBinding(initial.keymap, key("BracketRight", { metaKey: true }), false, true), "studio::NextSession");
  assert.equal(windowBinding(initial.keymap, key("ArrowDown"), false), undefined);
});

test("the digits never answer while typing, and answer from the list only while the card is on screen", () => {
  // While typing: the digit is the field's, even with a card on screen.
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

test("shift-cmd-u is bound by default and jumps to the newest unseen session, in any project", () => {
  const binding = press("KeyU", { metaKey: true, shiftKey: true });
  assert.equal(binding, "studio::JumpToUnseen");
  assert.equal(press("KeyJ", { metaKey: true }), "studio::NextNeedsYou");
  const listed = [
    { id: "a", unseen: true, last_event_at: 5 },
    { id: "b", unseen: true, last_event_at: 9 },
    { id: "c", unseen: false, last_event_at: 20 },
  ];
  assert.equal(newestUnseen(listed), "b");
  assert.equal(newestUnseen([listed[2]]), null);
  assert.equal(newestUnseen([]), null);
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
  // An unseen session gone idle is still next.
  assert.equal(nextNeedsYou([{ id: "a", state: "idle", unseen: true }], null), "a");
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
    keymap: `[{ "bindings": { "cmd-l": "studio::ToggleList", "]": null } }]`,
  });
  const ran = [];
  const actions = new Map(Object.entries({
    "studio::ToggleList": () => ran.push("list"),
    "studio::ToggleRail": () => {},
    "studio::Answer": (n) => ran.push(`answer ${n}`),
    "studio::Doctor": () => {},
  }));
  const entries = paletteEntries(keymap, [...actions.keys()]);
  const find = (title) => entries.find((entry) => entry.title === title);
  assert.deepEqual(find("Toggle list").keys, ["⌘L"]);
  // Unbound, it is still listed, with no key.
  assert.deepEqual(find("Toggle rail").keys, []);
  assert.deepEqual(find("Answer 3").keys, ["3"]);
  // An action with no default key, such as a later menu, is listed too.
  assert.deepEqual(find("Doctor").keys, []);
  // Only the actions the page runs, not one it does not have.
  assert.equal(entries.some((entry) => entry.title === "Zoom in"), false);
  assert.equal(new Set(entries.map((entry) => entry.title)).size, entries.length);
  run(actions, find("Toggle list").binding);
  run(actions, find("Answer 3").binding);
  assert.deepEqual(ran, ["list", "answer 3"]);
});

test("every default window action has a title and a key in the palette", () => {
  const names = [...initial.keymap.get("Workspace").values(), ...initial.keymap.get("List").values()].map((b) => (typeof b === "string" ? b : b[0]));
  const entries = paletteEntries(initial.keymap, names);
  for (const entry of entries) assert.ok(entry.keys.length > 0, entry.title);
  assert.deepEqual(entries.find((entry) => entry.title === "Palette").keys, ["⇧⌘P"]);
  assert.deepEqual(entries.find((entry) => entry.title === "Next session").keys, ["⌘]"]);
  assert.deepEqual(entries.find((entry) => entry.title === "Select next").keys, ["↓"]);
  assert.deepEqual(entries.find((entry) => entry.title === "Toggle focus").keys, ["⇧⌘↩"]);
  assert.deepEqual(entries.find((entry) => entry.title === "Toggle list").keys, ["["]);
  assert.deepEqual(entries.find((entry) => entry.title === "Toggle rail").keys, ["]"]);
  assert.deepEqual(entries.find((entry) => entry.title === "Fit map").keys, ["0"]);
});

test("↑ and ↓ move the palette's highlighted row, and the window's keys leave them to its field (#24)", () => {
  // The palette's field is an input outside the list, where the window binds neither key.
  assert.equal(windowBinding(initial.keymap, key("ArrowDown"), true), undefined);
  assert.equal(windowBinding(initial.keymap, key("ArrowUp"), true), undefined);
});

test("the arrow keys step over session rows and subagent lines only, never another row", () => {
  const line = (classes, current = false) => ({ classes, current });
  const lines = [line(["row", "session-row"]), line(["row", "worker"]), line(["row"]), line(["row", "session-row"]), line(["row", "helper"])];
  assert.equal(stepLine(lines, 1), 0);
  lines[0].current = true;
  assert.equal(stepLine(lines, 1), 1);
  lines[0].current = false; lines[1].current = true;
  assert.equal(stepLine(lines, 1), 3);
  lines[1].current = false; lines[3].current = true;
  assert.equal(stepLine(lines, -1), 1);
  assert.equal(stepLine(lines, 1), 4);
  lines[3].current = false; lines[4].current = true;
  assert.equal(stepLine(lines, 1), null);
});

test("next needs-you and jump to unseen over the sidebar's sessions never land on an ended one", () => {
  const sidebar = [{ id: "a", state: "idle", unseen: false, last_event_at: 1 }, { id: "b", state: "idle", unseen: true, last_event_at: 2 }];
  assert.equal(nextNeedsYou(sidebar, "a"), "b");
  assert.equal(newestUnseen(sidebar), "b");
  assert.equal(stepSession(sidebar, "b", 1), "a");
});

test("a button's title names the key the keymap binds its action to now (#159)", () => {
  assert.equal(titled(initial.keymap, "Map", "studio::ShowMap"), "Map (⇧⌘H, ⇧⌘M)");
  const rebound = reload(initial, { settings: "{}", keymap: JSON.stringify([{ context: "Window", bindings: { "cmd-.": "studio::ShowMap" } }]) });
  assert.ok(titled(rebound.keymap, "Map", "studio::ShowMap").includes("⌘."), "the rebound key shows");
  assert.equal(titled(new Map(), "Map", "studio::ShowMap"), "Map");
});

// The settings page (#195): ⌘, opens it, Esc closes it even from a control in it, and the raw file's action is in the palette, unbound.
// The settings page is a dialog over the window (#193): while it is open, the window's keys behind it stay still, and only Dismiss runs.
// The map is the page's base (#386): Esc peels the map's layers and then does nothing, and the old Home key is the map's.
test("Esc on the bare map does nothing: no layer closes and the page stays on the map", async () => {
  const { escapeMap } = await import("../../dist/map-model.js");
  let closed = 0;
  const shut = { open: () => false, close: () => closed++ };
  assert.equal(escapeMap({ drawer: shut, brief: shut, scrubbed: shut, sheet: shut, panel: shut }), null);
  assert.equal(escapeMap({}), null);
  assert.equal(closed, 0);
  assert.equal(windowBinding(initial.keymap, key("Escape", {}, "Escape"), false), "studio::Dismiss");
});

test("a keymap.json that still names studio::GoHome runs studio::ShowMap, back to the map", () => {
  const { keymap } = reload(initial, { settings: null, keymap: `[{ "bindings": { "cmd-h": "studio::GoHome" } }]` });
  assert.equal(windowBinding(keymap, key("KeyH", { metaKey: true }, "h"), false), "studio::ShowMap");
  // The palette lists the one action, with the key the alias bound.
  const entries = paletteEntries(keymap, ["studio::ShowMap"]);
  assert.equal(entries.filter((e) => e.title === "Go home").length, 0);
  assert.ok(entries.find((e) => e.binding === "studio::ShowMap").keys.includes("⌘H"));
  // By default both old keys are the one action.
  assert.equal(windowBinding(initial.keymap, key("KeyH", { metaKey: true, shiftKey: true }, "h"), false), "studio::ShowMap");
  assert.equal(windowBinding(initial.keymap, key("KeyM", { metaKey: true, shiftKey: true }, "m"), false), "studio::ShowMap");
});

// #548: an action's name comes from keymap.json, so a name every object has (`toString`) must not find a function to run.
test("a binding named __proto__, constructor or toString runs nothing: it is an unknown action", () => {
  const ran = [];
  const actions = new Map([["studio::ToggleList", () => ran.push("list")]]);
  for (const name of ["__proto__", "constructor", "toString", "hasOwnProperty", "get"]) {
    assert.equal(run(actions, name), false, name);
    assert.equal(run(actions, [name, "1"]), false, name);
  }
  assert.equal(run(actions, "studio::ToggleList"), true);
  assert.deepEqual(ran, ["list"]);
  // In the palette it is an action the page does not have, as any unknown name is.
  const { keymap } = reload(initial, { settings: null, keymap: `[{ "bindings": { "cmd-8": "constructor", "cmd-7": "toString" } }]` });
  const titles = paletteEntries(keymap, [...actions.keys()]).map((entry) => entry.title);
  assert.deepEqual(titles, ["Toggle list"]);
});

// Linux (spec #510, user stories 22-25; #705). The page picks its default keymap by platform: Cmd on macOS, Ctrl elsewhere.
// The keys the browser or the desktop already owns on Linux, so no default may sit on them. Chrome's from its Windows & Linux list
// (support.google.com/chrome/answer/157179, read 2026-11-27); Firefox's and IBus's from their usual bindings (unverified here).
const BROWSER_OWNED = [
  // Chrome: tabs, windows, find, history, downloads, address bar, print, save, reload, open, source, bookmark, zoom.
  ...["1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "=", "-", "tab"].map((k) => `ctrl-${k}`),
  ..."nwtfghjlkeprsoud".split("").map((k) => `ctrl-${k}`),
  // Chrome: incognito, reopen, close window, bookmarks bar and manager, find previous, devtools, hard reload, bookmark all, profiles;
  // Ctrl+Enter and Ctrl+Shift+Enter in the address bar.
  ..."ntwbogjrdm".split("").map((k) => `ctrl-shift-${k}`), "ctrl-shift-delete", "ctrl-enter", "ctrl-shift-enter",
  // Firefox: library, page info, bookmarks sidebar, mute, quit, redo; private window, history, responsive view, consoles,
  // downloads, screenshot, text direction, add-ons, inspector. IBus: Ctrl+Shift+U types a Unicode character.
  ..."ibmqy".split("").map((k) => `ctrl-${k}`),
  ..."phmkeyjsxaciqu".split("").map((k) => `ctrl-shift-${k}`),
  // Chrome's menu and its back and forward.
  "alt-f", "alt-e", "alt-d", "alt-left", "alt-right", "alt-home",
];

test("the default keymap is Cmd's on macOS and Ctrl's on Linux, never on a key the browser owns", () => {
  assert.equal(platformOf("MacIntel"), "mac");
  assert.equal(platformOf("macOS"), "mac");
  assert.equal(platformOf("Linux x86_64"), "linux");
  assert.equal(platformOf("Linux"), "linux");
  const mac = defaultsFor("mac").keymap.get("Workspace");
  assert.equal(mac.get("cmd-j"), "studio::NextNeedsYou");
  assert.equal(mac.get("shift-cmd-p"), "studio::Palette");
  const linux = defaultsFor("linux").keymap.get("Workspace");
  const strokes = [...linux.keys()];
  assert.ok(strokes.every((s) => !s.includes("cmd")), `a Linux default holds Cmd: ${strokes}`);
  for (const stroke of strokes) assert.ok(!BROWSER_OWNED.includes(stroke), `${stroke} is the browser's`);
  // Every action the macOS defaults bind is bound on Linux too, and each Cmd key moved to Ctrl.
  const actions = (map) => new Set([...map.values()].map((b) => JSON.stringify(b)));
  assert.deepEqual(actions(linux), actions(mac));
  for (const [stroke, action] of linux) if (!mac.has(stroke)) assert.match(stroke, /^ctrl-/, `${stroke} -> ${action}`);
  assert.deepEqual([...defaultsFor("linux").keymap.get("List")], [...defaultsFor("mac").keymap.get("List")]);
});

test("on Linux the Ctrl defaults run, and a held Ctrl key repeats as a held Cmd key does on macOS", () => {
  const { keymap } = defaultsFor("linux");
  const ctrl = (code, more = {}) => key(code, { ctrlKey: true, ...more });
  assert.equal(windowBinding(keymap, ctrl("BracketRight"), false, false, false, "linux"), "studio::NextSession");
  assert.equal(windowBinding(keymap, ctrl("BracketLeft"), false, false, false, "linux"), "studio::PreviousSession");
  assert.equal(windowBinding(keymap, ctrl("Backslash"), false, false, false, "linux"), "studio::NextNeedsYou");
  assert.equal(windowBinding(keymap, ctrl("Backslash", { shiftKey: true }), false, false, false, "linux"), "studio::JumpToUnseen");
  assert.equal(windowBinding(keymap, ctrl("Space", { shiftKey: true }), false, false, false, "linux"), "studio::Palette");
  assert.equal(windowBinding(keymap, ctrl("KeyL", { shiftKey: true }), false, false, false, "linux"), "studio::ShowMap");
  assert.equal(windowBinding(keymap, ctrl("KeyF", { shiftKey: true }), false, false, false, "linux"), "studio::ToggleFocus");
  // Cmd (Super) does nothing on Linux, and a held Ctrl key steps on.
  assert.equal(windowBinding(keymap, key("BracketRight", { metaKey: true }), false, false, false, "linux"), undefined);
  assert.equal(windowBinding(keymap, { ...ctrl("BracketRight"), repeat: true }, false, false, false, "linux"), "studio::NextSession");
  // Typing in a field, a Ctrl shortcut still runs, as a Cmd one does on macOS.
  assert.equal(windowBinding(keymap, ctrl("BracketRight"), true, false, false, "linux"), "studio::NextSession");
});

test("on Linux the text-editing Ctrl keys keep their text meaning in a field, even when keymap.json binds them", () => {
  const strokes = ["ctrl-a", "ctrl-c", "ctrl-v", "ctrl-x", "ctrl-z", "ctrl-shift-z", "ctrl-y", "ctrl-left", "ctrl-shift-right", "ctrl-backspace", "ctrl-delete", "ctrl-home", "ctrl-shift-end"];
  const bindings = Object.fromEntries(strokes.map((s) => [s, "studio::Palette"]));
  const { keymap } = reload(defaultsFor("linux"), { settings: null, keymap: JSON.stringify([{ bindings }]) }, "linux");
  const codes = { a: "KeyA", c: "KeyC", v: "KeyV", x: "KeyX", z: "KeyZ", y: "KeyY", left: "ArrowLeft", right: "ArrowRight", backspace: "Backspace", delete: "Delete", home: "Home", end: "End" };
  for (const stroke of strokes) {
    const name = stroke.replace(/^ctrl-(shift-)?/, "");
    const press = key(codes[name], { ctrlKey: true, shiftKey: stroke.includes("shift-") });
    assert.equal(windowBinding(keymap, press, true, false, false, "linux"), undefined, `${stroke} while typing`);
    // Outside a field the user's binding runs.
    assert.equal(windowBinding(keymap, press, false, false, false, "linux"), "studio::Palette", `${stroke} outside a field`);
  }
});

test("the palette shows Ctrl on Linux and the macOS glyphs on macOS", () => {
  assert.equal(keyLabel("ctrl-shift-space", "linux"), "Ctrl+Shift+Space");
  assert.equal(keyLabel("ctrl-]", "linux"), "Ctrl+]");
  assert.equal(keyLabel("ctrl-alt-left", "linux"), "Ctrl+Alt+←");
  assert.equal(keyLabel("cmd-k", "linux"), "Super+K");
  assert.equal(keyLabel("escape", "linux"), "Esc");
  assert.equal(keyLabel("0", "linux"), "0");
  assert.equal(keyLabel("shift-cmd-p", "mac"), "⇧⌘P");
  const linux = defaultsFor("linux").keymap;
  const palette = paletteEntries(linux, ["studio::Palette", "studio::NextSession"], "linux");
  assert.deepEqual(palette.map((e) => e.keys), [["Ctrl+]"], ["Ctrl+Shift+Space"]]);
  assert.equal(titled(linux, "Map", "studio::ShowMap", "linux"), "Map (Ctrl+Shift+L)");
  assert.deepEqual(paletteEntries(defaultsFor("mac").keymap, ["studio::Palette"], "mac")[0].keys, ["⇧⌘P"]);
});
