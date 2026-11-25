# Changelog

All notable changes to plotkeeper's npm package. Newest first; each version's section is the notes of its GitHub Release.

## 0.2.0

### Added

- After a new push to a PR you've read, its brief shows only what changed since your read ("Since you read"), or keeps your read when the push touched nothing on your map, with the full brief one click away. (#518)
- A PR's brief says when its facts are partial (a cut diff, a failed claim check, a language with no import reader) and when `main` has moved since the PR's base, so you know what it couldn't see. (#518)
- Each brief step has "See the code" for that step's parts and a copy icon on "Could break" that writes a ready prompt, and a part's fog, stale or broken mark says why and from which PR. (#518)

### Changed

- The brief's "Also touched" step groups files by kind (no part, tests, config and build, docs, ignored, other), so the leftovers are easier to scan. (#518)
- A part's sheet leads with a short purpose, then what it holds with its key files, then who it talks to and why, and the map draws one line per pair of parts. (#500)

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
