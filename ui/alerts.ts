// Notices (danielrispler/plotkeeper#331): the tab title's unseen count and the browser notification. They replace the Dock badge,
// the macOS notifications and the sounds.

// The tab is the one Daniel is looking at: visible and focused, the same two conditions that decide Seen (`seen.ts`).
const looking = () => document.visibilityState === "visible" && document.hasFocus();

// The title carries the number of unseen sessions, as the bar's unseen chip counts them.
export const titleFor = (unseen: number, base = "plotkeeper"): string => (unseen > 0 ? `(${unseen}) ${base}` : base);

export function showTitle(unseen: number) {
  document.title = titleFor(unseen);
}

export type Alert = { title: string; body: string };

// A browser notification, only while no tab is visible and focused (the server also sends it only then). Returns whether it fired.
export function notifyIfAway({ title, body }: Alert): boolean {
  if (looking() || typeof Notification === "undefined" || Notification.permission !== "granted") return false;
  // One tag, so a second tab's copy replaces this one instead of stacking.
  new Notification(title, { body, tag: "plotkeeper" });
  return true;
}

// Asks for the permission once from the page: the browser wants a gesture, so this runs on Daniel's first press, and a flag in the
// page's storage keeps a dismissed prompt from coming back.
export async function askOnce(): Promise<void> {
  if (typeof Notification === "undefined" || Notification.permission !== "default") return;
  try {
    if (localStorage.getItem("notifications-asked")) return;
    localStorage.setItem("notifications-asked", "1");
  } catch {}
  await Notification.requestPermission();
}
