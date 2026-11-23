// The end-of-story bar (spec danielrispler/plotkeeper#378, ticket #393): one compact row that ends a PR brief's chapter (`map-brief.ts`)
// and, once it exists, a part catch-up. It holds an optional 1 to 5 rating of the explanation, an optional short note, and Skip; the
// digit keys rate and S skips, never while typing in the note. What is given goes to the shell's stop-rule log (`server/src/story_bar.rs`),
// keyed by project and by PR and chapter or by part, as `rating-offered`, `rating-given`, `rating-skipped` and `note-given`. Nothing is
// read back: a bar opened again starts empty, and no rating is ever shown as a score.
//
// A reader mounts `storyBar(about, gist).node` after its last step and calls `offer()` when that step shows. Given a gist (a story of a
// Core part, `explainsBack`), the bar's `slot` holds explain-back (#397): one line, Enter shows the gist under it and the three marks, logged
// as `explain-back-given` and `explain-back-marked`. The line is an input, so the window's single keys stay off while it is typed.

import { failure, invoke, text } from "./dom.js";
import { localDay } from "./stop-rules.js";

export type BarKind = "rating-offered" | "rating-given" | "rating-skipped" | "note-given" | "explain-back-given" | "explain-back-marked";
export type BarSend = (kind: BarKind, extra?: { rating?: number; note?: string; line?: string; mark?: Mark }) => void;
// The longest note kept, in characters (the shell refuses a longer one).
export const NOTE_MAX = 500;
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
export const SKIP_KEY = "s";

// What a bar ends: a chapter of a PR's brief, or a part catch-up.
export type StoryAbout = { project: string; pr: number; chapter: number } | { project: string; part: string };

// The bar's state and what it tells the log: offered once, each rating picked, a note, and a skip when nothing was given before it.
export function barModel(send: BarSend) {
  let offered = false;
  let given = false;
  let closed = false;
  let rated: number | null = null;
  let said: string | null = null;
  let marked: Mark | null = null;
  return {
    offer() {
      if (offered) return;
      offered = true;
      send("rating-offered");
    },
    rate(n: number) {
      if (closed || !Number.isInteger(n) || n < 1 || n > 5) return;
      rated = n;
      given = true;
      send("rating-given", { rating: n });
    },
    note(line: string): boolean {
      const note = line.trim();
      if (closed || !note || note.length > NOTE_MAX) return false;
      given = true;
      send("note-given", { note });
      return true;
    },
    skip() {
      if (closed) return;
      closed = true;
      if (!given) send("rating-skipped");
    },
    // Daniel's explain-back line: one per bar.
    explain(line: string): boolean {
      const kept = line.trim();
      if (closed || said !== null || !kept || kept.length > LINE_MAX) return false;
      said = kept;
      given = true;
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
    // The rating picked while this bar is open, for its pressed button only.
    rated: () => rated,
    closed: () => closed,
  };
}

// What a key does to the bar: a digit 1 to 5 rates, the skip key skips; nothing while typing or with a modifier held.
export function barKey(key: { key: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean }, typing: boolean): { rate: number } | "skip" | null {
  if (typing || key.metaKey || key.ctrlKey || key.altKey) return null;
  if (key.key.toLowerCase() === SKIP_KEY) return "skip";
  return /^[1-5]$/.test(key.key) ? { rate: Number(key.key) } : null;
}

export type Bar = { node: HTMLElement; slot: HTMLElement; offer: () => void; key: (found: ReturnType<typeof barKey>) => boolean };
type Model = ReturnType<typeof barModel>;
// The bar on screen that the keys act on: the last one offered, while it is shown and open.
let current: Bar | null = null;

// The bar for `about`, logging through the shell; with `gist`, it offers explain-back first.
export function storyBar(about: StoryAbout, gist?: string): Bar {
  const { project, ...story } = about;
  const model = barModel((kind, extra = {}) => {
    const given = { rating: extra.rating ?? null, note: extra.note ?? null, line: extra.line ?? null, mark: extra.mark ?? null };
    invoke("story_bar", { kind, project, day: localDay(), about: story, ...given }).catch(failure("story_bar"));
  });
  const node = text("div", "", "story-bar");
  node.setAttribute("role", "group");
  node.setAttribute("aria-label", "How clear was this explanation");
  const slot = text("div", "", "bar-slot");
  if (gist) slot.append(explainBack(model, gist));
  const ask = text("span", "How clear was this?", "bar-ask");
  const scale = text("div", "", "bar-scale");
  const buttons = [1, 2, 3, 4, 5].map((n) => {
    const button = text("button", String(n), "btn bar-rate");
    button.setAttribute("type", "button");
    button.title = `Rate this explanation ${n} of 5 (key ${n}); 1 is unclear, 5 is clear`;
    button.addEventListener("click", () => (model.rate(n), paint()));
    return button;
  });
  scale.append(...buttons);
  const field = document.createElement("input");
  field.className = "bar-note";
  field.type = "text";
  field.maxLength = NOTE_MAX;
  field.placeholder = "A note on what was unclear (Enter keeps it)";
  field.title = "An optional note on the explanation; Enter keeps it";
  field.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    if (model.note(field.value)) field.replaceWith(text("span", "Note kept.", "bar-kept"));
  });
  const skip = text("button", "Skip", "btn quiet small bar-skip");
  skip.setAttribute("type", "button");
  skip.title = `Skip this bar (key ${SKIP_KEY.toUpperCase()})`;
  skip.addEventListener("click", () => (model.skip(), paint()));
  node.append(slot, ask, scale, field, skip);
  const paint = () => {
    buttons.forEach((button, i) => button.setAttribute("aria-pressed", String(model.rated() === i + 1)));
    node.hidden = model.closed();
  };
  paint();
  const bar: Bar = {
    node,
    slot,
    offer: () => {
      current = bar;
      model.offer();
    },
    key: (found) => {
      if (!found || model.closed()) return false;
      if (found === "skip") model.skip();
      else model.rate(found.rate);
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
  field.placeholder = "Say this story back in one line (Enter shows the gist)";
  field.title = "Optional: say the story back in your own words, then compare it with its gist";
  field.setAttribute("aria-label", "Say this story back in one line");
  field.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    if (model.explain(field.value)) box.replaceChildren(...compared(model, gist));
  });
  box.append(field);
  return box;
}

// The line said, the gist, and the marks.
function compared(model: Model, gist: string): HTMLElement[] {
  const said = text("p", model.explained() ?? "", "bar-said");
  const against = text("p", "", "bar-gist");
  against.append(text("span", "Gist", "bar-gist-label"), document.createTextNode(` ${gist}`));
  const marks = text("div", "", "bar-marks");
  marks.setAttribute("role", "group");
  marks.setAttribute("aria-label", "How close was your line");
  const buttons = MARKS.map(({ mark, label }) => {
    const button = text("button", label, "btn bar-mark");
    button.setAttribute("type", "button");
    button.setAttribute("aria-pressed", "false");
    button.title = `Mark your line: ${label.toLowerCase()}`;
    button.addEventListener("click", () => {
      if (model.mark(mark)) buttons.forEach((each, i) => each.setAttribute("aria-pressed", String(MARKS[i].mark === mark)));
    });
    return button;
  });
  marks.append(...buttons);
  return [said, against, marks];
}

// A window key for the bar on screen; true when the bar took it. The caller passes `typing` (focus in a field) so the note keeps its keys.
export function barKeyDown(event: KeyboardEvent, typing: boolean): boolean {
  if (!current || !current.node.isConnected || current.node.hidden || current.node.offsetParent === null) return false;
  return current.key(barKey(event, typing));
}
