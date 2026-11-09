// Settings and keymap (#524, #517 decision 17): `settings.json` and `keymap.json` in the studio's
// data folder, in Zed's format (JSON with comments and trailing commas). The shell sends both files'
// text at start and each time one is saved; a file that does not parse is reported and the last good
// one stays in effect.

// An action, or an action with its argument, as Zed writes `["terminal::SendText", "\u0001"]`.
export type Binding = string | [string, string];
// Keystrokes, as Zed writes them ("cmd-1", "escape"), to actions, by context: "Workspace" for the
// window, "Terminal" for the terminal pane.
export type Keymap = Record<string, Record<string, Binding>>;

export interface DoctorEntry {
  label: string;
  command: string;
  // `shell` runs the command in a new terminal; `session` starts `claude` with it as the prompt.
  kind: "shell" | "session";
}

export interface Settings {
  zoom: number;
  // null: the stylesheet's `--size-mono`.
  terminal: { font_size: number | null };
  // The summary writer's model and effort, and whether opening a turn writes its summary.
  summaries: { model: string; effort: string; on_open: boolean };
  doctor: DoctorEntry[];
}

export interface Loaded {
  settings: Settings;
  keymap: Keymap;
  // One line per file that did not parse, such as "settings.json: Unexpected end of JSON input".
  errors: string[];
}

export const ZOOM_MIN = 0.5;
export const ZOOM_MAX = 2;
const ZOOM_STEP = 0.1;

export const clampZoom = (zoom: number) => Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom));
// The zoom `by` steps from `zoom`, inside its bounds; 0 resets it.
export const zoomed = (zoom: number, by: number) => clampZoom(by === 0 ? 1 : Math.round((zoom + by * ZOOM_STEP) * 10) / 10);

const defaultSettings: Settings = {
  zoom: 1,
  terminal: { font_size: null },
  summaries: { model: "haiku", effort: "low", on_open: false },
  doctor: [
    { label: "Check the installation", command: "claude doctor", kind: "shell" },
    { label: "Full checkup", command: "/doctor", kind: "session" },
  ],
};

// Today's shortcuts. The terminal's Cmd+C and Cmd+V stay the Edit menu's (`keys.ts`).
const defaultKeymap: Keymap = {
  Workspace: {
    "cmd-1": "studio::ToggleList",
    "cmd-2": "studio::ToggleRail",
    escape: "studio::Dismiss",
    "cmd-=": "studio::ZoomIn",
    "cmd--": "studio::ZoomOut",
    "cmd-0": "studio::ResetZoom",
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

export const initial: Loaded = { settings: defaultSettings, keymap: defaultKeymap, errors: [] };

// Drops comments and trailing commas outside strings, so `JSON.parse` reads Zed's format.
function parseJsonc(text: string): unknown {
  const plain = text.replace(/("(?:\\.|[^"\\])*")|\/\/[^\n]*|\/\*[\s\S]*?\*\//g, (_, str) => str ?? "");
  return JSON.parse(plain.replace(/("(?:\\.|[^"\\])*")|,(\s*[}\]])/g, (_, str, end) => str ?? end));
}

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
// `value` when it has the type of `fallback`, else `fallback`.
const pick = <T>(value: unknown, fallback: T): T => (typeof value === typeof fallback ? (value as T) : fallback);

function readSettings(json: unknown): Settings {
  if (!isObject(json)) throw new Error("expected an object");
  const terminal = isObject(json.terminal) ? json.terminal : {};
  const summaries = isObject(json.summaries) ? json.summaries : {};
  const d = defaultSettings;
  return {
    zoom: clampZoom(pick(json.zoom, d.zoom)),
    terminal: { font_size: typeof terminal.font_size === "number" ? terminal.font_size : null },
    summaries: {
      model: pick(summaries.model, d.summaries.model),
      effort: pick(summaries.effort, d.summaries.effort),
      on_open: pick(summaries.on_open, d.summaries.on_open),
    },
    doctor: Array.isArray(json.doctor)
      ? json.doctor.filter(
          (e): e is DoctorEntry =>
            isObject(e) && typeof e.label === "string" && typeof e.command === "string" && (e.kind === "shell" || e.kind === "session"),
        )
      : d.doctor,
  };
}

// A keystroke with its modifiers in one order, ctrl-alt-shift-cmd, as `keys.ts` reads a key.
function normal(stroke: string): string {
  const [, mods, key] = /^((?:(?:ctrl|alt|shift|cmd)-)*)(.+)$/.exec(stroke.toLowerCase())!;
  return [...["ctrl", "alt", "shift", "cmd"].filter((m) => mods.includes(`${m}-`)), key].join("-");
}

const isBinding =(v: unknown): v is Binding =>
  typeof v === "string" || (Array.isArray(v) && v.length === 2 && v.every((s) => typeof s === "string"));

// Zed's keymap: an array of `{ "context", "bindings" }`, over the defaults. Unlike Zed, binding an
// action to a new key unbinds its default keys, so a rebound shortcut moves; `null` unbinds a key.
function readKeymap(json: unknown): Keymap {
  if (!Array.isArray(json)) throw new Error("expected an array");
  const keymap: Keymap = Object.fromEntries(Object.entries(defaultKeymap).map(([c, b]) => [c, { ...b }]));
  for (const entry of json) {
    if (!isObject(entry) || !isObject(entry.bindings)) continue;
    const context = typeof entry.context === "string" ? entry.context : "Workspace";
    const bindings = (keymap[context] ??= {});
    const same = (a: Binding, b: Binding) => JSON.stringify(a) === JSON.stringify(b);
    for (const action of Object.values(entry.bindings).filter(isBinding)) {
      for (const [key, bound] of Object.entries(defaultKeymap[context] ?? {})) if (same(bound, action)) delete bindings[key];
    }
    for (const [stroke, action] of Object.entries(entry.bindings)) {
      const key = normal(stroke);
      if (action === null) delete bindings[key];
      else if (isBinding(action)) bindings[key] = action;
    }
  }
  return keymap;
}

// The settings and keymap from the files' text (null: no file), each falling back to `previous`
// when its file does not parse.
export function reload(previous: Loaded, files: { settings: string | null; keymap: string | null }): Loaded {
  const errors: string[] = [];
  const read = <T>(name: string, text: string | null, parse: (json: unknown) => T, fallback: T, last: T): T => {
    if (text === null || !text.trim()) return fallback;
    try {
      return parse(parseJsonc(text));
    } catch (e) {
      errors.push(`${name}: ${e instanceof Error ? e.message : e}`);
      return last;
    }
  };
  const settings = read("settings.json", files.settings, readSettings, defaultSettings, previous.settings);
  const keymap = read("keymap.json", files.keymap, readKeymap, defaultKeymap, previous.keymap);
  return { settings, keymap, errors };
}
