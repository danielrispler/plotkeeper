// The page `npm run snap` renders: dist's index.html with a stub Tauri API in front of the app's
// module script. The stub answers each command from the fixture's key of that name, else null.
// It exists only in the page written to a temp directory, never in dist or the shipped app.

export const stubSource = (fixture) => `window.__TAURI__ = (() => {
  const fixture = ${JSON.stringify(fixture)};
  const listeners = {};
  const listen = async (name, handler) => ((listeners[name] ??= []).push(handler), () => {});
  // The daemon answers a list request with an empty list, which the terminals' revival waits for.
  const invoke = async (command, args) => {
    if (command === "terminal_send" && args?.msg?.op === "list") setTimeout(() => (listeners.terminal ?? []).forEach((h) => h({ payload: { ev: "list", terms: [] } })));
    return fixture[command] ?? null;
  };
  return { event: { listen }, core: { invoke } };
})();`;

export const snapPage = (index, fixture) =>
  index.replace('<script type="module"', () => `<script>${stubSource(fixture)}</script>\n  <script type="module"`);
