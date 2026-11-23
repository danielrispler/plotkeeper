# Background runs

A **background run** is a headless agent run with scoped tools that plotkeeper starts on Daniel's click (spec #378, ticket #390). It is never a session or a terminal: plotkeeper still starts no session. The runner is `server/src/runner.rs` and its launcher is `plotkeeper-run` (`server/src/bin/plotkeeper-run.rs`). No module starts a run yet; back a law and micro-worlds will.

## The seam

A run goes in as a request: its kind, its working folder, its prompt and its limits. It comes out as an outcome: the result text, the tool calls it was denied, its cost and its exit code. The harness, the agent program, only builds the command: `claude` is the one harness, and the tests' harness is a shell script.

There are two kinds. **Back a law** has `Bash`, `Read`, `Edit`, `Write`, `Glob` and `Grep`. **Micro-world** has the same without `Bash`. The command carries only the flags spec #378 gives; how a micro-world reads the project is left to its own ticket.

A prompt holds only text plotkeeper wrote or Daniel confirmed. An issue's, a PR's or a comment's text never goes into a run, since strangers can write it on a public repository.

## The command

For `claude` the command is built per run and never by hand:

```
claude --restricted --safe-mode --strict-mcp-config --permission-mode dontAsk --permission-prompts none
  [--model <backgroundRuns.model>] --tools <the kind's tools> --allowedTools <the allow list>
  --settings <the sandbox block> --output-format json -p <the prompt>
```

Standard input is closed, so the prompt is the last argument. `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN` are removed, so the run uses the CLI's own login, and `PLOTKEEPER_INTERNAL=1` keeps the hook from recording it, as for the writer. `--restricted` drops the user's, the project's and the local settings files; `--safe-mode` drops hooks, plugins, skills and `CLAUDE.md`.

The sandbox block is this, with plotkeeper's data folder first in `denyRead`, since it holds the page's token:

```json
{"disableAllHooks": true,
 "sandbox": {"enabled": true, "failIfUnavailable": true,
             "allowUnsandboxedCommands": false,
             "autoAllowBashIfSandboxed": false,
             "filesystem": {"denyRead": ["<plotkeeper data folder>", "~/.claude", "~/.claude.json",
                                         "~/.ssh", "~/.aws", "~/.config/gh", "~/.npmrc", "~/.netrc",
                                         "~/.cargo/credentials.toml", "~/.gnupg", "~/.kube"]}}}
```

`autoAllowBashIfSandboxed` must stay `false`: with `true`, probes ran commands outside the allow list.

## The allow list

`--allowedTools` lists edits inside the working folder, and for a kind with `Bash`:

- read-only git, `git status`, `git diff` and `git log`, plus `git add`, since the working folder is a clone;
- one entry per command of the project's `checks`, each bare and with arguments (`Bash(cargo test)` and `Bash(cargo test *)`).

A `checks` entry with `git` or `gh` as any of its words is left out, such as `make verify && git push`, so no list ever allows a push or `gh`.

**`--allowedTools` is not a security boundary.** Once a run can write files and run a check, an allowed check runs code the agent wrote. What bounds a run is the OS sandbox (writes only inside the working folder, no network for its tools, no reading of the denied folders) and that only trusted text reaches it. Tools may still read the home folder outside the deny list.

## Settings

All in `settings.json` ([Settings files](../settings.md)), read by the server anew for each run, so a saved change counts from the next run:

- `backgroundRuns.harness`: `claude`, the default and the only harness. Any other value refuses the run.
- `backgroundRuns.model`: the model. Unset, the CLI's own default.
- `checks`: the project's entry, picked as a session's is ([Checks](checks.md)): the longest folder that is the project's or holds it. There is no default list here. A project with no entry gets "Set this project's checks" and the prompt to copy; a micro-world, which has no `Bash`, needs none.
- `setup`: per project folder like `checks`, one command, such as `npm ci`. plotkeeper runs it in the working folder before the agent starts, outside the sandbox and with the network, so offline checks find their dependencies. With none set, it is skipped. A setup that fails ends the run before the agent starts.

## A run's folder

Runs live in the runs folder, `~/Library/Caches/plotkeeper/runs/`, outside plotkeeper's data folder, which a run may not read. A run's folder is `<id>/`, and it holds:

- `work/`: the working folder, the agent's current folder and the only place it writes. The module that asks for the run fills it first, with a clone or nothing.
- `run.json`: the run as asked for, with its built command.
- `lock`, `launch.json`, `status.json`: the lifecycle, below.
- `stdout`, `stderr`, `setup.log`: what the harness and the setup command printed.

## Lifecycle

plotkeeper starts each run through the launcher, `plotkeeper-run <run's folder>`, in a process group of its own, so the run outlives plotkeeper: stopping plotkeeper to rebuild it does not kill a run.

