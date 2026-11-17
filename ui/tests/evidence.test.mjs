// The evidence line under a turn's Headline (danielrispler/plotkeeper#113). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El } from "./dom.mjs";

globalThis.document = { createElement: (tag) => new El(tag), getElementById: () => new El("div") };
const { diffRequest } = await import("../../dist/diff.js");
const diffOpens = [];
diffRequest.open = (session, turn) => diffOpens.push([session, turn]);
const { evidenceLine, evidenceRows, evidenceElement, fileRow, hasEvidence } = await import("../../dist/evidence.js");

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

test("a file row opens its hunk in place, opens the editor at the first changed line, and stays open across a refresh", async () => {
  const calls = [];
  const hunk = { text: "@@ -1 +1,2 @@\n a\n+b\n", left_out: 12, binary: false, first_line: 2 };
  globalThis.window = {
    __TAURI__: {
      core: {
        invoke: async (command, args) => {
          calls.push([command, args]);
          return command === "turn_file_hunk" ? hunk : null;
        },
      },
    },
  };
  const turn = {
    files: files([file("a.rs", 1, 0), file("new.rs", 1, 1, { from: "old.rs" }), file("img.png", null, null)]),
    checks: [check("cargo test", "failed")],
  };
  const make = () => evidenceElement("s2", 7, turn, { only: "all" });
  const rail = new El("ul");
  const line = make();
  rail.append(new El("p"), line);
  line.find("button")[0].listeners.click();
  const before = [...rail.childNodes];
  const inner = [...line.childNodes];
  const items = [...line.find("li")];
  const row = items.find((li) => /a\.rs/.test(li.text));
  const state = items.map((li) => li.find("pre").map((p) => p.hasAttribute("hidden")));
  assert.ok(row.find("pre")[0].hasAttribute("hidden"));

  row.find("button")[0].listeners.click();
  await new Promise((resolve) => setTimeout(resolve));
  // Opening a hunk is told to the self-test (#130), then the hunk is read.
  assert.deepEqual(calls[0], ["hunk_opened", { session: "s2", turn: 7 }]);
  assert.deepEqual(calls[1], ["turn_file_hunk", { session: "s2", turn: 7, root: "/r", path: "a.rs", from: null }]);
  assert.match(row.find("pre")[0].text, /\+b/);
  assert.doesNotMatch(row.find("pre")[0].text, /more lines/);
  // "12 more lines" opens the turn's whole diff (#131).
  const more = row.find("button").find((b) => /12 more lines/.test(b.text));
  more.listeners.click();
  assert.deepEqual(diffOpens.at(-1), ["s2", 7]);
  assert.ok(!row.find("pre")[0].hasAttribute("hidden"));
  // Every other node is the same object, and every other row keeps its state.
  assert.ok(same(rail.childNodes, before));
  assert.ok(same(line.childNodes, inner));
  assert.ok(!line.find("ul")[0].hasAttribute("hidden"));
  items.forEach((li, i) => li !== row && assert.deepEqual(li.find("pre").map((p) => p.hasAttribute("hidden")), state[i]));

  // "Open in editor" goes to the first changed line, from the worktree's folder.
  row.find("button")[1].listeners.click();
  const { editor, ...opened } = calls.at(-1)[1];
  assert.equal(calls.at(-1)[0], "open_file");
  assert.deepEqual(opened, { path: "a.rs", line: 2, column: 1, cwd: "/r" });

  // A refresh draws it open again, with the hunk kept.
  const again = make();
  const reopened = again.find("li").find((li) => /a\.rs/.test(li.text));
  assert.ok(!reopened.find("pre")[0].hasAttribute("hidden"));
  assert.match(reopened.find("pre")[0].text, /\+b/);
  assert.ok(again.find("li").find((li) => /new\.rs/.test(li.text)).find("pre")[0].hasAttribute("hidden"));
  assert.match(again.find("li").find((li) => /old\.rs → new\.rs/.test(li.text)).text, /old\.rs → new\.rs/);
});

test("a binary file's hunk says so and a hunk that cannot be read says that", async () => {
  let answer = { text: "", left_out: 0, binary: true, first_line: 1 };
  globalThis.window = { __TAURI__: { core: { invoke: async () => answer } } };
  const turn = { files: files([file("img.png", null, null), file("gone.rs", 1, 0)]), checks: [] };
  const line = evidenceElement("s3", 1, turn, { only: "all" });
  line.find("button")[0].listeners.click();
  const [img, gone] = line.find("li");
  img.find("button")[0].listeners.click();
  await new Promise((resolve) => setTimeout(resolve));
  assert.match(img.find("pre")[0].text, /Binary file/);
  answer = null;
  gone.find("button")[0].listeners.click();
  await new Promise((resolve) => setTimeout(resolve));
  assert.match(gone.find("pre")[0].text, /could not be read/);
});

test("the opened line offers the full diff only when the turn has files", () => {
  const line = evidenceElement("s9", 1, { files: files([file("a.rs", 1, 0)]), checks: [] }, { only: "all" });
  const open = line.find("button").find((b) => b.text === "Open full diff");
  open.listeners.click();
  assert.deepEqual(diffOpens.at(-1), ["s9", 1]);
  assert.equal(evidenceElement("s9", 2, { files: null, checks: [check("cargo test", "passed")] }, { only: "all" }).find("button").some((b) => b.text === "Open full diff"), false);
});

test("an open turn's hunk is read again at each opening and once the turn ended, and an ended turn's is kept", async () => {
  let reads = 0;
  globalThis.window = {
    __TAURI__: { core: { invoke: async (command) => (command === "turn_file_hunk" ? { text: `read ${++reads}`, left_out: 0, binary: false, first_line: 1 } : null) } },
  };
  const row = { kind: "file", label: "k.rs", file: file("k.rs", 1, 0) };
  const settle = () => new Promise((resolve) => setTimeout(resolve));
  const toggle = async (item) => (item.find("button")[0].listeners.click(), settle());
  // While the turn is open: open, close, open again reads twice.
  const open = fileRow("s3", 1, row, "", false);
  await toggle(open);
  await toggle(open);
  await toggle(open);
  assert.equal(reads, 2);
  // The turn ended: its row, drawn again with the file still open, reads the hunk again; then it is kept.
  const ended = fileRow("s3", 1, row, "", true);
  await settle();
  assert.equal(reads, 3);
  assert.match(ended.find("pre")[0].text, /read 3/);
  await toggle(ended);
  await toggle(ended);
  assert.equal(reads, 3);
  fileRow("s3", 1, row, "", true);
  await settle();
  assert.equal(reads, 3);
});
