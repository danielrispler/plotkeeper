// The page's key mapping (danielrispler/Falinks#461, decision 7; #326): pure functions from a key event to the action it
// triggers. The shortcuts come from `keymap.json` (danielrispler/Falinks#524).

import { type Binding, initial, type Keymap, MODIFIERS } from "./settings.js";

// The fields of a `KeyboardEvent` the mapping reads.
export interface Key {
  key: string;
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

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
// While typing in a field, a binding without Cmd leaves the key to it; only Dismiss runs, since it does not take the key.
// The answer keys answer only while `answerable`: focus is outside text fields, and the docked card is on screen
// (danielrispler/Falinks#575, decision 7).
export function windowBinding(keymap: Keymap, key: Key, typing: boolean, inList = false, answerable = false): Binding | undefined {
  const action = (inList ? bound(keymap, "List", key) : undefined) ?? bound(keymap, "Workspace", key);
  if (Array.isArray(action) && action[0] === "studio::Answer" && !answerable) return undefined;
  return typing && !key.metaKey && action !== "studio::Dismiss" ? undefined : action;
}

// The window's actions by name (danielrispler/Falinks#575); an action with an argument, such as `["studio::Answer", "1"]`,
// gets it. A new view or menu adds its action here and appears in the palette, bound or not.
export type Actions = Record<string, (arg?: string) => void>;

// Runs `binding`'s action; false when the window has no such action.
export function run(actions: Actions, binding: Binding | undefined): boolean {
  const [name, arg] = typeof binding === "string" ? [binding] : (binding ?? [""]);
  actions[name]?.(arg);
  return name in actions;
}

// The line the arrow keys step to, `by` from the current one, or from the edge when none is current: only session rows and
// subagent lines count, Null at the end.
export function stepLine(lines: { classes: string[]; current: boolean }[], by: 1 | -1): number | null {
  const steps = lines.flatMap((line, at) => (["session-row", "worker", "helper"].some((c) => line.classes.includes(c)) ? [at] : []));
  const here = steps.findIndex((at) => lines[at].current);
  return steps[here === -1 ? (by === 1 ? 0 : steps.length - 1) : here + by] ?? null;
}

// The session `by` steps from `selected` in the list's order, wrapping; with none selected, the first
// or the last.
export function stepSession(sessions: { id: string }[], selected: string | null, by: 1 | -1): string | null {
  if (!sessions.length) return null;
  const at = sessions.findIndex((session) => session.id === selected);
  const from = at === -1 ? (by === 1 ? -1 : 0) : at;
  return sessions[(from + by + sessions.length) % sessions.length].id;
}

// The first session that needs Daniel after `selected`, wrapping; with none, the first unseen (danielrispler/Falinks#556).
export function nextNeedsYou(sessions: { id: string; state: string; unseen?: boolean }[], selected: string | null): string | null {
  const at = sessions.findIndex((session) => session.id === selected);
  const order = [...sessions.slice(at + 1), ...sessions.slice(0, at + 1)];
  return (order.find((session) => session.state === "needs_you") ?? order.find((session) => session.unseen))?.id ?? null;
}

// The unseen session with the newest `last_event_at`, in any project; none when none is unseen.
export function newestUnseen(sessions: { id: string; unseen?: boolean; last_event_at: number }[]): string | null {
  return sessions.filter((session) => session.unseen).reduce<typeof sessions[number] | null>((best, s) => (!best || s.last_event_at > best.last_event_at ? s : best), null)?.id ?? null;
}

const symbols: Record<string, string> = { ctrl: "⌃", alt: "⌥", shift: "⇧", cmd: "⌘", up: "↑", down: "↓", left: "←", right: "→", escape: "Esc", enter: "↩", backspace: "⌫", tab: "⇥", space: "Space" };

// A keystroke as macOS menus show it: "cmd-shift-p" is "⇧⌘P".
export function keyLabel(stroke: string): string {
  const [, mods, name] = new RegExp(`^((?:(?:${MODIFIERS.join("|")})-)*)(.+)$`).exec(stroke)!;
  return [...MODIFIERS.filter((m) => mods.includes(`${m}-`)), name].map((part) => symbols[part] ?? part.toUpperCase()).join("");
}

// `label` with the keys `keymap` binds `name` to now, as "Settings (⌘,)"; the bare label when none.
export function titled(keymap: Keymap, label: string, name: string): string {
  const keys = Object.values(keymap).flatMap((bindings) =>
    Object.entries(bindings).filter(([, binding]) => (typeof binding === "string" ? binding : binding[0]) === name).map(([stroke]) => keyLabel(stroke)));
  return keys.length ? `${label} (${[...new Set(keys)].join(", ")})` : label;
}

// What the palette calls an action: "studio::NextSession" is "Next session", with its argument after.
const title = (binding: Binding) => {
  const [name, arg] = typeof binding === "string" ? [binding] : binding;
  const words = name.replace(/^.*::/, "").replace(/(?<=[a-z])(?=[A-Z])/g, " ").toLowerCase();
  return [words[0].toUpperCase() + words.slice(1), arg].filter(Boolean).join(" ");
};

// `note`: a second line, such as a Doctor entry's cost.
export type Entry = { binding: Binding; title: string; keys: string[]; note?: string };

// The palette (danielrispler/Falinks#575): every action in `actions` that the window runs, in the keymap's order, each with
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
