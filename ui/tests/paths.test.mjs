// File paths in the terminal pane (#466): which text is a clickable path, and how a dropped file's
// path is typed. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { pathLinks, quotePath, worktreeOf } from "../../dist/paths.js";

test("paths with a folder are links, with a line number when one follows", () => {
  assert.deepEqual(pathLinks("error at apps/studio/ui/keys.ts:12:3 here"), [
    { text: "apps/studio/ui/keys.ts:12:3", start: 9 },
  ]);
  assert.deepEqual(pathLinks("see /Users/d/x.rs and ./a/b"), [
    { text: "/Users/d/x.rs", start: 4 },
    { text: "./a/b", start: 22 },
  ]);
});

test("URLs, bare words and fractions are not paths", () => {
  assert.deepEqual(pathLinks("https://github.com/a/b ok 1/2 main.rs"), []);
});

test("a dropped path is typed as is when the shell reads it so, else in single quotes", () => {
  assert.equal(quotePath("/Users/d/shot.png"), "/Users/d/shot.png");
  assert.equal(quotePath("/Users/d/Screen Shot.png"), "'/Users/d/Screen Shot.png'");
  assert.equal(quotePath("/tmp/it's"), "'/tmp/it'\\''s'");
});

test("a session's task worktree is its folder's under .claude/worktrees, else none", () => {
  assert.equal(worktreeOf("/r/Falinks/.claude/worktrees/fix-x/apps/studio"), "/r/Falinks/.claude/worktrees/fix-x");
  assert.equal(worktreeOf("/r/Falinks/.claude/worktrees/fix-x"), "/r/Falinks/.claude/worktrees/fix-x");
  assert.equal(worktreeOf("/r/Falinks/apps/studio"), null);
});
