// The npm package's launcher (danielrispler/plotkeeper#706): `npm/launcher.js` runs the real `plotkeeper` from the
// platform package npm installed beside it, and passes its arguments, standard streams, exit code and signals
// through. Each test lays out a package as npm installs it, with a stand-in binary, and runs the launcher as a process.
// Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const platform = `${process.platform}-${process.arch}`;
// The platform package npm installs on this machine.
const platformPackage = process.platform === "darwin" ? "plotkeeper-darwin" : `plotkeeper-${platform}`;

// A stand-in for the real binary. `echo` prints its arguments, then its stdin, writes to stderr and exits with
// $STAND_IN_CODE; `wait` says it is ready, then answers SIGINT with exit 0 and SIGTERM with exit 7, each said on
// stdout; `die` kills itself with SIGHUP.
const standIn = `#!${process.execPath}
const [mode, ...args] = process.argv.slice(2);
if (mode === "echo") {
  const input = require("node:fs").readFileSync(0, "utf8");
  process.stdout.write(JSON.stringify(args) + "\\n" + input);
  process.stderr.write("to stderr\\n");
  process.exit(Number(process.env.STAND_IN_CODE));
} else if (mode === "wait") {
  process.on("SIGINT", () => { process.stdout.write("got SIGINT\\n"); process.exit(0); });
  process.on("SIGTERM", () => { process.stdout.write("got SIGTERM\\n"); process.exit(7); });
  // Gives up after 10s, so a stand-in no signal reached does not outlive the test.
  setTimeout(() => process.exit(9), 10000);
  process.stdout.write("ready\\n");
} else if (mode === "die") {
  process.kill(process.pid, "SIGHUP");
}
`;

// The launcher package in a throwaway folder, with this machine's platform package under its node_modules unless
// `withPlatform` is false. Returns the launcher's path.
function installed(t, { withPlatform = true } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "plotkeeper-launcher-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const launcher = join(dir, "plotkeeper", "launcher.js");
  mkdirSync(dirname(launcher));
  copyFileSync(resolve(root, "npm/launcher.js"), launcher);
  copyFileSync(resolve(root, "npm/package.json"), join(dir, "plotkeeper", "package.json"));
  if (withPlatform) {
    const pkg = join(dir, "plotkeeper", "node_modules", platformPackage);
    mkdirSync(join(pkg, "bin"), { recursive: true });
    writeFileSync(join(pkg, "package.json"), JSON.stringify({ name: platformPackage, version: "0.0.0" }));
    writeFileSync(join(pkg, "bin", "plotkeeper"), standIn);
    chmodSync(join(pkg, "bin", "plotkeeper"), 0o755);
  }
  return launcher;
}

// Starts the launcher with `mode`, waits for the stand-in's "ready", sends `signal` to the launcher alone, and
// resolves with the launcher's stdout, exit code and signal.
function signalled(launcher, mode, signal) {
  return new Promise((done, failed) => {
    const child = spawn(process.execPath, [launcher, mode], { stdio: ["ignore", "pipe", "inherit"] });
    let stdout = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
      if (stdout === "ready\n") child.kill(signal);
    });
    child.on("error", failed);
    child.on("exit", (code, exitSignal) => done({ stdout, code, signal: exitSignal }));
  });
}

test("the launcher passes arguments, stdin, stdout, stderr and the exit code through", (t) => {
  // @law: launcher-passes-everything-through
  const launcher = installed(t);
  const run = spawnSync(process.execPath, [launcher, "echo", "a b", "--yes", ""], {
    input: "from stdin\n",
    encoding: "utf8",
    env: { ...process.env, STAND_IN_CODE: "3" },
  });
  assert.equal(run.stdout, `${JSON.stringify(["a b", "--yes", ""])}\nfrom stdin\n`);
  assert.equal(run.stderr, "to stderr\n");
  assert.equal(run.status, 3);
});

test("the launcher passes SIGINT to plotkeeper and exits as plotkeeper does", async (t) => {
  // @law: launcher-passes-everything-through
  const result = await signalled(installed(t), "wait", "SIGINT");
  assert.deepEqual(result, { stdout: "ready\ngot SIGINT\n", code: 0, signal: null });
});

test("the launcher passes SIGTERM to plotkeeper and exits as plotkeeper does", async (t) => {
  // @law: launcher-passes-everything-through
  const result = await signalled(installed(t), "wait", "SIGTERM");
  assert.deepEqual(result, { stdout: "ready\ngot SIGTERM\n", code: 7, signal: null });
});

test("a plotkeeper killed by a signal leaves the launcher killed by the same signal", (t) => {
  const run = spawnSync(process.execPath, [installed(t), "die"], { encoding: "utf8" });
  assert.equal(run.signal, "SIGHUP");
});

test("a missing platform package fails with a message naming the platform and the package", (t) => {
  const run = spawnSync(process.execPath, [installed(t, { withPlatform: false }), "--version"], { encoding: "utf8" });
  assert.equal(run.status, 1);
  assert.equal(run.stdout, "");
  assert.match(run.stderr, new RegExp(`plotkeeper: no plotkeeper for ${platform}: the package ${platformPackage}`));
  assert.match(run.stderr, /--omit=optional/);
});
