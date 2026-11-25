// The page's explanations (spec #520, ticket #522): one short muted line under each section of a dense panel, and the selected option's
// meaning under its choice. Like `hovers.ts`, a text is written once here and every place that shows it reads it from here; each is
// worded to "you" from `docs/studio/CONTEXT.md`, at most 12 words. One page-wide setting, `explanations`, shows or hides them all.

import { failure, hint, invoke, pressable, text } from "./dom.js";
import type { LinkKind, Tier } from "./map-model.js";
import { studio } from "./state.js";

export const SHEET = {
  holds: "What this part owns: its files, and notes on them.",
  tier: "How much of this part you keep in your head.",
  tiers: {
    core: "Core: any change here fogs it, so you catch up.",
    interface: "Interface-only: fogs only when its laws, links or flows change.",
    "black-box": "Black box: never fogs; you trust it without following it.",
  } satisfies Record<Tier, string>,
  laws: "What must stay true here; a test can stand behind each.",
  talksTo: "Its links to other parts: → it uses them, ← they use it.",
  kinds: {
    file: "file: one reads or writes the other's files.",
    spawn: "spawn: one starts the other as a program.",
    http: "http: one calls the other over the network.",
    import: "import: one's code imports the other's code.",
  } satisfies Record<LinkKind, string>,
  // The spec's wording was 13 words; one "it" went to keep the 12-word rule.
  notYet: "Not yet ratified: the agent added it; Accept keeps, Overturn undoes it.",
  fog: "Changes on main you haven't caught up on yet.",
  // Under the fog buttons, by which are shown: the part catch-up (the spec's wording), the PR brief chapter that lights the part, or
  // "I know this" alone.
  fogActions: {
    catchUp: "Part catch-up reads what changed; I know this just clears it.",
    chapter: "The brief shows what changed; I know this just clears it.",
    know: "I know this clears the fog without reading what changed.",
  },
  ci: "The last CI run on main, for this part's laws.",
  history: "Commits on main that changed this part, newest last.",
  // "This session here" (#561), shown while a session's story is open beside the sheet.
  here: "The open session's turns that changed this part, newest first.",
} as const;

// The line under each heading the part sheet draws, by the heading's words.
export const SHEET_HEADINGS: Record<string, string> = {
  Holds: SHEET.holds,
  Tier: SHEET.tier,
  Laws: SHEET.laws,
  "Talks to": SHEET.talksTo,
  Fog: SHEET.fog,
  "CI on main": SHEET.ci,
  History: SHEET.history,
  "This session here": SHEET.here,
};

// The PR brief's reader (#524): a line under each step's title, by the step's kind, and under Also touched. A part catch-up plays in
// the same reader, so a kind's line holds for a PR and a catch-up alike.
export const BRIEF = {
  steps: {
    Where: "Where this change sits on your map.",
    Before: "How it worked before this change.",
    Now: "How it works after this change.",
    "Must stay true": "A law this change must keep true.",
    "Could break": "What this change might break, and the cheapest way to see it.",
    "Not in the plan": "What the agent did that the spec didn't ask for.",
    "Your turn": "A question for you; answer it to check your reading.",
  } as Record<string, string>,
  also: "Other files this PR changed, grouped by kind.",
} as const;

// Ratify, the brief's last step (#524): a line under its title, and "not yet ratified"'s under its first undecided item, worded as
// on the sheet.
export const RATIFY = {
  accept: "Changes this PR made to your map; say whether each stands.",
  guard: "A law with a test keeps this bug from coming back.",
  notYet: SHEET.notYet,
} as const;

// The line under each heading the brief's reader draws, by its words: a step's kind, and Also touched.
export const BRIEF_HEADINGS: Record<string, string> = { ...BRIEF.steps, "Also touched": BRIEF.also };
// The line under the Ratify step's title, by the title `ratifyStep` gives it.
export const RATIFY_HEADINGS: Record<string, string> = { "Accept or overturn": RATIFY.accept, "Guard with a law": RATIFY.guard };

// A step's line: the Ratify step's by its title, any other by its kind; none for a kind the reader has no line for.
export const stepLine = (step: { k: string; h: string }): string | undefined => (step.k === "Ratify" ? RATIFY_HEADINGS[step.h] : BRIEF.steps[step.k]);

// Whether the explanations show: the `explanations` setting, on for a fresh install.
export const explanationsOn = () => studio.files.settings.explanations;

// What applies `settings.json`'s new text once a write returns it (`main.ts`), so the switch takes effect at once.
let saved: (settings: string) => void = () => {};
export function startExplanations(apply: (settings: string) => void) {
  saved = apply;
}

