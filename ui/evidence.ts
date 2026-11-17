// The evidence line (danielrispler/plotkeeper#113): under a turn's Headline, what the hooks and git recorded for the turn, with
// no model in it. The top level counts the turn's files and each check's final result; a click opens it, in place, to the
// files with their line counts, the external changes, and each check with its status. A failed check's row opens its exit code
// and the tail of its output. Opening changes only the nodes it opens: the open state is kept by key, so a redraw of the rail
// draws the line open again.

import { code, invoke, pressable, text } from "./dom.js";
import { askAnchor, type Hunk, hunkText } from "./ask.js";
import { diffRequest } from "./diff.js";
import { openAt } from "./paths.js";
import type { Capabilities, Check, Turn, TurnFile } from "./types.js";

type Facts = Pick<Turn, "files" | "checks">;
export type EvidenceRow = { kind: "file" | "external" | "check"; label: string; call?: string; agent?: string | null; file?: TurnFile };

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

// The top level: "3 files · 2 checks passed". A check's status is its last run's, so only one that ended failed is named;
// one that only "ran" is counted apart.
export function evidenceLine({ files, checks }: Facts): string {
  const count = (status: Check["status"]) => checks.filter((c) => c.status === status).length;
  const [passed, ran] = [count("passed"), count("ran")];
  return [
    ...(files?.files.length ? [plural(files.files.length, "file")] : []),
    ...(passed ? [`${plural(passed, "check")} passed`] : []),
    ...(ran ? [`${plural(ran, "check")} ran`] : []),
    ...checks.filter((c) => c.status === "failed").map((c) => `✗ ${c.suite} failed`),
  ].join(" · ");
}

// Whether the turn has an evidence line: its agent reports trees (agy's does not), and it has files or checks.
export const hasEvidence = (turn: Facts, capabilities: Pick<Capabilities, "turn_trees">): boolean =>
  capabilities.turn_trees && (turn.files?.files.length ?? 0) + turn.checks.length > 0;

// The opened list: each file, "+N external" for the other changes in the worktree, and each check with its status.
export function evidenceRows({ files, checks }: Facts): EvidenceRow[] {
  const counts = (added: number | null, removed: number | null) => (added === null || removed === null ? "" : ` +${added} −${removed}`);
  return [
    ...(files?.files ?? []).map((f): EvidenceRow => ({ kind: "file", label: `${f.from ? `${f.from} → ` : ""}${f.path}${counts(f.added, f.removed)}`, file: f })),
    ...(files?.external ? [{ kind: "external" as const, label: `+${files.external} external` }] : []),
    ...checks.map((c): EvidenceRow => {
      const after = c.retries ? ` after ${`${c.retries} ${c.retries === 1 ? "retry" : "retries"}`}` : "";
      const last = c.runs.at(-1);
      return {
        kind: "check",
        label: `${c.suite} ${c.status}${c.status === "ran" ? "" : after}`,
        ...(c.status === "failed" && last ? { call: last.call, agent: last.agent } : {}),
      };
    }),
  ];
}

const openLines = new Set<string>();
const openOutputs = new Set<string>();
const outputs = new Map<string, string>();

type Output = { exit_code: number | null; tail: string };
const outputText = (output: Output) => `${output.exit_code === null ? "No exit code" : `exit code ${output.exit_code}`}\n${output.tail}`;

// A failed check's row: its label, which opens its exit code and the output's tail under it. Only this row changes.
function checkRow(session: string, row: EvidenceRow): HTMLElement {
  const item = document.createElement("li");
  item.className = "evidence-check";
  if (row.call === undefined) {
    item.append(text("span", row.label));
    return item;
  }
  const key = `${session} ${row.agent ?? ""} ${row.call}`;
  const pre = text("pre", outputs.get(key) ?? "", "evidence-output");
  const show = () => (openOutputs.has(key) ? pre.removeAttribute("hidden") : pre.setAttribute("hidden", ""));
  const fetch = () => {
    if (outputs.has(key)) return;
    pre.textContent = "Reading the output…";
    invoke<Output | null>("check_output", { session, agent: row.agent, call: row.call })
      .then((output) => {
        outputs.set(key, output ? outputText(output) : "No output was recorded.");
        pre.textContent = outputs.get(key)!;
      })
      .catch((e) => {
        pre.textContent = `The output could not be read: ${e}`;
      });
  };
  const button = pressable(document.createElement("button"), openOutputs.has(key), () => {
    if (openOutputs.has(key)) openOutputs.delete(key);
    else openOutputs.add(key);
    button.setAttribute("aria-expanded", String(openOutputs.has(key)));
    show();
    if (openOutputs.has(key)) fetch();
  });
  button.classList.add("evidence-fail");
  button.setAttribute("aria-expanded", String(openOutputs.has(key)));
  button.append(text("span", "✗", "mark"), code(row.label));
  show();
  if (openOutputs.has(key)) fetch();
  item.append(button, pre, askAnchor(session, { kind: "check", agent: row.agent ?? null, call: row.call }, "this failed check"));
  return item;
}

