// The quiet frame (#566): named tokens and one button component. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";

const ui = new URL("../", import.meta.url);
const read = (name) => fs.readFileSync(new URL(name, ui), "utf8");
const scripts = fs.readdirSync(ui).filter((name) => name.endsWith(".ts"));
// A raw size, spacing, duration or colour: pixels, milliseconds, a hex colour or a colour function.
const raw = /\d(\.\d+)?(px|ms)\b|#[0-9a-fA-F]{3,8}\b|\b(rgb|rgba|hsl|hsla)\(/;

test("the token file names sizes, spacing and colours", () => {
  const tokens = read("tokens.css");
  for (const name of ["--size", "--sp-4", "--radius", "--chrome", "--hover", "--focus"]) assert.match(tokens, new RegExp(`${name}:`));
});

test("no style outside the token file uses a raw size, spacing or colour", () => {
  const offending = [
    // Comments go first: they cite issues, such as #463, which read as colours.
    ...read("style.css").replace(/\/\*[\s\S]*?\*\//g, "").split("\n").map((line) => ["style.css", line]),
    ...read("index.html").split("\n").filter((line) => /style=/.test(line)).map((line) => ["index.html", line]),
    ...scripts.flatMap((name) => read(name).split("\n").map((line, i) => [`${name}:${i + 1}`, line]))
      .filter(([, line]) => /\.style\b|setProperty/.test(line)),
  ].filter(([, line]) => raw.test(line));
  assert.deepEqual(offending, []);
});

test("every button in the page is the button component", () => {
  const buttons = read("index.html").match(/<button[^>]*>/g);
  assert.ok(buttons.length > 10);
  assert.deepEqual(buttons.filter((tag) => !/class="[^"]*\bbtn\b/.test(tag)), []);
});

// shortcut: a line check, not a DOM; a button built over several lines needs `pressable` or "btn" on
// its creating line. Upgrade to a DOM test if a builder outgrows that.
test("every button the scripts build is the button component", () => {
  const built = scripts.flatMap((name) =>
    read(name).split("\n").map((line, i) => [`${name}:${i + 1}`, line])
      .filter(([, line]) => /createElement\("button"\)|(text|prose)\("button"/.test(line)),
  );
  assert.ok(built.length > 10);
  assert.deepEqual(built.filter(([, line]) => !/pressable\(|\bbtn\b/.test(line)), []);
  assert.match(read("dom.ts"), /classList\.add\("btn"\)/);
  // A `className =` after `pressable` would drop the component's class.
  const clobbered = scripts.flatMap((name) => {
    const source = read(name);
    const made = [...source.matchAll(/const (\w+) = pressable\(/g)].map((m) => m[1]);
    return made.filter((variable) => new RegExp(`\\b${variable}\\.className =`).test(source)).map((v) => `${name}: ${v}`);
  });
  assert.deepEqual(clobbered, []);
});
