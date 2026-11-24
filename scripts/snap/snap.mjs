// `npm run snap -- <fixture> <out.png>`: renders the studio UI from dist/ against a stubbed server (scripts/snap/page.mjs)
// fed by a fixture (scripts/snap/fixtures.ts) to a PNG, with headless Chrome. Run `npm run build` first.
// Chrome never opens a window, so it takes no focus. Set CHROME to a Chrome binary to override the default.
import { spawn } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { fixtures } from "./fixtures.ts";
import { chromeFailure, consoleLines, failed, snapPage } from "./page.mjs";

const [name, out] = process.argv.slice(2);
// Optional, from the environment: SNAP_SIZE=1728x1117 is the window's size (default 1900x900), and SNAP_BODY=rail-open,list-open
// are classes put on <body>, such as the rail's overlay open. A fixture's `snapScript` runs once the UI has drawn: what it logs
// is printed, and a line that starts `snap:fail`, or an uncaught error in the page, makes this exit non-zero.
const [width, height] = (process.env.SNAP_SIZE ?? "1900x900").split("x");
// SNAP_SCALE=2 draws at twice the pixel density (the PNG is twice as wide).
const scale = process.env.SNAP_SCALE ?? "1";
// SNAP_CHROME_TIMEOUT_MS (default 90 s) is how long Chrome may run: past it Chrome is killed and the fixture fails, so a Chrome that
// never writes its screenshot (or, with its DevTools port open, never ends) cannot hang a run until CI's own limit.
const chromeTimeout = Number(process.env.SNAP_CHROME_TIMEOUT_MS ?? 90000);
const bodyClasses = (process.env.SNAP_BODY ?? "").split(",").filter(Boolean);
if (!fixtures[name] || !out) {
  console.error(`usage: npm run snap -- <${Object.keys(fixtures).join("|")}> <out.png>`);
  process.exit(2);
}
const dist = resolve(dirname(fileURLToPath(import.meta.url)), "../../dist");
const chrome = process.env.CHROME ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

// A copy of dist/ with the stubbed index, so the stub never reaches dist.
const site = mkdtempSync(join(tmpdir(), "snap-site-"));
const profile = mkdtempSync(join(tmpdir(), "snap-chrome-"));
cpSync(dist, site, { recursive: true });
const fixture = fixtures[name]();
writeFileSync(join(site, "index.html"), snapPage(readFileSync(join(dist, "index.html"), "utf8"), fixture, { bodyClasses }));

