// The rail opens on "now", as a feed of turns (danielrispler/Falinks#574). Run: `npm test`.
import { test, mock } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { createRequire } from "node:module";
import { connect, fakeDocument, Live, tauri } from "./dom.mjs";

// The page the story draws into (spec #661): live stand-ins by id, so a test reads which nodes a draw kept in the page. The clock is
// the test's, set before the story loads, as times.ts starts its once-a-second count at load when there is a document.
mock.timers.enable({ apis: ["setInterval", "Date"], now: 1_800_000_000_000 });
const page = new Map();
globalThis.document = fakeDocument(
  { getElementById: (id) => page.get(id) ?? page.set(id, connect(document.createElement("div"))).get(id), body: { dataset: {} } },
  Live,
);
globalThis.marked = createRequire(import.meta.url)("marked");
const replies = new Map();
tauri([], (command, args) => replies.get(command)?.(args) ?? null);
const { askedLabel, blockKind, cardTime, feedTurns, foldedLine, foldedTurn, headlineOf, nowOf, oneAtATime, showView, startStory, turnAt } = await import("../../dist/story.js");
const state = await import("../../dist/state.js");
const { setSelection, studio } = state;

test("summaries on open are written one at a time, in the order asked, newest first", async () => {
  const started = [];
  const done = [];
  const ask = oneAtATime((id) => {
    started.push(id);
    return new Promise((resolve) => done.push(resolve));
  });
  for (const id of ["newest", "older", "oldest"]) ask(id);
  assert.deepEqual(started, ["newest"]);
  done[0]();
  await new Promise((resolve) => setTimeout(resolve));
  assert.deepEqual(started, ["newest", "older"]);
  done[1]();
  await new Promise((resolve) => setTimeout(resolve));
  assert.deepEqual(started, ["newest", "older", "oldest"]);
});

// #548: a run that fails is caught, and the queue goes on to the next id.
test("a summary whose write throws leaves no unhandled rejection and the next one still runs", async () => {
  const unhandled = [];
  const note = (reason) => unhandled.push(String(reason));
  process.on("unhandledRejection", note);
  const started = [];
  const ask = oneAtATime((id) => {
    started.push(id);
    return id === "bad" ? Promise.reject(new Error("no writer")) : Promise.resolve();
  });
  ask("bad");
  ask("next");
  await new Promise((resolve) => setTimeout(resolve, 5));
  process.off("unhandledRejection", note);
  assert.deepEqual(started, ["bad", "next"]);
  assert.deepEqual(unhandled, []);
});

const html = fs.readFileSync(new URL("../index.html", import.meta.url), "utf8");
const rail = html.slice(html.indexOf('<aside class="rail"'), html.indexOf("</aside>"));
// A turn, a question and a summary as the core sends them, less the fields the feed never reads.
const turn = (started_at, prompt) => /** @type {import("../../dist/types.js").Turn} */ ({ started_at, ended_at: null, prompt, id: null, message: null });
/** @param {Partial<import("../../dist/types.js").Question>} fields */
const question = (fields) => /** @type {import("../../dist/types.js").Question} */ (fields);

test("the rail is the dock, the intention and the feed; the newest card is inside its turn", () => {
  const order = ["dock", "now-asked", "intention", "feed"].map((id) => rail.indexOf(`id="${id}"`));
  assert.ok(order.every((at) => at > 0));
  assert.deepEqual([...order].sort((a, b) => a - b), order);
  assert.ok(!rail.includes('id="now-card"'));
});

test("an event id's time places it in a turn, as a design line's citation of an older card does", () => {
  const turns = [turn(10, "a"), turn(20, "b")];
  assert.equal(cardTime("477b6b89-5461@1793715833094"), 1793715833094);
  assert.equal(turnAt(turns, 5), null);
  assert.equal(turnAt(turns, 10), 0);
  assert.equal(turnAt(turns, 25), 1);
});

