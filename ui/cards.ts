// The cards, in the story rail: the session's brief, then its cards, the last 30 minutes' and the
// older ones. The writer writes them in English (#487); cards stored in Hebrew keep their direction.

import { askElement } from "./ask.js";
import { code, element, prose, invoke, pressable, text } from "./dom.js";
import { refreshCards } from "./selection.js";
import { studio } from "./state.js";
import { showDecisions } from "./decisions.js";
import type { CardRequest, CheckResult, Guess, Level, Quiz, Rating, Reason, SessionCards, StoredCard } from "./types.js";

// Records Daniel's tap, his flag on the key or his rating, and draws the cards again at once.
async function record(command: "guess" | "key_wrong" | "rate", args: Record<string, unknown>) {
  await invoke(command, args);
  await refreshCards();
}

// The verdict on a check's card (#476): the check's result settles the guess, so until it ends the
// card says only that it runs. Null on a card that waits for the tap.
export function checkVerdict(result: CheckResult | null, guess: Guess | null): string | null {
  if (result === null) return guess ? "The check still runs; its result settles your guess." : null;
  if (result === "not_counted") return "The check ended, but its exit code does not say whether it passed, so no guess counts.";
  const said = result === "passed" ? "It passed." : "It failed.";
  if (!guess) return said;
  return `${said} ${guess.right ? "You were right." : "You were wrong."}`;
}

// How a card's quiz stands: whether it is past its quiz, so its options are closed; whether the
// reasons under its options show, and with them the right one named; its verdict; and whether its key
// can be flagged wrong. Any other card is past its quiz at the tap, which the writer's key grades. On
// a check's card the check's result is the key (#476): the quiz closes when the check ends, the
// reasons and the verdict wait for it, and its key cannot be wrong.
export function quizState({ request, guess, check_result }: StoredCard) {
  if (request.check === null) {
    const verdict = guess && (guess.right ? "You were right." : "You were wrong.");
    return { answered: guess !== null, reasons: guess !== null, keyNamed: true, verdict, flaggable: true };
  }
  const ended = check_result !== null;
  return {
    answered: guess !== null || ended,
    reasons: ended,
    keyNamed: check_result !== "not_counted",
    verdict: checkVerdict(check_result, guess),
    flaggable: false,
  };
}

// A card's quiz and its options, one tap each. Once Daniel taps, it shows whether he was right and,
// under each option, why it is right or wrong, with the right one named; a tapped option stays
// marked, with no colour, as colour is only for "needs you". On a check's card the check's result
// is the key, so the reasons and the verdict wait for it.
function quizElements({ request: { id }, guess }: StoredCard, quiz: Quiz, state: ReturnType<typeof quizState>): HTMLElement[] {
  const options = document.createElement("ol");
  options.className = "options";
  options.append(
    ...quiz.options.map((option, i) => {
      const item = document.createElement("li");
      const button = pressable(prose("button", option.text) as HTMLButtonElement, guess?.option === i, () =>
        record("guess", { id, option: i }),
      );
      button.disabled = state.answered;
      item.append(button);
      if (state.reasons) {
        const why = prose("p", option.why);
        // A left-to-right label, which `dir="auto"` skips, so a Hebrew reason keeps its direction.
        if (state.keyNamed && i === quiz.key) why.prepend(Object.assign(text("span", "The right answer. "), { dir: "ltr" }));
        item.append(why);
      }
      return item;
    }),
  );
  const title = prose("p", quiz.text);
  title.className = "quiz";
  if (!state.verdict) return [title, options];
  const verdict = prose("p", state.verdict);
  verdict.className = "verdict";
  return [title, verdict, options];
}

// A card: its quiz first, then, once Daniel taps an option, the
// explanation, the file paths, those other subagents changed, the code left to right, the architecture section when the change is
// structural, why the change matters, what was not checked, his flag on the key, "explain more" and
// his rating. A card with no quiz shows the explanation at once. The shell keeps each tap in the card
// store, so after a restart a guessed card is drawn guessed and rated.
function cardElement(stored: StoredCard): HTMLElement {
  const { request, card, key_wrong, more, rating } = stored;
  const node = document.createElement("article");
  node.className = "card";
  const quiz = card.quiz;
  const state = quizState(stored);
  if (quiz) node.append(...quizElements(stored, quiz, state));
  // Every card offers "ask about this" (#482), a quiz card before its guess too.
  // shortcut: the ask sends the session's current turn's intentions, not the card's own turn's; the
  // core keeps only the current turn's responses. Keep each turn's intentions by its prompt if asks
  // on older cards mislead.
  const item = [card.summary, card.explanation, card.code].filter(Boolean).join("\n\n");
  const ask = askElement(request.session_id, item);
  if (quiz && !state.answered) {
    node.append(ask);
    return node;
  }
  const explanation = prose("p", card.explanation);
  explanation.className = "explanation";
  const paths = document.createElement("p");
  paths.className = "paths";
  paths.append(...card.paths.map(code));
  const source = document.createElement("pre");
  source.dir = "ltr";
  source.textContent = card.code;
  node.append(explanation, paths, ...changedBy(request), source);
  if (card.architecture) node.append(prose("h2", "Architecture"), prose("p", card.architecture));
  node.append(
    prose("h2", "Why it matters"),
    prose("p", card.why_it_matters),
    prose("h2", "Not checked"),
    prose("p", card.not_checked),
  );
  const flaggable = quiz && state.flaggable;
  if (flaggable) node.append(keyWrongButton(request.id, key_wrong));
  node.append(moreElement(request.id, more), ratingElement(request.id, rating, flaggable ? key_wrong : null), ask);
  return node;
}

