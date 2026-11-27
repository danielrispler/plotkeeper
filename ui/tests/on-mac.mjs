// Loaded before every node test (`npm run test:node`): the page reads its platform from `navigator` (`ui/settings.ts`), and Node's
// says the host's, so on a Linux runner the tests that expect the macOS keys and glyphs would get Linux's. They run as on a Mac
// wherever they run; the Linux keymap's tests name their platform (#705).
Object.defineProperty(globalThis, "navigator", { value: { platform: "MacIntel" }, configurable: true, writable: true });
