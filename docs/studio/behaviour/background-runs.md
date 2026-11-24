# Background runs

A **background run** is a headless agent run with scoped tools that plotkeeper starts on Daniel's click (spec #378, ticket #390). It is never a session or a terminal: plotkeeper still starts no session. The runner is `server/src/runner.rs` and its launcher is `plotkeeper-run` (`server/src/bin/plotkeeper-run.rs`). Micro-worlds start runs ([below](#micro-worlds)); back a law will. The sandbox gate (#395, below) proved the containment on a real `claude`.

## The seam

A run goes in as a request: its kind, its working folder, its prompt and its limits. It comes out as an outcome: the result text, the tool calls it was denied, its cost and its exit code. The harness, the agent program, only builds the command: `claude` is the one harness, and the tests' harness is a shell script.

There are two kinds. **Back a law** has `Bash`, `Read`, `Edit`, `Write`, `Glob` and `Grep`. **Micro-world** has the same without `Bash`. The command carries only the flags spec #378 gives; a micro-world reads the project from its working folder and writes only into `.micro-world/` there ([Micro-worlds](#micro-worlds)).

A prompt holds only text plotkeeper wrote or Daniel confirmed. An issue's, a PR's or a comment's text never goes into a run, since strangers can write it on a public repository.

## The command

For `claude` the command is built per run and never by hand:

```
claude --restricted --safe-mode --strict-mcp-config --permission-mode dontAsk --permission-prompts none
  [--model <backgroundRuns.model>] --tools <the kind's tools> --allowedTools <the allow list>
  --settings <the sandbox block> --output-format json -p <the prompt>
```

Standard input is closed, so the prompt is the last argument. `ANTHROPIC_API_KEY` and `ANTHROPIC_AUTH_TOKEN` are removed, so the run uses the CLI's own login, and `PLOTKEEPER_INTERNAL=1` keeps the hook from recording it, as for the writer. `CLAUDE_CODE_TMPDIR` names the run's private temp folder, `/tmp/pk-<16 hex>`: the sandbox then gives the tools `/tmp/pk-<16 hex>/claude-<uid>` as `TMPDIR`. Without it the tools' `TMPDIR` is the shared `/tmp/claude-<uid>`, where every session keeps its scratchpad, and the sandbox lets them write there. Claude Code uses the folder only while `<it>/claude-<uid>` is at most 44 bytes, so the name is of fixed length, 42 bytes with the longest uid. `--restricted` drops the user's, the project's and the local settings files; `--safe-mode` drops hooks, plugins, skills and `CLAUDE.md`.

The sandbox block is this, with plotkeeper's data folder first in `denyRead`, since it holds the page's token:

```json
{"disableAllHooks": true,
 "sandbox": {"enabled": true, "failIfUnavailable": true,
             "allowUnsandboxedCommands": false,
             "autoAllowBashIfSandboxed": false,
             "network": {"allowLocalBinding": true},
             "filesystem": {"denyRead": ["<plotkeeper data folder>", "~/.claude", "~/.claude.json",
                                         "~/.ssh", "~/.aws", "~/.config/gh", "~/.npmrc", "~/.netrc",
                                         "~/.cargo/credentials.toml", "~/.gnupg", "~/.kube"]}}}
```

`autoAllowBashIfSandboxed` must stay `false`: with `true`, probes ran commands outside the allow list. `allowLocalBinding` lets a project's tests serve on loopback, as plotkeeper's own do (`understanding/tests/http_writer.rs`, `fake_all`); without it they fail in every run, whatever the agent changed. On macOS it also lets a run's tools connect to any server on the Mac's loopback: a limit v1 accepts (below).

## The allow list

`--allowedTools` lists edits inside the working folder (for a micro-world, only inside its `.micro-world/`), and for a kind with `Bash`:

- read-only git, `git status`, `git diff` and `git log`, plus `git add`, since the working folder is a clone;
- one entry per command of the project's `checks`, each bare and with arguments (`Bash(cargo test)` and `Bash(cargo test *)`).

A `checks` entry with `git` or `gh` as any of its words is left out, such as `make verify && git push`, so no list ever allows a push or `gh`.

**`--allowedTools` is not a security boundary.** Once a run can write files and run a check, an allowed check runs code the agent wrote. What bounds a run is the OS sandbox (writes only inside the working folder and its private temp folder, no network for its tools beyond loopback, no reading of the denied folders) and that only trusted text reaches it. Tools may still read the home folder outside the deny list.

