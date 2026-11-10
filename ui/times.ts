// Times (#473): how long a session, its turn, its step and its checks have run, from the core's
// timestamps. Running times count up once a second.

import type { CheckLine, Phase, Session, Span, Work } from "./types.js";

// A duration, such as "45s", "12m" or "2h 05m".
export function duration(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h ${String(minutes % 60).padStart(2, "0")}m`;
}

const spanText = (span: Span, now: number) => duration((span.end ?? now) - span.start);

// A time: its hover, and how to draw it at a time, such as "Turn 12m".
export type Time = [title: string, draw: (now: number) => string];

// The session's, its latest turn's and the step's times, each labelled with hover text (#517,
// decision 4), such as "Session 2h 05m", "Turn 12m" and "Step 45s". An ended session's time stops at
// its last event.
export function times(session: Pick<Session, "state" | "started_at" | "last_event_at" | "turn_span">, step: Span | null): Time[] {
  const end = session.state === "ended" ? session.last_event_at : null;
  const parts: Time[] = [["Time since the session started", (now) => `Session ${spanText({ start: session.started_at, end }, now)}`]];
  const turn = session.turn_span;
  if (turn) parts.push(["Time on the current prompt", (now) => `Turn ${spanText(turn, now)}`]);
  if (step) parts.push(["Time on the current step", (now) => `Step ${spanText(step, now)}`]);
  return parts;
}

// Time `time`'s node, which counts up.
export const timeNode = ([title, draw]: Time) => Object.assign(counts(document.createElement("span"), draw), { title });

// A check's time: how long it took, or while it runs, against its typical time ("3m of usually 7m").
export function checkTime(line: CheckLine, now: number): string {
  const took = duration((line.ended_at ?? now) - line.started_at);
  return line.ended_at === null && line.typical_ms !== null ? `${took} of usually ${duration(line.typical_ms)}` : took;
}

const phaseNames: Record<Phase, string> = {
  orienting: "Orienting",
  designing: "Designing",
  implementing: "Implementing",
  checking: "Checking",
  reviewing: "Reviewing",
  opening_pr: "Opening the PR",
  waiting_on_ci: "Waiting on CI",
  merged: "Merged",
};

// The phase and the estimate (#475), such as "Implementing · est. 16m–29m left", or
// "Implementing · depends on you" in place of a time while the session waits on Daniel; empty
// before the first phase.
export function estimateText(work: Pick<Work, "phases" | "estimate">): string {
  const phase = work.phases.at(-1)?.phase;
  if (phase === undefined) return "";
  if (phase === "merged") return phaseNames.merged;
  const range = work.estimate.range;
  const time = work.estimate.depends_on_you
    ? "depends on you"
    : range === null
      ? "no estimate"
      : `est. ${duration(range[0])}–${duration(range[1])} left`;
  return `${phaseNames[phase]} · ${time}`;
}

// The elements whose text counts up, each with how to draw it at a time.
const counting = new Map<HTMLElement, (now: number) => string>();

// Sets `node`'s text from `draw` now, and again each second while it is in the window.
// Each gets its own key, so `patch` swaps it in rather than keep an old node counting the old time.
let made = 0;
export function counts(node: HTMLElement, draw: (now: number) => string): HTMLElement {
  node.textContent = draw(Date.now());
  node.dataset.key = `count ${++made}`;
  counting.set(node, draw);
  return node;
}

if (typeof setInterval === "function" && typeof document === "object") {
  setInterval(() => {
    const now = Date.now();
    for (const [node, draw] of counting) {
      if (node.isConnected) node.textContent = draw(now);
      else counting.delete(node);
    }
  }, 1000);
}
