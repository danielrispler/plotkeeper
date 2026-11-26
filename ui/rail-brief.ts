// The rail's quiet row "PR brief ready" (spec danielrispler/plotkeeper#334, ticket #344): in the newest turn of the live session whose PR a brief
// waits for. Pressing it opens the Map view on that brief. The same live session's row in the session list carries a quiet mark in the map's
// mist (#630), "PR #N · brief waiting", which opens it too. A PR with no live studio session has neither (it is in the Map view's tray and
// lights its project header's dot). The server says which PRs have a brief waiting (`briefs_waiting`), asked when a session's PR number is first
// seen and again when the Map view is told a brief changed (`map-landed`), never on a timer; the rail and the list share the answer.

import { failure, hint, invoke, keyed, listen, pressable, text } from "./dom.js";
import { openMap } from "./map.js";
import { openBriefWhenDrawn } from "./map-brief.js";
import type { Session, WaitingBriefs } from "./types.js";

/** The PR of `session` that a brief waits for, or null: it needs a live session, a PR, and a brief waiting for that PR. */
function waitingPr(session: Pick<Session, "identity" | "state">, waiting: WaitingBriefs | null): number | null {
  const pr = session.identity.pr;
  return pr === null || session.state === "ended" || !waiting?.numbers.includes(pr) ? null : pr;
}

/** The row for `session`, or null when no brief waits for its PR. */
export function briefRowFor(
  session: Pick<Session, "identity" | "state">,
  waiting: WaitingBriefs | null,
  open: (project: string, pr: number) => void,
): HTMLElement | null {
  const pr = waitingPr(session, waiting);
  if (pr === null || !waiting) return null;
  const row = pressable(text("button", "PR brief ready", "btn quiet brief-ready"), null);
  hint(row, `Open the brief of #${pr} on the map`);
  row.addEventListener("click", () => open(waiting.project, pr));
  return row;
}

/**
 * The session row's mark (#630), or null when no brief waits for its PR: a button beside the row, as a button holds no button, drawn in the map's
 * mist with no colour, keyed by its session and PR so a kept node's listener opens the right brief.
 */
export function briefMarkFor(
  session: Pick<Session, "id" | "identity" | "state">,
  waiting: WaitingBriefs | null,
  open: (project: string, pr: number) => void,
): HTMLElement | null {
  const pr = waitingPr(session, waiting);
  if (pr === null || !waiting) return null;
  const button = keyed(pressable(text("button", "", "brief-mark"), null, () => open(waiting.project, pr)), `${session.id} brief ${pr}`);
  button.setAttribute("aria-label", `Open the brief of PR #${pr}`);
  const swatch = text("span", "", "mist-swatch");
  swatch.setAttribute("aria-hidden", "true");
  button.append(swatch);
  return hint(button, `PR #${pr} · brief waiting`, "start");
}

export function openBriefOnMap(project: string, pr: number) {
  openMap(project);
  openBriefWhenDrawn(project, pr);
}

// What the server last said, by the session's project folder, what was asked of it (a project with its PR number), and the views that read it, the rail's
// feed and the session list, each drawn again when an answer changes what shows.
const known = new Map<string, WaitingBriefs | null>();
const asked = new Set<string>();
const readers = new Set<() => void>();

/** What the server last said for `session`'s project; asks it once per PR number, and `redraw` runs when the answer changes. */
export function waitingOf(session: Session, redraw: () => void): WaitingBriefs | null {
  readers.add(redraw);
  const pr = session.identity.pr;
  if (pr === null) return null;
  const { project } = session;
  const key = `${project}#${pr}`;
  if (!asked.has(key)) {
    asked.add(key);
    invoke("briefs_waiting", { project })
      .then((now) => {
        if (JSON.stringify(known.get(project) ?? null) !== JSON.stringify(now)) {
          known.set(project, now);
          for (const draw of readers) draw();
        }
      })
      .catch(failure("briefs_waiting"));
  }
  return known.get(project) ?? null;
}

/** The newest turn's row of `session`. */
export function railBriefRow(session: Session, redraw: () => void): HTMLElement | null {
  return briefRowFor(session, waitingOf(session, redraw), openBriefOnMap);
}

/** A brief was written or changed: ask again at the next draw of each view that read it. */
export function startRailBriefs() {
  listen<string>("map-landed", () => {
    asked.clear();
    for (const draw of readers) draw();
  });
}
