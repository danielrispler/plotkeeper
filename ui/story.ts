// The story rail, on the window's left: the selected session's state and step, its brief, cards and
// checks, with its open question docked at the bottom. A subagent's view shows its own line,
// questions, brief and cards; the checks are its session's.

import { code, element, invoke, text } from "./dom.js";
import { askElement } from "./ask.js";
import { encode, request } from "./daemon.js";
import { agentState, contextText, sessionName } from "./list.js";
import { closePane, startClaude } from "./panes.js";
import { openDecision } from "./answer.js";
import { showCleared } from "./decisions.js";
import { decisionElement, questionElement } from "./questions.js";
import { worktreeOf } from "./paths.js";
import { checkTime, counts, estimateText, timesText } from "./times.js";
import { studio } from "./state.js";
import { stateLabels, type CheckLine, type Losses, type Places, type Question, type Selection, type Session, type Transcript } from "./types.js";

export function showView(selected: Selection | null) {
  element("empty").hidden = selected !== null;
  element("view").hidden = selected === null;
  element("identity").textContent = "";
  element("times").replaceChildren();
  if (selected === null) {
    showQuestions([]);
    showCleared([]);
    return;
  }
  const session = selected.session;
  const agent = session.agents.find((a) => a.id === selected.agent) ?? null;
  const [label, state] = agent === null ? [stateLabels[session.state], session.state] : agentState(session, agent);
  const step = (agent ?? session).last_step;
  const stepSpan = (agent ?? session).step_span;
  const estimate = document.createElement("span");
  estimate.className = "estimate";
  estimate.textContent = agent === null ? estimateText(session.work) : "";
  element("times").replaceChildren(counts(document.createElement("span"), (now) => timesText(session, stepSpan, now)), estimate);
  element("view").toggleAttribute("data-agent", agent !== null);
  const share = contextText(session.transcript.context);
  element("identity").textContent =
    agent === null
      ? [sessionName(session), share && `context ${share}`].filter(Boolean).join(" · ")
      : `${sessionName(session)} · ${agent.name}`;
  element("state").textContent = label;
  element("state").dataset.state = state;
  // "Open" resumes an ended session in a new terminal, in its folder, where Claude Code keeps it.
  element("reopen").hidden = agent !== null || session.state !== "ended";
  element("reopen").onclick = () => startClaude(session.cwd, session.resume_id).catch((e) => console.warn(e));
  element("rename").hidden = agent !== null;
  element("rename").onclick = () => askName(session);
  // Only a terminal the studio runs can be closed from here.
  element("close").hidden = agent !== null || session.terminal === null;
  element("close").onclick = () => askClose(session);
  element("delete").hidden = agent !== null;
  element("delete").onclick = () => askDelete(session).catch(report);
  element("tool").textContent = step?.tool ?? (agent === null ? session.returned : null) ?? "No step yet";
  element("detail").textContent = step?.detail ?? "";
  element("command").textContent = step?.command ?? "";
  element("step").classList.toggle("has-command", Boolean(step?.command));
  // A subagent's transcript is its own, which the studio does not read.
  showStory(session.id, agent === null ? session.transcript : null);
  showQuestions(session.questions.filter((question) => question.agent === (agent?.id ?? null)));
  showChecks(session.checks);
  showCleared(agent === null ? session.cleared : []);
}

// After this many steps the intention dims, as it may no longer say what the agent does (#472).
const FRESH_STEPS = 5;

// The intention, its age, the turn's full responses, each with "ask about this" (#482), and the mark
// when the transcript is unreadable.
function showStory(session: string, story: Transcript | null) {
  const steps = story?.steps_ago ?? 0;
  element("intention-text").textContent = story?.intention ?? "";
  element("intention-ask").replaceChildren(...(story?.intention ? [askElement(session, story.intention)] : []));
  element("intention-age").textContent = steps === 0 ? "" : `· ${steps} ${steps === 1 ? "step" : "steps"} ago`;
  element("intention").classList.toggle("old", steps > FRESH_STEPS);
  element("unavailable").hidden = !story?.unavailable;
  element("unavailable-why").textContent = story?.unavailable ? `The transcript cannot be read: ${story.unavailable}.` : "";
  const responses = story?.responses ?? [];
  element("responses").hidden = responses.length === 0;
  element("responses-title").textContent = `Responses (${responses.length})`;
  element("responses-body").replaceChildren(
    ...responses.flatMap((response) => [text("pre", response), askElement(session, response)]),
  );
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
  element("close-name").textContent = sessionName(session);
  dialog.onclose = () => dialog.returnValue === "close" && close();
  dialog.returnValue = "";
  dialog.showModal();
}

