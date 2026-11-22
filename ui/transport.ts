// The page's one seam to plotkeeper (danielrispler/plotkeeper#325, spec #319 "The page and its transport"): a command is
// `POST /api/<command>` with its arguments as JSON, an event comes on the one `EventSource` at `/events`, and a log line is posted to
// `/api/log`. It installs the seam `dom.ts` reads, so nothing else in the page knows the transport.

import type { Command } from "./types.js";

type Seam = Window["__TAURI__"];

// The reply as JSON; a refusal's body is its reason, which is what the caller's `catch` gets, as a command's `Err` was.
async function post(command: Command | "log" | "page_focus", args: unknown): Promise<any> {
  const response = await fetch(`/api/${command}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(args ?? {}) });
  const body = await response.json().catch(() => null);
  if (!response.ok) throw typeof body === "string" ? body : `${command}: ${response.status}`;
  return body;
}

// The seam over `fetch` and `EventSource`, which are read when used, so a page or test may stub them.
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
  return {
    core: { invoke: <T>(command: string, args?: Record<string, unknown>) => post(command as Command, args) as Promise<T> },
    event: {
      listen: async <T>(name: string, handler: (event: { payload: T }) => void) => {
        const on = (message: Event) => handler({ payload: JSON.parse((message as MessageEvent<string>).data) });
        stream().addEventListener(name, on);
        return () => source?.removeEventListener(name, on);
      },
    },
    log: { warn: logger("warn"), error: logger("error") },
  };
}

window.__TAURI__ ??= transport();
