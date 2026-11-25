// Wait tips (#523, spec #520): while the selected session is Running, one muted line at the foot of its Intention box teaches one
// feature. The list is hand-written; the chooser is pure, and its show history lives in the page's storage.

import { keysOf } from "./keys.js";
import type { Keymap } from "./settings.js";

// The facts a tip's `when` may need, all of which the page already has: the shown project has a map, a PR brief is waiting
// in its tray, the session's agent records each turn's files. No usage is tracked.
type Condition = "map" | "brief" | "trees";
export type Context = Record<Condition, boolean>;

// From the grilling and prototype (2026-11-25).
export type Tip = {
  // Keys its show history; never reused.
  id: string;
  // Plain text, one line at the rail's width; "{key}" marks where the action's live key goes.
  text: string;
  // A palette action id, such as "studio::ShowHistory"; its key is drawn with the live keymap.
  action?: string;
  when?: Condition;
};

// The show history: the Running-stretch counter and, per tip, its show count and the stretch of its last show.
export type Shows = { stretch: number; tips: Record<string, { count: number; last: number }> };

// The longest line, "Tip: " and the default keys among it, that fits the rail's width at the small size.
export const LINE = 56;
// A tip shows at most this many times, ever.
export const MAX_SHOWS = 5;
// The longest cooldown, in Running stretches.
const LONGEST = 4;

export const TIPS: Tip[] = [
  { id: "palette", text: "Every action is in the palette ({key}).", action: "studio::Palette" },
  { id: "focus", text: "Focus mode ({key}) folds the list and rail away.", action: "studio::ToggleFocus" },
  { id: "needs-you", text: "Next needs you ({key}) jumps to a waiting session.", action: "studio::NextNeedsYou" },
  { id: "unseen", text: "Jump to unseen ({key}) opens the newest unseen.", action: "studio::JumpToUnseen" },
  { id: "next-session", text: "Next session ({key}) steps down the session list.", action: "studio::NextSession" },
  { id: "list", text: "Toggle list ({key}) opens or hides the list.", action: "studio::ToggleList" },
  { id: "history", text: "Show history ({key}) lists ended sessions.", action: "studio::ShowHistory" },
  { id: "ask", text: "Ask about ({key}) asks a side question.", action: "studio::AskAbout" },
  { id: "fit-map", text: "Fit map ({key}) puts the whole map in view.", action: "studio::FitMap", when: "map" },
  { id: "pre-flight", text: "Pre-flight ({key}) frames a task's parts.", action: "studio::PreFlight", when: "map" },
  { id: "full-diff", text: "Open full diff ({key}): the newest turn.", action: "studio::OpenFullDiff", when: "trees" },
  { id: "part-sheet", text: "Click a part on the map to open its sheet.", when: "map" },
  { id: "fog", text: "A fogged part changed on main since you caught up.", when: "map" },
  // The explained panels (the sheet, the brief's reader and Ratify) are all on the map.
  { id: "explanations", text: "A panel's ? toggles explanations ({key}).", action: "studio::ToggleExplanations", when: "map" },
  { id: "brief", text: "A PR brief waits in the map's tray; open it there.", when: "brief" },
];

// How many Running stretches a tip waits after it shows: never back to back, and a lone eligible tip every other stretch.
export const cooldown = (eligible: number): number => Math.max(1, Math.min(LONGEST, eligible - 1));

// The tip's line, "Tip: …", with its action's live key in place of "{key}", or where to find an unbound one.
export function tipText(tip: Tip, keymap: Keymap): string {
  const keys = tip.action ? keysOf(keymap, tip.action) : [];
  return `Tip: ${tip.text.replace("{key}", keys.length ? keys.join(", ") : "in the palette")}`;
}

// At the start of a Running stretch: the tip it shows, if any, and the history after it. Eligible tips are those whose `when`
// holds, under the cap and out of cooldown; the one shown longest ago, or never, wins, the list's order breaking ties. Not
// Running, or with tips off, nothing shows and the counter stays.
export function choose({ tips, shows, context, running, on }: { tips: Tip[]; shows: Shows; context: Context; running: boolean; on: boolean }): { tip: Tip | null; shows: Shows } {
  if (!running || !on) return { tip: null, shows };
  const stretch = shows.stretch + 1;
  const open = tips.filter((tip) => (tip.when === undefined || context[tip.when]) && (shows.tips[tip.id]?.count ?? 0) < MAX_SHOWS);
  const wait = cooldown(open.length);
  const last = (tip: Tip) => shows.tips[tip.id]?.last ?? -Infinity;
  const ready = open.filter((tip) => stretch - last(tip) > wait);
  const tip = ready.reduce<Tip | null>((best, t) => (best === null || last(t) < last(best) ? t : best), null);
  const record = tip ? { [tip.id]: { count: (shows.tips[tip.id]?.count ?? 0) + 1, last: stretch } } : {};
  return { tip, shows: { stretch, tips: { ...shows.tips, ...record } } };
}

// Where the history is kept: `localStorage` in the page. Losing it shows tips again from the start.
export type Store = { get(): string | null; set(text: string): void };

const KEY = "wait-tips";
export const pageStore: Store = {
  get: () => {
    try {
      return localStorage.getItem(KEY);
    } catch {
      return null;
    }
  },
  set: (text) => {
    try {
      localStorage.setItem(KEY, text);
    } catch {}
  },
};

function load(store: Store): Shows {
  try {
    const shows = JSON.parse(store.get() ?? "");
    if (typeof shows?.stretch === "number" && typeof shows.tips === "object" && shows.tips !== null) return shows;
  } catch {}
  return { stretch: 0, tips: {} };
}

type Seen = { id: string; state: string };

// The tip for the selected session, one per Running stretch: picked when its stretch starts and kept while it runs. A stretch
// ends when the session is seen not running, selected or in `listed`.
export function stretches(store: Store, tips: Tip[] = TIPS) {
  const current = new Map<string, Tip | null>();
  return (selected: Seen, listed: Seen[], context: Context, on: boolean): Tip | null => {
    for (const session of [...listed, selected]) if (session.state !== "running") current.delete(session.id);
    if (selected.state !== "running") return null;
    if (!current.has(selected.id)) {
      const picked = choose({ tips, shows: load(store), context, running: true, on });
      if (on) store.set(JSON.stringify(picked.shows));
      current.set(selected.id, picked.tip);
    }
    return on ? (current.get(selected.id) ?? null) : null;
  };
}
