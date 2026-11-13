// The session's decision history (danielrispler/Falinks#479): its answered questions, plans and permissions, and its
// "cleared" markers (danielrispler/Falinks#474), newest first, in a collapsed section above the turns. The decisions come
// with the cards, from the store; the markers come with the selection.

import { code, element, prose, text } from "./dom.js";
import { askKind, type Decision } from "./types.js";

let decisions: Decision[] = [];
let cleared: number[] = [];

export function showDecisions(list: Decision[]) {
  decisions = list;
  draw();
}

export function showCleared(times: number[]) {
  cleared = times;
  draw();
}

// What Daniel answered, in words: a question's choices, or a plan's or a permission's verdict.
export function answerText(decision: Decision): string {
  const { tool, answer } = decision;
  if (typeof answer !== "string") return Object.values(answer).join(" · ");
  if (askKind(tool) === "plan") return answer === "allow" ? "Approved" : "Rejected";
  return answer === "allow" ? "Allowed" : "Denied";
}

// What kind of decision it was.
export function kindText(decision: Decision): string {
  const kind = askKind(decision.tool);
  if (kind === "question") return "Question";
  return kind === "plan" ? "Plan" : `Permission · ${decision.tool}`;
}

export const sourceText = (decision: Decision) => (decision.source === "card" ? "on the card" : "in the terminal");

const when = (at: number) =>
  new Date(at).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

function draw() {
  const entries: [number, HTMLElement][] = [
    ...decisions.map((d): [number, HTMLElement] => [d.at, entry(d)]),
    ...cleared.map((at): [number, HTMLElement] => [at, marker(at)]),
  ];
  entries.sort(([a], [b]) => b - a);
  element("decisions-list").replaceChildren(...entries.map(([, node]) => node));
  element("decisions-title").textContent = `Decisions (${decisions.length})`;
  element("decisions").hidden = entries.length === 0;
}

function marker(at: number): HTMLElement {
  const item = document.createElement("li");
  item.className = "cleared";
  item.append(text("span", "Cleared", "kind"), text("span", ` · ${when(at)}`, "when"));
  return item;
}

function entry(decision: Decision): HTMLElement {
  const item = document.createElement("li");
  item.append(text("p", `${kindText(decision)} · ${when(decision.at)} · ${sourceText(decision)}`, "kind"));
  const input = decision.input;
  const kind = askKind(decision.tool);
  if (kind === "question") {
    const answers = typeof decision.answer === "string" ? {} : decision.answer;
    for (const asked of input.questions ?? []) {
      const options = asked.options.map((o) => o.label).join(" / ");
      item.append(text("p", asked.question, "asked"), text("p", options, "options"));
      item.append(text("p", `Answer: ${answers[asked.question] ?? "none"}`, "answer"));
    }
    return item;
  }
  if (kind === "plan") {
    const plan = document.createElement("details");
    const body = prose("div", String(input.plan ?? ""));
    body.className = "plan";
    plan.append(text("summary", String(input.plan ?? "").split("\n")[0] ?? ""), body);
    item.append(plan);
  } else {
    // What the call does: a command, a path or a link; otherwise its whole input.
    const detail = input.command ?? input.file_path ?? input.url ?? JSON.stringify(input);
    const shown = document.createElement("p");
    shown.append(code(String(detail)));
    item.append(shown);
  }
  item.append(text("p", `Answer: ${answerText(decision)}`, "answer"));
  return item;
}
