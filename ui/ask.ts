// Ask on an anchor (danielrispler/plotkeeper#125): a file, a failed check, a subagent line or a Headline carries "Ask". Opening
// it shows what the step ledger and git recorded about the thing, at once, with no model: the core reads it through the
// ledger's offsets. A suggested question, or one typed, goes to the card writer's call (danielrispler/Falinks#461, decision 20) with
// those facts and, for a file, its hunk; the answer stays here, labelled as interpretation under the facts as recorded and the session never sees it. A failed call or a reply with no answer shows why, with
// a retry.

import { availability, failure, held, hint, invoke, isUnavailable, keepOnly, messageOf, onEnter, pressable, text } from "./dom.js";
import { keyLabel, primary } from "./keys.js";
import { rendered } from "./markdown.js";
import { platform } from "./settings.js";
import type { Anchor, Facts, Hunk } from "./types.js";

/** A hunk as plain text; `more` ends it with "… N more lines" when the cap left some out, for a view with no button of its own. */
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

// Each anchor's box, kept across the view's draws (one each time the session, its cards or a summary changes), so a question being
// typed, an open box and an answer all stay; a session that leaves the list takes its boxes with it (`forgetAsks`).
const asks = new Map<string, HTMLDetailsElement>();
// The session of each box's key.
const askedIn = new Map<string, string>();

/** Drops the boxes of sessions no longer listed. */
export function forgetAsks(listed: ReadonlySet<string>) {
  keepOnly(asks, (key) => listed.has(askedIn.get(key) ?? ""));
  keepOnly(askedIn, (key) => asks.has(key));
}
// `scope` keeps the boxes of one anchor drawn in two places apart, as a node has one parent.
const keyOf = (session: string, anchor: Anchor, scope: string) => `${scope}${session}\n${JSON.stringify(anchor)}`;

/**
 * The "Ask" box of `anchor` in session `session`, labelled `what` for the model ("this file"), keyed by its key and held, so the
 * view's `patch` keeps it where it is, the focus in it too (spec #661).
 */
export function askAnchor(session: string, anchor: Anchor, what: string, scope = ""): HTMLDetailsElement {
  const key = keyOf(session, anchor, scope);
  const box = asks.get(key) ?? askBox(session, anchor, what);
  box.setAttribute("data-key", `ask ${key}`);
  asks.set(key, box);
  askedIn.set(key, session);
  return held(box);
}

/** Whether the box of `anchor` is open, or holds an answer or a question being asked (spec #661, Q29). */
export function asked(session: string, anchor: Anchor): boolean {
  const box = asks.get(keyOf(session, anchor, ""));
  return !!box && (box.open || !!box.querySelector("div.ask-reply")?.hasChildNodes());
}

/** Opens the box of `anchor` and puts the cursor in it, when it is drawn; false when it is not. */
export function openAsk(session: string, anchor: Anchor): boolean {
  const node = asks.get(keyOf(session, anchor, ""));
  if (!node?.isConnected) return false;
  node.open = true;
  node.scrollIntoView?.({ block: "nearest" });
  node.querySelector("textarea")?.focus();
  return true;
}

