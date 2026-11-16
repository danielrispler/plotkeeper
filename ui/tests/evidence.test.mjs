// The evidence line under a turn's Headline (danielrispler/plotkeeper#113). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El } from "./dom.mjs";

globalThis.document = { createElement: (tag) => new El(tag), getElementById: () => new El("div") };
const { evidenceLine, evidenceRows, evidenceElement, hasEvidence } = await import("../../dist/evidence.js");

const file = (path, added, removed, extra = {}) => ({ root: "/r", path, from: null, added, removed, ...extra });
const run = (status, call, exit_code = null) => ({ command: "cargo test", status, exit_code, call, agent: null });
const check = (suite, status, retries = 0, runs = [run(status, `c-${suite}`, status === "failed" ? 101 : 0)]) => ({ suite, status, retries, runs });
const same = (a, b) => a.length === b.length && a.every((node, i) => node === b[i]);
const files = (list, external = 0) => ({ files: list, external });

test("the top level: files and each check's final result, singular and plural", () => {
  assert.equal(evidenceLine({ files: files([file("a", 1, 0)]), checks: [] }), "1 file");
  assert.equal(evidenceLine({ files: files([file("a", 1, 0), file("b", 2, 2), file("c", 0, 4)]), checks: [] }), "3 files");
  assert.equal(evidenceLine({ files: null, checks: [check("cargo test", "passed")] }), "1 check passed");
  assert.equal(
    evidenceLine({ files: files([file("a", 1, 0), file("b", 1, 1), file("c", 1, 1)]), checks: [check("cargo test", "passed"), check("npm run check", "passed")] }),
    "3 files · 2 checks passed",
  );
});

test("a failure is named only for a check whose last run failed; a retried pass is a pass; a check that ran is counted apart", () => {
  const failed = evidenceLine({ files: files([file("a", 1, 0)]), checks: [check("cargo test", "failed", 0), check("npm run check", "passed")] });
  assert.equal(failed, "1 file · 1 check passed · ✗ cargo test failed");
  const retried = evidenceLine({ files: null, checks: [check("cargo test", "passed", 1)] });
  assert.equal(retried, "1 check passed");
  assert.doesNotMatch(retried, /✗|failed/);
  assert.equal(evidenceLine({ files: null, checks: [check("cargo test", "ran"), check("go test", "ran")] }), "2 checks ran");
  assert.equal(evidenceLine({ files: null, checks: [check("a", "passed"), check("b", "ran")] }), "1 check passed · 1 check ran");
});

test("opened: each file with its line counts, the external count muted, each check with its status", () => {
  const rows = evidenceRows({
    files: files([file("ui/story.ts", 12, 3), file("img.png", null, null), file("new.ts", 4, 0, { from: "old.ts" })], 2),
    checks: [check("cargo test", "passed", 1), check("npm run check", "passed"), check("go test", "ran"), check("cargo clippy", "failed", 2)],
  });
  assert.deepEqual(rows.map((r) => [r.kind, r.label]), [
    ["file", "ui/story.ts +12 −3"],
    ["file", "img.png"],
    ["file", "old.ts → new.ts +4 −0"],
    ["external", "+2 external"],
    ["check", "cargo test passed after 1 retry"],
    ["check", "npm run check passed"],
    ["check", "go test ran"],
    ["check", "cargo clippy failed after 2 retries"],
  ]);
  assert.equal(rows.find((r) => r.label.startsWith("cargo clippy")).call, "c-cargo clippy");
  assert.equal(rows.find((r) => r.label === "go test ran").call, undefined);
  assert.equal(evidenceRows({ files: files([file("a", 1, 0)], 0), checks: [] }).some((r) => r.kind === "external"), false);
});

test("no line for an agy session, or a turn with no files and no checks", () => {
  const turn = { files: files([file("a", 1, 0)]), checks: [check("cargo test", "passed")] };
  assert.equal(hasEvidence(turn, { turn_trees: true }), true);
  assert.equal(hasEvidence(turn, { turn_trees: false }), false);
  assert.equal(hasEvidence({ files: null, checks: [] }, { turn_trees: true }), false);
  assert.equal(hasEvidence({ files: files([]), checks: [] }, { turn_trees: true }), false);
});

test("opening the line, then a failed check's output, changes only those nodes, and the open line outlasts a refresh", async () => {
  globalThis.window = { __TAURI__: { core: { invoke: async () => ({ exit_code: 101, tail: "error[E0308]: mismatched types" }) } } };
  const turn = { files: files([file("a.rs", 3, 1)], 1), checks: [check("cargo test", "failed")] };
  const make = () => evidenceElement("s1", 4, turn, { only: "all" });

  const rail = new El("ul");
  const sibling = new El("p", {}, ["Headline"]);
  const line = make();
  rail.append(sibling, line);
  const before = [...rail.childNodes];
  const inner = [...line.childNodes];
  assert.match(line.text, /1 file · ✗ cargo test failed/);
  assert.equal(line.find("button")[0].getAttribute("aria-expanded"), "false");
  assert.ok(line.find("ul")[0].hasAttribute("hidden"));

  line.find("button")[0].listeners.click();
  assert.equal(line.find("button")[0].getAttribute("aria-expanded"), "true");
  assert.ok(!line.find("ul")[0].hasAttribute("hidden"));
  assert.ok(same(rail.childNodes, before));
  assert.ok(same(line.childNodes, inner));

  const failedRow = line.find("li").find((li) => /cargo test/.test(li.text));
  const rowChildren = [...failedRow.childNodes];
  failedRow.find("button")[0].listeners.click();
  await new Promise((resolve) => setTimeout(resolve));
  assert.match(failedRow.text, /exit code 101/);
  assert.match(failedRow.text, /mismatched types/);
  assert.ok(same(rail.childNodes, before));
  assert.equal(failedRow.childNodes[0], rowChildren[0]);

  // A refresh of the studio's state draws the line again: it is still open, and so is the output.
  const again = make();
  assert.ok(!again.find("ul")[0].hasAttribute("hidden"));
  assert.match(again.text, /mismatched types/);
  // Another turn's line is not.
  assert.ok(evidenceElement("s1", 3, turn, { only: "all" }).find("ul")[0].hasAttribute("hidden"));
});

test("while running, only a check whose last run failed shows", () => {
  const turn = { files: files([file("a", 1, 0)]), checks: [check("cargo test", "passed"), check("cargo clippy", "failed")] };
  const el = evidenceElement("s1", 9, turn, { only: "failed" });
  assert.match(el.text, /✗ cargo clippy failed/);
  assert.doesNotMatch(el.text, /file|cargo test/);
  assert.equal(evidenceElement("s1", 9, { files: null, checks: [check("cargo test", "passed")] }, { only: "failed" }), null);
});
