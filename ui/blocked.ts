// A blocked command's card (danielrispler/plotkeeper#170, #171): the Bash command auto mode blocked, in full, with its
// folder, the rule that blocked it and the subagent that asked. While the answer hook waits for Daniel's click, Allow
// lets the agent retry it once and Decline closes the card; there is no edit, so what runs is what the card shows.

import { blockedRequestFor, sendBlockedAnswer } from "./answer.js";
import { code, text } from "./dom.js";
import type { BlockedCommand } from "./types.js";

export type BlockedView = {
  state: string;
  command: string;
  folder: string;
  reason: string;
  agent: string | null;
  buttons: string[];
};

const states = { blocked: "Blocked", allowed: "Allowed" };

// What the card shows for `card`, asked by `agent` (the subagent's name, null for the session's own agent), or null once
// Daniel declined it. `waiting`: its hook still waits for his click, so Allow and Decline have something to answer.
export function blockedView(card: BlockedCommand, agent: string | null, waiting: boolean): BlockedView | null {
  if (card.status === "declined") return null;
  return {
    state: states[card.status],
    command: card.command,
    folder: card.cwd,
    reason: card.reason,
    agent,
    buttons: card.status === "blocked" && waiting ? ["Allow", "Decline"] : [],
  };
}

export function blockedElement(session: string, card: BlockedCommand, agent: string | null): HTMLElement | null {
  const request = blockedRequestFor(card.call);
  const view = blockedView(card, agent, request?.waiting === true);
  if (view === null) return null;
  const block = document.createElement("article");
  block.className = "question decision blocked-command";
  const state = text("p", view.state, "state");
  if (card.status === "blocked") state.dataset.state = "needs_you";
  const asked = text("p", view.agent === null ? "Auto mode blocked a command" : `Auto mode blocked a command of ${view.agent}`, "asked");
  const command = document.createElement("p");
  command.append(code(view.command));
  block.append(state, asked, command, text("p", `In ${view.folder}`, "description"), text("p", view.reason, "description"));
  if (view.buttons.length > 0 && request !== null) {
    const row = document.createElement("p");
    row.className = "answer-buttons";
    for (const label of view.buttons) {
      const button = text("button", label, "btn") as HTMLButtonElement;
      button.type = "button";
      button.addEventListener("click", () => sendBlockedAnswer(session, request, label === "Allow" ? "allow" : "deny"));
      row.append(button);
    }
    block.append(row);
  }
  return block;
}
