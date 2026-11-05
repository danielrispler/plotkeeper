// File paths in the terminal pane: the text a click opens, and a dropped file's path as typed.

// A path with at least one folder, absolute or relative, and an optional `:line:column`. It may not
// start inside a URL or a longer word.
const path = /(?<![\w:/.~-])(?:~|\.{1,2})?\/?[\w.@+-]+(?:\/[\w.@+-]+)+(?::\d+){0,2}/g;

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
