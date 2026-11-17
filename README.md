# Studio

The control room's app (map danielrispler/Falinks#266, specs danielrispler/Falinks#328 and danielrispler/Falinks#369), for macOS.

## Build

Needs Node and the Rust toolchain.

    npm ci
    npm run tauri build

This builds `target/release/bundle/macos/Studio.app` with an ad-hoc signature (`codesign -dv` shows `Signature=adhoc`), and `target/release/studio-hook`, which the plugin runs. The bundle carries `studio-terminal-daemon` beside the app's executable, in `Contents/MacOS/`.

Open the app with `open target/release/bundle/macos/Studio.app`. `npm start` does both: it builds, recompiling only what changed, then opens the app.

## Docs

- [Sessions](docs/studio/behaviour/sessions.md): the session list, its states, notifications and starting sessions
- [The story rail](docs/studio/behaviour/rail.md): header, now, turn summary, evidence line, Ask, the narrow window
- [Cards](docs/studio/behaviour/cards.md): cards, the card writer, fingerprints, workers' cards and wake-ups
- [The Quiz](docs/studio/behaviour/quiz.md): the Quiz, the trials and the weekly rates
- [The Brief](docs/studio/behaviour/brief.md): the Brief and "Where we were"
- [Checks](docs/studio/behaviour/checks.md): the check list, check cards and exit-code rules
- [The terminal pane](docs/studio/behaviour/terminal.md): the terminal pane, its keys, restarts and daemon upgrades
- [Storage](docs/studio/behaviour/storage.md): the event log, the card store and plan usage
- [Load the plugin](docs/studio/plugin.md): loading the plugin
- [Settings files](docs/studio/settings.md): settings files and the keymap
- [Development](docs/studio/development.md): the crates, the tests and recorded logs
