// Auto mode (#541): a session starts in it from the picker, keeps it when revived, and the rail's
// one line always shows it. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { startedAuto } from "../../dist/panes.js";
import { modeText } from "../../dist/story.js";

test("the rail's line names any mode but the default, and auto mode always", () => {
  assert.equal(modeText("auto"), "auto mode");
  assert.equal(modeText("plan"), "plan mode");
  assert.equal(modeText("bypassPermissions"), "bypass permissions");
  assert.equal(modeText("default"), "");
  assert.equal(modeText(null), "");
  // On the line, before the phase, which the line's ellipsis cuts first; never only in a title.
  const html = fs.readFileSync(new URL("../index.html", import.meta.url), "utf8");
  const line = html.match(/<p id="headline"[^\n]*<\/p>/)[0];
  assert.ok(line.indexOf('id="state"') < line.indexOf('id="mode"') && line.indexOf('id="mode"') < line.indexOf('id="phase"'));
});

test("a revived terminal resumes in auto mode only when its spawn started in it", () => {
  const base = ["env", "CLAUDE_CODE_PLUGIN_DIRS=/p", "CLAUDE_CLIENT_PRESENCE_FILE=/f", "claude", "--settings", "{}"];
  assert.equal(startedAuto([...base, "--permission-mode", "auto"]), true);
  assert.equal(startedAuto([...base, "--permission-mode", "auto", "--resume", "9e0b5f13"]), true);
  assert.equal(startedAuto([...base, "--", "fix it"]), false);
  assert.equal(startedAuto([]), false);
});

test("the picker's Auto mode choice is off by default and reaches the start command", () => {
  const picker = fs.readFileSync(new URL("../picker.ts", import.meta.url), "utf8");
  assert.match(picker, /type: "checkbox" }\)/);
  assert.doesNotMatch(picker, /checked: true/);
  // Both ways to start from the picker pass the choice on.
  assert.match(picker, /startClaude\(place\.path, null, "", auto\(\)\)/);
  assert.match(picker, /startClaude\(task\.path, null, prompt \? value : "", auto\(\)\)/);
  assert.match(picker, /quickStart\(places\.main, auto\)/);
});
