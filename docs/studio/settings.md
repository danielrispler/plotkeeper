# Settings files

The studio's projects, the main checkouts Home and Workspaces list, are an ordered list in `projects.json` beside them (#93). With no file, the first launch creates it with one entry, `repository` below, else this checkout; after that the file decides, and `repository` is the starting project only.

`settings.json` and `keymap.json` sit in the studio's data folder, `~/Library/Application Support/dev.falinks.studio/`, in Zed's format: JSON with comments and trailing commas (danielrispler/Falinks#524). The app reloads them when either is saved. A file that does not parse is reported in the notice line, and the last good one stays in effect. Missing keys keep their defaults; a key of the wrong type is reported in the same line and keeps its default.

## The settings page

Cmd+, (`studio::OpenSettings`) and the gear at the top bar's far right open the settings page: a dialog over the window, about 880 by 620, closed by Esc, the X ("Close (Esc)") or a click outside it. One scrolling page, each row a title, a muted description and its control. It has four headings, nine rows:

- **Appearance:** Default zoom (a stepper, 0.5 to 2 in steps of 0.1, editing `zoom`; the window's zoom changes at once, while ⌘+, ⌘− and ⌘0 stay temporary and never write the file) and Terminal font size (a stepper, 8 to 40; unset, it shows the stylesheet's size).
- **Alerts:** Sounds, and Notify "your turn" while focused (toggles).
- **Summaries:** Model (a dropdown of Haiku, Sonnet, Opus and Fable; a model from the file that is not listed shows as its own entry and is never overwritten by opening the page), Effort (a dropdown of Default, which passes no `--effort`, then low, medium, high, xhigh and max) and Write a summary when a turn is opened (a toggle).
- **Files:** Open settings.json, Open keymap.json.

The rest (`doctor`, `checks`, `editor`, `repository`, `quiz_trial`, `quiz_self_test`) stay in the file.

Each change saves at once, by editing `settings.json` in place: the one property's text changes, so comments, trailing commas, formatting and every other key stay as they were; a missing parent object (such as `terminal`) is created, and a missing file is created with the one key. A row whose value differs from its default shows an undo icon, "Reset to default", which removes the key from the file; changing a row to its default value does the same. The file is saved whole, through a symbolic link when it is one. While the page is open the window's other keys stay still: only Esc works. A key of the wrong type counts as its default.

The page refuses to write in two cases, and shows the reason at its top:

- `settings.json` does not parse. The page shows the settings in effect (the last good ones), the line "settings.json has errors: fix it first" with an "Open settings.json" button, and every control disabled. Nothing is written.
- `settings.json` differs from the text the page last read, such as after a save from your editor. Nothing is written; the saved file reaches the page through the reload, and the change can be made again.

`studio::OpenSettingsFile` (in the palette, unbound) opens `settings.json` in your editor, creating it empty when missing; the page's "Open settings.json" and "Open keymap.json" rows do the same for each file.

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

The window's actions are listed, with their keys, in the palette (Cmd+Shift+P): `studio::ToggleList`, `studio::ToggleRail`, `studio::AskAbout` (Ask on the newest turn's Headline), `studio::OpenFullDiff` (the newest turn's whole diff), `studio::Dismiss` (Esc), `studio::ZoomIn`, `studio::ZoomOut`, `studio::ResetZoom`, `studio::ToggleSplit` (Cmd+D), `studio::TogglePanel` (Ctrl+`), `studio::NextSession`, `studio::PreviousSession`, `studio::NextNeedsYou`, `studio::JumpToUnseen` (Cmd+Shift+U, the newest unseen session in any project), `studio::SelectNext`, `studio::SelectPrevious`, `["studio::Answer", "1"]` to `"4"`, `studio::NewSession`, `studio::NewTerminal` (Cmd+T), `studio::SwitchBranch` (the main checkout's branch picker: `git switch` with no stash, disabled while a session there runs or needs you; the git line's branch token opens it too), `studio::GoHome` (Cmd+Shift+H, selects no session and shows Home), `studio::ShowHistory` (the ended sessions, searchable; unbound), `studio::OpenSettings` (Cmd+,, the settings page), `studio::OpenSettingsFile` (`settings.json` in your editor; unbound), `studio::Fetch` (fetches the repository now, #33), `studio::PullMain` (Pull main: fast-forwards every project's default branch to origin's, #164), `studio::RefreshLanded` (looks up every task for Landed again), `studio::RevealLog` (Reveal log: Finder opens with the Studio log selected), `studio::Palette` and `studio::Skills` (the skills browser, danielrispler/Falinks#528, in the rail's place; Esc returns).

The pane's are `terminal::SelectAll`, `terminal::Find`, `terminal::Clear` and `["terminal::SendText", "<text>"]`. Unlike Zed, binding an action to a new key unbinds its default key; `null` unbinds a key. A `Workspace` shortcut without Cmd, other than `studio::Dismiss` and `studio::TogglePanel`, leaves the key to the terminal or text box you are typing in:

```json
[{ "context": "Workspace", "bindings": { "cmd-l": "studio::ToggleList", "escape": null } }]
```
