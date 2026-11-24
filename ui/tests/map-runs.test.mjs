// The Map view's background runs (spec #378, ticket #390), run on the `tsc` output: the queue, a running run, Cancel, the pause's
// one indicator and the prompt a failed run copies.
import { test } from "node:test";
import assert from "node:assert/strict";
import { El, fakeDocument } from "./dom.mjs";

globalThis.document = fakeDocument({ body: new El("body") });
const { paintRuns, runRows, runsNodes } = await import("../../dist/map-runs.js");

const run = (id, state, extra = {}) => ({ id, kind: "back-law", state, at: Date.UTC(2026, 10, 23, 12, 0), outcome: null, line: null, denials: [], prompt: null, ...extra });
const ended = (id, outcome, line, extra = {}) => run(id, "ended", { outcome, line, prompt: outcome === "done" ? null : "Write a test that backs the law.", ...extra });
// `confirm` answers a confirm dialog with `yes`, and notes what it asked.
const acts = (calls, copy = async () => "copied", yes = true) => ({
  cancel: (id) => calls.push(["cancel", id]),
  confirm: async (ask) => (calls.push(["confirm", ask]), yes),
  resume: () => calls.push(["resume"]),
  copy: (prompt) => (calls.push(["copy", prompt]), copy(prompt)),
});
// The buttons of a node, by their words.
const button = (node, words) => node.find("button").find((each) => each.text === words);
const texts = (nodes) => nodes.map((node) => node.text).join("\n");

test("the queue shows in order: the running run first as it was asked for, then the ones that wait", () => {
  const rows = runRows({ paused: null, runs: [run("a", "running"), run("b", "queued"), run("c", "queued", { kind: "micro-world" })] });
  assert.deepEqual(rows.map((row) => [row.id, row.title, row.action]), [["a", "Back a law", "Cancel"], ["b", "Back a law", "Cancel"], ["c", "Micro-world", "Cancel"]]);
  assert.match(rows[0].note, /^Running since \d\d:\d\d$/);
  assert.equal(rows[1].note, "Queued, next");
  assert.equal(rows[2].note, "Queued");
  assert.deepEqual(runRows({ paused: null, runs: [] }), []);
  assert.deepEqual(runRows(undefined), [], "a payload from before runs has none");
});

test("with no run and no pause nothing is drawn", () => {
  assert.deepEqual(runsNodes({ paused: null, runs: [] }, acts([])), []);
  assert.deepEqual(runsNodes(undefined, acts([])), []);
});

test("Cancel on a queued run asks the server at once; on a running run it asks first, naming the worktree and its changes", async () => {
  const calls = [];
  const nodes = runsNodes({ paused: null, runs: [run("a", "running"), run("b", "queued")] }, acts(calls));
  assert.match(nodes[0].text, /Background runs/);
  const rows = nodes.filter((node) => node.dataset.run);
  assert.deepEqual(rows.map((row) => [row.dataset.run, row.dataset.state]), [["a", "running"], ["b", "queued"]]);
  button(rows[1], "Cancel").listeners.click();
  assert.deepEqual(calls, [["cancel", "b"]], "a queued run leaves at once");
  await button(rows[0], "Cancel").listeners.click();
  const [asked, ask] = calls[1] ?? [];
  assert.equal(asked, "confirm", "a running run asks before it is cancelled");
  assert.equal(ask.verb, "Cancel run", "the confirm button repeats the verb");
  assert.match(ask.lost, /worktree/);
  assert.match(ask.lost, /changes/);
  assert.deepEqual(calls[2], ["cancel", "a"], "confirmed, it asks the server");
  assert.equal(button(rows[0], "Copy prompt"), undefined, "a run that has not failed has no prompt to copy");
});

test("Cancel on a running run, declined, sends nothing and can be pressed again", async () => {
  const calls = [];
  const rows = runsNodes({ paused: null, runs: [run("a", "running")] }, acts(calls, undefined, false)).filter((node) => node.dataset.run);
  const cancel = button(rows[0], "Cancel");
  await cancel.listeners.click();
  assert.deepEqual(calls.map(([kind]) => kind), ["confirm"]);
  assert.notEqual(cancel.disabled, true);
});

