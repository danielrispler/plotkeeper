# Settings files

The studio's projects, the main checkouts Home and Workspaces list, are an ordered list in `projects.json` beside them (#93). With no file, the first launch creates it with one entry, `repository` below, else this checkout; after that the file decides, and `repository` is the starting project only.

`settings.json` and `keymap.json` sit in the studio's data folder, `~/Library/Application Support/dev.falinks.studio/`, in Zed's format: JSON with comments and trailing commas (danielrispler/Falinks#524). The app reloads them when either is saved. A file that does not parse is reported in the notice line, and the last good one stays in effect. Missing keys keep their defaults; a key of the wrong type is reported in the same line and keeps its default.

Each `doctor` entry is a palette action, "Doctor: <label>", marked with its kind's cost and run in a new terminal in the selected session's folder (danielrispler/Falinks#527): a `shell` one runs its command ("no tokens"), a `session` one starts `claude` with it as the prompt ("starts a Claude session, spends tokens"). A keymap can bind one as `["studio::Doctor", "<command>"]`:

```jsonc
{
  "zoom": 1,                     // 0.5 to 2
  "terminal": { "font_size": 15 }, // 8 to 40; default: the stylesheet's 12.5
  "summaries": { "model": "haiku", "effort": "low", "on_open": false },
  "notifications": { "while_focused": false }, // also notify "your turn" while focused, but not for the selected session
  "sounds": true,                // false: no needs-you or your-turn sound
  "repository": "/path/to/plotkeeper", // the starting project only, and where the shell starts; default: this checkout
  "checks": { "/path/to/app": ["make verify"] }, // per project folder, the commands that are checks; default: `cargo test` and the like, and `npm run check`
  "quiz_trial": "2026-11-20",    // the day the confidence tap and "why?" trial starts; two weeks
  "quiz_self_test": "2026-11-20", // the day the quiz-first against body-only self-test starts; three weeks
  "doctor": [
    { "label": "Check the installation", "command": "claude doctor", "kind": "shell" },
    { "label": "Full checkup", "command": "/doctor", "kind": "session" }
  ]
}
```

`keymap.json` is Zed's array of contexts, `Workspace` for the window, `List` for the session list while it has focus, and `Terminal` for the pane.

The window's actions are listed, with their keys, in the palette (Cmd+Shift+P): `studio::ToggleList`, `studio::ToggleRail`, `studio::AskAbout` (Ask on the newest turn's Headline), `studio::OpenFullDiff` (the newest turn's whole diff), `studio::Dismiss` (Esc), `studio::ZoomIn`, `studio::ZoomOut`, `studio::ResetZoom`, `studio::ToggleSplit` (Cmd+D), `studio::TogglePanel` (Ctrl+`), `studio::NextSession`, `studio::PreviousSession`, `studio::NextNeedsYou`, `studio::JumpToUnseen` (Cmd+Shift+U, the newest unseen session in any project), `studio::SelectNext`, `studio::SelectPrevious`, `["studio::Answer", "1"]` to `"4"`, `studio::NewSession`, `studio::NewTerminal` (Cmd+T), `studio::SwitchBranch` (the main checkout's branch picker: `git switch` with no stash, disabled while a session there runs or needs you; the git line's branch token opens it too), `studio::GoHome` (Cmd+Shift+H, selects no session and shows Home), `studio::ShowHistory` (the ended sessions, searchable; unbound), `studio::OpenSettings`, `studio::Fetch` (fetches the repository now, #33), `studio::PullMain` (Pull main: fast-forwards every project's default branch to origin's, #164), `studio::RefreshLanded` (looks up every task for Landed again), `studio::Palette` and `studio::Skills` (the skills browser, danielrispler/Falinks#528, in the rail's place; Esc returns).

The pane's are `terminal::SelectAll`, `terminal::Find`, `terminal::Clear` and `["terminal::SendText", "<text>"]`. Unlike Zed, binding an action to a new key unbinds its default key; `null` unbinds a key. A `Workspace` shortcut without Cmd, other than `studio::Dismiss` and `studio::TogglePanel`, leaves the key to the terminal or text box you are typing in:

```json
[{ "context": "Workspace", "bindings": { "cmd-l": "studio::ToggleList", "escape": null } }]
```
