// A blocked command's card (danielrispler/plotkeeper#170, #171): the Bash command auto mode blocked, in full, with its
// folder, the rule that blocked it and the subagent that asked. While the answer hook waits for Daniel's click, Allow
// lets the agent retry it once and Decline closes the card; there is no edit, so what runs is what the card shows.

import { blockedRequestFor, sendBlockedAnswer } from "./answer.js";
import { code, invoke, text } from "./dom.js";
import { duration } from "./times.js";
import type { BlockedCommand, BlockedStatus } from "./types.js";

export type BlockedView = {
  state: string;
  command: string;
  folder: string;
  reason: string;
  agent: string | null;
  buttons: string[];
  // Running: the time since its retry started. Finished: its exit code. The tail is the output's last lines, once read.
  detail: string | null;
  tail: string | null;
};

export type BlockedOutput = { exit_code: number | null; tail: string };

const states: Record<Exclude<BlockedStatus, "declined">, string> = {
  blocked: "Blocked",
  allowed: "Allowed",
  running: "Running",
  finished: "Finished",
  running_in_background: "Running in background",
  blocked_again: "Blocked again",
};

// What the card shows for `card`, asked by `agent` (the subagent's name, null for the session's own agent), or null once
// Daniel declined it. `waiting`: its hook still waits for his click, so Allow and Decline have something to answer.
function detailOf(card: BlockedCommand, now: number): string | null {
  if (card.status === "running") return duration(now - (card.run_at ?? now));
  if (card.status !== "finished") return null;
  return card.exit_code == null ? "No exit code" : `exit code ${card.exit_code}`;
}

// The tails read so far, by the run's call id.
const tails = new Map<string, BlockedOutput | null>();

export function blockedView(
  card: BlockedCommand,
  agent: string | null,
  waiting: boolean,
  now: number = Date.now(),
  output: BlockedOutput | null = null,
): BlockedView | null {
  if (card.status === "declined") return null;
  return {
    state: states[card.status],
    command: card.command,
    folder: card.cwd,
    reason: card.reason,
    agent,
    buttons: (card.status === "blocked" && waiting) || card.status === "blocked_again" ? ["Allow", "Decline"] : [],
    detail: detailOf(card, now),
    tail: card.status === "finished" ? (output?.tail ?? null) : null,
  };
}

export function blockedElement(session: string, card: BlockedCommand, agent: string | null): HTMLElement | null {
  const request = blockedRequestFor(card.call);
  const run = card.status === "finished" ? card.run : null;
  const output = run ? (tails.get(run) ?? null) : null;
  const view = blockedView(card, agent, request?.waiting === true, Date.now(), output);
  if (view === null) return null;
  const block = document.createElement("article");
  block.className = "question decision blocked-command";
  const state = text("p", view.state, "state");
  if (card.status === "blocked" || card.status === "blocked_again") state.dataset.state = "needs_you";
  const asked = text("p", view.agent === null ? "Auto mode blocked a command" : `Auto mode blocked a command of ${view.agent}`, "asked");
  const command = document.createElement("p");
  command.append(code(view.command));
  block.append(state, asked, command, text("p", `In ${view.folder}`, "description"), text("p", view.reason, "description"));
  if (view.detail !== null) block.append(text("p", view.detail, "description"));
  if (run && !tails.has(run)) {
    const pre = text("pre", "Reading the output…", "evidence-output");
    block.append(pre);
    invoke<BlockedOutput | null>("check_output", { session, agent: card.agent, call: run })
      .then((read) => {
        tails.set(run, read);
        pre.textContent = read?.tail ?? "No output was recorded.";
      })
      .catch((e) => {
        pre.textContent = `The output could not be read: ${e}`;
      });
  } else if (view.tail !== null) block.append(text("pre", view.tail, "evidence-output"));
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
