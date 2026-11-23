# terminal-daemon: archived, unmaintained

This was `studio-terminal-daemon`, the background process that owned the desktop app's terminals so they outlived the app (danielrispler/Falinks#322, danielrispler/Falinks#464).

It is archived with the desktop app (danielrispler/plotkeeper#332, spec #319 "The archive"). It is out of the Cargo workspace, CI and `npm run check`. The only crate that used it was `src-tauri/`, also archived. The code is kept as it was for reference; nothing here is maintained.

plotkeeper has no terminals: you run your agents in your own terminal (herdr, for example), and the hook records them.
