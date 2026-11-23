// The Map view's "Ready for review" tray (spec danielrispler/plotkeeper#334, tickets #336 and #339): the PRs of the project that are open and
// not drafts, which the server finds on a turn's end, the page's focus and the view's opening (`server/src/prs.rs`). A PR whose brief is
// stored has one row, or one row per chapter of a chaptered brief; a row opens that brief on the map (`map-brief.ts`).

import { text } from "./dom.js";
import { mapShown, onDraw, syncSide, type MapView } from "./map.js";
import { briefProblem, openBriefOf } from "./map-brief.js";
import { watchSeen } from "./seen.js";
import { logStop } from "./stop-rules.js";
import type { ReadyPr } from "./types.js";

export type TrayRow = { number: number; chapter: number | null; title: string; note: string; openable: boolean };

// The tray's rows; none when nothing is ready, so the tray is not drawn.
export const trayRows = (ready: ReadyPr[]): TrayRow[] =>
  ready.flatMap((pr): TrayRow[] => {
    const title = pr.title || `PR #${pr.number}`;
    const brief = pr.brief;
    if (brief?.quiet) return [{ number: pr.number, chapter: null, title, note: `#${pr.number} · no brief: nothing you track changed`, openable: false }];
    if (!brief || brief.chapters.length === 0) {
      // Being written: the row says so and waits. Otherwise opening the row writes the brief (the start writes only a few, newest first).
      if (brief?.writing) return [{ number: pr.number, chapter: null, title, note: `#${pr.number} · writing the brief…`, openable: false }];
      return [{ number: pr.number, chapter: 0, title, note: `#${pr.number} · the brief is written when you open it`, openable: true }];
    }
    const state = brief.writing ? "writing it again…" : brief.stale ? "a newer push; rewritten when opened" : "brief ready";
    if (brief.chapters.length === 1) return [{ number: pr.number, chapter: 0, title: brief.chapters[0] || title, note: `#${pr.number} · ${state}`, openable: true }];
    return brief.chapters.map((chapter, i) => ({
      number: pr.number,
      chapter: i,
      title: chapter,
      note: `#${pr.number} ${title} · ${i + 1} of ${brief.chapters.length}${i === 0 ? ` · ${state}` : ""}`,
      openable: true,
    }));
  });

// The rows are built again on every draw; the watchers of the old ones stop with them. An offered brief is told once per PR
// (`logStop`), whichever of its rows is seen first.
let watching: (() => void)[] = [];

function paint(view: MapView) {
  watching.forEach((stop) => stop());
  watching = [];
  const rows = view.scrub ? [] : trayRows(view.state.ready ?? []);
  view.slots.tray.replaceChildren(
    ...(rows.length
      ? [
          text("h3", "Ready for review", "side-title"),
          ...rows.map((row) => {
            const item = text(row.openable ? "button" : "div", "", "tray-row");
            item.dataset.pr = String(row.number);
            if (row.chapter !== null) item.dataset.chapter = String(row.chapter);
            const problem = briefProblem(row.number);
            item.append(text("span", row.title, "tray-title"), text("span", problem ?? row.note, "tray-note"));
            if (row.openable) {
              item.setAttribute("type", "button");
              item.title = "Play this brief on the map";
              item.addEventListener("click", () => openBriefOf(view.project, row.number, row.chapter ?? 0));
              watching.push(watchSeen(item, { active: mapShown, onSeen: () => logStop("brief-offered", view.project, row.number) }));
            }
            return item;
          }),
        ]
      : []),
  );
  syncSide(view.slots);
}

onDraw(paint);
