// The window's width rule (#463 pick): the terminal keeps at least 100 columns. The session list
// collapses first, to its strip; below that, the story rail becomes an overlay. The widths are the
// columns' in style.css.

export type LayoutMode = "full" | "strip" | "overlay";

const list = 248;
const strip = 48;
const rail = 520;
// The terminal's horizontal padding, which holds no column.
const padding = 28;
const minColumns = 100;

// The terminal's columns at a width, for a cell of `cell` pixels.
const columns = (width: number, cell: number) => Math.floor((width - padding) / cell);

export function layoutMode(window: number, cell: number): LayoutMode {
  if (columns(window - list - rail, cell) >= minColumns) return "full";
  if (columns(window - strip - rail, cell) >= minColumns) return "strip";
  return "overlay";
}
