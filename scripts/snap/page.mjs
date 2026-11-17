// The page `npm run snap` renders: dist's index.html with a stub Tauri API in front of the app's
// module script. The stub answers each command from the fixture's key of that name, else null.
// It exists only in the page written to a temp directory, never in dist or the shipped app.

export const stubSource = (fixture) => `window.__TAURI__ = (() => {
  const fixture = ${JSON.stringify(fixture)};
  const listeners = {};
  const listen = async (name, handler) => ((listeners[name] ??= []).push(handler), () => {});
  // The daemon answers a list request with an empty list, which the terminals' revival waits for.
  const invoke = async (command, args) => {
    if (command === "terminal_send" && args?.msg?.op === "list") setTimeout(() => (listeners.terminal ?? []).forEach((h) => h({ payload: { ev: "list", terms: [] } })));
    const reply = fixture[command];
    // A reply that depends on one argument: { __by: "id", values: { <id>: reply } }.
    if (reply?.__by) return reply.values[args?.[reply.__by]] ?? null;
    return reply ?? null;
  };
  return { event: { listen }, core: { invoke } };
})();`;

// What runs in the page's head, after the stub: the body's classes (set once the body exists), then the fixture's script once
// the UI has drawn (the page has loaded and its first replies have landed). The script is the body of an async function that
// gets `log`; what it logs reaches snap's output, and a line that starts `snap:fail` makes snap exit non-zero. A script that
// throws logs a `snap:fail` line.
export const headSource = ({ bodyClasses = [], script = null } = {}) => `
  addEventListener("DOMContentLoaded", () => document.body.classList.add(...${JSON.stringify(bodyClasses)}));${
    script === null ? "" : `
  addEventListener("load", () => setTimeout(async () => {
    const log = (...parts) => console.log(parts.join(" "));
    try {
      await (async (log) => {\n${script}\n})(log);
    } catch (error) {
      log("snap:fail the script threw: " + error);
    }
  }, 2500));`}`;

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

// Whether the run failed: a log line that starts `snap:fail`.
export const failed = (lines) => lines.some((line) => line.startsWith("snap:fail"));
