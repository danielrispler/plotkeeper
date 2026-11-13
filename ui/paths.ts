// File paths in the terminal pane and in rendered replies: the text a click opens in Daniel's editor
// (danielrispler/Falinks#557), and a dropped file's path as typed.

import { invoke } from "./dom.js";
import { studio } from "./state.js";

// A path's start (home, `.` or `..`, and a root), one of its segments, and its line suffix: `:line`,
// `:line:column`, `:line-end`, `#Lline` or `#Lline-Lend`, capturing the line and column.
const start = String.raw`(?:~|\.{1,2})?\/?`;
const segment = String.raw`[\w.@+-]+`;
const suffix = String.raw`(?::(\d+)(?:-\d+|:(\d+))?|#L(\d+)(?:-L\d+)?)`;
// A path with at least one folder, absolute or relative, and an optional line. It may not start inside
// a URL or a longer word.
const path = new RegExp(String.raw`(?<![\w:/.~-])${start}${segment}(?:\/${segment})+${suffix}?`, "g");
// The line suffix of a link's text: a line, then a column after `:`, and any range end ignored.
const lineSuffix = new RegExp(`${suffix}$`);
// A whole text that is a path, with or without a folder, and its line suffix.
const wholePath = new RegExp(String.raw`^(${start}${segment}(?:\/${segment})*)(${suffix})?$`);
// The extensions a bare file name with a line, such as `a.rs:42`, may have; so `example.com:443` and
// `std.io:3` are not files.
const sourceExtension = /\.(?:rs|ts|tsx|js|mjs|cjs|go|py|md|json|toml|ya?ml|sh|html|css|txt|lock)$/i;

// The paths in one line of terminal text, with where each starts.
export function pathLinks(line: string): { text: string; start: number }[] {
  return [...line.matchAll(path)]
    .map((m) => ({ text: m[0].replace(/\.+$/, ""), start: m.index }))
    .filter((link) => /[A-Za-z]/.test(link.text));
}

// Whether inline code in a reply is a whole file path (danielrispler/Falinks#581): its last segment has a file extension,
// with no `@`, and either a folder whose first segment is no host name, or a line suffix and a source
// extension. So `src/a.rs`, `src/a.rs:42` and `a.rs:42` are; `origin/main:3`, `@types/node`,
// `git@github.com:22`, `example.com/index.html`, `std.io:3` and `localhost:8080` are not.
export function codePath(text: string): boolean {
  const m = wholePath.exec(text);
  if (!m || m[1].includes("@")) return false;
  const segments = m[1].split("/");
  const name = segments[segments.length - 1];
  if (!/[\w-]\.[A-Za-z]\w*$/.test(name)) return false;
  if (segments.length > 1) return !/^[\w-]+(?:\.[\w-]+)+$/.test(segments[0]);
  return !!m[2] && sourceExtension.test(name);
}

// The path as the shell reads it back: as is when safe, else single-quoted.
export const quotePath = (p: string) => (/^[\w./@+:,-]+$/.test(p) ? p : `'${p.replaceAll("'", "'\\''")}'`);

// The task worktree a folder is in: `<main>/.claude/worktrees/<task>`, else null.
export function worktreeOf(cwd: string): string | null {
  return cwd.match(/^(.*\/\.claude\/worktrees\/[^/]+)/)?.[1] ?? null;
}

// A path's last 2 segments, such as `scratchpad/map266.md` (danielrispler/Falinks#571); the full path goes on hover.
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
