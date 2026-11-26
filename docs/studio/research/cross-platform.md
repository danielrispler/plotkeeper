# Linux and Windows: what it would take

> **Answer:** Linux is close: the whole workspace, tests included, already compiles for `x86_64-unknown-linux-gnu` unchanged. What is left is about a dozen runtime spots where plotkeeper calls a macOS program or names a macOS folder: the Keychain through `/usr/bin/security`, `~/Library/...`, `ps` output read as a path, the Seatbelt probe of the sandbox gate, `/bin/zsh` as a fallback, Cmd shortcuts, and a macOS-only package and CI. About **1 to 1.5 weeks** for an agent-assisted developer, most of it running the suite on Linux and fixing what it finds. Native Windows is a real port: it does not compile (24 errors in `studio-hook` alone, before the server is even reached), and three designs rest on Unix semantics that Windows does not share: file locks, process groups, and hook commands run through a POSIX shell. About **3 to 5 more weeks** on top of Linux. WSL2 is the cheap Windows story: the Linux build, for agents that also run inside WSL2, for about a day more than Linux.
> **Evidence:** Strong for what the code depends on: every site is cited at `89b4fc3f3`, and the compile results are from `cargo check` runs on this Mac. Strong for the platform facts, read in each vendor's docs on 25 November 2026. **Unverified:** that the test suite passes on Linux (no Linux machine or container was run), how agy, Copilot and Qwen Code run hooks on Windows, and every effort number, which is a judgement.

Research for Daniel's question of 25 November 2026: what would it cost for plotkeeper, macOS-only today (`README.md:7`, `:47`; `npm/package.json` `"os": ["darwin"]`), to run on Linux and Windows? Terms follow `docs/studio/CONTEXT.md`. `archive/` is out of scope.

## How this was read

