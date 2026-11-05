// An agent's question, laid out as its choices, the one Daniel chose marked. While the answer hook
// waits for it (#461, decision 16), its choices are buttons, chosen by click or Enter, and "Answer in
// the terminal" hands it back to the TUI; otherwise answering stays in the terminal. A plan or a
// permission the hook waits on gets the same card with allow and deny (#478).

import { askFor, sendAnswer, type Ask } from "./answer.js";
import { code, pressable, prose, text } from "./dom.js";
import { askKind, type Question } from "./types.js";

// Whether a question is open, answered in the terminal, or not answered: its call failed, or its turn
// or its agent ended first. An open one needs Daniel, so only it takes the colour.
const answerLabels = { open: "Needs you", not_answered: "Not answered", answered: "Answered" };

// The labels picked on the card so far, by question text, for the open request `nonce`. They outlast
// the view's redraws; a new request, once this one is answered, starts afresh.
let picks = { nonce: "", picked: new Map<string, string[]>() };
const pickedFor = (nonce: string) => {
  if (picks.nonce !== nonce) picks = { nonce, picked: new Map() };
  return picks.picked;
};

export function questionElement(question: Question): HTMLElement {
  const { questions, answer } = question;
  const ask = answer === "open" ? askFor(question) : null;
  const picked = ask === null ? new Map<string, string[]>() : pickedFor(ask.nonce);
  const block = document.createElement("article");
  block.className = "question";
  const status = typeof answer === "string" ? answer : "answered";
  const state = text("p", answerLabels[status], "state");
  if (status === "open") state.dataset.state = "needs_you";
  block.append(state);
  const send = () => {
    if (ask === null || picked.size < questions.length) return;
    const answers = Object.fromEntries(
      questions.map((asked) => [asked.question, asked.multiSelect ? picked.get(asked.question)! : picked.get(asked.question)![0]]),
    );
    sendAnswer(ask, { answers });
  };
  // One single-choice question is sent by its click; anything more by "Send".
  const sendButton = ask !== null && (questions.length > 1 || questions.some((asked) => asked.multiSelect));
  for (const asked of questions) {
    const chosen = typeof answer === "string" ? null : (answer.answered[asked.question] ?? null);
    const choices = document.createElement("ul");
    choices.className = "choices";
    choices.append(
      ...asked.options.map((choice) => {
        const item = document.createElement("li");
        const content = [text("span", choice.label, "label"), text("span", choice.description ?? "", "description")];
        const pickedNow = picked.get(asked.question)?.includes(choice.label) ?? false;
        if (ask !== null) {
          // A button, so Tab and Enter choose as a click does.
          const button = pressable(document.createElement("button"), pickedNow, () => {
            const labels = picked.get(asked.question) ?? [];
            const on = !labels.includes(choice.label);
            const next = asked.multiSelect ? labels.filter((l) => l !== choice.label) : [];
            if (on) next.push(choice.label);
            if (next.length) picked.set(asked.question, next);
            else picked.delete(asked.question);
            for (const other of choices.querySelectorAll("li")) {
              const label = other.querySelector(".label")?.textContent ?? "";
              other.classList.toggle("chosen", next.includes(label));
              other.querySelector("button")?.setAttribute("aria-pressed", String(next.includes(label)));
            }
            const ready = picked.size === questions.length;
            block.querySelector<HTMLButtonElement>(".send")?.toggleAttribute("disabled", !ready);
            if (!sendButton) send();
          });
          button.append(...content);
          item.append(button);
        } else item.append(...content);
        // A label may hold a comma, so only a multi-select answer is split (#434, decision 41).
        const labels = asked.multiSelect ? chosen?.split(", ") : [chosen];
        item.classList.toggle("chosen", pickedNow || (labels?.includes(choice.label) ?? false));
        return item;
      }),
    );
    block.append(text("p", asked.header ?? "", "header"), text("p", asked.question, "asked"), choices);
    if (chosen !== null) block.append(text("p", `Your answer: ${chosen}`));
  }
  if (ask !== null) block.append(answerButtons(ask, sendButton ? [["Send", send, "send"]] : []));
  block.querySelector<HTMLButtonElement>(".send")?.toggleAttribute("disabled", picked.size < questions.length);
  return block;
}

// A plan or a permission the answer hook waits on (#478): what it asks, then allow or deny.
export function decisionElement(ask: Ask): HTMLElement {
  const block = document.createElement("article");
  block.className = "question decision";
  const state = text("p", answerLabels.open, "state");
  state.dataset.state = "needs_you";
  block.append(state);
  const decide = (decision: "allow" | "deny") => () => sendAnswer(ask, { decision });
  const input = ask.tool_input;
  if (askKind(ask.tool_name) === "plan") {
    block.append(text("p", "Plan", "header"), text("p", "Approve this plan?", "asked"));
    const plan = prose("div", String(input.plan ?? ""));
    plan.className = "plan";
    block.append(plan);
    block.append(answerButtons(ask, [["Approve", decide("allow")], ["Reject", decide("deny")]]));
  } else {
    // What the call does: a command, a path or a link; otherwise its whole input.
    const detail = input.command ?? input.file_path ?? input.url ?? JSON.stringify(input);
    block.append(text("p", "Permission", "header"), text("p", `Allow ${ask.tool_name}?`, "asked"));
    const shown = document.createElement("p");
    shown.append(code(String(detail)));
    if (typeof input.description === "string") shown.append(text("span", ` ${input.description}`, "description"));
    block.append(shown);
    block.append(answerButtons(ask, [["Allow", decide("allow")], ["Deny", decide("deny")]]));
  }
  return block;
}

// The card's own buttons, each a label, its click and a class, then "Answer in the terminal". "Send"
// starts disabled, until every question has a choice.
function answerButtons(ask: Ask, buttons: [string, () => void, string?][]): HTMLElement {
  const row = document.createElement("p");
  row.className = "answer-buttons";
  const button = (label: string, click: () => void, className = "") => {
    const node = text("button", label, className) as HTMLButtonElement;
    node.type = "button";
    node.addEventListener("click", click);
    return node;
  };
  row.append(...buttons.map(([label, click, className]) => button(label, click, className)));
  row.querySelector<HTMLButtonElement>(".send")?.toggleAttribute("disabled", true);
  row.append(button("Answer in the terminal", () => sendAnswer(ask, null)));
  return row;
}
