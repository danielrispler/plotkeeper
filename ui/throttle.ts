// A place's git reads are throttled (#26, Freshness, local state): the first event reads at once,
// then at most one read per `ms` while events keep coming, and always one read after the last event.
// `clock` and `schedule` are injected, so a test moves time.

export function throttle(
  read: () => void,
  ms: number,
  clock: () => number,
  schedule: (run: () => void, delay: number) => unknown,
): () => void {
  let last = -Infinity;
  let waiting = false;
  const readNow = () => {
    waiting = false;
    last = clock();
    read();
  };
  return () => {
    // A read is already due after this event.
    if (waiting) return;
    const wait = last + ms - clock();
    if (wait <= 0) return readNow();
    waiting = true;
    schedule(readNow, wait);
  };
}