test("turns render newest first, each with its subagents and questions", () => {
  const turns = [turn(10, "Fix the parser"), turn(20, "worker w1 returned"), turn(30, "Open the PR")];
  const agents = [/** @type {import("../../dist/types.js").Subagent} */ ({ id: "w1", started_at: 11, turn: 0 })];
  const questions = [question({ tool_use_id: "q1", answer: "not_answered", turn: 1 })];
  const feed = feedTurns(turns, { agents, questions });
  assert.deepEqual(feed.map((b) => b.turn.prompt), ["Open the PR", "worker w1 returned", "Fix the parser"]);
  assert.deepEqual(feed[1].questions.map((q) => q.tool_use_id), ["q1"]);
  assert.deepEqual(feed[2].agents.map((a) => a.id), ["w1"]);
});

test("what started in no turn, such as a subagent's question, goes in one last block", () => {
  const feed = feedTurns([], { agents: [], questions: [question({ tool_use_id: "q1", answer: "not_answered", turn: null })] });
  assert.equal(feed.length, 1);
  assert.equal(feed[0].turn, null);
  assert.deepEqual(feed[0].questions.map((q) => q.tool_use_id), ["q1"]);
  assert.deepEqual(feedTurns([], { agents: [], questions: [] }), []);
});

const summary = /** @type {import("../../dist/types.js").TurnSummary} */ ({ summary: { summary: "Fixed the lint.\nThe test reads the token.\nNot checked: the strip.", quiz: null } });

test("a stored summary is a Headline and reported lines; with none, the final response's first line", () => {
  assert.deepEqual(headlineOf(summary, "Fixed it, mostly."), {
    headline: "Fixed the lint.",
    body: [
      { kind: "reported", text: "The test reads the token." },
      { kind: "reported", text: "Not checked: the strip." },
    ],
  });
  assert.deepEqual(headlineOf(null, "Fixed it, mostly."), { headline: "Fixed it, mostly.", body: [] });
  assert.deepEqual(headlineOf(null, null), { headline: null, body: [] });
});

test("a summary with a Headline and typed lines gives them as they are, and a stripped Headline gives the response's first line", () => {
  const typed = (headline) => /** @type {import("../../dist/types.js").TurnSummary} */ ({
    summary: { summary: "x", headline, lines: [{ kind: "open", text: "Nobody looked." }, { kind: "ask", text: "Merge it?" }], quiz: null },
  });
  assert.deepEqual(headlineOf(typed("Agent says: fixed the overlay."), "Done."), {
    headline: "Agent says: fixed the overlay.",
    body: [
      { kind: "open", text: "Nobody looked." },
      { kind: "ask", text: "Merge it?" },
    ],
  });
  assert.equal(headlineOf(typed(""), "Done.").headline, "Done.");
});

test("what now holds in each state", () => {
  const turn = { prompt: "Fix it", message: "Done.", intention: "Reading the lint" };
  // Running: what was asked and the intention in its box; no Headline yet.
  assert.deepEqual(nowOf("running", turn, null), { asked: "Fix it", intention: "box", headline: null, body: [], soFar: false, fullResponse: false });
  // Needs you: the intention is small, as why it asks, and the turn's facts so far.
  assert.deepEqual(nowOf("needs_you", turn, null), { asked: null, intention: "why", headline: null, body: [], soFar: true, fullResponse: false });
  // Your turn: what was asked, the Headline and Body lines, and the full response; no intention box.
  const done = nowOf("your_turn", turn, summary);
  assert.equal(done.asked, "Fix it");
  assert.equal(done.intention, null);
  assert.equal(done.headline, "Fixed the lint.");
  assert.equal(done.body.length, 2);
  assert.ok(done.fullResponse);
  // Idle and ended read like your turn; with no summary, the response's first line is the Headline.
  for (const state of /** @type {const} */ (["idle", "ended"])) assert.equal(nowOf(state, turn, null).headline, "Done.");
});

test("the block a session needs Daniel for is named by its kind", () => {
  assert.equal(blockKind([], null), null);
  assert.equal(blockKind([{ answer: "open" }], null), "question");
  assert.equal(blockKind([{ answer: "not_answered" }], null), null);
  assert.equal(blockKind([], { tool_name: "ExitPlanMode" }), "plan");
  assert.equal(blockKind([], { tool_name: "Bash" }), "permission");
});