## Settings

All in `settings.json` ([Settings files](../settings.md)), read by the server anew for each run, so a saved change counts from the next run:

- `backgroundRuns.harness`: `claude`, the default and the only harness. Any other value refuses the run.
- `backgroundRuns.model`: the model. Unset, the CLI's own default.
- `checks`: the project's entry, picked as a session's is ([Checks](checks.md)): the longest folder that is the project's or holds it. There is no default list here. A project with no entry gets "Set this project's checks" and the prompt to copy; a micro-world, which has no `Bash`, needs none.
- `setup`: per project folder like `checks`, one command, such as `npm ci`. plotkeeper runs it in the working folder before the agent starts, outside the sandbox and with the network, so offline checks find their dependencies. With none set, it is skipped. A setup that fails ends the run before the agent starts.

## A run's folder

Runs live in the runs folder, `~/Library/Caches/plotkeeper/runs/`, outside plotkeeper's data folder, which a run may not read. A run's folder is `<id>/`, and it holds:

- `work/`: the working folder, the agent's current folder and the only place it writes. The module that asks for the run fills it first, with a clone or nothing.
- `run.json`: the run as asked for, with its built command, its sandbox check and its private temp folder.
- `lock`, `launch.json`, `status.json`: the lifecycle, below.
- `stdout`, `stderr`, `setup.log`: what the harness and the setup command printed.

## Lifecycle

plotkeeper starts each run through the launcher, `plotkeeper-run <run's folder>`, in a process group of its own, so the run outlives plotkeeper: stopping plotkeeper to rebuild it does not kill a run.

- **The start.** The launcher first checks that the sandbox can start, by running `/usr/bin/true` under `sandbox-exec`. It cannot inside another sandbox, such as a plotkeeper started from a sandboxed shell, and Claude Code then keeps the run going with every command refused and exits 0. A failed check ends the run as failed, "The sandbox could not start", before setup and before the harness; the check counts against the run's time limit. Then the setup command, then the run's private temp folder, made new and `0700`, then the harness. A folder already there fails the run and is left alone. The launcher removes the folder it made when the harness exits, and Cancel removes the folder of a run it kills.

- **The lock.** plotkeeper takes an exclusive lock on the folder's `lock` and hands it to the launcher as its standard input. The launcher gives it to no child and holds it until it exits, however it exits. A run is alive exactly while that lock is held, which plotkeeper asks without waiting.
- **The status.** When the harness exits, or the time limit passes, the launcher writes `status.json` whole, by a temporary file and a rename. Its last act is then to kill its own process group, itself included, so nothing a run started outlives it: not a run stopped at its limit, and not a stray child of a run that finished.
- **The end.** plotkeeper waits on the lock, with no timer. When it frees, a status means the run ended, and no status means its process died: the run is failed and the next in its queue starts.
- **Cancel.** For a running run, plotkeeper checks that the lock is held and that the process `launch.json` names still has the start time written there, kills the launcher's process group, and removes the run's folder. A process whose start time differs is never signalled: nothing is stopped or removed, and the notice line says so. A queued run is dropped and its folder removed. A run whose launcher exited a moment before the click is recorded as it ended, not as cancelled. A plotkeeper that died right after it started a launcher may have left no process id; such a run cannot be cancelled and ends at its limit.
- **A start.** plotkeeper picks the runs back up from the runs folder: a queued run waits its turn again, a run whose lock is held is still running, and a run whose lock is free ended meanwhile, or died if it left no status. An ended run it had not recorded is recorded and listed. The folder of an ended run it had recorded is an orphan and is removed, as is a folder with no run in it. A folder whose `run.json` this build cannot read, such as a newer build's, is left alone while its lock is held: this build neither lists nor records that run, and a later start removes the folder once its lock is free. Whatever order the folders give, a queued run never starts while a run of its project is running.

## Limits

- One run at a time per project. The others wait in the order they were asked for, and the queue shows on the map.
- A run starts only on a click. Nothing starts one on a timer.
- 30 minutes, 10 for a micro-world, setup included. At the limit the launcher writes the status and kills its whole process group.
- A third of the cores for builds and tests: `CARGO_BUILD_JOBS` and `NEXTEST_TEST_THREADS`.
- A run that fails because the login is missing or a usage or rate limit is hit pauses every project's queue. The map shows one indicator with **Resume**, and nothing starts until Daniel presses it. Every project with runs is told, and the pause outlives a restart.

## Outcome

