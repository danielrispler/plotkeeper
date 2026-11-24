# Archive

Retired code, kept for reference (#415). Nothing in this folder is built, tested, linted, formatted or maintained: the Cargo workspace excludes it, `tsc` and the Node tests never read it, the system map ignores it, and CI never runs it. Don't edit it; to bring something back, copy it out and make it work where it lands.

- `src-tauri/`, `terminal-daemon/`: the Tauri desktop app and the daemon that owned its terminals (#332).
- `terminal-spike/`, `v0-spike/`: the early spikes.
- `real-calls.sh`: round 5's real model calls, whose only step left built the Tauri app.
- `workbench/`, `core/`, `understanding/`, `ui/`: code that only its own tests used, with those tests, cut from the files of the same path. They are fragments, copied verbatim, and do not compile on their own.
- The context share and its warning (#416): the status line wrapper and snapshot scripts (`workbench/statusline.sh`, `statusline-usage.sh`), the `claude` command that set them, the snapshot reader, the share's reading in `core/`, and the warning in `ui/context-warning.ts`, each with its tests. Only sessions the desktop app started saved a snapshot.

For why something was written or retired, read its history: `git log --follow -- <path>` on the old path, or the issues each file cites.
