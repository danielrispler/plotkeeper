// The README stays a showcase (danielrispler/plotkeeper#148, decision 7): at most 700 words, no line over 400 characters,
// and every relative link leads to a file in the repository. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const readme = readFileSync(resolve(root, "README.md"), "utf8");

// The relative targets of the README's links and images, without any #fragment.
const relativeLinks = (text) =>
  [...text.matchAll(/!?\[[^\]]*\]\(([^)\s]+)\)/g)]
    .map((match) => match[1])
    .filter((target) => !/^([a-z]+:|#)/i.test(target))
    .map((target) => target.split("#")[0]);

test("the README is at most 700 words", () => {
  const words = readme.split(/\s+/).filter(Boolean).length;
  assert.ok(words <= 700, `README.md has ${words} words`);
});

test("no README line is over 400 characters", () => {
  const long = readme.split("\n").findIndex((line) => line.length > 400);
  assert.equal(long, -1, `README.md line ${long + 1} is over 400 characters`);
});

test("every relative link in the README leads to a file in the repository", () => {
  const links = relativeLinks(readme);
  assert.ok(links.length > 0, "README.md has no relative links");
  const missing = links.filter((target) => !existsSync(resolve(root, target)));
  assert.deepEqual(missing, []);
});

test("the link finder reads links and images and skips URLs and anchors", () => {
  const text = "[a](docs/x.md) ![b](pic.png#z) [c](https://x.org/y) [d](#top)";
  assert.deepEqual(relativeLinks(text), ["docs/x.md", "pic.png"]);
});

test("the README shows the four pictures", () => {
  const shown = [...readme.matchAll(/!\[[^\]]*\]\(([^)\s]+)\)/g)].map((match) => match[1]);
  const pictures = shown.filter((target) => target.startsWith("docs/studio/assets/readme/") && target.endsWith(".png"));
  assert.equal(new Set(pictures).size, 4);
});
