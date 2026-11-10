// A program that fails at start (#558): its code, its last lines and a notice. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import xterm from "@xterm/headless";
import fs from "node:fs";
import { execFileSync } from "node:child_process";
import { doctorArgv, failureTakesPane } from "../../dist/panes.js";
import { FAILED_OSC, startFailure, startNotice } from "../../dist/terminal.js";

const read = (name) => fs.readFileSync(new URL(`../${name}`, import.meta.url), "utf8");

// A fake terminal: a headless xterm that the program's output was written to.
function screen(output) {
  const term = new xterm.Terminal({ cols: 80, rows: 24, allowProposedApi: true });
  return new Promise((done) => term.write(output, () => done(term.buffer.active)));
}

const output = "Welcome\r\nloading\r\n\r\nzsh: command not found: claude\r\n";

test("an exit with code 127 after 0.5 s shows the code, the last lines and a notice", async () => {
  const failure = startFailure("code 127", 500, await screen(output));
  assert.deepEqual(failure, { status: "code 127", lines: ["Welcome", "loading", "", "zsh: command not found: claude"] });
  assert.equal(startNotice("Falinks", failure), "Falinks could not start: code 127");
});

test("only the last lines show, and a signal at start counts", async () => {
  const many = Array.from({ length: 30 }, (_, i) => `line ${i}`).join("\r\n");
  assert.deepEqual(startFailure("signal 9", 100, await screen(many)).lines, ["line 25", "line 26", "line 27", "line 28", "line 29"]);
});

test("the notice is also a toast with Show output, beside the needs-you toast, never focused", () => {
  assert.match(read("index.html"), /<div id="start-toast" class="toast start" role="status" hidden><\/div>/);
  const toast = read("toast.ts");
  assert.match(toast, /"Show output"/);
  assert.match(toast, /tabIndex = -1/);
});

test("a second failure stacks under the first, never replaces it", () => {
  const start = read("toast.ts").slice(read("toast.ts").indexOf("export function showStartToast"));
  const body = start.slice(0, start.indexOf("\n}\n"));
  assert.match(body, /toast\.append\(\.\.\.row\)/);
  assert.doesNotMatch(body, /replaceChildren/);
});

test("a panel shell's Show output opens the panel, not the main column", () => {
  assert.match(read("panes.ts"), /isPanel\(id\) \? showPanelShell\(id, focusPane\) : show\(id, \(\) => null\)/);
});

test("a failing Doctor command reports its own code, and its shell stays", () => {
  const out = execFileSync("/bin/sh", ["-c", doctorArgv("(exit 3)")[2]], { env: { ...process.env, SHELL: "/usr/bin/true" } }).toString();
  assert.equal(out, `\x1b]${FAILED_OSC};3\x07`);
  const ok = execFileSync("/bin/sh", ["-c", doctorArgv("true")[2]], { env: { ...process.env, SHELL: "/usr/bin/true" } }).toString();
  assert.equal(ok, "");
  // Its notice is named after the entry's label, not "sh".
  assert.match(read("panes.ts"), /labels\.get\(id\) \?\? name/);
});

test("a failed pane takes the terminal's place only when it is the shown one; a hidden one only toasts", () => {
  assert.equal(failureTakesPane("claude-1", "claude-1"), true);
  assert.equal(failureTakesPane("claude-1", "shell"), false);
});

test("an exit with code 1 after 5 s, and an exit with code 0 after 0.5 s, show no notice", async () => {
  const buffer = await screen(output);
  assert.equal(startFailure("code 1", 5000, buffer), null);
  assert.equal(startFailure("code 0", 500, buffer), null);
});
