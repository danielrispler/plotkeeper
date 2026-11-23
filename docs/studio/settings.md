# Settings files

`settings.json` and `keymap.json` sit in plotkeeper's data folder, `~/Library/Application Support/plotkeeper/`, in Zed's format: JSON with comments and trailing commas (danielrispler/Falinks#524). The desktop app reloads them when either is saved; `plotkeeper` reads them at start, so restart it after editing one. A file that does not parse is reported in the notice line, and the last good one stays in effect. Missing keys keep their defaults; a key of the wrong type is reported in the same line and keeps its default.

The page has no project list and no terminal (danielrispler/plotkeeper#326), and no settings page but one section: the summary writer, under Writer at the top of the page (spec #320, #363). Every other key you edit in your editor. The keys it reads:

```jsonc
{
  "summaries": { "model": "haiku", "effort": "low", "on_open": false, "writer": "claude" }, // writer: "claude", "agy" or "anthropic" (the API key)
  "catchUp": { "hours": 2 },   // at start, write PR briefs for ready PRs whose head moved within the hours, the newest first and at most 3
  "prBriefs": { "model": "opus" }, // the model that writes a PR brief and checks its claims; default: opus
  "context_warning": 30,         // the context share, 0 to 100, past which a session warns once; 0: never
  "checks": { "/path/to/app": ["make verify"] } // per project folder, the commands that are checks; default: `cargo test` and the like, and `npm run check`
}
```

## The writer section

Writer, at the top of the page, lists an Anthropic API key, when one is set, then the writers the login shell's `command -v` finds, Claude (`claude`) then agy (`agy`); one that is not found, or a key that is not set, is listed but cannot be picked. Pick one, a model and, for Claude, an effort, and press Test: it makes one tiny call and shows its outcome, passed, or failed with why (not logged in, quota or rate limit, timeout, not installed, an empty or failed reply) and that writer's own fix. Nothing checks a login on its own, so opening the page spends nothing. A pass saves `summaries.writer`, `summaries.model` and `summaries.effort` in `settings.json` (an effort left at default removes its key); a failure saves nothing. The section opens on the writer in effect, or, while summaries are off, on the first writer found.

**The API key.** Under the list, paste an Anthropic API key and press Save key to add one, or a new one to replace it; Remove key removes it. The key is kept in the macOS Keychain, in plotkeeper's own entry (service `plotkeeper`, account `anthropic-api-key`, which Keychain Access shows), and nowhere else: not in `settings.json`, not in the page and not in the log. plotkeeper never reads a key from another tool's config or from the environment, such as `ANTHROPIC_API_KEY`. The page learns only whether a key is set. With one set, the Anthropic API key writer comes first in the list and in the preset, and writes over HTTP with no CLI: the Messages API, with the model (`claude-haiku-5-5`, `claude-sonnet-5-5` or `claude-opus-5-5`; any full `claude-` id in `settings.json`, an alias such as `haiku` read as the first) and effort picked, schema-checked output when a call has a schema, and a 2-minute timeout. A refused key fails as having no working API key, with the fix to replace it; a quota or rate limit, a timeout and a server error fail as their kinds, which name only the HTTP status, never the error's body.

**The Test gate.** A user who arrives with no writer configured gets no summaries, Goals, Design so far, names or risk notes until a writer passes Test; the notice line says so. You count as configured when `settings.json` names `summaries.writer`, or when the data folder held `card-writer/`, which only an earlier start of plotkeeper (or the app before it) makes, at the first start of this version; that first look is kept in `writer-gate.json` in the data folder, and a passed Test sets it open for good. An existing setup therefore writes summaries after the upgrade with no Test.

PR briefs are written by `claude` whatever writer is picked. Without `claude`, no brief is attempted, and the writer section and the Map view's tray say briefs need Claude.

The writer is the program that writes the turn summaries, the session's Goal and Design so far, and names (#327; PR briefs always use `claude -p`, whatever it says, with their own model setting above): `summaries.writer` is `"claude"` (the default, `claude -p`) or `"agy"` (the Antigravity CLI, run with JSON schema output in a neutral empty folder of its own, with a 3-minute timeout that covers a logged-out `agy`'s 60-second wait). Its `model` and `effort` are read by that program, so an `agy` model is one of `agy models`, such as `gemini-4-neon-low`; a `model` that is not in that list (the default `haiku`, with the writer moved to `agy`) is written with the first of the list, `gemini-4-neon-low`, and its `effort` is dropped, while `claude` takes any model id. A writer that fails says "Summaries are off:" in the notice line, with why and its own fix, and reports why in one log line: not logged in, quota or rate limit, timeout, not installed, an empty reply, or another failure. Both writers set `PLOTKEEPER_INTERNAL=1`, so the hook never records their runs.

An older file's `zoom`, `terminal`, `sounds`, `doctor`, `editor` and `repository` keys are ignored: the browser zooms the page, and there is no terminal, sound, Doctor or editor on it.

`keymap.json` is Zed's array of contexts: `Workspace` for the page and `List` for the session list while it has focus.

```json
[{ "context": "Workspace", "bindings": { "cmd-l": "studio::ToggleList", "escape": null } }]
```

The page's actions are listed, with their keys, in the palette (Cmd+Shift+P): `studio::ToggleList`, `studio::ToggleRail`, `studio::AskAbout` (Ask on the newest turn's Headline), `studio::OpenFullDiff` (the newest turn's whole diff), `studio::Dismiss` (Esc), `studio::ToggleFocus`, `studio::NextSession`, `studio::PreviousSession`, `studio::NextNeedsYou`, `studio::JumpToUnseen` (Cmd+Shift+U, the newest unseen session in any project), `studio::SelectNext`, `studio::SelectPrevious`, `["studio::Answer", "1"]` to `"4"`, `studio::GoHome` (Cmd+Shift+H, a toggle: from a session it remembers that session and shows Home; on Home it selects the remembered session if the list still holds it, else does nothing), `studio::ShowMap` (Cmd+Shift+M), `studio::ShowHistory` (the ended sessions, searchable; unbound), `studio::RefreshLanded` (looks up every task for Landed again) and `studio::Palette`.

Unlike Zed, binding an action to a new key unbinds its default key; `null` unbinds a key. A `Workspace` shortcut without Cmd, other than `studio::Dismiss`, leaves the key to the text box you are typing in.
