// The cards, in the story rail: the session's brief, and each card, which the rail places in the turn
// it was written in, or at the top when it is the newest (danielrispler/Falinks#574). The writer writes them in English
// (danielrispler/Falinks#487); cards stored in Hebrew keep their direction.

import { code, element, hover, prose, invoke, pressable, text } from "./dom.js";
import { refreshCards } from "./selection.js";
import { studio } from "./state.js";
import { showDecisions } from "./decisions.js";
import { fileRow } from "./evidence.js";
import { drawFeed, forgetMissingSummaries, openCitation } from "./story.js";
import { confidenceRow, pickedHere, trialStage, whyInput, whyWaiting } from "./quiz.js";
import type { CardRequest, CheckResult, Guess, Level, Quiz, Rating, Reason, SessionCards, StoredCard, Tap, TurnFile } from "./types.js";

// The turn a card is drawn in: its file chips open the file's hunk between the turn's trees, as its evidence line's rows do.
export type CardTurn = { session: string; index: number; files: TurnFile[]; ended: boolean };

// Records Daniel's tap, his flag on the key or his rating, and draws the cards again at once.
async function record(command: "guess" | "key_wrong" | "rate" | "quiz_tap", args: Record<string, unknown>) {
  await invoke(command, args);
  await refreshCards();
}

