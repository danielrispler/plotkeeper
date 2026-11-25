// Ratifying's pure part (ticket #343), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { copyOverturn, decideWith, detailLines, noteFor, openRows, overturnPrompt, overturnReport, ratifyStep, sheetLinkItem, sheetPending, statusWords } from "../../dist/ratify.js";

const row = (kind, id, status = "not-yet", extra = {}) => ({ pr: 7, kind, id, text: `${id} text`, change: "", status, ...extra });

test("the sheet marks a law and a link at the part that are not yet ratified, with the PR that changed them", () => {
  const links = [{ id: "a-b", from: "a", to: "b" }, { id: "c-d", from: "c", to: "d" }, { id: "x-a", from: "x", to: "a" }];
  const pending = [{ pr: 9, kind: "law", id: "l1" }, { pr: 8, kind: "link", id: "a-b" }, { pr: 8, kind: "link", id: "c-d" }, { pr: 5, kind: "part", id: "a" }];
  const found = sheetPending(pending, "a", links);
  assert.deepEqual([...found.laws], [["l1", 9]]);
  assert.deepEqual(found.links.map((l) => [l.id, l.pr]), [["a-b", 8]], "only links at this part, only the unratified");
  assert.equal(sheetPending([], "a", links).laws.size, 0);
});

