// The side pane: the state and "now" line of the session Daniel prompted last, and its cards. The core
// decides what it shows; this file only draws what the shell sends, on load and on each "now" or
// "cards" event.

type Step = { tool: string; detail: string | null };
type SessionState = "working" | "needs_you" | "your_turn";
type GuessPrompt = { session_id: string; tool_use_id: string; step: Step; guess: boolean | null };
type Session = {
  id: string;
  cwd: string;
  last_step: Step | null;
  state: SessionState;
  guess: GuessPrompt | null;
};
type Check = { guess: boolean | null; exit_passed: boolean | null };
type CardRequest = { session_id: string; tool_use_id: string; step: Step; change: string; check: Check | null };
type Card = {
  question: string;
  answer: string;
  paths: string[];
  code: string;
  why_it_matters: string;
  not_checked: string;
};
// The store also records when the question showed and how the guess settled; the pane never shows
// either, nor any rate.
type StoredCard = { request: CardRequest; card: Card; revealed_at: number | null; passed: boolean | null };

const stateLabels: Record<SessionState, string> = {
  working: "Working",
  needs_you: "Needs you",
  your_turn: "Your turn",
};

declare global {
  interface Window {
    __TAURI__: {
      core: { invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> };
      event: { listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<unknown> };
    };
  }
}

const element = (id: string) => document.getElementById(id)!;

// "Will it pass?" on the check that is the wait's card step, open until its result arrives. After a
// tap it shows which answer Daniel gave, never whether he was right.
let openGuess: GuessPrompt | null = null;
const taps = [
  ["guess-yes", true],
  ["guess-no", false],
] as const;

function showGuess(prompt: GuessPrompt | null) {
  openGuess = prompt;
  element("guess").hidden = prompt === null;
  if (prompt === null) return;
  element("guess-command").textContent = prompt.step.detail ?? "";
  for (const [id, willPass] of taps) {
    const button = element(id) as HTMLButtonElement;
    button.disabled = prompt.guess !== null;
    button.setAttribute("aria-pressed", String(prompt.guess === willPass));
  }
}

for (const [id, willPass] of taps) {
  element(id).addEventListener("click", () => {
    if (openGuess) window.__TAURI__.core.invoke("guess", { toolUseId: openGuess.tool_use_id, willPass });
  });
}

function show(session: Session | null) {
  showGuess(session?.guess ?? null);
  element("empty").hidden = session !== null;
  element("session").hidden = session === null;
  element("state").hidden = session === null;
  element("step").hidden = session === null;
  if (session === null) return;
  const folder = session.cwd.split("/").pop() || session.cwd;
  element("session").textContent = `${folder} · ${session.id.slice(0, 8)}`;
  element("state").textContent = stateLabels[session.state];
  element("state").dataset.state = session.state;
  element("tool").textContent = session.last_step?.tool ?? "No step yet";
  element("detail").textContent = session.last_step?.detail ?? "";
}

// Code, left to right, in its own isolate.
function code(text: string): HTMLElement {
  const node = document.createElement("code");
  node.dir = "ltr";
  node.textContent = text;
  return node;
}

// Hebrew text whose code is in backticks. Each code span becomes a left-to-right isolate, so `a <= b`
// is not mirrored inside the right-to-left text.
function hebrew(tag: string, text: string): HTMLElement {
  const node = document.createElement(tag);
  node.append(...text.split("`").map((part, i) => (i % 2 ? code(part) : part)));
  return node;
}

// Whether a check passed, as settled from its exit code or the card writer's verdict.
function verdict(passed: boolean | null): string {
  if (passed === null) return "הפלט לא מראה אם הבדיקה עברה.";
  return passed ? "הבדיקה עברה." : "הבדיקה נכשלה.";
}

// A card in Hebrew, right to left: the question, then, on a tap, whether a check passed, the answer,
// the file paths, the code left to right, why the step matters and what was not checked. The shell
// keeps each reveal in the card store, so after a restart a revealed card is drawn open.
function cardElement({ request, card, revealed_at, passed }: StoredCard): HTMLElement {
  const details = document.createElement("details");
  details.className = "card";
  details.dir = "rtl";
  details.lang = "he";
  details.dataset.id = request.tool_use_id;
  details.open = revealed_at !== null;
  details.addEventListener("toggle", () => {
    if (details.open) window.__TAURI__.core.invoke("reveal", { toolUseId: request.tool_use_id });
  });
  const answer = hebrew("p", card.answer);
  answer.className = "answer";
  const paths = document.createElement("p");
  paths.className = "paths";
  paths.append(...card.paths.map(code));
  const source = document.createElement("pre");
  source.dir = "ltr";
  source.textContent = card.code;
  const result = hebrew("p", verdict(passed));
  result.className = "verdict";
  details.append(
    hebrew("summary", card.question),
    ...(request.check ? [result] : []),
    answer,
    paths,
    source,
    hebrew("h2", "למה זה חשוב"),
    hebrew("p", card.why_it_matters),
    hebrew("h2", "מה לא נבדק"),
    hebrew("p", card.not_checked),
  );
  return details;
}

// Closed cards whose question the pane drew while the window was hidden. A question counts as shown
// when it is drawn in a visible window, or when the window next becomes visible; the store keeps the
// first time, which the weekly rates measure reveals from.
const unshown = new Set<string>();

function markShown() {
  if (document.visibilityState !== "visible") return;
  for (const toolUseId of unshown) window.__TAURI__.core.invoke("shown", { toolUseId });
  unshown.clear();
}

document.addEventListener("visibilitychange", markShown);

// Each card is a <details>: its question shows, and a tap reveals the rest. Cards already drawn are
// kept, keyed by their step's tool_use_id, so one Daniel opened stays open on the next event.
function showCards(cards: StoredCard[]) {
  const list = element("cards");
  const ids = new Set(cards.map(({ request }) => request.tool_use_id));
  const drawn = new Set<string>();
  for (const node of [...list.children] as HTMLElement[]) {
    if (ids.has(node.dataset.id!)) drawn.add(node.dataset.id!);
    else {
      node.remove();
      unshown.delete(node.dataset.id!);
    }
  }
  for (const stored of cards) {
    if (drawn.has(stored.request.tool_use_id)) continue;
    list.prepend(cardElement(stored));
    if (stored.revealed_at === null) unshown.add(stored.request.tool_use_id);
  }
  markShown();
  element("cards-title").hidden = cards.length === 0;
}

window.__TAURI__.event.listen<Session | null>("now", (event) => show(event.payload));
window.__TAURI__.core.invoke<Session | null>("now").then(show);
window.__TAURI__.event.listen<StoredCard[]>("cards", (event) => showCards(event.payload));
window.__TAURI__.core.invoke<StoredCard[]>("cards").then(showCards);

export {};
