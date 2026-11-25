// The session's brief in the story rail: its Goal, decisions, open lines and "Design so far". Cards are retired as a turn's or a
// worker's explanation (danielrispler/plotkeeper#348); the PR brief is. This file keeps the brief and the one place the shell's
// session payload is taken in (`showCards`).

import { element, glyphOnly, pressable, prose, text } from "./dom.js";
import { drawSidebar } from "./list.js";
import { setCards, studio } from "./state.js";
import { showDecisions } from "./decisions.js";
import { drawFeed, fillDockNotes, forgetMissingSummaries, openCitation } from "./story.js";
import type { SessionCards } from "./types.js";

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

// A design line's citation: a button that opens the turn it came from in the rail (an older line may cite a card, which opens its turn).
function citation(id: string): HTMLElement {
  const button = pressable(text("button", "", "cite"), null, () => openCitation(id));
  button.title = "Open the turn it came from";
  button.setAttribute("aria-label", "Open the turn it came from");
  button.append(glyphOnly("↗"));
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

export function showCards(cards: SessionCards) {
  // A risk note that arrived fills its card's place in the dock where it stands, which only a session's own events redraw otherwise.
  const notes = JSON.stringify(cards.risk_notes) !== JSON.stringify(studio.cards.risk_notes);
  setCards(cards);
  showBrief(cards);
  showDecisions(cards.decisions, cards.decision_notes);
  // The store changed: a summary missing before may be in it now.
  forgetMissingSummaries();
  drawFeed();
  if (notes) fillDockNotes();
  // A summary was stored: the list's unseen rows show its Headline (#382).
  drawSidebar();
}
