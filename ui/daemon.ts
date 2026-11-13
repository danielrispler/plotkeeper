// The terminal daemon, through the shell (danielrispler/Falinks#466): the requests and events of the protocol in
// `terminal-daemon/src/lib.rs`, with terminal bytes base64-encoded in `data`.

import { invoke, listen } from "./dom.js";

// What the shell answers on connect: the configured repository, where new terminals start.
// `outdated` when the daemon is below this app's protocol (danielrispler/Falinks#470).
export interface Config {
  repo: string;
  outdated: boolean;
}

// One daemon event: `spawned`, `error`, `replay`, `output`, `exit`, `list`, or `lost` from the shell.
export interface DaemonEvent {
  ev: string;
  id?: string;
  data?: string;
  cols?: number;
  rows?: number;
  exited?: string | null;
  status?: string;
  message?: string;
  [field: string]: unknown;
}

// Connects to the daemon, starting it if none runs; again after `lost`, it reconnects.
export const connect = () => invoke<Config>("terminal_connect");

// Sends one request, such as `{ op: "attach", id }`.
export const request = (msg: Record<string, unknown>) => invoke<void>("terminal_send", { msg });

// Every daemon event, for every terminal; each pane keeps those of its own id. Resolves, once
// listening, to the function that stops it.
export const onEvent = (handler: (event: DaemonEvent) => void) => listen<DaemonEvent>("terminal", handler);

// One terminal in the daemon's `list` reply.
export interface Listed {
  id: string;
  argv: string[];
  cwd: string;
  exited: string | null;
}

// The daemon's terminals, from its next `list` reply.
const waiting: ((terms: Listed[]) => void)[] = [];
let listening: Promise<unknown> | null = null;
export async function list(): Promise<Listed[]> {
  listening ??= onEvent((event) => {
    if (event.ev === "list") waiting.splice(0).forEach((reply) => reply((event.terms as Listed[]) ?? []));
  });
  await listening;
  const reply = new Promise<Listed[]>((resolve) => waiting.push(resolve));
  await request({ op: "list" });
  return reply;
}

export const encode = (bytes: Uint8Array) => btoa(Array.from(bytes, (b) => String.fromCharCode(b)).join(""));
export const decode = (data: string) => Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
