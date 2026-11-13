// The terminal panel (danielrispler/Falinks#542), run on the `tsc` output. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { isPanel, panelShell } from "../../dist/panel.js";

const repo = "/Users/daniel/work/Falinks";
const a = { cwd: `${repo}/.claude/worktrees/issue-542` };
const b = { cwd: `${repo}/.claude/worktrees/issue-529` };

test("selecting a session in another folder shows that folder's shell, and returning shows the first one", () => {
  const first = panelShell(a, repo);
  assert.deepEqual(first, { id: `panel-${a.cwd}`, cwd: a.cwd });
  assert.deepEqual(panelShell(b, repo), { id: `panel-${b.cwd}`, cwd: b.cwd });
  // The same terminal of the daemon, so its scrollback is intact.
  assert.deepEqual(panelShell(a, repo), first);
  // Two sessions in one folder share its shell.
  assert.deepEqual(panelShell({ cwd: a.cwd }, repo), first);
});

test("with nothing selected, the panel's shell runs in the configured repository", () => {
  assert.deepEqual(panelShell(null, repo), { id: `panel-${repo}`, cwd: repo });
});

test("the panel's shells are told apart from the panes' terminals, so revival keeps them in the panel", () => {
  assert.equal(isPanel(panelShell(a, repo).id), true);
  for (const id of ["shell", "claude-1b2c", "elsewhere"]) assert.equal(isPanel(id), false);
});

test("the panel is a band under the terminal column, under a split's panes, never beside the list or the rail", () => {
  const html = fs.readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const column = html.slice(html.indexOf('<section id="terminal"'), html.indexOf("</section>", html.indexOf('<section id="terminal"')));
  assert.ok(column.indexOf('id="panel"') > column.indexOf('id="terminal-body"'));
});
