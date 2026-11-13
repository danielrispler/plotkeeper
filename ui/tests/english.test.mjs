// The studio is English only, left to right (danielrispler/Falinks#487): no Hebrew label and no right-to-left markup in
// the UI's sources. Cards stored in Hebrew still lay out right to left, by `dir="auto"` on their text.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";

const ui = new URL("..", import.meta.url);
const sources = readdirSync(ui).filter((name) => /\.(ts|html|css)$/.test(name));

test("the UI has no Hebrew label and no right-to-left markup", () => {
  for (const name of sources) {
    const source = readFileSync(new URL(name, ui), "utf8");
    assert.doesNotMatch(source, /[֐-׿]/, `${name} has Hebrew`);
    assert.doesNotMatch(source, /dir\s*=\s*"rtl"|lang\s*=\s*"he"/, `${name} lays text right to left`);
  }
});
