// File paths in the terminal pane and in rendered replies: the text a click opens in Daniel's editor
// (#557), and a dropped file's path as typed.

import { invoke } from "./dom.js";
import { studio } from "./state.js";

// A path with at least one folder, absolute or relative, and an optional line: `:line`, `:line:column`,
// `:line-end`, `#Lline` or `#Lline-Lend`. It may not start inside a URL or a longer word.
const path = /(?<![\w:/.~-])(?:~|\.{1,2})?\/?[\w.@+-]+(?:\/[\w.@+-]+)+(?::\d+(?:-\d+|:\d+)?|#L\d+(?:-L\d+)?)?/g;
// The line suffix of a link's text: a line, then a column after `:`, and any range end ignored.
const lineSuffix = /(?::(\d+)(?:-\d+|:(\d+))?|#L(\d+)(?:-L\d+)?)$/;

// The paths in one line of terminal text, with where each starts.
export function pathLinks(line: string): { text: string; start: number }[] {
  return [...line.matchAll(path)]
    .map((m) => ({ text: m[0].replace(/\.+$/, ""), start: m.index }))
    .filter((link) => /[A-Za-z]/.test(link.text));
}

// The path as the shell reads it back: as is when safe, else single-quoted.
export const quotePath = (p: string) => (/^[\w./@+:,-]+$/.test(p) ? p : `'${p.replaceAll("'", "'\\''")}'`);

// The task worktree a folder is in: `<main>/.claude/worktrees/<task>`, else null.
export function worktreeOf(cwd: string): string | null {
  return cwd.match(/^(.*\/\.claude\/worktrees\/[^/]+)/)?.[1] ?? null;
}

// A path's last 2 segments, such as `scratchpad/map266.md` (#571); the full path goes on hover.
export const shortPath = (p: string) => p.split("/").filter(Boolean).slice(-2).join("/");

// `line` with each path of more than 2 segments shortened, and the full line for its hover, or "" when
// nothing was shortened.
export function shortPaths(line: string): [shown: string, title: string] {
  const shown = line.replace(path, (p) => (p.split("/").filter(Boolean).length > 2 ? shortPath(p) : p));
  return [shown, shown === line ? "" : line];
}

export type FileTarget = { path: string; line: number; column: number };

// The file a link's text names, at its line and column (1 when none): a path with a line suffix, or
// a `file://` URL, any host dropped. null: a URL of another scheme.
export function fileTarget(text: string): FileTarget | null {
  if (/^file:/i.test(text)) {
    const url = new URL(text);
    text = decodeURIComponent(url.pathname) + url.hash;
  } else if (/^[a-z][\w+.-]*:(?!\d)/i.test(text)) return null;
  const m = lineSuffix.exec(text);
  const path = m ? text.slice(0, m.index) : text;
  return { path, line: Number(m?.[1] ?? m?.[3] ?? 1), column: Number(m?.[2] ?? 1) };
}

// Opens the file `text` names in Daniel's editor at its line, through the shell's one opener; a
// relative path from `cwd`, the folder of the session that printed it (null: the repository).
export function openFile(text: string, cwd: string | null) {
  const target = fileTarget(text);
  if (target) invoke("open_file", { ...target, cwd, editor: studio.files.settings.editor }).catch((e) => console.warn(e));
}