test("a folded turn's line holds the prompt and Headline, and a mark for a failed check", () => {
  const run = (status) => ({ suite: "check", status, retries: 0, runs: [] });
  const line = (checks, headline = "Fixed the parser") => foldedLine({ prompt: "  Fix the parser\nand the lexer", checks }, headline);
  assert.deepEqual(line([]), { prompt: "Fix the parser", headline: "Fixed the parser", failed: false });
  assert.deepEqual(line([run("failed")]), { prompt: "Fix the parser", headline: "Fixed the parser", failed: true });
  // A failure the agent fixed ends as passed, with its retry: no mark.
  assert.equal(line([{ ...run("passed"), retries: 1 }]).failed, false);
  assert.deepEqual(line([run("ran")], null), { prompt: "Fix the parser", headline: null, failed: false });
  assert.equal(foldedLine({ prompt: "", checks: [] }, null).prompt, "No prompt");
});

// A session as the server sends it, with two turns: an older one, folded, and the newest, at your turn.
const NOW = 1_800_000_000_000;
const ended = (n, fields = {}) => ({ started_at: NOW - (10 - n) * 60e3, ended_at: NOW - (9 - n) * 60e3, prompt: `Prompt ${n}`, id: `t${n}`, message: `Reply ${n}.\nMore of reply ${n}.`, checks: [], listed: [], files: { files: [], merged: 0 }, ...fields });
/** @returns {any} */
const sessionOf = (fields = {}) => ({
  id: "s1", capabilities: { agent: "claude", asks: true, replies: true, turn_trees: true, permission_mode: true }, model: null, cwd: "/w/p", folder_name: "p",
  terminal: null, status_only: false, shell: false, name: "Story", prompt_name: null, resume_id: "r", permission_mode: null, started_mode: null, cleared: [],
  asks_before_closing: false, landed: false, last_step: null, started_at: NOW - 3600e3, last_event_at: NOW, turn_span: null, step_span: null, state: "your_turn",
  agents: [], asker: null, prompts: [], turns: [ended(1), ended(2)], questions: [], blocked_commands: [], returned: null,
  transcript: { intention: null, steps_ago: 0, unavailable: null, responses: ["Reply 2.\nMore of reply 2."], effort: null, context: null },
  identity: { issue: null, pr: null, name: "p" }, work: { kind: null, phases: [], agent_ms: 0, estimate: { range: null, depends_on_you: false } }, seen: 2, since_looked: { turns: 0 },
  ...fields,
});
const draw = (session) => {
  const selected = { session, agent: null };
  setSelection(selected);
  showView(selected);
};
/** @returns {any} */
const feed = () => document.getElementById("feed");
const every = (node, out = []) => {
  out.push(node);
  for (const child of node.childNodes ?? []) if (child.nodeType === 1) every(child, out);
  return out;
};
const kept = (before, after) => before.filter((node, i) => node === after[i]).length;

test("two draws of the same payload leave the feed's turns, folds and buttons the same nodes", () => {
  draw(sessionOf());
  const before = every(feed());
  const turns = [...feed().childNodes];
  assert.equal(turns.length, 2, "the newest turn and one folded turn");
  draw(sessionOf());
  const after = every(feed());
  assert.deepEqual([...feed().childNodes], turns);
  for (const tag of ["details", "button", "li"]) {
    const was = before.filter((node) => node.tag === tag);
    assert.ok(was.length > 0, `the feed draws a ${tag}`);
    assert.deepEqual(after.filter((node) => node.tag === tag), was, `each ${tag} is the node it was`);
  }
  assert.equal(kept(before, after), before.length);
});

const settle = async () => {
  for (let n = 0; n < 5; n++) await new Promise((resolve) => setImmediate(resolve));
};

test("a draw after a turn's summary arrives swaps only that turn's node", async () => {
  /** @type {(value: unknown) => void} */
  let arrive = () => {};
  replies.set("turn_summary", ({ id }) => (id === "u2" ? new Promise((resolve) => (arrive = resolve)) : null));
  const session = sessionOf({ id: "s2", turns: [ended(1, { id: "u1" }), ended(2, { id: "u2" })] });
  draw(session);
  const [newest, past] = feed().childNodes;
  const pastButton = past.querySelector("button");
  await settle();
  arrive({ summary: { summary: "x", headline: "Fixed the parser.", lines: [], quiz: null } });
  await settle();
  draw(session);
  assert.notEqual(feed().childNodes[0], newest, "the turn whose summary arrived is drawn anew");
  assert.equal(feed().childNodes[0].querySelector("p.headline-text")?.innerHTML.includes("Fixed the parser."), true);
  assert.equal(feed().childNodes[1], past, "the other turn is the node it was");
  assert.equal(past.querySelector("button"), pastButton);
  replies.delete("turn_summary");
});

