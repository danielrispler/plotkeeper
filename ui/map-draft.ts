// The Map draft on the map (spec danielrispler/plotkeeper#753, "The drafting run", stories 14-22; ticket #763): while First map drafts,
// the scan's parts are its plates, and each fills in as its writer group lands, "being checked" until its reviewer answers. The panel beside
// them says how far the draft is, offers Retry of a failed group alone, and holds a card a part: its name, caption and purpose to edit in
// place, the reviewer's flag on its purpose with the lines it cites, its tier, and Redraft this part. Accept stays locked until every
// reviewer answered (`ready`); its ways out are #764's. Discard asks first, then returns to the scan.
//
// The server keeps the Map draft (`map_draft.rs`); each answer, and each `map-landed` while it drafts, draws it again here. A screen
// already drawn is patched, plates and panel alike, so a field being typed in keeps its words and its focus while another group lands;
// the fields not being typed in say the Map draft's text again.

import { availability, confirmed, hint, isUnavailable, patch, pressable, text } from "./dom.js";
import { draftProgress, type DraftStage, type MapDoc, parseMap, partStage, type Tier } from "./map-model.js";
import { plateCanvas } from "./map-scan.js";
import type { MapDraft, MapState } from "./types.js";

type Doc = Extract<MapDoc, { ok: true }>;
type Part = Doc["parts"][number];
/** A part's field the person edits in the Map draft. */
export type DraftField = "name" | "caption" | "purpose" | "tier";
/**
 * What the Map draft's presses do: `edit` keeps one field of a part in the Map draft, `retry` runs a failed group again, `redraft` runs
 * one part's writer again, `discard` throws the Map draft away; `note` is why the last of them did not go through, or null.
 */
export type DraftActs = {
  edit: (part: string, field: DraftField, value: string) => void;
  retry: (group: number) => void;
  redraft: (part: string) => void;
  discard: () => void;
  note: string | null;
};

const tiers: [Tier, string][] = [["core", "Core"], ["interface", "Interface"], ["black-box", "Black box"]];
const coreLine = "Mark the parts you work in as Core: they'll tell you when they change.";
// A part whose writer or reviewer is at work: it cannot be redrafted, and while it is written its fields wait for its text.
const busy = (stage: DraftStage) => stage === "writing" || stage === "redrafting" || stage === "checking";
const writing = (stage: DraftStage) => stage === "writing" || stage === "redrafting";
// "a, b and c".
const listed = (items: string[]) => (items.length < 2 ? items.join("") : `${items.slice(0, -1).join(", ")} and ${items.at(-1)}`);

function stageWords(stage: DraftStage, draft: MapDraft, id: string): string {
  const words: Record<DraftStage, string> = {
    writing: "Being written…",
    redrafting: "Being redrafted…",
    checking: "Being checked",
    checked: "Checked",
    flagged: `Flagged: ${draft.verdicts[id]?.verdict ?? "flagged"}`,
    "writing-failed": "Writing failed",
    "checking-failed": "Checking failed",
  };
  return words[stage];
}

// The id of a part's field, which its plate's press focuses.
const fieldId = (id: string, field: DraftField) => `map-drafting-${id}-${field}`;

// A part's plate: its name and caption once written (its folder before), and where it stands; its press goes to its card in the panel.
function plateOf(part: Part, draft: MapDraft): HTMLElement {
  const stage = partStage(draft, part.id);
  const plate = text("div", "", `plate draft ${stage}`);
  plate.dataset.id = part.id;
  plate.dataset.key = `draft plate ${part.id}`;
  const open = pressable(text("button", "", "btn plate-open"), null, () => {
    const name = document.getElementById(fieldId(part.id, "name"));
    name?.closest(".map-drafting-part")?.scrollIntoView({ block: "nearest" });
    name?.focus({ preventScroll: true });
  });
  open.dataset.key = `draft plate open ${part.id}`;
  const caption = stage === "writing" ? (part.paths.join(", ") || part.caption) : part.caption;
  const state = stageWords(stage, draft, part.id);
  open.append(text("span", part.name, "plate-name"), text("span", caption, "plate-caption"), text("span", state, "plate-state"));
  hint(open, `${part.name}: ${state}\nEdit it in the panel`);
  plate.append(open);
  return plate;
}

