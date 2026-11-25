// "See the code" of a brief's step (spec #477, ticket #490): which files each step's button asks for, run on the `tsc` output.
import { test } from "node:test";
import assert from "node:assert/strict";
import { stepCode } from "../../dist/map-model.js";

const step = (k, on = []) => ({ k, on });
const parts = new Set(["server", "window"]);

test("a step asks for the files of the parts it lights, leaving its actors out", () => {
  const list = [step("Where", ["server", "daniel"]), step("Now", ["window"]), step("Ratify", ["server"])];
  assert.deepEqual(stepCode(list, 0, parts), ["server"]);
  assert.deepEqual(stepCode(list, 1, parts), ["window"]);
});

test("the Ratify step shows every file, or the last step when there is no Ratify step", () => {
  assert.equal(stepCode([step("Now", ["server"]), step("Ratify", ["server"])], 1, parts), "all");
  const list = [step("Where", ["server"]), step("Could break", ["window"])];
  assert.equal(stepCode(list, 1, parts), "all");
  assert.deepEqual(stepCode(list, 0, parts), ["server"]);
});

test("a step that lights only actors, or nothing, lights no part", () => {
  const list = [step("Where", ["daniel"]), step("Before"), step("Now", ["server"])];
  assert.equal(stepCode(list, 0, parts), "none");
  assert.equal(stepCode(list, 1, parts), "none");
});
