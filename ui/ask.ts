// Ask on an anchor (danielrispler/plotkeeper#125): a file, a failed check, a subagent line or a Headline carries "Ask". Opening
// it shows what the step ledger and git recorded about the thing, at once, with no model: the core reads it through the
// ledger's offsets. A suggested question, or one typed, goes to the card writer's call (danielrispler/Falinks#461, decision 20) with
// those facts and, for a file, its hunk; the answer stays here, labelled as interpretation under the facts as recorded and the session never sees it. A failed call or a reply with no answer shows why, with
// a retry.

import { invoke, text } from "./dom.js";
import { rendered } from "./markdown.js";
import { studio } from "./state.js";

// What Ask is opened on: the core's `Anchor`.
export type Anchor =
  | { kind: "file"; turn: number; root: string; path: string; from?: string | null }
  | { kind: "check"; agent: string | null; call: string }
  | { kind: "subagent"; agent: string }
  | { kind: "headline"; turn: number };

// The recorded facts, in sections: `at` is Unix milliseconds when known.
export type Facts = { sections: { label: string; facts: { at: number | null; text: string }[] }[] };

export type Hunk = { text: string; left_out: number; binary: boolean; first_line: number };
// A hunk as plain text; `more` ends it with "… N more lines" when the cap left some out, for a view with no button of its own.
export const hunkText = (h: Hunk | null, more = true) =>
  h === null ? "The hunk could not be read." : h.binary ? "Binary file: no lines to show." : h.text + (more && h.left_out ? `… ${h.left_out} more lines` : "");

// Two or three questions each anchor type offers, after the question types developers ask of a trace (Sillito et al. 2006).
const questions: Record<Anchor["kind"], string[]> = {
  file: ["Why did it touch this file?", "What else could this change break?", "Why was it changed this way?"],
  check: ["Why did this check fail?", "What would fix it?"],
  subagent: ["What did this worker do?", "Why was it started?", "Did it finish what it was asked?"],
  headline: ["What did this turn change?", "What is still open?", "What should I check first?"],
};
export const suggestions = (anchor: Anchor): string[] => questions[anchor.kind];

const clock = (at: number) => new Date(at).toLocaleTimeString([], { hour12: false });

// Each anchor's box, kept across the view's redraws (every second), so a question being typed, an open box and an answer all
// stay. shortcut: never pruned; prune by the listed sessions if it grows.
const asks = new Map<string, HTMLDetailsElement>();
const keyOf = (session: string, anchor: Anchor) => `${session}\n${JSON.stringify(anchor)}`;

// The "Ask" box of `anchor` in session `session`, labelled `what` for the model ("this file").
export function askAnchor(session: string, anchor: Anchor, what: string): HTMLDetailsElement {
  const key = keyOf(session, anchor);
  if (!asks.has(key)) asks.set(key, askBox(session, anchor, what));
  return asks.get(key)!;
}

// Opens the box of `anchor` and puts the cursor in it, when it is drawn; false when it is not.
export function openAsk(session: string, anchor: Anchor): boolean {
  const node = asks.get(keyOf(session, anchor));
  if (!node?.isConnected) return false;
  node.open = true;
  node.scrollIntoView?.({ block: "nearest" });
  node.querySelector("textarea")?.focus();
  return true;
}

