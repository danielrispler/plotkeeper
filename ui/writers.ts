// The writer section (spec #320, "Writer settings on the page", ticket #363): in the Settings panel over the map (spec #380, #387), the writers the login
// shell's `command -v` found, a model and an effort for the one picked, and Test, one tiny call whose typed outcome shows here.
// Nothing checks a login on its own. A pass saves the choice in `settings.json` and turns summaries on; until a writer
// passes, a user who arrived with none configured gets no summaries (the Test gate), which the writer's notice line says.
// PR briefs are written by `claude` whatever writer is picked, so without it the section, and the Map view's tray, say
// "briefs need Claude".
// An Anthropic API key (ticket #366) is pasted here and kept in the macOS Keychain; the server says only whether one is set,
// and the API-key writer is listed, first, once it is. Below it, the OpenAI-compatible API (ticket #367): a base URL, kept in
// `settings.json` as `summaries.base_url` (none: OpenAI's own), and an optional key, also kept in the Keychain, as a local
// Ollama needs none.

import { confirmed, drawWithStatus, failure, invoke, text } from "./dom.js";
import type { Settings, SettingsFiles } from "./settings.js";
import type { Writers, WriterKind, WriterTest } from "./types.js";
import { GATE } from "./writer-notice.js";

export type Choice = { writer: WriterKind; model: string; effort: string };

// The writer picker's options: every writer, one that was not found shown but not pickable.
export const choiceOptions = (list: Writers): { kind: WriterKind; label: string; disabled: boolean }[] =>
  list.writers.map((w) => ({ kind: w.kind, label: w.found ? w.name : `${w.name} (${apiWriter(w.kind) ? "not set" : "not found"})`, disabled: !w.found }));

// The writers that call an API with a key pasted here, instead of running a program.
const apiWriter = (kind: WriterKind) => kind === "anthropic" || kind === "openai";

// OpenAI's own endpoint, which the OpenAI-compatible writer calls when no base URL is set.
export const OPENAI_URL = "https://api.openai.com/v1";

// Whether writer `kind`'s API key is set, as its key row says it; the key itself never reaches the page.
export const keySet = (list: Writers, kind: WriterKind = "anthropic"): boolean => list.keys.includes(kind);
export function keyLine(list: Writers, kind: WriterKind = "anthropic"): string {
  if (kind === "anthropic") return keySet(list) ? "Anthropic API key: set, kept in the macOS Keychain." : "Anthropic API key: not set.";
  if (!list.writers.some((w) => w.kind === kind && w.found)) return "OpenAI-compatible API: not set.";
  const key = keySet(list, kind) ? "key: set, kept in the macOS Keychain" : "no key";
  return `OpenAI-compatible API at ${list.base_url ?? OPENAI_URL}, ${key}.`;
}

// Claude's aliases, which the summaries' model defaults to: no OpenAI-compatible endpoint serves them.
const claudeAliases = ["", "haiku", "sonnet", "opus", "fable"];

// A model as writer `kind` takes it: one any writer but Claude and the API keys does not list is its first, as the server
// reads it (`WriterKind::model`); Claude takes any id, the Anthropic API any full `claude-` id, and the OpenAI-compatible
// API any id but Claude's aliases.
function modelOf(list: Writers, kind: WriterKind, model: string): string {
  const models = list.writers.find((w) => w.kind === kind)?.models ?? [];
  const any = kind === "claude" || (kind === "anthropic" && model.startsWith("claude-")) || (kind === "openai" && !claudeAliases.includes(model));
  return !any && !models.includes(model) ? (models[0] ?? model) : model;
}

// The choice the section opens with: the writer that writes now when summaries are on, else the first found in the
// spec's order (the server lists them so), at its first model and default effort.
export function preset(list: Writers, now: Settings["summaries"]): Choice {
  if (list.summaries_on) {
    const efforts = list.writers.find((w) => w.kind === now.writer)?.efforts ?? [];
    return { writer: now.writer, model: modelOf(list, now.writer, now.model), effort: efforts.includes(now.effort) ? now.effort : "" };
  }
  const first = list.writers.find((w) => w.found) ?? list.writers[0];
  return { writer: first?.kind ?? "claude", model: first?.models[0] ?? "", effort: "" };
}

// A Test's outcome as the section shows it.
export const testLine = (test: WriterTest): { passed: boolean; text: string } =>
  test.outcome === "passed" ? { passed: true, text: test.line } : { passed: false, text: `Failed: ${test.line}` };

// The writer's notice line while summaries wait for a Test (`writer-notice.ts`).
export const gateNotice = (list: Writers): string | null => (list.summaries_on ? null : GATE);

