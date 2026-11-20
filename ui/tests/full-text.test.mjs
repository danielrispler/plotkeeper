// Full text on hover (#162): every text the layout can cut holds its full text in its own `title` or an ancestor's, and no
// element has an empty `title`, which hides the tooltip of the element under it. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { El } from "./dom.mjs";
import { row, projectHead } from "../../dist/list.js";
import { showPalette, workspaceRow as pickerRow, branchRow } from "../../dist/picker.js";
import { showStartToast, showToast } from "../../dist/toast.js";
import { checkElement, foldedTurn, showBarName } from "../../dist/story.js";
import { workspaceRow } from "../../dist/workspaces.js";
import { studio } from "../../dist/state.js";

// The stand-in page: elements by id, as the shell's index.html has them.
const page = new Map();
globalThis.document = {
  createElement: (tag) => Object.assign(new El(tag), { dataset: {}, style: {}, classList: { add() {}, toggle() {}, contains: () => false }, removeAttribute() {}, toggleAttribute() {} }),
  getElementById: (id) => page.get(id) ?? page.set(id, document.createElement("div")).get(id),
  body: { dataset: { layout: "focus" } },
};

const every = (node, out = []) => {
  if (node instanceof El) {
    out.push(node);
    for (const child of node.childNodes) every(child, out);
  }
  return out;
};
const titled = (node, parents = []) => [node.title, ...parents.map((p) => p.title)].filter((t) => typeof t === "string");
// The path from `root` down to the first element of class `name`, so its ancestors' titles can be read.
const path = (root, name, trail = []) => {
  const here = [...trail, root];
  if (root.className.split(" ").includes(name)) return here;
  for (const child of root.childNodes) {
    const found = child instanceof El && path(child, name, here);
    if (found) return found;
  }
  return null;
};
const holds = (root, name, full) => {
  const found = path(root, name);
  assert.ok(found, `${name} is drawn`);
  const [node, ...ancestors] = found.reverse();
  assert.ok(titled(node, ancestors).some((title) => title.includes(full)), `${name} holds its full text in a title`);
};
const noEmptyTitle = (root) => {
  for (const node of every(root)) assert.notEqual(node.title, "", `an empty title on <${node.tag} class="${node.className}">`);
};

const long = "Session UI redesign: long name that the row cuts to fit the list";
const step = "Edit src/some/very/long/path/to/a/file/that/the/second/line/cuts.rs";
const token = "issue-162-a-long-branch-name";
const session = {
  id: "s",
  name: null,
  prompt_name: long,
  folder_name: "f",
  identity: { issue: null, name: "PR #9 · issue-9" },
  capabilities: { agent: "claude" },
  transcript: { context: null },
  state: "running",
  line: step,
  questions: [],
  agents: [],
  unseen: false,
  landed: false,
  started_at: 0,
  last_event_at: 0,
  turn_span: null,
  status_only: false,
  shell: false,
};

test("a session row's name, step and token hold their full text in a title, and none is empty", () => {
  const button = row(session, new Map(), token);
  holds(button, "name", long);
  holds(button, "name", "PR #9 · issue-9");
  holds(button, "row-step", step);
  holds(button, "token", token);
  noEmptyTitle(button);
});

test("a named session's name needs no title of its own: the row's holds it", () => {
  const button = row({ ...session, name: "Short", prompt_name: null }, new Map());
  holds(button, "name", "Short");
  noEmptyTitle(button);
});

test("a Workspaces row's name and facts hold their full text in a title", () => {
  const workspace = { path: "/r/.claude/worktrees/issue-1", branch: "issue-1" };
  const facts = "↑2 ↓1  ●3  2 sessions";
  const node = workspaceRow("/r", { workspace, name: "issue-1-a-long-task-name", facts, actions: ["New session here"], landed: false });
  holds(node, "name", "issue-1-a-long-task-name");
  holds(node, "facts", facts);
  noEmptyTitle(node);
});

test("the session header holds the full name", () => {
  showBarName(long);
  const header = document.getElementById("session-header");
  assert.ok(header.title.includes(long));
  showBarName("");
  assert.notEqual(header.title, "");
});

test("a folded turn line's prompt and Headline hold their full text", () => {
  const headline = "A Headline the line cuts to fit the rail";
  const turn = { id: null, prompt: "First line of a long prompt\nsecond line", message: headline, checks: [] };
  studio.selection = null;
  const node = foldedTurn({ id: "s" }, { index: 0, turn, cards: [] });
  holds(node, "prompt", "First line of a long prompt\nsecond line");
  holds(node, "headline-text", headline);
  noEmptyTitle(node);
});

test("a toast's name and line hold their full text, and a start toast's message", () => {
  studio.selection = null;
  studio.listed = [{ ...session, state: "needs_you", questions: [{ answer: "open", questions: [{ question: "A long question the toast cuts?" }] }] }];
  showToast();
  const toast = document.getElementById("toast");
  assert.equal(toast.hidden, false);
  holds(toast, "name", long);
  holds(toast, "line", "A long question the toast cuts?");
  noEmptyTitle(toast);
  showStartToast("A long start failure message the toast cuts", () => {});
  const start = document.getElementById("start-toast");
  holds(start, "name", "A long start failure message the toast cuts");
  noEmptyTitle(start);
});

