// Seam 3 (danielrispler/Falinks#461): a terminal's recorded bytes, replayed into a headless xterm of another size with the resize sent,
// render the same screen as a terminal started at that size. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, existsSync, rmSync } from "node:fs";
import { createConnection } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import xterm from "@xterm/headless";

const studio = new URL("../..", import.meta.url).pathname;

// Draws a screen that fits the window, and draws it again on every resize, as a full-screen program does.
const program = `seq 1 300; draw() { set -- $(stty size); printf '\\033[H\\033[2J'; i=1;
  while [ $i -lt $1 ]; do printf 'row %d of %d, %d cols\\r\\n' $i $1 $2; i=$((i+1)); done; printf 'bottom %d' $1; }
  trap draw WINCH; draw; while :; do sleep 0.05; done`;

function startDaemon() {
  execFileSync("cargo", ["build", "--quiet", "-p", "studio-terminal-daemon"], { cwd: studio, stdio: "inherit" });
  const bin = join(process.env.CARGO_TARGET_DIR ?? join(studio, "target"), "debug", "studio-terminal-daemon");
  const dir = mkdtempSync(join(tmpdir(), "stdr-"));
  const socket = join(dir, "d.sock");
  const child = spawn(bin, ["serve", "--socket", socket], { env: { ...process.env, HOME: dir, SHELL: "/bin/sh" } });
  return { socket, stop: () => (child.kill("SIGKILL"), rmSync(dir, { recursive: true, force: true })) };
}

async function connect(socket) {
  for (let i = 0; !existsSync(socket); i++) {
    if (i > 250) throw new Error("the daemon did not open its socket");
    await new Promise((r) => setTimeout(r, 20));
  }
  const sock = createConnection(socket);
  let pending = "";
  const waiters = [];
  const events = [];
  sock.on("data", (chunk) => {
    pending += chunk;
    let at;
    while ((at = pending.indexOf("\n")) >= 0) {
      events.push(JSON.parse(pending.slice(0, at)));
      pending = pending.slice(at + 1);
      waiters.splice(0).forEach((w) => w());
    }
  });
  const c = {
    send: (msg) => sock.write(JSON.stringify(msg) + "\n"),
    // Resolves with the next event that matches, dropping the ones before it.
    next: async (matches) => {
      for (;;) {
        const i = events.findIndex(matches);
        if (i >= 0) return events.splice(0, i + 1).pop();
        await new Promise((r) => waiters.push(r));
      }
    },
    close: () => sock.destroy(),
  };
  const hello = await c.next((e) => e.ev === "hello");
  assert.ok(hello.protocol >= 1);
  return c;
}

const write = (term, bytes) => new Promise((r) => term.write(bytes, r));

// Feeds the terminal's output into `term` until the screen shows `done`.
async function feedUntil(c, id, term, done) {
  while (!screen(term).includes(done)) {
    const ev = await c.next((e) => e.id === id && e.ev === "output");
    await write(term, Buffer.from(ev.data, "base64"));
  }
}

function screen(term) {
  const b = term.buffer.active;
  const rows = [];
  for (let y = b.baseY; y < b.baseY + term.rows; y++) rows.push(b.getLine(y).translateToString(true));
  return rows.join("\n");
}

test("a replay into another size, with the resize sent, matches a fresh render at that size", { timeout: 60_000 }, async (t) => {
  const d = startDaemon();
  t.after(d.stop);
  const c = await connect(d.socket);
  t.after(c.close);

  c.send({ op: "spawn", id: "small", argv: ["sh", "-c", program], cwd: "/", cols: 80, rows: 24 });
  c.send({ op: "attach", id: "small" });
  const small = new xterm.Terminal({ cols: 80, rows: 24, allowProposedApi: true });
  await write(small, Buffer.from((await c.next((e) => e.id === "small" && e.ev === "replay")).data, "base64"));
  await feedUntil(c, "small", small, "bottom 24");

  // The app reattaches in a bigger window: replay, then resize.
  const app = await connect(d.socket);
  t.after(app.close);
  const replayed = new xterm.Terminal({ cols: 120, rows: 40, allowProposedApi: true });
  app.send({ op: "attach", id: "small" });
  const replay = await app.next((e) => e.id === "small" && e.ev === "replay");
  await write(replayed, Buffer.from(replay.data, "base64"));
  app.send({ op: "resize", id: "small", cols: 120, rows: 40 });
  await feedUntil(app, "small", replayed, "bottom 40");

  c.send({ op: "spawn", id: "fresh", argv: ["sh", "-c", program], cwd: "/", cols: 120, rows: 40 });
  c.send({ op: "attach", id: "fresh" });
  const fresh = new xterm.Terminal({ cols: 120, rows: 40, allowProposedApi: true });
  await write(fresh, Buffer.from((await c.next((e) => e.id === "fresh" && e.ev === "replay")).data, "base64"));
  await feedUntil(c, "fresh", fresh, "bottom 40");

  assert.equal(screen(replayed), screen(fresh));
  assert.match(screen(fresh), /row 39 of 40, 120 cols/);
});