// Who changed what: the files another subagent, such as a helper, edited in the card's worktree, one
// line per subagent, by the name the core gave it.
function changedBy({ session_id, changed_by }: CardRequest): HTMLElement[] {
  const agents = studio.listed.find((session) => session.id === session_id)?.agents ?? [];
  return Object.entries(changed_by).map(([id, files]) => {
    const name = agents.find((agent) => agent.id === id)?.name ?? id;
    const line = document.createElement("p");
    line.className = "paths";
    line.append(text("span", `Changed by ${name}:`), ...files.map(code));
    return line;
  });
}

// "The key is wrong": one record, flagged from the quiz or from the rating, and never undone.
function keyWrongButton(id: string, keyWrong: boolean): HTMLElement {
  const label = keyWrong ? "You flagged the key as wrong" : "The key is wrong";
  const flag = pressable(prose("button", label) as HTMLButtonElement, keyWrong, () => record("key_wrong", { id }));
  flag.classList.add("key-wrong");
  flag.disabled = keyWrong;
  return flag;
}

// The cards whose longer explanation Daniel opened, and those whose writer call is running; both
// outlast a card's drawing again.
const moreOpen = new Set<string>();
const asking = new Set<string>();

// "Explain more": the writer's longer explanation, under the card. The first tap asks the writer;
// later taps open and close the text the store keeps.
function moreElement(id: string, more: string | null): HTMLElement {
  const node = document.createElement("details");
  node.className = "more";
  node.open = moreOpen.has(id);
  const text = prose("p", more ?? "Writing…");
  text.className = "explanation";
  node.append(prose("summary", "Explain more"), text);
  node.addEventListener("toggle", () => {
    if (node.open) moreOpen.add(id);
    else moreOpen.delete(id);
    if (node.open && more === null && !asking.has(id)) explainMore(id);
  });
  return node;
}

// Asks the writer, for seconds, then draws the card again: with its text, or closed when the writer
// gave none, so the next tap asks again.
async function explainMore(id: string) {
  asking.add(id);
  const more = await invoke<string | null>("explain_more", { id });
  asking.delete(id);
  if (more === null) moreOpen.delete(id);
  drawn.delete(id);
  await refreshCards();
}

const levels: [Level, string][] = [
  ["good", "Good"],
  ["fair", "Fair"],
  ["poor", "Poor"],
];
const reasons: [Reason, string][] = [
  ["unclear", "Unclear"],
  ["wrong", "Wrong"],
  ["too_shallow", "Too shallow"],
  ["too_easy", "Too easy"],
];

// Daniel's rating of the explanation: a level, then, once he picked one, the reasons, each tap
// toggling one. `keyWrong` is the quiz's flag, offered as a reason on a card with a quiz.
function ratingElement(id: string, rating: Rating | null, keyWrong: boolean | null): HTMLElement {
  const choice = (label: string, pressed: boolean, picked: Rating) =>
    pressable(prose("button", label), pressed, () => record("rate", { id, rating: picked }));
  const node = document.createElement("div");
  node.className = "rating";
  const row = document.createElement("p");
  row.append(
    prose("span", "Rate the explanation:"),
    ...levels.map(([level, label]) =>
      choice(label, rating?.level === level, { level, reasons: rating?.reasons ?? [] }),
    ),
  );
  node.append(row);
  if (!rating) return node;
  const why = document.createElement("p");
  why.append(
    ...reasons.map(([reason, label]) => {
      const has = rating.reasons.includes(reason);
      const toggled = has ? rating.reasons.filter((r) => r !== reason) : [...rating.reasons, reason];
      return choice(label, has, { level: rating.level, reasons: toggled });
    }),
  );
  if (keyWrong !== null) why.append(keyWrongButton(id, keyWrong));
  node.append(why);
  return node;
}

// Unguessed cards whose quiz the pane drew while the window was hidden. A quiz counts as shown when
// it is drawn in a visible window, or when the window next becomes visible; the store keeps the first
// time, which the weekly rates measure guesses from.
const unshown = new Set<string>();

export function markShown() {
  if (document.visibilityState !== "visible") return;
  if (unshown.size) invoke("shown", { ids: [...unshown] });
  unshown.clear();
}


// Cards already drawn, keyed by their id, with the record each was drawn from. A card is drawn again
// only when its record changed, such as after a tap, and is moved whole to the older group.
const drawn = new Map<string, { record: string; node: HTMLElement }>();

// The session's brief at the top of its view, one paragraph per line.
function showBrief(brief: string | null) {
  element("brief").replaceChildren(...(brief ?? "").split("\n").map((line) => prose("p", line)));
  element("brief").hidden = brief === null;
}

export function showCards({ brief, recent, older, decisions }: SessionCards) {
  showBrief(brief);
  showDecisions(decisions);
  const ids = new Set([...recent, ...older].map(({ request }) => request.id));
  for (const id of drawn.keys()) {
    if (ids.has(id)) continue;
    drawn.delete(id);
    unshown.delete(id);
  }
  // The newest card first.
  const nodes = (cards: StoredCard[]) =>
    [...cards].reverse().map((stored) => {
      const id = stored.request.id;
      const record = JSON.stringify(stored);
      if (drawn.get(id)?.record !== record) {
        if (!drawn.has(id) && stored.guess === null) unshown.add(id);
        drawn.set(id, { record, node: cardElement(stored) });
      }
      return drawn.get(id)!.node;
    });
  element("recent").replaceChildren(...nodes(recent));
  element("older").replaceChildren(...nodes(older));
  element("recent-title").hidden = recent.length === 0;
  element("older-title").hidden = older.length === 0;
  markShown();
}
