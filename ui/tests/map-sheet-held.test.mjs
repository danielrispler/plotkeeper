// A part's held items on its sheet (spec #520, ticket #525), run on the `tsc` output: an item with no paths is drawn as a note, so it never
// reads as a section heading; an item with paths is unchanged.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument, tauri } from "./dom.mjs";

tauri([]);
globalThis.document = fakeDocument({ body: new El("body") });
// The stand-in media query: never reduced motion.
globalThis.matchMedia = () => /** @type {any} */ ({ matches: false, addEventListener() {} });
const { heldList } = await import("../../dist/map-sheet.js");

const all = (node) => [node, ...node.childNodes.filter((child) => child instanceof El).flatMap(all)];
const words = (node) => node.childNodes.map((child) => (child instanceof El ? words(child) : child.text)).join("");
const classed = (node, name) => all(node).filter((each) => each.className.split(" ").includes(name));

test("a held item with no paths is drawn as a note: its name in italics, then \"· note\", then its text; one with paths is unchanged", () => {
  const list = /** @type {any} */ (heldList([
    { name: "Plugins", text: "One per agent.", files: ["agents/claude/plugin.json"] },
    { name: "Rendering", text: "How a plugin's output is drawn.", files: [] },
  ]));
  const [withPaths, without] = list.childNodes.filter((child) => child instanceof El);
  assert.deepEqual(classed(list, "sheet-held-note"), [without], "only the item without paths is a note");
  assert.equal(classed(withPaths, "sheet-held-kind").length, 0, "an item with paths has no \"· note\"");
  assert.deepEqual(classed(withPaths, "sheet-held-name").map(words), ["Plugins"]);
  assert.deepEqual(classed(withPaths, "sheet-held-files").length, 1);
  assert.deepEqual(classed(without, "sheet-held-name").map(words), ["Rendering"], "the name stands alone, for the italics");
  assert.deepEqual(classed(without, "sheet-held-kind").map(words), ["· note"]);
  assert.equal(words(without), "Rendering · noteHow a plugin's output is drawn.");
  assert.equal(classed(without, "sheet-held-files").length, 0);
});
