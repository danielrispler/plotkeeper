# Settings files

`settings.json` and `keymap.json` sit in plotkeeper's data folder, `~/Library/Application Support/plotkeeper/`, in Zed's format: JSON with comments and trailing commas (danielrispler/Falinks#524). plotkeeper reloads them when either is saved. A file that does not parse is reported in the notice line, and the last good one stays in effect. Missing keys keep their defaults; a key of the wrong type is reported in the same line and keeps its default.

The page has no settings page, no project list and no terminal (danielrispler/plotkeeper#326): you edit the files in your editor. The keys it reads:

```jsonc
{
  "summaries": { "model": "haiku", "effort": "low", "on_open": false, "writer": "claude" }, // writer: "claude" or "agy"
  "context_warning": 30,         // the context share, 0 to 100, past which a session warns once; 0: never
  "checks": { "/path/to/app": ["make verify"] }, // per project folder, the commands that are checks; default: `cargo test` and the like, and `npm run check`
  "quiz_trial": "2026-11-20",    // the day the confidence tap and "why?" trial starts; two weeks
  "quiz_self_test": "2026-11-20" // the day the quiz-first against body-only self-test starts; three weeks
}
```

The writer is the program that writes cards (#327): `summaries.writer` is `"claude"` (the default, `claude -p`) or `"agy"` (the Antigravity CLI, run with JSON schema output in a neutral empty folder of its own, with a 3-minute timeout that covers a logged-out `agy`'s 60-second wait). Its `model` and `effort` are read by that program, so an `agy` model is one of `agy models`, such as `gemini-4-neon-low`. A writer that fails reports why in one log line: not logged in, quota or rate limit, timeout, not installed, an empty reply, or another failure. Both writers set `PLOTKEEPER_INTERNAL=1`, so the hook never records their runs.

An older file's `zoom`, `terminal`, `sounds`, `doctor`, `editor` and `repository` keys are ignored: the browser zooms the page, and there is no terminal, sound, Doctor or editor on it.

`keymap.json` is Zed's array of contexts: `Workspace` for the page and `List` for the session list while it has focus.

```json
[{ "context": "Workspace", "bindings": { "cmd-l": "studio::ToggleList", "escape": null } }]
```

The page's actions are listed, with their keys, in the palette (Cmd+Shift+P): `studio::ToggleList`, `studio::ToggleRail`, `studio::AskAbout` (Ask on the newest turn's Headline), `studio::OpenFullDiff` (the newest turn's whole diff), `studio::Dismiss` (Esc), `studio::ToggleFocus`, `studio::NextSession`, `studio::PreviousSession`, `studio::NextNeedsYou`, `studio::JumpToUnseen` (Cmd+Shift+U, the newest unseen session in any project), `studio::SelectNext`, `studio::SelectPrevious`, `["studio::Answer", "1"]` to `"4"`, `studio::GoHome` (Cmd+Shift+H, a toggle: from a session it remembers that session and shows Home; on Home it selects the remembered session if the list still holds it, else does nothing), `studio::ShowMap` (Cmd+Shift+M), `studio::ShowHistory` (the ended sessions, searchable; unbound), `studio::RefreshLanded` (looks up every task for Landed again) and `studio::Palette`.

Unlike Zed, binding an action to a new key unbinds its default key; `null` unbinds a key. A `Workspace` shortcut without Cmd, other than `studio::Dismiss`, leaves the key to the text box you are typing in.
