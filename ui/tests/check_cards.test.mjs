// The verdict on a check's card (danielrispler/Falinks#476): the check's result settles the guess. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { checkVerdict, quizState } from "../../dist/cards.js";

const guess = (right) => ({ option: 1, right, at: 1793264010000 });

test("while the check runs, a tap waits for its result and no tap asks for one", () => {
  assert.equal(checkVerdict(null, null), null);
  assert.equal(checkVerdict(null, guess(false)), "The check still runs; its result settles your guess.");
});

test("the result says whether the guess was right, or only the result with no guess", () => {
  assert.equal(checkVerdict("failed", guess(true)), "It failed. You were right.");
  assert.equal(checkVerdict("passed", guess(false)), "It passed. You were wrong.");
  assert.equal(checkVerdict("passed", null), "It passed.");
});

test("an exit code that does not say counts no guess", () => {
  assert.match(checkVerdict("not_counted", guess(false)), /no guess counts/);
});

const stored = (check, guess, check_result) => ({ request: { id: "a@1", check }, guess, check_result });

test("a check's card waits for its result, then names no key when the exit code does not say", () => {
  const running = quizState(stored("cargo test", null, null));
  assert.deepEqual([running.answered, running.reasons, running.flaggable], [false, false, false]);
  // A tap closes the options, but the reasons wait for the result.
  const tapped = quizState(stored("cargo test", guess(null), null));
  assert.deepEqual([tapped.answered, tapped.reasons], [true, false]);
  const unknown = quizState(stored("cargo test", null, "not_counted"));
  assert.deepEqual([unknown.answered, unknown.reasons, unknown.keyNamed], [true, true, false]);
});

test("any other card is answered by the tap, which the writer's key grades", () => {
  const tapped = quizState(stored(null, guess(true), null));
  assert.deepEqual(tapped, { answered: true, reasons: true, keyNamed: true, verdict: "You were right.", flaggable: true });
  assert.equal(quizState(stored(null, null, null)).answered, false);
});
