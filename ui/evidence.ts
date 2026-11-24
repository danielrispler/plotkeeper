// The evidence line (danielrispler/plotkeeper#113): under a turn's Headline, what the hooks and git recorded for the turn, with
// no model in it. The top level counts the turn's files and each check's final result; a click opens it, in place, to the
// files with their line counts, and each check with its status. A failed check's row opens its exit code
// and the tail of its output. Opening changes only the nodes it opens: the open state is kept by key, so a redraw of the rail
// draws the line open again.

import { code, failure, fullTitle, glyphOnly, invoke, pressable, text } from "./dom.js";
import { askAnchor, type Hunk, hunkText } from "./ask.js";
import { diffRequest } from "./diff.js";
import type { Capabilities, Check, Turn, TurnFile } from "./types.js";
import { HOVERS } from "./hovers.js";

type Facts = Pick<Turn, "files" | "checks">;
type Ended = { ended_at: Turn["ended_at"] };
export type EvidenceRow = { kind: "file" | "check" | "merged"; label: string; call?: string; agent?: string | null; file?: TurnFile };

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

const mergedLabel = (n: number) => `+${n} merged`;

// A check's name: its suite, and its worktree's folder name when it ran outside the session's own worktree.
const named = (c: Check) => {
  const tree = c.worktree.replace(/\/+$/, "").split("/").pop();
  return c.foreign && tree ? `${c.suite} in ${tree}` : c.suite;
};

// A check is listed once it has ended: a running one is not passed, failed or ran.
const ended = (c: Check) => c.status !== "running";

// The top level: "3 files · +2 merged · 2 checks passed". A check's status is its last run's, so only one that ended failed is named;
// one that only "ran" is counted apart.
// `words` gives a failed check's name in plain words once it is known (`failed_check_name`); until then, and for a check it does
// not know, the check is named by its suite.
export function evidenceLine({ files, checks: all }: Facts, words: (check: Check) => string | undefined = () => undefined): string {
  const checks = all.filter(ended);
  const count = (status: Check["status"]) => checks.filter((c) => c.status === status).length;
  const [passed, ran] = [count("passed"), count("ran")];
  return [
    ...(files?.files.length ? [plural(files.files.length, "file")] : []),
    ...(files?.merged ? [mergedLabel(files.merged)] : []),
    ...(passed ? [`${plural(passed, "check")} passed`] : []),
    ...(ran ? [`${plural(ran, "check")} ran`] : []),
    ...checks.filter((c) => c.status === "failed").map((c) => `✗ ${words(c) ?? `${named(c)} failed`}`),
  ].join(" · ");
}

// Whether the turn has an evidence line: its agent reports trees (agy's does not), and it has files or checks.
export const hasEvidence = (turn: Facts, capabilities: Pick<Capabilities, "turn_trees">): boolean =>
  capabilities.turn_trees && (turn.files?.files.length ?? 0) + (turn.files?.merged ?? 0) + turn.checks.filter(ended).length > 0;

// The opened list: each file with its agent, and each check with its status.
export function evidenceRows({ files, checks: all }: Facts): EvidenceRow[] {
  const checks = all.filter(ended);
  const counts = (added: number | null, removed: number | null) => (added === null || removed === null ? "" : ` +${added} −${removed}`);
  return [
    ...(files?.files ?? []).map((f): EvidenceRow => ({ kind: "file", label: `${f.from ? `${f.from} → ` : ""}${f.path}${counts(f.added, f.removed)}`, agent: f.agent, file: f })),
    ...(files?.merged ? [{ kind: "merged", label: mergedLabel(files.merged) } as EvidenceRow] : []),
    ...checks.map((c): EvidenceRow => {
      const after = c.retries ? ` after ${`${c.retries} ${c.retries === 1 ? "retry" : "retries"}`}` : "";
      const last = c.runs.at(-1);
      return {
        kind: "check",
        label: `${named(c)} ${c.status}${c.status === "ran" ? "" : after}`,
        // The worker that made the last run, named as a file's agent is; a failed run's call reads its output.
        ...(last?.agent ? { agent: last.agent } : {}),
        ...(c.status === "failed" && last ? { call: last.call, agent: last.agent } : {}),
      };
    }),
  ];
}

const openLines = new Set<string>();
const openOutputs = new Set<string>();
const outputs = new Map<string, string>();

// A failed check's plain words by its last run (session, agent, call), kept once read: the failing tests and their part, or the
// step that failed (danielrispler/plotkeeper#345).
const failedWords = new Map<string, string>();
const wordsKey = (session: string, c: Check) => {
  const last = c.runs.at(-1);
  return last ? `${session} ${last.agent ?? ""} ${last.call}` : null;
};

