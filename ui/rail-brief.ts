// The rail's quiet row "PR brief ready" (spec danielrispler/plotkeeper#334, ticket #344): in the newest turn of the live session whose PR a brief
// waits for. Pressing it opens the Map view on that brief. A PR with no live studio session has no row (it is in the Map view's tray and
// lights its project header's dot). The shell says which PRs have a brief waiting (`briefs_waiting`), asked when a session's PR number is first seen and
// again when the Map view is told a brief changed (`map-landed`), never on a timer.

import { failure, invoke, listen, text } from "./dom.js";
import { openMap } from "./map.js";
import { openBriefWhenDrawn } from "./map-brief.js";
import type { Session, WaitingBriefs } from "./types.js";

// The row for `session`, or null: it needs a live session, a PR, and a brief waiting for that PR.
export function briefRowFor(
  session: Pick<Session, "identity" | "state">,
  waiting: WaitingBriefs | null,
  open: (project: string, pr: number) => void,
): HTMLElement | null {
  const pr = session.identity.pr;
  if (pr === null || session.state === "ended" || !waiting?.numbers.includes(pr)) return null;
  const row = text("button", "PR brief ready", "btn quiet brief-ready");
  row.setAttribute("type", "button");
  row.title = `Open the brief of #${pr} on the map`;
  row.addEventListener("click", () => open(waiting.project, pr));
  return row;
}

function openBriefOnMap(project: string, pr: number) {
  openMap(project);
  openBriefWhenDrawn(project, pr);
}

// What the shell last said, by the session's folder, and what was asked of it (a folder with its PR number).
const known = new Map<string, WaitingBriefs | null>();
const asked = new Set<string>();

// The newest turn's row of `session`; asks the shell once per PR number, and `redraw` runs when the answer changes what shows.
export function railBriefRow(session: Session, redraw: () => void): HTMLElement | null {
  const pr = session.identity.pr;
  if (pr === null) return null;
  const key = `${session.cwd}#${pr}`;
  if (!asked.has(key)) {
    asked.add(key);
    invoke("briefs_waiting", { cwd: session.cwd })
      .then((now) => {
        if (JSON.stringify(known.get(session.cwd) ?? null) !== JSON.stringify(now)) {
          known.set(session.cwd, now);
          redraw();
        }
      })
      .catch(failure("briefs_waiting"));
  }
  return briefRowFor(session, known.get(session.cwd) ?? null, openBriefOnMap);
}

// A brief was written or changed: ask again at the next draw.
export function startRailBriefs(redraw: () => void) {
  listen<string>("map-landed", () => {
    asked.clear();
    redraw();
  });
}
