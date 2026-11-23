// Settings and keymap (danielrispler/Falinks#524, danielrispler/Falinks#517 decision 17): `settings.json` and `keymap.json` in the
// data folder, in Zed's format (JSON with comments and trailing commas). The server sends both files'
// text at start and each time one is saved; a file that does not parse is reported and the last good
// one stays in effect.

import type { WriterKind } from "./types.js";

// An action, or an action with its argument, as Zed writes `["terminal::SendText", "\u0001"]`.
export type Binding = string | [string, string];
// Keystrokes, as Zed writes them ("cmd-1", "escape"), to actions, by context: "Workspace" for the
// page, "List" for the session list while it has focus.
export type Keymap = Record<string, Record<string, Binding>>;

// The settings: `settings.json`'s choices.
export interface Settings {
  // The summary writer's model and effort ("" for the model's default), whether opening a turn
  // writes its summary, and the program that writes cards: Claude Code, the Antigravity CLI (#327) or Codex (#364).
  summaries: { model: string; effort: string; on_open: boolean; writer: WriterKind };
  // The context share, a whole percent, past which a session raises its context warning; 0: never.
  context_warning: number;
  // The commands that are checks, per project folder (danielrispler/plotkeeper#112): each a command such as
  // "make verify", matched when a session's command runs it. A project not here uses the studio's list.
  checks: Record<string, string[]>;
}

// Both settings files, parsed: the settings and keymap in effect.
export interface Files {
  settings: Settings;
  keymap: Keymap;
  // One line per file that did not parse or key of the wrong type, such as
  // "settings.json: Unexpected end of JSON input".
  errors: string[];
  // `settings.json` does not parse (a key of the wrong type is not that): `settings` is the last good one.
  broken: boolean;
}

// The writer the file names, or undefined, reporting a name that is none of them.
function writerKind(value: unknown, report: (line: string) => void): WriterKind | undefined {
  if (value === undefined || value === "claude" || value === "agy" || value === "codex" || value === "anthropic") return value;
  report('summaries.writer: expected "claude", "agy", "codex" or "anthropic"');
  return undefined;
}

const defaultSettings: Settings = {
  // Haiku at its default effort (danielrispler/Falinks#517, decision 14).
  summaries: { model: "haiku", effort: "", on_open: false, writer: "claude" },
  context_warning: 30,
  checks: {},
};

// Today's shortcuts. "List" holds the keys of the session list while it has focus, over the page's (danielrispler/Falinks#575).
const defaultKeymap: Keymap = {
  Workspace: {
    "cmd-1": "studio::ToggleList",
    "cmd-2": "studio::ToggleRail",
    escape: "studio::Dismiss",
    // Focus mode (danielrispler/Falinks#573), in `MODIFIERS` order, as the lookup reads it.
    "shift-cmd-enter": "studio::ToggleFocus",
    // The keyboard spine (danielrispler/Falinks#565, decision 7).
    "cmd-]": "studio::NextSession",
    "cmd-[": "studio::PreviousSession",
    "cmd-j": "studio::NextNeedsYou",
    "shift-cmd-u": "studio::JumpToUnseen",
    "1": ["studio::Answer", "1"],
    "2": ["studio::Answer", "2"],
    "3": ["studio::Answer", "3"],
    "4": ["studio::Answer", "4"],
    "shift-cmd-h": "studio::GoHome",
    // The Map view (danielrispler/plotkeeper#306): the selected session's project, else the first with a map; again, leaves it.
    "shift-cmd-m": "studio::ShowMap",
    // Modifiers in `MODIFIERS` order, as a keystroke reads.
    "shift-cmd-p": "studio::Palette",
  },
  List: {
    down: "studio::SelectNext",
    up: "studio::SelectPrevious",
  },
};

export const initial: Files = { settings: defaultSettings, keymap: defaultKeymap, errors: [], broken: false };