export type CheckOutput = { exit_code: number | null; tail: string };
const outputText = (output: CheckOutput) => `${output.exit_code === null ? "No exit code" : `exit code ${output.exit_code}`}\n${output.tail}`;

// A failed check's row: its label, which opens its exit code and the output's tail under it. Only this row changes.
function checkRow(session: string, row: EvidenceRow): HTMLElement {
  const item = document.createElement("li");
  item.className = "evidence-check";
  if (row.call === undefined) {
    item.append(text("span", row.label), ...(row.agent ? [text("span", row.agent, "evidence-agent")] : []));
    return item;
  }
  const key = `${session} ${row.agent ?? ""} ${row.call}`;
  const pre = text("pre", outputs.get(key) ?? "", "evidence-output");
  // What the read says, the output or why it could not be read, is announced politely (#456).
  pre.setAttribute("aria-live", "polite");
  // The output scrolls on its own, so it takes focus, with a name, for the keys to scroll it (#462).
  pre.setAttribute("tabindex", "0");
  pre.setAttribute("aria-label", `Output of ${row.label}`);
  const show = () => (openOutputs.has(key) ? pre.removeAttribute("hidden") : pre.setAttribute("hidden", ""));
  const fetch = () => {
    if (outputs.has(key)) return;
    pre.textContent = "Reading the output…";
    invoke("check_output", { session, agent: row.agent, call: row.call })
      .then((output) => {
        outputs.set(key, output ? outputText(output) : "No output was recorded.");
        pre.textContent = outputs.get(key)!;
      })
      .catch((e) => {
        pre.textContent = `The output could not be read: ${e}`;
      });
  };
  const button = pressable(document.createElement("button"), null, () => {
    if (openOutputs.has(key)) openOutputs.delete(key);
    else openOutputs.add(key);
    button.setAttribute("aria-expanded", String(openOutputs.has(key)));
    show();
    if (openOutputs.has(key)) fetch();
  });
  button.classList.add("evidence-fail");
  button.title = `Show or hide the output of ${row.label}`;
  button.setAttribute("aria-expanded", String(openOutputs.has(key)));
  // The label already says "failed": the ✗ is for the eye alone (#456).
  button.append(glyphOnly("✗", "mark"), code(row.label), ...(row.agent ? [text("span", row.agent, "evidence-agent")] : []));
  show();
  if (openOutputs.has(key)) fetch();
  item.append(button, pre, askAnchor(session, { kind: "check", agent: row.agent ?? null, call: row.call }, "this failed check"));
  return item;
}

const openHunks = new Set<string>();
// A hunk read once its turn ended is final. One read of a turn still open is kept for the redraws of that opening only, while the
// file's line counts stay as they were at the read, and a turn that ended since is read again.
const hunks = new Map<string, { hunk: Hunk | null; ended: boolean; added: number | null; removed: number | null }>();

// A file's row: its label, which opens the real hunk between the turn's two trees under it, with "Open in editor" at its
// first changed line. Only this row changes.
export function fileRow(session: string, index: number, row: EvidenceRow, scope = "", ended = false): HTMLElement {
  const file = row.file!;
  const key = `${session} ${index} ${file.root} ${file.path}`;
  // Whether it is open is its own to each place that draws it; the hunk read is shared.
  const openKey = `${scope}${key}`;
  const item = document.createElement("li");
  item.className = "evidence-file";
  // What is kept of its hunk: final, or the read of an open turn while the turn is still open.
  const kept = () => {
    const entry = hunks.get(key);
    const same = entry && entry.added === file.added && entry.removed === file.removed;
    return entry && (entry.ended || (!ended && same)) ? entry : undefined;
  };
  const pre = text("pre", kept() ? hunkText(kept()!.hunk, false) : "", "evidence-hunk");
  pre.setAttribute("aria-live", "polite");
  pre.setAttribute("tabindex", "0");
  pre.setAttribute("aria-label", `Changes to ${file.path}`);
  const left = (h: Hunk | null) => {
    more.textContent = `${h?.left_out ?? 0} more lines`;
    if (openHunks.has(openKey) && h?.left_out) more.removeAttribute("hidden");
    else more.setAttribute("hidden", "");
  };
  const more = pressable(document.createElement("button"), null, () => diffRequest.open(session, index));
  more.classList.add("evidence-more");
  more.title = HOVERS.wholeDiff;
  const show = () => {
    const open = openHunks.has(openKey);
    for (const node of [pre]) open ? node.removeAttribute("hidden") : node.setAttribute("hidden", "");
    left(kept()?.hunk ?? null);
  };
  const fetch = () => {
    if (kept()) return;
    pre.textContent = "Reading the hunk…";
    invoke("turn_file_hunk", { session, turn: index, root: file.root, path: file.path, from: file.from })
      .then((h) => {
        hunks.set(key, { hunk: h, ended, added: file.added, removed: file.removed });
        pre.textContent = hunkText(h, false);
        left(h);
      })
      .catch((e) => {
        pre.textContent = `The hunk could not be read: ${e}`;
      });
  };
  const button = pressable(document.createElement("button"), null, () => {
    if (openHunks.has(openKey)) openHunks.delete(openKey);
    else openHunks.add(openKey);
    button.setAttribute("aria-expanded", String(openHunks.has(openKey)));
    // An open turn's hunk is read again at each opening.
    if (!ended) hunks.delete(key);
    show();
    if (openHunks.has(openKey)) fetch();
  });
  button.classList.add("evidence-fail");
  button.title = `Show or hide this file's change: ${file.path}`;
  button.setAttribute("aria-expanded", String(openHunks.has(openKey)));
  button.append(text("span", row.label), ...(row.agent ? [text("span", row.agent, "evidence-agent")] : []));
  show();
  if (openHunks.has(openKey)) fetch();
  item.append(button, pre, more, askAnchor(session, { kind: "file", turn: index, root: file.root, path: file.path, from: file.from }, `the file ${file.path}`, scope));
  return item;
}

