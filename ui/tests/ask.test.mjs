// Ask on anchors (danielrispler/plotkeeper#125). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El } from "./dom.mjs";

globalThis.document = { createElement: (tag) => new El(tag), getElementById: () => new El("div") };
const { suggestions, factsText } = await import("../../dist/ask.js");

test("each anchor type offers two or three suggested questions", () => {
  const anchors = [
    { kind: "file", turn: 0, root: "/r", path: "a.rs" },
    { kind: "check", agent: null, call: "c" },
    { kind: "subagent", agent: "w" },
    { kind: "headline", turn: 0 },
  ];
  for (const anchor of anchors) assert.ok([2, 3].includes(suggestions(anchor).length), anchor.kind);
  assert.equal(suggestions(anchors[0])[0], "Why did it touch this file?");
});

test("the facts read as labelled lines, each step with its time", () => {
  const at = Date.UTC(2026, 10, 16, 12, 0, 0);
  const text = factsText({ sections: [{ label: "Command", facts: [{ at: null, text: "npm run check" }] }, { label: "Steps", facts: [{ at, text: "Edit by the session's agent" }] }] });
  assert.match(text, /^Command:\nnpm run check\n\nSteps:\n.+Edit by the session's agent$/);
});

test("the palette has an Ask about action", async () => {
  const { paletteEntries } = await import("../../dist/keys.js");
  const { initial } = await import("../../dist/settings.js");
  assert.ok(paletteEntries(initial.keymap, ["studio::AskAbout"]).some((entry) => entry.title === "Ask about"));
});
