// The session's name and context share (#480). Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { contextText, contextTitle, sessionName } from "../../dist/list.js";

const identity = { issue: 480, branch: "issue-480-naming", pr: 490, worktree: "agent-a1b2", name: "#480 · PR #490 · agent-a1b2" };

test("a session is named by its issue, PR and worktree, unless Daniel renamed it", () => {
  assert.equal(sessionName({ name: null, identity, folder_name: "agent-a1b2" }), "#480 · PR #490 · agent-a1b2");
  assert.equal(sessionName({ name: "Naming", identity, folder_name: "agent-a1b2" }), "Naming");
});

test("with no identity yet, a session is named by its folder", () => {
  const empty = { issue: null, branch: null, pr: null, worktree: "", name: "" };
  assert.equal(sessionName({ name: null, identity: empty, folder_name: "Falinks" }), "Falinks");
});

test("the context share is labelled, and its hover gives the tokens and the status line's view (#517, decision 3)", () => {
  const context = { tokens: 84_800, window: 1_000_000, share: 9 };
  assert.equal(contextText(context), "context 9%");
  assert.equal(contextTitle(context), "85k of 1,000k tokens used · Claude's status line shows this as 91% left");
  assert.equal(contextText(null), "");
  assert.equal(contextTitle(null), "");
});

test("with no snapshot, the context shows its tokens only and the hover says the window is unknown (#517, decision 2)", () => {
  const context = { tokens: 84_800, window: null, share: null };
  assert.equal(contextText(context), "85k");
  assert.equal(contextTitle(context), "85k tokens used · the window size is unknown");
});
