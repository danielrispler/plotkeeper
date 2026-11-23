// The Map view's background runs (spec danielrispler/plotkeeper#378, ticket #390): the project's queue beside the map, a running run and Cancel,
// how an ended run ended, and one indicator while a login or limit failure pauses every queue. A run is a headless agent the server starts
// on Daniel's click (`server/src/runner.rs`), never a session; no module starts one yet. A run that failed, or finished with tool calls
// denied, offers its prompt to copy into his own agent, as an overturn does (`ratify.ts`).

import { failure, invoke, text } from "./dom.js";
import { onDraw, reloadMap, syncSide, type MapView } from "./map.js";
import { copyOverturn, overturnReport } from "./ratify.js";
import type { RunView, Runs } from "./types.js";

export type RunRow = { id: string; state: RunView["state"]; title: string; note: string; action: "Cancel" | "Dismiss"; denials: string[]; prompt: string | null };
// What the rows' buttons do: `cancel` stops a queued or running run and removes an ended one.
export type RunActs = { cancel: (id: string) => void; resume: () => void; copy: (prompt: string) => Promise<"copied" | "manual"> };

const kindWords: Record<RunView["kind"], string> = { "back-law": "Back a law", "micro-world": "Micro-world" };
const clock = (at: number) => {
  const time = new Date(at);
  return `${String(time.getHours()).padStart(2, "0")}:${String(time.getMinutes()).padStart(2, "0")}`;
};

// The project's runs as rows, in the order the server lists them, which is the order they were asked for.
export function runRows(runs: Runs | null | undefined): RunRow[] {
  const list = runs?.runs ?? [];
  const next = list.find((run) => run.state === "queued")?.id;
  return list.map((run) => {
    const waits = runs?.paused ? "Queued, paused" : run.id === next ? "Queued, next" : "Queued";
    const note = run.state === "ended" ? (run.line ?? "Ended") : run.state === "running" ? `Running since ${clock(run.at)}` : waits;
    return { id: run.id, state: run.state, title: kindWords[run.kind] ?? run.kind, note, action: run.state === "ended" ? "Dismiss" : "Cancel", denials: run.denials ?? [], prompt: run.prompt ?? null };
  });
}

const button = (words: string, title: string, click: () => unknown) => {
  const node = text("button", words, "btn quiet");
  node.setAttribute("type", "button");
  node.title = title;
  node.addEventListener("click", click);
  return node;
};

function rowNode(row: RunRow, act: RunActs): HTMLElement {
  const node = text("div", "", "run-row");
  node.dataset.run = row.id;
  node.dataset.state = row.state;
  const actions = text("span", "", "run-actions");
  node.append(text("span", row.title, "run-title"), text("span", row.note, "run-note"));
  if (row.denials.length) {
    const denied = text("ul", "", "run-denials");
    denied.append(...row.denials.map((denial) => text("li", denial)));
    node.append(denied);
  }
  const prompt = row.prompt;
  if (prompt !== null) {
    // The copy starts inside the click, so the browser lets it reach the clipboard.
    const copy = button("Copy prompt", "Copy this run's prompt, to paste into your own agent", async () => {
      const report = overturnReport(await act.copy(prompt), prompt);
      const line = text("p", report.line, "run-copied");
      line.setAttribute("role", "status");
      const shown: HTMLElement[] = [line];
      if (report.manual !== null) {
        const area = document.createElement("textarea");
        area.className = "ratify-prompt";
        area.value = report.manual;
        area.readOnly = true;
        area.rows = 4;
        shown.push(area);
      }
      copy.disabled = true;
      node.append(...shown);
    }) as HTMLButtonElement;
    actions.append(copy);
  }
  const title = row.state === "ended" ? "Remove this run from the list" : row.state === "running" ? "Stop this run and remove what it made" : "Take this run out of the queue";
  actions.append(button(row.action, title, () => act.cancel(row.id)));
  node.append(actions);
  return node;
}

// What the runs area holds: nothing with no run and no pause; else its title, the pause's one indicator with Resume, and a row per run.
export function runsNodes(runs: Runs | null | undefined, act: RunActs): HTMLElement[] {
  const rows = runRows(runs);
  const paused = runs?.paused ?? null;
  if (!rows.length && paused === null) return [];
  const nodes = [text("h3", "Background runs", "side-title")];
  if (paused !== null) {
    const indicator = text("p", "", "runs-paused");
    indicator.setAttribute("role", "status");
    indicator.append(text("span", `Background runs are paused. ${paused}.`), button("Resume", "Start the queued runs again", act.resume));
    nodes.push(indicator);
  }
  return [...nodes, ...rows.map((row) => rowNode(row, act))];
}

// The server tells the page when a run starts or ends (`map-landed`); a click's own change is read back at once.
const acts: RunActs = {
  cancel: (id) => void invoke("run_cancel", { id }).then(reloadMap).catch(failure("run_cancel")),
  resume: () => void invoke("runs_resume").then(reloadMap).catch(failure("runs_resume")),
  copy: (prompt) => copyOverturn(prompt, navigator.clipboard?.writeText?.bind(navigator.clipboard)),
};

function paint(view: MapView) {
  // A past commit's map is read-only and shows no runs.
  view.slots.runs.replaceChildren(...(view.scrub ? [] : runsNodes(view.state.runs, acts)));
  syncSide(view.slots);
}

onDraw(paint);