test("an ended run says how it ended, and denials in a finished run are listed, never shown as a clean success", () => {
  const denied = ended("d", "denied", "Finished, but 2 tool calls were denied", { denials: ["Bash: git push origin HEAD", "Write: /etc/hosts"] });
  const rows = runRows({ paused: null, runs: [ended("ok", "done", "Finished"), denied] });
  assert.deepEqual(rows.map((row) => [row.note, row.action, row.prompt === null]), [["Finished", "Dismiss", true], ["Finished, but 2 tool calls were denied", "Dismiss", false]]);
  const calls = [];
  const nodes = runsNodes({ paused: null, runs: [denied] }, acts(calls)).filter((node) => node.dataset.run);
  assert.match(nodes[0].text, /Bash: git push origin HEAD/);
  assert.match(nodes[0].text, /Write: \/etc\/hosts/);
  button(nodes[0], "Dismiss").listeners.click();
  assert.deepEqual(calls, [["cancel", "d"]], "dismissing an ended run removes it");
});

test("a failed run copies its prompt, and says so", async () => {
  const calls = [];
  const failed = ended("f", "failed", "Failed: the build broke");
  const [row] = runsNodes({ paused: null, runs: [failed] }, acts(calls)).filter((node) => node.dataset.run);
  await button(row, "Copy prompt").listeners.click();
  assert.deepEqual(calls, [["copy", "Write a test that backs the law."]]);
  assert.match(row.text, /Prompt copied, paste it into your agent/);
  assert.equal(row.find("textarea").length, 0);
});

test("when the clipboard refuses, the prompt shows in a box to copy by hand", async () => {
  const failed = ended("f", "timed-out", "Stopped at its limit of 30 minutes");
  const [row] = runsNodes({ paused: null, runs: [failed] }, acts([], async () => "manual")).filter((node) => node.dataset.run);
  await button(row, "Copy prompt").listeners.click();
  assert.match(row.text, /Copy this prompt and paste it into your agent/);
  assert.equal(row.find("textarea")[0].value, "Write a test that backs the law.");
});

test("a pause shows one indicator with Resume, whatever the runs", () => {
  const calls = [];
  const paused = "Claude is not logged in: run `claude` in a terminal and log in with /login";
  const nodes = runsNodes({ paused, runs: [ended("f", "not-logged-in", paused), run("b", "queued")] }, acts(calls));
  const indicators = nodes.filter((node) => node.className.includes("runs-paused"));
  assert.equal(indicators.length, 1);
  assert.match(indicators[0].text, /Background runs are paused/);
  assert.match(indicators[0].text, /Claude is not logged in/);
  button(indicators[0], "Resume").listeners.click();
  assert.deepEqual(calls, [["resume"]]);
  assert.equal(runRows({ paused, runs: [run("b", "queued")] })[0].note, "Queued, paused");
  // The indicator shows with no run of this project too: the pause holds every project's queue.
  assert.equal(texts(runsNodes({ paused, runs: [] }, acts([]))).includes("Resume"), true);
});

test("the note of a copied prompt is drawn again with its row, so a redraw of the map keeps it", () => {
  const failed = ended("f", "failed", "Failed: the build broke");
  const note = { id: "f", line: "Copy this prompt and paste it into your agent", manual: "Write a test that backs the law." };
  const rows = runsNodes({ paused: null, runs: [failed, ended("g", "failed", "Failed")] }, acts([]), note).filter((node) => node.dataset.run);
  assert.match(rows[0].text, /Copy this prompt and paste it into your agent/);
  assert.equal(rows[0].find("textarea")[0].value, "Write a test that backs the law.");
  assert.equal(rows[1].find("textarea").length, 0, "only the run it was copied for");
});

test("a copy tells who keeps the note which run it was for", async () => {
  const noted = [];
  const failed = ended("f", "failed", "Failed: the build broke");
  const [row] = runsNodes({ paused: null, runs: [failed] }, { ...acts([]), noted: (note) => noted.push(note) }).filter((node) => node.dataset.run);
  await button(row, "Copy prompt").listeners.click();
  assert.deepEqual(noted, [{ id: "f", line: "Prompt copied, paste it into your agent", manual: null }]);
});

test("a second click while the copy is on its way adds no second note, and the prompt can be copied again later", async () => {
  let release;
  const slow = () => new Promise((done) => (release = () => done("copied")));
  const failed = ended("f", "failed", "Failed: the build broke");
  const [row] = runsNodes({ paused: null, runs: [failed] }, acts([], slow)).filter((node) => node.dataset.run);
  const copy = button(row, "Copy prompt");
  const first = copy.listeners.click();
  assert.equal(copy.disabled, true, "the button waits for the copy");
  release();
  await first;
  assert.equal(copy.disabled, false);
  const again = copy.listeners.click();
  release();
  await again;
  assert.equal(row.find("p").filter((p) => p.text.includes("Prompt copied")).length, 1, "the new note replaces the old one");
  // A redraw with the note kept leaves the button usable.
  const [redrawn] = runsNodes({ paused: null, runs: [failed] }, acts([]), { id: "f", line: "Prompt copied, paste it into your agent", manual: null }).filter((node) => node.dataset.run);
  assert.notEqual(button(redrawn, "Copy prompt").disabled, true);
});

