// The rail opens on "now", as a feed of turns (danielrispler/Falinks#574). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { blockKind, cardTime, feedTurns, foldedLine, headlineOf, nowOf, oneAtATime, turnAt } from "../../dist/story.js";

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

const html = fs.readFileSync(new URL("../index.html", import.meta.url), "utf8");
const rail = html.slice(html.indexOf('<aside class="rail"'), html.indexOf("</aside>"));
const card = (at) => ({ request: { id: `s@${at}` } });
const turn = (started_at, prompt) => ({ started_at, ended_at: null, prompt, id: null, message: null });

test("the rail is the dock, the intention and the feed; the newest card is inside its turn", () => {
  const order = ["dock", "now-asked", "intention", "feed"].map((id) => rail.indexOf(`id="${id}"`));
  assert.ok(order.every((at) => at > 0));
  assert.deepEqual([...order].sort((a, b) => a - b), order);
  assert.ok(!rail.includes('id="now-card"'));
});

test("a card belongs to the turn it was written in, by its id's time", () => {
  const turns = [turn(10, "a"), turn(20, "b")];
  assert.equal(cardTime("477b6b89-5461@1793715833094"), 1793715833094);
  assert.equal(turnAt(turns, 5), null);
  assert.equal(turnAt(turns, 10), 0);
  assert.equal(turnAt(turns, 25), 1);
});

test("turns render newest first, each with its cards, subagents and questions", () => {
  const turns = [turn(10, "Fix the parser"), turn(20, "worker w1 returned"), turn(30, "Open the PR")];
  const agents = [{ id: "w1", started_at: 11, turn: 0 }];
  const questions = [{ tool_use_id: "q1", answer: "not_answered", turn: 1 }];
  const feed = feedTurns(turns, { cards: [card(15), card(35)], agents, questions });
  assert.deepEqual(feed.map((b) => b.turn.prompt), ["Open the PR", "worker w1 returned", "Fix the parser"]);
  assert.deepEqual(feed[0].cards.map((c) => c.request.id), ["s@35"]);
  assert.deepEqual(feed[1].questions.map((q) => q.tool_use_id), ["q1"]);
  assert.deepEqual(feed[2].agents.map((a) => a.id), ["w1"]);
  assert.deepEqual(feed[2].cards.map((c) => c.request.id), ["s@15"]);
});

test("what started in no turn, such as a subagent's cards, goes in one last block", () => {
  const feed = feedTurns([], { cards: [card(1), card(2)], agents: [], questions: [] });
  assert.equal(feed.length, 1);
  assert.equal(feed[0].turn, null);
  assert.deepEqual(feed[0].cards.map((c) => c.request.id), ["s@2", "s@1"]);
  assert.deepEqual(feedTurns([], { cards: [], agents: [], questions: [] }), []);
});

const summary = { summary: { summary: "Fixed the lint.\nThe test reads the token.\nNot checked: the strip.", quiz: null } };

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
  const typed = (headline) => ({
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
  for (const state of ["idle", "ended"]) assert.equal(nowOf(state, turn, null).headline, "Done.");
});

test("the block a session needs Daniel for is named by its kind", () => {
  assert.equal(blockKind([], null), null);
  assert.equal(blockKind([{ answer: "open" }], null), "question");
  assert.equal(blockKind([{ answer: "not_answered" }], null), null);
  assert.equal(blockKind([], { tool_name: "ExitPlanMode" }), "plan");
  assert.equal(blockKind([], { tool_name: "Bash" }), "permission");
});

test("a folded turn's line holds the prompt and Headline, a dot for an open Quiz, a mark for a failed check", () => {
  const run = (status) => ({ suite: "check", status, retries: 0, runs: [] });
  const line = (checks, open, headline = "Fixed the parser") =>
    foldedLine({ prompt: "  Fix the parser\nand the lexer", checks }, headline, open);
  assert.deepEqual(line([], true), { prompt: "Fix the parser", headline: "Fixed the parser", unanswered: true, failed: false });
  assert.deepEqual(line([run("failed")], false), { prompt: "Fix the parser", headline: "Fixed the parser", unanswered: false, failed: true });
  // A failure the agent fixed ends as passed, with its retry: no mark.
  assert.equal(line([{ ...run("passed"), retries: 1 }], false).failed, false);
  assert.deepEqual(line([run("ran")], false, null), { prompt: "Fix the parser", headline: null, unanswered: false, failed: false });
  assert.equal(foldedLine({ prompt: "", checks: [] }, null, false).prompt, "No prompt");
});

// A blocked command's card with no button is in the turn of its latest step: its retry's start, else its block.
const blocked = (call, at, extra = {}) => ({ call, command: "ls", cwd: "/r", reason: "r", agent: null, at, status: "allowed", ...extra });
const placedCalls = (feed) => feed.map((b) => [b.index, b.blocked.map((c) => c.call)]);

test("a blocked card is in the turn it was blocked in, also once its retry ran in that turn", () => {
  const turns = [turn(10, "a"), turn(20, "b")];
  const cards = [blocked("c1", 12), blocked("c2", 25, { status: "finished", run_at: 27 })];
  const feed = feedTurns(turns, { cards: [], agents: [], questions: [], blocked: cards });
  assert.deepEqual(placedCalls(feed), [[1, ["c2"]], [0, ["c1"]]]);
});

test("a late Allow whose retry starts in a later turn puts the card in the later turn", () => {
  const turns = [turn(10, "a"), turn(20, "b")];
  const card = blocked("c1", 12, { status: "running", run_at: 22 });
  const feed = feedTurns(turns, { cards: [], agents: [], questions: [], blocked: [card] });
  assert.deepEqual(placedCalls(feed), [[1, ["c1"]], [0, []]]);
});

test("a blocked card with no turn goes in the last block, as a subagent's view shows its cards", () => {
  const feed = feedTurns([], { cards: [], agents: [], questions: [], blocked: [blocked("c1", 5), blocked("c2", 6)] });
  assert.equal(feed.length, 1);
  assert.equal(feed[0].turn, null);
  assert.deepEqual(feed[0].blocked.map((c) => c.call), ["c2", "c1"]);
  const early = feedTurns([turn(10, "a")], { cards: [], agents: [], questions: [], blocked: [blocked("c3", 5)] });
  assert.deepEqual(placedCalls(early), [[0, []], [null, ["c3"]]]);
});
