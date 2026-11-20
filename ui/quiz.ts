// A turn's own Quiz in "now" (danielrispler/plotkeeper#118, #120): open under the evidence line, one click to answer. A pick, or
// "Show the rest anyway", reveals the reasons of every option and the Body lines held back until then. Revealing changes only
// the Quiz's region: its children are drawn again and nothing else in the rail is, so the scroll position stays. What Daniel
// did is kept by turn id, which outlasts a redraw of the rail, and sent to the card store, which keeps it with its time.

import { failure, pressable, prose, text } from "./dom.js";
import type { Quiz, QuizTap, SummaryLine, Tap } from "./types.js";
import { HOVERS } from "./hovers.js";

export type QuizStage = "open" | "picked" | "skipped";
export type QuizView = { reasons: boolean; rest: boolean; chips: boolean; skip: boolean };

// The stage of a Quiz: open before an answer, picked after a pick, skipped after "Show the rest anyway". The first answer counts.
export const quizStage = (pick: number | null, skipped: boolean): QuizStage => (pick !== null ? "picked" : skipped ? "skipped" : "open");

// What each stage shows: before an answer the question, its options and "Show the rest anyway"; after either answer the reasons
// of every option, the held-back Body lines and the three chips.
export const quizView = (stage: QuizStage): QuizView =>
  stage === "open" ? { reasons: false, rest: false, chips: false, skip: true } : { reasons: true, rest: true, chips: true, skip: false };

// Whether a turn shows an open Quiz: never beside a decision, so not when its Body has an `ask` line or the session needs him.
export const quizAllowed = (body: SummaryLine[], needsYou: boolean): boolean => !needsYou && !body.some((line) => line.kind === "ask");

// The Quiz as the writer is shown it for "explain more": its question, and each option with its reason.
export const quizItem = (quiz: Quiz) =>
  `A quiz. ${quiz.text}\n${quiz.options.map((o, i) => `${i === quiz.key ? "Right" : "Wrong"}: ${o.text}. ${o.why}`).join("\n")}`;

// The trial of the confidence tap and the "why?" line runs for two weeks from the day a setting names (#129).
const TRIAL_MS = 14 * 24 * 60 * 60 * 1000;

// Whether the trial runs at `now` (Unix ms), given `start`, the setting's day as "YYYY-MM-DD"; null: no trial.
export function trialOn(start: string | null, now: number): boolean {
  const from = start === null ? NaN : Date.parse(start);
  return !Number.isNaN(from) && now >= from && now < from + TRIAL_MS;
}

type Chip = "understood" | "explain_more" | "key_wrong";
const chipHovers: Record<Chip, string> = {
  understood: HOVERS.understood,
  explain_more: HOVERS.explainMore,
  key_wrong: HOVERS.keyWrong,
};
type Kept = {
  pick: number | null;
  skipped: boolean;
  chips: Set<Chip>;
  more: string | null;
  asking: boolean;
  // The trial: how sure he was, his line on why, and whether that step is over (sent, or skipped with Enter).
  sure: boolean | null;
  // Home's "not sure" toggle (#242), off until Daniel turns it on before his pick.
  unsure: boolean;
  why: string | null;
  whyDone: boolean;
  // The "why?" line as typed so far, and whether it has the focus, so that a redraw of the rail keeps both.
  draft: string;
  typing: boolean;
};
const kept = new Map<string, Kept>();
const keptOf = (id: string): Kept => {
  if (!kept.has(id)) kept.set(id, { pick: null, skipped: false, chips: new Set(), more: null, asking: false, sure: null, unsure: false, why: null, whyDone: false, draft: "", typing: false });
  return kept.get(id)!;
};

// The taps the store holds for turn `id`, at its first sight: they fill what the window does not know yet.
export function seedQuiz(id: string, taps: QuizTap[] | undefined) {
  const one = keptOf(id);
  for (const tap of taps ?? []) {
    if (tap.kind === "pick") {
      if (one.pick === null && !one.skipped) one.pick = tap.option;
      // A pick stored before a restart was revealed or is not waited on again.
      one.whyDone = true;
    } else if (tap.kind === "confidence") one.sure ??= tap.sure;
    else if (tap.kind === "why") one.why ??= tap.text;
    else if (tap.kind === "skip") {
      if (one.pick === null) one.skipped = true;
    } else if (tap.kind !== "shown" && tap.kind !== "hunk" && tap.kind !== "not_now") one.chips.add(tap.kind);
  }
}