function askBox(session: string, anchor: Anchor, what: string): HTMLDetailsElement {
  const node = document.createElement("details");
  node.className = "ask-about";
  const recorded = document.createElement("div");
  recorded.className = "ask-facts";
  // Read at each opening, as a running turn's facts change.
  const read = () => {
    if (!recorded.hasChildNodes()) recorded.replaceChildren(text("p", "Reading what was recorded…", "asking"));
    invoke<Facts | null>("ask_facts", { session, anchor })
      .then((facts) => {
        const sections = (facts?.sections ?? []).map((s) => {
          const list = document.createElement("ul");
          list.append(...s.facts.map((f) => text("li", `${f.at ? `${clock(f.at)} ` : ""}${f.text}`)));
          return [text("p", s.label, "label"), list];
        });
        recorded.replaceChildren(...(sections.length ? sections.flat() : [text("p", "Nothing was recorded about this.", "asking")]));
        if (anchor.kind === "file") showHunk(anchor);
      })
      .catch((e) => recorded.replaceChildren(text("p", `The facts could not be read: ${e}`, "ask-error")));
  };
  const hunk = text("pre", "", "evidence-hunk");
  const showHunk = (a: Extract<Anchor, { kind: "file" }>) => {
    invoke<Hunk | null>("turn_file_hunk", { session, turn: a.turn, root: a.root, path: a.path, from: a.from ?? null })
      .then((h) => {
        hunk.textContent = hunkText(h);
        recorded.append(text("p", "The hunk", "label"), hunk);
      })
      .catch((e) => console.warn(e));
  };
  const field = document.createElement("textarea");
  field.rows = 2;
  field.placeholder = "Your question. The session never sees it.";
  field.setAttribute("aria-label", `Your question about ${what}`);
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
      const answer = await invoke<string>("ask_anchor", { session, anchor, what, question });
      const cwd = studio.listed.find((s) => s.id === session)?.cwd ?? null;
      // The recorded facts are above; the answer is the model's reading of them.
      reply.replaceChildren(text("p", "Interpretation", "label"), rendered("div", answer, "", cwd));
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
  const why = "turn" in anchor ? whyNotBox(session, anchor.turn) : null;
  const picks = document.createElement("div");
  picks.className = "ask-suggestions";
  for (const question of suggestions(anchor)) {
    const pick = button(question);
    pick.addEventListener("click", () => {
      field.value = question;
      void ask();
    });
    picks.append(pick);
  }
  node.addEventListener("toggle", () => node.open && read());
  node.append(text("summary", "Ask", "ask-summary"), recorded, picks, field, send, reply, ...(why ? [why] : []));
  return node;
}

// What was recorded about `name`, then the writer's possible reasons folded under their own label, so a guess never reads as a
// fact. With nothing recorded there are no reasons, as there was no call.
export function whyNotElement(name: string, result: { facts: Facts; reasons: string | null }, cwd: string | null = null): HTMLElement {
  const node = document.createElement("div");
  node.className = "ask-why-not";
  const sections = result.facts.sections;
  if (!sections.length) node.append(text("p", `Nothing was recorded: the turn never read, searched or mentioned ${name}.`));
  for (const section of sections) {
    const list = document.createElement("ul");
    list.append(...section.facts.map((f) => text("li", `${f.at ? `${clock(f.at)} ` : ""}${f.text}`)));
    node.append(text("p", section.label, "label"), list);
  }
  if (result.reasons) {
    const reasons = document.createElement("details");
    reasons.append(text("summary", "possible reasons (not recorded)"), rendered("div", result.reasons, "", cwd));
    node.append(reasons);
  }
  return node;
}

// "Why didn't it touch ...?" on turn `turn`.
function whyNotBox(session: string, turn: number): HTMLElement {
  const node = document.createElement("div");
  node.className = "ask-why";
  const field = document.createElement("input");
  field.placeholder = "a file or a name";
  field.setAttribute("aria-label", "A file or a name it did not touch");
  const go = button("Why didn't it touch this?");
  const out = document.createElement("div");
  out.setAttribute("aria-live", "polite");
  const run = async () => {
    const name = field.value.trim();
    if (!name) return field.focus();
    go.disabled = true;
    out.replaceChildren(text("p", "Reading what was recorded…", "asking"));
    try {
      const result = await invoke<{ facts: Facts; reasons: string | null }>("ask_why_not", { session, turn, name });
      const cwd = studio.listed.find((s) => s.id === session)?.cwd ?? null;
      out.replaceChildren(whyNotElement(name, result, cwd));
    } catch (e) {
      out.replaceChildren(text("p", `No answer: ${e}.`, "ask-error"));
    }
    go.disabled = false;
  };
  go.addEventListener("click", run);
  field.addEventListener("keydown", (event) => event.key === "Enter" && run());
  node.append(field, go, out);
  return node;
}

function button(label: string): HTMLButtonElement {
  const node = text("button", label, "btn") as HTMLButtonElement;
  node.type = "button";
  return node;
}
