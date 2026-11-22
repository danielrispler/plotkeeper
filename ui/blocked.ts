// A blocked command's card (danielrispler/plotkeeper#170, #171): the Bash command auto mode blocked, in full, with its
// folder, the rule that blocked it and the subagent that asked. While its request stands, Allow
// lets the agent retry it once and Decline closes the card; there is no edit, so what runs is what the card shows.

import { blockedRequestFor, sendBlockedAnswer } from "./answer.js";
import { code, failure, invoke, text } from "./dom.js";
import { counts, duration } from "./times.js";
import { typeInto, type TypingIo } from "./typing.js";
import { blockedWaits, type BlockedCommand, type BlockedStatus } from "./types.js";

export type BlockedView = {
  state: string;
  command: string;
  folder: string;
  reason: string;
  agent: string | null;
  buttons: BlockedButton[];
  // Running: the time since its retry started. Finished: its exit code. The tail is the output's last lines, once read;
  // a background run has none.
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

// What each of a blocked command's buttons does, on its hover: a label without one does not compile.
type BlockedButton = "Allow" | "Decline" | "Ask again";
const blockedHovers: Record<BlockedButton, string> = {
  Allow: "Let the agent run this command once",
  Decline: "Refuse the command; the agent is told",
  "Ask again": "Renew the approval and ask the agent to run the command again",
};

// The buttons of a card that waits: Allow and Decline while Daniel has not chosen, Ask again once his Allow went unused.
function buttonsOf(status: BlockedStatus, answerable: boolean): BlockedButton[] {
  if (!blockedWaits(status)) return [];
  if (status === "allowed_not_run") return ["Ask again"];
  return (status === "blocked" || status === "blocked_again") && answerable ? ["Allow", "Decline"] : [];
}

// Whether the dock holds `card`: exactly when it shows a button, so orange means Daniel has something to click
// (danielrispler/plotkeeper#297). Every other card sits in the feed, neutral.
export const docksInDock = (card: BlockedCommand, answerable: boolean): boolean => buttonsOf(card.status, answerable).length > 0;

// Whether a docked card shows its state line: Blocked again and Allowed, not run say what they are; a plain Blocked card keeps the dock's look.
export const stateInDock = (status: BlockedStatus): boolean => status === "blocked_again" || status === "allowed_not_run";

// The call whose output a finished card shows, and the agent that ran it, which may be the session's own for a
// subagent's command; null before it finished, and for a background run, whose output the studio does not read.
export function outputCall(card: BlockedCommand): { agent: string | null; call: string } | null {
  if (card.status !== "finished" || !card.run || card.task) return null;
  return { agent: card.run_by ?? null, call: card.run };
}

// What the card shows of `read`, the output's last lines.
const tailText = (read: BlockedOutput | null): string => (read?.tail.trim() ? read.tail : "No output was recorded.");

// The tails read so far, by the run's call id.
const tails = new Map<string, BlockedOutput | null>();

// How long a late Allow's retry line waits for its answer to reach the session.
const ANSWER_MS = 3_000;

// The line typed into the session's terminal to have the agent retry `card`'s command: for the session's own block, or,
// when `agent` (the subagent's name) asked, one that tells the session to run it in the subagent's folder.
export function retryLine(card: BlockedCommand, agent: string | null): string {
  if (agent === null) return `Daniel approved the blocked command: \`${card.command}\`. Run it again.`;
  return `Daniel approved the command subagent \`${agent}\` was blocked on. Run \`cd ${card.cwd}\` as its own Bash call, then run exactly: \`${card.command}\`. Or pass it to the subagent if it still runs.`;
}

// Types the retry line of `card` into terminal `id`, through `typeInto`: never over Daniel's draft or into a session that needs him.
// It first waits, up to 3 s, until `answered`: a late Allow's answer has reached the session and ended the needs-you of its block.
export async function typeRetry(
  card: BlockedCommand,
  agent: string | null,
  id: string,
  blocked: () => boolean,
  io: TypingIo,
  answered: () => boolean = () => true,
): Promise<boolean> {
  const until = io.now() + ANSWER_MS;
  while (!answered() && io.now() < until) await io.sleep(100);
  return typeInto(id, retryLine(card, agent), blocked, io);
}

// What the card shows for `card`, asked by `agent` (the subagent's name, null for the session's own agent), or null once
// Daniel declined it. `answerable`: its request file is still there, so Allow and Decline have something to answer, also after its hook has returned.
// `output`: what reading its output gave, `undefined` until it is read.
export function blockedView(
  card: BlockedCommand,
  agent: string | null,
  answerable: boolean,
  now: number = Date.now(),
  output?: BlockedOutput | null,
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
    tail: outputCall(card) && output !== undefined ? tailText(output) : null,
  };
}

// `retry` types the card's retry line into the session's terminal (`story.ts`).
export function blockedElement(session: string, card: BlockedCommand, agent: string | null, retry: (card: BlockedCommand) => void): HTMLElement | null {
  const request = blockedRequestFor(card.call);
  const read = outputCall(card);
  const view = blockedView(card, agent, request !== null, Date.now(), read ? tails.get(read.call) : undefined);
  if (view === null) return null;
  const block = document.createElement("article");
  block.className = "question decision blocked-command";
  const state = text("p", view.state, "state");
  // Orange only while the card has a button; in the dock a state line shows for the cards that name their state.
  if (view.buttons.length > 0) state.dataset.state = "needs_you";
  if (stateInDock(card.status)) state.classList.add("kept");
  const asked = text("p", view.agent === null ? "Auto mode blocked a command" : `Auto mode blocked a command of ${view.agent}`, "asked");
  const command = document.createElement("p");
  command.append(code(view.command));
  block.append(state, asked, command, text("p", `In ${view.folder}`, "description"), text("p", view.reason, "description"));
  // A Running card's time counts up each second, as a running check's does, until the card leaves the page.
  if (card.status === "running") block.append(counts(text("p", "", "description"), (at) => detailOf(card, at) ?? ""));
  else if (view.detail !== null) block.append(text("p", view.detail, "description"));
  if (read && !tails.has(read.call)) {
    const pre = text("pre", "Reading the output…", "evidence-output");
    block.append(pre);
    invoke("check_output", { session, agent: read.agent, call: read.call })
      .then((output) => {
        tails.set(read.call, output);
        pre.textContent = tailText(output);
      })
      .catch((e) => {
        pre.textContent = `The output could not be read: ${e}`;
      });
  } else if (view.tail !== null) block.append(text("pre", view.tail, "evidence-output"));
  if (view.buttons.length > 0) {
    const row = document.createElement("p");
    row.className = "answer-buttons";
    for (const label of view.buttons) {
      const button = text("button", label, "btn") as HTMLButtonElement;
      button.type = "button";
      button.title = blockedHovers[label];
      button.addEventListener("click", async () => {
        if (label === "Ask again") {
          // The pass may have expired or been spent, so the approval is renewed before the line asks for the retry.
          await invoke("ask_again", { session, command: card.command, cwd: card.cwd }).catch(failure("ask_again", "error"));
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
