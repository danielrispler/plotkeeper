// What's new (#527, spec #520): after plotkeeper is updated to a new version, one toast says "Updated to X.Y.Z" with a "What's new"
// link to that version's GitHub Release, tagged `vX.Y.Z` (#512); the changelog's text stays there. The page keeps the last version it
// showed in `localStorage`: none stored (a first install) stores the current one and shows nothing, the same one shows nothing, and an
// older one than stored (a downgrade) stores the current one and shows nothing. A browser that keeps nothing shows nothing.

import { pageStore } from "./dom.js";

const KEY = "shown-version";

export type WhatsNew = { toast: boolean; store: boolean };

/**
 * Whether a load of version `current` shows the toast and stores `current`, from the `stored` version and whether the tab is visible:
 * the new version is stored only once its toast showed while the tab was visible.
 */
export function whatsNew(stored: string | null, current: string, visible: boolean): WhatsNew {
  if (stored === null) return { toast: false, store: true };
  if (stored === current) return { toast: false, store: false };
  if (!newer(current, stored)) return { toast: false, store: true };
  return { toast: true, store: visible };
}

// Whether version `a` is newer than `b`, X.Y.Z compared part by part as numbers.
function newer(a: string, b: string): boolean {
  const parts = (version: string) => version.split(".").map((part) => Number.parseInt(part, 10) || 0);
  const [x, y] = [parts(a), parts(b)];
  for (let at = 0; at < Math.max(x.length, y.length); at++) if ((x[at] ?? 0) !== (y[at] ?? 0)) return (x[at] ?? 0) > (y[at] ?? 0);
  return false;
}

// The page's storage, read and written in a try: a browser that refuses it (blocked site data) reads none and keeps nothing.
const kept = pageStore(KEY);

export const releaseUrl = (version: string) => `https://github.com/danielrispler/plotkeeper/releases/tag/v${version}`;

/**
 * At start, with the server's `version`: `show` draws the toast, which stays until dismissed. A tab hidden at the load stores the
 * version at its first visible moment, with the toast still there.
 */
export function startWhatsNew(version: string, show: (version: string, url: string) => void) {
  const visible = () => document.visibilityState === "visible";
  const store = () => kept.set(version);
  const decided = whatsNew(kept.get(), version, visible());
  if (decided.toast) show(version, releaseUrl(version));
  if (decided.store) return store();
  if (!decided.toast) return;
  const seen = () => {
    if (!visible()) return;
    store();
    document.removeEventListener("visibilitychange", seen);
  };
  document.addEventListener("visibilitychange", seen);
}
