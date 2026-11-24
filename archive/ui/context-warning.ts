// Removed in #416 with the context share and its warning: the share's text and hover, the warning's crossings and note, its
// wiring in main.ts and its `context_warning` setting. Only sessions the desktop app started had a share (their status line
// snapshot). Fragments, kept verbatim, grouped by the file each came from; they do not compile on their own.

// From ui/types.ts:

// `context`: as Claude Code's status line measures it, from its snapshot: the tokens, the window,
// the percent used and left; with no snapshot, the transcript's tokens only (danielrispler/Falinks#517, decisions 1 and 2; danielrispler/Falinks#519).

export type Context = { tokens: number; window: number | null; share: number | null; left: number | null };

// From ui/list.ts:

// How full a session's context is (danielrispler/Falinks#517, decisions 2 and 3): "context 9%", its tokens such as "85k"
// when the window is unknown, or "" before it is known.
export function contextText(context: Context | null): string {
  if (!context) return "";
  return context.share === null ? thousands(context.tokens) : `context ${context.share}%`;
}

// Whether a context share is past the context warning's `threshold`, a whole percent; 0 never is.
export const pastWarning = (context: Context | null, threshold: number) =>
  threshold > 0 && context?.share != null && context.share > threshold;

// The context share's hover: its tokens of the window, and what Claude's status line shows: its own
// percent left.
export function contextTitle(context: Context | null): string {
  if (!context) return "";
  const used = thousands(context.tokens);
  if (context.window === null || context.left === null) return `${used} tokens used · the window size is unknown`;
  return `${used} of ${thousands(context.window)} tokens used · Claude's status line shows this as ${context.left}% left`;
}

// The mark's node, with the context share's hover; past the context warning, it reads at full strength, not in colour.
function markNode(session: Session): HTMLElement {

  const context = session.transcript.context;
  const past = pastWarning(context, studio.files.settings.context_warning);
  return fullTitle(text("span", rowMark(session), past ? "id past" : "id"), contextTitle(context));

      // A share past the context warning shows in the label's hover too.
      const past = !session.shell && !session.status_only && pastWarning(session.transcript.context, studio.files.settings.context_warning);
      button.title = `${sessionName(session)} · ${status(session.state, session.unseen, session.landed)[1]}${past ? ` · ${rowMark(session)}` : ""}`;

// From ui/notes.ts:

// The note for the sessions whose context share passed `threshold`: the first one's share.
export function contextNote(sessions: Session[], threshold: number): Note | null {
  if (!sessions.length) return null;
  const share = sessions[0].transcript.context?.share;
  const count = sessions.length > 1 ? `${sessions.length} past ${threshold}%` : "";
  return { session: sessions[0], count, line: `Context passed ${threshold}%: now ${share}%` };
}

// The sessions whose context share passed `threshold`, in the list's order, by `armed`: whether each session would warn
// on passing it. A session's first known share only arms it, when it is not past, so one already past when the app starts,
// or when its share first shows, raises no note. Each warns once, and is armed again when its share falls back to the
// threshold or under, as after `/compact` or `/clear`. A share not known keeps its place; an ended session is left as it is.
export function crossings(sessions: Session[], threshold: number, armed: Map<string, boolean>): Session[] {
  const crossed: Session[] = [];
  for (const session of sessions) {
    const context = session.transcript.context;
    if (session.state === "ended" || context?.share == null) continue;
    const past = pastWarning(context, threshold);
    if (past && armed.get(session.id)) crossed.push(session);
    armed.set(session.id, !past);
  }
  return crossed;
}

// Each session's arming, and the threshold it was armed at, whose change arms every session again.
const armed = new Map<string, boolean>();
let armedAt: number | null = null;

// Notes the sessions in `sessions` that passed `threshold`.
export function warnContext(sessions: Session[], threshold: number) {
  if (armedAt !== null && threshold !== armedAt) for (const id of armed.keys()) armed.set(id, true);
  armedAt = threshold;
  const note = contextNote(crossings(sessions, threshold, armed), threshold);
  if (note) showNote(note);
}

import { pastWarning, sessionName } from "./list.js";

// From ui/main.ts:

import { finishedNote, showNote, warnContext, watchNotes } from "./notes.js";

// The context warning waits for both the settings and the list, so it first arms the sessions at the threshold in the
// file, not the default's (`notes.ts`).
const ready = { settings: false, list: false };
const warn = () => ready.settings && ready.list && warnContext(studio.listed, studio.files.settings.context_warning);
listen<SettingsFiles>("settings-files", (files) => {
  applySettings(files);
  warn();
});
invoke("settings_files").then((files) => {
  applySettings(files);
  ready.settings = true;
  warn();
});

  showToast();
  ready.list = true;
  warn();
};

// From ui/settings.ts:

  // The context share, a whole percent, past which a session raises its context warning; 0: never.
  context_warning: number;

  context_warning: 30,

    context_warning: Math.min(100, Math.max(0, Math.round(pick("context_warning", json.context_warning, d.context_warning)))),
