# Windows: what it would take

> **Answer:** Native Windows is a real port, but a smaller one in elapsed time than the first round said, and "wait for demand" no longer pays. Linux was estimated at 5.5 to 9 agent-assisted days and merged about 10 hours after its tickets were cut (48 hours after its spec), so this note's day counts are effort units, not calendar time. A core native v1 (Claude Code, Codex and Cursor; no background runs, micro-worlds or back a law; API keys refused; unsigned binaries) is about 13 to 23 effort days, foundations included, which I expect to land in **1 to 2.5 weeks of wall clock**. The bottleneck is not coding: it is a Windows CI round per fix (no local Windows build), a Windows machine for the few trials CI cannot do, and Daniel's review. Most of the "unverified" platform questions can be settled by one GitHub Actions spike (drafted, about 20 to 30 minutes, free here). Background runs are not "blocked": srt has an alpha Windows backend, Codex has a native sandbox and a `codex sandbox` command, and Microsoft says Claude Code will adopt its MXC containers. They are still risky and stay refused in v1. Workspace facts are unchanged: 60 compile errors in shipping code and 107 in tests, and four designs that need rethinking (process trees, locks, hook commands, agent paths).
> **Options** (effort in agent-assisted work days, a judgement; wall clock calibrated on #510): **(a) WSL2 only**, documented: 1 to 2 days; its own trial needs a real PC or CI, since WSL2 does not run in a Windows VM on Daniel's Apple-silicon Mac. **(c) Native core v1** (above): 13 to 23 days, 1 to 2.5 weeks of wall clock. **(c+) all eight harnesses**: (c) plus 3 to 6 days (agy's redesign, Qwen, Cline, Copilot, OpenCode). **(b) Full parity**: (c) plus 5 to 14 days, through srt's Windows backend or Codex's sandbox, both alpha or new; no wall-clock guess, as it waits on a security trial on a real Windows machine. **Recommended:** now, with no Windows code, do the five cheap foundation changes (`.gitattributes`, reserving the two npm names, Claude Code's exec-form hooks, a separate `events.lock`, and a Rust fake helper so that no new `#!/bin/sh` fakes are added), and run the CI spike once. Then write (c) as a spec when Daniel chooses, not on a demand signal. Keep runs refused on native Windows and point to WSL2.
> **Evidence:** Strong for the code (cited at `3d44062aa` through `git show`. Main has moved to `e6034bfd5` since, and `server/src/runner.rs`, `back_a_law.rs`, `web.rs`, `read_loop.rs`, `lib.rs`, `bin/plotkeeper.rs` and `understanding/src/part_catch_up.rs` changed, so line numbers there may have shifted), for #510's history (`gh issue`, `gh pr`, `git log`), for CI timings (`gh run view`, 12 runs) and for signing (Authenticode blocks read from 8 published npm packages). Platform and vendor facts come from Microsoft Learn and support pages, vendor docs, and vendor source at pinned commits, all read on 2026-11-28. **Unverified:** anything run on Windows (still nothing), Microsoft's documented default ACL of `%LOCALAPPDATA%` (none found), whether SmartScreen checks an exe that has no Mark of the Web, and every effort and wall-clock number.

Research for Daniel's question of 28 November 2026: Windows support is important; what would it take, and what are the cheaper options? Builds on `docs/studio/research/cross-platform.md` (read at `89b4fc3f3`), which this note updates where main has moved. Linux support (spec #510) has shipped since, and #710 documented Linux and WSL2 in the README. Terms follow `docs/studio/CONTEXT.md`. `archive/` is out of scope.

**Second round, the same day.** Daniel challenged the first round's recommendation: there are no users yet, so contract and on-disk changes are cheap now, and the estimate had already drifted from 3 to 5 weeks to 4 to 6. This round treats the first round's conclusions as claims to test. It adds four sections: prior art and security trade-offs, which foundation changes to do now, the estimate split into work and waiting, and what CI can settle. A line starting **Moved:** gives the old value and the reason it changed.

## How this was read

- **This repo** at `3d44062aa` (main, 2026-11-28), by grep over the workspace crates' `src/` and `tests/` for `cfg(target_os)`, `cfg(unix)`, `std::os::unix`, `libc::`, `Command::new`, `/bin/`, `"-c"`/`"-lc"`, `secret-tool`, `/usr/bin/security`, `process_group`, `try_lock`, `/tmp`, `/dev/`, `starts_with('/')`, `split('/')`, and the UI for `split("/")`, `metaKey` and the platform check. The plugin folders and `npm/` were read whole.
- **Compile checks.** A copy of the workspace (no `.git`, `target`, `node_modules`, `local-evidence` or worktrees) in the session scratchpad was checked with `cargo check --keep-going --workspace --all-targets --target x86_64-pc-windows-msvc`, toolchain from `rust-toolchain.toml`. As in the prior note, `ureq` lost its `tls` feature in the copy, since `ring`'s C code does not cross-compile from a Mac. Because a crate that fails stops its dependents from being checked, I then stubbed the failing crates' Unix-only items in the copy (a research shim that stands in for `std::os::unix` and `libc` items), layer by layer, so each crate was checked once with its own code unchanged. Each error is counted once, at the first run where its crate was reached. The checkout was not changed. `aarch64-pc-windows-msvc` is not installed (`rustup target list --installed`) and was not installed, so arm64 was not checked; it would give the same errors, since none is architecture-specific (my reading, high).
- **Docs**, each cited by URL and read on 2026-11-28: Microsoft Learn, Rust std, docs.rs, Node.js, npm, GitHub, and each harness vendor's docs. Nothing was installed or run on Windows. No toolchain was installed and Docker was not started.
- Each claim says **verified** (code at `3d44062aa`, a command's output, or a primary source) or **unverified**, with a confidence. A vendor's GitHub issue is a user report; the first round relied on none, and the second round labels each one it cites.
- **Second round.** Code read at `3d44062aa` through `git show`, though main moved to `e6034bfd5` during the work. `gh issue`, `gh pr` and `git log` for #510's history, and `gh run view` on 12 `studio.yml` runs for CI timings. Eight npm packages' Windows tarballs were downloaded into empty scratchpad folders, and their PE files' Authenticode tables were read by a script kept outside those folders; nothing from them was run. Vendor source was read at pinned commits through `gh api` and raw GitHub, and `process-wrap` 11.0.0's crate source was read. The spike's probe crate was type-checked for `x86_64-pc-windows-msvc` on this Mac. Three background reads, of sandboxing, of signing and local-file facts, and of the runner image and harness CLIs, gathered sources. Their load-bearing facts are marked "delegated read", and two were checked again directly: the srt README's Windows lines, and Codex's `cmd.exe /C` hook runner.

## Headline

1. **Still no terminal, PTY or file watcher.** The read loop polls the log every 200 ms (`server/src/read_loop.rs:181-185`). The usual hard parts of a port are absent. **Verified**, high.
2. **Windows does not compile, and the first wall moved.** The first crate to fail is now `studio-workbench` (15 errors), because #548's one process runner put process groups and `waitid` there (`workbench/src/run.rs:12`, `:115-123`, `:212-245`). Every crate depends on it. Behind it, `studio-hook` fails with 21 (the same count as at `89b4fc3f3`), `studio-server` with 24. `studio-core`, `studio-wayfinder`, `studio-system-map` (library) and `studio-understanding` (library) compile; wayfinder's 4 errors of the prior note are gone because it now runs commands through the workbench. **Verified** (command output), high.
3. **The Linux seams default to Linux, not to "unsupported".** Fourteen `cfg` lines in shipping code split macOS from the rest (`hook/src/lib.rs:26-69`, `server/src/runner.rs:167-171`, `:248`, `server/src/settings.rs:746-749`, `server/src/install.rs:62`, `server/src/bin/plotkeeper.rs:39`) and three split Linux from the rest, all in the process walk (`hook/src/lib.rs:1317`, `:1333`, `:1350`). On Windows each would take the wrong arm: XDG folders under the user profile, `secret-tool`, the `bwrap` check through `/bin/sh`, and `ps` for the process walk. So a Windows port adds a third arm to each, not new seams. That is good news for the shape of the work and bad news for silent failure. **Verified**, high.
4. **Four designs need rethinking**: the process runner and the run launcher (D1), the two locks (D2), hook commands per harness (D3), and agent paths (D4). Background runs are out of reach on native Windows (D5). Platform facts **verified**; the effect on plotkeeper is my reading of the code, **medium-high**.

## Compile errors, measured

`cargo check --keep-going --workspace --all-targets --target x86_64-pc-windows-msvc` at `3d44062aa`, per crate and target kind, each error counted once:

| Crate | Shipping code | Tests and examples | First blocking file |
|---|---|---|---|
| `studio-workbench` | 15 | 10 | `src/run.rs` (process groups, `waitid`) |
| `studio-hook` | 21, plus 1 in `src/main.rs` that the library's failure masked | 6 | `src/lib.rs`, `src/answer.rs`, `src/sys.rs` |
| `studio-server` | 24 | 68 (15 in `src/**/tests.rs`, 50 in `tests/`, 3 in `examples/sandbox_gate.rs`) | `src/runner.rs` (16 of the 24) |
| `studio-understanding` | 0 | 14 | `tests/contract.rs` |
| `studio-system-map` | 0 | 2 | `tests/edges.rs`, `tests/laws.rs` |
| `studio-test-support` (test-only crate) | - | 7 | `src/lib.rs` (`killpg`, `ps -o lstart`) |
| `studio-core`, `studio-wayfinder` | 0 | 0 | - |
| **Total** | **60** | **107** | 34 files |

By kind: `std::os::unix` not found 53; Unix modes (`from_mode`, `.mode()`, `custom_flags`, `O_NOFOLLOW`) 36; process and signal calls (`kill`, `killpg`, `waitid`, `sigaction`, `getpgrp`, `getuid`) 37; `dev`/`ino`/`uid` metadata 16; tests whose constants exist only for macOS and Linux (`DATA`, `expected`) 8; `process_group` 5; other (`is_fifo`, `mtime`, `ExitStatus::from_raw`, `OsStr::from_bytes`) 12. **Verified** (command output), high.

By grep, 36 lines in shipping code use `std::os::unix` or `libc::` (hook 10, server 17, workbench 9), against 48 in the prior note: the Linux and #548 work folded several into one place each (`hook/src/sys.rs`, `workbench/src/run.rs`). In tests, 20 files use them and 20 files write 36 shell-script fakes (`#!/bin/sh`), out of 115 test files. **Verified**, high.

## Every platform-dependent site on main

"Seam" says whether the Linux work left one place to add a Windows arm. Size: **trivial** (an hour or two), **small** (up to a day), **design** (changes how parts talk). Sizes are my judgement.

| # | Site | Where at `3d44062aa` | On Windows today | Windows needs | Seam | Size |
|---|---|---|---|---|---|---|
| 1 | Data folder | `hook/src/lib.rs:25-31` | XDG arm: `%USERPROFILE%\.local\share\plotkeeper` | `%LOCALAPPDATA%\plotkeeper` (`FOLDERID_LocalAppData`, [dirs `data_local_dir`](https://docs.rs/dirs/latest/dirs/fn.data_local_dir.html), verified) | Yes | Trivial |
| 2 | Runs folder | `hook/src/lib.rs:36-42`; also `scripts/check/slot.mjs` `slotDir` | XDG arm | `%LOCALAPPDATA%\plotkeeper\cache\runs` or similar | Yes | Trivial |
| 3 | Hook binary names | `hook/src/lib.rs:89` (`HOOK_BINARIES`); `__PLOTKEEPER_BIN__/plotkeeper-hook` in every plugin | No `.exe` | `std::env::consts::EXE_SUFFIX` everywhere a name is built or rendered | No | Small |
| 4 | Hook binary refresh by rename | `server/src/hook_copies.rs:24-58` | Fails while a hook runs (D6) | Versioned `bin/<version>/` and rewire | Yes | Small |
| 5 | Event log lock, append-only open, `(dev, ino)` identity | `hook/src/lib.rs:125-145`, `:229-308` | Lock fails; identity does not compile | D2 | Yes (one function) | Design |
| 6 | Run lock handed to the launcher as stdin | `server/src/runner.rs:1254-1263`, `:587-616` | Lock would follow plotkeeper, not the launcher | D2 | Yes | Design |
| 7 | One process runner: process group, `kill(-pgid)`, `waitid(WNOWAIT)` | `workbench/src/run.rs:100-245` | Does not compile | Job Objects (D1) | Yes (#548) | Design |
| 8 | Launcher: its own group, `SIGTERM` stop flag, kill own group | `server/src/runner.rs:816-837`; Stop at `:1552-1555` | Does not compile | Job Object plus a stop request that is not a signal (D1) | Yes | Design |
| 9 | Is a pid alive | `hook/src/sys.rs:6-15` | Does not compile | `OpenProcess` plus exit code, or `sysinfo` | Yes (#548) | Small |
| 10 | A pid's start time, `ps -o lstart` | `server/src/runner.rs:619-627`; `test-support/src/lib.rs:139` | No `ps` | `GetProcessTimes` | Yes | Small |
| 11 | The hook's parent pid | `hook/src/main.rs:72` | Does not compile | Toolhelp snapshot (D7) | Yes | Small |
| 12 | Process walk: `inspect`, `is_shell`, `in_app`, `codex_daemon` | `hook/src/lib.rs:1208-1345` | `ps` arm, fails, so the parent is taken as the agent | Windows arm plus Windows names (D7) | Yes (injected `inspect`) | Small to medium |
| 13 | Private files: modes, `O_NOFOLLOW`, owner uid | `hook/src/answer.rs:90`, `:99`, `:287-317`; `server/src/lock.rs:106`; `runner.rs:735-747`; `back_a_law.rs:1295-1302` | Does not compile | Owner-only ACL and an owner-SID check (D8) | No | Small (security review) |
| 14 | Random bytes from `/dev/urandom` | `hook/src/answer.rs:321-324`; `server/src/web.rs:87` | Fails at run time | `getrandom` or `ProcessPrng` | No | Trivial |
| 15 | API keys | `server/src/settings.rs:503-751`, picked at `:745-751` | SecretTool arm: "secret-tool is not installed" | Credential Manager (D9) or refuse | Yes (`ApiKeys`) | Small plus a rule change |
| 16 | Sandbox gate | `server/src/runner.rs:142-171` | `bwrap` check through `/bin/sh`: fails, so the run fails closed | A clear "not on native Windows" refusal (D5) | Yes | Trivial |
| 17 | Shared and private temp, `/tmp/claude-<uid>`, `/tmp/pk-…` | `server/src/runner.rs:244-252`, `:355-359`; `server/src/projects.rs:27-28` | Does not compile (`getuid`) | Goes with D5 | - | Trivial (refuse) |
| 18 | Programs found by `$SHELL -lc "command -v …"` (6 sites) | `understanding/src/lib.rs:529-540`, `:665`; `workbench/src/lib.rs:64-77`; `server/src/runner.rs:207`; `read_loop.rs:168-178`; `learning.rs:51`; `back_a_law.rs:443` | Runs `/bin/sh`, which does not exist | `PATH` plus `PATHEXT` search, `.cmd` shims (D10) | Mostly (`find`) | Small |
| 19 | A run's setup command, `/bin/sh -c` | `server/src/runner.rs:755-757` | n/a once runs refuse | Goes with D5 | - | - |
| 20 | Install finds a harness by the executable bit | `server/src/install.rs:265-270` | Does not compile (`.mode()`) | Extension check against `PATHEXT` | Yes | Trivial |
| 21 | Harness programs run by bare name (`qwen`, `copilot`, `cline`, `agy`, writers' `claude`, `codex`) | `server/src/install.rs:273`, `:598-637`; `understanding/src/writer.rs:366` | Misses npm's `.cmd` shims (D10) | Resolve the shim or the real `.exe` | Partly | Small |
| 22 | Hook commands as shell strings, 8 harnesses | `plugin*/` (below, D3); rendered in `server/src/install.rs` | Breaks for most | Per-harness rendering | Partly | Design |
| 23 | Browser opener | `server/src/bin/plotkeeper.rs:39-46` | `xdg-open`, fails, prints the address | `cmd /c start` or `explorer.exe <url>` | Yes | Trivial |
| 24 | Paths assumed to start with `/` | 21 `starts_with('/')` in shipping Rust, e.g. `core/src/checks.rs:56`, `:484`, `core/src/turn_files.rs:100`, `hook/src/lib.rs:654`, `:661`, `:1010`, `understanding/src/lib.rs:537` | Windows paths dropped as not absolute | D4 | No | Design |
| 25 | Paths split on `/` | 70 splits in shipping Rust, mostly on git's repo-relative paths; UI `ui/paths.ts:13`, `:18`, `ui/projects.ts:24`, `ui/map.ts:287`, `ui/evidence.ts:23` | Repo-relative ones work; absolute ones do not | D4 | No | Small |
| 26 | Git's `/dev/null` | `server/src/back_a_law.rs:1029`, `:1252`; `understanding/src/part_catch_up.rs:681` | Unknown under Git for Windows | Check, or use an empty file | No | Trivial |
| 27 | Shortcuts | `ui/settings.ts:62-67` (`platformOf`), `:109-128` (Ctrl keymap) | `Win32` maps to the Ctrl keymap already | A pass over Edge's and Windows' own keys | Yes | Trivial |
| 28 | npm scripts with `rm`, `cp`, `mkdir -p`; `check` slot with process groups | `package.json:7`, `:19`; `scripts/check/slot.mjs:163-167`, `:225` | npm runs scripts in `cmd.exe` | Node scripts, for contributors on Windows only | No | Small |
| 29 | npm package | `npm/launcher.js:9-14`, `:18`, `:40`; `npm/package.json` (`"os": ["darwin", "linux"]`) | "no plotkeeper for win32-x64" | `plotkeeper-win32-x64` (and arm64) with `.exe` binaries | Yes (#706) | Small |
| 30 | CI | `.github/workflows/studio.yml:27` (macos), `:58` (ubuntu); `release.yml` | No Windows job | A `windows-latest` job | Yes (pattern) | Small |
| 31 | Tests: `std::os::unix` in 20 files, 36 shell fakes in 20 files, 8 `cfg(target_os)` gates in tests | e.g. `server/tests/install.rs:18-21`, `server/tests/hook_copies.rs:43-47` | 107 errors, then unknown run-time failures | Rust fakes or `cfg(unix)` with Windows twins for laws | No | Design (bulk of effort) |
| 32 | Docs and rules | `AGENTS.md` (key stores), `docs/agents/domain.md`, README "Linux and WSL2" | Silent on Windows | Edits | - | Small |

Each site was read at `3d44062aa`: **verified**, high. The "On Windows today" and "Windows needs" columns are my reading, explained below.

Not platform-dependent, checked: `ctrlc` with `termination` (Ctrl+C and console close on Windows), `tiny_http` on 127.0.0.1, `tempfile`, `std::env::home_dir` (reads `USERPROFILE` on Windows, per the prior note), `.lines()` (95 sites, which strip `\r\n`; no shipping code splits on `'\n'`), and the map's extractors, which take their file lists from `git ls-files -z` (`system-map/src/lib.rs:649`), whose paths use `/` on every platform (**unverified** for Git for Windows, high).

## The design problems

### D1. Process trees: the process runner and the launcher

Today every `git` and `gh` runs in a process group of its own; on a deadline the runner kills the group, and it asks `waitid(WNOWAIT)` so the group's id stays taken while leftovers are killed (`workbench/src/run.rs:100-125`, `:204-245`). The launcher leads its own group, sets a `SIGTERM` handler that only sets a flag, and kills its group as its last act (`server/src/runner.rs:816-837`). Stop sends `SIGTERM` then `SIGKILL` to the launcher's group (`runner.rs:1552-1555`). **Verified**.

Windows has no process groups to signal. The equivalent is a Job Object: "Operations performed on a job object affect all processes associated with the job object"; a child made with `CreateProcess` joins its parent's job by default; `TerminateJobObject` ends them all; with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, closing the last handle "terminates all associated processes"; nesting works since Windows 8; only named jobs can be opened again ([Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects), **verified**).

The catch is putting a child into a job before it can start its own children. Rust's stable `std::os::windows::process::CommandExt` has `creation_flags`, `raw_arg` and `show_window`; `spawn_with_attributes` (which would pass `PROC_THREAD_ATTRIBUTE_JOB_LIST`) is nightly-only ([CommandExt](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html), **verified**). `PROC_THREAD_ATTRIBUTE_JOB_LIST` exists since Windows 10 ([UpdateProcThreadAttribute](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute), **verified**). On stable Rust the usual way is to start the child suspended, assign it, then resume it; the `process-wrap` crate (11.0.0) does exactly this: "Internally the `JobObject` wrapper always sets the `CREATE_SUSPENDED` flag" and resumes after setup, beside a Unix process-group wrapper ([process-wrap](https://docs.rs/process-wrap/latest/process_wrap/), **verified**). So one crate could replace both arms of the runner.

What changes in plotkeeper (my reading, medium-high):

- `run_until` and `run_logged` keep their contract; the Windows arm makes a job per command and terminates it at the deadline or exit. The `WNOWAIT` trick is not needed: a job keeps its members whether or not the child was reaped.
- The launcher puts the harness in a job with kill-on-close and holds the only handle, so everything a run started dies with the launcher, even when it is killed. That is stronger than today's group kill.
- Stop cannot be a `SIGTERM`. A stop file in the run's folder that the launcher polls, or a named event, replaces the signal; the hard kill is `TerminateProcess` on the launcher after matching its start time (row 10).
- The `LauncherGuard` in `test-support` and the `check` slot script use `killpg` and `process.kill(-pid)` (`test-support/src/lib.rs:98-126`; `scripts/check/slot.mjs:163-167`) and need the same.

Effort: 2 to 3 days for the runner and the launcher together; the runner alone, which reduced scope needs, 1 to 2 (judgement).

**Moved:** the runner alone is now 0.5 to 1.5 days, because `process-wrap` 11.0.0's source settles the open point. Its std `JobObject` sets `CREATE_SUSPENDED`, assigns the job and resumes the threads. With `KillOnDrop` it sets `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (`src/windows.rs:207-243`; `src/std/job_object.rs:81-140`). It leaves stdio alone, so output to a file passes through. It never breaks away from a parent job. Nested jobs work "by default" since Windows 8 ([nested jobs](https://learn.microsoft.com/en-us/windows/win32/procthread/nested-jobs)). **Verified** (crate source read; the Microsoft quote through a delegated read). It has no wait with a deadline: the runner polls `try_wait` and terminates the job, as it does with groups today.

### D2. File locks: the event log and the run

Rust's `File::lock` and `try_lock` are `LockFileEx` on Windows, stable since 1.89, and "On Windows, locking a file will fail if the file is opened only for append. To lock a file, open it with one of `.read(true)`, `.read(true).append(true)`, or `.write(true)`." The docs say only that the lock "may be advisory or mandatory" ([File](https://doc.rust-lang.org/std/fs/struct.File.html), **verified**). Microsoft says what it is: "Locking a portion of a file for exclusive access denies all other processes both read and write access to the specified region of the file"; "If the file handle is inherited by a process created by the locking process, the child process is not granted access to the locked region"; and a process's locks are freed when it exits or closes the file, after a delay that "depends upon available system resources" ([LockFileEx](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex), **verified**).

**The event log.** Every hook opens the log `.create(true).append(true)` and `try_lock`s it (`hook/src/lib.rs:129-140`), so on Windows every append fails to lock. Adding `.read(true)` makes it lock, but then the lock is mandatory: the read loop's `read_until` on the log would fail while a hook holds it. The read loop treats a read error as the end of the pass (`server/src/read_loop.rs:275-300`, `unwrap_or(0)`) and retries 200 ms later, so it would cost latency, not lines (my reading, medium). The prune reads the whole log unlocked first (`hook/src/lib.rs:230-305`, the lock taken only at `:306`), and a failed read there aborts that prune. The clean design is a separate `events.lock` beside the log, locked by every writer and the prune, so the log itself is never locked and readers never see a lock. Small code, but it changes the log's documented contract (`docs/agents/domain.md`) and touches laws, so it wants its own slice and tests.

Two more parts of the same function:

- **Same-file identity.** The lock loop and the prune compare `(dev, ino)` to see that the log was not replaced (`hook/src/lib.rs:140-141`, `:307-308`). On Windows, `volume_serial_number` and `file_index` are nightly-only (`windows_by_handle`, rust-lang/rust#63010) ([MetadataExt](https://doc.rust-lang.org/std/os/windows/fs/trait.MetadataExt.html), **verified**; this settles the prior note's W1, `cross-platform.md:113`, which left it unverified). `GetFileInformationByHandle` through `windows-sys`, or the `same-file` crate, does it on stable (**unverified** for the crate's method, high).
- **The prune's swap.** It renames the new log over the old one while hooks and the reader may hold it open. Rust opens files with `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE` by default, which "allows other processes to read, write, and delete/rename the same file while it is open" ([OpenOptionsExt](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html), **verified**), and `fs::rename` behaves as on Unix on Windows 10 1607 and later "if the filesystem supports `FileRenameInfoEx`" ([rename](https://doc.rust-lang.org/std/fs/fn.rename.html), **verified**), whose POSIX flag allows "replacing a file even if there are existing handles to it" ([FILE_RENAME_INFORMATION](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information), **verified**). So the swap should work on NTFS; **unverified** in a run, medium-high.

**The run's lock.** plotkeeper takes the run's lock and hands the file to `plotkeeper-run` as its stdin: "the lock is free exactly when the launcher is gone" (`server/src/runner.rs:1254-1263`; launcher doc at `:631-636`). On Windows the inherited handle gives the child no access to the locked region, and the lock is plotkeeper's, freed when plotkeeper exits. So after a restart `pick_up` would read a running run as dead, and while plotkeeper runs a dead launcher's run would look alive. The launcher must take the lock itself, and plotkeeper must wait until it has. **Inference** from the docs, medium-high (the prior note's W2 said the same). With D5 this matters only for full parity.

### D3. Hook commands, harness by harness

plotkeeper wires eight harnesses: Claude Code, agy, Codex, Cursor, Copilot, Qwen Code, OpenCode and Cline (`server/src/install.rs:644-664`). The prior note checked three. Today's entries, and what each vendor says about Windows:

| Harness | plotkeeper's entry today | Windows, per the vendor | What Windows needs | Status |
|---|---|---|---|---|
| Claude Code | Shell form, `"\"…/plotkeeper-hook\" --agent claude"` (`plugin/hooks/hooks.json:3-20`) | Shell form runs in "Git Bash on Windows, or PowerShell when Git Bash isn't installed"; in PowerShell a command that is only a quoted path is printed, not run. Exec form (`command` plus `args`) has "no shell" and "No shell tokenization happens on any platform"; on Windows `command` must resolve to a real `.exe`, and absolute paths are valid. Without Git Bash, a `PreToolUse` or `PermissionRequest` hook's exit other than 0 or 2 "counts as a failure" ([hooks](https://code.claude.com/docs/en/hooks)) | Exec form with `plotkeeper-hook.exe`, which also works on macOS and Linux; check `plotkeeper-answer`'s exit codes | Verified (docs) |
| Codex | Shell form (`plugin-codex/hooks.json`) | `commandWindows`, "an optional Windows-only command override"; user file `%USERPROFILE%\.codex\hooks.json` ([Codex hooks](https://learn.chatgpt.com/docs/hooks)). The source runs a hook through `%COMSPEC%` (fallback `cmd.exe`) with `/C` and `CREATE_NO_WINDOW` (`codex-rs/hooks/src/engine/command_runner.rs:461`, `:479`, `:505` at `rust-v0.172.0`) | Probably nothing: `cmd /C` runs a quoted path followed by arguments, and finds `.exe` through `PATHEXT`. The CI spike checks it, `commandWindows` included | Shell verified (source); form unverified, medium-high |
| Cursor | Shell form (`plugin-cursor/hooks.json`) | "On Windows, the desktop app runs hook commands through PowerShell"; no per-OS field; the CLI is not covered ([Cursor hooks](https://cursor.com/docs/hooks)) | `& "…\plotkeeper-hook.exe" --agent cursor` on Windows | Desktop verified; CLI unverified |
| Copilot CLI | Shell form `command` (`plugin-copilot/hooks.json`) | `command` is "Copied to both `bash` and `powershell` when those fields are absent"; `exec` "Runs the executable directly without a shell" with `args`, "Only supported in Copilot CLI" ([CLI hooks reference](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-hooks-reference)) | `exec` plus `args` | Verified (docs) |
| Qwen Code | Shell form (`plugin-qwen/qwen-extension.json`) | 1.x docs: `args` "invokes an executable directly without a shell"; without it `shell` "can select `bash` or `powershell`"; extension hooks not described ([hooks 1.x](https://qwenlm.github.io/qwen-code-docs/1.x/en/users/hooks/)) | `args`, if extensions honour it | Extensions unverified |
| agy | A POSIX shell script: `p=$(cat); (printf %s "$p" \| PLOTKEEPER_AGENT_PID=$PPID "…" --agent agy >/dev/null 2>&1 &) ; exit 0` (`plugin-agy/hooks.json`) | `command` is "The shell command to execute"; no Windows word ([agy hooks](https://antigravity.google/docs/hooks)). agy installs on Windows by PowerShell or CMD to `%LOCALAPPDATA%\agy\bin` ([getting started](https://www.antigravity.google/docs/getting-started?tab=cli)) | A redesign: the hook must read stdin, find the agent and detach by itself, since `$PPID`, `&` and `$(cat)` are POSIX | Shell unverified; design |
| OpenCode | JS plugin, `spawn(HOOK, ["--agent", "opencode"])`, no shell (`plugin-opencode/plotkeeper.js:16-27`) | "While OpenCode can run directly on Windows, we recommend using … WSL" ([OpenCode on Windows](https://opencode.ai/docs/windows-wsl)) | `HOOK` ending in `.exe`; `windowsHide: true`, since Node's default is `false` ("Hide the subprocess console window that would normally be created on Windows") ([Node child_process](https://nodejs.org/api/child_process.html)) | Verified (docs) |
| Cline | JS SDK plugin, same `spawn` (`plugin-cline/plotkeeper.js:14-20`) | Cline's CLI pages read did not list platforms | As OpenCode | Platform unverified |

So: three harnesses can run the hook with no shell on every platform (Claude Code exec form, Copilot `exec`, the two JS plugins), one has a Windows override (Codex), one needs a PowerShell call form (Cursor desktop), one probably has exec (Qwen), and one needs a redesign (agy). Each needs a trial on a real Windows machine, since a wrong form fails silently: the agent works, nothing is recorded. Effort 2 to 4 days plus trials (judgement, medium-low).

**Moved:** two of these trials no longer need a real machine. Claude Code (`-p` with a dummy key and `ANTHROPIC_BASE_URL`) and Codex (a custom provider, plus `--dangerously-bypass-hook-trust`) can both run a real session against a mock model API on a CI runner ("What CI can settle", below). Claude Code's exec form exists since 2.1.139, published 2026-05-11: "Added hook `args: string[]` field (exec form) that spawns the command directly without a shell" ([CHANGELOG](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md); date from `npm view @anthropic-ai/claude-code time`), **verified**. So the plugin can switch to it on every platform now.

Two Windows-only effects to watch, both **unverified**, medium: a GUI host (Cursor desktop, OpenCode's background service) that starts the console program `plotkeeper-hook.exe` may flash a console window per event unless the host hides it; and Git Bash's `bash.exe` or `powershell.exe` will sit between the agent and the hook in the process tree (D7).

### D4. Paths in what agents report

On native Windows, payloads and transcripts carry `C:\Users\…` paths. The core and the hook decide what is a file path with `starts_with('/')`: which files a turn touched (`core/src/turn_files.rs:100`), check targets (`core/src/checks.rs:56`, `:484`, `:691`), a session's roots (`core/src/studio/roots.rs:93`), worktree membership (`hook/src/lib.rs:654`), paths in Bash commands (`:661`) and `Edit`/`Write` targets (`:1010`). On Windows each check fails, so a turn would show no changed files and no project (**inference**, medium-high). The UI splits absolute paths on `/` for short names (row 25).

A cheap design exists: normalise at the hook boundary to forward slashes with a lower-case drive letter, since "File I/O functions in the Windows API convert "/" to "\"" ([Maximum path length](https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation), **verified**), and git prints `/` anyway. Then most checks only need "absolute" to mean `/…` or `x:/…`. Left over: case (NTFS paths are case-insensitive by default, so keys must fold case; **unverified** for per-folder case sensitivity, high), `\\?\` prefixes from `canonicalize`, UNC paths, and paths longer than `MAX_PATH`, which is 260 characters unless the registry value `LongPathsEnabled` is set and the program's manifest says `longPathAware` (same page, **verified**). Git for Windows's own long-path setting is separate (**unverified**). Effort 2 to 4 days, the least predictable item (judgement, low).

### D5. Background runs, micro-worlds and back a law

"The sandbox runs on macOS, Linux, and WSL2. On native Windows, Claude Code runs commands unsandboxed", and "with `failIfUnavailable` set, Claude Code exits at startup on those machines" ([sandboxing](https://code.claude.com/docs/en/sandboxing), **verified**; also the "Sandboxing: Not supported" row for native Windows in [setup](https://code.claude.com/docs/en/setup)). plotkeeper's runs require the sandbox and set `failIfUnavailable: true` (`server/src/runner.rs:222-236`, `failIfUnavailable` at `:227`). Today a Windows build would fail the run at the `bwrap` check through `/bin/sh` (`runner.rs:155-171`), so it fails closed, but with a Linux message. The honest native scope is a refusal with its own message for background runs and micro-worlds (which use the same launcher), and for back a law until its `srt` path is checked: `srt` has a Windows backend whose `initialize()` refuses to start if an egress probe gets out (`sandbox-loopback-preflight-temp.md`, rows on srt Windows, verified there).

Other sandboxes exist on Windows but not for plotkeeper's harness: Codex's Windows sandbox uses "dedicated lower-privilege sandbox users, filesystem permission boundaries, firewall rules" (`elevated`), a restricted token (`unelevated`), or "native process isolation" (`mxc`) ([Codex Windows sandbox](https://learn.chatgpt.com/docs/windows/windows-sandbox), **verified**). Full parity would mean a second run harness and a new security review, not a port.

**Moved:** "blocked" was too strong. Three routes exist, each with its own risk; see "Sandboxing on Windows" below. The estimate for (b) drops from 10 to 20 days to 5 to 14.

### D6. Replacing hook binaries while a hook runs

`plotkeeper-answer` can wait up to 130 s on a card (`plugin/hooks/hooks.json:8`, `:13`), and each start refreshes the copies by renaming over them (`server/src/hook_copies.rs:51-58`). On Windows, "Even if ReplaceIfExists is set to TRUE, the rename operation will still fail if a file with the same name already exists and is a directory, a read-only file, or a currently executing file" ([FILE_RENAME_INFORMATION](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information), **verified**). This settles the prior note's W6 (`cross-platform.md:144`), which was unverified: the refresh fails while any hook process runs. Claude Code's own docs note the same lock for its winget upgrade: "Windows locks the executable" ([setup](https://code.claude.com/docs/en/setup), **verified**). Fix: versioned `bin/<version>/` folders, with the hooks rewired when the version changes, and old folders removed when nothing runs from them. Half a day to a day (judgement).

**Moved:** the fix is now rename-aside, not versioned folders. Windows lets a running executable be renamed but not deleted or replaced. The `self-replace` crate (1.5.1) says a running executable "can be renamed, but it cannot be unlinked", and moves it aside to free the name ([self-replace](https://docs.rs/self-replace/latest/self_replace/), **verified** as the crate's statement; the OS behaviour is medium-high until the spike's `running-exe` probe runs). So the Windows arm of `replace` (`server/src/hook_copies.rs:51-58`) can rename the running copy to `plotkeeper-hook.old-<n>.exe`, put the new one at the stable path, and delete old copies at a later start once nothing runs them. Versioned folders would cost every platform something: a harness's fingerprint "changes when a template does, and when the binaries' path does" (`server/src/install.rs:761-762`). So every upgrade would rewire every harness, and Codex would skip plotkeeper's hooks until the user trusted them again (`CODEX_RETRUST`, `install.rs:666`). Effort 0.25 to 0.5 days (judgement).

### D7. Which process is the agent

The hook walks up from its parent, skipping one shell, and refuses IDE and Codex-daemon parents (`hook/src/lib.rs:1208-1282`). The checks read Unix paths: `is_shell` takes the name after the last `/` and knows `sh`, `bash`, `zsh`, `dash`, `fish`; `in_app` looks for `.app/Contents/` or Linux install folders; `codex_daemon` wants `/packages/app-server-daemon/…/codex` (`:1231-1276`). On Windows the lookup would take the `ps` arm (`:1317`), which fails, so the parent itself is taken as the agent: often `bash.exe` (Git Bash), `powershell.exe` or `cmd.exe`, which exits at once (**inference**, high).

The Windows lookup: a Toolhelp snapshot gives each process's `th32ParentProcessID`, but its `szExeFile` is "The name of the executable file", not the path; the full path comes from `QueryFullProcessImageName` ([PROCESSENTRY32W](https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/ns-tlhelp32-processentry32w), **verified**). `sysinfo` wraps both (prior note, verified there). Then: shells by `.exe` name (`bash.exe`, `sh.exe`, `powershell.exe`, `pwsh.exe`, `cmd.exe`), `\` separators, IDE folders (`…\AppData\Local\Programs\Microsoft VS Code\`, Cursor's), and the Codex daemon's Windows path, all **unverified** until a trial. Windows does not reparent orphans, so a parent id may name a newer, unrelated process; comparing creation times guards that (my reading, medium-high). The existing injected `inspect` keeps the tests table-driven (`hook/tests/pid.rs`). 1 to 1.5 days (judgement).

### D8. Private files and the answer file

The answer file is the trust boundary: the hook accepts only "a file of its own user with mode 0600 that echoes this request's nonce", opened with `O_NOFOLLOW | O_NONBLOCK` (`hook/src/answer.rs:19-20`, `:287-317`). Windows has no mode bits, FIFOs or `getuid`. The Windows arm creates the file with an owner-only DACL and checks the owner SID and the DACL on the open handle, opening with `FILE_FLAG_OPEN_REPARSE_POINT` instead of `O_NOFOLLOW`. Whether files under `%LOCALAPPDATA%` already keep other users out by inherited ACL is **unverified** (medium-high). The nonce reads `/dev/urandom` (`answer.rs:323`), as does the server's token (`server/src/web.rs:87`); both need the OS's generator. Small, but it is a security check and wants a careful review (judgement).

**Moved:** the custom DACL is dropped from v1. Comparable tools rely on the inherited profile ACL ("Local files and tokens", below). The Windows arm keeps three checks: the owner SID is the user's, the open does not follow a reparse point, and the nonce matches. Effort 0.5 to 1 day, down from 1 to 1.5.

### D9. API keys

The `ApiKeys` trait is the seam; the Keychain (`/usr/bin/security`) and `secret-tool` are its two arms (`server/src/settings.rs:503-751`). On Windows the store is the Credential Manager. `keyring` 4.2.0 lists `windows-native-keyring-store` as its Windows backend ([keyring](https://docs.rs/keyring/latest/keyring/), **verified**), which "uses the Windows Credential Manager as its back end", one "generic credential" per entry ([windows-native-keyring-store 1.1.0](https://docs.rs/windows-native-keyring-store/latest/windows_native_keyring_store/), **verified**); calling `CredWriteW`/`CredReadW` directly is the alternative. Spec #510 put the `keyring` crate out of scope for Linux, and the AGENTS.md hard rule names only the Keychain and the Secret Service, so either way is Daniel's rule change. The cheaper first step is the headless-Linux behaviour: refuse to save a key and point to a CLI writer.

### D10. Finding programs

Six places ask the login shell (`$SHELL -lc "command -v …"`, fallback `/bin/sh`) because a Dock-started app gets launchd's `PATH` (row 18). Windows has neither; the process's own `PATH` is the answer. But npm installs `codex`, `qwen`, `copilot` and `cline` on Windows as `.cmd` shims ([package.json `bin`](https://docs.npmjs.com/cli/v11/configuring-npm/package-json): "a cmd (Windows Command File) will be created", **verified**), and Rust's `Command::new` finds a bare name only as `.exe`: "Files with other extensions must include the extension, otherwise they will not be found" ([Command](https://doc.rust-lang.org/std/process/struct.Command.html), **verified**). Running a `.cmd` goes through `cmd.exe`'s own argument rules, which the same page warns about. So the lookup must search `PATHEXT` and either run the shim with care or find the real `.exe` behind it. Claude Code and agy install native `.exe` files ([setup](https://code.claude.com/docs/en/setup); agy getting started, both **verified**). 1 to 2 days (judgement).

### Smaller items

- **Line endings.** The repo has no `.gitattributes` (checked), and Git for Windows usually checks out with CRLF (**unverified**, medium-high). Code that reads lines uses `.lines()`, which strips `\r\n`, but tests that compare bytes, the 49 files with `include_str!`/`include_bytes!`, the frozen samples in `understanding/tests/fixtures/stored/` and any content fingerprint would see `\r`. A `.gitattributes` with `* text=auto eol=lf` before the first Windows CI run avoids it (judgement).
- **Defender and test speed.** nextest: Windows "is usually slower than on Unix platforms", antivirus "might interfere with process execution, making your test runs significantly slower", and "process creation is significantly slower on Windows"; it suggests excluding the repo, `target` and Cargo home, or a Dev Drive ([nextest on Windows](https://nexte.st/docs/installation/windows/), **verified**). The hook's 1.2 s budget for its trees (`studio.yml:49`, `:80`) may not hold there (**unverified**).
- **SmartScreen and Smart App Control.** SmartScreen checks "downloaded files against a list of files that are well known and downloaded frequently" and warns when one is not; it checks files any app "attempts to download and run" ([SmartScreen](https://learn.microsoft.com/en-us/windows/security/operating-system-security/virus-and-threat-protection/microsoft-defender-smartscreen/), **verified**). Whether a file npm wrote counts as downloaded is **unverified** (the prior note's open point at `cross-platform.md:163` stays open). Smart App Control is stricter: in enforcement mode "Apps cannot be run unless they are recognized by Microsoft's app intelligence services, or they are signed with a trusted certificate", and "unknown, unsigned code are blocked by default" ([Smart App Control](https://learn.microsoft.com/en-us/windows/apps/develop/smart-app-control/overview), **verified**). So on a machine with it on, an unsigned `plotkeeper-hook.exe` may not run at all. The prior note did not cover this. **Moved:** both open points are now narrowed, under "Code signing, SmartScreen and Smart App Control" below. npm sets no Mark of the Web. Smart App Control checks each binary as it loads, whether or not it carries the mark, and since 2026 it can be turned on without a clean install.
- **Code signing.** Microsoft's Artifact Signing (formerly Trusted Signing) is a managed signing service with Basic and Premium SKUs and an identity validation step ([Artifact Signing](https://learn.microsoft.com/en-us/azure/artifact-signing/overview), **verified**); a Basic price of about USD 9.99 a month was seen only in a search summary of Azure's pricing page (**unverified**). Claude Code's Windows binaries are signed "by Anthropic, PBC" ([setup](https://code.claude.com/docs/en/setup), **verified**), which is the bar users will compare against. **Moved:** that bar holds only for agent vendors. Build tools on npm ship unsigned Windows binaries, and so did OpenCode until after its 1.0 (table below).
- **npm package.** The #706 launcher pattern carries over: add `"win32-x64": "plotkeeper-win32-x64"` (and arm64) and `plotkeeper.exe`. Claude Code's npm package ships `win32-x64` and `win32-arm64` platform packages ([setup](https://code.claude.com/docs/en/setup), **verified**). The launcher forwards signals with `child.kill(signal)` (`npm/launcher.js:40`); on Windows `SIGTERM` and `SIGINT` "terminate the process forcefully and abruptly" ([Node child_process](https://nodejs.org/api/child_process.html), **verified**), so a stop sent to the launcher alone would skip plotkeeper's clean stop. Ctrl+C in a console reaches both, as on Unix (**unverified** on Windows, medium-high).
- **CI.** Standard GitHub-hosted runners are free "In public repositories" ([Actions billing](https://docs.github.com/en/billing/concepts/product-billing/github-actions), **verified**), and this repo is public (`gh repo view`). So a third OS costs wall time, not money. The private-repo rates on the same page are USD 0.010 a minute for Windows 2-core against 0.006 for Linux 2-core and 0.062 for macOS. Public Windows runners: `windows-latest`/`windows-2025`/`windows-2022` (x64) and `windows-11-arm`, each 4 CPU and 16 GB ([runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), **verified**). `shell: bash` on Windows uses "the bash shell included with Git for Windows" ([workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax), **verified**), so `pack.sh` and `smoke.sh` can run there. nextest ships Windows x86_64 and aarch64 builds ([nextest install](https://nexte.st/docs/installation/pre-built-binaries/), **verified**).
- **Snap fixtures** render the page in Chrome on macOS (`snap.yml:28`) and need no Windows job; the page is the same page (judgement).

## Options

### (a) WSL2 only

What it is: the Linux build inside a WSL2 distribution, with the agents in the same distribution. The README already names it, as "untried" (`README.md:57`).

- **Works, per the docs:** a Windows browser reaches a WSL2 server "using `localhost` (just like you normally would)" in the default NAT mode; mirrored mode (Windows 11 22H2 and later) lets both sides use `127.0.0.1` ([WSL networking](https://learn.microsoft.com/en-us/windows/wsl/networking), **verified**). Whether NAT mode forwards a server bound to `127.0.0.1` rather than `0.0.0.0` is still not stated there (**unverified**, medium-high; the prior note's open point at `cross-platform.md:169`). Background runs: Claude Code's sandbox runs in WSL2, not WSL1 ([sandboxing](https://code.claude.com/docs/en/sandboxing), **verified**). Claude Code, Codex, Cursor's CLI and OpenCode all document WSL as a supported or recommended setup (their pages above, **verified**).
- **Does not work:** agents that run on the Windows side (native Claude Code, Cursor's desktop app on a Windows folder, Copilot in PowerShell) write their hooks under `%USERPROFILE%` and are not recorded; API keys, as WSL2 has no Secret Service by default (the README says so); `xdg-open` is usually absent, so plotkeeper prints the address (`server/src/bin/plotkeeper.rs:44-46`). Recording Windows-side agents into a WSL plotkeeper would need a Windows hook writing across `\\wsl$`, whose locking is unknown; not recommended (judgement).
- **Missing today:** one real trial on a Windows machine, a README section with exact steps, and the npm launcher's message on Windows, which now says "it runs on macOS, and on Linux on x64 and arm64" (`npm/launcher.js:25`) and could point to WSL2.
- **Effort** 1 to 2 days (judgement, medium), most of it the trial, which needs a Windows machine Daniel does not have here. **Risk** low. **Maintenance** none beyond Linux.
- **Moved:** the trial is harder than it looked. Daniel's Mac is Apple silicon (`uname -m`: arm64, M5). Microsoft's page on Windows 11 for these Macs names Parallels Desktop as the authorized VM, running Windows 11 Arm with x64 emulation. It also says "Experiences that depend on an additional layer of virtualization (nested virtualization) aren't supported", and it names WSL among them ([Windows 11 on Apple silicon Macs](https://support.microsoft.com/windows/options-for-using-windows-11-with-mac-computers-with-apple-m1-m2-and-m3-chips-cd15fd62-9b34-4b78-b0bc-121baa3c568c), **verified**). So WSL2 cannot be tried in a VM on this Mac. A GitHub `windows-latest` runner ships WSL2 3.0.5.0 with no distribution, but user reports say WSL2 jobs freeze in about 1 to 2 percent of runs ([runner-images#14938](https://github.com/actions/runner-images/issues/14938), user reports). The spike's `wsl2` job tries it under a time cap.

### (b) Native Windows, full parity

Everything in (c), plus the launcher on Job Objects (D1), the run's lock (D2), and background runs, micro-worlds and back a law on native Windows. The blocker is D5: with Claude Code there is no sandbox on native Windows, so parity means adding a second run harness with its own Windows sandbox (Codex's is the documented one) and redoing the sandbox gate and its security reasoning (#395, #411) for it. **Effort** (c) plus 10 to 20 days, vendor-dependent (judgement, low). **Risk** high: a security boundary built on a sandbox plotkeeper has never measured.

**Moved:** (c) plus 5 to 14 days, not blocked (judgement, low). The 10 to 20 days rested on one sentence, "a second run harness and a new security review", with no breakdown. The macOS gate (#395) took 19 hours from ticket to close, and the Linux gate run (#775) 16 hours, so a gate is about a day once a machine exists. The routes and their parts are under "Sandboxing on Windows". **Risk** stays high: every route is alpha or new, and each needs a gate trial on a real Windows machine with admin rights.

### (c) Native Windows, reduced scope

Native recording of sessions, the page, the map and the CLI writers; background runs, micro-worlds and back a law refuse with a clear message; API keys refused at first (as on headless Linux) or in the Credential Manager after the rule change.

- **Works after the work:** recording for the harnesses whose Windows hook form is confirmed in a trial, the read loop and page, the map's extraction (git-driven), fingerprints and briefs.
- **Does not:** background runs and what uses the launcher; agy until its hook is redesigned; any harness whose trial fails.
- **Effort**, judgement, agent-assisted work days including review and a trial per harness:

| Area | Days | Confidence |
|---|---|---|
| Compile: a platform module, Windows stubs that refuse, 60 shipping errors | 2-3 | Medium |
| Process runner on Job Objects (D1, runner only) | 1-2 | Medium |
| Event log lock file and identity (D2) | 1-1.5 | Medium |
| Hook commands, 8 harnesses, plus trials (D3) | 2-4 | Medium-low |
| Agent pid walk (D7) | 1-1.5 | Medium |
| Program lookup, `.cmd` shims (D10) | 1-2 | Medium |
| Binary refresh by version (D6) | 0.5-1 | Medium-high |
| Private files, answer file, random (D8) | 1-1.5 | Medium |
| Paths at the boundary, case, UI helpers (D4) | 2-4 | Low |
| Folders, `.exe` names, opener, refusals (rows 1-3, 16, 23) | 1 | High |
| API keys: refuse (or Credential Manager, +1) (D9) | 0.25-1 | High |
| Tests: 107 compile errors, 36 shell fakes, then run-time failures | 3-5 | Low |
| CI job, `.gitattributes`, package, smoke test | 1.5-3 | Medium |
| Docs, `system-map.json`, AGENTS.md | 0.5-1 | High |
| **Total** | **about 18-31 days (4 to 6 weeks)** | |

This is above the prior note's "3 to 5 more weeks" (`cross-platform.md:200`), for three reasons found on main: the process runner (#548) now carries every `git` call, so Job Objects are no longer optional even without runs; there are eight harnesses, not three; and `.cmd` shims, console windows, CRLF and Smart App Control were not counted.

**Moved:** this table is the first round's, kept for comparison. Its 18 to 31 days are effort units for all eight harnesses. "The estimate, split into work and waiting" below re-estimates a core v1 at 13 to 23 days, and turns effort into wall clock using #510's actual history.

- **Risk:** medium-high. Most failures are silent (a hook that never runs, a pid that is a shell, a path dropped as not absolute), so trials on real Windows matter more than green tests.
- **Maintenance:** a third CI OS on every PR (free here, but Windows runs are slower); every new feature that starts processes, touches files' modes or reads agent paths needs a Windows arm; the suite's habit of `#!/bin/sh` fakes must change to Rust fakes in `test-support`, or new tests keep arriving Unix-only. My guess is 10 to 20 percent more effort on such features (judgement, low).

## Demand

- **Developers on Windows.** Stack Overflow's 2025 survey asked "What is the primary operating system in which you work?" (31,569 respondents, more than one answer allowed). For professional use: Windows 49.5%, macOS 32.9%, Ubuntu 27.7%, WSL 16.8%, other Linux (non-WSL) 16.7% ([2025 results, Technology](https://survey.stackoverflow.co/2025/technology), read from the page's data on 2026-11-28, **verified**). The 2026 results page loaded but its data held no operating-system question (**unverified** whether it was asked). The WSL share is a third of the Windows share, which bounds what option (a) reaches (my reading).
- **The agents plotkeeper targets run on native Windows**, per their own docs: Claude Code (Windows 10 1809+, sandbox not supported), Codex (native, with its own sandbox), Cursor (desktop, and a native CLI install), Copilot CLI (PowerShell, Windows PowerShell 5.1 also; [install](https://docs.github.com/en/copilot/how-tos/set-up/install-copilot-cli)), agy (PowerShell and CMD installers), OpenCode (native, WSL recommended). Qwen Code and Cline: not found stated in the pages read (**unverified**).

## Prior art and security trade-offs (second round)

What comparable tools do on native Windows, read on 2026-11-28. For vendor source, the commit or tag read is given. Vendor GitHub issues are labelled user reports.

### Sandboxing on Windows

| Tool | What it does on native Windows | Source |
|---|---|---|
| Claude Code | No sandbox. "On native Windows, Claude Code runs commands unsandboxed". By default an unsupported platform runs unsandboxed with no prompt. `failIfUnavailable: true` makes it "exit at startup". Permission rules and modes still apply, as "complementary layers". No roadmap in the docs. | [sandboxing](https://code.claude.com/docs/en/sandboxing), [setup](https://code.claude.com/docs/en/setup), **verified** |
| Codex CLI 0.172.0 | A native sandbox in three modes: `mxc` (recommended, Windows 11 24H2 or 25H2 builds 26100.9278 / 26200.9278 and later), `elevated` (a sandbox user with WFP firewall filters; one admin setup), and `unelevated` (a restricted token, no kernel egress block). With no mode set, `workspace-write` silently becomes read-only rather than running unsandboxed. The unsandboxed opt-in is `--dangerously-bypass-approvals-and-sandbox` ("EXTREMELY DANGEROUS. Intended solely for running in environments that are externally sandboxed"). `codex sandbox [--permission-profile …] -- <cmd>` runs any command under it. | [Windows sandbox](https://learn.chatgpt.com/docs/windows/windows-sandbox), **verified**; source at `065e18b0`: `core/src/config/windows_sandbox_config.rs`, `config/src/config_toml.rs` (`effective_sandbox_mode`), `utils/cli/src/shared_options.rs`, `cli/src/main.rs`, **verified** by a delegated source read |
| Gemini CLI 0.71.1 | Sandbox off by default everywhere. On Windows, `sandbox=true` finds Docker or Podman only. A `windows-native` backend sets a Low integrity level on writable paths with `icacls`, and no network isolation is described. A requested sandbox that cannot start is a `FatalSandboxError`, so it fails closed. | `docs/cli/sandbox.md`, `packages/cli/src/config/sandboxConfig.ts` at `60f8082b`, **verified** (delegated read) |
| OpenCode | No OS sandbox at all: "OpenCode does **not** sandbox the agent. The permission system exists as a UX feature". | [SECURITY.md](https://github.com/anomalyco/opencode/blob/dev/SECURITY.md), [permissions](https://opencode.ai/docs/permissions), **verified** |
| Copilot CLI | "Local sandboxing is turned off by default", and it is built on Microsoft's MXC. On an unsupported host, "A notice explains that shell commands and services will run without a sandbox". Managed settings can make it fail closed. | [cloud and local sandboxes](https://docs.github.com/en/copilot/concepts/security-governance-and-network-settings/about-cloud-and-local-sandboxes), **verified** |
| Cursor CLI | Seatbelt on macOS and Landlock on Linux. Windows is not mentioned. | [run modes](https://cursor.com/docs/agent/security/run-modes), **verified** that the page is silent |
| srt (sandbox-runtime) 0.0.83, 2026-11-13 | "**Windows**: Alpha — uses a bundled `srt-win.exe` helper". It runs the command as a dedicated `srt-sandbox` user, with "a Windows Filtering Platform (WFP) egress fence keyed on the sandbox account's SID, a write-restricted token, and explicit ACEs". It needs a one-time elevated `npx @anthropic-ai/sandbox-runtime windows-install` (one UAC prompt). Each session, `initialize()` probes egress and "refuses to start if any of them gets out". Known limitations include unfenced anonymous-token impersonation, unfenced system-resolver DNS, and no Schannel TLS inside. About 30 open Windows issues (user reports), among them path and ACE bypasses (#837, #839). | [README](https://github.com/anthropic-experimental/sandbox-runtime) lines 680, 742, 749, 780; release via `gh api`, **verified** |
| Microsoft MXC | Microsoft says "Many others will be releasing support for MXC: Anthropic Claude Code, …", with no date. | [Windows Developer blog, 2026-10-07](https://blogs.windows.com/windowsdeveloper/2026/10/07/microsoft-execution-containers-policy-driven-containment-for-ai-agents/), **verified** (delegated read) |

**Is unsandboxed-behind-a-warning the norm? For interactive use, yes.** Every tool above lets a person at the keyboard run unsandboxed, by default or with a flag. **For unattended runs, no.** Codex's headless default falls back to read-only, not to unsandboxed writes. Gemini fails closed when a sandbox was asked for. Claude Code offers `failIfUnavailable`, which plotkeeper sets (`server/src/runner.rs:227`). plotkeeper's background runs are unattended and pre-approved: they may run the project's own checks (`allowed_tools`, `runner.rs:266`), which execute repository code. So the closest prior art supports keeping them refused. **Verified** for the tools' behaviour; the comparison is my reading, medium-high.

**What the first round's 10 to 20 days rested on:** one judgement, "a second run harness and a new security review, not a port" (D5, and option (b)), at low confidence. It assumed Codex was the only route, and it did not know that Codex downgrades to read-only instead of failing, or that srt's Windows backend refuses to start on a leaky fence. Routes to parity, re-estimated (judgement, low):

| Route | Parts | Effort | Main risk |
|---|---|---|---|
| srt-win around the whole `claude -p` run | Launcher on Job Objects (1), the run lock taken by the launcher (0.5 to 1), srt-win in the runner, plus a readiness check (1 to 2), a Windows gate run on a real machine with admin (1 to 2), security write-up (1) | 4.5 to 7 | Alpha backend; known fence gaps; not documented for Claude Code on Windows. Claude Code 2.1.265 fixed file tools "when running inside an AppContainer or restricted-token sandbox", so someone runs it that way (CHANGELOG, **verified**) |
| Codex as a second run harness | The parts above, plus a `Harness` for `codex exec` (the seam exists: `runner.rs:197-200`), parsing its output, and requiring `windows.sandbox="mxc"` or a passing `codex sandbox -- <probe>` before a run, since the default is a silent downgrade | 7 to 12 | A second harness on every platform; MXC needs recent Windows 11 builds |
| Wait for Claude Code on MXC | Then the macOS gate's approach, with a Windows probe | 3 to 5, once shipped | No date |
| Back a law only, through srt-win | srt is already its sandbox, and srt-win fails closed | 1.5 to 3 | Alpha; Schannel TLS fails inside, which matters only for tests that need the network |

### Code signing, SmartScreen and Smart App Control

Windows executables in the published npm packages, checked on 2026-11-28 by reading each PE file's Authenticode certificate table in the downloaded tarball. Nothing was run. The table shows whether a signature block is present and the leaf certificate's subject; the signature was not validated cryptographically.

| Package (version) | Windows binary | Signed? | Signer |
|---|---|---|---|
| `@esbuild/win32-x64` 0.28.2 | `esbuild.exe` | No | - |
| `@biomejs/cli-win32-x64` 2.6.9 | `biome.exe` | No | - |
| `turbo-windows-64` 2.8.17 | `turbo.exe` | No | - |
| `@rollup/rollup-win32-x64-msvc` 4.68.9 | `rollup.win32-x64-msvc.node` | No | - |
| `opencode-windows-x64` 0.1.0 (2025-06-12) and 1.0.0 (2025-10-31) | `opencode.exe` | No | - |
| `opencode-windows-x64` 1.19.6 | `opencode.exe` | Yes | Anomaly Innovations, Inc, through Microsoft ID Verified CS EOC CA 04 |
| `@openai/codex` 0.172.0-win32-x64 | `codex.exe` and helpers | Yes, except the bundled `rg.exe` | OpenAI OpCo, LLC, through Microsoft ID Verified CS AOC CA 04 |
| `@github/copilot-win32-x64` 1.0.117 | `copilot.exe` | Yes | GitHub, Inc., through Microsoft ID Verified CS AOC CA 03 |
| `@anthropic-ai/claude-code-win32-x64` 2.1.332 | `claude.exe` | Yes | Anthropic, PBC, through DigiCert Trusted G4 Code Signing |

**Verified** (command output). So build tools ship unsigned, while agent vendors sign. OpenCode shipped unsigned through its 1.0 and signed later. The "Microsoft ID Verified" chains are Microsoft's managed signing service (Artifact Signing), my reading, high.

When each Windows check bites:

- **Mark of the Web is written only by programs that choose to.** `IAttachmentExecute::Save` "may attach evidence to the local path in its NTFS alternate data stream" ([Save](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-iattachmentexecute-save), **verified**). The examples Microsoft gives are "browser downloads or email attachments" ([Office MOTW](https://learn.microsoft.com/en-us/microsoft-365-apps/security/internet-macros-blocked), **verified**). A code search found no `Zone.Identifier` in npm/cli, pacote or node-tar (delegated search, default branches only). So the `.exe` that npm extracts carries no mark (**inference**, high; the spike's survey checks the stream).
- **SmartScreen** then has nothing to trigger on for `npx plotkeeper`. No Microsoft page says whether it checks a file with no mark when it is started from a terminal (**unverified**, about 80 percent that it does not prompt).
- **Smart App Control checks every binary as it loads.** "Because Smart App Control evaluates binaries as it loads them, be sure to test all code paths" ([test your app](https://learn.microsoft.com/en-us/windows/apps/develop/smart-app-control/test-your-app-with-smart-app-control), **verified**). "If the app is unsigned … Smart App Control will consider it untrusted and block it" when the cloud cannot predict, and there is "no way to bypass Smart App Control protection for individual apps" ([what is SAC](https://support.microsoft.com/en-us/topic/what-is-smart-app-control-285ea03d-fa88-4d56-882e-6698afdb7003), **verified**). A user report describes exactly plotkeeper's case: an npm-installed, unsigned `biome.exe` blocked by Smart App Control ([biomejs/biome#12126](https://github.com/biomejs/biome/issues/12126), user report).
- **When it is on.** It starts in evaluation mode on Windows 11 22H2 and later, and "switches off within 48 hours for enterprise managed devices unless the user turns it on first" ([App Control](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/app-control-for-business/appcontrol), **verified**). It also turns itself off for users it detects as developers ([overview](https://learn.microsoft.com/en-us/windows/apps/develop/smart-app-control/overview), **verified**). **Moved:** "clean install only" no longer holds. Windows 11 26H2: "You can turn Smart App Control (SAC) on or off without requiring a clean installation of Windows" ([What's new in 26H2](https://learn.microsoft.com/en-us/windows/whats-new/whats-new-windows-11-version-26h2), **verified**). One developer test page still calls it "a one-way operation"; it reads as stale.

### Local files and tokens

- **Microsoft's documented default ACL for `%LOCALAPPDATA%`: none found.** The nearest statements: SYSTEM gets Full Control on NTFS by default, and Administrators have Full Control on the device ([local accounts](https://learn.microsoft.com/en-us/windows/security/identity-protection/access-control/local-accounts), **verified**). The usual reading (SYSTEM, Administrators and the user, no other users) is **unverified**, high. The spike prints `icacls` for it on both runner images.
- **What other tools do: no custom DACL when they first create a file.**
  - Claude Code: "On Windows, credentials are stored in `%USERPROFILE%\.claude\.credentials.json`", and the file inherits "the access controls of your user profile directory, which restricts the file to your user account by default" ([authentication](https://code.claude.com/docs/en/authentication), **verified**).
  - Codex: `auth.json` inherits on first write. On a replace it copies the old file's DACL and protects it from broader inheritance (`codex-rs/login/src/auth/storage_file_windows.rs` at `065e18b0`, **verified**, delegated read).
  - gh: writes config with mode `0600` (`cli/go-gh` `pkg/config/config.go:353-358` at `4e83b33c`), and on Windows Go uses "only the 0o200 bit" ([os](https://pkg.go.dev/os)), so the file inherits. **Verified** (delegated read).
  - Gemini CLI: `oauth_creds.json` with `mode: 0o600`, so it also inherits on Windows (**inference**, high).
- **Where tokens live on Windows:**
  - Claude Code: a plaintext file (above).
  - Codex: `cli_auth_credentials_store` is `file | keyring | auto | ephemeral`, and `auto` "uses the OS credential store when available, otherwise falls back to `auth.json`" ([auth](https://learn.chatgpt.com/docs/auth)). The source default is `file`.
  - gh: "stored securely in the system credential store", falling back to a plain text file ([gh auth login](https://cli.github.com/manual/gh_auth_login)).
  - Copilot CLI: "your operating system's keychain under the service name `copilot-cli`", with a prompt to fall back to plaintext ([authenticate](https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/authenticate-copilot-cli)).
  - Gemini CLI: a plaintext file unless forced to encrypt (source at `60f8082b`).
  - All **verified**.

### The concrete risk of each relaxation in v1

| Protection | Relaxed or deferred how | Attacker or failure | What leaks or breaks | Likelihood (judgement) | Verdict |
|---|---|---|---|---|---|
| Background-run sandbox (D5) | Run unsandboxed behind an opt-in | Prompt injection in what the agent reads, or a malicious dependency run by an allowed check command (`runner.rs:266`) | The run's tools read `%USERPROFILE%\.claude\.credentials.json` (plaintext on Windows), `~/.ssh`, cloud keys, and send them out. Today `denyRead` and the network fence stop that (`runner.rs:222-238`) | Low per run, but runs are unattended and repeated; high impact | **Keep refusing** on native Windows; WSL2 for runs. Revisit with srt-win out of alpha, or Claude Code on MXC |
| Code signing | Ship unsigned | Smart App Control enforcing on the user's machine | No leak. `plotkeeper.exe` or a hook binary is blocked; a blocked hook means nothing is recorded, silently | Low to medium: most developer machines leave evaluation mode as off, but it can now be turned on at any time | **Defer** signing. Add a start-time self-test that spawns `plotkeeper-hook.exe` and reports a block, plus a README line |
| SmartScreen | Nothing to do | - | Nothing, if npm writes no mark (inference above) | Low | **Accept**; the spike confirms |
| Answer and token files (D8) | Inherited profile ACL instead of a custom DACL | Another non-admin user on a shared machine writes a forged answer file, which approves a permission prompt, or reads the server token | A forged approval | Low: needs a second local user, and the profile ACL is expected to keep them out. An admin or SYSTEM attacker already owns the machine, on Unix too | **Accept**, with the owner-SID check, no reparse points and the nonce. The spike prints the ACL |
| API keys (D9) | Refuse to store on Windows, as on headless Linux (#510) | - | Nothing leaks. The API-key writers are unavailable; the CLI writers work | - | **Defer**. A plaintext fallback would expose the key to any process of the user. The Credential Manager protects it at rest but, as far as I know, not from other processes of the same user (**unverified**, medium) |

## Foundational changes: now or later

plotkeeper already migrates its own installs. Each start rewires a wired harness whose fingerprint changed, without asking (`server/src/install.rs:822-835`, `:919-990`). It also refreshes the hook copies from the binaries beside it (`server/src/hook_copies.rs:24-48`, called from `server/src/bin/plotkeeper.rs:170`). So changing a hook entry or a binary later costs users little, with one exception: every change to Codex's entries makes Codex skip them until the user trusts them again (`install.rs:666`). The real one-way doors are elsewhere: npm names someone else could take, and the test suite's growing Unix habit. **Verified** for the code; the weighing is my judgement.

| Change | Safe on macOS and Linux now? | Cost to migrate once users exist | Size now (judgement) | Do now? |
|---|---|---|---|---|
| Separate `events.lock` beside the log, taken by every writer (`hook/src/lib.rs:125-145`; `server/src/lib.rs:573`) and the prune (`:306`) | Yes: `flock` on a second file behaves the same. It also removes the `(dev, ino)` loop, since the log is replaced only under the lock | Low to medium. During an upgrade an old hook (locking the log) and a new prune (locking `events.lock`) would not exclude each other, which could lose a line; one transition release that locks both closes it | 0.5 to 1 day, with the `docs/agents/domain.md` contract and the log's `@law` tests | **Yes**: it removes a Windows design item and a nightly-only API need |
| Versioned `bin/<version>/` | Yes, but it rewires every harness on each upgrade, and Codex re-asks for trust each time (`install.rs:761-762`, `:666`) | n/a | 0.5 to 1 day | **No.** Use rename-aside on Windows only (D6, Moved) |
| Exec-form hook entries | Claude Code: yes, since 2.1.139 (May 2026), in plugins too. Copilot: `exec` exists. Codex and Cursor: no exec form | Low: rewired at the next start. For Codex the cost is a re-trust per user per change, which is the one item that is cheaper now (Daniel is the only user) | 0.25 to 0.5 day for Claude Code | **Yes for Claude Code.** Leave Codex alone unless the spike shows `cmd /C` fails |
| `.gitattributes` with `* text=auto eol=lf` | Yes; the repo has none, and it is LF today | None (repository-only) | Under an hour | **Yes**, before any Windows checkout compares a fixture |
| Rust fakes instead of `#!/bin/sh` fakes | Yes | None for users; it is test debt. The count grew from 25 fakes in 13 files at `89b4fc3f3` (25 Nov, `cross-platform.md:51`) to 36 in 20 files at `3d44062aa` (28 Nov, line 40 above): about 11 in three days | Helper plus a rule: 0.5 day. Converting all: 1.5 to 3 days | **The helper and the rule now** (in `test-support`, and a line under AGENTS.md "Tests"); convert the rest in the port |
| npm names `plotkeeper-win32-x64` and `plotkeeper-win32-arm64` | n/a | One-way: a name taken by someone else is lost. Both returned E404 on 2026-11-28 (`npm view`) | 10 minutes for Daniel, a 0.0.0 placeholder as #706 did | **No.** npm refused the placeholder ("Package name triggered spam detection", 2026-11-28), and its policy calls a package with "no genuine function" squatted ([Dispute Resolution](https://docs.npmjs.com/policies/disputes)). Publish the names with the Windows build; a taken name costs a rename, since `plotkeeper`'s `optionalDependencies` choose what installs |
| Paths normalised at the hook boundary (D4) | It changes only Windows events; macOS and Linux paths stay as they are | Low: no Windows logs exist yet, and the frozen samples stay (`understanding/tests/fixtures/stored/`) | Part of the port | No; nothing to normalise until Windows runs |
| The run lock taken by the launcher; a stop file instead of `SIGTERM` (D1, D2) | Yes, but it changes how server and launcher talk on every platform | Low: an old launcher and a new server keep the same file lock | 1 to 1.5 days | No; only full parity needs it |
| A Windows `cargo check` in CI | A non-required job, red until the port | None | 0.25 day | After the spike, as the port's first ticket |

**Reassessing "wait for demand".** The first round argued that waiting costs nothing. That holds for most of the port, since plotkeeper rewires and refreshes its own installs. It does not hold for five cheap items: the npm names (one-way), the test fakes (debt growing weekly), Claude Code's exec form (it fixes quoting on every platform too), `events.lock` (it deletes a design problem), and `.gitattributes` (before any Windows checkout). Together they are 1.5 to 2.5 days. They need no Windows code and leave macOS and Linux behaviour unchanged. With them done and the spike run, the native port is about 11.5 to 20.5 effort days.

## The estimate, split into work and waiting

### Calibration: Linux (spec #510)

| Measure | Value | Source |
|---|---|---|
| Estimate | 5.5 to 9 agent-assisted days, "1 to 1.5 weeks" | `cross-platform.md:3`, `:200`; #510 "Further Notes" |
| Spec opened | 2026-11-25 08:40 UTC | `gh issue view 510` |
| Tickets cut (#699 to #710, 11 tickets) | 2026-11-26 22:41 UTC | `gh issue list --search "510 in:body"` |
| PRs merged | 2: #712 (CI job, #699), opened 22:47, merged 23:23; #714 (the other ten tickets, 32 commits, +1,996 / −314), opened 23:03, merged 2026-11-27 08:52 | `gh pr list`, `gh pr view 714` |
| Throwaway CI probe PRs | 3 (#713, #721, #737), closed | same |
| First ticket to last merge | **10 h 11 min** | computed |
| Spec to last merge | **48 h 12 min** | computed |
| Rework inside the run | 37 Linux test failures from BSD tool flags (#708); one hung release dry run (SIGINT reached only `npx`); 3 outside-review fixes and several correctness-pass fixes, all before merge | #714 body |
| Follow-ups | #709's Docker trial found "Defects found: none". #775 (the full gate on Linux, a planned trial) took 16 h and merged as #797, opening #794 and #795 on gate probes. No Linux bug was reopened | `gh issue view 709`, `775`; `git log` |

**Verified** (command output). So a "day" in these notes is about 1 to 2 hours of an unattended `/implement-spec` run: 10 hours against 44 to 72 working hours, a factor of 0.14 to 0.23. From spec to merge, grilling and review included, the factor is about 0.22 to 0.36 (two calendar days against 5.5 to 9 work days). Windows will not get the same factor, for three reasons:

- **No local build.** For Linux, failures surfaced in CI too (three probe PRs), but the trials ran in Docker on the Mac (#704, #709). For Windows every fix waits on a CI round.
- **Design items.** D1, D2 and D4 change how parts talk; Linux had none.
- **Trials need a Windows machine.** That is a Parallels VM on the Mac (Windows 11 Arm, with x64 emulation, no WSL2), a real PC, or CI.

I apply 1.5 to 2 times the Linux factor (judgement, low).

### CI latency today

From `gh run view` on 12 `studio.yml` runs on 2026-11-27 and 28: the `ubuntu` job took 107 to 153 s and the `macos` job 232 to 427 s. macOS queueing added up to 3.6 minutes. Whole runs took 5 to 9 minutes. **Verified** (command output). Windows has no measurement yet. nextest says Windows "is usually slower" and process creation "significantly slower" (above), and the runner has Defender's real-time scan off with `C:\` and `D:\` excluded ([Configure-WindowsDefender.ps1](https://github.com/actions/runner-images/blob/main/images/windows/scripts/build/Configure-WindowsDefender.ps1), **verified**, delegated read). My guess for a warm `npm run check` on `windows-latest` is 6 to 12 minutes (**unverified**; the spike's `compile` job gives a first number).

### Core v1: Claude Code, Codex and Cursor; keys refused; unsigned

Effort, in the note's units (judgement). The parallel group says which slices can run at once after slice 1.

| Slice | Days | Group | Change from the first round |
|---|---|---|---|
| 0. Foundations (`.gitattributes`, npm names, exec form, `events.lock`, fake helper) and the CI spike | 1.5 to 2.5 | before | New |
| 1. Platform module, refusals (keys included), folders, `.exe` names, opener, random; shipping code compiles; a red Windows CI job | 2 to 3 | first | Was 3 rows, 3.25 to 5 |
| 2. Process runner on Job Objects through `process-wrap` 11.0.0 (D1, runner only) | 0.5 to 1.5 | A | Was 1 to 2: its std `JobObject` plus `KillOnDrop` sets `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (`src/windows.rs:207-243`), and `wait` waits for the whole job, verified in its source |
| 3. Event log identity on Windows (D2) | 0.25 to 0.5 | A | Was 1 to 1.5; the lock file moved to slice 0 |
| 4. Answer file and private files (D8) | 0.5 to 1 | A | Was 1 to 1.5; no custom DACL |
| 5. Program lookup, `.cmd` shims (D10) | 0.5 to 1 | A | Was 1 to 2: Claude Code installs a native `.exe`; only npm installs need a shim lookup |
| 6. Agent pid walk (D7) | 1 to 1.5 | A | Same |
| 7. Paths at the boundary (D4) | 2 to 4 | A | Same, still the least predictable |
| 8. Hook entries for 3 harnesses and rename-aside (D3, D6) | 1 to 1.5 | B, after 5 | Was 2.5 to 5 for eight harnesses and versioned folders |
| 9. Tests to green: the rest of the shell fakes, Unix-only tests gated, Windows twins for laws | 2.5 to 4 | B, after A | Was 3 to 5 |
| 10. Package, release, smoke on the Windows runner (unsigned) | 1 to 1.5 | C | Was part of 1.5 to 3 |
| 11. Docs, `system-map.json`, README | 0.5 to 1 | C | Same |
| **Total** | **13 to 23 days** (11.5 to 20.5 without slice 0) | | **Moved:** from 18 to 31 days, by scope (3 harnesses, not 8), prior art (no custom DACL, `process-wrap` verified) and the rename-aside fix |

The critical path is 0, then 1, then 7 (the longest of group A), then 9 and 10: about 9 to 15 effort days. Group A's six slices are independent of each other, the same shape as #510's ten tickets that one run implemented together.

Wall clock (judgement, low):

| Part | Estimate |
|---|---|
| Agent coding | 13 to 23 days × 8 h × 0.14 to 0.23 × 1.5 to 2 ≈ 22 to 85 hours of orchestrated runs (#510's parallel implementers count once), so about 2 to 6 calendar days with Daniel's gates between them |
| Windows CI rounds | 10 to 25 rounds of 10 to 20 minutes, serial on the critical path: 2 to 8 hours, inside the above if implementers wait on the job |
| Trials CI cannot do (Cursor desktop and CLI with a login, console windows, Smart App Control, a real profile's ACL) | 0.5 to 1 day for Daniel to set up a Parallels VM, then about 1 day of trial |
| Daniel's review | 0.5 to 1.5 days spread over 2 to 4 PRs |
| **Total** | **about 1 to 2.5 weeks**, most likely about 1.5 |

**Moved:** the first round said "4 to 6 weeks", but those were effort days read as calendar weeks. The first round's 18 to 31 effort days, with the same calibration, would be about 1.5 to 3 weeks.

Deferred from core v1, each its own small spec: agy's hook redesign (1 to 2 days), Qwen, Cline, Copilot and OpenCode entries with their trials (1.5 to 3; OpenCode and Cline need only `.exe` and `windowsHide`), the Credential Manager (0.5 to 1, plus Daniel's rule change), and signing (about 1 day of setup plus identity validation and a monthly fee, the price **unverified**).

## What CI can settle without a Windows machine

The runner, per its image list for `windows-latest` (Windows Server 2025, image 20261123): Git 2.56.1.windows.1, Git Bash 5.3.15 at `C:\Program Files\Git\bin\bash.exe`, PowerShell 7.6.10, Node 22.23.5, Python 3.12.10, Rust 1.100.0 with MSVC (Visual Studio 2026, `VC.Tools.x86.x64`), "Windows Subsystem for Linux (WSLv1): Enabled" and WSL 3.0.5.0 with no distribution. cargo-nextest and cargo-machete are not listed ([Windows2025-VS2026-Readme.md](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-VS2026-Readme.md), **verified**, delegated read). Windows PowerShell 5.1 ships with the OS (**unverified**, high). GitHub says Windows VMs "run as administrators with User Account Control (UAC) disabled" ([runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), **verified**). From 2026-11-30 the `D:` drive is formatted ReFS, not NTFS ([runner-images#15315](https://github.com/actions/runner-images/issues/15315), a GitHub announcement, **verified**), so NTFS facts must be probed on `C:`. `windows-11-arm` is a Windows 11 client image, free here.

| Open item | CI settles it? | How (spike job and probe) |
|---|---|---|
| A rename over open handles (the prune's swap) | Yes, on NTFS (`C:`) and on ReFS (`D:`; Dev Drive users have ReFS too) | `probe`: `rename-over-open-handles` |
| `LockFileEx` on an append-only handle; reads blocked while locked; a lock file beside the log | Yes | `probe`: `lock-append-only`, `lock-blocks-reader`, `lock-file-beside-log` |
| The run lock not following an inherited handle | Yes | `probe`: `lock-not-inherited` |
| Same-file identity on stable Rust | Yes | `probe`: `same-file-identity` (`GetFileInformationByHandle`) |
| Replacing a running `.exe`; rename-aside | Yes | `probe`: `running-exe` |
| `QueryFullProcessImageName` and the pid walk through Git Bash, pwsh, Windows PowerShell and cmd; an orphan's stale parent id | Yes | `probe`: `walk-*`, `orphan-parent` |
| The walk under a real agent | Yes for Claude Code and Codex | `hooks`: each hook entry records its own process chain |
| `process-wrap` for `run_logged` (output to a file, a deadline) and kill-on-close, nested inside the runner's job | Yes | `probe`: `job-deadline-with-output`, `job-tree` (also reports `IsProcessInJob`) |
| `.cmd` shims and `Command::new` | Yes | `probe`: `cmd-shims` |
| `/dev/null` under Git for Windows | Mostly settled already: Git's Windows layer maps the exact path `/dev/null` to `nul` (`compat/mingw.c:863`, git master, **verified**). CI checks both of plotkeeper's uses | `survey`: `hash-object -t tree /dev/null`; a commit with `core.hooksPath=/dev/null` skips a failing hook |
| Line endings at checkout | Yes | `survey`: `core.autocrlf` origin and `git ls-files --eol` |
| `%LOCALAPPDATA%` default ACL | Partly: the runner is an admin with UAC off; the Windows 11 Arm image is closer to a user's machine | `survey`: `icacls` on the profile, `%LOCALAPPDATA%` and a new file there |
| Mark of the Web on files npm extracts | Yes | `survey`: alternate streams of an npm-installed `esbuild.exe`, against `Invoke-WebRequest` and `curl.exe` |
| Claude Code hook forms (exec, shell in Git Bash, quoted path in PowerShell, `&` form) | Yes, with no login: `-p` with a dummy `ANTHROPIC_API_KEY` and `ANTHROPIC_BASE_URL` pointing at a local mock. `-p` "treats the folder as trusted, so hooks … run"; `--bare` must not be used ([hooks](https://code.claude.com/docs/en/hooks), [headless](https://code.claude.com/docs/en/headless), [env vars](https://code.claude.com/docs/en/env-vars), **verified**, delegated read) | `hooks` (claude) |
| Codex hook forms (`command`, `commandWindows`) | Yes, with no login: a custom provider (`requires_openai_auth` false by default, `wire_api = "responses"`) and `--dangerously-bypass-hook-trust`, "Intended only for automation". Codex's own tests drive hooks against a mock the same way (`codex-rs/utils/cli/src/shared_options.rs:61-64`, `core/tests/suite/hooks.rs` at `065e18b0`, **verified**, delegated read) | `hooks` (codex) |
| Cursor and Copilot hook forms | No for Cursor (it needs an account: `CURSOR_API_KEY` or a browser login). Maybe for Copilot through BYOK and `COPILOT_OFFLINE` (**unverified**) | Later round, or the VM |
| A server bound to `127.0.0.1` in WSL2 reached from Windows | Maybe: WSL2 is installed, but nested virtualization is not documented for Windows runners and WSL2 jobs freeze about 1 to 2 percent of the time (user reports) | `wsl2`, capped at 20 minutes |
| Compiling natively, `ring` included | Yes | `compile`: error count per file |
| Console windows flashing under a GUI host; Ctrl+C reaching both the launcher and plotkeeper | No: the runner has no interactive desktop | VM |
| SmartScreen and Smart App Control prompts | No: Server 2025 has no Smart App Control, and the client image's state is unknown (the survey prints it) | VM with Smart App Control turned on, now possible without reinstalling |
| Real logins: Cursor desktop, Codex's MXC or elevated sandbox setup, the srt-win UAC install | No | VM or PC with admin |

**The spike draft** is in `/private/tmp/claude-501/-Users-danielrispler-work-plotkeeper/e1da018e-6d47-4b9e-a89d-69b0fddf86c3/scratchpad/windows-ci-spike/`. It has five parts:

- `windows-spike.yml`, with five jobs: `survey` and `probe` on `windows-latest` and `windows-11-arm`, plus `compile`, `hooks` (claude, codex) and `wsl2`.
- `probe/`, a standalone Rust crate. Its main file is 13 probes that print one JSON line each, plus the child roles they spawn. It type-checks for `x86_64-pc-windows-msvc` with `cargo check` on this Mac.
- `survey.ps1`.
- `hooks.ps1`.
- `mock-llm.mjs`, a streamed Messages and Responses stand-in.

It runs on a push to a throwaway `spike/windows-ci` branch, which is Daniel's call; nothing was pushed. Cost (judgement): 50 to 80 runner minutes, free on this public repo. Wall clock is 20 to 30 minutes, the cold `compile` job being the longest. Allow one or two fix rounds for the mocks' event shapes, so 1 to 2 hours in all, plus about half a day of agent time to read the results into this note.

## CI spike results

The spike ran once on 2026-11-28: [run 46141976334](https://github.com/danielrispler/plotkeeper/actions/runs/46141976334), from `origin/main` plus the spike files. The branch was then deleted, but the run's logs and artifacts stay on GitHub. One change from the draft: the probe got its own `[workspace]`, since the repository's workspace would otherwise claim it. Every row below is **verified** by that run's output unless it says otherwise.

| Item | Result |
|---|---|
| Rename over open handles | Works on `C:` and `D:`, on x64 and arm64. The open reader keeps the old bytes, and the path shows the new ones. `D:` was still NTFS on 11-28. |
| `LockFileEx` | On an append-only handle the lock fails ("I/O error"). With read added it locks. While another process holds the lock, a reader of the same file gets error 33 (`ERROR_LOCK_VIOLATION`). So the lock is mandatory, as the docs say. |
| `events.lock` beside the log | The design works: the log reads while `events.lock` is held, and a second `try_lock` gets `WouldBlock`. |
| Run lock and an inherited handle | The lock went away when the locker exited, although its child still held the handle. Unix keeps it, so on Windows the run lock follows plotkeeper, not the launcher, as the note said. |
| Replacing a running `.exe` | A rename over it fails (error 5). A move aside, then placing the new file, both work. Deleting the running copy fails (5), and deleting it after exit works. This confirms the rename-aside design. |
| Same-file identity | `GetFileInformationByHandle` matches two handles of one file and changes after the swap. |
| Process walk | `QueryFullProcessImageName` gives full paths for `pwsh.exe`, `powershell.exe` and `cmd.exe`. An orphan's parent id names a process that is gone. |
| **`Command::new("bash")` from Rust** | **New finding.** It runs `C:\Windows\System32\bash.exe`, WSL's launcher, which printed "Windows Subsystem for Linux has no installed distributions". It does not run Git Bash, although `Get-Command bash` in PowerShell finds Git Bash first: Windows looks in System32 before `PATH`. So any Rust code or test that spawns `bash` or `sh` by bare name gets the wrong program on Windows. On arm64 the same call printed nothing. |
| Job Objects (`process-wrap`) | Nesting inside the runner's job works (`IsProcessInJob` true), and dropping the wrapper killed the probe's own child processes. **Not settled:** the `bash` grandchild and the `run_logged` case ran WSL's `bash.exe` (above), which exited at once. A second round needs Git Bash's full path. |
| `.cmd` shims | `Command::new("npm")` fails with `NotFound`; `npm.cmd` works. |
| `/dev/null` under Git for Windows | Both uses work: `hash-object -t tree /dev/null` gives the empty tree, and `core.hooksPath=/dev/null` skips a failing hook. |
| Line endings | With no `.gitattributes`, checkout gave 794 files CRLF (`core.autocrlf` from Git for Windows' system config). This confirms the `.gitattributes` change. |
| ACLs | On both images, `%USERPROFILE%` and `%LOCALAPPDATA%` grant full control to SYSTEM, Administrators and the user only, and a new folder and file there inherit the same three. The runner is an admin with UAC off, so a real user's profile is still open, though the inheritance is now observed. |
| Mark of the Web | None of npm, `Invoke-WebRequest` or `curl.exe` set a `Zone.Identifier` stream. npm setting none is now observed rather than inferred. |
| Defender and Smart App Control | Server 2025 x64: real-time protection off, SAC off. Windows 11 arm64: real-time protection on, SAC off. Neither image says anything about SAC on a user's machine. |
| Native compile | `ring` and the other dependencies build. The workspace stops at `studio-workbench` (15 errors, all in `workbench/src/run.rs`) and `studio-test-support` (4 in the lib, 7 more in its tests), the crates the rest depend on. The job's error counter printed 0 because of colour codes; I counted the list by hand. |
| **WSL2** | **Settled for the runner.** Ubuntu 24.04 ran under WSL2 (kernel `6.18.58.1-microsoft-standard-WSL2`). A server bound to `127.0.0.1` inside WSL answered from Windows on `localhost` and on `127.0.0.1` (200 and 200). So option (a)'s open point holds in WSL's default networking. A user's own networking settings are untested. |
| Claude Code 2.1.332 hooks | The exec form ran: SessionStart, UserPromptSubmit and SessionEnd were recorded, with `claude.exe` as the hook's direct parent. The shell form ran through Git Bash (`bash.exe` > `bash.exe` > `claude.exe`). A quoted path under `shell: powershell` did not run, as the docs say, and the `& "…"` form did. Stop never fired: the mock API got no request (no `POST` in its log), and Claude Code gave up after 10 retries, exit 1. Why the mock was not reached is **unverified**. |
| Codex 0.172.0 hooks | **Not settled.** Both entries, `command` alone and `command` plus `commandWindows`, ended "SessionStart Failed" and "UserPromptSubmit Failed", and nothing was recorded. The mock was not reached here either ("Reconnecting... waiting for network"), and the job hit its 15-minute cap and was cancelled. Codex's logs held no reason. A likely cause is `cmd.exe /C`'s rule that strips the outer quotes when a command starts with `"` and holds more quotes; both commands start with a quoted path (**unverified**, medium). |

**What moved:**
- The lock, rename, identity, walk and running-exe designs are confirmed, on both architectures.
- WSL2 localhost works, on the runner.
- Claude Code's exec form works on native Windows.
- New risk: bare `bash` or `sh` from Rust is WSL's launcher, so the port must resolve Git Bash by path or never use a shell.
- Still open: Codex's hook form and Job Objects with a real grandchild. A second spike round settles both: Git Bash by full path, a Codex entry with no leading quote, and the mock started in the same step as the agent.

## Claude Code's exec form and older versions

Claude Code before 2.1.139 does not know `args`. Its hook schema, read from the strings in the `@anthropic-ai/claude-code-linux-x64` 2.1.138 binary, is a zod object of `type`, `command`, `if`, `shell`, `timeout`, `statusMessage`, `once`, `async`, `asyncRewake` and two internal fields. 2.1.139's adds `args: array(string).optional()`, described as "When present, `command` is resolved as an executable and spawned directly with these arguments — no shell … When absent, `command` runs through a shell" (**verified**, both binaries' strings; nothing was run).

What an older version does with an exec-form entry: the object is not `.strict()`, so zod drops the unknown `args`, and `command` runs through a shell (**inference**, high). An exec-form `command` is the bare path, unquoted, and plotkeeper's hook binaries live in `data_dir/bin` (`hook/src/lib.rs:84-85`). On macOS that is `~/Library/Application Support/plotkeeper/bin`, which has a space. So:

- **macOS, Claude Code < 2.1.139:** the shell splits the path at the space and the hook never runs. Recording stops with no error (**inference**, high).
- **Linux, Claude Code < 2.1.139:** the default `~/.local/share/plotkeeper/bin` has no space, so the hook runs, but without `--agent claude` (`hook/src/main.rs:51-56`). Lines then carry no `studio_agent` and are read by shape (#355) (**inference**, medium).
- No one `command` string serves both. Quoting it for the old shell would make the new exec form look for a program whose name includes the quotes (**inference**, medium-high).
- plotkeeper does not read agent versions today: no `--version` call in `server/src/install.rs` (**verified**, grep).

How likely an older version is: 2.1.139 came out on 2026-05-11 (`npm view @anthropic-ai/claude-code time`). This Mac has 2.1.326, and the runner installed 2.1.332. Claude Code updates itself by default, so an old version means auto-update was turned off or the version pinned (**unverified** how common that is).

Three ways for the spec:

1. **Exec form on Windows only.** macOS and Linux keep today's quoted shell form, which works there. This carries no risk to existing setups and keeps the "no macOS or Linux behaviour change" promise of the foundation step. It costs a per-OS template. **Recommended.**
2. Exec form everywhere, with plotkeeper reading `claude --version` at install and start and writing the shell form below 2.1.139. More code, and a version probe on every start.
3. Exec form everywhere, and require 2.1.139 or later. Simplest, but recording silently stops for anyone on an older pinned version.

**Moved:** the second round's foundation list said exec form "fixes quoting on every platform too" and could land now with no macOS or Linux change. That holds only with option 1, or with a version floor.

## Recommended path

**Moved:** the first round said "(a) now, (c) only on a demand signal". The demand gate rested on 4 to 6 weeks of cost. Calibrated, the core is 1 to 2.5 weeks, and Daniel has said Windows matters.

1. **Now, no Windows code (1.5 to 2.5 days):** `.gitattributes`; Claude Code's plugin on exec form; `events.lock` with its contract and law tests; a Rust fake helper in `test-support` and a line in AGENTS.md "Tests" so that no new `#!/bin/sh` fakes are added.
2. **Done 2026-11-28: the CI spike ran once** ("CI spike results"). A second round would settle Codex's hook form and Job Objects with a real grandchild.
3. **Now: README "Windows" section** pointing to WSL2 for background runs, marked untried, and the npm launcher's message on `win32` pointing there.
4. **Next, when Daniel chooses: spec (c), the core v1**, in the slice order above. Background runs, micro-worlds and back a law refuse on native Windows with their own message. A Parallels VM for the trials CI cannot do.
5. **Later, each on its own:** the other five harnesses, the Credential Manager, signing (before any wider announcement, or when a Smart App Control report arrives), and parity through srt-win once it leaves alpha or Claude Code ships MXC support.

## Decisions that are Daniel's

- **Scope:** WSL2 only, native core v1, or full parity; and whether arm64 Windows is in. The VM on his Mac is arm64, so arm64 is the build he can try.
- **Harnesses on native Windows at first:** Claude Code, Codex and Cursor (this note's core), or more.
- **Background runs on native Windows:** refused (recommended), or an unsandboxed opt-in, which the closest prior art does not offer for unattended runs.
- **API keys:** refuse on Windows (as headless Linux), or store them in the Credential Manager, which needs the AGENTS.md hard rule to name it and reopens #510's "no `keyring` crate".
- **CI:** pushing the spike branch; later a third required job on every PR, free for this public repo and slower than the others.
- **Signing:** ship v1 unsigned (build tools do; agent vendors sign), or pay for Artifact Signing now.
- **A Windows machine for trials:** a Parallels VM on the Mac (Windows 11 Arm; no WSL2 inside), a PC, or CI only.
- **The five cheap foundation changes, the npm names among them,** which are worth doing before any Windows decision.

## Updates to `cross-platform.md`

- `:3` says "24 errors in `studio-hook` alone" and `:19` says 21; the hook had 21 then and has 21 now. The first failing crate is now `studio-workbench`, and `studio-wayfinder` compiles.
- `:19`: 48 Unix-only lines in `src/` is now 36 in shipping code, plus 5 in test-only code.
- `:36` (row 11): the `/bin/zsh` fallback is `/bin/sh` since #510; Windows still has neither.
- `:31` (row 6): Linux reads `/proc` now; Windows falls into the `ps` arm (`hook/src/lib.rs:1317`).
- `:113` (W1): the Windows `MetadataExt` identity methods are confirmed nightly-only.
- `:134` (W4): Copilot (`exec`, `powershell` fields) and Qwen Code 1.x (`args`, `shell`) are now read; agy's docs are silent; OpenCode and Cline, added since, use Node `spawn` with no shell. Codex runs hooks through `cmd.exe /C` (second round).
- `:144` (W6): confirmed; a rename over a running executable fails. The second round replaces the versioned-folder fix with rename-aside.
- `:163`: Mark of the Web is not set by npm (inference, high), so SmartScreen likely does not apply; Smart App Control does, at load time.
- `:200`: the first round raised the native estimate to 4 to 6 weeks; the second round reads those as effort days. Calibrated on #510, which took 10 hours of runs against a 5.5 to 9 day estimate, the core v1 is 1 to 2.5 weeks of wall clock.

## Still open

- From the spike: Codex's hook form on Windows, Job Objects killing a real grandchild and `run_logged`, and why neither agent reached the mock API ("CI spike results").
- Needs a real Windows machine: the Cursor desktop app's and CLI's hook forms, Qwen extensions, agy and Cline; console windows under GUI hosts; Smart App Control's block on an unsigned `plotkeeper-hook.exe`; Ctrl+C in a Windows console; the ACL of a real user's profile.
- No Microsoft page states `%LOCALAPPDATA%`'s default ACL, or whether SmartScreen checks a file with no mark started from a terminal.
- Whether Claude Code runs inside srt-win (`srt claude …`), and whether `codex exec` refuses or silently downgrades when an `elevated` setup was configured but never run.
- Whether the Credential Manager keeps a generic credential from other processes of the same user (expected not).
- The price and eligibility of Artifact Signing for an individual.
- `aarch64-pc-windows-msvc` was not checked on main (the spike's arm64 legs build the probe only).
- The 2026 Stack Overflow results' operating-system data.