// What draws again when the explanations switch: each explained panel's redraw.
const redraws: (() => void)[] = [];
export const onExplanations = (redraw: () => void) => void redraws.push(redraw);
// Called each time the settings apply (`main.ts`): a changed `explanations` draws every explained panel again.
let shown: boolean | null = null;
export function explanationsApplied() {
  if (shown !== null && shown !== explanationsOn()) redraws.forEach((redraw) => redraw());
  shown = explanationsOn();
}

// The setting the write in flight asks for, null with none, and what flips made meanwhile ask for, null with none. A write carries the
// text the page last read, so one sent before the last one's reply lands would be refused as stale: it waits, and once that reply
// applies, one more write goes only if the setting is not yet what was asked (a double click asks for where it started, so it ends
// there, with nothing more written).
let asked: boolean | null = null;
let queued: boolean | null = null;

// Turns the explanations `on` or off in `settings.json`, through the server's settings write, from the text the page last read.
function setExplanations(on: boolean) {
  if (asked !== null) {
    queued = on;
    return;
  }
  asked = on;
  invoke("set_explanations", { on, expected: studio.texts.settings })
    .then((settings) => {
      saved(settings);
      return true;
    })
    .catch((e) => {
      failure("set_explanations")(e);
      return false;
    })
    .then((wrote) => {
      const next = queued;
      [asked, queued] = [null, null];
      // A refused write drops the waiting flip: the page's text may be stale, and the error is already shown.
      if (wrote && next !== null && next !== explanationsOn()) setExplanations(next);
    });
}

// Flips the explanations: the palette's `studio::ToggleExplanations`, and the `?` of each explained panel. A flip while a write is in
// flight flips what was last asked, not what shows yet.
export const toggleExplanations = () => setExplanations(!(queued ?? asked ?? explanationsOn()));

// The `?` toggle beside a panel's ×: its name stays "Explanations" while `aria-pressed` says whether they show (the name never repeats
// the state); filled when on, a muted ring when off (`style.css`).
// The click reads the setting then, so a `?` kept across redraws (the brief's, built once per brief) flips what shows now.
export function explainToggle(on: boolean): HTMLElement {
  const button = pressable(text("button", "?", "quiet explain-toggle"), on, toggleExplanations);
  button.setAttribute("aria-label", "Explanations");
  return pressToggle(button, on);
}

// Sets a `?`'s pressed state and hover to `on`, in place, so a kept `?` keeps the focus a click gave it.
export function pressToggle(button: HTMLElement, on: boolean): HTMLElement {
  button.setAttribute("aria-pressed", String(on));
  hint(button, on ? "Hide explanations" : "Show explanations");
  return button;
}

// Each line's id, new at every draw, for `aria-describedby`.
let lines = 0;
export function explainLine(words: string, className = ""): HTMLElement {
  const node = text("p", words, `explain${className ? ` ${className}` : ""}`);
  node.id = `explain-${++lines}`;
  return node;
}

// A section's heading with its line under it while `on`, the heading described by it; the bare heading while off.
export function explained(heading: HTMLElement, words: string, on: boolean): HTMLElement[] {
  if (!on) return [heading];
  const note = explainLine(words);
  heading.setAttribute("aria-describedby", note.id);
  return [heading, note];
}

// A choice (the tier buttons) with the selected option's meaning under it while `on`, the option described by it.
export function explainedChoice(choice: HTMLElement, selected: HTMLElement | undefined, words: string, on: boolean): HTMLElement[] {
  if (!on) return [choice];
  const note = explainLine(words, "explain-choice");
  selected?.setAttribute("aria-describedby", note.id);
  return [choice, note];
}

// "Talks to"'s rows, each `links` entry its link's kind and whether it is not yet ratified: while `on`, a kind's line follows only the
// first link of that kind, and "not yet ratified"'s only the first unratified link, inside the row.
export function explainedLinks(rows: HTMLElement[], links: { kind?: LinkKind | undefined; unratified: boolean }[], on: boolean): HTMLElement[] {
  if (!on) return rows;
  const told = new Set<string>();
  rows.forEach((row, at) => {
    const { kind, unratified } = links[at] ?? { unratified: false };
    if (kind && !told.has(kind)) {
      told.add(kind);
      row.append(explainLine(SHEET.kinds[kind], "explain-link"));
    }
    if (unratified && !told.has("not-yet")) {
      told.add("not-yet");
      row.append(explainLine(SHEET.notYet, "explain-link"));
    }
  });
  return rows;
}