const openHunks = new Set<string>();
const hunks = new Map<string, Hunk | null>();

// A file's row: its label, which opens the real hunk between the turn's two trees under it, with "Open in editor" at its
// first changed line. Only this row changes.
export function fileRow(session: string, index: number, row: EvidenceRow, scope = ""): HTMLElement {
  const file = row.file!;
  const key = `${session} ${index} ${file.root} ${file.path}`;
  // Whether it is open is its own to each place that draws it; the hunk read is shared.
  const openKey = `${scope}${key}`;
  const item = document.createElement("li");
  item.className = "evidence-file";
  const pre = text("pre", hunks.has(key) ? hunkText(hunks.get(key)!, false) : "", "evidence-hunk");
  const edit = pressable(document.createElement("button"), false, () => {
    const first = hunks.get(key)?.first_line ?? 1;
    openAt({ path: file.path, line: first, column: 1 }, file.root);
  });
  edit.classList.add("evidence-edit");
  edit.textContent = "Open in editor";
  const left = (h: Hunk | null) => {
    more.textContent = `${h?.left_out ?? 0} more lines`;
    if (openHunks.has(openKey) && h?.left_out) more.removeAttribute("hidden");
    else more.setAttribute("hidden", "");
  };
  const more = pressable(document.createElement("button"), false, () => diffRequest.open(session, index));
  more.classList.add("evidence-more");
  const show = () => {
    const open = openHunks.has(openKey);
    for (const node of [pre, edit]) open ? node.removeAttribute("hidden") : node.setAttribute("hidden", "");
    left(hunks.get(key) ?? null);
  };
  const fetch = () => {
    if (hunks.has(key)) return;
    pre.textContent = "Reading the hunk…";
    invoke("hunk_opened", { session, turn: index }).catch((e) => console.warn(e));
    invoke<Hunk | null>("turn_file_hunk", { session, turn: index, root: file.root, path: file.path, from: file.from })
      .then((h) => {
        hunks.set(key, h);
        pre.textContent = hunkText(h, false);
        left(h);
      })
      .catch((e) => {
        pre.textContent = `The hunk could not be read: ${e}`;
      });
  };
  const button = pressable(document.createElement("button"), openHunks.has(openKey), () => {
    if (openHunks.has(openKey)) openHunks.delete(openKey);
    else openHunks.add(openKey);
    button.setAttribute("aria-expanded", String(openHunks.has(openKey)));
    show();
    if (openHunks.has(openKey)) fetch();
  });
  button.classList.add("evidence-fail");
  button.setAttribute("aria-expanded", String(openHunks.has(openKey)));
  button.append(text("span", row.label));
  show();
  if (openHunks.has(openKey)) fetch();
  item.append(button, edit, pre, more, askAnchor(session, { kind: "file", turn: index, root: file.root, path: file.path, from: file.from }, `the file ${file.path}`));
  return item;
}

// "Open full diff": the turn's whole diff in the terminal column (`diff.ts`).
function fullDiffRow(session: string, index: number): HTMLElement {
  const item = document.createElement("li");
  item.append(pressable(text("button", "Open full diff", "evidence-edit"), false, () => diffRequest.open(session, index)));
  return item;
}

// The line for turn `index` of `session`. `only: "failed"` is the running turn's: just the checks whose last run failed.
// Null when there is nothing to show.
export function evidenceElement(session: string, index: number, turn: Facts, { only }: { only: "all" | "failed" }): HTMLElement | null {
  const facts: Facts = only === "failed" ? { files: null, checks: turn.checks.filter((c) => c.status === "failed") } : turn;
  const line = evidenceLine(facts);
  if (!line) return null;
  const key = `${session} ${index} evidence`;
  const node = document.createElement("div");
  node.className = "evidence";
  const list = document.createElement("ul");
  list.className = "evidence-rows";
  list.append(
    ...evidenceRows(facts).map((row) => (row.kind === "check" ? checkRow(session, row) : row.file ? fileRow(session, index, row) : text("li", row.label, "external"))),
    ...(facts.files?.files.length ? [fullDiffRow(session, index)] : []),
  );
  const show = () => (openLines.has(key) ? list.removeAttribute("hidden") : list.setAttribute("hidden", ""));
  const button = pressable(document.createElement("button"), openLines.has(key), () => {
    if (openLines.has(key)) openLines.delete(key);
    else openLines.add(key);
    button.setAttribute("aria-expanded", String(openLines.has(key)));
    show();
  });
  button.classList.add("evidence-toggle");
  button.setAttribute("aria-expanded", String(openLines.has(key)));
  button.append(text("span", line));
  show();
  node.append(button, list);
  return node;
}
