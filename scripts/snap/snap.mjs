// `npm run snap -- <fixture> <out.png>`: renders the studio UI from dist/ against a stub Tauri API
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

// A fixture with `realPointer` runs Chrome with its DevTools port open, and the page's `__snapClick` asks here for a real mouse click,
// sent as Chrome's own input (`Input.dispatchMouseEvent`): what `element.click()` skips, such as pointer capture, then happens as in use.
async function realClick(x, y) {
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
    socket.send(JSON.stringify({ id, method: "Input.dispatchMouseEvent", params: { x, y, ...params } }));
  });
  await send({ type: "mouseMoved" });
  await send({ type: "mousePressed", button: "left", buttons: 1, clickCount: 1 });
  await send({ type: "mouseReleased", button: "left", buttons: 0, clickCount: 1 });
  socket.close();
}

const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css" };
const server = createServer(async (req, res) => {
  const url = new URL(req.url, "http://x");
  if (url.pathname === "/__snap/click") {
    try {
      if (!fixture.realPointer) throw new Error("the fixture has no realPointer");
      await realClick(Number(url.searchParams.get("x")), Number(url.searchParams.get("y")));
      res.end("clicked");
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
  "--virtual-time-budget=5000", `--screenshot=${resolve(out)}`, ...(fixture.realPointer ? ["--remote-debugging-port=0"] : []),
  `http://127.0.0.1:${server.address().port}/index.html`,
];
// A screenshot left from an earlier run must not pass for this one's.
rmSync(resolve(out), { force: true });
const run = await new Promise((done) => {
  // Chrome leads its own process group, so ending it ends its helper processes too, and none is left holding stderr open.
  const child = spawn(chrome, chromeArgs, { stdio: ["ignore", "ignore", "pipe"], detached: true });
  const end = () => {
    try {
      process.kill(-child.pid, "SIGKILL");
    } catch {}
  };
  let stderr = "";
  let shot = false;
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    end();
  }, chromeTimeout);
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
    // Chrome is ended once the screenshot is written: the page's console lines came before it, and its teardown is of no use to a
    // throwaway profile. On a starved CPU that teardown can outlast Chrome's own watchdog, which then exits 2 (#413); with its
    // DevTools port open, Chrome would not end at all.
    if (!shot && /bytes written to file/.test(stderr)) {
      shot = true;
      end();
    }
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