// A labelled field of a part's card, saying the Map draft's text, kept in the Map draft once committed.
function fieldOf(part: Part, field: Exclude<DraftField, "tier">, words: string, readOnly: boolean, acts: DraftActs): HTMLElement {
  const row = text("div", "", "map-drafting-field");
  const label = text("label", words, "map-scan-label");
  const input = field === "purpose" ? text("textarea", part.purpose) : text("input", "");
  input.id = fieldId(part.id, field);
  input.name = field;
  input.setAttribute("autocomplete", "off");
  if (input instanceof HTMLInputElement) input.setAttribute("value", part[field]);
  // The writer's words are not checked for spelling as the person's own are.
  input.spellcheck = false;
  input.readOnly = readOnly;
  label.htmlFor = input.id;
  input.addEventListener("change", () => acts.edit(part.id, field, input.value));
  row.append(label, input);
  return row;
}

// The reviewer's flag on a part's purpose: the verdict, its reason and the lines it cites. It never locks Accept.
function flagOf(draft: MapDraft, id: string): HTMLElement | null {
  const item = draft.verdicts[id];
  if (!item?.flagged) return null;
  const flag = text("div", "", "map-drafting-flag");
  flag.append(text("p", `The reviewer says ${item.verdict}: ${item.why}`, "map-drafting-why"));
  if (item.cites.length) {
    const cites = text("ul", "", "map-drafting-cites");
    cites.setAttribute("aria-label", "The lines it cites");
    for (const cite of item.cites) {
      const line = text("code", cite);
      line.setAttribute("translate", "no");
      const each = text("li", "");
      each.append(line);
      cites.append(each);
    }
    flag.append(cites);
  }
  return flag;
}

// A part's card: where it stands, its fields, its flag, its tier and Redraft this part.
function cardOf(part: Part, draft: MapDraft, acts: DraftActs): HTMLElement {
  const stage = partStage(draft, part.id);
  const card = text("section", "", "map-drafting-part");
  card.dataset.id = part.id;
  card.dataset.key = `draft part ${part.id}`;
  card.setAttribute("aria-label", part.paths.join(", ") || part.id);
  const head = text("p", "", "map-drafting-head");
  const where = text("code", part.paths.join(", ") || part.id);
  where.setAttribute("translate", "no");
  head.append(where, text("span", stageWords(stage, draft, part.id), `map-drafting-state ${stage}`));
  const wait = writing(stage);
  card.append(head, fieldOf(part, "name", "Name", wait, acts), fieldOf(part, "caption", "Caption", wait, acts), fieldOf(part, "purpose", "Purpose", wait, acts));
  const flag = flagOf(draft, part.id);
  if (flag) card.append(flag);
  const foot = text("div", "", "map-drafting-foot");
  const label = text("label", "Tier", "map-scan-label");
  const pick = text("select", "", "map-scan-pick");
  pick.id = fieldId(part.id, "tier");
  pick.name = "tier";
  label.htmlFor = pick.id;
  for (const [tier, words] of tiers) {
    const option = Object.assign(text("option", words), { value: tier });
    if (tier === part.tier) option.setAttribute("selected", "");
    pick.append(option);
  }
  pick.addEventListener("change", () => acts.edit(part.id, "tier", pick.value));
  const redraft = pressable(text("button", "Redraft this part", "btn small"), null, () => {
    if (!isUnavailable(redraft)) acts.redraft(part.id);
  });
  redraft.dataset.key = `draft redraft ${part.id}`;
  const why = busy(stage) ? "This part is being written or checked: redraft it once its reviewer answers" : null;
  availability(redraft, "Write this part again, with its flag as input; its reviewer checks it again, and Accept locks until it answers", why);
  foot.append(label, pick, redraft);
  card.append(foot);
  return card;
}

// Each failed group: what failed, for which parts, why, and Retry of that group alone.
function failedOf(draft: MapDraft, doc: Doc, acts: DraftActs): HTMLElement[] {
  const nameOf = (id: string) => doc.parts.find((part) => part.id === id)?.name ?? id;
  return draft.groups.flatMap((group, at) => {
    if (group.state !== "writer-failed" && group.state !== "reviewer-failed") return [];
    const row = text("div", "", "map-drafting-failed");
    row.dataset.key = `draft failed ${at}`;
    const what = group.state === "writer-failed" ? "Writing" : "Checking";
    const line = text("p", `${what} ${listed(group.parts.map(nameOf))} failed: ${group.why ?? "no reason was given"}`, "map-scan-why");
    const retry = pressable(text("button", "Retry", "btn small"), null, () => acts.retry(at));
    retry.dataset.key = `draft retry ${at}`;
    hint(retry, "Run this group again; the other parts stay as they are");
    row.append(line, retry);
    return [row];
  });
}