// Whether turn `id`'s Quiz was answered, by a pick or a skip.
export const quizAnswered = (id: string) => quizStage(keptOf(id).pick, keptOf(id).skipped) !== "open";

// The trial's confidence tap, "Sure" and "Not sure", before the pick; `redraw` draws what shows it again.
export function confidenceRow(id: string, record: (tap: Tap) => void, redraw: () => void): HTMLElement {
  const one = keptOf(id);
  const row = document.createElement("p");
  row.className = "quiz-sure";
  const sure = (value: boolean, label: string) => {
    const button = pressable(prose("button", label) as HTMLButtonElement, one.sure === value, () => {
      if (one.sure !== null) return;
      one.sure = value;
      record({ kind: "confidence", sure: value });
      redraw();
    });
    button.disabled = one.sure !== null;
    button.title = one.sure !== null ? "Already answered; your confidence is kept" : `Say you are ${label.toLowerCase()} before you pick`;
    return button;
  };
  row.append(sure(true, "Sure"), sure(false, "Not sure"));
  return row;
}

// The trial's "In a few words, why?" line, after the pick: Enter sends it, or skips it when empty. Its draft and focus outlast the
// rail's redraws.
export function whyInput(id: string, record: (tap: Tap) => void, redraw: () => void): HTMLInputElement {
  const one = keptOf(id);
  const input = document.createElement("input");
  input.type = "text";
  input.className = "quiz-why";
  input.placeholder = "In a few words, why?";
  input.value = one.draft;
  input.addEventListener("input", () => void (one.draft = input.value));
  input.addEventListener("focus", () => void (one.typing = true));
  // Taking the input out of the page ends its focus without Daniel leaving it.
  input.addEventListener("blur", () => void (one.typing = one.typing && input.isConnected));
  input.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    const line = (event.target as HTMLInputElement).value.trim();
    one.whyDone = true;
    one.typing = false;
    if (line) {
      one.why = line;
      record({ kind: "why", text: line });
    }
    redraw();
  });
  if (one.typing) queueMicrotask(() => input.focus?.());
  return input;
}

// Whether turn or card `id` was picked here and still waits for its "why?" line, which holds the reveal back.
export const whyWaiting = (id: string) => keptOf(id).pick !== null && !keptOf(id).whyDone;

// Notes a pick of card `id` made here, so that its reveal waits for the "why?" line while the trial runs.
export const pickedHere = (id: string, option: number) => void (keptOf(id).pick = option);

// What the trial's steps of `id` show, to tell a card's drawing that it changed.
export const trialStage = (id: string) => `${keptOf(id).sure}/${whyWaiting(id)}`;

export type QuizDeps = {
  // Sends the tap to the card store.
  record: (tap: Tap) => void;
  // The writer's longer explanation of the Quiz.
  explain: () => Promise<string>;
  // A Body line, drawn as the rail draws it.
  line: (line: SummaryLine) => HTMLElement;
  // Whether the trial's confidence tap and "why?" line show on this Quiz (#129): always on a retest, on a Quiz of "now" only
  // while the trial runs.
  trial?: boolean;
  // Home's card (#242), in place of the trial: no "Sure / Not sure" row, no "why?" line and no "Show the rest anyway". A pick writes the
  // confidence tap first, `sure` unless the "not sure" toggle was on, which sits in the card with `actions`; `date` is shown on the reveal.
  home?: { actions: (answered: boolean) => HTMLElement[]; date: string | null };
};

