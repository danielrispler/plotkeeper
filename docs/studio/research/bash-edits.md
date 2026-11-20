# Agents that edit through Bash

> **Answer:** No primary source explains a switch on 2026-11-15/16. What the sources do show is that Claude Code's auto mode has told the model, since about 2026-08, to do much of its work through Bash, with `sed`, heredocs and short scripts "instead of the dedicated Read, Edit, or Write tools". Today's wording leaves the choice to the model. Plotkeeper's own transcripts show that this text was already in the subagents' context from 10-13, while they still used `Edit`, and that the CLI (`2.1.311`) did not change across the switch. The subagents' model did change, on 11-14 15:22 (Opus 5.5 to Sonnet 5.5). The diagnosis finds the trigger in the studio's own setup: the orchestrators stopped spawning workers isolated, and subagents that share the session's folder edit through Bash. No flag is needed.
> **Hooks:** a `PostToolUse` hook for `Bash` carries `tool_input.command`, the event's `cwd` and, in auto mode, `tool_response.bashEditDiff.changedFiles`: absolute paths of the files the command changed in a Git repository (official, "best effort", public beta). There is also a `FileChanged` hook, which fires from a filesystem watcher whatever wrote the file, but it watches only files you name up front. No setting forces agents back to `Edit`; `CLAUDE.md` is "context, not enforced configuration", and a `PreToolUse` hook that denies the Bash write is the documented way to enforce it.
> **Evidence:** Strong for the hook fields and for plotkeeper's own logs. Medium for the instruction's history: it rests on GitHub issues from users, which quote the text and date it, plus one official sentence that says Claude Code directs Claude to edit through Bash. Weak for the cause of the 11-15/16 switch: nothing found.
> **For the studio:** `bashEditDiff` is the cheapest signal, but in plotkeeper's log it is missing on 638 of 771 subagent Bash edits whose event `cwd` is the main checkout (section 7). A `git status` or `git diff` of the worktree at `Stop` finds those edits regardless.

