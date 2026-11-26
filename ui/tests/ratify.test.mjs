// Ratifying's pure part (ticket #343), run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { copyOverturn, decideWith, detailLines, evidenceLine, mapCheckLine, noteFor, openRows, overturnPrompt, overturnReport, ratifyStep, headBrings, reviewFlags, reviewLine, sheetLinkItem, sheetPending, siteDrawer, sitesDrawer, statusWords } from "../../dist/ratify.js";

// A row with no detail of its own, as the page reads one whose kind has none.
const row = (kind, id, status = "not-yet", extra = {}) => /** @type {import("../../dist/types.js").RatifyRow} */ ({ pr: 7, kind, id, text: `${id} text`, change: "", status, ...extra });

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
/** @type {import("../../dist/types.js").RatifyDetail} */
const partDetail = {
  of: "part",
  name: words("Server", "Server"),
  purpose: words("Serves the page.", "Serves the page and keeps its state."),
  caption: words("Serves the page", "Serves the page"),
  holds: [
    { name: "Fog", text: words("Greys a part.", "Greys a part a PR changed."), paths_added: ["server/src/fog/"], paths_removed: [] },
    { name: "Same", text: words("Unchanged.", "Unchanged."), paths_added: [], paths_removed: [] },
    { name: "Briefs", text: words(null, "Tells a PR's story."), paths_added: ["server/src/pr_briefs.rs"], paths_removed: [] },
    { name: "Locks", text: words("One writer.", null), paths_added: [], paths_removed: [] },
  ],
  paths_added: ["server/src/"],
  paths_removed: ["src-tauri/"],
};
/** @type {import("../../dist/types.js").RatifyDetail} */
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
  /** @type {import("../../dist/types.js").RatifyDetail} */
  const added = { of: "part", name: words(null, "Install"), purpose: words(null, "Installs the hook."), caption: words(null, "Wires the hook"), holds: [{ name: "Copies", text: words(null, "The hook's copies."), paths_added: [], paths_removed: [] }], paths_added: ["server/src/install.rs"], paths_removed: [] };
  assert.deepEqual(detailLines(added), [
    { label: "Name", before: null, after: "Install" },
    { label: "Caption", before: null, after: "Wires the hook" },
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

test("a changed caption shows as Caption, and a link whose ends swapped as Direction, old beside new (#643)", () => {
  const captioned = { ...partDetail, name: words("Server", "Server"), caption: words("The page", "The page and its state"), holds: [], paths_added: [], paths_removed: [], purpose: words("p", "p") };
  assert.deepEqual(detailLines(captioned), [{ label: "Caption", before: "The page", after: "The page and its state" }]);
  const added = { ...captioned, name: words(null, "Install"), caption: words(null, "Wires the agents"), purpose: words(null, "Installs the hook.") };
  assert.deepEqual(detailLines(added).map((line) => line.label), ["Name", "Caption", "Purpose"], "the caption reads after the name, as on the plate");
  const flipped = { ...linkDetail, kind: words("import", "import"), reason: words("", ""), direction: words("server to ui", "ui to server") };
  assert.deepEqual(detailLines(flipped), [{ label: "Direction", before: "server to ui", after: "ui to server" }]);
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

test("the head's map check line counts its problems, and there is none when the check passes (#641)", () => {
  assert.equal(mapCheckLine([]), null);
  assert.equal(mapCheckLine(undefined), null, "a brief stored before the check");
  assert.equal(mapCheckLine(["stray/s.ts is tracked but no part owns it"]), "The map check fails at this head: 1 problem");
  assert.equal(mapCheckLine(["a", "b", "c"]), "The map check fails at this head: 3 problems");
});

test("a line at Ratify's head brings the step with nothing to ratify; rows and Guard with a law keep their titles (#641)", () => {
  const step = ratifyStep([], ["core"], false, true);
  assert.equal(step?.k, "Ratify");
  assert.equal(step?.h, "The map at this head");
  assert.deepEqual(step?.on, ["core"]);
  assert.equal(ratifyStep([row("law", "l1")], [], false, true)?.h, "Accept or overturn");
  assert.equal(ratifyStep([], [], true, true)?.h, "Guard with a law");
  assert.equal(ratifyStep([], [], false, false), null);
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
  const copy = () => {
    order.push("copy");
    return Promise.resolve("copied");
  };
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

// Link evidence (#499): the import sites an added link's brief stored, read at the PR's head.
const evidence = { head: "0123456789abcdef", sites: [{ file: "alpha/a.ts", line: 3 }, { file: "alpha/c.ts", line: 1 }, { file: "alpha/c.ts", line: 2 }, { file: "beta/Cargo.toml" }] };

test("an added link's second line is one site, then how many more imports across how many files", () => {
  assert.deepEqual(evidenceLine(evidence), { site: "alpha/a.ts:3", more: "(+3 imports across 2 files)" });
  assert.deepEqual(evidenceLine({ head: "h", sites: [{ file: "a.ts", line: 1 }, { file: "a.ts", line: 4 }] }), { site: "a.ts:1", more: "(+1 import across 1 file)" });
  assert.deepEqual(evidenceLine({ head: "h", sites: [{ file: "Cargo.toml" }] }), { site: "Cargo.toml", more: null }, "a site with no line, and nothing more");
  assert.equal(evidenceLine(undefined), null);
  assert.equal(evidenceLine({ head: "h", sites: [] }), null);
});

test("an overturn of an added link quotes the import sites behind it", () => {
  const link = row("link", "alpha-gamma", "not-yet", { text: "alpha to gamma", change: "added", evidence });
  const prompt = overturnPrompt(link, "gamma-calls");
  assert.match(prompt, /link alpha-gamma \(added\): “alpha to gamma”\. Its imports at the PR's head: alpha\/a\.ts:3, alpha\/c\.ts:1, alpha\/c\.ts:2, beta\/Cargo\.toml\. Undo it/);
  assert.doesNotMatch(overturnPrompt(row("link", "a-b", "not-yet", { text: "a to b", change: "removed" }), null), /imports/, "a removed link has no evidence");
});

test("a site's snippet opens in the drawer numbered from its first line, its own line marked", () => {
  const drawer = siteDrawer({ file: "alpha/a.ts", line: 3, first: 1, lines: ["// a", "", "import { g } from \"../gamma/g\";"] }, evidence.head);
  assert.equal(drawer.title, "alpha/a.ts:3");
  assert.equal(drawer.note, "As the PR's head 0123456 holds it");
  assert.deepEqual(drawer.blocks, [{ label: "alpha/a.ts", code: "// a\n\nimport { g } from \"../gamma/g\";", jumps: [], first: 1, mark: 3 }]);
  assert.equal(siteDrawer({ file: "Cargo.toml", first: 1, lines: ["[package]"] }, "h").blocks[0].mark, undefined, "no line, nothing marked");
});

test("the count opens every site in the drawer, each opening its snippet", () => {
  const opened = [];
  const drawer = sitesDrawer({ text: "alpha to gamma", evidence }, (site) => opened.push(site));
  assert.equal(drawer.title, "Imports behind alpha to gamma");
  assert.equal(drawer.note, "4 imports across 3 files, as the PR's head 0123456 holds them");
  assert.deepEqual(drawer.picks.map((pick) => pick.label), ["alpha/a.ts:3", "alpha/c.ts:1", "alpha/c.ts:2", "beta/Cargo.toml"]);
  drawer.picks[1].open();
  assert.deepEqual(opened, [{ file: "alpha/c.ts", line: 1 }]);
});

// The Map text review (#645, spec #634 "Ratify's display"): its verdicts by item id, as `pr_map_review` gives them.
const verdict = (id, flagged, why = `${id} why`, cites = []) => ({ id, verdict: flagged ? "overreach" : "supported", why, cites, flagged });
const review = (state, items = [], line = null, left_out = 0) => ({ head: "cafe", state, items, checked: items.length, flagged: items.filter((each) => each.flagged).length, line, left_out, still_true: [] });

test("a row's review flags are its part's, held items', link's or law's flagged verdicts, each with its field, why and cites (#645)", () => {
  const part = row("part", "server", "not-yet", {
    detail: { of: "part", name: { before: null, after: "Server" }, purpose: { before: null, after: "Serves." }, caption: { before: null, after: null }, holds: [{ name: "Queue", text: { before: null, after: "Every failure is requeued." }, paths_added: [], paths_removed: [] }], paths_added: [], paths_removed: [] },
  });
  const got = review("done", [
    verdict("part:server/purpose", true, "It also writes the page.", ["server/src/lib.rs:41", "server/src/api.rs:9"]),
    verdict("part:server/name", false),
    verdict("held:server/1", true, "Only a timeout is requeued."),
    verdict("part:serverless/purpose", true, "another part's"),
    verdict("link:server-ui/reason", true, "The page never calls it.", ["ui/map.ts:3"]),
    verdict("law:one-queue/text", true, "Two queues run."),
  ]);
  assert.deepEqual(reviewFlags(got, part), [
    "Purpose (overreach): It also writes the page. See server/src/lib.rs:41, server/src/api.rs:9.",
    "Holds Queue (overreach): Only a timeout is requeued.",
  ]);
  assert.deepEqual(reviewFlags(got, row("link", "server-ui")), ["Reason (overreach): The page never calls it. See ui/map.ts:3."]);
  assert.deepEqual(reviewFlags(got, row("law", "one-queue")), ["Text (overreach): Two queues run."]);
  assert.deepEqual(reviewFlags(got, row("decision", "server")), [], "a decision is not map text");
  assert.deepEqual(reviewFlags(review("done", [verdict("part:server/name", false)]), part), [], "an unflagged row shows nothing");
  assert.deepEqual(reviewFlags(null, part), []);
});

test("the review's line at Ratify's head says each state, and offers the button before a review and Run again after a failure (#645)", () => {
  assert.equal(reviewLine(null), null, "a head that changed no map text has none");
  assert.deepEqual(reviewLine(review("idle")), { line: null, start: "Review map text" });
  assert.deepEqual(reviewLine(review("reviewing")), { line: "Reviewing map text…", start: null });
  assert.deepEqual(reviewLine(review("done", [verdict("a", true), verdict("b", false), verdict("c", false)])), { line: "Map text review: 3 checked, 1 flagged", start: null });
  assert.deepEqual(reviewLine(review("failed", [], "the run timed out")), { line: "Map text review failed: the run timed out", start: "Run again" });
  assert.deepEqual(reviewLine(review("failed")), { line: "Map text review failed", start: "Run again" });
  // An item whose id the review does not take is left out of its prompt, and the line says so (#634's review).
  assert.deepEqual(reviewLine(review("done", [verdict("a", false)], null, 1)), { line: "Map text review: 1 checked, 0 flagged, 1 left out for an id it does not take", start: null });
  assert.deepEqual(reviewLine(review("idle", [], null, 2)), { line: "2 left out for an id the review does not take", start: "Review map text" });
});

test("an overturn of a flagged row quotes the review's why, and an unflagged one says nothing of the review (#645)", () => {
  const part = row("part", "server", "not-yet", { text: "Server", change: "edited" });
  const flags = reviewFlags(review("done", [verdict("part:server/purpose", true, "It also writes the page.", ["server/src/lib.rs:41"])]), part);
  const prompt = overturnPrompt({ ...part, flags }, "map-text");
  assert.match(prompt, /The map text review flagged it: Purpose \(overreach\): It also writes the page\. See server\/src\/lib\.rs:41\./);
  assert.match(prompt, /Undo it and tell me what you changed\.$/);
  assert.doesNotMatch(overturnPrompt({ ...part, flags: [] }, null), /review/);
});

test("the Ratify step shows with nothing to ratify while the review runs, has flags, waits for its button or failed, or the map check fails (#645, #641)", () => {
  assert.equal(headBrings(undefined, review("reviewing")), true);
  assert.equal(headBrings([], review("done", [verdict("a", true)])), true);
  assert.equal(headBrings(["a problem"], null), true);
  // "Review map text" and "Run again" are on Ratify's head, so the step shows for them (#634's review).
  assert.equal(headBrings([], review("idle")), true, "a PR not by Daniel's login keeps its button reachable");
  assert.equal(headBrings([], review("failed")), true, "a failed review keeps Run again reachable");
  assert.equal(headBrings([], review("done", [verdict("a", false)], null, 1)), true, "an item left out is said");
  assert.equal(headBrings([], review("done", [verdict("a", false)])), false, "a review with no flags brings nothing");
  assert.equal(headBrings(undefined, null), false);
});
