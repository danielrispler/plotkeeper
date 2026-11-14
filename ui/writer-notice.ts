// The notice that no card writer can run (danielrispler/plotkeeper#51): one line of the notice line, beside the others.

const WRITER = /^Cards and briefs are off: /;

// The notice line's text with the writer's line set to `notice`, or removed when it is null.
export function withWriterNotice(text: string, notice: string | null): string {
  const others = text.split("\n").filter((line) => line && !WRITER.test(line));
  return (notice === null ? others : [...others, notice]).join("\n");
}
