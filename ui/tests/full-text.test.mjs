// Full text on hover (#162): every text the layout can cut holds its full text in its own `title` or an ancestor's, and no
// element has an empty `title`, which hides the tooltip of the element under it. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { El, fakeDocument } from "./dom.mjs";
import { row, projectHead } from "../../dist/list.js";
import { showPalette } from "../../dist/palette.js";
import { showToast } from "../../dist/toast.js";
import { checkElement, foldedTurn } from "../../dist/story.js";
import { studio } from "../../dist/state.js";

// The stand-in page: elements by id, as the shell's index.html has them.
const page = new Map();
globalThis.document = fakeDocument({
  getElementById: (id) => page.get(id) ?? page.set(id, document.createElement("div")).get(id),
  body: { dataset: { layout: "focus" } },
});

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
});

test("a check's command holds its full text in a title", () => {
  const command = "cargo test --locked --workspace --all-targets -- --some-long-flag";
  const node = checkElement({ statement: command, status: "failed", ended_at: 2, started_at: 1, typical_ms: null });
  const code = every(node).find((n) => n.tag === "code");
  assert.ok(code, "the command is drawn");
  assert.ok(titled(code).some((title) => title.includes(command)));
});

test("the palette's rows hold their full text in a title", () => {
  const palette = document.getElementById("palette");
  // A closed popover, as in the page.
  Object.assign(palette, { matches: () => false, showPopover() {}, hidePopover() {} });
  const make = document.createElement;
  document.createElement = (tag) => Object.assign(make(tag), tag === "input" ? { value: "", focus() {} } : {});
  showPalette([{ title: "Switch to a session with a very long action name", keys: ["⌘K"], note: "A note the row cuts to fit", binding: {} }], () => {});
  const rows = every(palette).filter((n) => n.getAttribute?.("role") === "option");
  assert.ok(rows.length > 0, "the palette draws its rows");
  for (const drawn of rows) {
    holds(drawn, "name", "Switch to a session with a very long action name");
    holds(drawn, "l2", "A note the row cuts to fit");
    noEmptyTitle(drawn);
  }
  document.createElement = make;
});

test("a project group's toggle has a title, also for the home folder", () => {
  const group = (key, name) => ({ key, name, rollup: null, states: [] });
  const head = projectHead(group("/Users/someone/elsewhere/a-repository", "a-repository"));
  holds(head, "project-toggle", "/Users/someone/elsewhere/a-repository");
  noEmptyTitle(projectHead(group("", "~")));
  holds(projectHead(group("", "~")), "project-toggle", "~");
});

// Every CSS rule that cuts text, by selector, with where its full text is: a new one fails this test until it is covered.
const covered = {
  ".top": "a container: its items are the session header and two chips",
  ".strip .btn": "list.ts: the button's title holds the name and state",
  ".row .l1": "a container: its name, token and glyph carry the titles",
  ".row .l2": "a container: its step carries the title",
  ".row .name": "list.ts: the row button's title holds it; a prompt-named session's own title adds the identity",
  ".row .row-step": "list.ts: the step's title",
  ".row .token": "list.ts: the token's title",
  ".project-toggle .name": "list.ts: the toggle's title is the project's path",
  ".session-header .name": "story.ts: the header's title holds the full name",
  ".session-name": "story.ts: the rail header's name is its own title (#444)",
  ".line-2 > .block": "story.ts: the block's kind is its own title",
  ".line-2 > .phase": "story.ts: the estimate is its own title",
  ".intention > .tip": "story.ts: a Wait tip's line is its own title (#523)",
  ".toast .name": "toast.ts: the name's title",
  ".toast .line": "toast.ts: the line's title",
  ".toast.copy .copy-lines": "notes.ts: the copy toast's lines' title holds them",
  ".turn .prompt": "story.ts: the prompt's title",
  ".turn-line .headline-text": "story.ts: the Headline's title",
  ".turn .subagents button": "story.ts: the button's title holds its lines",
  ".turn .subagents button > :nth-child(2)": "story.ts: the button's title holds it",
  ".answered-card > summary": "cards.ts: the summary's title",
  ".unseen-card > summary": "cards.ts: the summary's title",
  ".subagent-log .log-title": "log.ts: the title's title",
  ".subagent-log .log-call summary": "log.ts: the summary's title",
  ".map-canvas": "map.ts: the map area is a camera, not a text; it pans and zooms to what it cuts",
  ".bar-project": "map.ts: the top bar's project name has the project's path as its title",
  ".map-view": "map.ts: the map is cut at its rounded corner; the camera pans to what it cuts",
  ".map-panel": "panel.ts: the left panel's body scrolls, so nothing in it is cut; the panel only rounds its corners",
  ".plate-name": "map.ts: the plate's title holds the name, caption and purpose",
  ".plate-caption": "map.ts: the plate's title holds the name, caption and purpose",
  ".spot-chip-name": "map-agents.ts: the spotlit session's chip's title holds its name, state and line",
  ".spot-chip-line": "map-agents.ts: the spotlit session's chip's title holds its name, state and line",
  ".sheet-here-headline": "map-sheet.ts: a This session here row's Headline holds its full text as its title",
  ".risk-note": "blocked.ts: the note's title holds all of it",
  ".ask::backdrop": "style.css: a dialog's backdrop holds no text; overflow hidden only makes it a scroll container that keeps its scroll",
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
