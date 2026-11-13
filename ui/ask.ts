// "Ask about this" (danielrispler/Falinks#461, decision 20): a side question on a response, card or intention. The shell
// answers it with the card writer's own call, read with the turn's intentions; the answer stays here
// and the session never sees it. A failed call or a reply with no answer shows why, with a retry.

import { invoke, text } from "./dom.js";
import { rendered } from "./markdown.js";
import { studio } from "./state.js";

// Each item's box, kept across the view's redraws (every second), so a question being typed, an
// open box and an answer all stay. shortcut: never pruned; prune by the listed sessions if it grows.
const asks = new Map<string, HTMLElement>();

// The "Ask about this" box under `item`, the text of session `session`'s response, card or intention.
export function askElement(session: string, item: string): HTMLElement {
  const key = `${session}\n${item}`;
  if (!asks.has(key)) asks.set(key, askBox(session, item));
  return asks.get(key)!;
}

function askBox(session: string, item: string): HTMLElement {
  const node = document.createElement("details");
  node.className = "ask-about";
  const field = document.createElement("textarea");
  field.rows = 2;
  field.placeholder = "Your question. The session never sees it.";
  field.setAttribute("aria-label", "Your question about this");
  const send = button("Ask");
  const reply = document.createElement("div");
  reply.className = "ask-reply";
  reply.setAttribute("aria-live", "polite");
  const ask = async () => {
    const question = field.value.trim();
    if (!question) return field.focus();
    send.disabled = true;
    reply.replaceChildren(text("p", "Asking…", "asking"));
    try {
      const answer = await invoke<string>("ask", { session, item, question });
      const cwd = studio.listed.find((s) => s.id === session)?.cwd ?? null;
      reply.replaceChildren(rendered("div", answer, "", cwd));
    } catch (e) {
      const retry = button("Retry");
      retry.addEventListener("click", ask);
      reply.replaceChildren(text("p", `No answer: ${e}.`, "ask-error"), retry);
    }
    send.disabled = false;
  };
  send.addEventListener("click", ask);
  // ⌘↩ asks, as Enter alone makes a new line.
  field.addEventListener("keydown", (event) => event.metaKey && event.key === "Enter" && ask());
  node.append(text("summary", "Ask about this"), field, send, reply);
  return node;
}

function button(label: string): HTMLButtonElement {
  const node = text("button", label, "btn") as HTMLButtonElement;
  node.type = "button";
  return node;
}
