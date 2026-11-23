// The page's frame (spec #380, ticket #386): Treatment C's top bar, the map filling the rest, and the session list at the right. Below
// 1007 px the list gives way to its strip (`narrowBelow`, the width under which the map keeps one overlay open). Focus mode
// (danielrispler/Falinks#573) folds the list, and the map takes the window. The widths are style.css's too.

import { narrowBelow } from "./map-model.js";

export type LayoutMode = "full" | "strip" | "focus";
// The list as shown: its width in px (0 when folded).
export type Layout = { mode: LayoutMode; list: number };

const list = 252;
const strip = 56;

export function arrange(window: number, { focus = false } = {}): Layout {
  if (focus) return { mode: "focus", list: 0 };
  return window >= narrowBelow ? { mode: "full", list } : { mode: "strip", list: strip };
}

// Whether Home's content is seen: Home is the open pane and the Map view, which hides it with `visibility`, is not over it.
export const homeSeen = (homeOpen: boolean, mapOpen: boolean) => homeOpen && !mapOpen;