test("the overturn prompt stands alone: the PR, its branch while it is open, the item's id and its wording", () => {
  const law = row("law", "l1", "not-yet", { text: "The hook never waits.", change: "changed" });
  const open = overturnPrompt(law, "fog-on-the-map");
  assert.match(open, /PR #7/);
  assert.match(open, /branch fog-on-the-map/);
  assert.match(open, /law l1 \(changed\): “The hook never waits\.”/);
  assert.match(open, /Undo it/);
  const merged = overturnPrompt(law, null);
  assert.match(merged, /PR #7/);
  assert.doesNotMatch(merged, /branch/, "a PR that is not open has no branch to name");
  const decision = overturnPrompt(row("decision", "d1", "not-yet", { text: "Briefs are kept per project." }), null);
  assert.match(decision, /decision d1: “Briefs are kept per project\.”/);
  assert.match(decision, /PR #7/);
});

const words = (before, after) => ({ before, after });
const partDetail = {
  of: "part",
  name: words("Server", "Server"),
  purpose: words("Serves the page.", "Serves the page and keeps its state."),
  holds: [
    { name: "Fog", text: words("Greys a part.", "Greys a part a PR changed."), paths_added: ["server/src/fog/"], paths_removed: [] },
    { name: "Same", text: words("Unchanged.", "Unchanged."), paths_added: [], paths_removed: [] },
    { name: "Briefs", text: words(null, "Tells a PR's story."), paths_added: ["server/src/pr_briefs.rs"], paths_removed: [] },
    { name: "Locks", text: words("One writer.", null), paths_added: [], paths_removed: [] },
  ],
  paths_added: ["server/src/"],
  paths_removed: ["src-tauri/"],
};
const linkDetail = { of: "link", kind: words("import", "http"), reason: words("", "The page calls the server over HTTP.") };

test("an overturn of a part quotes its new purpose and its held items' names, and of a link its ends, kind and reason", () => {
  const part = overturnPrompt(row("part", "server", "not-yet", { text: "Server", change: "changed", detail: partDetail }), null);
  assert.match(part, /part server \(changed\): “Server”/);
  assert.match(part, /purpose: “Serves the page and keeps its state\.”/);
  assert.match(part, /holds Fog, Same, Briefs\./, "the new side's held items, not the removed one");
  const link = overturnPrompt(row("link", "ui-server", "not-yet", { text: "ui to server", change: "changed", detail: linkDetail }), null);
  assert.match(link, /link ui-server \(changed\): “ui to server”/);
  assert.match(link, /kind http, reason “The page calls the server over HTTP\.”/);
  const removed = overturnPrompt(row("part", "old", "not-yet", { text: "Old", change: "removed", detail: { ...partDetail, purpose: words("Gone.", null), holds: [] } }), null);
  assert.doesNotMatch(removed, /purpose/, "a removed part has no new text to quote");
});

test("an overturn from the sheet quotes a link's ends as the brief does, its kind and its reason", () => {
  const item = sheetLinkItem({ id: "ui-server", from: "ui", to: "server", kind: "http", reason: "The page calls the server over HTTP." }, 8);
  const prompt = overturnPrompt(item, null);
  assert.match(prompt, /PR #8/);
  assert.match(prompt, /link ui-server: “ui to server”/, "the brief's text: the ends by id");
  assert.match(prompt, /kind http, reason “The page calls the server over HTTP\.”/);
  assert.equal(item.text, "ui to server");
  assert.match(overturnPrompt(sheetLinkItem({ id: "a-b", from: "a", to: "b" }, 8), null), /link a-b: “a to b”, kind none\. Undo it/);
});

test("an overturn's sentence ends with one full stop, whatever it quotes last", () => {
  const ends = (r) => overturnPrompt(r, null).replace(/ Undo it and tell me what you changed\.$/, "");
  assert.match(ends(row("law", "l1", "not-yet", { text: "The hook never waits" })), /“The hook never waits”\.$/);
  assert.match(ends(row("law", "l1", "not-yet", { text: "The hook never waits." })), /waits\.”$/, "no second stop after a quote's own");
  const reason = { of: "link", kind: words(null, "http"), reason: words(null, "The page calls the server") };
  assert.match(ends(row("link", "a-b", "not-yet", { text: "a to b", detail: reason })), /reason “The page calls the server”\.$/);
  assert.match(ends(row("link", "a-b", "not-yet", { text: "a to b", detail: { ...reason, reason: words(null, "") } })), /kind http\.$/);
  const bare = { ...partDetail, purpose: words(null, "Serves the page"), holds: [] };
  assert.match(ends(row("part", "server", "not-yet", { text: "Server", detail: bare })), /purpose: “Serves the page”\.$/);
  assert.match(ends(row("part", "server", "not-yet", { text: "Server", detail: { ...bare, holds: partDetail.holds } })), /holds Fog, Same, Briefs\.$/);
});

test("a detail shows only what changed, old beside new, and an added part shows everything with nothing before", () => {
  assert.deepEqual(detailLines(partDetail), [
    { label: "Purpose", before: "Serves the page.", after: "Serves the page and keeps its state." },
    { label: "Paths", before: "src-tauri/", after: "server/src/", code: true },
    { label: "Holds Fog", before: "Greys a part.", after: "Greys a part a PR changed." },
    { label: "Fog: paths", before: null, after: "server/src/fog/", code: true },
    { label: "Holds Briefs", before: null, after: "Tells a PR's story." },
    { label: "Briefs: paths", before: null, after: "server/src/pr_briefs.rs", code: true },
    { label: "Holds Locks", before: "One writer.", after: null },
  ]);
  const added = { of: "part", name: words(null, "Install"), purpose: words(null, "Installs the hook."), holds: [{ name: "Copies", text: words(null, "The hook's copies."), paths_added: [], paths_removed: [] }], paths_added: ["server/src/install.rs"], paths_removed: [] };
  assert.deepEqual(detailLines(added), [
    { label: "Name", before: null, after: "Install" },
    { label: "Purpose", before: null, after: "Installs the hook." },
    { label: "Paths", before: null, after: "server/src/install.rs", code: true },
    { label: "Holds Copies", before: null, after: "The hook's copies." },
  ]);
  assert.deepEqual(detailLines(linkDetail), [
    { label: "Kind", before: "import", after: "http", code: true },
    { label: "Reason", before: "", after: "The page calls the server over HTTP." },
  ]);
  assert.deepEqual(detailLines(null), []);
});

test("an import link added or removed without a reason shows no Reason line", () => {
  assert.deepEqual(detailLines({ of: "link", kind: words(null, "import"), reason: words(null, "") }), [{ label: "Kind", before: null, after: "import", code: true }]);
  assert.deepEqual(detailLines({ of: "link", kind: words("import", null), reason: words("", null) }), [{ label: "Kind", before: "import", after: null, code: true }]);
});

test("an overturn of a part with an empty purpose quotes no empty purpose", () => {
  const empty = { ...partDetail, purpose: words(null, "") };
  const prompt = overturnPrompt(row("part", "server", "not-yet", { text: "Server", detail: empty }), null);
  assert.doesNotMatch(prompt, /purpose/);
  assert.match(prompt, /“Server”, and it holds Fog, Same, Briefs\. Undo it/);
});

test("copying the prompt says so, and a clipboard that fails or is missing leaves the prompt to copy by hand", async () => {
  const written = [];
  assert.equal(await copyOverturn("the prompt", async (text) => void written.push(text)), "copied");
  assert.deepEqual(written, ["the prompt"]);
  assert.equal(await copyOverturn("the prompt", async () => Promise.reject(new Error("denied"))), "manual");
  assert.equal(await copyOverturn("the prompt", () => { throw new Error("blocked"); }), "manual");
  assert.equal(await copyOverturn("the prompt", undefined), "manual");
  assert.deepEqual(overturnReport("copied", "the prompt"), { line: "Prompt copied, paste it into your agent", manual: null });
  assert.deepEqual(overturnReport("manual", "the prompt"), { line: "Copy this prompt and paste it into your agent", manual: "the prompt" });
});

test("the ratify step exists only when there is something to ratify, and rows say where they stand", () => {
  assert.equal(ratifyStep([]), null);
  const step = ratifyStep([row("law", "l1")]);
  assert.equal(step.k, "Ratify");
  assert.deepEqual(ratifyStep([row("law", "l1")], ["shell"]).on, ["shell"], "it keeps the parts the brief lit");
  assert.deepEqual(openRows([row("law", "l1"), row("law", "l2", "accepted"), row("decision", "d1", "ratified")]).map((r) => r.id), ["l1"]);
  assert.equal(statusWords["not-yet"], "Not yet ratified");
});

test("a brief with Guard with a law ends with the step even with nothing to ratify, before and after its PR merges", () => {
  const step = ratifyStep([], ["core"], true);
  assert.equal(step.k, "Ratify");
  assert.equal(step.h, "Guard with a law");
  assert.equal(ratifyStep([row("law", "l1")], [], true).h, "Accept or overturn", "beside Ratify's items");
  assert.equal(ratifyStep([], [], false), null);
});

test("an overturn's note shows only where its item is drawn: its project, and the item's PR, kind and id", () => {
  const last = { project: "/p", pr: 7, kind: "law", id: "l1", line: "Prompt copied, paste it into your agent", manual: null };
  const brief = (pr, rows) => (item) => rows.some((r) => item.pr === pr && r.kind === item.kind && r.id === item.id);
  assert.equal(noteFor(null, "/p", () => true), null);
  assert.equal(noteFor(last, "/p", brief(7, [{ kind: "law", id: "l1" }])), last);
  assert.equal(noteFor(last, "/q", brief(7, [{ kind: "law", id: "l1" }])), null, "another project");
  assert.equal(noteFor(last, "/p", brief(8, [{ kind: "law", id: "l1" }])), null, "another PR's brief");
  assert.equal(noteFor(last, "/p", brief(7, [{ kind: "link", id: "l1" }])), null, "another item");
  assert.equal(noteFor(last, "/p", () => false), null, "another part's sheet");
});

test("an overturn copies its prompt within the click, before the decision's round trip, and Accept copies nothing", async () => {
  const order = [];
  const copy = async () => (order.push("copy"), "copied");
  const record = async () => {
    order.push("record");
    await new Promise((resolve) => setTimeout(resolve, 1));
  };
  assert.equal(await decideWith(true, copy, record), "copied");
  assert.deepEqual(order, ["copy", "record"], "the clipboard is written before anything is awaited");
  order.length = 0;
  assert.equal(await decideWith(false, copy, record), null);
  assert.deepEqual(order, ["record"]);
});
