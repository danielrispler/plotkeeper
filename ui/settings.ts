// Settings and keymap (danielrispler/Falinks#524, danielrispler/Falinks#517 decision 17): `settings.json` and `keymap.json` in the studio's
// data folder, in Zed's format (JSON with comments and trailing commas). The shell sends both files'
// text at start and each time one is saved; a file that does not parse is reported and the last good
// one stays in effect.

// An action, or an action with its argument, as Zed writes `["terminal::SendText", "\u0001"]`.
export type Binding = string | [string, string];
// Keystrokes, as Zed writes them ("cmd-1", "escape"), to actions, by context: "Workspace" for the
// window, "List" for the session list while it has focus, "Terminal" for the terminal pane.
export type Keymap = Record<string, Record<string, Binding>>;

export interface DoctorEntry {
  label: string;
  command: string;
  // `shell` runs the command in a new terminal; `session` starts `claude` with it as the prompt.
  kind: "shell" | "session";
}

// The settings: `settings.json`'s choices.
export interface Settings {
  zoom: number;
  // null: the stylesheet's `--size-mono`.
  terminal: { font_size: number | null };
  // The summary writer's model and effort ("" for the model's default), and whether opening a turn
  // writes its summary.
  summaries: { model: string; effort: string; on_open: boolean };
  // The model that writes a PR's brief and checks it (#337).
  pr_briefs: { model: string };
  // The needs-you and your-turn sounds (danielrispler/Falinks#556).
  sounds: boolean;
  // The context share, a whole percent, past which a session raises its context warning; 0: never.
  context_warning: number;
  doctor: DoctorEntry[];
  // The command that opens a file at its line (danielrispler/Falinks#557), an argv with `{path}`, `{line}` and `{column}`,
  // never a shell string. null: the shell's default, Zed's CLI when installed, else `open`.
  editor: string[] | null;
  // The configured repository (danielrispler/Falinks#605), which the shell reads from the file itself. null: the studio's checkout.
  repository: string | null;
  // The commands that are checks, per project folder (danielrispler/plotkeeper#112): each a command such as
  // "make verify", matched when a session's command runs it. A project not here uses the studio's list.
  checks: Record<string, string[]>;
  // The day, as "YYYY-MM-DD", the 21-day self-test of quiz-first against body-only turns starts (#130). null: no test.
  quiz_self_test: string | null;
}

// Both settings files, parsed: the settings and keymap in effect.
export interface Files {
  settings: Settings;
  keymap: Keymap;
  // One line per file that did not parse or key of the wrong type, such as
  // "settings.json: Unexpected end of JSON input".
  errors: string[];
  // `settings.json` does not parse (a key of the wrong type is not that): `settings` is the last good one, and the settings page
  // writes nothing (#195).
  broken: boolean;
}

export const ZOOM_MIN = 0.5;
export const ZOOM_MAX = 2;
const ZOOM_STEP = 0.1;

export const clampZoom = (zoom: number) => Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom));
// The zoom `by` steps from `zoom`, inside its bounds; 0 resets it.
export const zoomed = (zoom: number, by: number) => clampZoom(by === 0 ? 1 : Math.round((zoom + by * ZOOM_STEP) * 10) / 10);

export const FONT_MIN = 8;
export const FONT_MAX = 40;

// The zoom the window has after a reload of the settings: the file's when its `zoom` changed (the settings page writes it), else
// `current`, which ⌘+, ⌘− and ⌘0 may have moved; those keys never write the file.
export const reloadedZoom = (current: number, previous: Settings, next: Settings) => (next.zoom !== previous.zoom ? next.zoom : current);

const defaultSettings: Settings = {
  zoom: 1,
  terminal: { font_size: null },
  // Haiku at its default effort (danielrispler/Falinks#517, decision 14).
  summaries: { model: "haiku", effort: "", on_open: false },
  // Quality first: Opus, for the writer and the claim check (#334).
  pr_briefs: { model: "opus" },
  sounds: true,
  context_warning: 30,
  doctor: [
    { label: "Check the installation", command: "claude doctor", kind: "shell" },
    { label: "Full checkup", command: "/doctor", kind: "session" },
  ],
  editor: null,
  repository: null,
  checks: {},
  quiz_self_test: null,
};