A run ends as one of: finished; finished with tool calls denied; failed; stopped at its limit; died; setup failed; not logged in; rate limited. A run that exits 0 with denials in its result is reported with them, never as a clean success. A result that says it is an error, by `is_error` or by a `subtype` such as `error_max_turns`, is a failure whatever the exit code, and so is a run whose result or error output shows the sandbox failing (`sandbox_apply: Operation not permitted` or `Sandbox is required but`). A result that only names the settings does not match. Only a failed run's words are read for a login or a limit, with the writer's own word list, so a good result that mentions a rate limit pauses nothing; a failed run that only mentions one does pause, which costs one Resume.

Each run appends one line to `background-runs.jsonl` in the data folder: the time, its id, kind, project, outcome, denials, cost and duration. A cancelled run is recorded as `cancelled`.

## Fallback

A run that cannot start is refused at the click with why and its prompt: the harness is not installed, `backgroundRuns.harness` names another, or the project has no `checks`. A run that fails, or finishes with denials, keeps its prompt on its row. Either way the prompt is copied for Daniel to paste into his own agent, as Ratify's Overturn does ([PR briefs](pr-briefs.md)); when the clipboard refuses, it shows in a small box. The prompt copied is the run's own, which stands alone.

## On the map

The Map view's side area lists the project's runs under **Background runs** (`ui/map-runs.ts`): a running run with when it started, the queued ones, and each ended run with how it ended and the calls it was denied. A queued or running run has **Cancel**; an ended one has **Dismiss**, and **Copy prompt** unless it finished cleanly. An ended run stays until it is dismissed or plotkeeper starts again. A copied prompt's note stays under its row when the map is drawn again. The server tells the page when a run starts or ends, so the rows change with no timer. A past commit's map shows no runs.

## Micro-worlds