// The panel: how far the draft is, the failed groups, the Core line, a card a part, what the writers left out, Accept and Discard.
function panelOf(draft: MapDraft, doc: Doc, acts: DraftActs): HTMLElement {
  const panel = text("aside", "", "map-scan-panel map-drafting-panel");
  panel.setAttribute("aria-label", "Map draft");
  const said = (line: string, className: string, key: string) => {
    const node = text("p", line, className);
    node.dataset.key = key;
    return node;
  };
  const progress = said(draftProgress(draft, doc.parts.map((part) => part.id)), "map-drafting-progress", "draft progress");
  progress.setAttribute("role", "status");
  panel.append(
    said("Map draft", "map-empty-title", "draft title"),
    said("Claude Code names and describes these parts in the background, and a reviewer checks each purpose. Nothing is written to your repository until you accept.", "empty", "draft teach"),
    progress,
    ...failedOf(draft, doc, acts),
  );
  if (acts.note) {
    const note = said(`That did not go through: ${acts.note}. Try it again.`, "map-scan-why", "draft note");
    note.setAttribute("role", "status");
    panel.append(note);
  }
  panel.append(said(coreLine, "map-scan-note map-drafting-core", "draft core"), ...doc.parts.map((part) => cardOf(part, draft, acts)));
  if (draft.not_written.length) {
    const left = text("div", "", "map-drafting-left");
    left.dataset.key = "draft left out";
    const list = text("ul", "", "map-drafting-cites");
    list.append(...draft.not_written.map((item) => text("li", `${item.what}: ${item.why}`)));
    left.append(text("p", "The writers left these out rather than guess:", "map-scan-note"), list);
    panel.append(left);
  }
  const actions = text("div", "", "map-drafting-actions");
  actions.dataset.key = "draft actions";
  // Accept's ways out are #764's: here it only shows whether every reviewer answered.
  const accept = pressable(text("button", "Accept", "btn primary"), null);
  accept.dataset.key = "draft accept";
  availability(accept, "Accept this Map draft as the repository's map", draft.ready ? null : "Accept unlocks once a reviewer has checked every part");
  const discard = pressable(text("button", "Discard", "btn"), null, async () => {
    const lost = "The Map draft's names, captions and purposes and your edits are thrown away, and its runs stop. The scan stays, to draft again.";
    if (await confirmed({ heading: "Discard the Map draft?", lost, verb: "Discard draft" })) acts.discard();
  });
  discard.dataset.key = "draft discard";
  hint(discard, "Throw the Map draft away and return to the scan");
  actions.append(accept, discard);
  panel.append(actions);
  return panel;
}

// The Map draft's text in each field not being typed in, as a patch leaves a field the person changed with its own words.
function sayDraft(screen: Element) {
  for (const field of screen.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>(".map-drafting-part :is(input, textarea, select)")) {
    if (field === document.activeElement) continue;
    if (field instanceof HTMLSelectElement) field.value = field.querySelector("option[selected]")?.getAttribute("value") ?? field.value;
    else field.value = field.defaultValue;
  }
}

/**
 * Draws `draft`, `state`'s project's Map draft, into `body`; a draft whose map does not read draws nothing and says false. A screen already
 * drawn for the project is patched, its plates and its panel, so their nodes, the panel's scroll and the focus stay.
 */
export function drawDraft(body: HTMLElement, state: MapState, draft: MapDraft, acts: DraftActs): boolean {
  const doc = parseMap(draft.map);
  if (!doc.ok) return false;
  const key = `draft ${state.project}`;
  const parts = doc.parts.map((part) => part.id).join(" ");
  const canvas = plateCanvas(doc, (part) => plateOf(part, draft));
  canvas.dataset.parts = parts;
  const panel = panelOf(draft, doc, acts);
  const drawn = body.firstElementChild;
  if (drawn instanceof HTMLElement && drawn.classList.contains("map-drafting") && drawn.dataset.key === key && body.childElementCount === 1) {
    const kept = drawn.querySelector<HTMLElement>(".map-scan-canvas");
    const [world, next] = [kept?.querySelector(".map-world"), canvas.querySelector(".map-world")];
    // The same parts are placed where they were: their plates are patched, and the camera's fit stays.
    if (kept?.dataset.parts === parts && world && next) patch(world, [...next.childNodes]);
    else kept?.replaceWith(canvas);
    const side = drawn.querySelector(".map-drafting-panel");
    if (side) patch(side, [...panel.childNodes]);
  } else {
    const screen = text("div", "", "map-scan map-drafting");
    screen.dataset.key = key;
    screen.append(canvas, panel);
    body.replaceChildren(screen);
  }
  sayDraft(body);
  return true;
}