Research date: 2026-11-18. **Corrected after the diagnosis, on 2026-11-18:** sections 1, 3, 5 and 12 held errors, which [the diagnosis of #186, section 6](https://github.com/danielrispler/plotkeeper/issues/186#issuecomment-6973817740) lists and which are fixed below. Sections 6 to 9 stand. Ticket: [#186](https://github.com/danielrispler/plotkeeper/issues/186). The studio's code is read at commit [`5f4e517`](https://github.com/danielrispler/plotkeeper/tree/5f4e517466094ca9d632fb17b516ed86cd2cd2ad); code links below point there. Claude Code docs, its changelog and its GitHub issues were read on 2026-11-18.

**How to read this.** A sentence with a link is established by that source. A sentence marked **Inference** is my own reading, which the sources do not state. **Unverified** marks something that would need a run to confirm. A GitHub issue on `anthropics/claude-code` is a **user report**: it shows what the reporter saw and what text the harness showed them, not Anthropic's intent. "Plotkeeper's event log, analysed 2026-11-18" is Daniel's local `events.jsonl`; I re-ran some counts from it (section 1) and say so.

---

## Words used in this note

- **Auto mode:** Claude Code's `auto` permission mode, where a classifier reviews each action ([permission modes](https://code.claude.com/docs/en/permission-modes)).
- **Bash edit:** a file change made by a Bash command (`python3 - <<'EOF'`, `sed -i`, `cat > file`), not by the `Edit` or `Write` tool.
- **Steer:** the mode-specific block of text that Claude Code adds to the model's context in auto mode (and bypass mode), which this note quotes in section 3.

---

## 1. What plotkeeper's own logs show

From plotkeeper's event log, analysed 2026-11-18, as briefed: CLI `2.1.311` since 2026-11-03; subagents in `permission_mode: auto` from 11-13; subagent `Edit`/`Write` calls 131 (11-14), 52 (11-15), 1 (11-16), 1 (11-17); subagent Bash edits 89, 208, 391, 316. No `Edit` or `Write` call was denied or failed.

Three more facts I read from the local files on 2026-11-18 (counts of fields only; no scored run, no credential file), as corrected by the diagnosis:

1. **The subagents' model did change.** In the subagent transcripts (`~/.claude/projects/*plotkeeper*/**/subagents/*.jsonl`), the subagents moved from Opus 5.5 to Sonnet 5.5 on 11-14 at 15:22 (`env.CLAUDE_CODE_SUBAGENT_MODEL = "sonnet"`, in Daniel's `/doctor` session). The first version of this note compared 11-14 as a whole with 11-16 and called the model unchanged. On 11-14 `claude-sonnet-5-5` made 113 `Edit` and 34 `Write` calls and 870 Bash calls; on 11-16 it made 0 `Edit`, 1 `Write` and 1502 Bash calls. Every one of those lines carries CLI version `2.1.311`, which did not change.
2. **The steer did not change, and it is older than 11-14.** Claude Code writes an `auto_mode` attachment into the transcript when it injects the steer (described in a comment of 2026-09-05 on [anthropics/claude-code#90450](https://github.com/anthropics/claude-code/issues/90450), user report). It reads `bashFirst: true, bashFirstSteer: "relaxed", steerOnly: true` in the transcripts from 10-13 on, the earliest the diagnosis checked, and was the same on both sides of the switch.
3. **The main session did not switch.** The same log shows the main session still using `Edit` (126 calls on 11-16, 84 on 11-17) and `Write`, next to its own Bash calls. Only the subagents moved.

**What moved them.** The diagnosis (section 3) found that the fall is the end of isolated spawns: `Agent` calls with `isolation: "worktree"` per day from 11-14 were 14, 13, 0, 0, 0, and subagent `Edit`+`Write` calls were 131 (113 by isolated workers), 52 (43), 1, 1. A subagent that shares the session's folder used `Edit` or `Write` for 3% of its edits on Sonnet 5.5 (72% for an isolated Opus 5.5 one, 63% for an isolated Sonnet 5.5 one). **Inference:** the two changes of 11-14 (model and plugins) came within 22 minutes, so the data cannot say which of them matters for the shared-folder rows; isolation is what moves the share.

---

## 2. What the official sources say

### 2.1 The docs and changelog never describe the steer

I searched `permission-modes`, `tools-reference`, `auto-mode-config`, `sandboxing`, `permissions`, `memory`, `env-vars`, `settings-reference` and `hooks` (all `code.claude.com/docs/en/*.md`, read 2026-11-18) for the steer's wording ("simpler route", "dedicated Read, Edit, or Write", "through the Bash tool") and for its flags (`THRIFTY_SONIC`, `COZY_TEAPOT`). None of them contains it.

### 2.2 One official sentence admits it

The hooks reference says Claude Code records the files a Bash command changed "only in auto mode and `bypassPermissions` mode, and only when Claude Code directs Claude to edit files through Bash" ([hooks, Bash](https://code.claude.com/docs/en/hooks#bash)). So the product states that it directs Claude to edit files through Bash in those modes.

### 2.3 The changelog, with the dates it has

The [changelog](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md) (read at its head, `2.1.327`) has **no dates**. Entries that bear on the question:

- `2.1.269`: "Added a diff of the files a Bash command changed to the Bash tool result when the Bash tool handles file edits (setting `bashEditDiffEnabled`)". This is the feature behind `bashEditDiff` (section 6).
- `2.1.311` (the local CLI): "Fixed the Bash tool description claiming the working directory always persists between calls". Relevant to `cwd` (section 8).
- `2.1.321`: "Added Claude Fable 5.2 (`claude-fable-5-2`), now the default Fable model".
- Nothing from `2.1.300` to `2.1.327` mentions the Bash-first steer, tool choice for edits, or a change to what subagents are told. An older entry, from a much earlier version, did the reverse: "Improved system prompts to more clearly guide the model toward using dedicated tools (Read, Edit, Glob, Grep) instead of bash equivalents" (changelog, near line 9645 of the file I read).

The local CLI is `2.1.311` and the changelog's head is `2.1.327`, so six releases came out that this machine never ran. None of them touches the steer either.

### 2.4 Model release notes

The [API release notes](https://platform.claude.com/docs/en/release-notes/overview) give these launch dates: `claude-opus-5-5` 2026-09-22; `claude-sonnet-5-5` 2026-09-28; `claude-haiku-5-5` 2026-10-07; `claude-fable-5-2` 2026-11-09. They list no change to tool use, the text editor tool or file editing for 2026-11-15/16. The nearest entries are 11-12 (self-hosted Managed Agents), 11-16 (text watermark for older models) and 11-17 (linked billing). None concerns Claude Code.

---

## 3. What the user reports show (GitHub, `anthropics/claude-code`)

All of these are **user reports**.

| Date | Issue | What it shows |
|---|---|---|
| 2026-08-19 | [#87971](https://github.com/anthropics/claude-code/issues/87971) | The first report. CLI `2.1.235`. Quotes the steer in its strong form: "Do your work through the Bash tool wherever it can accomplish the job … make file changes with sed, heredocs, or short scripts, rather than using the dedicated Read, Edit, or Write tools. Fall back to a dedicated tool only when Bash genuinely cannot do the job." A commenter says it "is apparently an intended change in Claude's system prompt". |
| 2026-08-24 | [#89251](https://github.com/anthropics/claude-code/issues/89251) | The steer appears in bypass mode too, and routes writes around `PreToolUse` hooks that match `Write\|Edit`. |
| 2026-08-28 / 09-05 | [#90450](https://github.com/anthropics/claude-code/issues/90450) | A commenter reads the `auto_mode` attachment (`bashFirst`, `bashFirstSteer`: `strict` or `relaxed`). `CLAUDE_CODE_THRIFTY_SONIC=false` turns the steer off; `CLAUDE_CODE_COZY_TEAPOT=relaxed` selects the softer wording, which is today's text. Both variables are undocumented (not in the env-vars docs I read). |
| 2026-09-04/05 | [#92178](https://github.com/anthropics/claude-code/issues/92178), [#92271](https://github.com/anthropics/claude-code/issues/92271) | The `tengu_thrifty_sonic` flag gates the steer; on that install it was on for `fable[1m]` and off for Sonnet and Haiku. A later comment on #90450 (2026-09-08) says the assignment is made once per session and that subagents inherit the parent's, whatever model they run. |
| 2026-09-22 | [#96094](https://github.com/anthropics/claude-code/issues/96094) | The softer text is in use: "make small, mechanical file changes with sed, heredocs, or short scripts instead of the dedicated Read, Edit, or Write tools", described as "softer than an earlier version". |
| 2026-11-09 | #87971 comment | A user quotes today's full text, ending "The choice is yours: prefer Edit or Write when a shell edit would be fragile …". |
| 2026-11-10 | [#108715](https://github.com/anthropics/claude-code/issues/108715) | Quotes the same reminder in auto mode on CLI `2.1.317`. |
| 2026-11-11 | [#109143](https://github.com/anthropics/claude-code/issues/109143) | The closest match to this ticket: subagents (`general-purpose`, `model: sonnet`, CLI `2.1.322`) edit with `python3 - <<'EOF'` and `cat > file <<'EOF'` despite `CLAUDE.md` rules to use `Edit`/`Write`. One comment suggests a `PreToolUse` hook. |

What the reports establish: the steer exists since at least 2026-08-19; the relaxed wording is in use since at least 2026-09-22; other people see subagents edit through Bash against their `CLAUDE.md` in the same week (2026-11-11), with a Sonnet subagent model. What they do not establish: when the relaxed wording became the default, or whether any flag value changed on 11-15/16. The flag claims come from commenters reading the client, not from Anthropic.

**Inference, withdrawn:** the first version of this note said a remote flag, assigned per session, could explain a change with no CLI change, and that it fit the data. No flag is needed: the diagnosis finds the fall in the end of isolated spawns (section 1), with the same CLI and the same steer text on both sides.

---

## 4. Why the switch could not be caught from the changelog

The changelog has no dates (section 2.3), so I cannot place any entry against 11-15/16 except by the local CLI's version, `2.1.311`, which did not change. The API release notes list no event on those days (section 2.4). I found no Anthropic page that announces, dates or explains a change to the steer after August.

---

## 5. Things on the plotkeeper side that I checked

- The repo's own instructions do not tell agents to edit through Bash. `docs/agents/ticket-authoring.md` line 13 says the opposite for issue bodies: "change it with the Edit tool … Do not … rewrite it with a script" (commit `5f4e517`). I found no `heredoc`, `sed -i` or "through Bash" in `AGENTS.md`, `docs/agents/` or `.claude/` at that commit.
- The briefs were read by the diagnosis, which is the reading the first version of this note lacked. None names an edit tool. They do show how the orchestrators stopped isolating: before the target-dir rule (`9053cbe1c`, 11-15 00:15, "Steer parallel /implement-spec workers") they pointed every worker at the shared target; after it, each brief hands the worker a target dir, a branch and a worktree named by its ticket, made with `git worktree add`, where an isolated worktree is named `agent-<id>`, which the orchestrator cannot know before the spawn. From 11-15 10:19, 9 of 10 `/implement-spec` runs did not isolate their workers.

---

## 6. What a hook receives for a Bash edit

Official, from the [hooks reference](https://code.claude.com/docs/en/hooks), read 2026-11-18.

**Common fields, every event:** `session_id`, `prompt_id`, `transcript_path`, `cwd` ("Current working directory when the hook is invoked"), `scratchpad_dir`, `permission_mode` (`default`, `plan`, `acceptEdits`, `auto`, `dontAsk`, `bypassPermissions`), `effort`, `hook_event_name` ([common input fields](https://code.claude.com/docs/en/hooks#common-input-fields)). Inside a subagent the input also carries `agent_id` and `agent_type`, and settings and plugin hooks fire in subagents too ([hooks, subagents](https://code.claude.com/docs/en/hooks#hooks-in-subagents)).

**`PreToolUse` for Bash:** `tool_name`, `tool_use_id`, and `tool_input` with `command`, `description`, `timeout`, `run_in_background` ([PreToolUse input, Bash](https://code.claude.com/docs/en/hooks#bash)).

**`PostToolUse` for Bash:** the same, plus `tool_response` and `duration_ms` ([PostToolUse input](https://code.claude.com/docs/en/hooks#posttooluse-input)). The page's example shows `tool_response` for `Write` only. For Bash, plotkeeper's log shows the keys `bashEditDiff`, `interrupted`, `isImage`, `noOutputExpected`, `stderr`, `stdout` (event log, read 2026-11-18).

**`tool_response.bashEditDiff`** ([hooks, Bash](https://code.claude.com/docs/en/hooks#bash); setting [`bashEditDiffEnabled`](https://code.claude.com/docs/en/settings-reference#basheditdiffenabled); both "Requires Claude Code v2.1.269 or later"):

- Recorded when a Bash command changes files in a Git repository. In every permission mode if `bashEditDiffEnabled` is `true` (user or managed settings only); otherwise only in auto and bypass mode, and only when Claude Code directs Claude to edit through Bash. `false` turns it off. Background and read-only commands carry no diff.
- Fields: `changedFiles` (absolute paths, at most 200), `files` (diffs of up to 5 files, `created`/`deleted` flags), `moreFiles`, `unavailable`, `skipped` (set for `git checkout`, `git stash` and similar), `shared` (another Bash call, "such as a subagent's", ran in the same repository at the same time, so some listed changes may be that command's).
- Official caveats: "The list is best effort and in public beta. Claude Code can miss a change, include a file that another process changed at the same time, or stop at its size limits. The field shape may change. Use the list to find what to review, not to enforce a policy." Files Git ignores and files in submodules are not listed.
- `PostToolUse` for `Edit|Write` does not fire for a Bash edit: "Claude Code doesn't run a `PostToolUse` hook matching `Edit|Write` when a `Bash` command or a process outside Claude Code rewrites the same file" ([PostToolUse](https://code.claude.com/docs/en/hooks#posttooluse)).
- The docs also suggest a generic route: match every tool and "discover what changed itself, for example by running `git status --porcelain`, which also lists untracked files that `git diff` misses" (same section).

---

## 7. How `bashEditDiff` behaves in plotkeeper's log

I counted this on 2026-11-18 from the event log. It is local data, not a source about Claude Code. I took every subagent `PostToolUse` Bash event whose command looks like an edit (`python3 - <<`, `sed -i`, `cat >`, `open(…,'w')`), over the whole log (10-30 to 11-18), and split by whether the command starts with `cd` and whether the event's `cwd` is a worktree.

| Command | Event `cwd` | `bashEditDiff` lists changed files | present, nothing changed | absent |
|---|---|---|---|---|
| starts with `cd` | worktree | 413 | 48 | 197 |
| starts with `cd` | main checkout | 2 | 131 | 638 |
| no `cd` | worktree | 184 | 2 | 78 |
| no `cd` | main checkout | 1 | 17 | 36 |

In every case where `changedFiles` was present, its paths were under the event's `cwd`.

On 11-16 alone, of 388 edit-like subagent commands, 384 carry no `bashEditDiff` and 4 list a file.

**Inference:** the diff is taken for the repository of the session's `cwd` at the time. When the event's `cwd` is the main checkout and the command edits a worktree, the diff sees the main checkout, not the worktree, and reports nothing or is absent. The docs do not say which directory the diff uses; **Unverified.** The `shared` flag is also set on all 150-odd "present, nothing changed" cases for subagents on 11-17, which fits several subagents running in one session at once.

---

## 8. `cwd`, `CwdChanged` and `FileChanged`

**`cwd` follows Claude.** "The `cwd` field in the hook's input JSON is the worktree root after Claude enters a worktree, and the new directory after Claude runs `cd`" ([hooks, matcher patterns](https://code.claude.com/docs/en/hooks#matcher-patterns)). `${CLAUDE_PROJECT_DIR}` does not change: it stays the project root where the session started.

**`CwdChanged`** fires "when a shell command in the main conversation changes the working directory, for example when Claude executes a `cd` command", with `old_cwd` and `new_cwd` ([CwdChanged](https://code.claude.com/docs/en/hooks#cwdchanged)). It does not mention subagents. In plotkeeper's log (section 7), 771 of the 1,747 edit-like subagent commands start with `cd` and arrive with `cwd` = the main checkout, as the brief said; 658 start with `cd` and arrive with `cwd` = a worktree. So for subagents, `cwd` is sometimes the directory the subagent started in and sometimes where it `cd`ed to. That is an observation of 2026-11-18, not an official statement. The docs do not say how `cwd` behaves for a subagent's `cd`. **Unverified.**

**`FileChanged`** ([FileChanged](https://code.claude.com/docs/en/hooks#filechanged)):

- "Claude Code detects changes with a filesystem watcher, not by inspecting tool calls, so it runs the hook no matter what changed the file: an `Edit` or `Write` tool call, a script Claude runs with `Bash`, or a process outside Claude Code entirely."
- Input: the common fields, `file_path` (absolute) and `event` (`change`, `add`, `unlink`). No hook decision control; it cannot block.
- Limits: the `matcher` lists literal file names, split on `|`, "registered as a literal filename in the working directory", so it does not take globs or directories. More paths can be added with `watchPaths` returned from a `SessionStart`, `CwdChanged` or `FileChanged` hook. Matching a basename only. The watcher starts "only when something names a file to watch".
- It has no `agent_id` field in its documented input, so it cannot say which subagent wrote the file. **Inference** from the page's input table.

**Inference:** `FileChanged` does not suit the studio. It would need the studio to name every file in advance, and a worktree's files are not known before the work.

---

## 9. Can anything steer agents back to `Edit`?

Official guidance only.

- **`CLAUDE.md` does not enforce.** "Claude treats them as context, not enforced configuration. To block an action regardless of what Claude decides, use a PreToolUse hook instead" ([memory](https://code.claude.com/docs/en/memory)). "CLAUDE.md instructions shape Claude's behavior but are not a hard enforcement layer" (same page, managed settings). User reports say the instruction in `CLAUDE.md` loses to the steer (#109143, #87971).
- **A `PreToolUse` hook can deny.** It runs in subagents too, and can match `Bash` and return `permissionDecision: "deny"` ([PreToolUse decision control](https://code.claude.com/docs/en/hooks#pretooluse-decision-control)). Which commands count as an edit is left to the hook's author; the docs give no recognised list.
- **Permission rules reach only some Bash writes.** "Read and Edit deny rules apply to Claude's built-in file tools, to file commands Claude Code recognizes in Bash, such as `cat`, `head`, `tail`, `sed`, and `tee`, and to the targets of Bash redirections … They don't apply to … arbitrary subprocesses that read or write files indirectly, like a Python or Node script that opens files itself" ([permissions](https://code.claude.com/docs/en/permissions)). So `Edit(path)` deny rules stop `sed -i` and `> file`, and not the `python3 - <<EOF` heredoc, which is what plotkeeper's agents mostly use.
- **The sandbox is the OS-level answer.** "For OS-level enforcement that blocks all processes from accessing a path, enable the sandbox" (same page); the sandbox's write allow-list is `sandbox.filesystem.allowWrite` ([sandboxing](https://code.claude.com/docs/en/sandboxing)). It restricts where a Bash command may write; it does not make the agent prefer `Edit`, and the writes the studio wants (the worktree) are the ones it must allow.
- **Turning the steer off is not documented.** The only mentions are the undocumented flags named in the user reports of section 3 (`CLAUDE_CODE_THRIFTY_SONIC=false`, `CLAUDE_CODE_COZY_TEAPOT`). They are not in the docs I read, may change without notice and are not official guidance. Manual and `acceptEdits` mode did not receive the steer in the same reporter's test (#90450, user report).
- **`bashEditDiffEnabled`** is the official counterpart: a setting that exists because Claude Code expects Bash edits. It does not move edits back to `Edit`.

---

## 10. What this means for the studio

Code at [`5f4e517`](https://github.com/danielrispler/plotkeeper/tree/5f4e517466094ca9d632fb17b516ed86cd2cd2ad), as the ticket briefs it:

- The hook's `trees` function ([`hook/src/lib.rs:488`](https://github.com/danielrispler/plotkeeper/blob/5f4e517466094ca9d632fb17b516ed86cd2cd2ad/hook/src/lib.rs#L488)) takes a start tree for the event's `cwd` or for a `file_path` of an `Edit` or `Write` call (the filter is at [line 543](https://github.com/danielrispler/plotkeeper/blob/5f4e517466094ca9d632fb17b516ed86cd2cd2ad/hook/src/lib.rs#L543)).
- The ledger counts a diffed file as the turn's own only if an edit tool named it: `EDITS` and `edits` in [`core/src/turn_files.rs:10-13`](https://github.com/danielrispler/plotkeeper/blob/5f4e517466094ca9d632fb17b516ed86cd2cd2ad/core/src/turn_files.rs#L10-L13), `TurnFiles::of` at [line 81](https://github.com/danielrispler/plotkeeper/blob/5f4e517466094ca9d632fb17b516ed86cd2cd2ad/core/src/turn_files.rs#L81).
- Nothing in `hook/src/lib.rs`, `core/src/turn_files.rs` or `understanding/src/offer.rs` mentions `bashEditDiff`, so the field the hook already receives goes unused.

---

## 11. Designs I would try (all **Inference**)

None of this is official guidance. Each point rests on the sources above.

1. **Read `tool_response.bashEditDiff.changedFiles` in the hook** and treat those paths like `Edit` paths: record the start tree for the repository that holds them, and count them as the turn's own files. It is already in the event line. Its limits are the official ones (best effort, auto and bypass mode, beta shape) plus section 7's gap for `cd`.
2. **Do not trust `cwd` for a Bash command.** Parse the leading `cd <dir> &&` from `tool_input.command`, or find the worktree of each `changedFiles` path (`git rev-parse --show-toplevel` on its folder), and take the start tree there. Parsing is fragile for `cd` in a middle of a pipeline; the path route is not.
3. **Rely on git for the truth, at `Stop`.** The docs themselves suggest a hook that "discover[s] what changed itself, for example by running `git status --porcelain`". The studio already diffs worktrees at turn end; the gap is only the "own file" test. A file that changed in a worktree whose start tree is known can be counted as the turn's own when no other turn of that terminal touched it (the same `shared` problem the docs warn about).
4. **Record the agent's tool mix.** `permission_mode` and `agent_id` are in every event, so the studio can show "this turn edited through Bash" and skip the claim that nothing changed.
5. **Optional: a `PreToolUse` Bash hook that denies edit-like commands in worktrees** with a message that names `Edit`. It would restore the old event shape, but it fights Claude Code's own steer and also the studio's own use of Bash in `cargo` and `git` commands. Only worth it if 1 to 3 fail. **Unverified.**

---

## 12. What I could not establish

- **Why the orchestrators stopped isolating.** The diagnosis narrows it to the two `AGENTS.md` rules of 11-14 and 11-15 (worktrees under `.claude/worktrees/`, a target dir per worker), with the plugins that went off on 11-14 as a second candidate; the data cannot split them, and the orchestrators' thinking is not kept.
- **When the relaxed wording became the default.** User reports show it by 2026-09-22; the local transcripts show it from 10-13. Earlier local transcripts are gone.
- **Which directory `bashEditDiff` uses** when a command `cd`s elsewhere, and how a subagent's `cwd` is set. The docs are silent; section 7 is observation only.
- **That the steer is the cause at all.** It is the best-supported enabler. A model that was told it may edit through Bash can still choose either, and the main session in the same period kept using `Edit`.

## Sources read (all on 2026-11-18)

- Claude Code docs: [hooks](https://code.claude.com/docs/en/hooks), [settings reference](https://code.claude.com/docs/en/settings-reference), [permissions](https://code.claude.com/docs/en/permissions), [permission modes](https://code.claude.com/docs/en/permission-modes), [sandboxing](https://code.claude.com/docs/en/sandboxing), [memory](https://code.claude.com/docs/en/memory), [tools reference](https://code.claude.com/docs/en/tools-reference), [auto mode config](https://code.claude.com/docs/en/auto-mode-config), env-vars.
- [Claude Code changelog](https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md) (undated; head `2.1.327`).
- [API release notes](https://platform.claude.com/docs/en/release-notes/overview).
- GitHub issues on `anthropics/claude-code` (user reports): #87971, #89251, #90450, #91956, #92178, #92271, #96094, #108715, #109143, #109723.
- Local: plotkeeper's event log (`events.jsonl`) and subagent transcripts under `~/.claude/projects/`, counts only; `claude --version` = `2.1.311`.
