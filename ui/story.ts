// The story rail, on the window's left: the selected session's state and step, its brief, cards and
// checks, with its open question docked at the bottom. A subagent's view shows its own line,
// questions, brief and cards; the checks are its session's.

import { code, element, invoke, text } from "./dom.js";
import { encode, request } from "./daemon.js";
import { agentState } from "./list.js";
import { closePane, startClaude } from "./panes.js";
import { questionElement } from "./questions.js";
import { stateLabels, type CheckLine, type Question, type Selection, type Session } from "./types.js";

export function showView(selected: Selection | null) {
  element("empty").hidden = selected !== null;
  element("view").hidden = selected === null;
  element("identity").textContent = "";
  if (selected === null) {
    showQuestions([]);
    return;
  }
  const session = selected.session;
  const agent = session.agents.find((a) => a.id === selected.agent) ?? null;
  const [label, state] = agent === null ? [stateLabels[session.state], session.state] : agentState(session, agent);
  const step = (agent ?? session).last_step;
  element("view").toggleAttribute("data-agent", agent !== null);
  element("identity").textContent =
    agent === null ? `${session.name ?? session.folder_name} · ${session.id.slice(0, 8)}` : `${session.folder_name} · ${agent.name}`;
  element("state").textContent = label;
  element("state").dataset.state = state;
  // "Open" resumes an ended session in a new terminal, in its folder, where Claude Code keeps it.
  element("reopen").hidden = agent !== null || session.state !== "ended";
  element("reopen").onclick = () => startClaude(session.cwd, session.id).catch((e) => console.warn(e));
  element("rename").hidden = agent !== null;
  element("rename").onclick = () => askName(session);
  // Only a terminal the studio runs can be closed from here.
  element("close").hidden = agent !== null || session.terminal === null;
  element("close").onclick = () => askClose(session);
  element("tool").textContent = step?.tool ?? (agent === null ? session.returned : null) ?? "No step yet";
  element("detail").textContent = step?.detail ?? "";
  showQuestions(session.questions.filter((question) => question.agent === (agent?.id ?? null)));
  showChecks(session.checks);
}

// Rename (#469): the row's name, kept in the event log, and `/rename` typed into the session while
// its claude runs, so Claude Code shows the same name. An empty name gives the row its folder's again.
// shortcut: the command lands after anything Daniel left typed in its prompt; clear the line first
// if that bites.
function askName(session: Session) {
  const dialog = element("rename-dialog") as HTMLDialogElement;
  const field = element("rename-field") as HTMLInputElement;
  field.value = session.name ?? "";
  dialog.onclose = async () => {
    if (dialog.returnValue !== "rename") return;
    const name = field.value.trim();
    await invoke("rename", { id: session.id, name }).catch((e) => console.warn(e));
    if (!name || session.terminal === null || session.state === "ended") return;
    const type = (data: string) => request({ op: "input", id: session.terminal, data: encode(new TextEncoder().encode(data)) });
    await type(`/rename ${name}`).then(() => type("\r")).catch((e) => console.warn(e));
  };
  dialog.returnValue = "";
  dialog.showModal();
}

// Close (#469): ends the session's terminal, asking first while it works or has background tasks.
function askClose(session: Session) {
  const close = () => closePane(session.terminal!).catch((e) => console.warn(e));
  if (!session.asks_before_closing) return close();
  const dialog = element("close-dialog") as HTMLDialogElement;
  element("close-name").textContent = session.name ?? session.folder_name;
  dialog.onclose = () => dialog.returnValue === "close" && close();
  dialog.returnValue = "";
  dialog.showModal();
}

// The questions the viewed agent asked: an open one docked under the story, the rest in it, newest
// first.
function showQuestions(questions: Question[]) {
  const newest = [...questions].reverse();
  const open = newest.filter((question) => question.answer === "open");
  const past = newest.filter((question) => question.answer !== "open");
  element("dock").replaceChildren(...open.map(questionElement));
  element("dock").hidden = open.length === 0;
  element("questions").replaceChildren(...past.map(questionElement));
  element("questions-title").hidden = past.length === 0;
}

// Whether a check passed, from its exit code; with no colour, as colour is only for "needs you".
function outcome(passed: boolean | null): string {
  if (passed === null) return "Ran";
  return passed ? "Passed" : "Failed";
}

// The checks that ran, newest first, one line each.
function showChecks(checks: CheckLine[]) {
  element("checks").replaceChildren(
    ...[...checks].reverse().map((line) => {
      const item = document.createElement("li");
      item.append(text("span", outcome(line.passed), "outcome"), code(line.command));
      return item;
    }),
  );
  element("checks-title").hidden = checks.length === 0;
}
