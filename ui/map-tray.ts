// The Map view's "Ready for review" tray (spec danielrispler/plotkeeper#334, ticket #336): the PRs of the project that are open and not
// drafts, which the shell finds on a turn's end, the window's focus and the view's opening (`src-tauri/src/prs.rs`). A row says the brief
// is not written yet until the PR brief view exists.

import { text } from "./dom.js";
import { onDraw, syncSide, type MapView } from "./map.js";
import type { ReadyPr } from "./types.js";

export type TrayRow = { number: number; title: string; note: string };

// The tray's rows, one per ready PR; none when nothing is ready, so the tray is not drawn.
export const trayRows = (ready: ReadyPr[]): TrayRow[] =>
  ready.map((pr) => ({ number: pr.number, title: pr.title || `PR #${pr.number}`, note: `#${pr.number} · the brief is not written yet` }));

function paint(view: MapView) {
  const rows = view.scrub ? [] : trayRows(view.state.ready ?? []);
  view.slots.tray.replaceChildren(
    ...(rows.length
      ? [
          text("h3", "Ready for review", "side-title"),
          ...rows.map((row) => {
            const item = text("div", "", "tray-row");
            item.dataset.pr = String(row.number);
            item.append(text("span", row.title, "tray-title"), text("span", row.note, "tray-note"));
            return item;
          }),
        ]
      : []),
  );
  syncSide(view.slots);
}

onDraw(paint);
