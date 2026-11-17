// Agent logos (#158): one pure function maps a session to its logo, and the rows that show it. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { El } from "./dom.mjs";

const { logoKey } = await import("../../dist/logos.js");
const { row, rowMark } = await import("../../dist/list.js");
// The document is set after the import: times.ts starts a timer when one exists at load.
const make = (tag) => Object.assign(new El(tag), { dataset: {}, classList: { add() {}, toggle() {} } });
globalThis.document = { createElement: make, getElementById: () => make("div"), visibilityState: "hidden" };

const base = { id: "s", name: null, prompt_name: null, folder_name: "p", state: "idle", line: "", questions: [], shell: false, status_only: false, unseen: false, landed: false };
const caps = (agent) => ({ agent });
const claude = { ...base, capabilities: caps("claude"), identity: { issue: null, name: "p" }, transcript: { context: { tokens: 9000, share: 9 } } };
const agy = { ...base, capabilities: caps("agy"), identity: { issue: null, name: "p" }, transcript: { context: null } };
const statusOnly = (program) => ({ ...base, status_only: true, capabilities: caps(""), identity: { issue: null, name: `${program} · p` }, transcript: { context: null } });
const shell = { ...statusOnly("zsh"), shell: true };

test("a session's logo follows its adapter's agent, shell and, for a status-only row, its program", () => {
  assert.equal(logoKey(claude), "claude");
  assert.equal(logoKey(agy), "agy");
  assert.equal(logoKey(statusOnly("codex")), "codex");
  assert.equal(logoKey(statusOnly("aider")), "generic");
  assert.equal(logoKey(shell), "shell");
  // Only the program `codex` is Codex's: a longer name is not.
  assert.equal(logoKey(statusOnly("codex-fork")), "generic");
  // An agent alone, as the usage glance asks.
  assert.equal(logoKey({ agent: "claude" }), "claude");
  assert.equal(logoKey({ agent: "codex" }), "codex");
});

const logoOf = (node) => node.find("span").find((s) => s.className === "logo");

test("a session row in the list, Home and History shows the logo before its name; an agy row no longer shows \"agy\"", () => {
  const drawn = row(claude, new Map());
  const first = drawn.childNodes[0];
  assert.equal(first.childNodes[0], logoOf(drawn));
  assert.equal(logoOf(drawn).getAttribute("data-logo"), "claude");
  const agyRow = row(agy, new Map());
  assert.equal(logoOf(agyRow).getAttribute("data-logo"), "agy");
  assert.match(logoOf(agyRow).title, /no context share/);
  assert.equal(agyRow.text.includes("agy"), false);
  assert.equal(rowMark(agy), "");
  assert.equal(rowMark(claude), "context 9%");
});

test("the strip, the project rollup and subagent rows draw no logo", () => {
  const src = fs.readFileSync(new URL("../list.ts", import.meta.url), "utf8");
  const only = (fn) => src.slice(src.indexOf(`function ${fn}(`), src.indexOf("\n}\n", src.indexOf(`function ${fn}(`)));
  for (const fn of ["strip", "agentRow"]) assert.doesNotMatch(only(fn), /logo/i, fn);
  assert.match(only("row"), /logo/i);
});

test("the logo is monochrome: one colour, the muted text colour", () => {
  const css = fs.readFileSync(new URL("../style.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const rules = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)].filter(([, selector]) => /\.logo/.test(selector));
  assert.ok(rules.length > 0);
  for (const [, , body] of rules) assert.doesNotMatch(body, /needs-you/);
  assert.match(rules[0][2], /color: var\(--muted\)/);
});

test("the rail header shows the logo, titled with the model line", async () => {
  const { showView } = await import("../../dist/story.js");
  const els = new Map();
  const get = (id) => els.get(id) ?? (els.set(id, make("div")), els.get(id));
  globalThis.document = { ...globalThis.document, getElementById: get };
  const session = { ...claude, transcript: { ...claude.transcript, model: "claude-opus-5-5" }, permission_mode: null, work: {}, agents: [], turns: [], checks: [], cleared: [], prompts: [], model: null };
  try {
    showView({ session, agent: null });
  } catch {
    // The rest of the rail needs more of a page than the stand-in has; the header is drawn first.
  }
  const mark = get("mark");
  assert.equal(mark.childNodes[0].getAttribute("data-logo"), "claude");
  assert.equal(mark.childNodes[0].title, "Claude · Opus 5.5");
});