test("a counting time is the same node across draws, and keeps counting", () => {
  const running = (at) => ({ statement: "cargo test", status: "running", started_at: at, ended_at: null, typical_ms: null });
  const start = Date.now() - 30e3;
  const session = sessionOf({ id: "s3", state: "running", turns: [ended(1, { id: "v1" }), ended(2, { id: "v2", ended_at: null, listed: [running(start)] })] });
  draw(session);
  const time = feed().querySelector("span.check-time");
  assert.equal(time?.textContent, "30s");
  draw(session);
  assert.equal(feed().querySelector("span.check-time"), time, "the draw keeps the counting node");
  mock.timers.tick(1000);
  assert.equal(time.textContent, "31s", "the kept node counts on");
});

test("the focus in an Ask box, an open fold and the banner's open Brief survive a draw", () => {
  const { setCards } = /** @type {any} */ (state);
  setCards({ brief: null, goal: "Ship the story panel", answers: [], open: [], design: [], decisions: [], risk_notes: [], decision_notes: [] });
  const session = sessionOf({ id: "s4", state: "idle", turns: [ended(1, { id: "w1" }), ended(2, { id: "w2" })] });
  draw(session);
  const box = feed().querySelector("details.ask-about");
  assert.ok(box, "the newest turn's Headline has its Ask box");
  box.open = true;
  const field = box.querySelector("textarea");
  field.focus();
  assert.equal(document.activeElement, field);
  const response = feed().querySelector("details.fold");
  response.open = true;
  response.listeners.toggle?.({});
  /** @type {any} */
  const slot = document.getElementById("banner-slot");
  const brief = slot.querySelector("details.banner-brief");
  assert.ok(brief, "the banner holds the Brief");
  brief.open = true;
  brief.listeners.toggle?.({});
  draw({ ...session, last_event_at: session.last_event_at + 1 });
  assert.equal(document.activeElement, field, "the focus stays in the question box");
  assert.ok(box.isConnected && box.open);
  assert.equal(feed().querySelector("details.fold"), response);
  assert.ok(response.open, "the open fold stays open");
  assert.equal(slot.querySelector("details.banner-brief"), brief);
  assert.ok(brief.open, "the banner's Brief stays open");
});

// #675: a turn a wake-up opened is the returning task's, never Daniel's prompt; a turn stored before the flag reads as his.
test("a woken turn reads \"Woken by\" with the returning task; Daniel's turn reads \"You asked\"", () => {
  assert.equal(askedLabel({ wake_up: true }), "Woken by");
  assert.equal(askedLabel({ wake_up: false }), "You asked");
  assert.equal(askedLabel({}), "You asked");

  studio.selection = null;
  const past = (turn) =>
    foldedTurn(
      /** @type {import("../../dist/types.js").Session} */ ({ id: "s" }),
      /** @type {import("../../dist/story.js").FeedTurn} */ (/** @type {unknown} */ ({ index: 0, turn: { id: null, message: null, checks: [], ...turn }, cards: [] })),
    );
  // A past row's prompt column: the returning task, marked as a wake-up; Daniel's prompt as it is.
  const promptOf = (node) => node.querySelector(".prompt").textContent;
  assert.equal(promptOf(past({ prompt: "worker bqv59 returned", wake_up: true })), "Woken by worker bqv59 returned");
  assert.equal(promptOf(past({ prompt: "Fix the parser" })), "Fix the parser");
});

// Spec #661, Q32: a Headline taken from the reply's first line is not shown again at the top of Full response.
const responseOf = (turn) => [...turn.querySelectorAll("details.fold")].find((fold) => fold.querySelector("summary")?.textContent === "Full response") ?? null;
const responseText = (fold) => fold.querySelectorAll("div.response").map((div) => div.innerHTML).join("\n");

