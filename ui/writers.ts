// The writer section (spec #320, "Writer settings on the page", ticket #363): under Writer in the top bar, the writers the login
// shell's `command -v` found, a model and an effort for the one picked, and Test, one tiny call whose typed outcome shows here.
// Nothing checks a login on its own. A pass saves the choice in `settings.json` and turns summaries on; until a writer
// passes, a user who arrived with none configured gets no summaries (the Test gate), which the writer's notice line says.
// PR briefs are written by `claude` whatever writer is picked, so without it the section, and the Map view's tray, say
// "briefs need Claude".

import { element, failure, invoke, text } from "./dom.js";
import type { Settings, SettingsFiles } from "./settings.js";
import type { Writers, WriterKind, WriterTest } from "./types.js";

export type Choice = { writer: WriterKind; model: string; effort: string };

// The writer picker's options: every writer, one that was not found shown but not pickable.
export const choiceOptions = (list: Writers): { kind: WriterKind; label: string; disabled: boolean }[] =>
  list.writers.map((w) => ({ kind: w.kind, label: w.found ? w.name : `${w.name} (not found)`, disabled: !w.found }));

// A model as writer `kind` takes it: one any writer but Claude does not list is its first, as the server reads it
// (`WriterKind::model`); Claude takes any id.
function modelOf(list: Writers, kind: WriterKind, model: string): string {
  const models = list.writers.find((w) => w.kind === kind)?.models ?? [];
  return kind !== "claude" && !models.includes(model) ? (models[0] ?? model) : model;
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

// The writer's notice line while summaries wait for a Test; it starts as the writer's failure notice does, so a later one
// replaces it (`writer-notice.ts`).
export const gateNotice = (list: Writers): string | null =>
  list.summaries_on ? null : "Summaries are off: pick a writer under Writer, at the top of the page, and press Test.";

export const briefsLine = (list: Writers): string | null =>
  list.briefs ? null : "PR briefs need Claude: they are written by `claude`, which was not found, whatever writer is picked here. Install Claude Code and restart plotkeeper.";

// What the section knows: the server's list, the choice, and the last Test's outcome.
let list: Writers | null = null;
let choice: Choice = { writer: "claude", model: "", effort: "" };
let outcome: WriterTest | null = null;
let testing = false;

// Whether PR briefs can be written: until the list is known, yes.
export const briefsWritten = () => list?.briefs ?? true;

function select(id: string, title: string, options: { value: string; label: string; disabled?: boolean }[], value: string, change: (value: string) => void) {
  const node = document.createElement("select");
  node.id = id;
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
  // The writer's notice line, set or cleared.
  notice: (line: string | null) => void;
};

function draw(panel: HTMLElement, hooks: Hooks) {
  if (!list) return panel.replaceChildren(text("p", "Looking for writers…", "writer-hint"));
  const known = list;
  const writer = known.writers.find((w) => w.kind === choice.writer);
  const models = writer?.models ?? [];
  const nodes: HTMLElement[] = [
    text("h3", "Writer", "side-title"),
    text("p", "The program that writes the turn summaries, the Goal and Design so far, and the sessions' names.", "writer-hint"),
    field("Program", select("writer-pick", "The program that writes; one not found on this Mac is listed but cannot be picked",
      choiceOptions(known).map((o) => ({ value: o.kind, label: o.label, disabled: o.disabled })), choice.writer, (value) => {
        const next = known.writers.find((w) => w.kind === value);
        choice = { writer: value as WriterKind, model: next?.models[0] ?? "", effort: "" };
        outcome = null;
        draw(panel, hooks);
      })),
    field("Model", select("writer-model", "The model the program writes with",
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
  const briefs = briefsLine(known);
  if (briefs) {
    const shown = text("p", briefs, "writer-briefs");
    shown.id = "writer-briefs";
    nodes.push(shown);
  }
  panel.replaceChildren(...nodes);
}

function refresh(panel: HTMLElement, hooks: Hooks) {
  return invoke("writers")
    .then((writers) => {
      const first = list === null;
      list = writers;
      if (first) choice = preset(writers, hooks.summaries());
      hooks.notice(gateNotice(writers));
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

// Draws the section into its popover and asks for the list once; each opening presets the choice again from the settings.
export function startWriters(hooks: Hooks) {
  const panel = element("writers");
  panel.addEventListener("toggle", (event) => {
    if ((event as ToggleEvent).newState !== "open" || !list || testing) return;
    choice = preset(list, hooks.summaries());
    outcome = null;
    draw(panel, hooks);
  });
  draw(panel, hooks);
  void refresh(panel, hooks);
}