- **The lock.** plotkeeper takes an exclusive lock on the folder's `lock` and hands it to the launcher as its standard input. The launcher gives it to no child and holds it until it exits, however it exits. A run is alive exactly while that lock is held, which plotkeeper asks without waiting.
- **The status.** When the harness exits, or the time limit passes, the launcher writes `status.json` whole, by a temporary file and a rename. Its last act is then to kill its own process group, itself included, so nothing a run started outlives it: not a run stopped at its limit, and not a stray child of a run that finished.
- **The end.** plotkeeper waits on the lock, with no timer. When it frees, a status means the run ended, and no status means its process died: the run is failed and the next in its queue starts.
- **Cancel.** For a running run, plotkeeper checks that the lock is held and that the process `launch.json` names still has the start time written there, kills the launcher's process group, and removes the run's folder. A process whose start time differs is never signalled: nothing is stopped or removed, and the notice line says so. A queued run is dropped and its folder removed. A run whose launcher exited a moment before the click is recorded as it ended, not as cancelled. A plotkeeper that died right after it started a launcher may have left no process id; such a run cannot be cancelled and ends at its limit.
- **A start.** plotkeeper picks the runs back up from the runs folder: a queued run waits its turn again, a run whose lock is held is still running, and a run whose lock is free ended meanwhile, or died if it left no status. An ended run it had not recorded is recorded and listed. The folder of an ended run it had recorded is an orphan and is removed, as is a folder with no run in it. A folder whose `run.json` this build cannot read, such as a newer build's, is left alone while its lock is held, and removed once it is free. Whatever order the folders give, a queued run never starts while a run of its project is running.

## Limits

- One run at a time per project. The others wait in the order they were asked for, and the queue shows on the map.
- A run starts only on a click. Nothing starts one on a timer.
- 30 minutes, 10 for a micro-world, setup included. At the limit the launcher writes the status and kills its whole process group.
- A third of the cores for builds and tests: `CARGO_BUILD_JOBS` and `NEXTEST_TEST_THREADS`.
- A run that fails because the login is missing or a usage or rate limit is hit pauses every project's queue. The map shows one indicator with **Resume**, and nothing starts until Daniel presses it. Every project with runs is told, and the pause outlives a restart.

## Outcome

A run ends as one of: finished; finished with tool calls denied; failed; stopped at its limit; died; setup failed; not logged in; rate limited. A run that exits 0 with denials in its result is reported with them, never as a clean success. A result that says it is an error, by `is_error` or by a `subtype` such as `error_max_turns`, is a failure whatever the exit code. Only a failed run's words are read for a login or a limit, with the writer's own word list, so a good result that mentions a rate limit pauses nothing; a failed run that only mentions one does pause, which costs one Resume.

Each run appends one line to `background-runs.jsonl` in the data folder: the time, its id, kind, project, outcome, denials, cost and duration. A cancelled run is recorded as `cancelled`.

## Fallback

A run that cannot start is refused at the click with why and its prompt: the harness is not installed, `backgroundRuns.harness` names another, or the project has no `checks`. A run that fails, or finishes with denials, keeps its prompt on its row. Either way the prompt is copied for Daniel to paste into his own agent, as Ratify's Overturn does ([PR briefs](pr-briefs.md)); when the clipboard refuses, it shows in a small box. The prompt copied is the run's own, which stands alone.

## On the map

The Map view's side area lists the project's runs under **Background runs** (`ui/map-runs.ts`): a running run with when it started, the queued ones, and each ended run with how it ended and the calls it was denied. A queued or running run has **Cancel**; an ended one has **Dismiss**, and **Copy prompt** unless it finished cleanly. An ended run stays until it is dismissed or plotkeeper starts again. A copied prompt's note stays under its row when the map is drawn again. The server tells the page when a run starts or ends, so the rows change with no timer. A past commit's map shows no runs.

## Not yet proven on a real `claude`

The tests never start `claude`: they prove the lifecycle with a fake harness and read the built command. Spec #378's sandbox gate is run by hand, with a real `claude`, and its results are not recorded yet. Until they are, no module should start a run. Three things the gate must settle bear on this code:

- **The temp folder is open.** A probe showed the sandbox lets a run's tools write into Claude Code's per-user temp folder, where sessions keep the scripts Daniel runs by hand. The block above is spec #378's, and it has no rule that denies that write. The gate must show a write-deny rule that closes it, and that rule then joins the block; if none does, runs stay off, copy-prompt only.
- Whether `Bash(<entry> *)` also matches the bare command. Both forms are listed here.
- Whether the `Edit` rule on the working folder is what lets a run under `dontAsk` write there. Spec #378's allow list does not name it; a probe needed it for a micro-world.