test("the newest turn's Full response leaves out the first line its Headline already shows", () => {
  const reply = "Fixed the parser.\n\nThe lexer now reads tabs.";
  const session = sessionOf({ id: "f1", turns: [ended(1, { id: "x1" }), ended(2, { id: "x2", message: "Fixed the parser." })], transcript: { ...sessionOf().transcript, responses: ["Looking at the lexer first.", reply] } });
  draw(session);
  const fold = responseOf(feed().childNodes[0]);
  assert.ok(fold, "the newest turn has its Full response");
  const shown = responseText(fold);
  assert.ok(!shown.includes("Fixed the parser."), "the Headline's line is left out");
  assert.ok(shown.includes("The lexer now reads tabs."));
  assert.ok(shown.includes("Looking at the lexer first."), "the turn's earlier texts stay");
});

test("the newest turn has no Full response when its reply was only its Headline", () => {
  const session = sessionOf({ id: "f2", turns: [ended(1, { id: "y1" }), ended(2, { id: "y2", message: "Done." })], transcript: { ...sessionOf().transcript, responses: ["Done."] } });
  draw(session);
  assert.equal(responseOf(feed().childNodes[0]), null);
});

test("an older turn's Full response, fetched on opening, leaves out the Headline's line, or says the reply was only that", async () => {
  const fetched = { 0: "Renamed the flag.\nIt reads wake_up now.", 1: "Only this." };
  replies.set("turn_reply", ({ index }) => fetched[index]);
  const turns = [ended(1, { id: "z1", message: "Renamed the flag." }), ended(2, { id: "z2", message: "Only this." }), ended(3, { id: "z3" })];
  const session = sessionOf({ id: "f3", turns, transcript: { ...sessionOf().transcript, responses: ["Reply 3."] } });
  draw(session);
  for (const row of [...feed().childNodes].slice(1)) row.querySelector("button").listeners.click?.({});
  const open = (row) => {
    const fold = responseOf(row);
    assert.ok(fold, "an older turn keeps its fold until its reply is fetched");
    fold.open = true;
    fold.listeners.toggle?.({});
    return fold;
  };
  const [, second, first] = feed().childNodes;
  const [only, renamed] = [open(second), open(first)];
  await settle();
  assert.ok(!responseText(renamed).includes("Renamed the flag."), "the Headline's line is left out");
  assert.ok(responseText(renamed).includes("It reads wake_up now."));
  assert.equal(only.querySelector("p.response-only")?.textContent, "The reply was only its Headline.");
  replies.delete("turn_reply");
});

test("a stored Headline leaves Full response whole", async () => {
  replies.set("turn_summary", ({ id }) => (id === "h2" ? { summary: { summary: "x", headline: "Parser fixed.", lines: [], quiz: null } } : null));
  const session = sessionOf({ id: "f4", turns: [ended(1, { id: "h1" }), ended(2, { id: "h2", message: "Fixed the parser." })], transcript: { ...sessionOf().transcript, responses: ["Fixed the parser.\nThe lexer reads tabs."] } });
  draw(session);
  await settle();
  draw(session);
  const shown = responseText(responseOf(feed().childNodes[0]));
  assert.ok(shown.includes("Fixed the parser."), "the reply's first line stays");
  assert.ok(shown.includes("The lexer reads tabs."));
  replies.delete("turn_summary");
});

// Spec #661, Q33: Brief and Decisions sit at the panel's foot, so opening one brings it into view, to the nearest edge.
test("opening Brief or Decisions scrolls it into view, smoothly unless reduced motion is asked for", () => {
  let reduce = false;
  globalThis.matchMedia = (query) => /** @type {MediaQueryList} */ (/** @type {unknown} */ ({ matches: query.includes("no-preference") ? !reduce : reduce }));
  startStory(() => {});
  for (const id of ["brief-fold", "decisions"]) {
    /** @type {any} */
    const fold = document.getElementById(id);
    for (const [still, behavior] of /** @type {[boolean, string][]} */ ([[false, "smooth"], [true, "auto"]])) {
      reduce = still;
      fold.scrolledIntoView = null;
      fold.open = true;
      fold.listeners.toggle?.({});
      assert.deepEqual(fold.scrolledIntoView, { block: "nearest", behavior }, `${id} opened with reduced motion ${still}`);
      fold.open = false;
      fold.scrolledIntoView = null;
      fold.listeners.toggle?.({});
      assert.equal(fold.scrolledIntoView, null, `${id} closing does not scroll`);
    }
  }
  delete (/** @type {any} */ (globalThis).matchMedia);
});
