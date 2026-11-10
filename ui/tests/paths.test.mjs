// File paths in the terminal pane (#466): which text is a clickable path, and how a dropped file's
// path is typed. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { fileTarget, pathLinks, quotePath, worktreeOf } from "../../dist/paths.js";

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

test("each line format links whole and parses to its path, line and column", () => {
  const cases = {
    "src/a.rs": ["src/a.rs", 1, 1],
    "src/a.rs:12": ["src/a.rs", 12, 1],
    "src/a.rs:12:3": ["src/a.rs", 12, 3],
    "src/a.rs:5-12": ["src/a.rs", 5, 1],
    "src/a.rs#L5": ["src/a.rs", 5, 1],
    "src/a.rs#L5-L12": ["src/a.rs", 5, 1],
    "file:///Users/d/My%20Code/a.rs": ["/Users/d/My Code/a.rs", 1, 1],
    "file://host.local/Users/d/a.rs#L7": ["/Users/d/a.rs", 7, 1],
    "~/notes/x.md:2": ["~/notes/x.md", 2, 1],
  };
  for (const [text, [path, line, column]] of Object.entries(cases)) {
    assert.deepEqual(fileTarget(text), { path, line, column }, text);
    if (!text.startsWith("file:")) assert.deepEqual(pathLinks(`at ${text} now`), [{ text, start: 3 }], text);
  }
});

test("a URL of another scheme is no file", () => {
  assert.equal(fileTarget("https://e.com/a.rs"), null);
  assert.equal(fileTarget("javascript:alert(1)"), null);
  // A file name before its line is no scheme.
  assert.deepEqual(fileTarget("notes.md:12"), { path: "notes.md", line: 12, column: 1 });
});
