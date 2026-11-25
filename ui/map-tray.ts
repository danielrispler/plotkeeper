// The Map view's "Ready for review" tray (spec danielrispler/plotkeeper#334, tickets #336 and #339): the PRs of the project that are open and
// not drafts, which the server finds on a turn's end, the page's focus and the view's opening (`server/src/prs.rs`). A PR whose brief is
// stored has one row, or one row per chapter of a chaptered brief; a row opens that brief on the map (`map-brief.ts`).

import { text } from "./dom.js";
import { mapShown, onDraw, syncSide, type MapView } from "./map.js";
import { briefProblem, openBriefOf } from "./map-brief.js";
import { watchSeen } from "./seen.js";
import { logStop } from "./stop-rules.js";
import type { ReadyPr } from "./types.js";
import { briefsWritten } from "./writers.js";

// `full`: the row opens the full brief, past the head's "Since you read" story (#497).
export type TrayRow = { number: number; chapter: number | null; title: string; note: string; openable: boolean; full?: boolean };

// The tray's rows; none when nothing is ready, so the tray is not drawn. Without `claude` (`briefs` false) no brief is written,
// so a PR with none stored says briefs need Claude (spec #320). A brief at the PR's head says, in place of "brief ready", why it is
// what it is after a push (`note`, #477 part B): the read carried over, or the brief was rewritten.
export const trayRows = (ready: ReadyPr[], briefs = true): TrayRow[] =>
  ready.flatMap((pr): TrayRow[] => {
    const title = pr.title || `PR #${pr.number}`;
    const brief = pr.brief;
    if (brief?.quiet) return [{ number: pr.number, chapter: null, title, note: `#${pr.number} · no brief: nothing you track changed`, openable: false }];
    if (!brief || brief.chapters.length === 0) {
      if (!briefs) return [{ number: pr.number, chapter: null, title, note: `#${pr.number} · briefs need Claude`, openable: false }];
      // Being written: the row says so and waits. Otherwise opening the row writes the brief (the start writes only a few, newest first).
      if (brief?.writing) return [{ number: pr.number, chapter: null, title, note: `#${pr.number} · writing the brief…`, openable: false }];
      if (brief?.failed) return [{ number: pr.number, chapter: 0, title, note: `#${pr.number} · the brief could not be written; open it to try again`, openable: true }];
      return [{ number: pr.number, chapter: 0, title, note: `#${pr.number} · the brief is written when you open it`, openable: true }];
    }
    const state = brief.writing ? "writing it again…" : brief.failed ? "it could not be rewritten; open it to try again" : brief.stale ? "a newer push; rewritten when opened" : (brief.note ?? "brief ready");
    // A head with a "Since you read" story (#497): its rows come first, one per chapter of a chaptered story as for the full brief, so
    // each can be read to its end, and the full brief's rows open past them.
    const since = brief.since;
    const parts = brief.since_chapters ?? [];
    const story: TrayRow[] = !since
      ? []
      : parts.length > 1
        ? parts.map((chapter, i) => ({ number: pr.number, chapter: i, title: chapter, note: `#${pr.number} ${since} · since you read · ${i + 1} of ${parts.length}`, openable: true, full: false }))
        : [{ number: pr.number, chapter: 0, title: since, note: `#${pr.number} · since you read`, openable: true, full: false }];
    const full = since ? { full: true } : {};
    const of = since ? "full brief · " : "";
    if (brief.chapters.length === 1) return [...story, { number: pr.number, chapter: 0, title: brief.chapters[0] || title, note: `#${pr.number} · ${of}${state}`, openable: true, ...full }];
    return [
      ...story,
      ...brief.chapters.map((chapter, i) => ({
        number: pr.number,
        chapter: i,
        title: chapter,
        note: `#${pr.number} ${title} · ${of}${i + 1} of ${brief.chapters.length}${i === 0 ? ` · ${state}` : ""}`,
        openable: true,
        ...full,
      })),
    ];
  });

// The rows are built again on every draw; the watchers of the old ones stop with them. An offered brief is told once per PR
// (`logStop`), whichever of its rows is seen first.
let watching: (() => void)[] = [];

function paint(view: MapView) {
  watching.forEach((stop) => stop());
  watching = [];
  const rows = view.scrub ? [] : trayRows(view.state.ready ?? [], briefsWritten());
  view.slots.tray.replaceChildren(
    ...(rows.length
      ? [
          text("h3", "Ready for review", "side-title"),
          ...rows.map((row) => {
            const item = text(row.openable ? "button" : "div", "", "tray-row");
            item.dataset.pr = String(row.number);
            if (row.chapter !== null) item.dataset.chapter = String(row.chapter);
            const pr = (view.state.ready ?? []).find((each) => each.number === row.number);
            const problem = pr ? briefProblem(pr) : null;
            item.append(text("span", row.title, "tray-title"), text("span", problem ?? row.note, "tray-note"));
            if (row.openable) {
              item.setAttribute("type", "button");
              item.title = "Play this brief on the map";
              item.addEventListener("click", () => openBriefOf(view.project, row.number, row.chapter ?? 0, row.full ?? false));
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
