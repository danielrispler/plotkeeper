// The writer's section of Settings: the writers found, their Test, and API keys.

import type { Fixture } from "./base.ts";
import { confirmFirst, list, mapState } from "./fixtures-shared.ts";

// The writer section (#363) in the Settings panel (#387), opened by the top bar's gear: claude and agy found, or agy alone; `check` runs
// after it has drawn, and a script that presses Test waits for its outcome. The four the ticket names run at 1000x800 too (`…Narrow`).
const claudeWriter = { kind: "claude" as const, name: "Claude", found: true, models: ["haiku", "sonnet", "opus", "fable"], efforts: ["", "low", "medium", "high"] };
const agyWriter = { kind: "agy" as const, name: "agy", found: true, models: ["gemini-4-neon-low", "gemini-4-neon-medium", "gemini-4-neon-high", "gemini-4-argon-low", "gemini-4-hydrogen-low"], efforts: [] };
const anthropicWriter = { kind: "anthropic" as const, name: "Anthropic API key", found: false, models: ["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5"], efforts: ["", "low", "medium", "high"] };
const openaiWriter = { kind: "openai" as const, name: "OpenAI-compatible API", found: false, models: ["gpt-6-luna", "gpt-5.6-luna", "gpt-5.6-terra"], efforts: ["", "low", "medium", "high"] };
// The writer section's one status region (#452's review): before an action, the region and a watch on the section that fails when the
// region leaves it; after, the same region, still in the section, holding `words`.
const writerStatusBefore = `
      const section = document.getElementById("writers");
      const live = [...section.children].filter((node) => node.getAttribute("role") === "status");
      if (live.length !== 1 || live[0].textContent !== "") log("snap:fail the writer section has " + live.length + " status regions before the action, not one empty: " + live.map((node) => node.textContent));
      let left = false;
      new MutationObserver((changes) => (left ||= changes.some((change) => [...change.removedNodes].includes(live[0])))).observe(section, { childList: true });`;
const writerStatusAfter = (words: string) => `
      const now = [...section.children].filter((node) => node.getAttribute("role") === "status");
      if (now.length !== 1 || now[0] !== live[0] || left) log("snap:fail the status region did not stay in the section across the redraw");
      if (!now[0]?.textContent.includes(${JSON.stringify(words)})) log("snap:fail the status region says " + JSON.stringify(now[0]?.textContent));
      if (section.querySelectorAll("[role=status], [aria-live]").length !== 1) log("snap:fail another line of the section is live");`;

const writersOpen = (extra: Partial<Fixture>, check: string, press = false): Fixture => ({
  ...list(),
  map_state: mapState(),
  ...extra,
  snapScript: `
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    await wait(200);
    if (document.getElementById("writer-button")) log("snap:fail the top bar still has a Writer entry");
    document.getElementById("settings-button").click();
    await wait(400);
    const panel = document.querySelector("#map-panel:not([hidden]) #panel-body #writers");
    if (!panel) return log("snap:fail the writer section is not in the Settings panel");
    ${press ? `document.getElementById("writer-test").click(); await wait(300);` : ""}
    ${check}
    await wait(300);
    log("writers ok");`,
  snapDone: "writers ok",
});