// Shows a failed delete's reason over the story.
function report(error: unknown) {
  element("notice").textContent = String(error);
  element("notice").hidden = false;
}

// Opens `dialog` and resolves to the button Daniel chose, "" for Esc.
function choice(dialog: HTMLDialogElement): Promise<string> {
  return new Promise((resolve) => {
    dialog.onclose = () => resolve(dialog.returnValue);
    dialog.returnValue = "";
    dialog.showModal();
  });
}

// Delete: ends the session's terminal, removes its worktree when ticked, asking first when that would
// lose work, and its branch when the main checkout holds it, then drops its row. A branch git keeps
// is reported, and the row goes anyway.
async function askDelete(session: Session) {
  const dialog = element("delete-dialog") as HTMLDialogElement;
  const worktree = worktreeOf(session.cwd);
  const ticked = element("delete-worktree") as HTMLInputElement;
  ticked.checked = false;
  element("delete-name").textContent = sessionName(session);
  element("delete-working").hidden = !(session.terminal !== null && session.asks_before_closing);
  element("delete-worktree-option").hidden = worktree === null;
  element("delete-worktree-path").textContent = worktree ?? "";
  const others = studio.listed.filter((s) => s.id !== session.id && s.state !== "ended" && worktreeOf(s.cwd) === worktree);
  element("delete-shared").hidden = worktree === null || others.length === 0;
  if ((await choice(dialog)) !== "delete") return;
  if (session.terminal !== null) await closePane(session.terminal);
  if (worktree !== null && ticked.checked) await removeWorktree(worktree);
  await invoke("delete_session", { id: session.id });
}

// The branch is git's, read before the worktree goes: a session with no turn yet has none in its
// transcript.
async function removeWorktree(worktree: string) {
  const { tasks } = await invoke<Places>("places");
  const branch = tasks.find((task) => task.path === worktree)?.branch ?? null;
  const losses = await invoke<Losses | null>("remove_task", { path: worktree, confirmed: false });
  if (losses) {
    const list = (title: string, lines: string[]) => {
      if (!lines.length) return [];
      const items = document.createElement("ul");
      items.append(...lines.map((line) => text("li", line, "mono")));
      return [text("p", title), items];
    };
    element("delete-losses-list").replaceChildren(
      ...list("Uncommitted changes", losses.uncommitted),
      ...list("Commits not on origin (they stay on the branch)", losses.unpushed),
    );
    if ((await choice(element("delete-losses") as HTMLDialogElement)) !== "remove") return;
    await invoke("remove_task", { path: worktree, confirmed: true });
  }
  if (branch === null) return;
  await invoke("delete_merged_branch", { branch }).catch((e) => report(`Branch ${branch} kept: ${e}`));
}

// The questions the viewed agent asked: an open one docked under the story, newest first, and those
// never answered in the story; an answered one is in the decision history (#479). A plan or a permission the answer hook waits on docks too, whichever agent asks (#478).
function showQuestions(questions: Question[]) {
  const newest = [...questions].reverse();
  const open = newest.filter((question) => question.answer === "open");
  const past = newest.filter((question) => question.answer === "not_answered");
  const decision = openDecision();
  const docked = [...(decision ? [decisionElement(decision)] : []), ...open.map(questionElement)];
  element("dock").replaceChildren(...docked);
  element("dock").hidden = docked.length === 0;
  element("questions").replaceChildren(...past.map(questionElement));
  element("questions-title").hidden = past.length === 0;
}

// Whether a check passed, from its exit code; with no colour, as colour is only for "needs you".
function outcome(passed: boolean | null): string {
  if (passed === null) return "Ran";
  return passed ? "Passed" : "Failed";
}

// The checks, newest first, one line each, with how long each took or, while it runs, its time against
// its typical time.
function showChecks(checks: CheckLine[]) {
  element("checks").replaceChildren(
    ...[...checks].reverse().map((line) => {
      const item = document.createElement("li");
      const label = line.ended_at === null ? "Running" : outcome(line.passed);
      const time = counts(text("span", "", "check-time"), (now) => checkTime(line, now));
      item.append(text("span", label, "outcome"), code(line.command), time);
      return item;
    }),
  );
  element("checks-title").hidden = checks.length === 0;
}
