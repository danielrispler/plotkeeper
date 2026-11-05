// The plan's usage in the top bar (#481): the `rate_limits` Claude Code gives the status line, from
// the newest snapshot the status line saved. Each window has `used_percentage` (0 to 100) and
// `resets_at` (Unix seconds); Claude Code drops a window once it resets, and so does this.

type Window = { used_percentage: number; resets_at: number };
export type Limits = { five_hour?: Window; seven_day?: Window };

export function usageText(limits: Limits | null, now: number): string {
  const shown = (
    [
      ["5h", limits?.five_hour],
      ["week", limits?.seven_day],
    ] as const
  )
    .filter(([, w]) => w && w.resets_at > now)
    .map(([name, w]) => `${name} ${Math.floor(w!.used_percentage)}%`);
  return shown.length ? shown.join(" · ") : "no usage";
}
