#!/usr/bin/env node
// `node scripts/check/slot.mjs <command...>`: what `npm run check` runs (#783, spec #778). It takes one of two check slots
// shared by every worktree and clone on the machine, then runs the command, today's `npm run check:steps`, as a child in
// a process group of its own, passing its output and exit code through.
//
// A slot is a file named `1` or `2` in `check-slots/`, beside the runs folder in plotkeeper's per-user cache folder. A
// check writes its pid, worktree and start time to a temporary file there and hard-links it to a slot's name: the link
// fails when the name exists, so exactly one check wins a slot, and a slot file is never seen half written.
//
// The tests set the folder and the timing limits through `CHECK_SLOT_DIR`, `CHECK_SLOT_POLL_MS`, `CHECK_SLOT_TOUCH_MS`
// and `CHECK_SLOT_STALE_MS`; they are not a setting.
import { spawn } from "node:child_process";
import { closeSync, fstatSync, futimesSync, linkSync, mkdirSync, openSync, readFileSync, rmdirSync, statSync, unlinkSync, writeSync } from "node:fs";
import { constants, homedir } from "node:os";
import { join } from "node:path";

const SLOTS = ["1", "2"];
const env = process.env;
const POLL_MS = Number(env.CHECK_SLOT_POLL_MS ?? 1000);
const STALE_MS = Number(env.CHECK_SLOT_STALE_MS ?? 60_000);
const TOUCH_MS = Number(env.CHECK_SLOT_TOUCH_MS ?? 10_000);

/** `check-slots/` in plotkeeper's per-user cache folder (`docs/studio/behaviour/storage.md`). */
const slotDir = () => {
  if (env.CHECK_SLOT_DIR) return env.CHECK_SLOT_DIR;
  if (process.platform === "darwin") return join(homedir(), "Library/Caches/plotkeeper/check-slots");
  const xdg = env.XDG_CACHE_HOME;
  return join(xdg?.startsWith("/") ? xdg : join(homedir(), ".cache"), "plotkeeper/check-slots");
};

const dir = slotDir();
const [command, ...args] = process.argv.slice(2);

/** Writes this check's slot file under a temporary name and keeps it open, so it can be told apart from a later one. */
const prepare = () => {
  mkdirSync(dir, { recursive: true });
  const tmp = join(dir, `.${process.pid}.${Date.now()}.tmp`);
  const fd = openSync(tmp, "wx");
  writeSync(fd, JSON.stringify({ pid: process.pid, worktree: process.cwd(), since: Date.now() }));
  return { tmp, fd };
};

/** Links the slot file to the first free slot's name; the slot's path, or null when both are held. */
const take = (tmp) => {
  for (const slot of SLOTS) {
    try {
      linkSync(tmp, join(dir, slot));
      return join(dir, slot);
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
    }
  }
  return null;
};

/** Whether a process has this pid, asked without starting one. */
const alive = (pid) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === "EPERM";
  }
};

/**
 * A slot is stale when no process has its holder's pid, or when its file was not touched within the limit: a reused pid,
 * or a wrapper killed with SIGKILL whose pid lives on in something else.
 */
const stale = (path) => {
  if (Date.now() - statSync(path).mtimeMs > STALE_MS) return true;
  const holder = JSON.parse(readFileSync(path, "utf8"));
  return !(Number.isInteger(holder.pid) && alive(holder.pid));
};

/**
 * Removes a stale slot's file. Under `<slot>.reap`, a folder only one waiter can make, so two waiters that both saw it
 * stale cannot remove the file the first one then linked; a reap folder left by a killed waiter is removed when it is old.
 */
const reap = (slot) => {
  const path = join(dir, slot);
  const lock = `${path}.reap`;
  try {
    mkdirSync(lock);
  } catch {
    try {
      if (Date.now() - statSync(lock).mtimeMs > STALE_MS) rmdirSync(lock);
    } catch {}
    return;
  }
  try {
    if (stale(path)) unlinkSync(path);
  } catch {
  } finally {
    rmdirSync(lock);
  }
};

/** What each slot's file says, for the slots that hold one. */
const holders = () =>
  SLOTS.flatMap((slot) => {
    try {
      return [JSON.parse(readFileSync(join(dir, slot), "utf8"))];
    } catch {
      return [];
    }
  });

const age = (ms) => {
  const seconds = Math.max(0, Math.round(ms / 1000));
  return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
};

const waitingLine = () => {
  const held = holders().map((holder) => `${holder.worktree} (${age(Date.now() - holder.since)})`);
  return `check: waiting for a check slot, held by ${held.join(" and ")}\n`;
};

/** Removes the slot's name if it still names this check's file. */
const release = (mine) => {
  try {
    if (statSync(mine.slot).ino === fstatSync(mine.fd).ino) unlinkSync(mine.slot);
  } catch {}
  closeSync(mine.fd);
};

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// The slot file while this check waits for a slot, then the check's child once it runs.
let waiting = null;
let child = null;

/** Removes the slot file of a check that waits and holds no slot. */
const dropWaiting = () => {
  if (!waiting) return;
  try {
    unlinkSync(waiting.tmp);
    closeSync(waiting.fd);
  } catch {}
  waiting = null;
};

// A stop passes on to the child's process group; the child's end gives the slot back. A stop while waiting ends the
// check before it runs anything.
for (const signal of ["SIGINT", "SIGTERM"]) {
  process.on(signal, () => {
    if (child) {
      try {
        process.kill(-child.pid, signal);
      } catch {}
      return;
    }
    dropWaiting();
    process.exit(128 + constants.signals[signal]);
  });
}

const withoutSlot = (error) => {
  process.stderr.write(`check: the check slot folder ${dir} can't be written (${error.code}), so this check runs without a slot\n`);
  dropWaiting();
  return null;
};

/** Takes a free slot, or one taken back from a dead holder; null when both are held. */
const takeOrReap = (tmp) => {
  const slot = take(tmp);
  if (slot) return slot;
  for (const name of SLOTS) reap(name);
  return take(tmp);
};

/** Waits for a slot and takes it; null when the slot folder can't be written (a sandboxed run). */
const hold = async () => {
  let mine;
  try {
    mine = prepare();
  } catch (error) {
    return withoutSlot(error);
  }
  waiting = mine;
  let told = false;
  for (;;) {
    let slot;
    try {
      slot = takeOrReap(mine.tmp);
    } catch (error) {
      return withoutSlot(error);
    }
    if (slot) {
      unlinkSync(mine.tmp);
      waiting = null;
      return { ...mine, slot };
    }
    if (!told) process.stderr.write(waitingLine());
    told = true;
    await sleep(POLL_MS);
  }
};

const run = (mine) => {
  // Through the open file, so a slot that was taken back and is now another check's file is never touched.
  if (mine) setInterval(() => futimesSync(mine.fd, new Date(), new Date()), TOUCH_MS).unref();
  // No standard input: in a terminal the child's group is not the foreground one, and a read of the terminal from it, such
  // as nextest's key handler, would stop it with SIGTTIN.
  child = spawn(command, args, { stdio: ["ignore", "inherit", "inherit"], detached: true });
  child.on("close", (code, signal) => {
    if (mine) release(mine);
    process.exit(code ?? 128 + constants.signals[signal]);
  });
};

run(await hold());
