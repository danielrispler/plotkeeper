// Agent logos (#158): one pure function maps a session to its logo, and the rows that show it. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import { fakeDocument } from "./dom.mjs";

const { logoKey, logo } = await import("../../dist/logos.js");
const { row, rowMark, strip: stripOf, projectHead, agentRow } = await import("../../dist/list.js");
// The document is set after the import: times.ts starts a timer when one exists at load.
globalThis.document = fakeDocument({ visibilityState: "hidden" });
const make = (tag) => document.createElement(tag);

const base = { id: "s", name: null, prompt_name: null, folder_name: "p", state: "idle", line: "", questions: [], shell: false, status_only: false, unseen: false, landed: false };
const caps = (agent) => ({ agent, context: agent === "claude", transcript_model: agent === "claude" });
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

test("an agent the page does not know by name wears the generic logo, drawn with its title", () => {
  const newagent = { ...claude, capabilities: { agent: "newagent", context: true, transcript_model: true } };
  assert.equal(logoKey(newagent), "generic");
  assert.equal(logoKey("newagent"), "generic");
  const drawn = logoOf(row(newagent, new Map()));
  assert.equal(drawn.getAttribute("data-logo"), "generic");
  assert.ok(drawn.title.length > 0);
  assert.match(drawn.innerHTML, /<path d="[^"]+"/);
});

const logoOf = (node) => node.find("span").find((s) => s.className === "logo");

test("a recorded Codex session wears Codex's logo, titled by its name (#359)", () => {
  const codex = { ...base, capabilities: caps("codex"), identity: { issue: null, name: "p" }, transcript: { context: null } };
  assert.equal(logoKey(codex), "codex");
  const drawn = logoOf(row(codex, new Map()));
  assert.equal(drawn.getAttribute("data-logo"), "codex");
  // Its row's title is the note on what it reports; the mark's own title names Codex, no longer as status only.
  assert.match(drawn.title, /no context share/);
  assert.equal(logo("codex").title, "Codex");
});

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

test("the strip, the project header's rollup and a subagent row draw no logo", () => {
  const withLogos = (node) => node.find("span").filter((s) => s.className === "logo");
  const sessions = [{ ...claude, state: "running" }, { ...agy, id: "g", state: "needs_you" }, { ...statusOnly("codex"), id: "c", state: "running" }];
  const strip = stripOf(sessions);
  assert.ok(strip.length > 1);
  for (const button of strip) assert.equal(withLogos(button).length, 0);
  // Its rows do: the same sessions drawn as rows show a logo each.
  for (const s of sessions) assert.equal(withLogos(row(s, new Map())).length, 1);
  const head = projectHead({ key: "/w/p", name: "p", project: true, rollup: "running", states: [] });
  assert.equal(withLogos(head).length, 0);
  assert.ok(head.find("span").some((s) => s.dataset?.glyph === "spinner"));
  const worker = { id: "a1", parent: "session", kind: "worker", name: "Review", asked: null, agent_type: null, status: "running", started_at: 1, ended_at: null, last_step: null, step_span: null };
  const drawnAgent = agentRow({ ...claude, agents: [worker], asker: null }, new Map(), worker, 1);
  assert.equal(withLogos(drawnAgent).length, 0);
  assert.ok(drawnAgent.find("span").length > 0);
});

test("the logo is drawn in the muted text colour", () => {
  const css = fs.readFileSync(new URL("../style.css", import.meta.url), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
  const rules = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)].filter(([, selector]) => /\.logo/.test(selector));
  assert.ok(rules.length > 0);
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
  assert.equal(mark.childNodes.length, 1, "the header draws its logo");
  assert.equal(mark.childNodes[0].getAttribute("data-logo"), "claude");
  assert.equal(mark.childNodes[0].title, "Claude · Opus 5.5");
});

test("the rail header's logo for agy keeps the note that it has no context share, after the model line", async () => {
  const { showView } = await import("../../dist/story.js");
  const els = new Map();
  const get = (id) => els.get(id) ?? (els.set(id, make("div")), els.get(id));
  globalThis.document = { ...globalThis.document, getElementById: get };
  const session = { ...agy, transcript: agy.transcript, permission_mode: null, work: {}, agents: [], turns: [], checks: [], cleared: [], prompts: [], model: "gemini-3-flash" };
  try {
    showView({ session, agent: null });
  } catch {
    // The rest of the rail needs more of a page than the stand-in has; the header is drawn first.
  }
  const title = get("mark").childNodes[0].title;
  assert.match(title, /no context share/);
  assert.ok(title.length > "no context share".length);
});

const railOf = (session, agent = null) => {
  const els = new Map();
  const get = (id) => els.get(id) ?? (els.set(id, make("div")), els.get(id));
  globalThis.document = { ...globalThis.document, getElementById: get };
  const full = { permission_mode: null, work: {}, agents: [], turns: [], checks: [], cleared: [], prompts: [], model: null, ...session };
  return import("../../dist/story.js").then(({ showView }) => {
    showView({ session: full, agent });
    return get;
  });
};

test("the no-context note follows the session's capabilities, not the agent's name", async () => {
  const other = { ...agy, capabilities: { agent: "newagent", context: false }, model: null };
  const get = await railOf(other);
  assert.match(get("mark").childNodes[0].title, /no context share/);
  assert.match(logoOf(row(other, new Map())).title, /no context share/);
  const noNote = await railOf({ ...agy, capabilities: { agent: "agy", context: true } });
  assert.doesNotMatch(noNote("mark").childNodes[0].title, /no context share/);
  assert.doesNotMatch(logoOf(row({ ...agy, capabilities: { agent: "agy", context: true } }, new Map())).title, /no context share/);
  // A status-only row has no agent of its own to note.
  assert.doesNotMatch(logoOf(row(statusOnly("codex"), new Map())).title, /no context share/);
});

test("a selected subagent's rail logo carries no model line", async () => {
  const worker = { id: "a1", parent: "session", kind: "worker", name: "Review", asked: null, agent_type: null, status: "running", started_at: 1, ended_at: null, last_step: null, step_span: null };
  const session = { ...claude, transcript: { ...claude.transcript, model: "claude-opus-5-5" }, agents: [worker] };
  const get = await railOf(session, "a1");
  assert.doesNotMatch(get("mark").childNodes[0].title, /Opus/);
});

test("the rail's #detail has no title when the step has no detail", async () => {
  const get = await railOf({ ...claude, transcript: { ...claude.transcript, model: "claude-opus-5-5" } });
  assert.notEqual(get("detail").title, "");
});
