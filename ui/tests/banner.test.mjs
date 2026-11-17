// "Where we were" (danielrispler/plotkeeper#124). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { bannerFor, bannerLives, showsBanner, sinceText } from "../../dist/banner.js";

const MIN = 60_000;
const NOW = 1_800_000_000_000;
const ended = (minutes) => ({ started_at: NOW - (minutes + 5) * MIN, ended_at: NOW - minutes * MIN, files: null, checks: [] });
const session = (state, turns, unseen = false) => ({ id: "s", state, unseen, turns, seen: 0, since_looked: { turns: turns.length, files: 0, passed: 0, failed: 0, ran: 0 } });

test("an idle session shows the banner, and so does an ended one", () => {
  assert.equal(showsBanner(session("idle", [ended(200)]), NOW), true);
  assert.equal(showsBanner(session("ended", [ended(500)]), NOW), true);
  assert.equal(showsBanner(session("idle", []), NOW), false);
});

test("unseen turns show it once the last ended 15 minutes ago, not at 14", () => {
  assert.equal(showsBanner(session("your_turn", [ended(14)], true), NOW), false);
  assert.equal(showsBanner(session("your_turn", [ended(15)], true), NOW), true);
  assert.equal(showsBanner(session("your_turn", [ended(60)], false), NOW), false);
});

test("a running session with nothing unseen shows none", () => {
  assert.equal(showsBanner(session("running", [{ ...ended(60), ended_at: null }]), NOW), false);
});

test("the banner carries the core's since, whatever seen says by then", () => {
  const since = { turns: 2, files: 1, passed: 1, failed: 0, ran: 0 };
  assert.deepEqual(bannerFor({ ...session("idle", [ended(200)]), seen: 1, since_looked: since }, NOW).since, since);
});

test("the since line reads as facts", () => {
  assert.equal(sinceText({ turns: 2, files: 4, passed: 2, failed: 0, ran: 0 }), "2 turns · 4 files · checks passed");
  assert.equal(sinceText({ turns: 1, files: 0, passed: 1, failed: 1, ran: 0 }), "1 turn · checks 1 passed, 1 failed");
  assert.equal(sinceText({ turns: 0, files: 0, passed: 0, failed: 0, ran: 0 }), "Nothing new since you looked");
});

test("the banner stays while Daniel reads, and goes with another session or a new prompt", () => {
  const idle = session("idle", [ended(200)]);
  const banner = bannerFor(idle, NOW);
  assert.ok(banner);
  assert.equal(bannerLives(banner, idle), true);
  // Looking at the rail marks the turns seen: the banner is kept.
  assert.equal(bannerLives(banner, { ...idle, unseen: false }), true);
  assert.equal(bannerLives(banner, { ...idle, id: "other" }), false);
  assert.equal(bannerLives(banner, { ...idle, state: "running", turns: [...idle.turns, ended(0)] }), false);
  assert.equal(bannerLives(null, idle), false);
});
