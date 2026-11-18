// The top bar's fit (danielrispler/plotkeeper#127): each item shows whole or is in the "⋯" menu; `arrangeBar` (layout.ts) says
// which. The items are the same elements either way, moved between the bar and the menu, so every one keeps working.
// The bar is measured with every item in it, so an item's width never depends on where it sat last.

import { element } from "./dom.js";
import { arrangeBar, type BarItem } from "./layout.js";

// In the bar's order, left to right.
const order: BarItem[] = ["header", "needs-you", "git", "usage", "new-build", "new-session", "gear"];
// The items left of the spacer; the rest sit right of it.
const left: BarItem[] = ["header", "needs-you", "git"];
const ids: Record<BarItem, string> = {
  header: "session-header",
  "needs-you": "needs-you",
  git: "git-line",
  usage: "usage",
  "new-build": "new-build",
  "new-session": "new-session",
  gear: "settings-gear",
};

export function placeBar() {
  const top = document.querySelector<HTMLElement>(".top")!;
  const menu = element("bar-menu");
  const more = element("bar-more");
  // Every item back in the bar, in order and on its side of the spacer, to measure it whole.
  const spacer = top.querySelector(".sp")!;
  // The gear is last, right of the "⋯" button; the menu's popover follows it and takes no room.
  for (const item of order) top.insertBefore(element(ids[item]), left.includes(item) ? spacer : item === "gear" ? menu : more);
  more.hidden = false;
  const style = getComputedStyle(top);
  const gap = parseFloat(style.columnGap) || 0;
  // The spacer takes a gap on each side.
  const room = top.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight) - 2 * gap;
  const widths: Partial<Record<BarItem, number>> = {};
  for (const item of order) if (!element(ids[item]).hidden) widths[item] = element(ids[item]).offsetWidth;
  const { menu: moved } = arrangeBar(room, widths, gap, more.offsetWidth);
  more.hidden = moved.length === 0;
  for (const item of order) if (moved.includes(item)) menu.append(element(ids[item]));
  if (!moved.length) menu.hidePopover?.();
}

// Fits the bar again once the items change: a name, a chip, the git tokens, the usage. The placing itself moves items, so the
// watch stops while it runs.
export function watchBar() {
  const top = document.querySelector<HTMLElement>(".top")!;
  const watched = [top, element("bar-menu")];
  const options = { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["hidden"] };
  let queued = false;
  const watch = new MutationObserver(() => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      watch.disconnect();
      placeBar();
      watch.takeRecords();
      for (const node of watched) watch.observe(node, options);
    });
  });
  for (const node of watched) watch.observe(node, options);
  placeBar();
}
