// The marks and dots (spec #582, ticket 4): each explains itself by a hint on the pointer alone, stays out of the Tab order, and is
// `role="img"` named by its meaning; none carries a `title`. Run: `npm test`.
import { test, mock, beforeEach, afterEach } from "node:test";
import assert from "node:assert/strict";
import { fakeDocument } from "./dom.mjs";

const { mark, hintOf: hintFor } = await import("../../dist/dom.js");
const { logo } = await import("../../dist/logos.js");
const { lawMark, brokenMark, linkMark } = await import("../../dist/map.js");
const { row } = await import("../../dist/list.js");
const { staleMark } = await import("../../dist/map-stale.js");

const page = new Map();
globalThis.document = fakeDocument({
  getElementById: (id) => page.get(id) ?? page.set(id, document.createElement("div")).get(id),
  createElementNS: (_ns, tag) => document.createElement(tag),
});
/** @type {(node: any) => any} */
const hintOf = (node) => hintFor(node);
beforeEach(() => mock.timers.enable({ apis: ["setTimeout"] }));
afterEach(() => mock.timers.reset());

// What every mark must be: named, hinted by the same words, pointer-only, no title.
/** @type {(node: any, words: string) => void} */
const isMark = (/** @type {any} */ node, /** @type {string} */ words) => {
  assert.equal(node.getAttribute("role"), "img");
  assert.equal(node.getAttribute("aria-label"), words);
  assert.equal(hintOf(node)?.textContent, words);
  assert.equal(node.getAttribute("title"), null);
  assert.equal(node.getAttribute("interestfor"), null);
  assert.equal(node.getAttribute("tabindex"), null);
  assert.equal(node.listeners.focus, undefined, "a mark takes no focus");
  assert.ok(node.listeners.pointerenter, "a mark opens its hint on the pointer");
};

test("mark names a span by its words and opens its hint on the pointer alone", () => {
  const node = /** @type {any} */ (document.createElement("span"));
  node.setAttribute("data-key", "m1");
  mark(node, "Backed by a check");
  isMark(node, "Backed by a check");
  node.listeners.pointerenter({});
  mock.timers.tick(501);
  assert.equal(hintOf(node).popoverOpen, true);
  // The name is the hint's words, so the hint adds no description to read twice.
  assert.equal(node.getAttribute("aria-describedby"), null);
});

test("an agent's logo is a mark: named by its title, hinted, no title", () => {
  const drawn = logo("codex");
  isMark(drawn, "Codex");
  const noted = logo("claude", "Claude Code\nThis agent reports no context size");
  isMark(noted, "Claude Code\nThis agent reports no context size");
});

test("a part's law mark and broken-law mark are marks, and a link's mark is one on the SVG", () => {
  /** @type {import("../../dist/map-model.js").Law} */
  const law = { id: "l1", on: "core", backing: "check", text: "A test stands behind it" };
  isMark(lawMark(law, "plate-core-l1"), "Backed by a check: A test stands behind it");
  isMark(lawMark({ ...law, backing: "trust" }, "plate-core-l2"), "On trust: A test stands behind it");
  isMark(brokenMark("core"), "A test that backs laws here failed on main");
  const link = /** @type {any} */ (linkMark(["l1"], law, { x: 1, y: 2 }));
  isMark(link, "Backed by a check: A test stands behind it");
  assert.deepEqual(link.childNodes.filter((child) => child.tag === "title"), [], "no SVG <title>");
});

test("no source of the page sets a native title: not an attribute, a property, a setAttribute call or an SVG <title>", async () => {
  const { readdirSync, readFileSync } = await import("node:fs");
  const dir = new URL("../", import.meta.url);
  const files = [...readdirSync(dir).filter((name) => name.endsWith(".ts") || name === "index.html")];
  const sets = /\stitle=|\w\.title\s*=[^=]|setAttribute\(\s*["']title["']|\bsvg\(\s*["']title["']|<title>(?!Plotkeeper<\/title>)/;
  const found = files.flatMap((name) => readFileSync(new URL(name, dir), "utf8").split("\n").map((line, i) => [`${name}:${i + 1}`, line]))
    .filter(([, line]) => sets.test(line) && !/document\.title\s*=/.test(line));
  assert.deepEqual(found, []);
});

test("a session row's id mark is a mark, named by what it says in full", () => {
  const session = /** @type {any} */ ({
    id: "s", name: "Named", prompt_name: null, folder_name: "f", identity: { issue: null, name: "f" }, capabilities: { agent: "claude" },
    transcript: { context: { tokens: 12000 } }, state: "idle", line: null, questions: [], agents: [], unseen: false, landed: false,
    started_at: 0, last_event_at: 0, turn_span: null, status_only: false, shell: false,
  });
  /** @type {(drawn: any) => any} */
  const idOf = (drawn) => drawn.childNodes[0].childNodes.find((/** @type {any} */ child) => child.className === "id");
  isMark(idOf(row(session, new Map())), "12k tokens used");
  isMark(idOf(row({ ...session, id: "t", shell: true }, new Map())), "A plain terminal");
  isMark(idOf(row({ ...session, id: "u", status_only: true }, new Map())), "Another agent, followed by its reported status only");
});

test("a stale plate's band is a mark, named by its band's words", () => {
  isMark(staleMark("core", 2), "You last engaged with this part 6 weeks or more ago");
});
