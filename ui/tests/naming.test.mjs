// The session's name and context tokens (danielrispler/Falinks#480). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { contextText, contextTitle, sessionName } from "../../dist/list-model.js";

const identity = { issue: 480, branch: "issue-480-naming", pr: 490, worktree: "agent-a1b2", name: "#480 · PR #490 · agent-a1b2" };

test("a session is named by its issue, PR and worktree, unless Daniel renamed it", () => {
  assert.equal(sessionName({ name: null, identity, folder_name: "agent-a1b2" }), "#480 · PR #490 · agent-a1b2");
  assert.equal(sessionName({ name: "Naming", identity, folder_name: "agent-a1b2" }), "Naming");
});

test("with no identity yet, a session is named by its folder", () => {
  const empty = { issue: null, branch: null, pr: null, worktree: "", name: "" };
  assert.equal(sessionName({ name: null, identity: empty, folder_name: "Falinks" }), "Falinks");
});

test("the context shows its tokens, and its hover says them in words (#416)", () => {
  assert.equal(contextText({ tokens: 84_800 }), "85k");
  assert.equal(contextTitle({ tokens: 84_800 }), "85k tokens used");
  assert.equal(contextText(null), "");
  assert.equal(contextTitle(null), "");
});

test("the prompt name follows Daniel's rename and keeps the identity's issue in front", () => {
  const session = (name, prompt_name, id = identity) => ({ name, prompt_name, identity: id, folder_name: "agent-a1b2" });
  assert.equal(sessionName(session(null, "One event internal")), "#480 · One event internal");
  assert.equal(sessionName(session("Naming", "One event internal")), "Naming");
  // The issue comes from the identity as it is now, so a change of it changes the name.
  assert.equal(sessionName(session(null, "One event internal", { ...identity, issue: 37 })), "#37 · One event internal");
  // With no issue the name stands alone; with no prompt name yet the identity shows.
  assert.equal(sessionName(session(null, "Fix the typo", { ...identity, issue: null })), "Fix the typo");
  assert.equal(sessionName(session(null, null)), "#480 · PR #490 · agent-a1b2");
});
