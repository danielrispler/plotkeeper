// The end-of-story bar (spec danielrispler/plotkeeper#378, ticket #393): one compact row that ends a PR brief's chapter (`map-brief.ts`)
// and, once it exists, a part catch-up. It holds an optional 1 to 5 rating of the explanation, an optional short note, and Skip; the
// digit keys rate and S skips, never while typing in the note. What is given goes to the shell's stop-rule log (`server/src/story_bar.rs`),
// keyed by project and by PR and chapter or by part, as `rating-offered`, `rating-given`, `rating-skipped` and `note-given`. Nothing is
// read back: a bar opened again starts empty, and no rating is ever shown as a score.
//
// A reader mounts `storyBar(about).node` after its last step and calls `offer()` when that step shows. A later ticket (#397, explain-back
// on Core parts) adds its own field into `slot`, before the rating, and logs through the same command.

import { failure, invoke, text } from "./dom.js";
import { localDay } from "./stop-rules.js";

export type BarKind = "rating-offered" | "rating-given" | "rating-skipped" | "note-given";
export type BarSend = (kind: BarKind, extra?: { rating?: number; note?: string }) => void;
// The longest note kept, in characters (the shell refuses a longer one).
export const NOTE_MAX = 500;
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
// The bar on screen that the keys act on: the last one offered, while it is shown and open.
let current: Bar | null = null;

// The bar for `about`, logging through the shell.
export function storyBar(about: StoryAbout): Bar {
  const { project, ...story } = about;
  const model = barModel((kind, extra = {}) => {
    invoke("story_bar", { kind, project, day: localDay(), about: story, rating: extra.rating ?? null, note: extra.note ?? null }).catch(failure("story_bar"));
  });
  const node = text("div", "", "story-bar");
  node.setAttribute("role", "group");
  node.setAttribute("aria-label", "How clear was this explanation");
  const slot = text("div", "", "bar-slot");
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

// A window key for the bar on screen; true when the bar took it. The caller passes `typing` (focus in a field) so the note keeps its keys.
export function barKeyDown(event: KeyboardEvent, typing: boolean): boolean {
  if (!current || !current.node.isConnected || current.node.hidden || current.node.offsetParent === null) return false;
  return current.key(barKey(event, typing));
}
