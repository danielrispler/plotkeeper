// The README stays a showcase (danielrispler/plotkeeper#148, decision 7): concise, no line over 400 characters,
// every relative link leads to a file in the repository, and the npm transform rewrites all relative links and images
// to absolute GitHub URLs so nothing breaks on npmjs.com. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { rewriteReadmeForNpm } from "../../scripts/npm/prepare-readme.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const readme = readFileSync(resolve(root, "README.md"), "utf8");

// The relative targets of the README's links and images, without any #fragment.
const relativeLinks = (text) =>
  [...text.matchAll(/!?\[[^\]]*\]\(([^)\s]+)\)/g)]
    .map((match) => match[1])
    .filter((target) => !/^([a-z]+:|#|\/\/)/i.test(target))
    .map((target) => target.split("#")[0]);

test("the README is at most 850 words", () => {
  const words = readme.split(/\s+/).filter(Boolean).length;
  assert.ok(words <= 850, `README.md has ${words} words`);
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

test("the README shows the three pictures", () => {
  const shown = [...readme.matchAll(/!\[[^\]]*\]\(([^)\s]+)\)/g)].map((match) => match[1]);
  const pictures = shown.filter((target) => target.startsWith("docs/studio/assets/readme/") && target.endsWith(".png"));
  assert.equal(new Set(pictures).size, 3);
});

test("rewriteReadmeForNpm rewrites relative images and doc links to absolute GitHub URLs", () => {
  const input = [
    "![Hero](docs/studio/assets/readme/hero.png)",
    "![Nested](./docs/pic.png \"Title\")",
    "Read [the rail](docs/studio/behaviour/rail.md#section) and [MIT](LICENSE).",
    "Keep [web](https://example.com/a), [anchor](#quick-start), and `code [x](docs/nope.md)`.",
    "```md",
    "![ignored](docs/in-fence.png)",
    "```",
  ].join("\n");
  const out = rewriteReadmeForNpm(input, { repo: "danielrispler/plotkeeper", ref: "main" });
  assert.equal(
    out,
    [
      "![Hero](https://raw.githubusercontent.com/danielrispler/plotkeeper/main/docs/studio/assets/readme/hero.png)",
      "![Nested](https://raw.githubusercontent.com/danielrispler/plotkeeper/main/docs/pic.png \"Title\")",
      "Read [the rail](https://github.com/danielrispler/plotkeeper/blob/main/docs/studio/behaviour/rail.md#section) and [MIT](https://github.com/danielrispler/plotkeeper/blob/main/LICENSE).",
      "Keep [web](https://example.com/a), [anchor](#quick-start), and `code [x](docs/nope.md)`.",
      "```md",
      "![ignored](docs/in-fence.png)",
      "```",
    ].join("\n"),
  );
});

test("rewriting the real README leaves zero relative links or images", () => {
  const out = rewriteReadmeForNpm(readme, { repo: "danielrispler/plotkeeper", ref: "main" });
  assert.deepEqual(relativeLinks(out), []);
  assert.ok(
    out.includes("https://raw.githubusercontent.com/danielrispler/plotkeeper/main/docs/studio/assets/readme/hero.png"),
    "expected rewritten hero image URL",
  );
  assert.ok(
    out.includes("https://github.com/danielrispler/plotkeeper/blob/main/LICENSE"),
    "expected rewritten LICENSE URL",
  );
});
