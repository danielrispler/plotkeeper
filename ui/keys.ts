// The terminal pane's key mapping (#461, decision 7): one pure function from a key event, plus
// whether the clipboard holds an image, to what happens. The Cmd table is in the README; the
// terminal's Cmd shortcuts come from `keymap.json` (#524).

import { type Binding, initial, type Keymap } from "./settings.js";

// The fields of a `KeyboardEvent` the mapping reads.
export interface Key {
  key: string;
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

// `send`: these bytes go to the terminal and xterm skips the key. `command`: the pane does it.
// "terminal": xterm encodes the key itself. "native": xterm skips it and the webview's Edit menu
// copies or pastes, firing the events xterm handles. "app": xterm skips it and the app takes it.
export type Action =
  | { send: string }
  | { command: "selectAll" | "find" | "clear" }
  | "terminal"
  | "native"
  | "app";

// Zed's names for the keys whose `code` is not `Key<letter>` or `Digit<n>`.
const names: Record<string, string> = {
  ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down", Equal: "=", Minus: "-",
  Comma: ",", Period: ".", Slash: "/", Semicolon: ";", Quote: "'", BracketLeft: "[", BracketRight: "]",
  Backslash: "\\", Backquote: "`",
};

// The key as Zed's keymap writes it, such as "cmd-shift-k", by the physical key.
export function keystroke({ code, metaKey, ctrlKey, altKey, shiftKey }: Key): string {
  const name = /^(?:Key|Digit)(.)$/.exec(code)?.[1].toLowerCase() ?? names[code] ?? code.toLowerCase();
  const mods = [ctrlKey && "ctrl", altKey && "alt", shiftKey && "shift", metaKey && "cmd"].filter(Boolean);
  return [...mods, name].join("-");
}

// The action `key` triggers in `context` of `keymap`.
export const bound = (keymap: Keymap, context: string, key: Key): Binding | undefined => keymap[context]?.[keystroke(key)];

// The terminal's commands, as `keymap.json` names them.
const commands: Record<string, "selectAll" | "find" | "clear"> = {
  "terminal::SelectAll": "selectAll",
  "terminal::Find": "find",
  "terminal::Clear": "clear",
};

// Option as Meta: ESC, then the key. Word moves and word delete as readline binds them.
const meta: Record<string, string> = {
  ArrowLeft: "\x1bb",
  ArrowRight: "\x1bf",
  Backspace: "\x1b\x7f",
  Enter: "\x1b\r",
};

export function keyAction(key: Key, clipboardHasImage: boolean, keymap: Keymap = initial.keymap): Action {
  const { code, metaKey, ctrlKey, altKey, shiftKey } = key;
  const plainCmd = metaKey && !ctrlKey && !altKey && !shiftKey;
  // Claude Code reads the macOS clipboard itself on Ctrl+V and makes an `[Image #N]` chip.
  if (plainCmd && code === "KeyV" && clipboardHasImage) return { send: "\x16" };
  if (plainCmd && (code === "KeyC" || code === "KeyV")) return "native";
  const action = bound(keymap, "Terminal", key);
  if (Array.isArray(action) && action[0] === "terminal::SendText") return { send: action[1] };
  if (typeof action === "string" && commands[action]) return { command: commands[action] };
  // Every Cmd shortcut the terminal has not bound goes to the app.
  if (metaKey) return "app";
  if (altKey && !ctrlKey) {
    // By the physical key, since Option changes what the layout types (Option+B is "∫").
    const letter = /^Key([A-Z])$/.exec(code)?.[1];
    if (letter) return { send: "\x1b" + (shiftKey ? letter : letter.toLowerCase()) };
    const digit = /^Digit(\d)$/.exec(code)?.[1];
    if (digit && !shiftKey) return { send: "\x1b" + digit };
    if (meta[code] && !shiftKey) return { send: meta[code] };
    return "terminal";
  }
  // ESC CR, as Claude Code's /terminal-setup binds Shift+Enter for VS Code: a newline, not a submit.
  if (code === "Enter" && shiftKey && !ctrlKey) return { send: "\x1b\r" };
  return "terminal";
}
