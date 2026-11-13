// Places (danielrispler/Falinks#576): a folded section at the bottom of the list, one row per place with its counts and
// session count, offering a new session and, for a task, Remove. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { factsText, placeRows } from "../../dist/places.js";

const ui = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, ui), "utf8");

const places = {
  main: { path: "/r", branch: "main" },
  tasks: [
    { path: "/r/.claude/worktrees/issue-1", branch: "issue-1" },
    { path: "/r/.claude/worktrees/look", branch: null },
  ],
  hidden: 0,
};
const counts = [
  { ahead_behind: [0, 0], changed: 0 },
  { ahead_behind: [2, 1], changed: 3 },
  { ahead_behind: null, changed: 0 },
];
const sessions = ["/r", "/r/.claude/worktrees/issue-1", "/r/.claude/worktrees/issue-1/apps", "/elsewhere"].map((cwd) => ({ cwd }));

test("Places is folded", () => {
  const html = read("index.html").match(/<details id="places"[^>]*>/)[0];
  assert.doesNotMatch(html, /\sopen/);
});

test("each place has its name, its counts and its session count, and offers a new session and, for a task, Remove", () => {
  const rows = placeRows(places, counts, sessions);
  assert.deepEqual(
    rows.map(({ name, facts, actions }) => [name, facts, actions]),
    [
      ["Main checkout · main", "1 session", ["New session here"]],
      // A session in a task's subfolder counts for the task, not the main checkout around it.
      ["issue-1", "2 ahead · 1 behind · 3 changed · 2 sessions", ["New session here", "Remove"]],
      ["look (detached)", "", ["New session here", "Remove"]],
    ],
  );
});

test("counts that are zero, or unknown, show nothing; no base shows no ahead or behind", () => {
  assert.equal(factsText({ ahead_behind: [0, 0], changed: 0 }, 0), "");
  assert.equal(factsText(null, 0), "");
  assert.equal(factsText({ ahead_behind: [0, 4], changed: 1 }, 0), "4 behind · 1 changed");
  assert.equal(factsText({ ahead_behind: null, changed: 2 }, 1), "2 changed · 1 session");
});

test("the new-session picker no longer creates or removes tasks", () => {
  const picker = read("picker.ts");
  assert.doesNotMatch(picker, /create_task"|remove_task|Remove/);
  assert.match(read("places.ts"), /create_task"/);
});
