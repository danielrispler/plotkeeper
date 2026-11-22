// File paths in a step's detail: shown by their last 2 segments, the full path on hover.

// A path's start (home, `.` or `..`, and a root) and one of its segments, and its line suffix: `:line`,
// `:line:column`, `:line-end`, `#Lline` or `#Lline-Lend`.
const start = String.raw`(?:~|\.{1,2})?\/?`;
const segment = String.raw`[\w.@+-]+`;
const suffix = String.raw`(?::(\d+)(?:-\d+|:(\d+))?|#L(\d+)(?:-L\d+)?)`;
// A path with at least one folder, absolute or relative, and an optional line. It may not start inside
// a URL or a longer word.
const path = new RegExp(String.raw`(?<![\w:/.~-])${start}${segment}(?:\/${segment})+${suffix}?`, "g");

// A path's last 2 segments, such as `scratchpad/map266.md` (danielrispler/Falinks#571); the full path goes on hover.
export const shortPath = (p: string) => p.split("/").filter(Boolean).slice(-2).join("/");

// `line` with each path of more than 2 segments shortened, and the full line for its hover, or "" when
// nothing was shortened.
export function shortPaths(line: string): [shown: string, title: string] {
  const shown = line.replace(path, (p) => (p.split("/").filter(Boolean).length > 2 ? shortPath(p) : p));
  return [shown, shown === line ? "" : line];
}
