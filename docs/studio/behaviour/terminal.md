# The terminal pane

The centre of the window is one terminal (danielrispler/Falinks#466): a login shell in the configured repository: `settings.json`'s `repository` (danielrispler/Falinks#605), else the main checkout of the repository the app was built in. It runs in the terminal daemon, so it outlives the app; at start the app attaches to it again and replays its screen. When no daemon runs, the app starts `studio-terminal-daemon serve` from beside its own executable, logging to `terminal-daemon.log` beside the socket. A shell that exited starts again at the next key.

The pane is xterm.js 6 with the WebGL renderer, which falls back to the default renderer when its context is lost, Unicode 11 widths, fit, search and web links. The CSP allows inline styles, which the default renderer needs. Terminal text is left to right.

- Ctrl+V, and Cmd+V with an image on the clipboard, send Ctrl+V: Claude Code reads the macOS clipboard itself and makes an `[Image #N]` chip. Cmd+V with text pastes it, bracketed.
- Shift+Enter sends ESC CR, a newline in Claude Code. Option is Meta: ESC, then the physical key, so Option+B is ESC b on any layout.
- URLs and file paths with a folder (`ui/paths.ts`) open on click: files in your editor (the `editor` setting), and an executable, script or app revealed in Finder instead, a relative path from the folder of the session that printed it. A file dropped on the window types its path, quoted for the shell.
- Claude Code asks `ESC ] 7501 ; ?`, its Program Status query, and reports its status only to a terminal that answers. The pane answers `ESC ] 7501 ; ? BEL`.

Every Cmd shortcut goes to the terminal or to the app (`ui/keys.ts`):

| Shortcut | Goes to | Does |
|---|---|---|
| Cmd+C | terminal | copies the selection (the Edit menu's Copy) |
| Cmd+V | terminal | pastes text; with an image, sends Ctrl+V |
| Cmd+A | terminal | selects all of the scrollback |
| Cmd+F | terminal | finds in the scrollback: Enter next, Shift+Enter previous, Esc closes |
| Cmd+K | terminal | clears the scrollback |
| Cmd+Left, Cmd+Right | terminal | sends Ctrl+A, Ctrl+E: start and end of line |
| Cmd+Backspace | terminal | sends Ctrl+U: deletes to the line's start |
| Cmd+1, Cmd+2 | app | opens the session list, the story rail |
| Cmd+], Cmd+[ | app | selects the next, the previous session in the list's order |
| Cmd+J | app | selects the next session that needs you, else the next unseen one |
| Cmd+D | app | splits the terminal column: pins the pane shown and shows the next session beside it; again to close |
| Up, Down | app, while the list has focus | selects the row above, below |
| Cmd+Shift+Enter | app | focus mode: hides the list and the rail; again to leave |
| Ctrl+` | app, from the terminal too | the terminal panel: a shell in the shown session's folder, under the terminal; again to hide |
| 1 to 4 | app, while the docked card shows answer buttons; never from the terminal or a text box | chooses that answer on the docked card |
| Cmd+N, Cmd+, | app | opens the new-session picker; opens `settings.json` in your editor |
| Cmd+Shift+P | app | the palette: every action with its key; type to filter, Enter runs |
| Cmd+=, Cmd+-, Cmd+0 | app | zooms the window in, out (from half to double size) and back |
| any other Cmd shortcut | app | the app's menu: Cmd+Q, Cmd+W, Cmd+H, Cmd+M and the like |

Every shortcut here but Cmd+C, Cmd+V and the app's menu can be rebound in `keymap.json` (see [Settings files](../settings.md)).

## Restarts and daemon upgrades

The terminals live in the daemon, so a studio restart, rebuild or crash loses none (danielrispler/Falinks#470). At start the app attaches every terminal the daemon runs and replays its screen, then fits it to the window, whatever size it had. One app runs at a time: a second one hands over and exits, and the first comes forward. When the second is another build, or a rebuild replaces the running app's executable, the top bar shows a "Restart to update" button.

The daemon keeps a record of the terminals it runs, `terminal-daemon.terminals.json` beside its socket. When the app finds no daemon, after a crash or a reboot, it reads that record, starts a new daemon and brings each terminal back under its id: a shell in its folder, and a Claude terminal with `claude --resume <session_id>`, its latest session's id from the hooks, so the conversation survives and the shell's state does not. The same happens when the daemon dies while the app runs.

A daemon below the app's protocol, or built from other sources, stays in use until you ask for the restart, busy terminals or not: a bar offers "Restart now…" and "Later", and names what still runs. On "Restart now…" the app asks, listing each terminal and what it loses. It never restarts the daemon on its own. On yes it stops the old daemon (`SIGTERM`, then the hangup and `SIGKILL` to every process of its terminals), and the path above brings each terminal back in the new one.
