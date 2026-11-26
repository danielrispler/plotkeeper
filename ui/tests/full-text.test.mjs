// Full text of cut text (#162, spec #582): a text the layout cuts is its element's own text, which scrolls into view in place (the `cut`
// class), or, when it is shortened or very long, has the full text in a hint of its own or an ancestor's. No `title` holds it, and no
// element has an empty `title`. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { El, fakeDocument } from "./dom.mjs";
import { row, projectHead } from "../../dist/list.js";
import * as dom from "../../dist/dom.js";
import { hintOf } from "../../dist/dom.js";
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
// The words a node or an ancestor shows on hover: its hint (spec #582, whose triggers have no title).
const titled = (node, parents = []) => [node, ...parents].map((p) => hintOf(p)?.textContent).filter((t) => typeof t === "string");
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
  assert.ok(!node.title, `${name} has no title`);
  assert.ok(titled(node, ancestors).some((title) => title.includes(full)), `${name} holds its full text in a hint`);
};
// A cut text that is its element's own text, whole, and scrolls in place on hover.
const cuts = (root, name, full) => {
  const found = path(root, name);
  assert.ok(found, `${name} is drawn`);
  const node = found[found.length - 1];
  assert.equal(node.textContent, full, `${name} keeps its full text as its own`);
  assert.ok(node.className.split(" ").includes("cut"), `${name} scrolls in place`);
  assert.ok(!node.title, `${name} has no title`);
};
const noEmptyTitle = (root) => {
  for (const node of every(root)) assert.notEqual(node.title, "", `an empty title on <${node.tag} class="${node.className}">`);
};

const long = "Session UI redesign: long name that the row cuts to fit the list";
const step = "Edit src/some/very/long/path/to/a/file/that/the/second/line/cuts.rs";
const token = "issue-162-a-long-branch-name";
const session = /** @type {import("../../dist/types.js").Session} */ ({
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
});

test("a session row's name and token scroll in place, its step and identity are in a hint, and none is empty", () => {
  const button = row(session, new Map(), token);
  cuts(button, "name", long);
  holds(button, "name", "PR #9 · issue-9");
  holds(button, "row-step", step);
  cuts(button, "token", token);
  noEmptyTitle(button);
});

test("a prompt-named row's hint is its name and status, as its title was; the name's own hint adds the identity", () => {
  const button = row(session, new Map(), token);
  assert.equal(hintOf(button)?.textContent, `${long} · Running`);
  const name = path(button, "name")?.at(-1);
  assert.equal(hintOf(name)?.textContent, `${long} · PR #9 · issue-9`);
});

test("a named session's name needs no title of its own: the row's hint holds it", () => {
  const button = row({ ...session, name: "Short", prompt_name: null }, new Map());
  cuts(button, "name", "Short");
  noEmptyTitle(button);
});

test("a folded turn line's prompt holds its full text in a hint, its Headline scrolls in place", () => {
  const headline = "A Headline the line cuts to fit the rail";
  const turn = { id: null, prompt: "First line of a long prompt\nsecond line", message: headline, checks: [] };
  studio.selection = null;
  // The folded line reads only the session's id, and the block's turn.
  const node = foldedTurn(/** @type {import("../../dist/types.js").Session} */ ({ id: "s" }), /** @type {import("../../dist/story.js").FeedTurn} */ (/** @type {unknown} */ ({ index: 0, turn, cards: [] })));
  holds(node, "prompt", "First line of a long prompt\nsecond line");
  cuts(node, "headline-text", headline);
  noEmptyTitle(node);
});

test("a toast's name and line scroll in place", () => {
  studio.selection = null;
  studio.listed = [{ ...session, state: "needs_you", questions: [/** @type {import("../../dist/types.js").Question} */ ({ answer: "open", questions: [{ question: "A long question the toast cuts?" }] })] }];
  showToast();
  const toast = document.getElementById("toast");
  assert.equal(toast.hidden, false);
  cuts(toast, "name", long);
  cuts(toast, "line", "A long question the toast cuts?");
  noEmptyTitle(toast);
});

test("a check's command scrolls in place", () => {
  const command = "cargo test --locked --workspace --all-targets -- --some-long-flag";
  const node = checkElement({ statement: command, status: "failed", ended_at: 2, started_at: 1, typical_ms: null });
  const code = every(node).find((n) => n.tag === "code");
  assert.ok(code, "the command is drawn");
  cuts(node, "cut", command);
});

test("the palette's rows scroll their cut text in place", () => {
  const palette = document.getElementById("palette");
  // A closed popover, as in the page.
  Object.assign(palette, { matches: () => false, showPopover() {}, hidePopover() {} });
  const make = document.createElement;
  document.createElement = (tag) => Object.assign(make(tag), tag === "input" ? { value: "", focus() {} } : {});
  showPalette([{ title: "Switch to a session with a very long action name", keys: ["⌘K"], note: "A note the row cuts to fit", binding: /** @type {any} */ ({}) }], () => {});
  const rows = every(palette).filter((n) => n.getAttribute?.("role") === "option");
  assert.ok(rows.length > 0, "the palette draws its rows");
  for (const drawn of rows) {
    cuts(drawn, "name", "Switch to a session with a very long action name");
    cuts(drawn, "l2", "A note the row cuts to fit");
    noEmptyTitle(drawn);
  }
  document.createElement = make;
});

test("a project group's toggle has a hint, also for the home folder", () => {
  const group = (key, name) => /** @type {import("../../dist/projects.js").ProjectGroup<import("../../dist/types.js").Session>} */ ({ key, name, rollup: null, states: [] });
  const head = projectHead(group("/Users/someone/elsewhere/a-repository", "a-repository"));
  holds(head, "project-toggle", "/Users/someone/elsewhere/a-repository");
  noEmptyTitle(projectHead(group("", "~")));
  holds(projectHead(group("", "~")), "project-toggle", "~");
});

// Every CSS rule that cuts text, by selector, with where its full text is: a new one fails this test until it is covered.
const covered = {
  ".top": "a container: its items are the session header and two chips",
  ".strip .btn": "list.ts: the button's hint holds the name and state",
  ".row .l1": "a container: its name, token and glyph carry the titles",
  ".row .l2": "a container: its step carries the title",
  ".row .name": "list.ts: the row button's hint holds it; a prompt-named session's name has its own hint, which adds the identity",
  ".row .row-step": "list.ts: the step's title",
  ".row .token": "list.ts: the token's title",
  ".project-toggle .name": "list.ts: the toggle's hint is the project's path",
  ".session-header .name": "story.ts: the header's title holds the full name",
  ".session-name": "story.ts: the rail header's name scrolls in place (#444, #582)",
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

test("fullTitle is retired: no native title holds a cut text", () => {
  assert.equal(/** @type {any} */ (dom).fullTitle, undefined);
});

test("cut() keeps the element's text whole, inside its own box that moves", () => {
  const node = dom.cut(dom.text("span", "A long line the layout cuts", "x"));
  assert.equal(node.textContent, "A long line the layout cuts");
  assert.ok(node.className.split(" ").includes("cut"));
  assert.equal(node.childNodes.length, 1);
  assert.equal(/** @type {any} */ (node.childNodes[0]).tag, "span");
});
