#!/usr/bin/env node
// The `plotkeeper` command of the npm package (danielrispler/plotkeeper#706). The binaries are in one package per
// platform, each an optional dependency marked with its `os` and `cpu`, so npm installs only this machine's. This runs
// that package's `plotkeeper`, whose hook binaries and run launcher sit beside it in its `bin/`, and passes the
// arguments, the standard streams, the exit code and the signals through.
const { spawn } = require("node:child_process");
const { dirname, join } = require("node:path");

const PACKAGES = {
  "darwin-arm64": "plotkeeper-darwin",
  "darwin-x64": "plotkeeper-darwin",
  "linux-x64": "plotkeeper-linux-x64",
  "linux-arm64": "plotkeeper-linux-arm64",
};
// The signals a terminal or a process manager sends to stop plotkeeper, all of which it stops cleanly on.
const SIGNALS = ["SIGINT", "SIGTERM", "SIGHUP"];

function fail(message) {
  process.stderr.write(`plotkeeper: ${message}\n`);
  process.exit(1);
}

const platform = `${process.platform}-${process.arch}`;
const name = PACKAGES[platform];
if (name === undefined) fail(`no plotkeeper for ${platform}: it runs on macOS, and on Linux on x64 and arm64`);
let binary;
try {
  binary = join(dirname(require.resolve(`${name}/package.json`)), "bin", "plotkeeper");
} catch {
  fail(
    `no plotkeeper for ${platform}: the package ${name}, which holds it, is not installed. npm leaves optional ` +
      "dependencies out under --omit=optional or --no-optional; install plotkeeper again without them",
  );
}

// `inherit` hands plotkeeper the launcher's own stdin, stdout and stderr, so it sees the same terminal.
const child = spawn(binary, process.argv.slice(2), { stdio: "inherit" });
// A Ctrl+C in a terminal reaches both processes; a signal sent to the launcher alone reaches plotkeeper only through
// this. plotkeeper stops once however many it gets.
const forward = (signal) => () => child.kill(signal);
const forwarders = SIGNALS.map((signal) => [signal, forward(signal)]);
for (const [signal, listener] of forwarders) process.on(signal, listener);
child.on("error", (error) => fail(`could not start ${binary}: ${error.message}`));
child.on("exit", (code, signal) => {
  if (signal === null) process.exit(code ?? 1);
  // Killed by a signal: the launcher ends by the same one, so its parent sees what happened.
  for (const [s, listener] of forwarders) process.off(s, listener);
  process.kill(process.pid, signal);
});
