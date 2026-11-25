// Ask on anchors (danielrispler/plotkeeper#125). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fakeDocument, tauri } from "./dom.mjs";

globalThis.marked = createRequire(import.meta.url)("marked");

globalThis.document = fakeDocument();
const { suggestions, whyNotElement } = await import("../../dist/ask.js");

test("each anchor type offers two or three suggested questions", () => {
  /** @type {import("../../dist/types.js").Anchor[]} */
  const anchors = [
    { kind: "file", turn: 0, root: "/r", path: "a.rs" },
    { kind: "check", agent: null, call: "c" },
    { kind: "subagent", agent: "w" },
    { kind: "headline", turn: 0 },
  ];
  for (const anchor of anchors) assert.ok([2, 3].includes(suggestions(anchor).length), anchor.kind);
  assert.equal(suggestions(anchors[0])[0], "Why did it touch this file?");
});

const whole = (node) => node.text;

test("why didn't it touch Z shows the recorded facts first and the possible reasons folded and labelled", () => {
  const facts = { sections: [{ label: "Read it", facts: [{ at: null, text: "Read /r/z.rs" }] }] };
  // The stand-in DOM's nodes, read as such.
  const out = /** @type {any} */ (whyNotElement("z.rs", { facts, reasons: "Perhaps it was not needed." }));
  const [recorded, reasons] = [out.children[0], out.children[out.children.length - 1]];
  assert.equal(recorded.text, "Read it");
  assert.equal(reasons.tag, "details");
  assert.match(whole(reasons), /possible reasons \(not recorded\)/);
});

test("a name the turn never saw says so, with no reasons", () => {
  const out = whyNotElement("z.rs", { facts: { sections: [] }, reasons: null });
  assert.match(whole(out), /Nothing was recorded: the turn never read, searched or mentioned z\.rs/);
  assert.doesNotMatch(whole(out), /possible reasons/);
});

test("the palette has an Ask about action", async () => {
  const { paletteEntries } = await import("../../dist/keys.js");
  const { initial } = await import("../../dist/settings.js");
  assert.ok(paletteEntries(initial.keymap, ["studio::AskAbout"]).some((entry) => entry.title === "Ask about"));
});

test("Ask on a renamed file reads the hunk with the file's old path", async () => {
  const calls = [];
  tauri(calls, (command) => (command === "ask_facts" ? { sections: [] } : null));
  const { askAnchor } = await import("../../dist/ask.js");
  const node = /** @type {any} */ (askAnchor("s1", { kind: "file", turn: 3, root: "/r", path: "new.rs", from: "old.rs" }, "the file new.rs"));
  node.open = true;
  node.listeners.toggle();
  await new Promise((done) => setTimeout(done));
  assert.deepEqual(calls.find(([c]) => c === "turn_file_hunk"), ["turn_file_hunk", { session: "s1", turn: 3, root: "/r", path: "new.rs", from: "old.rs" }]);
});
