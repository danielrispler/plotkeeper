// The overlays' hover (danielrispler/plotkeeper#128): the rail's layer opens when the pointer rests on a target, the list's strip
// the same way. A state machine with an injected clock, so its timing is tested without a pointer (`tests/hover.test.mjs`).
//
// Rest on a target for `open` ms and it opens; leave the targets and the layer and it closes after `close` ms. A mouse button
// held down never opens it, so a drag across the edge or a text selection does not. A click pins it open; Esc and a click
// outside close it.

export type Clock = { set(run: () => void, ms: number): number; clear(timer: number): void };
export const realClock: Clock = { set: (run, ms) => window.setTimeout(run, ms), clear: (timer) => window.clearTimeout(timer) };

const openAfter = 300;
const closeAfter = 400;
// A peek marks its turn seen only once the rail has stayed open this long.
const seenAfter = 2000;

export type Hover = {
  /** The pointer came onto / left a zone: a target or the layer. */
  enter(zone: string): void;
  leave(zone: string): void;
  /** A mouse button went down or up anywhere. */
  pressed(down: boolean): void;
  /** A click on a target: pins it open, or closes it if pinned already. */
  click(): void;
  /** A key's toggle: opens it pinned, or closes it. */
  toggle(): void;
  /** Esc, or a click outside the targets and the layer. */
  dismiss(): void;
  readonly open: boolean;
  readonly pinned: boolean;
};

export function hover(clock: Clock, change: (open: boolean) => void): Hover {
  const inside = new Set<string>();
  let down = false;
  let open = false;
  let pinned = false;
  let timer: number | null = null;
  const cancel = () => {
    if (timer !== null) clock.clear(timer);
    timer = null;
  };
  const set = (value: boolean) => {
    cancel();
    if (value !== open) {
      open = value;
      change(open);
    }
    if (!open) pinned = false;
  };
  // Resting inside while closed starts the opening; being outside while open and not pinned starts the closing.
  const settle = () => {
    cancel();
    if (!open && inside.size && !down) timer = clock.set(() => set(true), openAfter);
    else if (open && !pinned && !inside.size) timer = clock.set(() => set(false), closeAfter);
  };
  return {
    enter(zone) {
      inside.add(zone);
      settle();
    },
    leave(zone) {
      inside.delete(zone);
      settle();
    },
    pressed(value) {
      down = value;
      settle();
    },
    click() {
      if (open && pinned) return set(false);
      pinned = true;
      set(true);
    },
    toggle() {
      if (open) return set(false);
      pinned = true;
      set(true);
    },
    dismiss() {
      set(false);
    },
    get open() {
      return open;
    },
    get pinned() {
      return pinned;
    },
  };
}

/**
 * Tells the server whether the rail shows its session. A rail that shows as an overlay (`delayed`) reports so only after it has
 * stayed `seenAfter` ms; closing, and any other layout, report at once. Each change is reported once.
 */
export function shownGate(clock: Clock, report: (shown: boolean) => void) {
  let last: boolean | null = null;
  let timer: number | null = null;
  const send = (shown: boolean) => {
    if (shown !== last) report(shown);
    last = shown;
  };
  return (shown: boolean, delayed: boolean) => {
    if (timer !== null) clock.clear(timer);
    timer = null;
    if (shown && delayed && last !== true) timer = clock.set(() => send(true), seenAfter);
    else if (!(shown && delayed && last === true)) send(shown);
  };
}
