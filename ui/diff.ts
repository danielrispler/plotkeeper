// The diff pane (danielrispler/plotkeeper#131): a turn's whole diff in the terminal column's place, as the subagent log
// is. Each file the agent or its workers changed is a fold with its line counts, its hunk between the turn's two trees read
// whole from git (`turn_file_hunk`, `all`), added and removed lines marked by a sign and a tint. The other changes in the
// worktree are one folded line at the end. The pane is drawn once when it opens, so folding a file moves nothing else.

import { invoke, pressable, text } from "./dom.js";
import { openAt } from "./paths.js";
import type { Capabilities, Session, Turn, TurnFile } from "./types.js";

export type DiffLineKind = "add" | "del" | "hunk" | "meta" | "ctx";
export type DiffLine = { kind: DiffLineKind; text: string };

// A hunk's lines classified for the tint; the sign stays in the text.
export function diffLines(hunk: string): DiffLine[] {
  const kind = (line: string): DiffLineKind =>
    /^(diff |index |--- |\+\+\+ |rename |similarity |new file|deleted file|old mode|new mode)/.test(line)
      ? "meta"
      : line.startsWith("@@")
        ? "hunk"
        : line.startsWith("+")
          ? "add"
          : line.startsWith("-")
            ? "del"
            : "ctx";
  return hunk
    .replace(/\n$/, "")
    .split("\n")
    .map((line) => ({ kind: kind(line), text: line }));
}

// The turn whose diff the palette opens: `index` of the newest turn that has files, none for an agent without trees.
export function newestDiff(turns: Pick<Turn, "files">[], capabilities: Pick<Capabilities, "turn_trees">): number | null {
  if (!capabilities.turn_trees) return null;
  for (let at = turns.length - 1; at >= 0; at--) if ((turns[at].files?.files.length ?? 0) > 0) return at;
  return null;
}

// Whether the shell's re-sent selection leaves the diff shown: the same session, and not Daniel's own click.
export const keepsDiff = (shown: { session: string } | null, session: Pick<Session, "id"> | null, clicked: boolean) =>
  shown !== null && !clicked && session?.id === shown.session;

// What the evidence line calls for the pane: set by the panes (which import this), so the line never imports them.
export const diffRequest = { open: (_session: string, _turn: number) => {} };

const counts = (f: TurnFile) => (f.added === null || f.removed === null ? "" : ` +${f.added} −${f.removed}`);
const unfolded = new Set<string>();
const folded = new Set<string>();

function fileFold(session: string, index: number, file: TurnFile): HTMLElement {
  const key = `${session} ${index} ${file.root} ${file.path}`;
  const fold = document.createElement("details");
  fold.className = "diff-file";
  fold.open = !folded.has(key);
  fold.addEventListener("toggle", () => {
    if (fold.open) folded.delete(key);
    else folded.add(key);
  });
  const edit = pressable(text("button", "Open in editor", "evidence-edit"), false, () => openAt({ path: file.path, line: first, column: 1 }, file.root));
  let first = 1;
  const body = text("div", "Reading the hunk…", "diff-body");
  fold.append(text("summary", `${file.from ? `${file.from} → ` : ""}${file.path}${counts(file)}`), edit, body);
  invoke<{ text: string; binary: boolean; first_line: number } | null>("turn_file_hunk", { session, turn: index, root: file.root, path: file.path, from: file.from, all: true })
    .then((hunk) => {
      first = hunk?.first_line ?? 1;
      if (!hunk || hunk.binary) return void body.replaceChildren(text("p", hunk ? "Binary file: no lines to show." : "The hunk could not be read.", "empty"));
      body.replaceChildren(...diffLines(hunk.text).map((line) => text("pre", line.text || " ", `diff-line ${line.kind}`)));
    })
    .catch((e) => body.replaceChildren(text("p", `The hunk could not be read: ${e}`, "empty")));
  return fold;
}

// Draws turn `index` of `session`'s diff in `place`; `toTerminal` returns to the terminal.
export function drawDiff(place: HTMLElement, session: Pick<Session, "id" | "turns">, index: number, toTerminal: () => void) {
  place.className = "pane turn-diff";
  const turn = session.turns[index];
  const files = turn?.files?.files ?? [];
  const external = turn?.files?.external ?? 0;
  const head = document.createElement("header");
  head.append(
    text("span", "Full diff", "log-title"),
    text("span", `${files.length} ${files.length === 1 ? "file" : "files"}`, "log-type"),
    pressable(text("button", "Terminal"), false, toTerminal),
  );
  const rest = document.createElement("details");
  rest.className = "diff-external";
  rest.append(text("summary", `+${external} external`), text("p", "Changes in the worktree that the agent did not make.", "empty"));
  place.replaceChildren(head, ...(files.length ? files.map((f) => fileFold(session.id, index, f)) : [text("p", "This turn changed no files.", "empty")]), ...(external ? [rest] : []));
}