// The merged files' row: a count, with no hunk, as they are not the turn's change.
function mergedRow(row: EvidenceRow): HTMLElement {
  return text("li", row.label, "evidence-merged");
}

// "Open full diff": the turn's whole diff in the terminal column (`diff.ts`).
function fullDiffRow(session: string, index: number): HTMLElement {
  const item = document.createElement("li");
  item.append(fullTitle(pressable(text("button", "Open full diff", "evidence-edit"), null, () => diffRequest.open(session, index)), HOVERS.wholeDiff));
  return item;
}

// The evidence line into `node`, each failed check's ✗ hidden from assistive tech, as its words already say "failed" (#456).
function lineInto(node: HTMLElement, line: string) {
  node.replaceChildren(...line.split("✗").flatMap((part, i) => (i ? [glyphOnly("✗"), part] : [part])));
}

// The line for turn `index` of `session`. `only: "failed"` is the running turn's: just the checks whose last run failed.
// Null when there is nothing to show.
export function evidenceElement(session: string, index: number, turn: Facts & Ended, { only }: { only: "all" | "failed" }): HTMLElement | null {
  const facts: Facts = only === "failed" ? { files: null, checks: turn.checks.filter((c) => c.status === "failed") } : turn;
  const words = (c: Check) => {
    const key = wordsKey(session, c);
    return key ? failedWords.get(key) : undefined;
  };
  const line = evidenceLine(facts, words);
  if (!line) return null;
  const key = `${session} ${index} evidence`;
  const node = document.createElement("div");
  node.className = "evidence";
  const list = document.createElement("ul");
  list.className = "evidence-rows";
  list.append(
    ...evidenceRows(facts).map((row) =>
      row.kind === "check" ? checkRow(session, row) : row.kind === "merged" ? mergedRow(row) : fileRow(session, index, row, "", turn.ended_at != null),
    ),
    ...(facts.files?.files.length ? [fullDiffRow(session, index)] : []),
  );
  const show = () => (openLines.has(key) ? list.removeAttribute("hidden") : list.setAttribute("hidden", ""));
  const button = pressable(document.createElement("button"), null, () => {
    if (openLines.has(key)) openLines.delete(key);
    else openLines.add(key);
    button.setAttribute("aria-expanded", String(openLines.has(key)));
    show();
  });
  button.classList.add("evidence-toggle");
  button.title = "Show or hide this turn's files and checks";
  button.setAttribute("aria-expanded", String(openLines.has(key)));
  const label = text("span", "");
  lineInto(label, line);
  button.append(label);
  // A failed check's words come from its output: read it once, then say the line again.
  for (const c of facts.checks.filter((c) => c.status === "failed")) {
    const key = wordsKey(session, c);
    const last = c.runs.at(-1);
    if (!key || !last || failedWords.has(key)) continue;
    invoke("check_output", { session, agent: last.agent, call: last.call })
      .then((output) => (output ? invoke("failed_check_name", { tail: output.tail, suite: c.suite, worktree: c.worktree }) : null))
      .then((named) => {
        if (!named) return;
        failedWords.set(key, named);
        lineInto(label, evidenceLine(facts, words));
      })
      .catch(failure("failed_check_name"));
  }
  show();
  node.append(button, list);
  return node;
}
