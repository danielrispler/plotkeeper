# src-tauri: archived, unmaintained

This was Studio's Tauri 2 desktop app: the native window, the terminals, notifications, the Dock badge and the command handlers the page called through Tauri.

It is archived (danielrispler/plotkeeper#332, spec #319 "The archive"). It is out of the Cargo workspace, CI and `npm run check`, and it no longer builds: spec #334 (PR #349) retired the functions its commands forwarded to. The code is kept as it was for reference; nothing here is maintained.

The learning layer now runs as `plotkeeper`, a local web page served by `server/` (`npm run start`).