test("Cancel waits for the server's answer, so a second click asks nothing twice", async () => {
  const calls = [];
  let answer;
  const act = { ...acts(calls), cancel: (id) => (calls.push(["cancel", id]), new Promise((done) => (answer = done))) };
  const [row] = runsNodes({ paused: null, runs: [run("a", "queued")] }, act).filter((node) => node.dataset.run);
  const cancel = button(row, "Cancel");
  const clicked = cancel.listeners.click();
  assert.equal(cancel.disabled, true);
  answer();
  await clicked;
  assert.equal(cancel.disabled, false, "a refused Cancel can be tried again");
  assert.deepEqual(calls, [["cancel", "a"]]);
});

// The status regions under `node`, itself among them.
const all = (node) => [node, ...node.childNodes.filter((child) => child instanceof El).flatMap(all)];
const statuses = (node) => all(node).filter((each) => each.getAttribute("role") === "status");
const later = () => new Promise((done) => setTimeout(done, 150));

test("the runs area says the pause in one status region, drawn empty before its words and kept across redraws (#465)", async () => {
  const box = new El("section");
  const paused = "Claude is not logged in";
  paintRuns(box, { paused: null, runs: [run("a", "running")] }, acts([]));
  const [live] = statuses(box);
  assert.ok(live, "the runs area has a status region");
  assert.equal(live.text, "", "the region is drawn empty, so the pause is a change a screen reader hears");
  paintRuns(box, { paused, runs: [run("a", "running")] }, acts([]));
  assert.deepEqual(statuses(box), [live], "the same region, and the indicator itself is not live");
  assert.equal(live.text, "Background runs are paused. Claude is not logged in.");
  paintRuns(box, { paused, runs: [run("a", "running")] }, acts([]));
  assert.deepEqual(statuses(box), [live], "a redraw keeps the region");
  assert.equal(live.text, "Background runs are paused. Claude is not logged in.");
  // With nothing to show the area is empty, so the side area can hide; the region comes back empty with the next pause, then says it.
  paintRuns(box, { paused: null, runs: [] }, acts([]));
  assert.deepEqual(box.childNodes, []);
  paintRuns(box, { paused, runs: [] }, acts([]));
  assert.deepEqual(statuses(box), [live]);
  assert.equal(live.text, "", "back in the area, the region is empty first");
  await later();
  assert.equal(live.text, "Background runs are paused. Claude is not logged in.");
});

test("a copy says its line in the page's one action region, which a redraw of the runs leaves as it is (#465)", async () => {
  const box = new El("section");
  const failed = ended("f", "failed", "Failed: the build broke");
  let note = null;
  const act = { ...acts([]), noted: (made) => (note = made) };
  paintRuns(box, { paused: null, runs: [failed] }, act);
  const copy = () => button(box.childNodes.find((node) => node.dataset?.run === "f"), "Copy prompt").listeners.click();
  await copy();
  const [live] = statuses(document.body);
  assert.ok(live, "the page has an action region");
  assert.equal(live.text, "", "the region is empty before the words");
  await later();
  assert.equal(live.text, "Prompt copied, paste it into your agent");
  paintRuns(box, { paused: null, runs: [failed] }, act, note);
  assert.match(box.text, /Prompt copied/, "the redrawn row shows the note");
  assert.deepEqual(statuses(box).map((each) => each.text), [""], "the note's line is not live; only the area's region, empty");
  assert.deepEqual(statuses(document.body), [live], "the redraw left the action region alone");
  assert.equal(live.text, "Prompt copied, paste it into your agent");
  // The same words again are heard again: the region empties first.
  await copy();
  assert.equal(live.text, "");
  await later();
  assert.equal(live.text, "Prompt copied, paste it into your agent");
});

test("a second click within the delay drops the first click's words, so only the last are said (#465's review)", async () => {
  const { sayAction } = await import("../../dist/dom.js");
  sayAction("first");
  const [live] = statuses(document.body);
  await new Promise((done) => setTimeout(done, 60));
  sayAction("second");
  await new Promise((done) => setTimeout(done, 70));
  assert.equal(live.text, "", "the first click's words never land after the second click emptied the region");
  await later();
  assert.equal(live.text, "second");
});
