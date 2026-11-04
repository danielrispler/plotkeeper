// The terminal pane's key mapping (#461, decision 7): one pure function from a key event, plus
// whether the clipboard holds an image, to what happens. The Cmd table is in the README.

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

// Cmd with no other modifier, by physical key; every Cmd shortcut not here goes to the app.
const cmd: Record<string, Action> = {
  KeyC: "native",
  KeyV: "native",
  KeyA: { command: "selectAll" },
  KeyF: { command: "find" },
  KeyK: { command: "clear" },
  // Start of line, end of line and delete to the line's start, as Zed's terminal sends them.
  ArrowLeft: { send: "\x01" },
  ArrowRight: { send: "\x05" },
  Backspace: { send: "\x15" },
};

// Option as Meta: ESC, then the key. Word moves and word delete as readline binds them.
const meta: Record<string, string> = {
  ArrowLeft: "\x1bb",
  ArrowRight: "\x1bf",
  Backspace: "\x1b\x7f",
  Enter: "\x1b\r",
};

export function keyAction(key: Key, clipboardHasImage: boolean): Action {
  const { code, metaKey, ctrlKey, altKey, shiftKey } = key;
  if (metaKey) {
    if (ctrlKey || altKey || shiftKey) return "app";
    // Claude Code reads the macOS clipboard itself on Ctrl+V and makes an `[Image #N]` chip.
    if (code === "KeyV" && clipboardHasImage) return { send: "\x16" };
    return cmd[code] ?? "app";
  }
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
