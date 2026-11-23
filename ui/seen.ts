// **Seen** (danielrispler/plotkeeper#306): an element is seen when it is in the viewport, not merely built in the DOM, in a visible
// and focused window, with the view that holds it open. One helper for Home's marks and the map's events.
//
// In the viewport means covering at least half of the element or at least half of the viewport (so a card taller than twice the
// viewport can be seen), for `seenAfter` ms without a break.

export const seenAfter = 500;

// How much of the element, and how much of the viewport, the element covers; both 0 to 1.
export type Overlap = { element: number; viewport: number };
export const covers = ({ element, viewport }: Overlap) => element >= 0.5 || viewport >= 0.5;

// The caller's side: `active` says whether the view holding the element is open (`railShown`, `homeShown`, or the map's own);
// `onSeen` runs once, when the element has been seen. With `interaction`, Daniel must also have pointed at, pressed, scrolled or
// focused the element (#331), so a card in view on a second screen is not seen while he works elsewhere.
export type SeenOptions = { active: () => boolean; onSeen: () => void; interaction?: boolean };

const watchers = new Set<() => void>();

// The window turned visible or focused, or a view opened or closed: every watcher looks again at its conditions.
export const recheckSeen = () => watchers.forEach((check) => check());

// Watches `node` until it is seen or the returned function stops it. Never fires twice.
const interactions = ["pointermove", "pointerdown", "keydown", "wheel", "touchstart", "focusin"];

export function watchSeen(node: Element, { active, onSeen, interaction = false }: SeenOptions): () => void {
  let overlap: Overlap = { element: 0, viewport: 0 };
  let timer: ReturnType<typeof setTimeout> | null = null;
  let done = false;
  let touched = !interaction;
  const holds = () => covers(overlap) && touched && document.visibilityState === "visible" && document.hasFocus() && active();
  const stop = () => {
    done = true;
    if (timer !== null) clearTimeout(timer);
    timer = null;
    watchers.delete(check);
    observer?.disconnect();
    if (interaction) for (const type of interactions) node.removeEventListener(type, touch);
  };
  const touch = () => {
    touched = true;
    check();
  };
  const check = () => {
    if (done) return;
    const ok = holds();
    if (ok && timer === null) {
      timer = setTimeout(() => {
        timer = null;
        if (!holds()) return check();
        stop();
        onSeen();
      }, seenAfter);
    } else if (!ok && timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  };
  // Thresholds every 5%, so the half of a tall element's viewport is reported as it is crossed.
  const observer =
    typeof IntersectionObserver === "undefined"
      ? null
      : new IntersectionObserver(
          (entries) => {
            const entry = entries.at(-1)!;
            const area = (r: { width: number; height: number }) => r.width * r.height;
            const root = entry.rootBounds ?? { width: window.innerWidth, height: window.innerHeight };
            overlap = { element: entry.intersectionRatio, viewport: area(root) ? area(entry.intersectionRect) / area(root) : 0 };
            check();
          },
          { threshold: Array.from({ length: 21 }, (_, i) => i / 20) },
        );
  if (interaction) for (const type of interactions) node.addEventListener(type, touch, { passive: true });
  observer?.observe(node);
  watchers.add(check);
  return stop;
}