export const briefsLine = (list: Writers): string | null =>
  list.briefs ? null : "PR briefs need Claude: they are written by `claude`, which was not found, whatever writer is picked here. Install Claude Code and restart plotkeeper.";

// What the section knows: the server's list, the choice, and the last Test's outcome.
let list: Writers | null = null;
let choice: Choice = { writer: "claude", model: "", effort: "" };
let outcome: WriterTest | null = null;
let testing = false;
// Each key row's last refusal, such as a pasted text that is no key.
const keyErrors: Partial<Record<WriterKind, string>> = {};

// Whether PR briefs can be written: until the list is known, yes.
export const briefsWritten = () => list?.briefs ?? true;

function select(id: string, title: string, options: { value: string; label: string; disabled?: boolean }[], value: string, change: (value: string) => void) {
  const node = document.createElement("select");
  node.id = id;
  node.name = id;
  node.title = title;
  for (const option of options) {
    const item = document.createElement("option");
    item.value = option.value;
    item.textContent = option.label;
    item.disabled = option.disabled ?? false;
    node.append(item);
  }
  node.value = value;
  node.addEventListener("change", () => change(node.value));
  return node;
}

function field(label: string, control: HTMLElement) {
  const row = text("label", "", "writer-field");
  row.append(text("span", label, "writer-label"), control);
  return row;
}

export type Hooks = {
  // The summaries' settings in effect, and the settings files' text a save is made from.
  summaries: () => Settings["summaries"];
  texts: () => SettingsFiles;
  // A pass saved `settings.json`: its new text, applied as a save is.
  saved: (settings: string) => void;
  // The gate in the writer's notice line, shown or cleared; clearing it keeps a writer's failure notice.
  gate: (gated: boolean) => void;
};

function draw(panel: HTMLElement, hooks: Hooks) {
  if (!list) return drawWithStatus(panel, [text("p", "Looking for writers…", "writer-hint")], "");
  const known = list;
  const writer = known.writers.find((w) => w.kind === choice.writer);
  const models = writer?.models ?? [];
  const nodes: HTMLElement[] = [
    text("h3", "Summary writer", "side-title"),
    text("p", "The program that writes the turn summaries, the Goal and Design so far, and the sessions' names.", "writer-hint"),
    field("Program", select("writer-pick", "The program that writes; one not found on this Mac is listed but cannot be picked",
      choiceOptions(known).map((o) => ({ value: o.kind, label: o.label, disabled: o.disabled })), choice.writer, (value) => {
        const next = known.writers.find((w) => w.kind === value);
        choice = { writer: value as WriterKind, model: next?.models[0] ?? "", effort: "" };
        outcome = null;
        draw(panel, hooks);
      })),
    field("Model", choice.writer === "openai" ? modelInput(models) : select("writer-model", "The model the program writes with",
      [...(models.includes(choice.model) ? [] : [choice.model]), ...models].map((m) => ({ value: m, label: m })), choice.model, (value) => {
        choice = { ...choice, model: value };
        outcome = null;
      })),
  ];
  if (writer?.efforts.length)
    nodes.push(field("Effort", select("writer-effort", "How hard the model thinks; default leaves it to the model",
      writer.efforts.map((e) => ({ value: e, label: e || "default" })), choice.effort, (value) => {
        choice = { ...choice, effort: value };
        outcome = null;
      })));
  const test = text("button", testing ? "Testing…" : "Test", "btn small writer-test");
  test.id = "writer-test";
  test.setAttribute("type", "button");
  test.title = "Make one tiny call with this writer, model and effort; a pass saves them and summaries are written with them";
  (test as HTMLButtonElement).disabled = testing;
  test.addEventListener("click", () => run(panel, hooks));
  nodes.push(test);
  if (outcome) {
    const line = testLine(outcome);
    const shown = text("p", line.text, `writer-outcome ${line.passed ? "passed" : "failed"}`);
    shown.id = "writer-outcome";
    nodes.push(shown);
  } else if (!known.summaries_on) {
    nodes.push(text("p", "Summaries are off until a writer passes Test.", "writer-outcome"));
  }
  nodes.push(...keyRow(panel, hooks, known), ...openaiRow(panel, hooks, known));
  const briefs = briefsLine(known);
  if (briefs) {
    const shown = text("p", briefs, "writer-briefs");
    shown.id = "writer-briefs";
    nodes.push(shown);
  }
  // The Test's outcome and the keys' errors are said in the section's one status region (#456, #452's review).
  const said = [outcome ? testLine(outcome).text : "", keyErrors.anthropic ?? "", keyErrors.openai ?? ""].filter(Boolean).join(" ");
  drawWithStatus(panel, nodes, said);
}

