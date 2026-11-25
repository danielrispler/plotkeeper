# Changelog

All notable changes to plotkeeper's npm package. Newest first; each version's section is the notes of its GitHub Release.

## 0.1.2

### Added

- The npm page shows plotkeeper's README and the package ships its LICENSE, so you can read what it does before installing. (#507)

### Changed

- The map's "not placed" list is one line, "N not placed", that opens on a click, so agents joining and leaving no longer move an open part sheet. (#505)

## 0.1.1

### Changed

- A plain wheel or two-finger scroll over the map zooms around the pointer, with no ⌘ needed. (#470)
- The list of agents not placed on the map leads the side column, so an open part sheet never pushes it down over a part. (#468)

### Fixed

- Status lines of background runs, ratify and micro-worlds are announced once by screen readers, not again at every redraw. (#468)

## 0.1.0

### Added

- First release on npm: `npx plotkeeper` lists the agents it found, asks once before wiring their hooks and opens the page. (#442)
- The page records Claude Code, Codex, agy and Cursor sessions, writes a Headline for each turn and a brief for each ready PR, on your own `claude -p` subscription. (#376)
- The page is keyboard-reachable, with named controls, readable contrast and confirmations before destructive actions. (#464)
