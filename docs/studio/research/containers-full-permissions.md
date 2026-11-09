# Containers with full permissions

> **Answer:** Don't build it now. Running a task's sessions in a container with `--dangerously-skip-permissions` breaks three things the studio relies on today: the hook writes to a macOS path with a macOS binary, the host reads the session by its container paths, and the answer hook checks the studio's pid, which a container cannot see. Making it work means mounting the worktree, the main checkout's `.git`, the studio's data folder and a Claude config folder into the container at their host paths. That hands a session with no prompts the main repository's `.git/hooks`, the event log of every other session and its own login token. On macOS it does contain the rest of the disk, because the container runs in a Linux VM. "Needs you" shrinks to questions and the `?` at a turn's end.
> **Smaller step:** if Daniel wants fewer prompts, offer a per-session choice of Claude Code's auto mode, which keeps the studio's hooks and "needs you" as they are. If he wants a whole-process boundary, try Anthropic's sandbox runtime (`srt claude`) on macOS first: it keeps host paths, pids and file events, and needs no Docker. Both keep "never in bypass mode by default".
> **Evidence:** Strong for Claude Code's behaviour (its docs) and for the studio's coupling (its code at one commit). Weak for file events on Docker Desktop's macOS mounts: no current Docker page states it, and user reports disagree. Nothing was run: no container, no `claude` in bypass mode.
> **herdr:** herdr does not contain agents. It moves them to another machine over SSH and carries back terminals, status and files, not hook events (section 8, from the #530 note).

Research date: 2026-11-09. Ticket: [#531](https://github.com/danielrispler/Falinks/issues/531). Spec: [#517](https://github.com/danielrispler/Falinks/issues/517), decision 23. Map: [#266](https://github.com/danielrispler/Falinks/issues/266). The studio's code is read at commit [`ad16f07`](https://github.com/danielrispler/Falinks/tree/ad16f07), the head of `issue-517-studio-round-5` that day; code links below point there. Claude Code docs are read at `code.claude.com/docs` on the same day.

**How to read this.** A sentence with a link is established by that source. A sentence marked **Inference** is my own reading, which the sources do not state. **Unverified** marks something that would need a run to confirm.

---

## Words used in this note

- **Bypass mode:** Claude Code's `bypassPermissions` permission mode, set by `--dangerously-skip-permissions`. It "disables permission prompts and safety checks so tool calls execute immediately" ([permission modes](https://code.claude.com/docs/en/permission-modes)).
- **Container:** here, a Linux container run by Docker Desktop or by Apple's `container` tool. On macOS both run Linux in a VM.
- **Sandbox runtime (`srt`):** Anthropic's `@anthropic-ai/sandbox-runtime`, which wraps a whole process, Claude Code included, in Seatbelt on macOS or bubblewrap on Linux ([sandbox environments](https://code.claude.com/docs/en/sandbox-environments#sandbox-runtime)).

---

## 1. What the engine does instead (ADR 0002)

The engine never uses bypass mode or containers. It runs Claude Code with `--permission-mode dontAsk`, an allow list and a deny list ([`adapters/claude/lib.rs:788`](https://github.com/danielrispler/Falinks/blob/ad16f07/adapters/claude/lib.rs#L788), [`:815-816`](https://github.com/danielrispler/Falinks/blob/ad16f07/adapters/claude/lib.rs#L815-L816)). `dontAsk` "auto-denies every tool call that would otherwise prompt you" ([permission modes](https://code.claude.com/docs/en/permission-modes)). Claude Code's own sandbox is off, and every Bash command goes through `falinks-claude-shell` as `CLAUDE_CODE_SHELL_PREFIX` ([`lib.rs:781-794`](https://github.com/danielrispler/Falinks/blob/ad16f07/adapters/claude/lib.rs#L781-L794)). That shell confines itself before it execs the command: writes only under scratch, no network ([`adapters/claude/shell.rs:1-10`](https://github.com/danielrispler/Falinks/blob/ad16f07/adapters/claude/shell.rs#L1-L10)). On Linux the confinement is Landlock and seccomp, installed by the single-threaded `falinks-contain` helper ([ADR 0002](https://github.com/danielrispler/Falinks/blob/ad16f07/docs/adr/0002-linux-containment-runs-in-a-helper-executable.md)). On macOS it is `sandbox-exec`. A platform that is not supported fails closed and "never falls back to weaker containment" ([`docs/platforms.md`](https://github.com/danielrispler/Falinks/blob/ad16f07/docs/platforms.md)).

The two approaches differ in kind:

| | Engine | Container plus bypass |
|---|---|---|
| Who decides a tool call | allow and deny rules; anything else is denied | nobody: every call runs, except the few no mode approves |
| What is confined | each Bash command, per command | the whole Claude Code process, hooks included |
| Boundary | Seatbelt or Landlock on the host kernel | the container's mounts and network; on macOS a Linux VM |
| Writes allowed | scratch only | everything mounted writable, which must include the worktree |
| Failure mode | fails closed | none built in: a missing or wrong mount just exposes more |

**Inference:** the engine's design answers "what may this worker do". A container answers "what can this session reach". The engine's model fits unattended workers; the container fits a session Daniel will not watch. The studio's sessions are ones Daniel watches.

---

## 2. Containment it actually gives

**macOS.** Docker Desktop runs containers in a Linux VM; Apple's `container` runs "a lightweight VM for each container that you create", with "the isolation properties of a full VM" ([apple/container technical overview](https://github.com/apple/container/blob/main/docs/technical-overview.md)). Full function needs macOS 26 (same page). So the macOS disk, keychain and processes are out of reach, except what is mounted. **Inference:** that is a stronger boundary than the engine's Seatbelt profile, for whatever is not mounted.

**Linux.** A container there shares the host kernel. Claude Code's docs put containers in the same row as dev containers and keep "kernel-level separation" for VMs: a dedicated VM "provides the strongest separation, with its own kernel" ([sandbox environments](https://code.claude.com/docs/en/sandbox-environments#virtual-machine)). The studio is a macOS app ([`apps/studio/README.md`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/README.md), first line), and `docs/platforms.md` has no studio row. So Linux is moot for the studio today.

**What it does not contain, on either.** Claude Code's own warning: with bypass mode, dev containers "do not prevent a malicious project from exfiltrating anything accessible inside the container, including the Claude Code credentials stored in `~/.claude`" ([dev containers](https://code.claude.com/docs/en/devcontainer)). Claude "can still modify any file in the bind-mounted workspace, which appears directly on your host, and reach anything the container's network policy allows" (same page, "Run without permission prompts"). The docs pair bypass mode with an egress firewall, which needs `NET_ADMIN` and `NET_RAW` (same page, "Restrict network egress"). And the CLI refuses bypass mode as root, so the container must run as a non-root user ([permission modes](https://code.claude.com/docs/en/permission-modes)).

What would have to be mounted (sections 3 to 5) is where the boundary leaks:

- **The main checkout's `.git`.** A task's worktree has a `.git` file that points at `<main>/.git/worktrees/<task>`, and links are absolute by default ([git-worktree](https://git-scm.com/docs/git-worktree), `--relative-paths`). Commits inside the container write objects to the main `.git`, so it must be mounted writable. That exposes `.git/hooks` and `.git/config`, which run code on the host at Daniel's next `git` command. Anthropic's sandbox runtime denies exactly these at the project root for this reason ([sandbox environments](https://code.claude.com/docs/en/sandbox-environments#what-the-runtime-blocks-on-its-own)). **Inference:** this is the largest hole; it turns "contained" into "one `git commit` from the host".
- **The studio's data folder.** See section 3: every session's event log and the answer files live there.
- **The Claude config folder.** See section 4: it holds the login.

---

## 3. How the hook and the terminal daemon reach the session

**The hook runs inside the container.** In bypass mode inside a container, hooks are inside the boundary ([sandbox environments](https://code.claude.com/docs/en/sandbox-environments#compare-sandboxing-approaches)). Today's plugin runs `${CLAUDE_PLUGIN_ROOT}/../target/release/studio-hook` ([`plugin/hooks/hooks.json`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/plugin/hooks/hooks.json)), a macOS build, which a Linux container cannot run. The container needs a Linux build of `studio-hook` and `studio-answer`.

**The hook writes to a macOS path.** `studio-hook` appends to `$HOME/Library/Application Support/dev.falinks.studio/events.jsonl` ([`hook/src/lib.rs:13-20`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/hook/src/lib.rs#L13-L20), [`hook/src/main.rs:32`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/hook/src/main.rs#L32)). Inside a container that is the container's home. The host's data folder would have to be mounted there, writable. That gives the session every other session's log lines, and lets it append forged lines that set any session's state. **Inference:** the card writer already treats a change as untrusted, but the core treats log lines as fact.

**The host reads the session by its paths.** The app tails the log every 200 ms ([`src-tauri/src/main.rs:400-401`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/src-tauri/src/main.rs#L400-L401), [`:554`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/src-tauri/src/main.rs#L554)), then acts on paths from the payload:

- it reads the transcript at the hook's `transcript_path` ([`core/src/transcript.rs:1-8`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/core/src/transcript.rs#L1-L8));
- it fingerprints the worktree by running `git` with the event's folder as its working directory ([`understanding/src/fingerprint.rs:288-294`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/understanding/src/fingerprint.rs#L288-L294)), and so does the hook ([`hook/src/lib.rs:110-114`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/hook/src/lib.rs#L110-L114)).

Both are container paths. Unless the worktree, the main `.git` and the transcript folder are mounted at the same absolute paths on both sides, the transcript is unreadable (the view falls back to steps only, per `CONTEXT.md`, **Transcript**) and no card is written. Claude Code also names a transcript's folder after the working directory's path ([`.claude` directory](https://code.claude.com/docs/en/claude-directory#session-scratchpad-directory)), so `claude --resume` on the host would not find a container session unless the paths match. **Unverified:** that identical-path mounts are enough; nothing was run.

**The answer hook cannot see the studio.** `studio-answer` waits for a card answer only while the studio "watches" the session. It checks that the pid in `watching.json` is alive with `kill(pid, 0)` ([`hook/src/answer.rs:111-124`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/hook/src/answer.rs#L111-L124)). The studio's macOS pid does not exist in the container's pid namespace. **Inference:** the check fails, the hook returns at once with no decision ([`answer.rs:216-219`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/hook/src/answer.rs#L216-L219)), and every question is answered in the terminal only. It fails safe, but the card's answer buttons stop working. The answer file's trust check also compares the file's owner with the hook's uid ([`answer.rs:296-300`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/hook/src/answer.rs#L296-L300)); across a VM file share, the owner the container sees is not Daniel's macOS uid. **Unverified.**

**The terminal daemon is the easy part.** It runs every terminal as `$SHELL -l -c 'exec "$@"' studio <argv>` ([`terminal-daemon/src/daemon.rs:355-363`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/terminal-daemon/src/daemon.rs#L355-L363)), and the session's argv comes from one function ([`workbench/src/lib.rs:71-81`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/workbench/src/lib.rs#L71-L81)). An argv of `docker run -it …` or `container run -it …` gets a PTY like any program. Two changes: pass `STUDIO_TERMINAL_ID` into the container, since the daemon sets it on the host only and the hook reads it from its own environment ([`hook/src/main.rs:28-30`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/hook/src/main.rs#L28-L30)); and make revival's `claude --resume` re-enter the same image with the same mounts. **Inference:** "Close" kills the terminal's process group, which ends the `docker` client; whether the container stops with it depends on `--rm` and signal forwarding. **Unverified.**

---

## 4. Logging in without the studio touching credentials

The hard rule: the studio never reads, copies or sends a CLI's credentials or tokens ([`apps/studio/AGENTS.md`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/AGENTS.md), "Hard rules").

- **Host login does not carry over.** On macOS, Claude Code keeps the login in the Keychain; on Linux in `~/.claude/.credentials.json`, mode 0600 ([authentication](https://code.claude.com/docs/en/authentication#credential-management)). A Linux container cannot read the macOS Keychain, so it needs its own login.
- **Login inside the container is allowed by the rule.** Run `claude` in the container and follow its browser prompt; with no browser, copy the URL and paste the code back ([dev containers](https://code.claude.com/docs/en/devcontainer), step 3). Daniel does this in the studio's terminal; the studio only carries the bytes, as it does for any typing.
- **Keep it across restarts with a named volume.** Mount a volume at the container's `~/.claude` and set `CLAUDE_CONFIG_DIR` to it, so `.claude.json` is kept too ([dev containers](https://code.claude.com/docs/en/devcontainer#persist-authentication-and-settings-across-rebuilds)).
- **Not allowed: the studio passing a token.** `claude setup-token` prints a one-year token that only works as `CLAUDE_CODE_OAUTH_TOKEN` in the environment ([authentication](https://code.claude.com/docs/en/authentication#generate-a-long-lived-token)). The studio would have to store and send it, which the hard rule forbids.

The conflict: the transcripts live in the same config folder as the login (`projects/<project>/<session>.jsonl`, [`.claude` directory](https://code.claude.com/docs/en/claude-directory#cleaned-up-automatically)), and section 3 needs them at a host path. **Inference:** a named volume for the config folder plus a host bind mount over its `projects/` subfolder would keep the login off the host's disk. **Unverified.** Either way, the bypass session itself can read its own login and send it out, which is the docs' own warning (section 2). **Inference:** a subscription token in a session with no prompts and open network is the main thing a container does not protect.

---

## 5. File watching on a worktree mounted from macOS

The studio itself does not use file-system events. It polls the event log every 200 ms ([`main.rs:400`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/src-tauri/src/main.rs#L400)) and fingerprints worktrees with `git` (section 3). So the question is about tools inside the container (watchers, `cargo watch`, dev servers) seeing changes on the mounted worktree, and the host seeing the container's writes.

- Docker Desktop's newer **synchronized file shares** keep a cache on ext4 inside the VM and sync both ways; Docker recommends them because VirtioFS and gRPC FUSE "are no longer scaling well" for large repositories ([synchronized file shares](https://docs.docker.com/desktop/features/synchronized-file-sharing/)). The page does not say whether inotify events reach containers.
- No current Docker page states inotify behaviour on VirtioFS. The archived osxfs page said most events propagated, except through symlinked mounts such as `/tmp` ([osxfs, archived](https://gitea.cncfstack.com/docker/docs/raw/commit/29f7d3396b90bc02b1350a4dde7dd4397688b11f/docker-for-mac/osxfs.md)). Current user reports say events can be missed or delayed under VirtioFS ([Docker forum](https://forums.docker.com/t/long-lived-tail-f-on-volume-mounts/148436); [Rancher Desktop #1979](https://github.com/rancher-sandbox/rancher-desktop/issues/1979)).
- Apple's `container` docs do not mention file events ([technical overview](https://github.com/apple/container/blob/main/docs/technical-overview.md)).

**Inference:** a sync-based share would make a file the agent wrote reach the host later than the hook's event, so a fingerprint at `Stop` could miss it. The studio's 20-minute fallback look would catch it, late. **Unverified** on every point; this needs a measured run.

---

## 6. What happens to "needs you"

Today "needs you" comes from `PreToolUse` of `AskUserQuestion` and `ExitPlanMode`, from `PermissionRequest`, from a `permission_prompt` notification, from `Elicitation`, and before #540 also from a `?` sentence at `Stop` ([`core/src/lib.rs:1094-1095`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/core/src/lib.rs#L1094-L1095), [`:2314-2320`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/core/src/lib.rs#L2314-L2320); map #266, minimal version).

In bypass mode:

- **Permissions go.** `PermissionRequest` hooks "run only when Claude Code is about to ask you for permission" ([hooks](https://code.claude.com/docs/en/hooks)), and bypass mode asks only for the few actions no mode approves: explicit ask rules, critical-path `rm`, and the out-of-workspace read and write blocks when turned on ([permission modes](https://code.claude.com/docs/en/permission-modes)). So permission waits nearly vanish.
- **Questions stay.** `AskUserQuestion` still prompts in every mode (same page). Elicitations from MCP servers are not permission prompts, so **Inference:** they stay too.
- **Plans mostly go.** In interactive bypass sessions "plan mode's blocks aren't enforced": an edit or command during planning runs without prompting (same page). **Inference:** plan approval stops being a gate, so a plan "needs you" is a formality.
- **A text question stays your turn.** Since #540 a `?` sentence at `Stop` no longer means "needs you", and the mode does not change that.

**Inference:** "needs you" would become rare and mean "the agent asked me something", which suits a calm control room. But the decision history loses its permission entries, and Daniel loses his chance to stop a risky call, which is what "never in bypass mode by default" protects (map #266: "agents start with normal permission prompts, never in bypass mode by default"). Hook payloads carry `permission_mode`, including `"bypassPermissions"` ([hooks](https://code.claude.com/docs/en/hooks), common input fields), so the studio could at least mark such a session.

---

## 7. Smaller steps the sources point to

1. **Auto mode per session.** Claude Code's docs offer it as the alternative for "fewer prompts without disabling safety checks": a classifier reviews each action ([dev containers](https://code.claude.com/docs/en/devcontainer), "Run without permission prompts"). It needs no container, keeps host paths, and still raises prompts the studio already handles. It "does not guarantee safety" ([permission modes](https://code.claude.com/docs/en/permission-modes)). The change is one flag in [`workbench/src/lib.rs:71-81`](https://github.com/danielrispler/Falinks/blob/ad16f07/apps/studio/workbench/src/lib.rs#L71-L81), opt-in from the picker.
2. **The sandbox runtime on macOS.** `srt claude` wraps the whole process, hooks and MCP servers included, in Seatbelt, with no Docker ([sandbox environments](https://code.claude.com/docs/en/sandbox-environments#sandbox-runtime)). It denies `.git/hooks`, `.git/config` and `.claude/` writes at the project root on its own, checked at write time on macOS (same section). **Inference:** host paths, the pid namespace and file events stay the same, so sections 3 and 5 mostly disappear; it needs `allowWrite` for the studio's data folder and the main `.git`'s objects. It is a "beta research preview" (same section). Bypass mode under it would still be the docs' recommended pairing, and still Daniel's choice per session, not a default.
3. **Mark bypass sessions.** Whatever Daniel runs by hand, show `permission_mode` on the row so a bypass session is never mistaken for a guarded one.

---

## 8. herdr's approach

This section rests on the herdr note for #530 ([`docs/studio/research/herdr.md` at `5940e56`](https://github.com/danielrispler/Falinks/blob/5940e56/docs/studio/research/herdr.md), on branch `issue-530-herdr-research`), and on the herdr-core docs it cites.

- **No containment of its own.** The note finds no container or permission-mode feature in herdr. Its isolation is "another machine": `hc machine add user@host`, then `hc run --on box …`; "the remote core owns its programs, terminal history, and files" ([herdr-core machines](https://github.com/herdrdev/herdr-core/blob/master/docs/machines.md)). A `--via` command carries the protocol through any pipe, with examples for `fly ssh console` and a Vercel Sandbox helper (same page). So a sandbox or VM can be the remote machine, but herdr leaves it to the user.
- **SSH is the trust layer.** herdr-core "does not add another encryption layer or authenticate a remote user itself" ([herdr-core security](https://github.com/herdrdev/herdr-core/blob/master/docs/security.md)).
- **It carries terminals, not understanding.** The note's verdict on remote machines: "A remote session would need those carried back too, not only its terminal. herdr does not solve that part: it carries terminals, status and files, not hook events" (herdr note, feature 3).

**Inference:** herdr hits the same wall as section 3. A container is a remote machine on the same Mac: the terminal crosses easily, and the hook events, transcripts and Git state do not. herdr's answer (any remote, user's choice, terminals only) works because herdr has no understanding layer. The studio has one, so containers and the herdr note's direction change A (remote places) need the same piece first: a way to bring hook events, transcripts and worktree state back from a session that is not on the host. That is one design, worth doing once, after Daniel decides direction A.

---

## Recommendation

**Don't build containers with full permissions now. Take a smaller step.**

Reasons, in order:

1. The coupling is deep. The hook binary, the event log path, transcript paths, worktree paths and the answer hook's pid check all assume one host (section 3). Making them work needs path-identical mounts of the worktree, the main `.git`, the studio's data folder and transcripts.
2. Those mounts undo much of the containment: `.git/hooks` and `.git/config` reach the host, the event log is forgeable, and the login sits inside a session with no prompts and open network (sections 2 and 4).
3. Daniel watches his sessions. The control room's value includes deciding permissions from a card; bypass removes that (section 6).

Smaller step: if Daniel wants fewer prompts, add an opt-in auto-mode choice to the session picker (step 1). If he wants a hard boundary, spike `srt claude` on macOS before any container (step 2). Revisit containers only for unattended runs Daniel does not watch, which is the engine's territory, not the studio's.

**Direction:** nothing here argues against "never in bypass mode by default". The recommendation keeps it.