test("a check's command holds its full text in a title", () => {
  const command = "cargo test --locked --workspace --all-targets -- --some-long-flag";
  const node = checkElement({ statement: command, status: "failed", ended_at: 2, started_at: 1, typical_ms: null });
  const code = every(node).find((n) => n.tag === "code");
  assert.ok(code, "the command is drawn");
  assert.ok(titled(code).some((title) => title.includes(command)));
});

test("the picker's, palette's and branch picker's rows hold their full text in a title", () => {
  const picker = document.getElementById("picker");
  const path = "/Users/someone/work/a-long-project-name/.claude/worktrees/issue-1-a-long-task-name";
  const worktree = pickerRow("issue-1-a-long-task-name", { path, branch: "issue-1" }, () => Promise.resolve());
  holds(worktree, "name", "issue-1-a-long-task-name");
  holds(worktree, "l2", path);
  noEmptyTitle(worktree);
  const make = document.createElement;
  document.createElement = (tag) => Object.assign(make(tag), tag === "input" ? { value: "", focus() {} } : {});
  showPalette([{ title: "Switch to a session with a very long action name", keys: ["⌘K"], note: "A note the row cuts to fit", binding: {} }], () => {});
  const rows = every(picker).filter((n) => n.tag === "button");
  assert.ok(rows.length > 0, "the palette draws its rows");
  for (const drawn of rows) {
    holds(drawn, "name", "Switch to a session with a very long action name");
    holds(drawn, "l2", "A note the row cuts to fit");
    noEmptyTitle(drawn);
  }
  document.createElement = make;
  const branch = branchRow("a-long-branch-name-the-row-cuts", "in task X", false, () => {});
  holds(branch, "name", "a-long-branch-name-the-row-cuts");
  noEmptyTitle(branch);
});

test("the outside-project group's toggle has a title, also for the home folder", () => {
  const group = (key, name) => ({ key, name, project: true, rollup: null, states: [], workspaces: null });
  const head = projectHead(group("/Users/someone/elsewhere/a-repository", "a-repository"));
  holds(head, "project-toggle", "/Users/someone/elsewhere/a-repository");
  noEmptyTitle(projectHead(group("", "~")));
  holds(projectHead(group("", "~")), "project-toggle", "~");
});

// Every CSS rule that cuts text, by selector, with where its full text is: a new one fails this test until it is covered.
const covered = {
  ".term": "the terminal pane, not a text",
  ".term-h": "panes.ts: the program's title is the title of #terminal-title",
  ".term-body": "the terminal's grid, not a text",
  ".top": "a container: each item shows whole or sits in the ⋯ menu (bar.ts)",
  ".strip .btn": "list.ts: the button's title holds the name and state",
  ".row .l1": "a container: its name, token and glyph carry the titles",
  ".row .l2": "a container: its step carries the title",
  ".row .name": "list.ts: the row button's title holds it; a prompt-named session's own title adds the identity",
  ".row .row-step": "list.ts: the step's title",
  ".row .token": "list.ts: the token's title",
  ".project-toggle .name": "list.ts: the toggle's title is the project's path",
  ".picker .project-pill .name": "picker.ts: the pill's title is the project's path",
  ".project-workspaces .row .facts": "workspaces.ts: the facts' title",
  ".session-header .name": "story.ts: the header's title holds the full name",
  ".toast .name": "toast.ts: the name's title",
  ".toast .line": "toast.ts: the line's title",
  ".turn .prompt": "story.ts: the prompt's title",
  ".turn-line .headline-text": "story.ts: the Headline's title",
  ".turn .subagents button": "story.ts: the button's title holds its lines",
  ".turn .subagents button > :nth-child(2)": "story.ts: the button's title holds it",
  ".answered-card > summary": "cards.ts: the summary's title",
  ".unseen-card > summary": "cards.ts: the summary's title",
  ".subagent-log .log-title": "log.ts: the title's title",
  ".subagent-log .log-call summary": "log.ts: the summary's title",
  ".usage-tile .group-name": "usage.ts: the group name's title",
  code: "a check's command has its own title (story.ts); any other code span wraps or sits in a text that wraps",
};

test("every CSS rule that cuts text is covered by the full-text rule", () => {
  const css = fs.readFileSync(new URL("../style.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const cutting = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)]
    .filter(([, , body]) => /text-overflow:\s*ellipsis|overflow:\s*hidden/.test(body))
    .flatMap(([, selector]) => selector.split(",").map((s) => s.trim().replace(/\s+/g, " ")));
  assert.ok(cutting.length > 10);
  assert.deepEqual(cutting.filter((selector) => !(selector in covered) && !(selector === "html" || selector === "body")), []);
});
