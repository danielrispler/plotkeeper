// The notice that no card writer can run (danielrispler/plotkeeper#51): one line of the notice line, beside the others.

const WRITER = /^Summaries are off: /;

// The notice line's text with the writer's line set to `notice`, or removed when it is null.
export function withWriterNotice(text: string, notice: string | null): string {
  const others = text.split("\n").filter((line) => line && !WRITER.test(line));
  return (notice === null ? others : [...others, notice]).join("\n");
}

// The writer's line while summaries wait for a Test (spec #320); it starts as a failure notice does, so a later one replaces it.
export const GATE = "Summaries are off: pick a writer under Writer, at the top of the page, and press Test.";

// The notice line's text with the gate shown, or, when `gated` is false, without it: a writer's failure notice stays.
export function withGateNotice(text: string, gated: boolean): string {
  if (gated) return withWriterNotice(text, GATE);
  return text.split("\n").filter((line) => line && line !== GATE).join("\n");
}
