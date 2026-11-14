// The delete dialog's "remove worktree" default (#55): ticked unless another session that hasn't ended shares the worktree. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { removeWorktreeByDefault } from "../../dist/paths.js";

const wt = "/r/.claude/worktrees/issue-1";
const session = (id, state, cwd = wt) => ({ id, state, cwd });

test("a session alone in its task's worktree opens ticked", () => {
  const me = session("a", "idle");
  assert.equal(removeWorktreeByDefault(me, [me, session("b", "idle", "/r/.claude/worktrees/other")]), true);
});

test("a session sharing its worktree with a live one opens unticked", () => {
  const me = session("a", "idle");
  assert.equal(removeWorktreeByDefault(me, [me, session("b", "working")]), false);
});

test("an ended sharer does not count", () => {
  const me = session("a", "idle");
  assert.equal(removeWorktreeByDefault(me, [me, session("b", "ended")]), true);
});

test("a main-checkout session has no worktree to tick", () => {
  const me = session("a", "idle", "/r");
  assert.equal(removeWorktreeByDefault(me, [me]), false);
});
