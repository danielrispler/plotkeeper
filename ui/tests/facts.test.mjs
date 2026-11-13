// Each fact once (danielrispler/Falinks#571): rows show no raw Markdown, long paths show their last 2 segments with the
// full path on hover, and no clock or estimate says nothing. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { plain, rowText, rowTime } from "../../dist/list.js";
import { shortPath, shortPaths } from "../../dist/paths.js";
import { estimateText, times } from "../../dist/times.js";

const MIN = 60_000;

test("a row line renders no raw Markdown, but keeps code as code", () => {
  assert.equal(plain("Yes, screenshots of **our app** at full width"), "Yes, screenshots of our app at full width");
  assert.equal(plain("Yes, screenshots of **our app…"), "Yes, screenshots of our app…");
  assert.equal(plain("## Plan: see [the spec](https://x.y/565) and __this__"), "Plan: see the spec and this");
  assert.equal(plain("Glob `**/*.rs` in **src**"), "Glob `**/*.rs` in src");
  assert.equal(plain("Issue `x`# 565"), "Issue `x`# 565");
});

test("a row line that drops a link's URL keeps the full line on hover", () => {
  const line = "See [the spec](https://x.y/565)";
  assert.deepEqual(rowText(line), ["See the spec", line]);
  assert.deepEqual(rowText("See **this**"), ["See this", ""]);
});

test("a long path shows its last 2 segments, with the full path in its title", () => {
  const full = "/private/tmp/claude-501/x/scratchpad/map266.md";
  assert.equal(shortPath(full), "scratchpad/map266.md");
  assert.equal(shortPath("ui/list.ts"), "ui/list.ts");
  assert.deepEqual(shortPaths(`Read ${full}`), ["Read scratchpad/map266.md", `Read ${full}`]);
  assert.deepEqual(shortPaths("Edit src/lib.rs"), ["Edit src/lib.rs", ""]);
  assert.deepEqual(shortPaths("Open https://github.com/a/b/c/d"), ["Open https://github.com/a/b/c/d", ""]);
});

test("a row shows no \"no estimate\" or \"Step 0s\"", () => {
  const work = { phases: [{ phase: "orienting", at: 0, agent_ms: 0 }], estimate: { range: null, depends_on_you: false } };
  assert.equal(estimateText(work), "Orienting");
  const session = { state: "running", started_at: 0, last_event_at: 0, turn_span: { start: 50 * MIN, end: null } };
  const drawn = times(session, { start: 61 * MIN, end: null }).map(([, draw]) => draw(61 * MIN + 30_000));
  assert.deepEqual(drawn, ["Session 1h 01m", "Turn 11m", ""]);
});

test("only the row being looked at shows a clock, at minute level", () => {
  const session = { state: "running", started_at: 0, last_event_at: 0, turn_span: { start: 0, end: null } };
  assert.equal(rowTime(session, false), null);
  assert.equal(rowTime(session, true)[1](12 * MIN + 59_000), "Turn 12m");
  assert.equal(rowTime(session, true)[1](45_000), "");
});
