// The session list and the selection, as the shell last sent them, and the settings files in effect.

import { initial, type SettingsFiles } from "./settings.js";
import type { Selection, Session, SessionCards } from "./types.js";

// `pending` is the row Daniel just pressed, shown selected until the shell replies (danielrispler/Falinks#570).
export const studio = {
  listed: [] as Session[],
  selection: null as Selection | null,
  pending: null as { id: string; agent: string | null } | null,
  // A `selected` or `cards` event came while `pending` waited, so the press's settling fetches both.
  missed: false,
  // The selected session's brief, cards and decisions, as the shell last sent them.
  cards: { brief: null, goal: null, answers: [], open: [], design: [], recent: [], older: [], workers: [], decisions: [], risk_notes: [] } as SessionCards,
  // Focus mode (danielrispler/Falinks#573): the list and the rail folded, the terminal takes the window.
  focus: false,
  files: initial,
  // The two files' text as the shell last sent it: what the settings page's write is made from (#195).
  texts: { settings: null, keymap: null } as SettingsFiles,
};
