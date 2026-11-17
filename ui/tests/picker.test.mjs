// The picker keeps the project it opened on (danielrispler/plotkeeper#108). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { quickTask } from "../../dist/picker.js";

const main = { path: "/a", name: "main" };

test("the quick start creates the task in the project the picker opened on", async () => {
  const calls = [];
  const create = async (command, args) => (calls.push([command, args]), { path: "/a/task" });
  await quickTask("/a", main, "#12", create);
  // A selection change to another project afterwards changes nothing: the project is fixed at open.
  await quickTask("/a", main, "fix the bug", create);
  assert.deepEqual(calls, [
    ["create_named_task", { project: "/a", request: "#12" }],
    ["create_named_task", { project: "/a", request: "fix the bug" }],
  ]);
});

test("an empty quick start starts in the main checkout without creating", async () => {
  const create = async () => assert.fail("no task is created");
  assert.equal(await quickTask("/a", main, "", create), main);
});

test("the bar holds one '+' and no New terminal button (#159)", async () => {
  const html = (await import("node:fs")).readFileSync(new URL("../index.html", import.meta.url), "utf8");
  assert.equal([...html.matchAll(/<button id="new-session"/g)].length, 1);
  assert.ok(!html.includes("new-terminal"));
  assert.ok(!html.includes("New terminal"));
});

test("the list's footer holds the settings gear beside History (#159)", async () => {
  const html = (await import("node:fs")).readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const foot = html.match(/<div class="list-foot">[\s\S]*?<\/div>\s*<\/div>/)[0];
  assert.ok(foot.includes('id="history-link"') && foot.includes('id="settings-gear"'));
});
