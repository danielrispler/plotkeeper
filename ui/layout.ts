// The window's width rule (#565, decision 4). As the window narrows, the columns give way in order:
// the list to its strip, the rail from its width to 320 px, a terminal pane from 100 to 80 columns, a
// split from side by side to stacked once either pane would get under 100 columns, and last, the rail
// to an overlay. Focus mode (decision 2) folds the list and the rail, and the terminal takes the
// window. The widths are the columns' in style.css; the list's and the rail's are the defaults the
// dividers start from.

export type LayoutMode = "full" | "strip" | "overlay" | "focus";
export type Widths = { list: number; rail: number };
// The columns as shown: the list's and the rail's widths in px (0 when folded or an overlay), and
// whether a split stacks its panes.
export type Layout = { mode: LayoutMode; list: number; rail: number; stacked: boolean };

export const defaultWidths: Widths = { list: 260, rail: 380 };
const strip = 56;
// The narrowest a divider drags each column to; the rail's is also the width rule's.
const least: Widths = { list: 160, rail: 320 };
// The terminal's horizontal padding, which holds no column.
const padding = 28;
const wide = 100;
const narrow = 80;

// The terminal's columns at a width, for a cell of `cell` pixels.
const columns = (width: number, cell: number) => Math.floor((width - padding) / cell);

export function arrange(
  window: number,
  cell: number,
  widths: Widths = defaultWidths,
  { focus = false, split = false } = {},
): Layout {
  // Whether `panes` panes side by side in `width` each get `cols` columns.
  const fits = (width: number, cols: number, panes = 1) => columns(width / panes, cell) >= cols;
  if (focus) return { mode: "focus", list: 0, rail: 0, stacked: split && !fits(window, wide, 2) };
  // The list in full, else the strip with the rail narrowed as far as its least width.
  const give = (panes: number): Omit<Layout, "stacked"> | null => {
    if (fits(window - widths.list - widths.rail, wide, panes)) return { mode: "full", ...widths };
    // The widest rail that leaves the terminal its columns.
    let rail = widths.rail;
    while (rail > least.rail && !fits(window - strip - rail, wide, panes)) rail--;
    return fits(window - strip - rail, wide, panes) ? { mode: "strip", list: strip, rail } : null;
  };
  const side = split ? give(2) : null;
  if (side) return { ...side, stacked: false };
  const one = give(1) ?? (fits(window - strip - least.rail, narrow) ? { mode: "strip" as const, list: strip, rail: least.rail } : null);
  return { ...(one ?? { mode: "overlay", list: strip, rail: 0 }), stacked: split };
}

// A column's divider dragged to `width`: no narrower than its least, and no wider than keeps the
// terminal its 100 columns beside the other column as `mode` shows it.
export function resized(column: keyof Widths, width: number, widths: Widths, mode: LayoutMode, window: number, cell: number): Widths {
  const other = column === "list" ? widths.rail : mode === "full" ? widths.list : strip;
  let most = Math.floor(window - other - padding - wide * cell);
  while (columns(window - other - most, cell) < wide) most--; // float error at the edge
  return { ...widths, [column]: Math.max(least[column], Math.min(Math.round(width), most)) };
}