**Explore this** builds a micro-world: a small interactive page that simulates a mechanism of the project from its real code (spec #378, ticket #399; `server/src/micro_worlds.rs`, `ui/micro-world.ts`). It is offered under each flow of a part's refresher, and under a PR brief's figure, where it is about the parts that step lights. The figure's own labels are the writer's words from a PR, so they never go into the run.

- **The run.** A micro-world run's working folder holds the project's files at the map's commit (`git archive`, so no git data and nothing untracked), and an empty `.micro-world/`, the only place it may write. Its prompt is map text only (the flow's hops, the parts' captions, descriptions and paths, their laws) and dsh-visual's rules, inline, since `--safe-mode` loads no skill: one self-contained page in `.micro-world/template.html`, no request of any kind, code cited as `{{ref:path|start text}}`, a light and a dark background, no sideways overflow. It needs no `checks`.
- **The host checks.** When the run ends, plotkeeper runs dsh-visual's `build.py` from the folder `microWorlds.dshVisual` names ([Settings](../settings.md)), which resolves every `{{ref:…}}` against git at the commit and fails on one that matches no line or several. It then puts `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:">` first in the page and runs `verify.mjs` on that page in headless Chrome (script and console errors, overflow at 390 and 1280 px, broken `#` links, light and dark backgrounds). `verify.mjs` runs as `node --import micro_world_guard.mjs`, which makes every context of its Chrome abort each request that is not a local file and sends Chrome through a proxy that leads nowhere, so model-written script never reaches the network; an aborted request fails the check. Without playwright-core where `verify.mjs` finds it, the guard throws and the check fails. `build.py` may take 2 minutes and `verify.mjs` 5; their output is in the run's folder, `build.log` and `verify.log`.
- **A failing page** is rewritten once: a second run finds the first template in its `.micro-world/` and is told the failing checks. Failing again, or a run that ends any other way than finished (denials included are still checked), the world shows why and offers the prompt to copy, with Try again. A cancelled run fails its world.
- **Without `microWorlds.dshVisual`**, no run starts: Explore this copies a prompt for Daniel's own agent, which uses the dsh-visual skill in his checkout. A page built before still opens.
- **Kept** in the data folder, `micro-worlds/<project>/`, one file per world by its key (`flow:<id>`, or `figure:<pr>:<chapter>:<step>`), with the commit it was built at; only a page that passed the checks is stored, and the API (`micro_world_page`) serves only a stored page. A world whose stored page was built before a change, between that commit and the map's commit now, to a path of a part on it says **This may be out of date**, with **Rebuild**, which starts a run as Explore this does; the older page opens meanwhile.
- **Shown** in a wide overlay in the glass, as wide as the left panel's widest: the page is fetched through the API and put in a frame by `srcdoc`, with `sandbox="allow-scripts"` and never `allow-same-origin`. The frame has an opaque origin, no network and no way to plotkeeper's API, and `srcdoc` sends no cookie. Esc closes it before any other layer of the map; another project closes it.
- **Stop rule**: micro-worlds are kept if opened at least twice in the review's window (`micro-world-opened`).

## The sandbox gate

The tests never start `claude`: they prove the lifecycle with a fake harness and read the built command. The sandbox gate (#395) runs each run through `Runner::submit` with a real `claude` and with `srt`, by hand and outside auto mode: `server/examples/sandbox_gate.rs`, whose header says how. Each verdict is read from the disk after the run (a file that must not exist, a marker that must not show), never from what the agent says, and a probe makes its escapes from an allowed check, `sh ./probe.sh`, so the OS sandbox and not the permission layer is what refuses them. Its last run, on Claude Code 2.1.325 with Sonnet, passed every check:

| Check | Result |
|---|---|
| A write outside the clone | Refused: the home folder, `/private/tmp`, the run's own folder, a folder beside the runs. |
| A write into Claude Code's per-user temp folder | Refused: its root, an existing session's scratchpad, a new folder. The tools' `TMPDIR` is the run's private folder. |
| A network call from a tool | Refused: `curl` gets a 403 from the proxy, also for `api.anthropic.com`, and raw TCP fails. The CLI itself reaches the API. |
| A read of each `denyRead` folder | Refused, for every folder and file of the block; a file outside it still reads. The CLI's login works with `~/.claude` and `~/.claude.json` denied. |
| `--restricted` and Read, Glob and Grep outside the clone | Denied, each listed in the result's denials. |
| A sandbox that cannot start | The launcher's check fails the run before the harness, "The sandbox could not start". |
| An offline build and test of plotkeeper after `setup` (`npm ci && cargo fetch --locked`) | `npm run build`, `cargo build --workspace`, `cargo nextest run -p studio-understanding` and `npm run test:node` pass. |
| `Bash(<entry> *)` and the bare command | The `*` form alone allows the bare command; both forms are still listed. |
| `autoAllowBashIfSandboxed: false` | `python3` and `touch`, outside the allow list, are denied and write nothing. |
| The `Edit` rule on the working folder | Needed: without it, Write is denied under `dontAsk`. |
| `srt` with no network | A check command writes only to its clone and its private temp folder (passed to `srt` as `CLAUDE_CODE_TMPDIR` and listed in `allowWrite`), has no network, cannot read the data folder, and binds loopback with `network.allowLocalBinding`. |

What the gate found on the way, and changed here:

- **The temp folder.** The tools' `TMPDIR` was the shared `/tmp/claude-<uid>`. A `denyWrite` on it breaks every build, since it is their `TMPDIR`; a long `CLAUDE_CODE_TMPDIR` moves only the CLI's own files. A short one, under the 44-byte limit, moves the tools' `TMPDIR` too.
- **Loopback.** With it closed, plotkeeper's own tests fail in every run; `allowLocalBinding` is on for runs and for the `srt` proof step.
- **`failIfUnavailable` does not stop such a run.** Inside another sandbox Claude Code starts, every command fails with `sandbox_apply: Operation not permitted`, and the run exits 0 as a success. Hence the launcher's check, and the reading of those words as a failure. An `(allow default)` profile still applies inside another sandbox, so the check's profile denies something.

The raw output of each round is in `local-evidence/2026-11-24-issue-395-sandbox-gate/` on Daniel's machine. Rerun the gate after a Claude Code update: its behaviour here changed between the versions tried.

## Limits v1 accepts

v1 is one user on his own Mac, runs start only on his click, and a prompt holds only trusted text. Hardening past that is #411 ([research](../research/sandbox-loopback-preflight-temp.md)):

- A run's tools can connect to any server on the Mac's loopback. plotkeeper's own server needs its cookie or token, which a run cannot read.
- Only the shell is sandboxed, not the whole `claude` process, which Anthropic's docs ask of fully unattended runs.
- The private temp folder holds only while `<it>/claude-<uid>` is at most 44 bytes; past that, Claude Code falls back to the shared folder without a word. The name is of fixed length, 42 bytes with the longest uid.
- A run whose launcher dies leaves its temp folder in `/tmp` (private, `0700`) until the Mac restarts.
- Tools may read the home folder outside the deny list.