// biome-ignore lint/complexity/noExcessiveLinesPerFunction: ratcheted at #554; split it rather than grow it
function askBox(session: string, anchor: Anchor, what: string): HTMLDetailsElement {
  const node = document.createElement("details");
  node.className = "ask-about";
  const recorded = document.createElement("div");
  recorded.className = "ask-facts";
  // Read at each opening, as a running turn's facts change.
  const read = () => {
    if (!recorded.hasChildNodes()) recorded.replaceChildren(text("p", "Reading what was recorded…", "asking"));
    invoke("ask_facts", { session, anchor })
      .then((facts) => {
        const sections = (facts?.sections ?? []).map((s) => {
          const list = document.createElement("ul");
          list.append(...s.facts.map((f) => text("li", `${f.at ? `${clock(f.at)} ` : ""}${f.text}`)));
          return [text("p", s.label, "label"), list];
        });
        recorded.replaceChildren(...(sections.length ? sections.flat() : [text("p", "Nothing was recorded about this.", "asking")]));
        if (anchor.kind === "file") showHunk(anchor);
      })
      .catch((e) => recorded.replaceChildren(text("p", `The facts could not be read: ${messageOf(e)}`, "ask-error")));
  };
  const hunk = text("pre", "", "evidence-hunk");
  const showHunk = (a: Extract<Anchor, { kind: "file" }>) => {
    invoke("turn_file_hunk", { session, turn: a.turn, root: a.root, path: a.path, from: a.from ?? null })
      .then((h) => {
        hunk.textContent = hunkText(h);
        recorded.append(text("p", "The hunk", "label"), hunk);
      })
      .catch(failure("turn_file_hunk"));
  };
  const field = document.createElement("textarea");
  field.rows = 2;
  field.name = "question";
  field.autocomplete = "off";
  field.placeholder = "Your question; the session never sees it…";
  field.setAttribute("aria-label", `Your question about ${what}`);
  const sendHover = `Ask about this, from what was recorded; the session never sees it (${keyLabel(platform === "mac" ? "cmd-enter" : "ctrl-enter")})`;
  const send = button("Ask");
  availability(send, sendHover);
  const reply = document.createElement("div");
  reply.className = "ask-reply";
  reply.setAttribute("aria-live", "polite");
  const ask = async () => {
    if (isUnavailable(send)) return;
    const question = field.value.trim();
    if (!question) return field.focus();
    availability(send, sendHover, "Waiting for the answer to your question");
    reply.replaceChildren(text("p", "Asking…", "asking"));
    try {
      const answer = await invoke("ask_anchor", { session, anchor, what, question });
      // The recorded facts are above; the answer is the model's reading of them.
      reply.replaceChildren(text("p", "Interpretation", "label"), rendered("div", answer, ""));
    } catch (e) {
      const retry = hint(button("Retry"), "Ask the same question again");
      retry.addEventListener("click", ask);
      reply.replaceChildren(text("p", `No answer: ${messageOf(e)}.`, "ask-error"), retry);
    }
    availability(send, sendHover);
  };
  send.addEventListener("click", ask);
  // ⌘↩ asks (Ctrl+Enter off macOS), as Enter alone makes a new line.
  field.addEventListener("keydown", (event) => primary(event) && event.key === "Enter" && ask());
  const why = "turn" in anchor ? whyNotBox(session, anchor.turn) : null;
  const picks = document.createElement("div");
  picks.className = "ask-suggestions";
  for (const question of suggestions(anchor)) {
    const pick = hint(button(question), "Ask this question about it");
    pick.addEventListener("click", () => {
      field.value = question;
      void ask();
    });
    picks.append(pick);
  }
  node.addEventListener("toggle", () => node.open && read());
  node.append(hint(text("summary", "Ask", "ask-summary"), `Ask a question about ${what}, from what was recorded`), recorded, picks, field, send, reply, ...(why ? [why] : []));
  return node;
}

/**
 * What was recorded about `name`, then the writer's possible reasons folded under their own label, so a guess never reads as a
 * fact. With nothing recorded there are no reasons, as there was no call.
 */
export function whyNotElement(name: string, result: { facts: Facts; reasons: string | null }): HTMLElement {
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
    reasons.append(hint(text("summary", "possible reasons (not recorded)"), "Show or hide the writer's guesses, which nothing recorded"), rendered("div", result.reasons, ""));
    node.append(reasons);
  }
  return node;
}

// "Why didn't it touch ...?" on turn `turn`.
function whyNotBox(session: string, turn: number): HTMLElement {
  const node = document.createElement("div");
  node.className = "ask-why";
  const field = document.createElement("input");
  field.name = "not-touched";
  field.autocomplete = "off";
  field.spellcheck = false;
  field.placeholder = "A file or a name, e.g. main.ts…";
  field.setAttribute("aria-label", "A file or a name it did not touch");
  const goHover = "Find what was recorded about the name you typed, and why the turn did not touch it (↩)";
  const go = button("Why didn't it touch this?");
  availability(go, goHover);
  const out = document.createElement("div");
  out.setAttribute("aria-live", "polite");
  const run = async () => {
    if (isUnavailable(go)) return;
    const name = field.value.trim();
    if (!name) return field.focus();
    availability(go, goHover, "Reading what was recorded");
    out.replaceChildren(text("p", "Reading what was recorded…", "asking"));
    try {
      const result = await invoke("ask_why_not", { session, turn, name });
      out.replaceChildren(whyNotElement(name, result));
    } catch (e) {
      failure("ask_why_not", "error")(e);
      out.replaceChildren(text("p", `No answer: ${messageOf(e)}.`, "ask-error"));
    }
    availability(go, goHover);
  };
  go.addEventListener("click", run);
  onEnter(field, () => void run());
  node.append(field, go, out);
  return node;
}

function button(label: string): HTMLButtonElement {
  return pressable(text("button", label, "btn"), null);
}
