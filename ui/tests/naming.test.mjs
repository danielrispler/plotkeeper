// The session's name and context share (#480). Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { contextText, sessionName } from "../../dist/list.js";

const identity = { issue: 480, branch: "issue-480-naming", pr: 490, worktree: "agent-a1b2", name: "#480 · PR #490 · agent-a1b2" };

test("a session is named by its issue, PR and worktree, unless Daniel renamed it", () => {
  assert.equal(sessionName({ name: null, identity, folder_name: "agent-a1b2" }), "#480 · PR #490 · agent-a1b2");
  assert.equal(sessionName({ name: "Naming", identity, folder_name: "agent-a1b2" }), "Naming");
});

test("with no identity yet, a session is named by its folder", () => {
  const empty = { issue: null, branch: null, pr: null, worktree: "", name: "" };
  assert.equal(sessionName({ name: null, identity: empty, folder_name: "Falinks" }), "Falinks");
});

test("the context share rounds to a whole percent of the window", () => {
  assert.equal(contextText({ tokens: 250_004, window: 1_000_000 }), "25%");
  assert.equal(contextText({ tokens: 100_002, window: 200_000 }), "50%");
  assert.equal(contextText(null), "");
});
