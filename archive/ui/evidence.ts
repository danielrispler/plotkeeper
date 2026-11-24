// Removed from ui/evidence.ts in #415: `openHunk`, which nothing imported.

// Opens `file`'s hunk in turn `index` of `session`, and the evidence line that holds it; `scope` is the place that draws it (#243).
export function openHunk(session: string, index: number, file: TurnFile, scope = "") {
  openHunks.add(`${scope}${session} ${index} ${file.root} ${file.path}`);
  if (!scope) openLines.add(`${session} ${index} evidence`);
}