// The verdict on a check's card (danielrispler/Falinks#476): the check's result settles the guess, so until it ends the
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
// a check's card the check's result is the key (danielrispler/Falinks#476): the quiz closes when the check ends, the
// reasons and the verdict wait for it, and its key cannot be wrong.
export function quizState({ request, guess, check_result, taps }: StoredCard) {
  if (request.check === null) {
    // "Show the rest anyway" (danielrispler/plotkeeper#120) answers it without a pick, so it has no verdict.
    const open = guess === null && !taps?.some((tap) => tap.kind === "skip");
    const verdict = guess && (guess.right ? "You were right." : "You were wrong.");
    return { answered: !open, reasons: !open, keyNamed: true, verdict, flaggable: true };
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
// While the trial runs (#129), a card of "now" asks the confidence tap before the pick and the "why?" line after it, and its reveal
// waits for the line, as a turn's Quiz does (`quizRegion`).
const waitsForWhy = ({ request: { id, check }, guess }: StoredCard, trial: boolean) => trial && check === null && guess !== null && whyWaiting(id);

function quizElements(stored: StoredCard, quiz: Quiz, shown: ReturnType<typeof quizState>, trial: boolean): HTMLElement[] {
  const { request: { id, check }, guess } = stored;
  const waiting = waitsForWhy(stored, trial);
  const state = waiting ? { ...shown, reasons: false, verdict: null } : shown;
  const send = (tap: Tap) => void invoke("quiz_tap", { id, tap }).catch((e) => console.warn(e));
  const options = document.createElement("ol");
  options.className = "options";
  options.append(
    ...quiz.options.map((option, i) => {
      const item = document.createElement("li");
      // The tap shows at once; the cards drawn from the shell's reply then settle it (danielrispler/Falinks#570).
      const button = pressable(prose("button", option.text) as HTMLButtonElement, guess?.option === i, () => {
        button.setAttribute("aria-pressed", "true");
        // Answered here, the card stays open to show its reveal.
        cardOpen.add(id);
        pickedHere(id, i);
        record("guess", { id, option: i });
      });
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
  if (!state.answered && check === null) {
    const skip = pressable(prose("button", "Show the rest anyway"), false, () => {
      cardOpen.add(id);
      return record("quiz_tap", { id, tap: { kind: "skip" } });
    });
    skip.classList.add("quiz-skip");
    return [title, ...(trial && check === null ? [confidenceRow(id, send, drawFeed)] : []), options, skip];
  }
  if (waiting) return [title, options, whyInput(id, send, drawFeed)];
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
function cardElement(stored: StoredCard, turn: CardTurn | null, trial: boolean): HTMLElement {
  const { request, card, key_wrong, more, rating, taps } = stored;
  const node = document.createElement("article");
  node.className = "card";
  const quiz = card.quiz;
  const state = quizState(stored);
  if (quiz) node.append(...quizElements(stored, quiz, state, trial));
  // Its turn holds the one "ask about this" (danielrispler/Falinks#574).
  if (quiz && (!state.answered || waitsForWhy(stored, trial))) return node;
  const explanation = prose("p", card.explanation);
  explanation.className = "explanation";
  node.append(explanation, fileChips(card.paths, turn), ...changedBy(request));
  // The code is pulled from git by path and line range; a card with none shows no code.
  if (card.code) {
    const source = document.createElement("pre");
    source.dir = "ltr";
    source.textContent = card.code;
    node.append(source);
  }
  if (card.architecture) node.append(prose("h2", "Architecture"), prose("p", card.architecture));
  node.append(
    prose("h2", "Why it matters"),
    prose("p", card.why_it_matters),
    prose("h2", "Not checked"),
    prose("p", card.not_checked),
  );
  const flaggable = quiz && state.flaggable;
  if (flaggable) node.append(understoodButton(request.id, taps), keyWrongButton(request.id, key_wrong));
  node.append(moreElement(request.id, more), ratingElement(request.id, rating, flaggable ? key_wrong : null));
  return node;
}

// The files the card names. One that is among its turn's files is a chip that opens the same hunk as the file's row in the
// evidence line, in place; any other is plain.
function fileChips(paths: string[], turn: CardTurn | null): HTMLElement {
  const list = document.createElement("ul");
  list.className = "paths card-files";
  list.append(
    ...paths.map((path) => {
      const file = turn?.files.find((f) => f.path === path);
      if (!turn || !file) {
        const item = document.createElement("li");
        item.append(code(path));
        return item;
      }
      return fileRow(turn.session, turn.index, { kind: "file", label: path, file }, "card ", turn.ended);
    }),
  );
  return list;
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
  // The store sets the one flag and keeps the tap with its time (danielrispler/plotkeeper#120).
  const flag = pressable(prose("button", label) as HTMLButtonElement, keyWrong, () => record("quiz_tap", { id, tap: { kind: "key_wrong" } }));
  flag.classList.add("key-wrong");
  flag.disabled = keyWrong;
  return flag;
}

// "I get why": an optional chip under a revealed answer, kept once with its time.
function understoodButton(id: string, taps: StoredCard["taps"]): HTMLElement {
  const done = taps?.some((tap) => tap.kind === "understood") ?? false;
  const chip = pressable(prose("button", "I get why") as HTMLButtonElement, done, () => record("quiz_tap", { id, tap: { kind: "understood" } }));
  chip.disabled = done;
  return chip;
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
  invoke("quiz_tap", { id, tap: { kind: "explain_more" } }).catch((e) => console.warn(e));
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

// The Brief's slots in order (danielrispler/plotkeeper#122): Goal, Decisions, Open. A brief stored as prose shows as it is,
// with no title, until the session has a Goal.
export type BriefSlot = { title: string; lines: string[]; cites?: string[] };
export function briefSlots(cards: SessionCards): BriefSlot[] {
  const slots: BriefSlot[] = [];
  if (cards.goal) slots.push({ title: "Goal", lines: [cards.goal] });
  else if (cards.brief) slots.push({ title: "", lines: cards.brief.split("\n") });
  if (cards.answers.length) slots.push({ title: "Decisions", lines: cards.answers.map((a) => `${a.question} ${a.reply}`) });
  if (cards.open.length) slots.push({ title: "Open", lines: cards.open.map((item) => item.text) });
  if (cards.design.length) slots.push({ title: "Design so far", lines: cards.design.map((line) => line.text), cites: cards.design.map((line) => line.cite) });
  return slots;
}

// A design line's citation: a button that opens the turn or card it came from in the rail.
function citation(id: string): HTMLElement {
  const button = pressable(text("button", "↗", "cite") as HTMLButtonElement, false, () => openCitation(id));
  button.title = "Open the turn or card it came from";
  return button;
}

function slotTitle(title: string): HTMLElement {
  const node = prose("p", title);
  node.className = "slot-title";
  return node;
}

// The session's brief: each slot with its title and one paragraph per line.
export const briefNodes = (cards: SessionCards): HTMLElement[] =>
  briefSlots(cards).flatMap(({ title, lines, cites }) => [
    ...(title ? [slotTitle(title)] : []),
    ...lines.map((line, i) => {
      const node = prose("p", line);
      const cite = cites?.[i];
      if (cite) node.append(" ", citation(cite));
      return node;
    }),
  ]);

// The brief, folded under the feed.
function showBrief(cards: SessionCards) {
  const nodes = briefNodes(cards);
  element("brief").replaceChildren(...nodes);
  element("brief-fold").hidden = nodes.length === 0;
}

// Card `stored`'s node, drawn again only when its record changed.
export function cardNode(stored: StoredCard, turn: CardTurn | null = null, trial = false): HTMLElement {
  const id = stored.request.id;
  const record = JSON.stringify([stored, turn, trial ? trialStage(id) : null]);
  if (drawn.get(id)?.record !== record) {
    if (!drawn.has(id) && stored.guess === null) unshown.add(id);
    drawn.set(id, { record, node: cardElement(stored, turn, trial) });
  }
  return drawn.get(id)!.node;
}

export function showCards(cards: SessionCards) {
  studio.cards = cards;
  showBrief(cards);
  showDecisions(cards.decisions);
  const ids = new Set([...cards.recent, ...cards.older].map(({ request }) => request.id));
  for (const id of drawn.keys()) {
    if (ids.has(id)) continue;
    drawn.delete(id);
    unshown.delete(id);
  }
  // The store changed: a summary missing before may be in it now.
  forgetMissingSummaries();
  drawFeed();
  markShown();
}

// Whether the card's quiz waits for Daniel's answer.
export const quizOpen = (stored: StoredCard) => stored.card.quiz !== null && !quizState(stored).answered;

// The cards of a turn whose Quiz may not show beside a decision (`quizAllowed`), without their Quiz: the core drops it only when the
// card is written, so a Quiz written before the `ask` line or the Needs you state stays in the store.
export const cardsFor = (cards: StoredCard[], allowed: boolean): StoredCard[] =>
  allowed ? cards : cards.map((stored) => (stored.card.quiz === null ? stored : { ...stored, card: { ...stored.card, quiz: null } }));

// A card in its turn (danielrispler/plotkeeper#117): an answered card is one line, its quiz, which opens to the card.
export function cardLine(stored: StoredCard, turn: CardTurn | null = null, trial = false): HTMLElement {
  const quiz = stored.card.quiz;
  if (!quiz || !quizState(stored).answered) return cardNode(stored, turn, trial);
  const id = stored.request.id;
  const node = document.createElement("details");
  node.className = "fold answered-card";
  node.open = cardOpen.has(id);
  node.append(hover(prose("summary", quiz.text), quiz.text), cardNode(stored, turn, trial));
  node.addEventListener("toggle", () => {
    if (node.open) cardOpen.add(id);
    else cardOpen.delete(id);
  });
  return node;
}
const cardOpen = new Set<string>();

// Opens card `id`'s line, for a citation of it (#123).
export const openCard = (id: string) => void cardOpen.add(id);
