// The terminal pane's key mapping (#461, decision 7): one pure function from a key event, plus
// whether the clipboard holds an image, to what happens. The Cmd table is in the README; the
// terminal's Cmd shortcuts come from `keymap.json` (#524).

import { type Binding, type DoctorEntry, initial, type Keymap, MODIFIERS } from "./settings.js";

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
  const held = { ctrl: ctrlKey, alt: altKey, shift: shiftKey, cmd: metaKey };
  return [...MODIFIERS.filter((m) => held[m]), name].join("-");
}

// The action `key` triggers in `context` of `keymap`.
export const bound = (keymap: Keymap, context: string, key: Key): Binding | undefined => keymap[context]?.[keystroke(key)];

// The window's action for `key`; `inList`: the session list has focus, so its own keys come first.
// While typing in a field or the terminal, a binding without Cmd leaves the key to it, so plain
// control-letters and digits reach the terminal; only Dismiss runs, since it does not take the key.
// The answer keys answer only while `answerable`: focus is outside the terminal and text fields, and
// the docked card is on screen (#575, decision 7). The terminal never loses a digit to the card.
export function windowBinding(keymap: Keymap, key: Key, typing: boolean, inList = false, answerable = false): Binding | undefined {
  const action = (inList ? bound(keymap, "List", key) : undefined) ?? bound(keymap, "Workspace", key);
  if (Array.isArray(action) && action[0] === "studio::Answer" && !answerable) return undefined;
  return typing && !key.metaKey && action !== "studio::Dismiss" ? undefined : action;
}

// The window's actions by name (#575); an action with an argument, such as `["studio::Answer", "1"]`,
// gets it. A new view or menu adds its action here and appears in the palette, bound or not.
export type Actions = Record<string, (arg?: string) => void>;

// Runs `binding`'s action; false when the window has no such action.
export function run(actions: Actions, binding: Binding | undefined): boolean {
  const [name, arg] = typeof binding === "string" ? [binding] : (binding ?? [""]);
  actions[name]?.(arg);
  return name in actions;
}

// The session `by` steps from `selected` in the list's order, wrapping; with none selected, the first
// or the last.
export function stepSession(sessions: { id: string }[], selected: string | null, by: 1 | -1): string | null {
  if (!sessions.length) return null;
  const at = sessions.findIndex((session) => session.id === selected);
  const from = at === -1 ? (by === 1 ? -1 : 0) : at;
  return sessions[(from + by + sessions.length) % sessions.length].id;
}

// The first session that needs Daniel after `selected`, wrapping; with none, the first unseen (#556).
export function nextNeedsYou(sessions: { id: string; state: string; unseen?: boolean }[], selected: string | null): string | null {
  const at = sessions.findIndex((session) => session.id === selected);
  const order = [...sessions.slice(at + 1), ...sessions.slice(0, at + 1)];
  return (order.find((session) => session.state === "needs_you") ?? order.find((session) => session.state === "your_turn" && session.unseen))?.id ?? null;
}

const symbols: Record<string, string> = { ctrl: "⌃", alt: "⌥", shift: "⇧", cmd: "⌘", up: "↑", down: "↓", left: "←", right: "→", escape: "Esc", enter: "↩", backspace: "⌫", tab: "⇥", space: "Space" };

// A keystroke as macOS menus show it: "cmd-shift-p" is "⇧⌘P".
export function keyLabel(stroke: string): string {
  const [, mods, name] = new RegExp(`^((?:(?:${MODIFIERS.join("|")})-)*)(.+)$`).exec(stroke)!;
  return [...MODIFIERS.filter((m) => mods.includes(`${m}-`)), name].map((part) => symbols[part] ?? part.toUpperCase()).join("");
}

// What the palette calls an action: "studio::NextSession" is "Next session", with its argument after.
const title = (binding: Binding) => {
  const [name, arg] = typeof binding === "string" ? [binding] : binding;
  const words = name.replace(/^.*::/, "").replace(/(?<=[a-z])(?=[A-Z])/g, " ").toLowerCase();
  return [words[0].toUpperCase() + words.slice(1), arg].filter(Boolean).join(" ");
};

// `note`: a second line, such as a Doctor entry's cost.
export type Entry = { binding: Binding; title: string; keys: string[]; note?: string };

// The palette (#575): every action in `actions` that the window runs, in the keymap's order, each with
// the keys `keymap` binds it to now. An action unbound since, or never bound, is listed with none.
export function paletteEntries(keymap: Keymap, actions: string[]): Entry[] {
  const entries = new Map<string, Entry>();
  const add = (binding: Binding) => {
    const id = JSON.stringify(binding);
    const name = typeof binding === "string" ? binding : binding[0];
    if (actions.includes(name) && !entries.has(id)) entries.set(id, { binding, title: title(binding), keys: [] });
    return entries.get(id);
  };
  for (const map of [initial.keymap, keymap]) for (const bindings of Object.values(map)) Object.values(bindings).forEach(add);
  for (const bindings of Object.values(keymap)) for (const [stroke, binding] of Object.entries(bindings)) add(binding)?.keys.push(keyLabel(stroke));
  const named = new Set([...entries.values()].map((entry) => (typeof entry.binding === "string" ? entry.binding : entry.binding[0])));
  for (const name of actions) if (!named.has(name)) add(name);
  return [...entries.values()];
}

// What each Doctor kind costs (#527): the warning comes from the kind, never from the entry.
const doctorWarnings: Record<DoctorEntry["kind"], string> = { shell: "no tokens", session: "starts a Claude session, spends tokens" };

// The Doctor's entries as palette actions (#565, decision 7), each run by its command.
export const doctorEntries = (doctor: DoctorEntry[]): Entry[] =>
  doctor.map(({ label, command, kind }) => ({ binding: ["studio::Doctor", command], title: `Doctor: ${label}`, keys: [], note: doctorWarnings[kind] }));

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
