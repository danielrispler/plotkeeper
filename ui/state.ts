// The session list and the selection, as the shell last sent them, and the settings files in effect.

import { initial } from "./settings.js";
import type { Selection, Session, SessionCards } from "./types.js";

// `pending` is the row Daniel just pressed, shown selected until the shell replies (#570).
export const studio = {
  listed: [] as Session[],
  selection: null as Selection | null,
  pending: null as { id: string; agent: string | null } | null,
  // A `selected` or `cards` event came while `pending` waited, so the press's settling fetches both.
  missed: false,
  // The selected session's brief, cards and decisions, as the shell last sent them.
  cards: { brief: null, recent: [], older: [], decisions: [] } as SessionCards,
  // Focus mode (#573): the list and the rail folded, the terminal takes the window.
  focus: false,
  files: initial,
};
