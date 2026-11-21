# Checks

A **check** is a command of the project's check list, as run in one worktree. A turn's runs of it are one check, and its last run's result is the check's. A check is not named by its command's first line: each statement that runs a list entry is a run of its own.

## The check list

`checks` in `settings.json` (see [Settings files](../settings.md)) names, for each project folder, the commands that count, and a session working in that folder or under it uses that list, the longest folder winning. A project with no entry uses the default: `cargo test|nextest|clippy|check|build`, `ci-checks`, `go test|vet` and `npm run check`. Every part of the studio reads the session's own list, the phases (Checking) included.

## What runs a check

A command is read as statements, split at `;`, line breaks, `&&`, `||`, `|` and `&` outside quoted text. A heredoc's body is not read, a backslash-escaped quote does not end quoted text, and a `#` comment is skipped. Redirections (`2>&1`, `> out`) are left out of a statement.

A statement runs an entry when the entry's first word is the statement's command and its other words follow in order among the words that are not flags, with a toolchain between (`cargo +nightly test`). A path to the command matches (`./target/debug/ci-checks`). `cargo run … --bin ci-checks` runs the one-word entry `ci-checks`.

Before its command a statement may hold only variable assignments (their value may be quoted or a `$(…)` or `$((…))`, which is read as part of the word, never as statements) and these words: `time`, `timeout` with its duration, `env`, `nice`, `exec`, `command`, `do`, `then`, `else`, `if`, `while`, `until`, `!` and `{`. A check behind any other wrapper is no check. A word that is only an argument is no check (`strings target/debug/ci-checks`), nor is quoted text, so a PR body that names `cargo test` runs nothing, nor is a heredoc's body.

Each statement that runs an entry is one run, so `cargo test && cargo clippy` is a run of two checks, and a `cargo test` that follows a heredoc edit in the same command is `cargo test`. A run is named by its own statement: its words from the command on. That is what a feed line shows, and what typical durations are kept under. Any command tool counts, not only Claude Code's `Bash`, so agy's commands are matched too.

The parser is not a shell. A check inside a function, an alias, `eval`, `xargs` or a script file is not seen.

## A run's worktree

A check is a suite in a worktree, so the same suite in two worktrees is two checks, and one's pass does not hide the other's failure. A run's worktree is, in order:

1. the root, among the worktree roots the hook recorded for the call, that holds the folder of the last `cd` with an absolute path before the run's statement; a relative `cd` changes nothing;
2. else the call's first recorded root;
3. else, for a call with no recorded root (a log from before roots were kept), the worktree of the session's folder, or the worker's own for a worker's call.

A check outside the session's own worktree names its worktree, in the Evidence line and in the Brief's Open slot (`cargo test in <worktree>`); a check in the session's own shows its suite only.

## A check's status

A turn's checks, its workers' included, come from the step ledger, in the order each first ran. A check's status is its last run's:

- **Passed** or **failed** when the exit code tells.
- **Ran** when something in the command can mask the exit code (see [The card store](storage.md#the-card-store)); a card calls it not counted.
- **Running** while the run has started and has no result. A running check is neither passed nor failed: it is in none of the counts, it is not on the Evidence line, and it closes nothing in the Brief's Open slot.

A run that failed before the last one is a retry. A failed check keeps its exit code and the last 40 lines of its output, read from the event log when its row is opened; the runs of one command share them. A command that was denied, interrupted or blocked is no run.

A full run that fails, followed by a narrower run of the same suite that passes, reads passed: the check is the suite, not the arguments, and the last run wins.

### When an exit code counts

A command exits with its last group's code: the statements after the last `;`, `&` or line break. A check in an earlier group is Ran, whatever the exit code. In the last group, each of its checks is Ran when the group holds a `||`, a `!` on any of its statements, a `&` that sends a statement to the background (the statement after a `&` is in the foreground, so `server & cargo test` counts), or a pipe when no `set` before the check turned `pipefail` on (`set -o pipefail`, `set -euo pipefail`) and none after it turned it off (`set +o pipefail`). Otherwise:

- Exit 0 says each check in the group passed, so `cargo fmt --check && cargo clippy` passing is two passes.
- A non-zero exit says a check failed only when it is the group's one statement besides `cd`, `export`, `set` and variable assignments (under `pipefail` a pipeline is one statement, so `set -o pipefail; cargo test 2>&1 | tail` failing is failed), so `cd x && cargo test` failing is failed. When the group holds two checks, or a check and another command, the code cannot say which failed or whether a later one ran, and each check in it is Ran, never failed.

### Background runs

A check an agent sends to the background is running until its notification lands, then holds what it reports: completed is passed when exit 0 would count, failed is failed when a non-zero exit would count, and anything else, a killed task included, is Ran. It stays in the turn that started it, and that turn's checks are read again when the notification lands. A shell `&` is Ran. A background run has no exit code.

## The feed

The core gives the rail's feed the checks to list, newest first, and the rail draws them with no rule of its own. Under a turn, for each check of the session's own agent, its latest run is listed while it is running or failed: a check that passed, or failed and then passed, is not listed. A worker's runs are not in the feed; they are in the Evidence line with who ran them, and they count in the banner and the Brief. A line shows "Running" or "Failed", the run's own statement and how long it took; while it runs, the time so far against the typical duration, such as "1m of usually 2m".

## Typical durations and the Check card

A command's typical duration is the lower median of how long its earlier runs took, kept under the keys of its check statements joined in order (one statement's key for a command with one check). A worktree is not part of the key, so a new worktree starts with the time of the others. Only the session's own agent's runs count: a foreground run that did not fail, and a background run once its notification says completed.

A check of the session's own agent whose typical duration is over 2 minutes gets a card at its start, one for the command, on the change since the last card, when there is one; a subagent's checks get no card. Its quiz asks whether the check will pass, and the command's result settles your guess: until it ends, the card says only that the check runs; once it ends, it says whether it passed and whether you were right, and shows the writer's reasons for a pass and a fail. A tap after the result counts for nothing.

When the exit code does not say whether the command's checks passed, the guess is not counted and no option is named the right one. That is so when any check of the command is Ran, such as in a chain that exits non-zero. A command with a failed check settles failed. That card has no "the key is wrong".
