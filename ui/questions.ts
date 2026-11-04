// An agent's question, laid out as its choices, the one Daniel chose marked. Answering stays in the
// terminal.

import { text } from "./dom.js";
import type { Question } from "./types.js";

// Whether a question is open, answered in the terminal, or not answered: its call failed, or its turn
// or its agent ended first. An open one needs Daniel, so only it takes the colour.
const answerLabels = { open: "Needs you", not_answered: "Not answered", answered: "Answered" };

export function questionElement({ questions, answer }: Question): HTMLElement {
  const block = document.createElement("article");
  block.className = "question";
  const status = typeof answer === "string" ? answer : "answered";
  const state = text("p", answerLabels[status], "state");
  if (status === "open") state.dataset.state = "needs_you";
  block.append(state);
  for (const asked of questions) {
    const chosen = typeof answer === "string" ? null : (answer.answered[asked.question] ?? null);
    const choices = document.createElement("ul");
    choices.className = "choices";
    choices.append(
      ...asked.options.map((choice) => {
        const item = document.createElement("li");
        item.append(text("span", choice.label, "label"), text("span", choice.description ?? "", "description"));
        // A label may hold a comma, so only a multi-select answer is split (#434, decision 41).
        const labels = asked.multiSelect ? chosen?.split(", ") : [chosen];
        item.classList.toggle("chosen", labels?.includes(choice.label) ?? false);
        return item;
      }),
    );
    block.append(text("p", asked.header ?? "", "header"), text("p", asked.question, "asked"), choices);
    if (chosen !== null) block.append(text("p", `Your answer: ${chosen}`));
  }
  return block;
}