// A fixture with `realPointer` or `realKeys` runs Chrome with its DevTools port open, and the page's `__snapClick`, `__snapDrag`,
// `__snapDoubleClick` and `__snapKey` ask here for real input, sent as Chrome's own (`Input.dispatchMouseEvent`, `Input.dispatchKeyEvent`):
// what `element.click()` or a synthetic KeyboardEvent skips, such as pointer capture, Tab moving focus or Esc closing a popover, then
// happens as in use. `events` are that method's params in order.
async function devtools(method, events) {
  const port = readFileSync(join(profile, "DevToolsActivePort"), "utf8").split("\n")[0];
  const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const socket = new WebSocket(targets.find((target) => target.type === "page").webSocketDebuggerUrl);
  await new Promise((open, fail) => ((socket.onopen = open), (socket.onerror = fail)));
  let id = 0;
  const waiting = new Map();
  socket.onmessage = (message) => {
    const reply = JSON.parse(message.data);
    waiting.get(reply.id)?.(reply);
  };
  const send = (params) => new Promise((done) => {
    waiting.set(++id, done);
    socket.send(JSON.stringify({ id, method, params }));
  });
  for (const event of events) await send(event);
  socket.close();
}
const realMouse = (events) => devtools("Input.dispatchMouseEvent", events);
// The keys a fixture may press: each a key down and up, with `text` for the keys that type (Enter, Space) so Chrome presses a focused
// button with them as it does in use. Shift+Tab holds Shift (modifier 8) over the Tab.
const keys = {
  Tab: { key: "Tab", code: "Tab", windowsVirtualKeyCode: 9 },
  "Shift+Tab": { key: "Tab", code: "Tab", windowsVirtualKeyCode: 9, modifiers: 8 },
  Enter: { key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" },
  Space: { key: " ", code: "Space", windowsVirtualKeyCode: 32, text: " " },
  Escape: { key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 },
  ArrowUp: { key: "ArrowUp", code: "ArrowUp", windowsVirtualKeyCode: 38 },
  ArrowDown: { key: "ArrowDown", code: "ArrowDown", windowsVirtualKeyCode: 40 },
};
const realKey = (name) => {
  const key = keys[name];
  if (!key) throw new Error(`no such key: ${name}; snap presses ${Object.keys(keys).join(", ")}`);
  const { text, ...rest } = key;
  return devtools("Input.dispatchKeyEvent", [{ type: text ? "keyDown" : "rawKeyDown", ...key }, { type: "keyUp", ...rest }]);
};
const press = (x, y, clickCount = 1) => [
  { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount },
  { type: "mouseReleased", x, y, button: "left", buttons: 0, clickCount },
];
const realClick = (x, y) => realMouse([{ type: "mouseMoved", x, y }, ...press(x, y)]);
const realDoubleClick = (x, y) => realMouse([{ type: "mouseMoved", x, y }, ...press(x, y, 1), ...press(x, y, 2)]);
// A press at (x, y), a move to (toX, toY) in eight steps with the button held, and the release there; `realDragStart` is the press and
// the first four steps, `realDragEnd` the last four and the release, so the page can act with the button held halfway.
const steps = (x, y, toX, toY, from, to) =>
  Array.from({ length: to - from }, (_, i) => ({ type: "mouseMoved", x: x + ((toX - x) * (from + i + 1)) / 8, y: y + ((toY - y) * (from + i + 1)) / 8, button: "left", buttons: 1 }));
const dragStart = (x, y, toX, toY) => [
  { type: "mouseMoved", x, y },
  { type: "mousePressed", x, y, button: "left", buttons: 1, clickCount: 1 },
  ...steps(x, y, toX, toY, 0, 4),
];
const dragEnd = (x, y, toX, toY) => [...steps(x, y, toX, toY, 4, 8), { type: "mouseReleased", x: toX, y: toY, button: "left", buttons: 0, clickCount: 1 }];
const realDrag = (x, y, toX, toY) => realMouse([...dragStart(x, y, toX, toY), ...dragEnd(x, y, toX, toY)]);
const realDragStart = (x, y, toX, toY) => realMouse(dragStart(x, y, toX, toY));
const realDragEnd = (x, y, toX, toY) => realMouse(dragEnd(x, y, toX, toY));

const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css" };
const server = createServer(async (req, res) => {
  const url = new URL(req.url, "http://x");
  const input = { "/__snap/click": realClick, "/__snap/dblclick": realDoubleClick, "/__snap/drag": realDrag, "/__snap/drag-start": realDragStart, "/__snap/drag-end": realDragEnd }[url.pathname];
  if (input || url.pathname === "/__snap/key") {
    try {
      if (input && !fixture.realPointer) throw new Error("the fixture has no realPointer");
      if (!input && !fixture.realKeys) throw new Error("the fixture has no realKeys");
      if (input) await input(...["x", "y", "toX", "toY"].map((key) => Number(url.searchParams.get(key))));
      else await realKey(url.searchParams.get("key"));
      res.end("done");
    } catch (why) {
      res.statusCode = 500;
      res.end(String(why));
    }
    return;
  }
  try {
    const file = join(site, new URL(req.url, "http://x").pathname.replace(/^\/$/, "/index.html"));
    if (!file.startsWith(site)) throw new Error("outside");
    res.setHeader("content-type", types[extname(file)] ?? "application/octet-stream");
    res.end(readFileSync(file));
  } catch {
    res.statusCode = 404;
    res.end();
  }
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const chromeArgs = [
  "--headless=new", "--enable-logging=stderr", "--v=0", "--disable-gpu", `--force-device-scale-factor=${scale}`, `--user-data-dir=${profile}`, `--window-size=${width},${height}`,
  "--virtual-time-budget=5000", `--screenshot=${resolve(out)}`, ...(fixture.realPointer || fixture.realKeys ? ["--remote-debugging-port=0"] : []),
  `http://127.0.0.1:${server.address().port}/index.html`,
];
// A screenshot left from an earlier run must not pass for this one's.
rmSync(resolve(out), { force: true });
const run = await new Promise((done) => {
  // Chrome leads its own process group, so killing the group ends its helper processes too, and none is left holding stderr open.
  // Being out of the terminal's group, it misses a Ctrl+C, so snap passes that on.
  const child = spawn(chrome, chromeArgs, { stdio: ["ignore", "ignore", "pipe"], detached: true });
  const killGroup = () => {
    try {
      process.kill(-child.pid, "SIGKILL");
    } catch {
      // The group has already ended.
    }
  };
  process.once("SIGINT", () => {
    killGroup();
    process.exit(130);
  });
  let stderr = "";
  let shot = false;
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    killGroup();
  }, chromeTimeout);
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
    // Chrome is ended once the screenshot is written: the page's console lines came before it, and its teardown is of no use to a
    // throwaway profile. On a starved CPU that teardown can outlast Chrome's own watchdog, which then exits 2 (#413); with its
    // DevTools port open, Chrome would not end at all.
    if (!shot && /bytes written to file/.test(stderr)) {
      shot = true;
      killGroup();
    }
  });
  // A process forked just as the group was killed can outlive the kill and hold stderr open, so the group is killed again once
  // Chrome itself has ended.
  child.on("exit", () => {
    if (shot || timedOut) killGroup();
  });
  child.on("close", (status) => {
    clearTimeout(timer);
    done({ status: shot ? 0 : status, stderr, timedOut });
  });
});
server.close();
rmSync(site, { recursive: true, force: true });
rmSync(profile, { recursive: true, force: true });
const log = consoleLines(run.stderr ?? "");
if (log.length) console.log(log.join("\n"));
if (run.timedOut) {
  console.error(`snap:fail Chrome ran past snap's time limit (${chromeTimeout} ms) and was killed`);
  console.error(chromeFailure(run.status ?? "killed", run.stderr ?? ""));
  process.exit(1);
}
if (run.status !== 0) {
  console.error(chromeFailure(run.status, run.stderr ?? ""));
  process.exit(run.status ?? 1);
}
if (failed(log, fixture.snapDone)) {
  if (!log.some((line) => line.startsWith("snap:fail"))) console.error(`snap:fail the script never printed "${fixture.snapDone}": Chrome's time budget cut it off`);
  process.exit(1);
}
if (!existsSync(resolve(out))) {
  console.error(`snap:fail Chrome wrote no screenshot to ${out}`);
  process.exit(1);
}
console.log(`wrote ${out}`);