// The Quiz of turn `id` with its held-back Body lines `held`, in one region that draws itself again on a tap.
export function quizRegion(id: string, quiz: Quiz, held: SummaryLine[], deps: QuizDeps): HTMLElement {
  const region = document.createElement("div");
  region.className = "quiz-region";
  const tap = (tap: Tap) => {
    deps.record(tap);
    draw();
  };
  const chip = (kind: Chip, label: string, run: () => void = () => {}) => {
    const one = keptOf(id);
    const button = pressable(prose("button", label) as HTMLButtonElement, one.chips.has(kind), () => {
      if (one.chips.has(kind)) return;
      one.chips.add(kind);
      run();
      tap({ kind });
    });
    button.disabled = one.chips.has(kind);
    button.title = one.chips.has(kind) ? "Already tapped; a tap is kept once" : chipHovers[kind];
    return button;
  };
  const draw = () => {
    const one = keptOf(id);
    const stage = quizStage(one.pick, one.skipped);
    // In the trial the reveal after a pick waits for the "why?" line, which Enter skips.
    const waiting = deps.trial === true && stage === "picked" && !one.whyDone;
    const view = waiting ? { reasons: false, rest: false, chips: false, skip: false } : quizView(stage);
    const options = document.createElement("ol");
    options.className = "options";
    options.append(
      ...quiz.options.map((option, i) => {
        const item = document.createElement("li");
        const button = pressable(prose("button", option.text) as HTMLButtonElement, one.pick === i, () => {
          if (deps.home && one.sure === null) {
            one.sure = !one.unsure;
            deps.record({ kind: "confidence", sure: one.sure, surface: "home" });
          }
          one.pick = i;
          tap({ kind: "pick", option: i });
        });
        button.disabled = stage !== "open";
        button.title = stage === "open" ? HOVERS.pickAnswer : HOVERS.answered;
        item.append(button);
        if (view.reasons) {
          const why = prose("p", option.why);
          if (i === quiz.key) why.prepend(Object.assign(text("span", "The right answer. "), { dir: "ltr" }));
          item.append(why);
          // His own line shows beside the writer's reason; it is not graded.
          if (one.pick === i && one.why) item.append(prose("p", `Your reason: ${one.why}`));
        }
        return item;
      }),
    );
    const question = prose("p", quiz.text);
    question.className = "quiz";
    const box = document.createElement("div");
    box.className = "card quiz-box";
    box.append(question);
    if (deps.trial && stage === "open") box.append(confidenceRow(id, deps.record, draw));
    box.append(options);
    if (waiting) box.append(whyInput(id, deps.record, draw));
    if (view.skip && !deps.home) {
      const skip = pressable(prose("button", "Show the rest anyway"), false, () => {
        one.skipped = true;
        tap({ kind: "skip" });
      });
      skip.classList.add("quiz-skip");
      skip.title = "Skip the question and show the rest of the turn";
      box.append(skip);
    }
    // The reveal's date; the link to the code and the hunk under it come here (#243).
    if (deps.home?.date && view.reasons) box.append(Object.assign(prose("p", `The turn of ${deps.home.date}: the code may have changed since.`), { className: "quiz-date" }));
    if (view.chips) {
      const row = document.createElement("p");
      row.className = "quiz-chips";
      row.append(
        chip("understood", "I get why"),
        chip("explain_more", "Not yet — explain more", () => {
          one.asking = true;
          deps
            .explain()
            .then((more) => void (one.more = more))
            .catch(failure("explain"))
            .finally(() => {
              one.asking = false;
              if (one.more === null) one.chips.delete("explain_more");
              draw();
            });
        }),
        chip("key_wrong", "Key wrong"),
      );
      box.append(row);
      if (one.asking || one.more !== null) box.append(Object.assign(prose("p", one.more ?? "Writing…"), { className: "explanation" }));
    }
    if (deps.home) {
      const actions = deps.home.actions(stage !== "open");
      if (stage === "open") {
        const unsure = pressable(prose("button", "Not sure"), one.unsure, () => {
          one.unsure = !one.unsure;
          draw();
        });
        unsure.classList.add("quiz-skip");
        unsure.title = "Mark the pick as a guess";
        actions.unshift(unsure);
      }
      const row = Object.assign(document.createElement("p"), { className: "slot-actions" });
      row.append(...actions);
      if (actions.length) box.append(row);
    }
    const rest = view.rest && held.length ? [Object.assign(document.createElement("ul"), { className: "body-lines" })] : [];
    for (const list of rest) list.append(...held.map(deps.line));
    region.replaceChildren(box, ...rest);
  };
  draw();
  return region;
}
