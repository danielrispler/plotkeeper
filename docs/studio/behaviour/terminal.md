# The terminal pane

Gone from the page (danielrispler/plotkeeper#326, spec #319). plotkeeper serves only what helps you understand, so the page has no terminals, no terminal panel, no split, no drag-and-drop of a file's path, and no window drag region. The terminal pane, the daemon's terminals and their restarts belong to the Tauri app, which is no longer built the page's way and is kept only until it is archived (danielrispler/plotkeeper#332).

The page's shortcuts (`ui/keys.ts`, from `keymap.json`, see [Settings files](../settings.md)) are:

| Shortcut | Does |
|---|---|
| Cmd+1, Cmd+2 | opens the session list, the story rail |
| Cmd+], Cmd+[ | selects the next, the previous session in the list's order |
| Cmd+J | selects the next session that needs you, else the next unseen one |
| Up, Down | while the list has focus, selects the row above, below |
| Cmd+Shift+Enter | focus mode: hides the list and the rail; again to leave |
| 1 to 4 | while the docked card shows answer buttons and no text box has focus, chooses that answer |
| Cmd+Shift+H | flips between Home and the session you came from |
| Cmd+Shift+M | the Map view |
| Cmd+Shift+P | the palette: every action with its key; type to filter, Enter runs |

The page has no zoom key: the browser zooms it. A Cmd shortcut the browser owns stays the browser's.
