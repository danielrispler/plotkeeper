# The terminal pane

Gone from the page (danielrispler/plotkeeper#326, spec #319). plotkeeper serves only what helps you understand, so the page has no terminals, no terminal panel, no split, no drag-and-drop of a file's path, and no window drag region. The terminal pane, the daemon's terminals and their restarts belong to the Tauri app, which is no longer built the page's way and is kept only until it is archived (danielrispler/plotkeeper#332).

The page's shortcuts (`ui/keys.ts`, from `keymap.json`, see [Settings files](../settings.md)) are:

The page picks its defaults by platform (#705): Cmd on macOS, Ctrl on Linux, where Super is the desktop's. A Linux key differs from the macOS one only where Ctrl would be the browser's or the desktop's.

| macOS | Linux | Does |
|---|---|---|
| [ | [ | shows or hides the session list: in a wide window it swaps the full list for its strip and back, below 1007 px it opens the list over the map and closes it |
| ] | ] | closes the selected session's story and opens it again, the session staying selected |
| 0 | 0 | fits the whole map in view |
| Cmd+], Cmd+[ | Ctrl+], Ctrl+[ | selects the next, the previous session in the list's order |
| Cmd+J | Ctrl+\ | selects the next session that needs you, else the next unseen one (Chrome's Ctrl+J opens Downloads) |
| Cmd+Shift+U | Ctrl+Shift+\ | selects the newest unseen session, in any project (IBus takes Ctrl+Shift+U to type a Unicode character) |
| Up, Down | Up, Down | while the list has focus, selects the row above, below |
| Cmd+Shift+Enter | Ctrl+Shift+F | focus mode: hides the list and the rail; again to leave (Chrome's Ctrl+Shift+Enter opens the address bar's site in a new window) |
| 1 to 4 | 1 to 4 | while the docked card shows answer buttons and no text box has focus, chooses that answer |
| Cmd+Shift+M, Cmd+Shift+H | Ctrl+Shift+L | back to the map: closes the left panel (`studio::ShowMap`; a `keymap.json` that still names `studio::GoHome` gets the same; Chrome's Ctrl+Shift+M switches profiles, Firefox's Ctrl+Shift+H opens its history) |
| Cmd+Shift+P | Ctrl+Shift+Space | the palette: every action with its key; type to filter, Enter runs (Firefox's Ctrl+Shift+P opens a private window) |
| Cmd+Enter | Ctrl+Enter | in an Ask box, asks |

The palette and the buttons' hovers show the keys as the platform writes them: ⇧⌘P on macOS, Ctrl+Shift+Space on Linux. The Linux keys' collisions are read from Chrome's shortcut list for Windows and Linux; Firefox's and IBus's are from their usual bindings, and no Linux browser has been tried (#510, "Limits").

The page has no zoom key: the browser zooms it. A Cmd shortcut the browser owns stays the browser's: `[`, `]` and `0` took the place of Cmd+1, Cmd+2 and Cmd+0, which Chrome uses to switch tabs and reset its zoom (spec #658). Like every shortcut without Cmd (Ctrl on Linux), they leave the key to a text box you are typing in. A Cmd or Ctrl shortcut runs while you type, except that on Linux Ctrl with A, C, V, X, Z, Y, an arrow, Home, End, Backspace or Delete (with Shift too) always keeps its text meaning in a text box, even when `keymap.json` binds it. A `keymap.json` that binds the old keys keeps them.