// The key row: whether a key is set, a field to add one or replace it, and Remove once one is set.
function keyRow(panel: HTMLElement, hooks: Hooks, known: Writers): HTMLElement[] {
  const set = keySet(known);
  const state = text("p", keyLine(known), "writer-hint");
  state.id = "writer-key-state";
  const input = document.createElement("input");
  input.id = "writer-key";
  input.name = "anthropic-api-key";
  input.type = "password";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.placeholder = set ? "Replace the API key…" : "Add an API key…";
  input.setAttribute("aria-label", "Anthropic API key");
  input.title = "An Anthropic API key; it is kept in the macOS Keychain and never shown again";
  const save = text("button", "Save key", "btn small");
  save.id = "writer-key-save";
  save.setAttribute("type", "button");
  save.title = "Keep the pasted key in the macOS Keychain, in place of any key set; the page never shows it again";
  save.addEventListener("click", () => {
    const key = input.value.trim();
    if (!key) return;
    input.value = "";
    invoke("set_api_key", { writer: "anthropic", key, base_url: null, expected: null })
      .then(() => {
        delete keyErrors.anthropic;
        const anthropic = known.writers.find((w) => w.kind === "anthropic");
        choice = { writer: "anthropic", model: anthropic?.models[0] ?? "", effort: "" };
        outcome = null;
      })
      .catch((e: unknown) => {
        keyErrors.anthropic = e instanceof Error ? e.message : String(e);
      })
      .finally(() => void refresh(panel, hooks));
  });
  const row = text("div", "", "writer-key");
  row.append(input, save);
  if (set) {
    const remove = text("button", "Remove key", "btn small");
    remove.id = "writer-key-remove";
    remove.setAttribute("type", "button");
    remove.title = "Remove the key from the macOS Keychain; the API-key writer then cannot write";
    remove.addEventListener("click", async () => {
      const lost = "The Anthropic API key is deleted from the macOS Keychain; the API-key writer cannot write until a key is pasted again.";
      if (!(await confirmed({ heading: "Remove the API key?", lost, verb: "Remove key" }))) return;
      invoke("remove_api_key", { writer: "anthropic", expected: null })
        .then(() => {
          delete keyErrors.anthropic;
          outcome = null;
        })
        .catch(failure("remove_api_key"))
        .finally(() => void refresh(panel, hooks, true));
    });
    row.append(remove);
  }
  const nodes = [state, row];
  if (keyErrors.anthropic) {
    nodes.push(keyError(keyErrors.anthropic, "writer-key-error", [input]));
  }
  return nodes;
}

// A key's error, tied to the `fields` it is about (#456); the section's status region says it.
function keyError(words: string, id: string, fields: HTMLElement[]): HTMLElement {
  const shown = text("p", words, "writer-outcome failed");
  shown.id = id;
  for (const field of fields) {
    field.setAttribute("aria-invalid", "true");
    field.setAttribute("aria-describedby", id);
  }
  return shown;
}

// The OpenAI-compatible writer's model: any id its endpoint serves, typed, with the listed ones offered.
function modelInput(models: string[]): HTMLElement {
  const input = document.createElement("input");
  input.id = "writer-model";
  input.name = "writer-model";
  input.title = "The model the endpoint serves, such as gpt-6-luna on OpenAI or llama3.2 on a local Ollama";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.value = choice.model;
  input.setAttribute("list", "writer-models");
  input.addEventListener("input", () => {
    choice = { ...choice, model: input.value.trim() };
    outcome = null;
  });
  const options = document.createElement("datalist");
  options.id = "writer-models";
  for (const model of models) {
    const option = document.createElement("option");
    option.value = model;
    options.append(option);
  }
  const both = text("span", "", "writer-model-input");
  both.append(input, options);
  return both;
}

