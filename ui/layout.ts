// The window's width rule (danielrispler/Falinks#565, decision 4). As the window narrows, the columns give way in order:
// the list to its strip, the rail from its width to 320 px, the centre column from 100 to 80 columns, and last, the rail
// to an overlay. Focus mode (decision 2) folds the list and the rail, and the centre column takes the
// window. The widths are the columns' in style.css; the list's and the rail's are the defaults the
// dividers start from.

export type LayoutMode = "full" | "strip" | "overlay" | "focus";
export type Widths = { list: number; rail: number };
// The columns as shown: the list's and the rail's widths in px (0 when folded or an overlay).
export type Layout = { mode: LayoutMode; list: number; rail: number };

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

export function arrange(window: number, cell: number, widths: Widths = defaultWidths, { focus = false } = {}): Layout {
  // Whether the centre column in `width` gets `cols` columns.
  const fits = (width: number, cols: number) => columns(width, cell) >= cols;
  if (focus) return { mode: "focus", list: 0, rail: 0 };
  // The list in full, else the strip with the rail narrowed as far as its least width.
  if (fits(window - widths.list - widths.rail, wide)) return { mode: "full", ...widths };
  // The widest rail that leaves the column its columns.
  let rail = widths.rail;
  while (rail > least.rail && !fits(window - strip - rail, wide)) rail--;
  if (fits(window - strip - rail, wide)) return { mode: "strip", list: strip, rail };
  return fits(window - strip - least.rail, narrow) ? { mode: "strip", list: strip, rail: least.rail } : { mode: "overlay", list: strip, rail: 0 };
}

// A column's divider dragged to `width`: no narrower than its least, and no wider than keeps the
// terminal its 100 columns beside the other column as `mode` shows it.
export function resized(column: keyof Widths, width: number, widths: Widths, mode: LayoutMode, window: number, cell: number): Widths {
  const other = column === "list" ? widths.rail : mode === "full" ? widths.list : strip;
  let most = Math.floor(window - other - padding - wide * cell);
  while (columns(window - other - most, cell) < wide) most--; // float error at the edge
  return { ...widths, [column]: Math.max(least[column], Math.min(Math.round(width), most)) };
}

// Whether the story rail shows the selected session (danielrispler/Falinks#556): not in focus mode, not behind a closed
// overlay, and not behind the Map view, which hides it with `visibility` (so the rail still intersects the viewport). Over the map, the story
// shows only in the left panel (#385), while `inPanel`.
export const railShown = (mode: LayoutMode, railOpen: boolean, mapOpen: boolean, inPanel = false) =>
  mode !== "focus" && (mapOpen ? inPanel : mode !== "overlay" || railOpen);

// Whether Home's content is seen: Home is the open pane and the Map view, which hides it with `visibility`, is not over it.
export const homeSeen = (homeOpen: boolean, mapOpen: boolean) => homeOpen && !mapOpen;
