// The page `npm run snap` renders: dist's index.html with a stub Tauri API in front of the app's
// module script. The stub answers each command from the fixture's key of that name, else null.
// It exists only in the page written to a temp directory, never in dist or the shipped app.

export const stubSource = (fixture) => `window.__TAURI__ = (() => {
  const fixture = ${JSON.stringify(fixture)};
  return { event: { listen: async () => () => {} }, core: { invoke: async (command) => fixture[command] ?? null } };
})();`;

export const snapPage = (index, fixture) =>
  index.replace('<script type="module"', () => `<script>${stubSource(fixture)}</script>\n  <script type="module"`);
