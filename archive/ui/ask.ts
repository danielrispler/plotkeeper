// Removed from ui/ask.ts in #415: `hunkFiles`, which nothing imported.

// A stored hunk (`--- path`, then that file's lines, per file) as its files in order; none for an empty text.
export function hunkFiles(stored: string): { path: string; lines: string }[] {
  const files: { path: string; lines: string }[] = [];
  for (const line of stored.split("\n")) {
    if (line.startsWith("--- ")) files.push({ path: line.slice(4), lines: "" });
    else if (files.length) files.at(-1)!.lines += `${line}\n`;
  }
  return files.map((f) => ({ ...f, lines: f.lines.replace(/\n+$/, "") }));
}
