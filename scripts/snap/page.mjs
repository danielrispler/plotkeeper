// The page `npm run snap` renders: dist's index.html with the page's transport stubbed in front of the app's module script: `fetch`
// answers `POST /api/<command>` from the fixture's key of that name, else null, and `EventSource` takes the page's listeners, which
// `window.__snapEmit(name, payload)` and the fixture's `appErrors` raise as the server's stream would; `window.__snapClick(node)`, `__snapDoubleClick(node)` and
// `__snapDrag(node, dx, dy)` are real mouse input, which snap sends (`realPointer`), `__snapKey(key)` a real key press (`realKeys`), and the fixture's `storage` is put in localStorage first.
// It exists only in the page written to a temp directory, never in dist or the shipped page.

import { hoverAudit } from "./hover.mjs";

export const stubSource = (fixture) => `(() => {
  const fixture = ${JSON.stringify(fixture)};
  // A fixture with \`realPointer\` clicks as a person does: \`await __snapClick(node)\` asks snap to send Chrome a real mouse press and
  // release at the node's middle (\`Input.dispatchMouseEvent\`), so pointer capture and the click's target are the browser's own.
  const serverFetch = window.fetch?.bind(window);
  const real = async (kind, query) => {
    const reply = await serverFetch("/__snap/" + kind + "?" + new URLSearchParams(query));
    if (!reply.ok) throw new Error("no real " + kind + ": " + (await reply.text()));
  };
  const middle = (node) => {
    const box = node.getBoundingClientRect();
    return { x: box.left + box.width / 2, y: box.top + box.height / 2 };
  };
  window.__snapClick = (node) => real("click", middle(node));
  window.__snapDoubleClick = (node) => real("dblclick", middle(node));
  // A fixture with \`realKeys\` presses keys as a person does: \`await __snapKey("Tab")\` asks snap to send Chrome a real key down and up
  // (\`Input.dispatchKeyEvent\`), so Tab moves focus and Esc closes a native popover. The keys: Tab, Shift+Tab, Enter, Space, Escape,
  // ArrowUp and ArrowDown.
  window.__snapKey = (key) => real("key", { key });
  // \`await __snapDrag(node, dx, dy)\` presses at the node's middle, moves by (dx, dy) with the button held and lets go there. With
  // \`midway\`, an async function, the button stays held halfway while it runs, then the rest of the move and the release follow.
  window.__snapDrag = async (node, dx, dy, midway) => {
    const { x, y } = middle(node);
    const query = { x, y, toX: x + dx, toY: y + dy };
    if (!midway) return real("drag", query);
    await real("drag-start", query);
    await midway();
    return real("drag-end", query);
  };
  // A fixture's \`storage\` is what this browser had kept from an earlier visit, written before the app starts.
  for (const [key, value] of Object.entries(fixture.storage ?? {})) localStorage.setItem(key, value);
  const listeners = {};
  const fire = (name, payload) => (listeners[name] ?? []).forEach((h) => h({ data: JSON.stringify(payload) }));
  // The app's errors are the fixture's appErrors key (none by default), sent on \`log://log\` as the page's log target sends them.
  window.EventSource = class {
    addEventListener(name, handler) {
      (listeners[name] ??= []).push(handler);
      if (name === "log://log") for (const message of fixture.appErrors ?? []) setTimeout(() => handler({ data: JSON.stringify({ level: 5, message }) }));
    }
    removeEventListener(name, handler) {
      listeners[name] = (listeners[name] ?? []).filter((h) => h !== handler);
    }
    close() {}
  };
  const invoke = async (command, args) => {
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
  // The page's own log lines (\`/api/log\`) are not echoed back; only an app error reaches the page, on \`log://log\`.
  window.fetch = async (url, init) => {
    const command = /\\/api\\/(\\w+)$/.exec(String(url))?.[1];
    if (!command) return { ok: false, status: 404, json: async () => "no such command" };
    try {
      const reply = command === "log" ? null : await invoke(command, init?.body ? JSON.parse(init.body) : undefined);
      return { ok: true, status: 200, json: async () => reply };
    } catch (why) {
      return { ok: false, status: 500, json: async () => String(why) };
    }
  };
  // A fixture's script raises a server event with __snapEmit, as the stream would.
  window.__snapEmit = fire;
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