/** Unconfigured, with both writers found: preset to Claude, summaries off until a Test passes. */
export const writersList = () =>
  writersOpen({ writers: { writers: [claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: null } }, `
    const picks = [...document.querySelectorAll("#writer-pick option")].map((o) => o.textContent + (o.disabled ? "!" : ""));
    if (picks.join() !== "Claude,agy") log("snap:fail the writers listed: " + picks.join());
    if (document.getElementById("writer-pick").value !== "claude") log("snap:fail the preset is not Claude");
    if (document.getElementById("writer-model").value !== "haiku") log("snap:fail the preset model is not haiku");
    if (!document.getElementById("notice").textContent.startsWith("Summaries are off: ")) log("snap:fail the gate's notice is not shown");`);
/** A passed Test of agy, the only writer found. */
export const writersPassed = () =>
  writersOpen({
    writers: { writers: [{ ...claudeWriter, found: false }, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: null },
    test_writer: { outcome: "passed", line: "Passed: `agy` answered. Summaries are written with it.", settings: null },
  }, `
    if (document.getElementById("writer-pick").value !== "agy") log("snap:fail the preset is not agy, the first found");
    const outcome = document.getElementById("writer-outcome");
    if (!outcome?.classList.contains("passed") || !outcome.textContent.startsWith("Passed")) log("snap:fail no passed outcome: " + outcome?.textContent);`, true);
/** A failed Test with its reason and the writer's own fix. */
export const writersFailed = () =>
  writersOpen({
    writers: { writers: [claudeWriter, agyWriter], summaries_on: true, briefs: true, keys: [], base_url: null },
    test_writer: { outcome: "not_logged_in", line: "`claude` is not logged in. Run `claude` in a terminal and log in with /login.", settings: null },
  }, `${writerStatusBefore}
    document.getElementById("writer-test").click();
    await wait(300);
    const outcome = document.getElementById("writer-outcome");
    if (!outcome?.classList.contains("failed") || !outcome.textContent.includes("/login")) log("snap:fail no failed outcome with its fix: " + outcome?.textContent);${writerStatusAfter("/login")}`);
/** No API key set (#366): the writer is listed but cannot be picked, and the key row offers to add one. */
export const writersKeyNotSet = () =>
  writersOpen({ writers: { writers: [anthropicWriter, claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: null } }, `
    const first = document.querySelector("#writer-pick option");
    if (first?.textContent !== "Anthropic API key (not set)" || !first.disabled) log("snap:fail the unset key is not listed first and unpickable: " + first?.textContent);
    if (!document.getElementById("writer-key-state")?.textContent.includes("not set")) log("snap:fail the key row does not say not set");
    if (document.getElementById("writer-key")?.placeholder !== "Add an API key…") log("snap:fail no field to add an API key");
    if (document.getElementById("writer-key-remove")) log("snap:fail Remove shown with no key set");
    // An empty base URL's placeholder reads as an example, not as part of the URL (#452's review).
    const example = document.getElementById("writer-base-url")?.placeholder;
    if (example !== "e.g. https://api.openai.com/v1…") log("snap:fail the base URL's placeholder is " + JSON.stringify(example));`);
/** An API key set: it comes first in the preset order, and a Test of the Anthropic writer passes. */
export const writersKeySet = () =>
  writersOpen({
    writers: { writers: [{ ...anthropicWriter, found: true }, claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: ["anthropic"], base_url: null },
    test_writer: { outcome: "passed", line: "Passed: `anthropic` answered. Summaries are written with it.", settings: null },
  }, `
    if (document.getElementById("writer-pick").value !== "anthropic") log("snap:fail the preset is not the API key");
    if (document.getElementById("writer-model").value !== "claude-haiku-5-5") log("snap:fail the preset model is not the first");
    if (!document.getElementById("writer-key-state")?.textContent.includes(": set")) log("snap:fail the key row does not say set");
    if (document.getElementById("writer-key")?.placeholder !== "Replace the API key…" || !document.getElementById("writer-key-remove")) log("snap:fail no replace or remove");
    const outcome = document.getElementById("writer-outcome");
    if (!outcome?.classList.contains("passed")) log("snap:fail no passed outcome: " + outcome?.textContent);
    ${confirmFirst(`document.getElementById("writer-key-remove").click()`, "remove_api_key", "Remove key", ["Keychain"])}`, true);
/**
 * A local Ollama set by its base URL alone (#367): the OpenAI-compatible API is preset before Claude, its model is typed,
 * its row shows the base URL and no key, and a Test of it passes.
 * The contrast pass (#455) runs here too: its fields' borders.
 */
export const writersOpenAI = (): Fixture => ({
  ...writersOpen({
    writers: { writers: [anthropicWriter, { ...openaiWriter, found: true }, claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: "http://localhost:11434/v1" },
    test_writer: { outcome: "passed", line: "Passed: `openai` answered. Summaries are written with it.", settings: null },
  }, `
    if (document.getElementById("writer-pick").value !== "openai") log("snap:fail the preset is not the OpenAI-compatible API");
    const model = document.getElementById("writer-model");
    if (model?.tagName !== "INPUT" || model.value !== "gpt-6-luna") log("snap:fail the model is not a typed field at the first model: " + model?.tagName);
    if (document.getElementById("writer-base-url")?.value !== "http://localhost:11434/v1") log("snap:fail the base URL field does not hold the base URL");
    if (!document.getElementById("writer-openai-state")?.textContent.includes("no key")) log("snap:fail the row does not say no key");
    if (!document.getElementById("writer-openai-remove")) log("snap:fail no Remove with a base URL set");
    const outcome = document.getElementById("writer-outcome");
    if (!outcome?.classList.contains("passed")) log("snap:fail no passed outcome: " + outcome?.textContent);
    ${confirmFirst(`document.getElementById("writer-openai-remove").click()`, "remove_api_key", "Remove", ["http://localhost:11434/v1", "Keychain"])}`, true),
  contrast: true,
});
/** No claude: agy writes the summaries, and the section says PR briefs need Claude. */
export const writersBriefsNeedClaude = () =>
  writersOpen({
    writers: { writers: [{ ...claudeWriter, found: false }, agyWriter], summaries_on: true, briefs: false, keys: [], base_url: null },
    settings_files: { settings: '{ "summaries": { "writer": "agy", "model": "gemini-4-neon-high" } }', keymap: null },
  }, `
    if (!document.getElementById("writer-briefs")?.textContent.includes("briefs need Claude")) log("snap:fail no briefs need Claude line");
    if (document.getElementById("writer-model").value !== "gemini-4-neon-high") log("snap:fail the model in effect is not shown");`);
/** #456: a key the Keychain refuses: its error is said in the section's status region, and tied to the key's field. */
export const writersKeyError = () =>
  writersOpen({
    writers: { writers: [anthropicWriter, claudeWriter, agyWriter], summaries_on: false, briefs: true, keys: [], base_url: null },
    set_api_key: { __throw: "the Keychain refused the key" } as never,
  }, `${writerStatusBefore}
    document.getElementById("writer-key").value = "sk-ant-not-a-key";
    document.getElementById("writer-key-save").click();
    await wait(300);
    const error = document.getElementById("writer-key-error");
    if (!error?.textContent.includes("Keychain refused")) return log("snap:fail no key error: " + error?.textContent);
${writerStatusAfter("Keychain refused")}
    const field = document.getElementById("writer-key");
    if (field.getAttribute("aria-invalid") !== "true" || !(field.getAttribute("aria-describedby") ?? "").split(" ").includes("writer-key-error")) log("snap:fail the key error is not tied to its field");`);
