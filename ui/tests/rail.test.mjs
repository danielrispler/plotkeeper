// The rail opens on "now", as a feed of turns (#574). Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { cardTime, feedTurns, railTop, turnAt } from "../../dist/story.js";

const html = fs.readFileSync(new URL("../index.html", import.meta.url), "utf8");
const rail = html.slice(html.indexOf('<aside class="rail"'), html.indexOf("</aside>"));
const card = (at) => ({ request: { id: `s@${at}` } });
const turn = (started_at, prompt) => ({ started_at, ended_at: null, prompt, id: null, message: null });

test("with an open question the rail starts with it; with none, with the newest card", () => {
  const cards = [card(1), card(5), card(3)];
  assert.equal(railTop(true, cards), null);
  assert.equal(railTop(false, cards), cards[2]);
  assert.equal(railTop(false, []), null);
  // In the page: the dock, then the newest card, the intention, the turn summary and the feed.
  const order = ["dock", "now-card", "intention", "now-summary", "feed"].map((id) => rail.indexOf(`id="${id}"`));
  assert.ok(order.every((at) => at > 0));
  assert.deepEqual([...order].sort((a, b) => a - b), order);
});

test("a card belongs to the turn it was written in, by its id's time", () => {
  const turns = [turn(10, "a"), turn(20, "b")];
  assert.equal(cardTime("477b6b89-5461@1793715833094"), 1793715833094);
  assert.equal(turnAt(turns, 5), null);
  assert.equal(turnAt(turns, 10), 0);
  assert.equal(turnAt(turns, 25), 1);
});

test("turns render newest first, each with its cards, checks, subagents and questions", () => {
  const turns = [turn(10, "Fix the parser"), turn(20, "worker w1 returned"), turn(30, "Open the PR")];
  const checks = [
    { command: "cargo test", started_at: 12, turn: 0 },
    { command: "cargo run --bin ci-checks", started_at: 31, turn: 2 },
    { command: "cargo test", started_at: 32, turn: 2 },
  ];
  const agents = [{ id: "w1", started_at: 11, turn: 0 }];
  const questions = [{ tool_use_id: "q1", answer: "not_answered", turn: 1 }];
  const feed = feedTurns(turns, { cards: [card(15), card(35)], checks, agents, questions });
  assert.deepEqual(feed.map((b) => b.turn.prompt), ["Open the PR", "worker w1 returned", "Fix the parser"]);
  assert.deepEqual(feed[0].checks.map((c) => c.started_at), [32, 31]);
  assert.deepEqual(feed[0].cards.map((c) => c.request.id), ["s@35"]);
  assert.deepEqual(feed[1].questions.map((q) => q.tool_use_id), ["q1"]);
  assert.deepEqual(feed[2].agents.map((a) => a.id), ["w1"]);
  assert.deepEqual(feed[2].cards.map((c) => c.request.id), ["s@15"]);
});

test("what started in no turn, such as a subagent's cards, goes in one last block", () => {
  const feed = feedTurns([], { cards: [card(1), card(2)], checks: [], agents: [], questions: [] });
  assert.equal(feed.length, 1);
  assert.equal(feed[0].turn, null);
  assert.deepEqual(feed[0].cards.map((c) => c.request.id), ["s@2", "s@1"]);
  assert.deepEqual(feedTurns([], { cards: [], checks: [], agents: [], questions: [] }), []);
});

test("the header is one line, with Rename, Close and Delete in its ⋯ menu", () => {
  const header = rail.slice(rail.indexOf('class="rail-h"'), rail.indexOf('id="dock"'));
  assert.match(header, /<p id="headline"[^>]*><span id="state"[^>]*><\/span> <span id="phase"[^>]*><\/span><\/p>/);
  assert.match(header, /<button id="rail-more"[^>]*popovertarget="rail-actions"[^>]*>⋯<\/button>/);
  const menu = header.slice(header.indexOf('id="rail-actions"'));
  for (const id of ["rename", "close", "delete"]) assert.match(menu, new RegExp(`<button id="${id}"`));
  // No other section of the old rail is left.
  for (const id of ["times", "recent", "older", "responses", "questions", "subagents"]) assert.doesNotMatch(rail, new RegExp(`id="${id}"`));
});
