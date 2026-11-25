// The page's one seam to plotkeeper (danielrispler/plotkeeper#325, spec #319 "The page and its transport"): a command is
// `POST /api/<command>` with its arguments as JSON, an event comes on the one `EventSource` at `/events`, and a log line is posted to
// `/api/log`. It installs the seam `dom.ts` reads, so nothing else in the page knows the transport.

import type { Command } from "./types.js";

// The seam: commands, events and log lines. Its global keeps the name the Tauri shell gave it (#332 archived the shell), since the node
// tests stand in for it under that name; only this file and `dom.ts`'s one reader name it.
type Seam = {
  core: { invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> };
  log: { warn(message: string): Promise<void>; error(message: string): Promise<void> };
  event: { listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void> };
};
declare global {
  interface Window {
    __TAURI__: Seam;
  }
}

/** A command the server refused: its message is the refusal's reason, the body the server sent, else the command and its status. */
export class CommandError extends Error {
  override name = "CommandError";
  readonly command: string;
  readonly status: number;
  constructor(command: string, status: number, message: string) {
    super(message);
    this.command = command;
    this.status = status;
  }
}

// The reply as JSON; a refusal rejects with a `CommandError` whose message is its reason, as a command's `Err` was.
async function post(command: Command | "log" | "page_focus", args: unknown): Promise<unknown> {
  const response = await fetch(`/api/${command}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(args ?? {}) });
  const body: unknown = await response.json().catch(() => null);
  if (!response.ok) throw new CommandError(command, response.status, typeof body === "string" ? body : `${command}: ${response.status}`);
  return body;
}

/** The seam over `fetch` and `EventSource`, which are read when used, so a page or test may stub them. */
export function transport(): Seam {
  let source: EventSource | undefined;
  // Whether this tab is one Daniel is looking at: visible and focused. The server reads it as the window's focus.
  const reportFocus = () => post("page_focus", { focused: document.visibilityState === "visible" && document.hasFocus() }).catch(() => {});
  const stream = () => {
    if (!source) {
      source = new EventSource("/events");
      source.addEventListener("open", reportFocus);
      addEventListener("focus", reportFocus);
      addEventListener("blur", reportFocus);
      document.addEventListener("visibilitychange", reportFocus);
    }
    return source;
  };
  const logger = (level: "warn" | "error") => async (line: string) => void (await post("log", { level, line }));
  // The trust boundary: a reply and an event's payload are typed as `Commands` and the listener say, not checked. The wire check
  // (`server/tests/wire.rs`) holds the server's replies and argument names to those types, so the casts here are the only ones.
  return {
    core: { invoke: <T>(command: string, args?: Record<string, unknown>) => post(command as Command, args) as Promise<T> },
    event: {
      listen: <T>(name: string, handler: (event: { payload: T }) => void) => {
        const on = (message: Event) => handler({ payload: JSON.parse((message as MessageEvent<string>).data) });
        stream().addEventListener(name, on);
        return Promise.resolve(() => source?.removeEventListener(name, on));
      },
    },
    log: { warn: logger("warn"), error: logger("error") },
  };
}

window.__TAURI__ ??= transport();