// Drops comments and trailing commas outside strings, so `JSON.parse` reads Zed's format.
function parseJsonc(text: string): unknown {
  const plain = text.replace(/("(?:\\.|[^"\\])*")|\/\/[^\n]*|\/\*[\s\S]*?\*\//g, (_, str) => str ?? "");
  return JSON.parse(plain.replace(/("(?:\\.|[^"\\])*")|,(\s*[}\]])/g, (_, str, end) => str ?? end));
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
type Report = (problem: string) => void;

function readSettings(json: unknown, report: Report): Settings {
  if (!isObject(json)) throw new Error("expected an object");
  // `value` when it has the type of `fallback`; else `fallback`, and a key of the wrong type is reported.
  const pick = <T>(name: string, value: unknown, fallback: T): T => {
    if (value === undefined || typeof value === typeof fallback) return (value ?? fallback) as T;
    report(`${name}: expected a ${typeof fallback}`);
    return fallback;
  };
  const summaries = isObject(json.summaries) ? json.summaries : {};
  const d = defaultSettings;
  return {
    summaries: {
      model: pick("summaries.model", summaries.model, d.summaries.model),
      effort: pick("summaries.effort", summaries.effort, d.summaries.effort),
      on_open: pick("summaries.on_open", summaries.on_open, d.summaries.on_open),
      writer: writerKind(summaries.writer, report) ?? d.summaries.writer,
    },
    context_warning: Math.min(100, Math.max(0, Math.round(pick("context_warning", json.context_warning, d.context_warning)))),
    checks: readChecks(json.checks, report),
  };
}

function readChecks(value: unknown, report: Report): Record<string, string[]> {
  if (value === undefined || value === null) return {};
  if (!isObject(value)) {
    report("checks: expected an object of project folders and their lists of commands");
    return {};
  }
  const lists: Record<string, string[]> = {};
  for (const [folder, list] of Object.entries(value)) {
    if (Array.isArray(list) && list.every((c) => typeof c === "string")) lists[folder] = list;
    else report(`checks.${folder}: expected an array of strings`);
  }
  return lists;
}

// The order modifiers take in a keystroke, here and in `keys.ts`.
export const MODIFIERS = ["ctrl", "alt", "shift", "cmd"] as const;

// A keystroke with its modifiers in `MODIFIERS` order; null: not a keystroke.
function normal(stroke: string): string | null {
  const parsed = new RegExp(`^((?:(?:${MODIFIERS.join("|")})-)*)(.+)$`).exec(stroke.toLowerCase());
  if (!parsed) return null;
  const [, mods, key] = parsed;
  return [...MODIFIERS.filter((m) => mods.includes(`${m}-`)), key].join("-");
}

const isBinding = (v: unknown): v is Binding =>
  typeof v === "string" || (Array.isArray(v) && v.length === 2 && v.every((s) => typeof s === "string"));

// Zed's keymap: an array of `{ "context", "bindings" }`, over the defaults. Unlike Zed, binding an
// action to a new key unbinds its default keys, so a rebound shortcut moves; `null` unbinds a key.
function readKeymap(json: unknown, report: Report): Keymap {
  if (!Array.isArray(json)) throw new Error("expected an array");
  const keymap: Keymap = Object.fromEntries(Object.entries(defaultKeymap).map(([c, b]) => [c, { ...b }]));
  for (const entry of json) {
    if (!isObject(entry) || !isObject(entry.bindings)) continue;
    const context = typeof entry.context === "string" ? entry.context : "Workspace";
    const bindings = (keymap[context] ??= {});
    const same = (a: Binding, b: Binding) => JSON.stringify(a) === JSON.stringify(b);
    // A key that is not a keystroke is reported and skipped, so it unbinds nothing either.
    const strokes = Object.entries(entry.bindings).flatMap(([stroke, action]) => {
      const key = normal(stroke);
      if (key === null) report(`${JSON.stringify(stroke)} is not a keystroke`);
      return key === null ? [] : [[key, action] as const];
    });
    for (const action of strokes.map(([, a]) => a).filter(isBinding)) {
      // Only keys still on their default: an earlier entry's binding stays.
      for (const [key, bound] of Object.entries(defaultKeymap[context] ?? {}))
        if (same(bound, action) && bindings[key] !== undefined && same(bindings[key], bound)) delete bindings[key];
    }
    for (const [key, action] of strokes) {
      if (action === null) delete bindings[key];
      else if (isBinding(action)) bindings[key] = action;
    }
  }
  return keymap;
}

// The two files' text, as the shell sends it; null: no file.
export type SettingsFiles = { settings: string | null; keymap: string | null };

// The settings files from their text, each falling back to `previous` when it does not parse.
export function reload(previous: Files, files: SettingsFiles): Files {
  const errors: string[] = [];
  let broken = false;
  const read = <T>(name: string, text: string | null, parse: (json: unknown, report: Report) => T, fallback: T, last: T): T => {
    if (text === null || !text.trim()) return fallback;
    try {
      return parse(parseJsonc(text), (problem) => errors.push(`${name}: ${problem}`));
    } catch (e) {
      errors.push(`${name}: ${e instanceof Error ? e.message : e}`);
      if (name === "settings.json") broken = true;
      return last;
    }
  };
  const settings = read("settings.json", files.settings, readSettings, defaultSettings, previous.settings);
  const keymap = read("keymap.json", files.keymap, readKeymap, defaultKeymap, previous.keymap);
  return { settings, keymap, errors, broken };
}
