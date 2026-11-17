// A blocked command's card (danielrispler/plotkeeper#170, #171): the Bash command auto mode blocked, in full, with its
// folder, the rule that blocked it and the subagent that asked. While its request stands, Allow
// lets the agent retry it once and Decline closes the card; there is no edit, so what runs is what the card shows.

import { blockedRequestFor, sendBlockedAnswer } from "./answer.js";
import { code, invoke, text } from "./dom.js";
import { duration } from "./times.js";
import { typeInto, type TypingIo } from "./typing.js";
import { blockedWaits, type BlockedCommand, type BlockedStatus } from "./types.js";

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
  allowed_not_run: "Allowed, not run",
  running: "Running",
  finished: "Finished",
  running_in_background: "Running in background",
  blocked_again: "Blocked again",
};

// The card's second line: the time since a running retry started, or a finished one's exit code.
function detailOf(card: BlockedCommand, now: number): string | null {
  if (card.status === "running") return duration(now - (card.run_at ?? now));
  if (card.status !== "finished") return null;
  return card.exit_code == null ? "No exit code" : `exit code ${card.exit_code}`;
}

// The buttons of a card that waits: Allow and Decline while Daniel has not chosen, Ask again once his Allow went unused.
function buttonsOf(status: BlockedStatus, answerable: boolean): string[] {
  if (!blockedWaits(status)) return [];
  if (status === "allowed_not_run") return ["Ask again"];
  return status === "blocked_again" || (status === "blocked" && answerable) ? ["Allow", "Decline"] : [];
}

// The tails read so far, by the run's call id.
const tails = new Map<string, BlockedOutput | null>();

// The line typed into the session's terminal to have the agent retry `card`'s command: for the session's own block, or,
// when `agent` (the subagent's name) asked, one that tells the session to run it in the subagent's folder.
export function retryLine(card: BlockedCommand, agent: string | null): string {
  if (agent === null) return `Daniel approved the blocked command: \`${card.command}\`. Run it again.`;
  return `Daniel approved the command subagent \`${agent}\` was blocked on. Run \`cd ${card.cwd}\` as its own Bash call, then run exactly: \`${card.command}\`. Or pass it to the subagent if it still runs.`;
}

// Types the retry line of `card` into terminal `id`, through `typeInto`: never over Daniel's draft or into a session that needs him.
export async function typeRetry(card: BlockedCommand, agent: string | null, id: string, blocked: () => boolean, io: TypingIo): Promise<boolean> {
  return typeInto(id, retryLine(card, agent), blocked, io);
}

// What the card shows for `card`, asked by `agent` (the subagent's name, null for the session's own agent), or null once
// Daniel declined it. `answerable`: its request file is still there, so Allow and Decline have something to answer, also after its hook has returned.
export function blockedView(
  card: BlockedCommand,
  agent: string | null,
  answerable: boolean,
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
    buttons: buttonsOf(card.status, answerable),
    detail: detailOf(card, now),
    tail: card.status === "finished" ? (output?.tail ?? null) : null,
  };
}

// `retry` types the card's retry line into the session's terminal (`story.ts`).
export function blockedElement(session: string, card: BlockedCommand, agent: string | null, retry: (card: BlockedCommand) => void): HTMLElement | null {
  const request = blockedRequestFor(card.call);
  const run = card.status === "finished" ? card.run : null;
  const output = run ? (tails.get(run) ?? null) : null;
  const view = blockedView(card, agent, request !== null, Date.now(), output);
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
  if (view.buttons.length > 0 && (request !== null || card.status === "allowed_not_run")) {
    const row = document.createElement("p");
    row.className = "answer-buttons";
    for (const label of view.buttons) {
      const button = text("button", label, "btn") as HTMLButtonElement;
      button.type = "button";
      button.addEventListener("click", async () => {
        if (label === "Ask again") {
          // The pass may have expired or been spent, so the approval is renewed before the line asks for the retry.
          await invoke("ask_again", { session, command: card.command, cwd: card.cwd }).catch((e) => console.warn(e));
          return retry(card);
        }
        if (request === null) return;
        const took = await sendBlockedAnswer(session, request, label === "Allow" ? "allow" : "deny");
        // Its hook had returned, so nothing waited for the click: the agent is asked to retry in its terminal.
        if (label === "Allow" && took === "late") retry(card);
      });
      row.append(button);
    }
    block.append(row);
  }
  return block;
}
