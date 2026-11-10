// Typing into a pane (#544), by herdr's rules (herdr-core `input.rs`): the studio is the only
// typist besides Daniel, so it waits for his draft, never answers a session's dialog, and pastes
// before it presses Enter. Every text the studio types into a terminal goes through `typeInto`.

// Daniel's draft in each terminal, by its id: roughly how many characters he left in the prompt,
// and when he last pressed a key.
const drafts = new Map<string, { length: number; at: number }>();

// Takes Daniel's keys `data` in terminal `id` at `at`: Enter sends the draft, Ctrl-U and Ctrl-C
// clear it, a backspace takes one character off, and other control keys and escape sequences
// change nothing but the time.
// shortcut: counts characters, not Claude Code's real prompt; read its screen if the count drifts.
export function noteKeys(id: string, data: string, at = Date.now()) {
  let length = drafts.get(id)?.length ?? 0;
  for (const key of data.replace(/\x1b(\[[0-9;?]*[ -/]*[@-~]|\][^\x07]*\x07|.)/g, "")) {
    if (key === "\r" || key === "\x15" || key === "\x03") length = 0;
    else if (key === "\x7f") length = Math.max(0, length - 1);
    else if (key >= " ") length++;
  }
  drafts.set(id, { length, at });
}

// How long Daniel must have stopped typing, how long the studio waits at most, and the pause
// between the paste and Enter.
const QUIET_MS = 3000;
const WAIT_MS = 60_000;
const ENTER_AFTER_MS = 200;
const POLL_MS = 250;

export interface TypingIo {
  now: () => number;
  sleep: (ms: number) => Promise<void>;
  send: (data: string) => Promise<unknown>;
  notice: (text: string) => void;
}

// Types `text` into terminal `id` as a bracketed paste, then Enter 200 ms later, once Daniel has
// no draft there and has not pressed a key for 3 s. It never types while `blocked`, the session
// needing him, since the text would answer its dialog, nor over a draft still there after 60 s:
// either shows a notice instead. Returns whether it typed.
export async function typeInto(id: string, text: string, blocked: () => boolean, io: TypingIo): Promise<boolean> {
  const until = io.now() + WAIT_MS;
  for (;;) {
    if (blocked()) {
      io.notice(`Not typed: the session needs you, and "${text}" would answer it. Type it yourself.`);
      return false;
    }
    const draft = drafts.get(id);
    if (!draft || (draft.length === 0 && io.now() - draft.at >= QUIET_MS)) break;
    if (io.now() >= until) {
      io.notice(`Not typed: your draft is still in the session's prompt. Send or clear it, then type "${text}".`);
      return false;
    }
    await io.sleep(POLL_MS);
  }
  await io.send(`\x1b[200~${text}\x1b[201~`);
  await io.sleep(ENTER_AFTER_MS);
  await io.send("\r");
  return true;
}
