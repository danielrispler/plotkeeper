// The end-of-story bar (spec danielrispler/plotkeeper#378, ticket #393; trimmed and restyled by #448, spec #443): one compact row that
// ends a PR brief's chapter (`map-brief.ts`) or a part catch-up. It holds explain-back, when the story has one, and Skip; the S key skips,
// never while typing. Its 1 to 5 rating and note are gone (#448): nothing read them back and no stop rule counted them.
//
// A reader mounts `storyBar(about, gist).node` after its last step and calls `offer()` when that step shows. Given a gist (a story of a
// Core part, `explainsBack`), the bar's `slot` holds explain-back (#397): one line, Enter shows the gist under it and the three marks as a
// radio group, logged to the shell's stop-rule log (`server/src/story_bar.rs`) as `explain-back-given` and `explain-back-marked`, keyed
// by project and by PR and chapter or by part. The line is an input, so the window's single keys stay off while it is typed, and an
// Enter that confirms an input method's composition does not submit it. Nothing is read back: a bar opened again starts empty.

import { failure, invoke, onEnter, text } from "./dom.js";
import { localDay } from "./stop-rules.js";

type BarKind = "explain-back-given" | "explain-back-marked";
export type BarSend = (kind: BarKind, extra?: { line?: string; mark?: Mark }) => void;
// The longest explain-back line kept, in characters (the shell refuses a longer one).
export const LINE_MAX = 300;
export type Mark = "got-it" | "partly" | "missed";
// The three marks, in words: a mark is never shown or kept as a number.
export const MARKS: { mark: Mark; label: string }[] = [
  { mark: "got-it", label: "Got it" },
  { mark: "partly", label: "Partly" },
  { mark: "missed", label: "Missed" },
];
// Whether a story's bar offers explain-back: only when the story has a gist (the writer adds one to a chapter that lights a Core part, and
// to a part catch-up) and one of its parts is Core under Daniel's tiers now.
export function explainsBack(gist: string | undefined, parts: string[], tierOf: (part: string) => string): boolean {
  return !!gist?.trim() && parts.some((part) => tierOf(part) === "core");
}
// The key that skips the whole bar.
const SKIP_KEY = "s";

// What a bar ends: a chapter of a PR's brief, or a part catch-up.
export type StoryAbout = { project: string; pr: number; chapter: number } | { project: string; part: string };

// The bar's state and what it tells the log: the explain-back line, each mark, and whether Skip closed it.
export function barModel(send: BarSend) {
  let closed = false;
  let said: string | null = null;
  let marked: Mark | null = null;
  return {
    skip() {
      closed = true;
    },
    // Daniel's explain-back line: one per bar.
    explain(line: string): boolean {
      const kept = line.trim();
      if (closed || said !== null || !kept || kept.length > LINE_MAX) return false;
      said = kept;
      send("explain-back-given", { line: kept });
      return true;
    },
    // His mark against the gist, once his line is in; each pick is logged.
    mark(mark: string): boolean {
      const found = MARKS.find((m) => m.mark === mark);
      if (closed || said === null || !found) return false;
      marked = found.mark;
      send("explain-back-marked", { mark: found.mark });
      return true;
    },
    explained: () => said,
    marked: () => marked,
    closed: () => closed,
  };
}

// What a key does to the bar: the skip key skips; nothing while typing or with a modifier held.
export function barKey(key: { key: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean }, typing: boolean): "skip" | null {
  if (typing || key.metaKey || key.ctrlKey || key.altKey) return null;
  return key.key.toLowerCase() === SKIP_KEY ? "skip" : null;
}

export type Bar = { node: HTMLElement; slot: HTMLElement; offer: () => void; key: (found: ReturnType<typeof barKey>) => boolean };
type Model = ReturnType<typeof barModel>;
// The bar on screen that the keys act on: the last one offered, while it is shown and open.
let current: Bar | null = null;
// Each bar's marks get a radio group name of their own.
let bars = 0;

// The bar for `about`, logging through the shell; with `gist`, it offers explain-back first.
export function storyBar(about: StoryAbout, gist?: string): Bar {
  const { project, ...story } = about;
  const model = barModel((kind, extra = {}) => {
    const given = { line: extra.line ?? null, mark: extra.mark ?? null };
    invoke("story_bar", { kind, project, day: localDay(), about: story, ...given }).catch(failure("story_bar"));
  });
  const node = text("div", "", "story-bar");
  node.setAttribute("role", "group");
  node.setAttribute("aria-label", "End of the story");
  const slot = text("div", "", "bar-slot");
  if (gist) slot.append(explainBack(model, gist));
  const skip = text("button", "Skip", "btn quiet small bar-skip");
  skip.setAttribute("type", "button");
  skip.title = `Skip this bar (key ${SKIP_KEY.toUpperCase()})`;
  skip.addEventListener("click", () => {
    model.skip();
    paint();
  });
  node.append(slot, skip);
  const paint = () => {
    node.hidden = model.closed();
  };
  paint();
  const bar: Bar = {
    node,
    slot,
    offer: () => {
      current = bar;
    },
    key: (found) => {
      if (!found || model.closed()) return false;
      model.skip();
      paint();
      return true;
    },
  };
  return bar;
}

// Explain-back: the line, then, once Enter keeps it, the line again with the gist under it and the three marks.
function explainBack(model: Model, gist: string): HTMLElement {
  const box = text("div", "", "bar-explain");
  const field = document.createElement("input");
  field.className = "bar-line";
  field.type = "text";
  field.maxLength = LINE_MAX;
  field.name = "explain-back";
  field.autocomplete = "off";
  field.placeholder = "Say it back in one line, e.g. “Reading a brief clears its parts’ fog”…";
  field.title = "Optional: say the story back in your own words, then Enter compares it with its gist";
  field.setAttribute("aria-label", "Say this story back in one line");
  onEnter(field, () => {
    if (model.explain(field.value)) box.replaceChildren(...compared(model, gist));
  });
  box.append(field);
  return box;
}

// The line said, the gist, and the marks: a radio group with a legend, each radio inside its label so both are one hit target.
function compared(model: Model, gist: string): HTMLElement[] {
  const said = text("p", model.explained() ?? "", "bar-said");
  const against = text("p", "", "bar-gist");
  against.append(text("span", "Gist", "bar-gist-label"), document.createTextNode(` ${gist}`));
  const marks = text("fieldset", "", "bar-marks");
  const name = `bar-mark-${++bars}`;
  marks.append(text("legend", "How close was your line?", "bar-marks-legend"));
  for (const { mark, label } of MARKS) {
    const choice = text("label", "", "bar-mark");
    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = name;
    radio.value = mark;
    radio.addEventListener("change", () => {
      if (radio.checked) model.mark(mark);
    });
    choice.append(radio, text("span", label, "bar-mark-word"));
    marks.append(choice);
  }
  return [said, against, marks];
}

// A window key for the bar on screen; true when the bar took it. The caller passes `typing` (focus in a field) so the line keeps its keys.
export function barKeyDown(event: KeyboardEvent, typing: boolean): boolean {
  if (!current?.node.isConnected || current.node.hidden || current.node.offsetParent === null) return false;
  return current.key(barKey(event, typing));
}
