# Checks

A check is a command of the project's check list. `checks` in `settings.json` (see [Settings files](../settings.md)) names, for each project folder, the commands that count, and a session working in that folder or under it uses that list, the longest folder winning. A project with no entry uses the default: `cargo test|nextest|clippy|check|build`, `ci-checks`, `go test|vet` and `npm run check`. A command matches when its words follow an entry's in order, with flags or a toolchain between (`cargo +nightly test`); quoted text is not run, so a PR body that names `cargo test` is no check.

A turn's checks, its workers' included, come from the step ledger, one suite per list entry, and a suite's status is its last run's: passed or failed when its exit code tells, ran when something in the command can mask it (see [The card store](storage.md#the-card-store)). A run that failed before the last one counts as a retry. A failed check keeps its exit code and the last 40 lines of its output, read from the event log when its row is opened.

A check of the session's own agent that usually runs over 2 minutes (the lower median of its earlier runs that did not fail) gets a card at its start, on the change since the last card, when there is one; a subagent's checks get no card. Its quiz asks whether the check will pass, and the check's result settles your guess: until it ends, the card says only that the check runs; once it ends, it says whether it passed and whether you were right, and shows the writer's reasons for a pass and a fail. A tap after the result counts for nothing.

When the exit code does not say whether the check passed, the guess is not counted and no option is named the right one. That card has no "the key is wrong".

A check's exit code says whether it passed when nothing in the command can mask it: the check is the command's last statement (after the last `;` or line break), with no `||`, no leading `!` and no `&` that sends it to the background, and a pipe only after a `set` that turns on `pipefail`. Check lines come from the event log, not the store.
