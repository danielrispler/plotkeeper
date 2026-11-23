// The page `npm run snap` renders: dist's index.html with a stub Tauri API in front of the app's
// module script. The stub answers each command from the fixture's key of that name, else null.
// It exists only in the page written to a temp directory, never in dist or the shipped app.

import { hoverAudit } from "./hover.mjs";

export const stubSource = (fixture) => `window.__TAURI__ = (() => {
  const fixture = ${JSON.stringify(fixture)};
  const listeners = {};
  // The app's errors are the fixture's appErrors key (none by default), sent on \`log://log\` as its Webview target sends them.
  const listen = async (name, handler) => {
    (listeners[name] ??= []).push(handler);
    if (name === "log://log") for (const message of fixture.appErrors ?? []) setTimeout(() => handler({ payload: { level: 5, message } }));
    return () => {};
  };
  // The daemon's terminals are the fixture's terminals key (none by default). A list request is answered with them, which the
  // terminals' revival waits for, and an attach with the terminal's recorded screen, as the daemon replays it at start.
  const terminals = fixture.terminals ?? [];
  const emit = (payload) => setTimeout(() => (listeners.terminal ?? []).forEach((h) => h({ payload })));
  const invoke = async (command, args) => {
    if (command === "terminal_send" && args?.msg?.op === "list") emit({ ev: "list", terms: terminals.map(({ replay, ...listed }) => listed) });
    if (command === "terminal_send" && args?.msg?.op === "attach") {
      const found = terminals.find((t) => t.id === args.msg.id);
      if (found) emit({ ev: "replay", id: found.id, ...found.replay, exited: found.exited });
    }
    const reply = fixture[command];
    // A reply that depends on one argument: { __by: "id", values: { <id>: reply } }.
    const found = reply?.__by ? (reply.values[args?.[reply.__by]] ?? null) : reply;
    // A reply of { __throw: "why" } is a command that fails with that reason.
    if (found?.__throw) throw found.__throw;
    // A reply of { __delay: ms, value } comes after ms: a command whose answer lands after the page has drawn.
    if (found?.__delay) {
      await new Promise((done) => setTimeout(done, found.__delay));
      return found.value ?? null;
    }
    return found ?? null;
  };
  // The UI's own log lines are not echoed back: the app's Webview target drops records from the webview. Only an app error
  // reaches the page on \`log://log\`.
  const log = { warn: async () => {}, error: async () => {} };
  // A fixture's script raises a shell event with __emit, as the shell's emit would.
  const __emit = (name, payload) => (listeners[name] ?? []).forEach((h) => h({ payload }));
  return { event: { listen }, core: { invoke }, log, __emit };
})();`;

// What runs in the page's head, after the stub: the body's classes (set once the body exists), then the fixture's script once
// the UI has drawn (the page has loaded and its first replies have landed). The script is the body of an async function that
// gets `log`; what it logs reaches snap's output, and a line that starts `snap:fail` makes snap exit non-zero. A script that
// throws logs a `snap:fail` line. The hover rule's check runs after it in every page, also after a script that returned early or
// threw (hover.mjs).
export const headSource = ({ bodyClasses = [], script = null } = {}) => `
  addEventListener("DOMContentLoaded", () => document.body.classList.add(...${JSON.stringify(bodyClasses)}));
  addEventListener("load", () => setTimeout(async () => {
    const log = (...parts) => console.log(parts.join(" "));
    try {
      await (async (log) => {\n${script ?? ""}\n})(log);
    } catch (error) {
      log("snap:fail the script threw: " + error);
    }
    try {
      await (async (log) => {\n${hoverAudit}\n})(log);
    } catch (error) {
      log("snap:fail the hover check threw: " + error);
    }
  }, 2500));`;

export const snapPage = (index, fixture, options = {}) =>
  index.replace(
    '<script type="module"',
    () => `<script>${stubSource(fixture)}${headSource({ script: fixture.snapScript ?? null, ...options })}</script>\n  <script type="module"`,
  );

// The lines of Chrome's stderr that are page console output, as the page logged them.
export const consoleLines = (stderr) =>
  stderr.split("\n").flatMap((line) => {
    const match = /CONSOLE[^\]]*\] "(.*)", source: /.exec(line);
    return match ? [match[1].replace(/\\"/g, '"').replace(/\\n/g, "\n")] : [];
  });

// Whether the run failed: a log line that starts `snap:fail`, or an uncaught exception or unhandled rejection in the page,
// which Chrome logs as `Uncaught …` (#144).
// A fixture that names its last line (`snapDone`) fails too when that line never came: Chrome's virtual time budget can cut the script
// off before its assertions run, and nothing else would say so.
export const failed = (lines, done = undefined) =>
  lines.some((line) => line.startsWith("snap:fail") || line.startsWith("Uncaught")) || (done !== undefined && !lines.includes(done));

// What `snap` prints when Chrome exits non-zero: the status and the last lines of its stderr, which the console lines leave out.
// A zero status reports nothing.
export const chromeFailure = (status, stderr, tail = 10) =>
  status === 0 ? "" : `chrome: exit status ${status}\n${stderr.trimEnd().split("\n").slice(-tail).join("\n")}`;
