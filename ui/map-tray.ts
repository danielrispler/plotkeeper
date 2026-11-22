// The Map view's "Ready for review" tray (spec danielrispler/plotkeeper#334, tickets #336 and #339): the PRs of the project that are open and
// not drafts, which the shell finds on a turn's end, the window's focus and the view's opening (`src-tauri/src/prs.rs`). A PR whose brief is
// stored has one row, or one row per chapter of a chaptered brief; a row opens that brief on the map (`map-brief.ts`).

import { text } from "./dom.js";
import { onDraw, syncSide, type MapView } from "./map.js";
import { briefProblem, openBriefOf } from "./map-brief.js";
import type { ReadyPr } from "./types.js";

export type TrayRow = { number: number; chapter: number | null; title: string; note: string; openable: boolean };

// The tray's rows; none when nothing is ready, so the tray is not drawn.
export const trayRows = (ready: ReadyPr[]): TrayRow[] =>
  ready.flatMap((pr): TrayRow[] => {
    const title = pr.title || `PR #${pr.number}`;
    const brief = pr.brief;
    if (!brief || brief.chapters.length === 0) {
      const note = brief?.writing ? "writing the brief…" : "the brief is not written yet";
      return [{ number: pr.number, chapter: null, title, note: `#${pr.number} · ${note}`, openable: false }];
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

function paint(view: MapView) {
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
            }
            return item;
          }),
        ]
      : []),
  );
  syncSide(view.slots);
}

onDraw(paint);
