// The window's width rule (#463 pick): the terminal keeps at least 100 columns. The session list
// collapses first, to its strip; below that, the story rail becomes an overlay. The widths are the
// columns' in style.css; the list's and the rail's are the defaults the dividers start from.

export type LayoutMode = "full" | "strip" | "overlay";
export type Widths = { list: number; rail: number };

export const defaultWidths: Widths = { list: 248, rail: 520 };
const strip = 48;
// The narrowest a divider drags each column to.
const least: Widths = { list: 160, rail: 320 };
// The terminal's horizontal padding, which holds no column.
const padding = 28;
const minColumns = 100;

// The terminal's columns at a width, for a cell of `cell` pixels.
const columns = (width: number, cell: number) => Math.floor((width - padding) / cell);

export function layoutMode(window: number, cell: number, widths: Widths = defaultWidths): LayoutMode {
  if (columns(window - widths.list - widths.rail, cell) >= minColumns) return "full";
  if (columns(window - strip - widths.rail, cell) >= minColumns) return "strip";
  return "overlay";
}

// A column's divider dragged to `width`: no narrower than its least, and no wider than keeps the
// terminal its 100 columns beside the other column as `mode` shows it.
export function resized(column: keyof Widths, width: number, widths: Widths, mode: LayoutMode, window: number, cell: number): Widths {
  const other = column === "list" ? widths.rail : mode === "full" ? widths.list : strip;
  const most = window - other - padding - minColumns * cell;
  return { ...widths, [column]: Math.round(Math.max(least[column], Math.min(width, most))) };
}