// Today's shortcuts. The terminal's Cmd+C and Cmd+V stay the Edit menu's (`keys.ts`). "List" holds
// the keys of the session list while it has focus, over the window's (danielrispler/Falinks#575).
const defaultKeymap: Keymap = {
  Workspace: {
    "cmd-1": "studio::ToggleList",
    "cmd-2": "studio::ToggleRail",
    escape: "studio::Dismiss",
    // Focus mode (danielrispler/Falinks#573), in `MODIFIERS` order, as the lookup reads it.
    "shift-cmd-enter": "studio::ToggleFocus",
    // Split terminals (danielrispler/Falinks#529): the pane shown stays beside the next one selected.
    "cmd-d": "studio::ToggleSplit",
    // The terminal panel (danielrispler/Falinks#542), as in VS Code; it works from inside a terminal too (`keys.ts`).
    "ctrl-`": "studio::TogglePanel",
    "cmd-=": "studio::ZoomIn",
    "cmd--": "studio::ZoomOut",
    "cmd-0": "studio::ResetZoom",
    // The keyboard spine (danielrispler/Falinks#565, decision 7).
    "cmd-]": "studio::NextSession",
    "cmd-[": "studio::PreviousSession",
    "cmd-j": "studio::NextNeedsYou",
    "shift-cmd-u": "studio::JumpToUnseen",
    "1": ["studio::Answer", "1"],
    "2": ["studio::Answer", "2"],
    "3": ["studio::Answer", "3"],
    "4": ["studio::Answer", "4"],
    "cmd-n": "studio::NewSession",
    "cmd-t": "studio::NewTerminal",
    "shift-cmd-h": "studio::GoHome",
    // The Map view (danielrispler/plotkeeper#306): the selected session's project, else the first with a map; again, leaves it.
    "shift-cmd-m": "studio::ShowMap",
    "cmd-,": "studio::OpenSettings",
    // Modifiers in `MODIFIERS` order, as a keystroke reads.
    "shift-cmd-p": "studio::Palette",
  },
  List: {
    down: "studio::SelectNext",
    up: "studio::SelectPrevious",
  },
  Terminal: {
    "cmd-a": "terminal::SelectAll",
    "cmd-f": "terminal::Find",
    "cmd-k": "terminal::Clear",
    // Start of line, end of line and delete to the line's start, as Zed's terminal sends them.
    "cmd-left": ["terminal::SendText", "\x01"],
    "cmd-right": ["terminal::SendText", "\x05"],
    "cmd-backspace": ["terminal::SendText", "\x15"],
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
  const terminal = isObject(json.terminal) ? json.terminal : {};
  const summaries = isObject(json.summaries) ? json.summaries : {};
  const prBriefs = isObject(json.pr_briefs) ? json.pr_briefs : {};
  const d = defaultSettings;
  // null when unset or of the wrong type: the stylesheet's size.
  const fontSize = pick("terminal.font_size", terminal.font_size ?? undefined, NaN);
  return {
    zoom: clampZoom(pick("zoom", json.zoom, d.zoom)),
    terminal: { font_size: Number.isNaN(fontSize) ? null : Math.min(FONT_MAX, Math.max(FONT_MIN, fontSize)) },
    summaries: {
      model: pick("summaries.model", summaries.model, d.summaries.model),
      effort: pick("summaries.effort", summaries.effort, d.summaries.effort),
      on_open: pick("summaries.on_open", summaries.on_open, d.summaries.on_open),
    },
    pr_briefs: { model: pick("pr_briefs.model", prBriefs.model, d.pr_briefs.model) },
    sounds: pick("sounds", json.sounds, d.sounds),
    context_warning: Math.min(100, Math.max(0, Math.round(pick("context_warning", json.context_warning, d.context_warning)))),
    doctor: Array.isArray(json.doctor)
      ? json.doctor.filter((e, i): e is DoctorEntry => {
          const ok =
            isObject(e) && typeof e.label === "string" && typeof e.command === "string" && (e.kind === "shell" || e.kind === "session");
          if (!ok) report(`doctor[${i}]: expected a label, a command and a kind of shell or session`);
          return ok;
        })
      : d.doctor,
    editor: readEditor(json.editor, report),
    repository: pick("repository", json.repository ?? undefined, "") || null,
    checks: readChecks(json.checks, report),
    quiz_self_test: pick("quiz_self_test", json.quiz_self_test ?? undefined, "") || null,
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

function readEditor(value: unknown, report: Report): string[] | null {
  if (value === undefined || value === null) return null;
  if (Array.isArray(value) && value.length > 0 && value.every((arg) => typeof arg === "string")) return value;
  report('editor: expected an array of strings, such as ["code", "-g", "{path}:{line}:{column}"]');
  return null;
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
