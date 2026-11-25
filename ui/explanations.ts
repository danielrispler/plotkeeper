// The page's explanations (spec #520, ticket #522): one short muted line under each section of a dense panel, and the selected option's
// meaning under its choice. Like `hovers.ts`, a text is written once here and every place that shows it reads it from here; each is
// worded to "you" from `docs/studio/CONTEXT.md`, at most 12 words. One page-wide setting, `explanations`, shows or hides them all.

import { failure, invoke, pressable, text } from "./dom.js";
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
  fogActions: "Part catch-up reads what changed; I know this just clears it.",
  ci: "The last CI run on main, for this part's laws.",
  history: "Commits on main that changed this part, newest last.",
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
};

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

// Turns the explanations `on` or off in `settings.json`, through the server's settings write, from the text the page last read.
function setExplanations(on: boolean) {
  invoke("set_explanations", { on, expected: studio.texts.settings }).then(saved).catch(failure("set_explanations"));
}

// Flips the explanations: the palette's `studio::ToggleExplanations`, and the `?` of each explained panel.
export const toggleExplanations = () => setExplanations(!explanationsOn());

// The `?` toggle beside a panel's ×: its name stays "Explanations" while `aria-pressed` says whether they show (the name never repeats
// the state); filled when on, a muted ring when off (`style.css`).
export function explainToggle(on: boolean): HTMLElement {
  const button = pressable(text("button", "?", "quiet explain-toggle"), on, () => setExplanations(!on));
  button.setAttribute("aria-label", "Explanations");
  button.title = on ? "Hide explanations" : "Show explanations";
  return button;
}

// Each line's id, new at every draw, for `aria-describedby`.
let lines = 0;
function line(words: string, className = ""): HTMLElement {
  const node = text("p", words, `explain${className ? ` ${className}` : ""}`);
  node.id = `explain-${++lines}`;
  return node;
}

// A section's heading with its line under it while `on`, the heading described by it; the bare heading while off.
export function explained(heading: HTMLElement, words: string, on: boolean): HTMLElement[] {
  if (!on) return [heading];
  const note = line(words);
  heading.setAttribute("aria-describedby", note.id);
  return [heading, note];
}

// A choice (the tier buttons) with the selected option's meaning under it while `on`, the option described by it.
export function explainedChoice(choice: HTMLElement, selected: HTMLElement | undefined, words: string, on: boolean): HTMLElement[] {
  if (!on) return [choice];
  const note = line(words, "explain-choice");
  selected?.setAttribute("aria-describedby", note.id);
  return [choice, note];
}

// "Talks to"'s rows, each `links` entry its link's kind and whether it is not yet ratified: while `on`, a kind's line follows only the
// first link of that kind, and "not yet ratified"'s only the first unratified link, inside the row.
export function explainedLinks(rows: HTMLElement[], links: { kind?: LinkKind; unratified: boolean }[], on: boolean): HTMLElement[] {
  if (!on) return rows;
  const told = new Set<string>();
  rows.forEach((row, at) => {
    const { kind, unratified } = links[at];
    if (kind && !told.has(kind)) {
      told.add(kind);
      row.append(line(SHEET.kinds[kind], "explain-link"));
    }
    if (unratified && !told.has("not-yet")) {
      told.add("not-yet");
      row.append(line(SHEET.notYet, "explain-link"));
    }
  });
  return rows;
}