// The OpenAI-compatible API's row: its base URL (empty: OpenAI's own) and an optional key, saved together, and Remove,
// which clears both, once either is set.
function openaiRow(panel: HTMLElement, hooks: Hooks, known: Writers): HTMLElement[] {
  const found = known.writers.some((w) => w.kind === "openai" && w.found);
  const state = text("p", keyLine(known, "openai"), "writer-hint");
  state.id = "writer-openai-state";
  const base = document.createElement("input");
  base.id = "writer-base-url";
  base.name = "openai-base-url";
  base.type = "url";
  base.autocomplete = "off";
  base.spellcheck = false;
  base.value = known.base_url ?? "";
  base.placeholder = `e.g. ${OPENAI_URL}…`;
  base.setAttribute("aria-label", "Base URL");
  base.title = "The API's base URL, such as https://openrouter.ai/api/v1 or http://localhost:11434/v1 for Ollama; empty: OpenAI's own";
  const input = document.createElement("input");
  input.id = "writer-openai-key";
  input.name = "openai-api-key";
  input.type = "password";
  input.autocomplete = "off";
  input.spellcheck = false;
  input.placeholder = keySet(known, "openai") ? "Replace the key (optional)…" : "Its API key (optional)…";
  input.setAttribute("aria-label", "API key of the OpenAI-compatible API");
  input.title = "The endpoint's API key, if it needs one; it is kept in the macOS Keychain and never shown again";
  const save = text("button", "Save", "btn small");
  save.id = "writer-openai-save";
  save.setAttribute("type", "button");
  save.title = "Keep the base URL in settings.json and any pasted key in the macOS Keychain; the page never shows the key again";
  save.addEventListener("click", () => {
    const key = input.value.trim();
    input.value = "";
    invoke("set_api_key", { writer: "openai", key: key || null, base_url: base.value.trim(), expected: hooks.texts().settings })
      .then((settings) => {
        delete keyErrors.openai;
        if (settings !== null) hooks.saved(settings);
        const openai = known.writers.find((w) => w.kind === "openai");
        choice = { writer: "openai", model: openai?.models[0] ?? "", effort: "" };
        outcome = null;
      })
      .catch((e: unknown) => {
        keyErrors.openai = e instanceof Error ? e.message : String(e);
      })
      .finally(() => void refresh(panel, hooks));
  });
  const row = text("div", "", "writer-key");
  row.append(base);
  const keyRow = text("div", "", "writer-key");
  keyRow.append(input, save);
  if (found) {
    const remove = text("button", "Remove", "btn small");
    remove.id = "writer-openai-remove";
    remove.setAttribute("type", "button");
    remove.title = "Remove the base URL from settings.json and the key from the macOS Keychain";
    remove.addEventListener("click", async () => {
      const url = known.base_url ? `The base URL ${known.base_url}` : "The base URL";
      const lost = `${url} is removed from settings.json, and the endpoint's key, if one is set, from the macOS Keychain.`;
      if (!(await confirmed({ heading: "Remove the OpenAI-compatible API?", lost, verb: "Remove" }))) return;
      invoke("remove_api_key", { writer: "openai", expected: hooks.texts().settings })
        .then((settings) => {
          delete keyErrors.openai;
          if (settings !== null) hooks.saved(settings);
          outcome = null;
        })
        .catch(failure("remove_api_key"))
        .finally(() => void refresh(panel, hooks, true));
    });
    keyRow.append(remove);
  }
  const nodes = [state, row, keyRow];
  if (keyErrors.openai) {
    nodes.push(keyError(keyErrors.openai, "writer-openai-error", [base, input]));
  }
  return nodes;
}

// Asks for the list; the first time, or with `again`, presets the choice from it.
function refresh(panel: HTMLElement, hooks: Hooks, again = false) {
  return invoke("writers")
    .then((writers) => {
      const first = list === null;
      list = writers;
      if (first || again) choice = preset(writers, hooks.summaries());
      hooks.gate(!writers.summaries_on);
      draw(panel, hooks);
      window.dispatchEvent(new Event("writers"));
    })
    .catch(failure("writers"));
}

function run(panel: HTMLElement, hooks: Hooks) {
  testing = true;
  outcome = null;
  draw(panel, hooks);
  invoke("test_writer", { ...choice, expected: hooks.texts().settings })
    .then((test) => {
      outcome = test;
      if (test.settings !== null) hooks.saved(test.settings);
    })
    .catch(failure("test_writer"))
    .finally(() => {
      testing = false;
      draw(panel, hooks);
      void refresh(panel, hooks);
    });
}

// The section itself: one node, drawn while the Settings panel is closed too, so the list and a Test running carry over.
let section: HTMLElement | null = null;
let given: Hooks;
const sectionNode = () => {
  if (!section) {
    section = text("div", "", "writers");
    section.id = "writers";
  }
  return section;
};

// Draws the section and asks for the list once.
export function startWriters(hooks: Hooks) {
  given = hooks;
  draw(sectionNode(), hooks);
  void refresh(sectionNode(), hooks);
}

// The Settings panel opened: the section goes into `box`, its choice preset again from the settings, as each opening of its popover did (#363).
export function writersIn(box: HTMLElement) {
  const panel = sectionNode();
  if (list && !testing) {
    choice = preset(list, given.summaries());
    outcome = null;
    draw(panel, given);
  }
  box.replaceChildren(panel);
}
