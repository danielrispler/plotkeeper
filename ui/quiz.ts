// A turn's own Quiz in "now" (danielrispler/plotkeeper#118, #120): open under the evidence line, one click to answer. A pick, or
// "Show the rest anyway", reveals the reasons of every option and the Body lines held back until then. Revealing changes only
// the Quiz's region: its children are drawn again and nothing else in the rail is, so the scroll position stays. What Daniel
// did is kept by turn id, which outlasts a redraw of the rail, and sent to the card store, which keeps it with its time.

import { pressable, prose, text } from "./dom.js";
import type { Quiz, QuizTap, SummaryLine, Tap } from "./types.js";

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
type Kept = {
  pick: number | null;
  skipped: boolean;
  chips: Set<Chip>;
  more: string | null;
  asking: boolean;
  // The trial: how sure he was, his line on why, and whether that step is over (sent, or skipped with Enter).
  sure: boolean | null;
  why: string | null;
  whyDone: boolean;
};
const kept = new Map<string, Kept>();
const keptOf = (id: string): Kept => {
  if (!kept.has(id)) kept.set(id, { pick: null, skipped: false, chips: new Set(), more: null, asking: false, sure: null, why: null, whyDone: false });
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
    } else one.chips.add(tap.kind);
  }
}

// Whether turn `id`'s Quiz was answered, by a pick or a skip.
export const quizAnswered = (id: string) => quizStage(keptOf(id).pick, keptOf(id).skipped) !== "open";

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
          one.pick = i;
          tap({ kind: "pick", option: i });
        });
        button.disabled = stage !== "open";
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
    if (deps.trial && stage === "open") {
      const row = document.createElement("p");
      row.className = "quiz-sure";
      const sure = (value: boolean, label: string) => {
        const button = pressable(prose("button", label) as HTMLButtonElement, one.sure === value, () => {
          if (one.sure !== null) return;
          one.sure = value;
          tap({ kind: "confidence", sure: value });
        });
        button.disabled = one.sure !== null;
        return button;
      };
      row.append(sure(true, "Sure"), sure(false, "Not sure"));
      box.append(row);
    }
    box.append(options);
    if (waiting) {
      const input = document.createElement("input");
      input.type = "text";
      input.className = "quiz-why";
      input.placeholder = "In a few words, why?";
      input.addEventListener("keydown", (event) => {
        if (event.key !== "Enter") return;
        const line = (event.target as HTMLInputElement).value.trim();
        one.whyDone = true;
        if (line) {
          one.why = line;
          deps.record({ kind: "why", text: line });
        }
        draw();
      });
      box.append(input);
    }
    if (view.skip) {
      const skip = pressable(prose("button", "Show the rest anyway"), false, () => {
        one.skipped = true;
        tap({ kind: "skip" });
      });
      skip.classList.add("quiz-skip");
      box.append(skip);
    }
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
            .catch((e) => console.warn(e))
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
    const rest = view.rest && held.length ? [Object.assign(document.createElement("ul"), { className: "body-lines" })] : [];
    for (const list of rest) list.append(...held.map(deps.line));
    region.replaceChildren(box, ...rest);
  };
  draw();
  return region;
}