- **This repo** at `89b4fc3f3`, by grep over the workspace's `src/` and `tests/` for `std::os::unix`, `libc::`, `cfg(target_os)`, `Command::new`, `Library/`, `/tmp`, `SHELL`, `ps`, and the UI for `metaKey` and `split("/")`.
- **Compile checks.** The installed toolchain (1.99.0, `rust-toolchain.toml`) has the Linux and Windows std targets. A copy of the workspace in the session scratchpad was checked with `cargo check --workspace --all-targets --target x86_64-unknown-linux-gnu`, and `cargo check --workspace --target x86_64-pc-windows-msvc`. One change was made to the copy only: `ureq` without its `tls` feature, because `ring`'s C code cannot be cross-compiled from this Mac without a Linux C toolchain. That changes a dependency's build, not plotkeeper's code. The worktree was not changed.
- **Docs**, each cited by URL: Rust std, docs.rs (`keyring`, `dirs`, `sysinfo`), Microsoft Learn, man7.org, npm, Claude Code, Codex and Cursor. Nothing was installed or run on Linux or Windows; Docker was present but its daemon was not running, and it was not started.
- Each claim says **verified** (read in the code, a command's output, or a doc) or **unverified**, with a confidence.

## Headline

1. **No terminal, no PTY, no file watcher.** The terminal pane went with the Tauri app (`docs/studio/behaviour/terminal.md:3`). The read loop polls the event log every 200 ms (`server/src/read_loop.rs:224`). The usual hard parts of a port, ConPTY and FSEvents, are not here. **Verified**, high.
2. **Linux compiles today.** `cargo check --all-targets` for `x86_64-unknown-linux-gnu` finished with no error and no warning. Only one line in the workspace branches on the OS, the browser opener, which already says `xdg-open` off macOS (`server/src/bin/plotkeeper.rs:59-63`). So the Linux work is about behaviour, not compilation. **Verified** (command output), high.
3. **Windows does not compile.** `studio-hook` fails with 21 errors and `wayfinder-startup` with 4: `std::os::unix`, `libc::kill`, `getuid`, `O_NOFOLLOW`, `.mode()`, `Metadata::dev/ino`, `OsStr::from_bytes`, `process_group`. The server and the understanding crate, which depend on the hook crate, were never reached. A grep counts 48 such lines in `src/` (hook 13, server 28, understanding 4, wayfinder 3). **Verified**, high.
4. **The three Windows designs that need rethinking**, not just a `cfg`: the event log's lock and the run's lock (section W2), killing a run's process tree (W3), and hook commands written for a POSIX shell (W4). **Verified** for the platform facts; the impact on plotkeeper is my reading of the code, **medium-high**.

## Every macOS dependency

| # | What | Where | Linux | Windows |
|---|---|---|---|---|
| 1 | API keys in the Keychain through `/usr/bin/security` (service `plotkeeper`, accounts `anthropic-api-key`, `openai-api-key`) | `server/src/settings.rs:465-586`; used at `server/src/lib.rs:191` | Replace | Replace |
| 2 | Data folder `~/Library/Application Support/plotkeeper` | `hook/src/lib.rs:22-25`; the old-folder move `:45-70` | Change the path | Change the path |
| 3 | Runs folder `~/Library/Caches/plotkeeper/runs` | `server/src/runner.rs:437-440`, `:450` | Change the path | Change the path |
| 4 | Private temp `/tmp/pk-<hex>` for a run | `server/src/runner.rs:300` | Works | Redo with W8 |
| 5 | Sandbox gate probes Seatbelt: `/usr/bin/sandbox-exec -p … /usr/bin/true`; failure text `sandbox_apply: Operation not permitted` | `server/src/runner.rs:126-140` | New probe | No sandbox (W8) |
| 6 | `ps -o ppid= -o comm=` read as the executable's **path** (`.app/Contents/`, `/packages/app-server-daemon/…/codex`) | `hook/src/lib.rs:1163-1173`, `:1118-1126` | Silently wrong (L3) | Replace |
| 7 | `ps -o lstart=` to tell a reused pid from the run's launcher | `server/src/runner.rs:552-561` | Works | Replace |
| 8 | Process groups: `process_group(0)` and `killpg(SIGKILL)` | `server/src/runner.rs:749-751`, `:1177`, `:1441`; `server/src/back_a_law.rs:1150`, `:1190`; `understanding/src/lib.rs:1015`, `:1063`; `wayfinder/src/main.rs:39`, `:59` | Works | Job objects (W3) |
| 9 | `kill(pid, 0)` for "is it alive" | `server/src/lib.rs:821-830`; `hook/src/answer.rs:189-194` | Works | Replace |
| 10 | The hook's parent pid, `std::os::unix::process::parent_id` | `hook/src/main.rs:75` | Works | Replace |
| 11 | Programs found by `$SHELL -lc "command -v <name>"`, fallback `/bin/zsh` (6 sites) | `understanding/src/lib.rs:524-537`; `workbench/src/lib.rs:57-72`; `server/src/runner.rs:170`, `lib.rs:387`, `read_loop.rs:213`, `back_a_law.rs:371` | Fallback `/bin/sh` | Replace (W5) |
| 12 | A project's setup command through `/bin/sh -c` | `server/src/runner.rs:688` | Works | Replace (W5) |
| 13 | `install` finds a harness by the executable bit | `server/src/install.rs:203-207` | Works | PATHEXT (W5) |
| 14 | `install` cleans the archived app's lines from `~/.zshrc` | `server/src/install.rs:1-9`, 40 mentions | Skip | Skip |
| 15 | Hook commands in shell form: `"\"<bin>/plotkeeper-hook\" --agent claude"` | `plugin/hooks/hooks.json:3-20`, `plugin-codex/hooks.json`, `plugin-cursor/hooks.json`; rendered at `server/src/install.rs:336-339` | Works | Breaks (W4) |
| 16 | Hook binaries without `.exe`, copied with Unix modes, replaced by rename while a hook may run | `hook/src/lib.rs:36` (`HOOK_BINARIES`); `server/src/hook_copies.rs:6`, `:24` | Works | W6 |
| 17 | Private files: `mode(0o600/0o700)`, `O_NOFOLLOW`, owner uid check on the answer file | `hook/src/answer.rs:155-175`, `:496-530`; `server/src/lock.rs:105`; `runner.rs:669-674`; `back_a_law.rs:1252-1258` | Works | W7 |
| 18 | Log identity by `(dev, ino)`; untracked paths as bytes | `hook/src/lib.rs:118-136`, `:299-300`, `:528` | Works | W2 |
| 19 | Exclusive `File::try_lock` on the event log, opened append-only; the run's lock handed to the launcher as its stdin | `hook/src/lib.rs:117-136`; `server/src/runner.rs:1165-1177` | Works | W2 |
| 20 | Paths assumed to start with `/` and split on `/` | 14 `starts_with('/')` in the crates' `src/`; `ui/paths.ts:13`, `ui/projects.ts:23`, `ui/map.ts:273`, `ui/evidence.ts:23` | Works | W9 |
| 21 | Shortcuts on Cmd (`metaKey`), shown as macOS glyphs | `ui/settings.ts:52-70`; `ui/keys.ts:24-40`, `:85`; `ui/ask.ts:119` | Ctrl (L6) | Ctrl (L6) |
| 22 | npm scripts use `rm`, `cp`, `mkdir -p` | `package.json:7`, `:12`, `:17` | Works | cmd.exe (W10) |
| 23 | Packaging: `lipo` universal binaries, `codesign -v`, `os: ["darwin"]` | `scripts/npm/pack.sh`; `npm/package.json` | New package | New package |
| 24 | CI on `macos-latest` only; smoke on Apple silicon and Intel | `.github/workflows/studio.yml:23-27`; `release.yml:45`, `:70-73`, `:90`; `snap.yml:28` | Add a job | Add a job |
| 25 | Smoke test names `$HOME/Library/Application Support` | `scripts/npm/smoke.sh:36` | Change | Rewrite |
| 26 | Tests: `Library/` in 11 files, `std::os::unix` in 24, shell-script fakes (`#!/bin/sh`) in 13 files (25 scripts) | e.g. `server/tests/install.rs`, `server/tests/runner.rs`, `hook/tests/data_folder.rs` | Paths only | Port or gate |
| 27 | Docs and rules naming macOS: the Keychain hard rule, the event log's path | `AGENTS.md` (Hard rules); `docs/agents/domain.md` | Edit | Edit |

Each row was read in the code at `89b4fc3f3`: **verified**, high. The two right-hand columns are my assessment, explained below.

Not macOS-specific, checked: `ctrlc` with `termination` (Ctrl+C and SIGTERM, and Windows console events), `tiny_http` on 127.0.0.1, `tempfile`, `std::env::home_dir` (on Windows it reads `USERPROFILE`, then `GetUserProfileDirectory`, since Rust 1.85: [std docs](https://doc.rust-lang.org/std/env/fn.home_dir.html), **verified**), and `File::lock` itself, which is std on every platform since 1.89 ([std docs](https://doc.rust-lang.org/std/fs/struct.File.html), **verified**).

## Linux

### L1. Folders (2 to 4 hours)

Linux convention is XDG: `dirs::data_dir()` is `$XDG_DATA_HOME` or `~/.local/share`, and `dirs::cache_dir()` is `$XDG_CACHE_HOME` or `~/.cache`; on macOS the same calls give the two `~/Library` folders plotkeeper uses now, and on Windows `%APPDATA%` (roaming) and `%LOCALAPPDATA%` ([data_dir](https://docs.rs/dirs/latest/dirs/fn.data_dir.html), [cache_dir](https://docs.rs/dirs/latest/dirs/fn.cache_dir.html), dirs 7.0.0, **verified**). `data_dir` is already "the one place that names it" (`hook/src/lib.rs:22`), so the change is two functions plus 11 test files and `smoke.sh:36`. A `#[cfg(target_os)]` match is enough; the `dirs` crate is optional. The old-folder move (`hook/src/lib.rs:45-70`) is macOS history and can stay macOS-only. On Windows, `%LOCALAPPDATA%` is the better data folder, since the event log should not roam (my judgement).

### L2. API keys (half a day to a day, plus a decision)

Today plotkeeper drives `/usr/bin/security` with the key on stdin (`settings.rs:496-586`), and the `ApiKeys` trait (`settings.rs:468-475`) is already the seam. Two ways:

- **The `keyring` crate** (4.2.0). Its default `v1` feature brings Apple's Keychain, Windows' native store and the zbus Secret Service store; `linux-keyutils-keyring-store` is optional ([features](https://docs.rs/crate/keyring/latest/features), **verified**). One crate covers all three, so this is the pick if Windows is planned. Its README speaks of "the native secure stores on Mac, Windows, and \*nix" and says nothing about a Linux machine with no Secret Service running ([keyring-rs](https://github.com/open-source-cooperative/keyring-rs), **verified** that it is silent).
- **The CLI way, as on macOS:** `secret-tool` from libsecret on Linux. Windows has no built-in CLI that reads a stored secret back, so it does not extend (**unverified**, medium-high).

Risk: a headless or minimal Linux (a server over SSH, WSL2 without a desktop) has no Secret Service daemon, so the store fails. A fallback is needed, either "set it in the environment" or a 0600 file in the data folder, and either one is a policy choice: spec #320 says keys are kept "Never another tool's config or the environment" (`settings.rs:465-467`). The AGENTS.md hard rule names the macOS Keychain as plotkeeper's only allowed store, so it needs Daniel's edit for Secret Service and Windows' store. **Unverified** that Secret Service is missing on WSL2 by default, medium-high.

### L3. Process facts (half a day)

The hook decides which process is the agent by walking up from its parent with `ps -o ppid= -o comm=` (`hook/src/lib.rs:1163-1173`, `agent_pid` at `:1095-1113`). On macOS `comm` is the executable's full path, and the code relies on that: `in_app` looks for `.app/Contents/` (`:1124-1126`) and `codex_daemon` for `/packages/app-server-daemon/` ending in `/codex` (`:1118-1120`). On Linux, procps defines `comm` as "command name (only the executable name)" ([ps(1)](https://man7.org/linux/man-pages/man1/ps.1.html), **verified**). Nothing fails: both checks just never match, so a Codex TUI session's pid would be the shared app-server daemon's, which outlives it (#369's reason for the check), and its session would not end when its agent goes (**unverified** in a real run, medium-high). The fix is to read `/proc/<pid>/exe` and `/proc/<pid>/stat`, or use `sysinfo`, whose `Process` has `parent()`, `exe()`, `start_time()`, `name()` and `cmd()` on Linux, macOS and Windows ([sysinfo 0.40.4](https://docs.rs/sysinfo/latest/sysinfo/struct.Process.html), **verified**). `lstart` works the same on Linux ([ps(1)](https://man7.org/linux/man-pages/man1/ps.1.html), **verified**). IDE detection on Linux (AppImage, `/usr/share/code`) has no `.app` marker, so it needs its own rule.

This is the riskiest Linux item because it fails silently: a test with a stand-in `inspect` stays green.

### L4. The sandbox gate (half a day to a day)

Background runs need Claude Code's OS sandbox on and required (`runner.rs:177-282`). The gate first proves Seatbelt can apply a profile (`SANDBOX_CHECK`, `runner.rs:126-135`) and fails the run on Seatbelt's failure text (`:137-140`, research `sandbox-loopback-preflight-temp.md`). On Linux, Claude Code's sandbox uses bubblewrap and socat, which the user installs ("`sudo apt-get install bubblewrap socat`"), and "By default, when the platform doesn't support the sandbox or the dependency check at startup fails, Claude Code runs commands without sandboxing" ([sandboxing](https://code.claude.com/docs/en/sandboxing), **verified**). So the gate needs a Linux probe (`bwrap --unshare-net … true`, the shape Codex uses before a run, `sandbox-loopback-preflight-temp.md` section on Codex) and a measured list of Linux failure texts. Linux is better here than macOS: bwrap gives the sandbox its own loopback and `/tmp` (same note, Problem 1 table). Back a law's `srt` already supports Linux through bwrap, and `srt`'s own dependency check covers ripgrep, bwrap and socat there (same note). **Verified** for the docs; the probe's exact form is **unverified**.

### L5. Small fixes (2 to 3 hours)

- `/bin/zsh` as the fallback when `SHELL` is unset, at 6 sites (row 11): `/bin/sh` everywhere. Ubuntu has no zsh by default (**unverified**, high).
- `~/.zshrc` cleanup in `install` (row 14) is for the archived macOS app; skip it off macOS.
- `xdg-open` is already used off macOS, and a failure only prints "open the address above" (`plotkeeper.rs:59-66`). **Verified**.
- Hook files: Codex's user file is `~/.codex/hooks.json` on macOS and Linux ([Codex hooks](https://learn.chatgpt.com/docs/hooks), **verified**), and Cursor's is `~/.cursor/hooks.json` with no platform variant ([Cursor hooks](https://cursor.com/docs/hooks), **verified**). Hook commands run through `sh -c` on Linux as on macOS ([Claude Code hooks](https://code.claude.com/docs/en/hooks), **verified**). So `install` needs no change for Linux; the quoted data-folder path even loses its space.

### L6. Shortcuts in the page (half a day)

The default keymap is all `cmd-` (`ui/settings.ts:52-70`) and the palette shows macOS glyphs (`ui/keys.ts:85`). On Linux and Windows, Cmd is the Super or Windows key, which the desktop usually takes. The defaults need `ctrl-` variants per platform, chosen in the page, and some collide with the browser (Ctrl+1/2 switch tabs, Ctrl+J opens downloads in Chrome), so they need new keys (**unverified**, high for the collisions). This is a UI change, so it goes through `$modern-web-guidance` and `$web-design-guidelines`.

### L7. Tests on Linux (1 to 2 days, the biggest unknown)

The suite compiles for Linux, and its shell fakes are POSIX `sh`, so most should run. What will surface is unknown until it runs: tests that write `Library/` paths (11 files), any that expect macOS `ps` output, `/tmp` against `/private/tmp` (`core/src/lib.rs:184` handles that symlink), GNU against BSD tool flags, and timing (the hook's 1.2 s trees budget, `studio.yml:47-48`). Per AGENTS.md, a test that fails twice is a bug, so budget for diagnosis, not just reruns. **Unverified**.

### L8. CI and packaging (1 to 1.5 days)

- CI: an `ubuntu-latest` job running `npm run check` beside `macos` in `studio.yml`; `cargo-nextest` comes from `taiki-e/install-action` already (`studio.yml:41-43`). `apt-get install bubblewrap socat` only if a test runs the real sandbox. The snap jobs read no Rust and can stay on macOS (`development.md`, line 59).
- Package: `pack.sh` builds universal macOS binaries with `lipo` and checks `codesign` (`scripts/npm/pack.sh`), and `npm/package.json` says `"os": ["darwin"]`. The pattern the install research found in Codex, esbuild and ccusage is per-platform packages under `optionalDependencies` with a small launcher (`install-one-command.md`, section 1, **verified** there). For Linux, `x86_64-unknown-linux-gnu` and `aarch64`; musl builds would remove the glibc version question (**unverified**, medium). Linux needs no code signing. The smoke script is POSIX `sh` and runs on Linux once its data path changes.

### L9. Docs (2 to 3 hours)

README's "macOS only", `docs/agents/domain.md`'s event log path, `development.md`'s macOS lines, the AGENTS.md key-store rule (L2), and `system-map.json` for any file that moves.

## Windows

Everything in Linux, plus the following.

### W1. Compile: the Unix-only calls (2 to 3 days)

48 lines across the crates (headline 3). Most have a std or `windows-sys` counterpart; a few need a design (W2, W3, W7). Same-file identity is the awkward one: `MetadataExt::volume_serial_number` and `file_index` on Windows are unstable in std, so `(dev, ino)` (`hook/src/lib.rs:132-133`, `:299-300`) needs `GetFileInformationByHandle` or the `same-file` crate (**unverified** for the stability status, medium-high). `OsStr::from_bytes` on git's untracked paths (`hook/src/lib.rs:528`) needs `-z` output decoded as UTF-8.

### W2. File locks (1 to 2 days; the riskiest item)

On Windows, `File::lock` is `LockFileEx`, and Rust's docs say "locking a file will fail if the file is opened only for append" ([std](https://doc.rust-lang.org/std/fs/struct.File.html), **verified**). The hook opens the event log with `.create(true).append(true)` and then `try_lock`s it (`hook/src/lib.rs:121-123`), so as written every hook would fail to lock the log on Windows. That one is easy: add `.read(true)`.

The deeper issue is that `LockFileEx` locks are mandatory. "Locking a portion of a file for exclusive access denies all other processes both read and write access to the specified region of the file" ([LockFileEx](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex), **verified**). On macOS the lock is `flock`, advisory, so the read loop reads the log while a hook holds it. On Windows that read would fail while any hook appends, and the prune's swap of the log under the lock (`hook/src/lib.rs:114-116`) would need checking too. The answer is to lock a separate `events.lock` file, not the log, which is a small change to the code but touches the log's documented contract.

The run's lock is subtler. plotkeeper takes the lock and hands the file to `plotkeeper-run` as its stdin; "the lock is free exactly when the launcher is gone" (`runner.rs:1165-1177`, `:563-570`). That holds for `flock`, whose lock belongs to the open file that parent and child share. On Windows, "If the file handle is inherited by a process created by the locking process, the child process is not granted access to the locked region", and the OS releases a process's locks when it exits ([LockFileEx](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex), **verified**). So the lock would belong to plotkeeper, not the launcher: it would be freed when plotkeeper stops, and `pick_up` would take a running run for a dead one. The launcher must take the lock itself. **Inference** from the docs, medium-high.

### W3. Process trees (2 to 3 days)

A run's launcher leads a process group and kills it, itself included, at the end (`runner.rs:744-752`); stop kills the group after matching the pid's start time (`runner.rs:1425-1450`). Windows has no process groups to signal. A job object is its equivalent: "Operations performed on a job object affect all processes associated with the job object", children made with `CreateProcess` join it by default, `TerminateJobObject` ends them all, and `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` ends them when the last handle closes ([Job objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), **verified**). A named job can be opened again after plotkeeper restarts (same page). Liveness (`kill(pid, 0)`, rows 9 and 10), the parent pid and the start time move to `OpenProcess` or `sysinfo` (L3). Four places make groups (row 8), so this is one small module with a Unix and a Windows side.

### W4. Hook commands (1 to 2 days plus a trial per harness)

Every rendered hook is a shell-form string, the path in double quotes then arguments (`plugin/hooks/hooks.json:3`). How each harness runs it on Windows:

- **Claude Code:** shell form runs in "Git Bash on Windows, or PowerShell when Git Bash isn't installed". The docs name this exact failure: a hook that ran in PowerShell "prints a `command` that is only quoted strings, such as `"command": "\"C:\\Tools\\guard.exe\""`, instead of running it". The fix they give is exec form: `command` plus `args`, no shell, which "requires `command` to resolve to a real executable such as a `.exe`" ([hooks](https://code.claude.com/docs/en/hooks), **verified**). Exec form also works on macOS and Linux, so the plugin can switch everywhere. **Unverified** that exec form accepts an absolute path, not only a name on `PATH`, but very likely.
- **Cursor:** "On Windows, the desktop app runs hook commands through PowerShell" ([Cursor hooks](https://cursor.com/docs/hooks), **verified**). A quoted path followed by arguments is not a call in PowerShell; it needs `& "…\plotkeeper-hook.exe" --agent cursor` (**unverified**, high). So Cursor's Windows entries are rendered differently.
- **Codex:** has `commandWindows`, "an optional Windows-only command override", and keeps the user file at `%USERPROFILE%\.codex\hooks.json` ([Codex hooks](https://learn.chatgpt.com/docs/hooks), **verified**). Its docs do not name the shell (**unverified**).
- **agy, Copilot, Qwen Code:** not checked.

Also: Claude Code on Windows without Git Bash treats any exit code other than 0 or 2 from a `PreToolUse` or `PermissionRequest` hook as a failure ([hooks](https://code.claude.com/docs/en/hooks), **verified**), so `plotkeeper-answer`'s exits need a look.

### W5. Finding programs and running setup (1 day)

`$SHELL -lc "command -v claude"` (row 11) exists because a Dock-started app gets launchd's `PATH` (`workbench/src/lib.rs:60`). Windows has no login shell and no `SHELL`; the process's `PATH` with `PATHEXT` is the answer, which also finds npm's `.cmd` shims for `claude` and `codex` (**unverified**, high). `install`'s executable-bit check (`install.rs:203-207`) becomes an extension check. A project's setup command (`runner.rs:688`) needs a shell to be chosen: PowerShell, or Git Bash when present, as Claude Code does.

### W6. Replacing hook binaries (half a day to a day)

Each start refreshes the data folder's `bin/` copies by rename (`hook_copies.rs:24`, `development.md` line 41). `plotkeeper-answer` can wait up to 610 seconds (`plugin/hooks/hooks.json:13`), so a copy is often running. Windows does not let a running `.exe` be overwritten or deleted, so the refresh would fail while any card waits. Versioned folders (`bin/<version>/`) with the hooks rewired on change is the usual fix, and rewiring is already done when the rendered plugin changes (`install-one-command.md`, "The start", step 5). **Unverified** for the exact rename behaviour: Rust's `fs::rename` uses POSIX semantics on Windows 10 1607 and later when the filesystem supports `FileRenameInfoEx` ([std](https://doc.rust-lang.org/std/fs/fn.rename.html), **verified**), but the page does not say whether that replaces a running image. Medium.

### W7. Private files (1 day)

The answer file is the trust boundary: the hook takes only "a file of its own user with mode 0600 that echoes this request's nonce" (`hook/src/answer.rs:19-20`, checks at `:496-530`). Windows has no mode bits. Files under `%LOCALAPPDATA%` already inherit an ACL that keeps other users out (**unverified**, medium-high), so the uid and mode checks become an owner-SID check, and `O_NOFOLLOW` becomes `FILE_FLAG_OPEN_REPARSE_POINT`. The FIFO case (`O_NONBLOCK`, `:501`) does not exist on Windows. Worth a careful review, since it is a security check.

### W8. Background runs and the sandbox (half a day to refuse; out of reach to support)

"On native Windows, Claude Code runs commands unsandboxed. To use the sandbox on a Windows machine, run Claude Code inside a WSL2 distribution" ([sandboxing](https://code.claude.com/docs/en/sandboxing), **verified**; the same in the [setup](https://code.claude.com/docs/en/setup) table, "Native Windows … Not supported"). plotkeeper's runs must have the sandbox (`runner.rs:177-182`, `:242-244`), so on native Windows the honest v1 is that background runs and back a law refuse with a clear message. `srt` has a Windows backend (`srt-win`, WFP egress probes; `sandbox-loopback-preflight-temp.md`, rows on srt Windows), so back a law's checks could come later; Claude Code's agent side cannot. The `/tmp/pk-…` private temp (`runner.rs:300`) goes with it.

### W9. Paths in what agents report (1 to 3 days, uncertain)

On native Windows, transcripts and hook payloads carry `C:\…` paths. The crates have 14 `starts_with('/')` checks and the page splits on `/` in four helpers (row 20); projects, worktrees, the map's parts and evidence are keyed by path strings. Each needs a normalising step (drive letter case, separators, `\\?\` prefixes from `canonicalize`). The spread is wide and some of it is in the understanding code, so this is the least predictable Windows item. **Unverified**, medium.

### W10. Build scripts, tests, CI, package (4 to 7 days)

- npm runs scripts with `cmd.exe` on Windows ([npm config](https://docs.npmjs.com/cli/v11/using-npm/config#script-shell), **verified**), so `rm -f`, `cp` and `mkdir -p` in `build` and `check` (`package.json:7`, `:17`) fail. Small Node scripts, or `script-shell` set to Git Bash for development.
- Tests: 24 files use `std::os::unix` and 13 write `#!/bin/sh` fakes (row 26). Each fake becomes a small Rust test binary, or the test is `#[cfg(unix)]` with a Windows twin where it guards a law. This is the bulk of the Windows effort (2 to 4 days).
- CI: a `windows-latest` job. `pack.sh` and `smoke.sh` are POSIX `sh`; GitHub's Windows runners have Git Bash, but the smoke test's paths and `kill` need a Windows pass.
- Package: npm on Windows "creates a cmd (Windows Command File) which executes the specified file in the `bin` field", and its docs assume a script with a `#!/usr/bin/env node` line ([package.json bin](https://docs.npmjs.com/cli/v11/configuring-npm/package-json#bin), **verified**). A bare native binary with no `.exe` is the unsupported case, so Windows needs `plotkeeper.exe` in a `win32-x64` package and a JS launcher, as Claude Code's own npm package does with per-platform optional dependencies including `win32-x64` and `win32-arm64` ([setup](https://code.claude.com/docs/en/setup), **verified**). Unsigned executables may meet SmartScreen; whether npm-installed files carry the Mark of the Web was not checked (**unverified**).

## WSL2: the cheap Windows story

Run the Linux build inside WSL2, with the agents in the same distribution.

- The browser on Windows reaches it: "If you are building a networking app … in your Linux distribution, you can access it from a Windows app (like your Edge or Chrome internet browser) using `localhost`" in the default NAT mode ([WSL networking](https://learn.microsoft.com/en-us/windows/wsl/networking), **verified**). plotkeeper serves on 127.0.0.1; whether NAT-mode forwarding covers a server bound to 127.0.0.1 rather than 0.0.0.0 was not tested (**unverified**, medium-high). Mirrored mode shares `localhost` both ways (same page).
- `xdg-open` may be missing in WSL; plotkeeper then prints the address (`plotkeeper.rs:64-66`), which is enough.
- Background runs work: Claude Code's sandbox supports WSL2, not WSL1 ([sandboxing](https://code.claude.com/docs/en/sandboxing), **verified**).
- The limit: plotkeeper in WSL2 sees only agents whose hooks it wired, which are the Linux-side `~/.claude`, `~/.codex`, `~/.cursor`. A Windows-native Claude Code or the Cursor desktop app on Windows writes to `%USERPROFILE%` and is not recorded. How Cursor's Windows app runs hooks for a WSL remote folder was not checked (**unverified**).
- Cost: Linux plus about a day: a README section, one trial on a real Windows machine, and the API key store question (L2), since WSL2 has no Secret Service by default.

## Effort

Days for one agent-assisted developer, meaning work days including review and a real-machine trial. These are my estimates, not measured.

| Area | Linux | Windows, on top of Linux | Confidence |
|---|---|---|---|
| Folders (L1) | 0.25-0.5 | 0.25 | High |
| API keys (L2) | 0.5-1, plus a rule edit | 0.25-0.5 | Medium (headless fallback is a decision) |
| Process facts (L3, W3 liveness) | 0.5 | in W3 | Medium-high |
| Sandbox gate (L4, W8) | 0.5-1 | 0.5 (refuse) | Medium |
| Small fixes (L5) | 0.25 | - | High |
| Shortcuts (L6) | 0.5 | - | High |
| Unix-only calls (W1) | - | 2-3 | Medium |
| File locks (W2) | - | 1-2 | Medium-low |
| Process trees (W3) | - | 2-3 | Medium |
| Hook commands (W4) | 0 | 1-2, plus trials | Medium-low (3 harnesses unchecked) |
| Finding programs, setup shell (W5) | - | 1 | Medium |
| Hook binary refresh (W6) | - | 0.5-1 | Medium |
| Private files (W7) | - | 1 | Medium |
| Paths in agent data (W9) | - | 1-3 | Low |
| Tests (L7, W10) | 1-2 | 2-4 | Low |
| Build scripts (W10) | 0 | 0.5-1 | High |
| CI (L8, W10) | 0.25-0.5 | 0.5-1 | High |
| Package (L8, W10) | 0.75-1 | 1-1.5 | Medium |
| Docs (L9) | 0.25-0.5 | 0.25-0.5 | High |
| **Total** | **about 5.5-9 days (1 to 2 weeks)** | **about 14-25 days more (3 to 5 weeks)** | |
| WSL2 path | Linux plus about 1 day | - | Medium |

So the user's guess holds for Linux: it is mostly configuration, not a port. For native Windows it does not: the work is spread over many small spots, and three of them (W2, W3, W4) change how parts talk to each other.

## Risks

1. **Windows file locks** (W2). Mandatory, per handle, not inherited. The event log's lock blocks its reader, and the run's lock would follow plotkeeper instead of the launcher, so a run in progress would look dead after a restart. Platform facts verified; the effect on plotkeeper is inferred from the code.
2. **Hooks on Windows** (W4). Claude Code without Git Bash and Cursor always run the current quoted command in PowerShell, where it does not run. Three harnesses (agy, Copilot, Qwen Code) are unchecked. Every recorded event depends on this.
3. **Silent wrongness on Linux** (L3, L4). `ps -o comm` gives a name, not a path, so the Codex daemon and IDE checks never match and sessions keep the wrong pid; and Claude Code runs unsandboxed when bwrap or socat is missing unless the gate's probe is redone for Linux. Neither shows as a compile error or, with stand-ins, a red test.

Also worth naming: headless Linux and WSL2 have no Secret Service (L2), and native Windows has no Claude Code sandbox, so background runs cannot be offered there (W8).

## Suggested order

1. **Linux first, with a CI job from day one.** Add `ubuntu-latest` to `studio.yml` early, even red, so every later change is checked on both. Then L1, L5, L3, L4, L2, the test fixes, L6, and packaging last.
2. **Publish WSL2 as the Windows answer** once Linux ships: a README section and one trial on a Windows machine.
3. **Native Windows only if there is demand**, in this order: W1 behind a small platform module (process, files, locks), W2 and W3, then W4 with a trial per harness, then W5-W9, with a `windows-latest` job added at W1 so the compile stays green. Background runs refuse on native Windows (W8).

## Still open

- Run the suite on Linux (a container or the CI job) to replace L7's guess with a list.
- Read agy's, Copilot's and Qwen Code's hook docs for Windows.
- Measure, on Windows: a rename over a running `.exe` (W6), and NAT-mode forwarding of a 127.0.0.1 bind from WSL2.
- Daniel's call on L2: which stores the hard rule allows, and what a machine with no store does.
