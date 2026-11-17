// The window's width rule (danielrispler/Falinks#565, decision 4). As the window narrows, the columns give way in order:
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

// A pane's box in the terminal's body, as insets from its edges in px.
export type Box = { top: number; right: number; bottom: number; left: number };
export type Split = { stacked: boolean; share: number };
// The margin around a pane, which is also half the gap that holds a split's divider.
const margin = 6;

// The panes' boxes in a terminal body of `width` × `height` px (danielrispler/Falinks#529): one pane fills it, else a split
// gives its first pane `share` of the body, side by side or stacked, the other pane the rest. A dragged
// share stops where a pane would get under 80 columns side by side, or under 120 px stacked.
export function paneBoxes(width: number, height: number, cell: number, split: Split | null): Box[] {
  const all = { top: margin, right: margin, bottom: margin, left: margin };
  if (!split) return [all];
  const length = split.stacked ? height : width;
  const least = split.stacked ? 120 : Math.ceil(narrow * cell) + 2 * margin;
  const at = Math.round(Math.max(least, Math.min(split.share * length, length - least)));
  const [start, end] = split.stacked ? (["top", "bottom"] as const) : (["left", "right"] as const);
  return [{ ...all, [end]: length - at + margin }, { ...all, [start]: at + margin }];
}

// The terminal panel's height (danielrispler/Falinks#542): a band under the terminal column (danielrispler/Falinks#565, decision 5), as its
// divider was dragged, at least 80 px but never over 40 % of the column. It costs height, never width.
export const panelHeight = (dragged: number, column: number) => Math.round(Math.min(Math.max(dragged, 80), 0.4 * column));

// Whether the story rail shows the selected session (danielrispler/Falinks#556): not in focus mode, not behind a closed
// overlay, and not under the skills browser, which takes its place.
export const railShown = (mode: LayoutMode, railOpen: boolean, skillsOpen: boolean) =>
  mode !== "focus" && (mode !== "overlay" || railOpen) && !skillsOpen;

// The top bar's items, in the order they survive a narrowing window (#127): the session's header and the needs-you chip never
// leave; the git line, "+" (new session) and the "⋯" menu come after. The menu takes, first to last, the new terminal, the
// usage, the new-build button, then "+" and the git line. An item shows whole or is in the menu.
export type BarItem = "header" | "needs-you" | "git" | "new-session" | "new-terminal" | "usage" | "new-build";
export type Bar = { shown: BarItem[]; menu: BarItem[] };
const leaves: BarItem[] = ["new-terminal", "usage", "new-build", "new-session", "git"];

// What shows and what is in the menu, in a bar of `room` px that has `items`, each with its width (an item the bar does not
// have now, such as the chip with nothing to say, is absent), `gap` between shown items and a "⋯" button of `more` px,
// drawn only when the menu holds an item.
export function arrangeBar(room: number, items: Partial<Record<BarItem, number>>, gap: number, more: number): Bar {
  const present = (Object.keys(items) as BarItem[]).filter((item) => items[item] !== undefined);
  const menu: BarItem[] = [];
  const width = () => {
    const shown = present.filter((item) => !menu.includes(item));
    const count = shown.length + (menu.length ? 1 : 0);
    return shown.reduce((sum, item) => sum + items[item]!, 0) + (menu.length ? more : 0) + gap * Math.max(0, count - 1);
  };
  for (const item of leaves) {
    if (width() <= room) break;
    if (present.includes(item)) menu.push(item);
  }
  return { shown: present.filter((item) => !menu.includes(item)), menu };
}
