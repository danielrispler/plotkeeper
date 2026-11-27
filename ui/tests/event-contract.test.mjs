// The event contract (spec #548, Group 3): every event the page listens for is one the server emits, and every event the
// server emits is one the page listens for. Every server `emit` takes its name as a string literal (`server/src/lib.rs`,
// `Events::emit`), so the names are read from the source; a `#[cfg(test)]` module's emits, inline or in its `tests.rs`, are the tests' own.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const filesIn = (dir, ending) =>
  readdirSync(new URL(dir, import.meta.url), { recursive: true, encoding: "utf8" })
    .filter((name) => name.endsWith(ending))
    .map((name) => ({ file: `${dir}${name}`, source: read(`${dir}${name}`) }));

/** The names in `files` that `pattern`'s first group captures, each with the first file that has it. */
function names(files, pattern) {
  const found = new Map();
  for (const { file, source } of files) {
    for (const [, name] of source.matchAll(pattern)) if (!found.has(name)) found.set(name, file);
  }
  return found;
}

/** One line per event that one side has and the other lacks. */
function mismatches({ page, server }) {
  const listened = names(page, /\blisten(?:<[^(]*>)?\(\s*"([^"]+)"/g);
  const shipped = server.map(({ file, source }) => ({ file, source: source.split("#[cfg(test)]")[0] }));
  const emitted = names(shipped, /\bemit\(\s*"([^"]+)"/g);
  return [
    ...[...listened]
      .filter(([name]) => !emitted.has(name))
      .map(([name, file]) => `the page listens for "${name}" (${file}), which the server never emits`),
    ...[...emitted]
      .filter(([name]) => !listened.has(name))
      .map(([name, file]) => `the server emits "${name}" (${file}), which the page never listens for`),
  ];
}

test("an event the page listens for and the server never emits is named", () => {
  const page = [{ file: "ui/main.ts", source: 'listen<string>("ghost", show);\nlisten<string>("notice", show);' }];
  const server = [{ file: "server/src/lib.rs", source: 'app.events.emit("notice", json!(1));' }];
  assert.deepEqual(mismatches({ page, server }), [
    'the page listens for "ghost" (ui/main.ts), which the server never emits',
  ]);
});

test("an event the server emits and the page never listens for is named", () => {
  const page = [{ file: "ui/main.ts", source: 'listen<string>("notice", show);' }];
  const server = [
    {
      file: "server/src/read_loop.rs",
      source: 'app.events.emit("notice", n);\napp.events.emit(\n    "orphan",\n    json!(1),\n);',
    },
  ];
  assert.deepEqual(mismatches({ page, server }), [
    'the server emits "orphan" (server/src/read_loop.rs), which the page never listens for',
  ]);
});

test("a test module's emits are left out", () => {
  const page = [{ file: "ui/main.ts", source: 'listen<string>("notice", show);' }];
  const server = [
    { file: "server/src/lib.rs", source: 'emit("notice", n);\n#[cfg(test)]\nmod tests {\n    emit("early", n);\n}' },
  ];
  assert.deepEqual(mismatches({ page, server }), []);
});

test("the page and the server agree on every event", () => {
  const page = filesIn("../", ".ts").filter(({ file }) => !file.includes("/tests/"));
  const server = filesIn("../../server/src/", ".rs").filter(({ file }) => !file.endsWith("/tests.rs"));
  assert.deepEqual(mismatches({ page, server }), []);
});
