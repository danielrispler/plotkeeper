# Sandbox gate: loopback, failing closed, and a private temp folder

Research for #395 (the sandbox gate), 24 November 2026. plotkeeper's background runs use Claude Code's OS sandbox for the agent and `srt` (`@anthropic-ai/sandbox-runtime`) for the checks. Three problems came out of measuring them on macOS (Claude Code 2.1.323): loopback, a sandbox that is unavailable but does not stop the run, and a temp folder shared with other sessions. This note asks how other tools solve each one.

Studio terms follow `docs/studio/CONTEXT.md`.

## How this was read

- **Code.** Shallow clones under the session scratchpad, read only, never built or run:
  - `anthropic-experimental/sandbox-runtime` at `0bbebed` (prefix `srt:`). The installed npm package is 0.0.83; its README is the one cited as `srt README`.
  - `openai/codex` at `56bb44c` (`codex:`)
  - `google-gemini/gemini-cli` at `aafdfdf` (`gemini:`)
  - Citation form: `srt:src/x.ts:10-20` is that path at that commit, lines 10 to 20.
- **Claude Code.** Closed source. The installed binary (Homebrew cask 2.1.325, two versions past the one measured) was read only through `strings`, which shows minified code. Claims from it are marked as such.
- **Docs.** Each tool's official pages, cited by URL.
- **Limits.** No sandbox was run and no tool was started. Each claim says **verified** (I read the source) or **unverified** (inferred or secondary), with a confidence.

## Headline

Ranked by fit for plotkeeper, best first. "Ranked" is my judgement; the facts under each are cited in the sections below.

### 1. Loopback (tests need their own servers, not the host's)

1. **A private loopback: own network namespace or VM.** Linux tools get it free from `bwrap --unshare-net` (srt, Codex, Gemini CLI); containers and microVMs have their own (devcontainer, Docker Sandboxes, OpenHands, E2B, Apple `container`). Consequence: the real fix, but macOS Seatbelt has no equivalent, so on macOS it means running the checks in a Linux VM or container.
2. **Bind anywhere, connect only to listed loopback ports.** Seatbelt can scope *connect* by port, `(allow network-outbound (remote tcp "localhost:PORT"))`; it cannot scope *bind* by port in practice. nono exposes exactly this (`--open-port`, ranges, cap 16,384 rules); srt, Codex and Gemini use it only for their own proxy ports. Consequence: a test's `127.0.0.1:0` server lands on an ephemeral port (49152-65535), so allowing that range only keeps the well-known host services (Postgres 5432, Ollama 11434, dev servers 3000/5173/8080) out, but not host listeners that themselves sit on ephemeral ports. Neither srt's config nor Claude Code's settings expose per-port loopback, so plotkeeper would need its own Seatbelt profile or a wrapper such as nono for the check run.
3. **All loopback (`allowLocalBinding: true`).** srt, Claude Code, Codex. Consequence: tests pass; every host loopback service is reachable. Every vendor documents this trade-off in plain words.
4. **One host port through the proxy** (`allowedDomains: ["127.0.0.1:3000"]`, srt and Claude Code). Consequence: reaches one known host service over HTTP; does nothing for a test that binds its own server.
5. **Keep loopback closed** (Codex network-off, srt and Claude Code default). Consequence: tests with local servers fail and must be excluded or reported as "needs network".

### 2. Sandbox unavailable but the run "succeeds"

1. **Prove the sandbox works before the run, with a canary, and stop on failure.** Run a trivial command under the same sandbox and expect success, and a forbidden action and expect denial. Precedents: Claude Code's reference devcontainer `init-firewall.sh` (must fail to reach example.com, must reach api.github.com, else `exit 1`); srt on Windows (`initialize()` "refuses to start" if its egress probe gets out). Consequence: catches the nested-Seatbelt case, which none of the startup checks below see.
2. **Apply the sandbox in the launcher and abort on error.** nono calls `sandbox_init()` in its own process and returns `SandboxInit` with the kernel's message before running anything. Consequence: fail closed by construction; not available for Claude Code's own per-command sandbox.
3. **Startup dependency check** (Claude Code `failIfUnavailable`, srt `checkDependencies`, Cursor on Linux). Consequence: misses this case. On macOS srt's check reports nothing at all, and Claude Code's docs say that after startup a failed sandbox only refuses commands while "The session keeps running", which is the exit-0 run plotkeeper saw.
4. **Read the run afterwards.** Codex guesses denials from exit codes and words ("operation not permitted", "sandbox"). Consequence: a good backstop (fail the run if any tool result shows `sandbox_apply:` or `Sandbox is required`), not a gate.

### 3. Shared temp folder

1. **A short `CLAUDE_CODE_TMPDIR` per run.** Documented: Claude Code appends `/claude-{uid}/` and gives sandboxed commands that folder as `$TMPDIR`, unless the path is long; the 2.1.325 binary puts the limit at 44 bytes. plotkeeper's earlier attempt most likely used a long path and fell back to the shared `/tmp/claude-<uid>`. Consequence: likely the supported fix for the agent side, e.g. `/tmp/pk-<8 hex>`; needs one measurement. For the `srt` check run, srt reads the same variable from its own environment for the child's `TMPDIR`, and the dir must be added to `allowWrite`.
2. **Per-run `mkdtemp` (0700), set `TMPDIR`/`TMP`/`TEMP`, allow writes only there.** Gemini CLI's whole-CLI Seatbelt sandbox does exactly this on macOS. Consequence: the clean model, and what plotkeeper can do for its own `srt` run.
3. **A private `/tmp` mount** (`bwrap --tmpfs /tmp`: Gemini CLI Linux; Cursor `privateTmp` on Linux). Consequence: Linux only.
4. **Per-project private temp** (Cursor Workspace mode). Consequence: isolates projects, not runs.
5. **Shared system temp** (Codex, nono, Gemini CLI's tool sandbox on macOS). Consequence: the common default; it has the same exposure as plotkeeper today.
6. **Deny-write each existing entry at run start** (plotkeeper's current choice). No tool found doing this. Consequence: works, but a session folder created after the run starts is not covered.

## Problem 1: loopback

| Tool | Mechanism | Source | Verified? |
| :- | :- | :- | :- |
| srt (macOS) | `allowLocalBinding` adds bind `*:*`, inbound `*:*`, outbound `localhost:*`; otherwise loopback only to its proxy ports, one rule per port; no per-port config key | `srt:src/sandbox/macos-sandbox-utils.ts:1280-1342`; `srt:src/sandbox/sandbox-config.ts:726` | Verified, high |
| srt (Linux) | `bwrap --unshare-net`: private loopback, `allowLocalBinding` ignored | `srt:src/sandbox/linux-sandbox-utils.ts:3699`; srt README | Verified, high |
| srt / Claude Code | IP literal in `allowedDomains` (`127.0.0.1:3000`) reaches one host port via the proxy | srt README "Resolved-address check"; [sandboxing](https://code.claude.com/docs/en/sandboxing) | Verified, high |
| Claude Code | macOS: `allowLocalBinding` = "connect to any port on localhost, which includes every other service listening there"; Linux: localhost private | [sandboxing](https://code.claude.com/docs/en/sandboxing) | Verified, high |
| Codex CLI | Same three rules as srt under `allow_local_binding`; per-port outbound for proxy `loopback_ports`; network off = no loopback; Linux `--unshare-net` | `codex:codex-rs/sandboxing/src/seatbelt.rs:288-321`; `codex:codex-rs/linux-sandbox/src/bwrap.rs:287` | Verified, high |
| Gemini CLI | Whole-CLI profiles: fixed ports only (debugger 9229 inbound, proxy 8877 outbound) or all network; tool sandbox: all or nothing on macOS, `--unshare-all` on Linux | `gemini:packages/cli/src/utils/sandbox-macos-strict-proxied.sb:127-133`; `gemini:packages/core/src/sandbox/macos/baseProfile.ts:217-223`; `.../linux/bwrapArgsBuilder.ts:45-53` | Verified, high |
| nono | Per-port and port-range loopback connect (`--open-port`), blanket bind/inbound because "Seatbelt cannot filter bind/inbound by port"; cap 16,384 ports ("sandbox_init crashes ... above ~17,770 rules") | `nono:crates/nono/src/sandbox/macos.rs:463-494, 843-903`; `nono:crates/nono/src/capability.rs:13-16`; `nono:crates/nono-cli/src/cli.rs:1229-1235` | Verified, high (the crash figure is nono's claim) |
| Claude Code devcontainer | Container's own `lo` accepted; also the host's /24 accepted; rest default DROP | `anthropics/claude-code` `.devcontainer/init-firewall.sh` at `d945a61`, lines 36-38, 99-109 | Verified, high |
| Docker Sandboxes | microVM, own kernel; outbound TCP proxied through the host by policy; "cannot access ... your host network directly" | [security](https://docs.docker.com/ai/sandboxes/security), [architecture](https://docs.docker.com/ai/sandboxes/architecture/) | Verified (docs), high; own loopback inferred |
| OpenHands | Docker container per sandbox; ports published on random host ports, or host networking opt-in | [Docker runtime](https://docs.openhands.dev/usage/runtimes/docker) | Verified (docs), medium |
| Apple `container` | Linux containers "as lightweight virtual machines on your Mac", macOS 26+ | [apple/container README](https://github.com/apple/container) | Verified, high; own loopback inferred |
| E2B, Daytona, Devin | Remote VMs or containers, so loopback is theirs, not the host's | not read this session | Unverified |

**Can Seatbelt express "connect only to ports this process bound"?** No. The profile is a fixed string given to `sandbox-exec -p` before the command starts (`srt:src/sandbox/macos-sandbox-utils.ts:1590-1600`), and the host filter takes only `localhost` or `*` (srt comment, `:1266`). A port list must be known up front. Verified that no tool does it; that it is impossible is inference, high.

**Is there a macOS network namespace?** No tool found one. Every macOS tool here either opens all loopback or lists ports; the ones with a private loopback on a Mac use a VM (Docker Sandboxes, Apple `container`). Inference from the set read, medium-high.

## Problem 2: sandbox unavailable

| Tool | Mechanism | Source | Verified? |
| :- | :- | :- | :- |
| Claude Code | `failIfUnavailable` (default off) exits at startup only when "the platform doesn't support the sandbox or the dependency check at startup fails"; exit 1, `sandbox_required_unavailable` in stream-json. After startup: commands refused, "The session keeps running" | [sandboxing](https://code.claude.com/docs/en/sandboxing); 2.1.325 binary strings | Verified, high (docs); medium-high (binary) |
| srt (macOS) | `checkDependencies` returns no errors on macOS; no probe | `srt:src/sandbox/sandbox-manager.ts:1883-1955` | Verified, high |
| srt (Windows) | `initialize()` runs egress probes as the sandbox account and refuses to start if one gets out | srt README, WFP section | Verified (README), high |
| Codex CLI | Post-hoc heuristic `is_likely_sandbox_denied`; Linux `bwrap /bin/true` preflight to pick options; tests skip on `sandbox_apply` | `codex:codex-rs/sandboxing/src/denial.rs:1-60`; `codex:codex-rs/linux-sandbox/src/linux_run_main.rs:487-507`; `seatbelt_tests.rs:2885` | Verified, high |
| nono | `sandbox_init()` in-process; non-zero aborts with the kernel's message | `nono:crates/nono/src/sandbox/macos.rs:940-978` | Verified, high |
| Cursor | Linux kernel too old: "falls back to asking for approval before running commands" | [run modes](https://cursor.com/docs/agent/security/run-modes) | Verified (docs), high |
| Gemini CLI | Docs list prerequisites; no fallback described | [sandbox docs](https://geminicli.com/docs/cli/sandbox/) | Verified (docs), medium |
| Claude Code devcontainer | Self-test after setup: must not reach example.com, must reach api.github.com, else `exit 1` | `init-firewall.sh` at `d945a61`, lines 121-138 | Verified, high |

## Problem 3: temp folder

| Tool | Mechanism | Source | Verified? |
| :- | :- | :- | :- |
| Claude Code | Sandboxed `$TMPDIR` = `<CLAUDE_CODE_TMPDIR or /tmp>/claude-{uid}`; falls back to `/tmp/claude-{uid}` if longer than 44 bytes | [env-vars](https://code.claude.com/docs/en/env-vars); 2.1.325 binary (`yKr=44`, `Ije()`) | Docs verified, high; 44-byte figure medium-high |
| srt | Child `TMPDIR` = `CLAUDE_CODE_TMPDIR` or `CLAUDE_TMPDIR` or `/tmp/claude`; only `/tmp/claude` writable by default | `srt:src/sandbox/sandbox-utils.ts:585-594, 794-806` | Verified, high |
| Gemini CLI (whole-CLI, macOS) | `mkdtemp` `gemini-sandbox-*`, 0700, `-D TMP_DIR`, `TMPDIR`/`TMP`/`TEMP` set; writes allowed only there and in the project | `gemini:packages/cli/src/utils/sandbox.ts:270-295, 375-397`; `sandbox-macos-permissive-open.sb:71-89` | Verified, high |
| Gemini CLI (tool sandbox) | macOS: whole `os.tmpdir()` writable; Linux: `--tmpfs /tmp` | `gemini:packages/core/src/sandbox/macos/seatbeltArgsBuilder.ts:96-97`; `.../linux/bwrapArgsBuilder.ts:63-64` | Verified, high |
| Codex CLI | `/tmp` and `$TMPDIR` writable in workspace-write unless excluded; macOS adds `/private/tmp`, `/private/var/tmp` | `codex:codex-rs/protocol/src/permissions.rs:849-880`; `seatbelt_scratch.rs:36-47` | Verified, high |
| Cursor | Workspace mode: private per-project `$TMPDIR`; `privateTmp` (Linux); `disableTmpWrite` | [run modes](https://cursor.com/docs/agent/security/run-modes) | Verified (docs), medium-high |
| nono | System `/tmp` and `$TMPDIR` granted | `nono:crates/nono-cli/data/policy.json:177-190, 296-306` | Verified, medium |
| Containers / VMs | Own `/tmp` | Docker Sandboxes docs (own kernel) | Inferred, high |

## What fits plotkeeper

Keeping Claude Code's built-in sandbox for the agent and `srt` for the checks on macOS:

- **Temp (agent side):** a short per-run `CLAUDE_CODE_TMPDIR` (under 44 bytes with `/claude-501` appended) in the environment plotkeeper gives `claude`. Supported and documented; measure once that the sandboxed `$TMPDIR` moves. It would replace the deny-each-entry list.
- **Temp (check side):** plotkeeper runs `srt` itself, so set `CLAUDE_CODE_TMPDIR=<per-run dir>` in srt's environment and add the dir to `allowWrite`. Gemini CLI's model.
- **Unavailable:** a canary before each run, from the same process context that will start `claude` and `srt`: `sandbox-exec -p '(version 1)(allow default)' /usr/bin/true` must exit 0 (nested Seatbelt fails here with `sandbox_apply`), and a deny probe under the real profile must fail. Stop the run with a clear report if either is wrong. Then, after the run, scan tool results for `sandbox_apply:` and `Sandbox is required`, and fail the run on any. Fully compatible; no vendor change needed. The exact canary command is my suggestion, not taken from a tool.
- **Loopback:** for the `srt` check run there are two compatible paths: `allowLocalBinding: true` (all host loopback), or replace `srt` for the check run with a Seatbelt profile of plotkeeper's own (or nono) that allows bind anywhere and connect to `localhost:49152-65535` only. The second is the only host-sandbox way to keep tests working while keeping out the usual host services, and it still lets tests reach host listeners on ephemeral ports. For the agent side, Claude Code offers only `allowLocalBinding` or one-port proxy entries.

Needs a container or VM:

- A private loopback and private `/tmp` together: run the checks in a Linux VM or container (Apple `container` on macOS 26, Docker Sandboxes, a devcontainer). That is the only way to get "own servers yes, host services no" without port lists, and it is a much larger change.

## Standards and the security level plotkeeper needs

Added at Daniel's request. Parts 1 and 2 are cited facts; part 3 is my inference.

### 1. What Anthropic states for Claude Code's sandbox

- **Threat model.** The engineering post ["Beyond permission prompts"](https://www.anthropic.com/engineering/claude-code-sandboxing) (20 October 2025) names prompt injection as the risk: access to code and files "can introduce risks, especially in the case of prompt injection". Filesystem isolation stops a compromised agent changing system files; network isolation stops it leaking data or fetching malware; "effective sandboxing requires _both_ filesystem and network isolation." The post lists no limits. **Verified**, high.
- **What it does not claim.** The sandboxing page: "Sandboxing reduces risk but is not a complete isolation boundary"; it "covers shell commands only. Claude's file tools, MCP servers, and hooks" run outside it; and "Don't rely on any of the sandbox's restrictions, including protected paths and `credentials.files` entries, to stop a malicious command." Its Limitations section adds domain fronting past the domain allowlist, Unix sockets (`docker.sock`) as an escape, and over-broad write paths. ([sandboxing](https://code.claude.com/docs/en/sandboxing).) **Verified**, high.
- **Defaults.** Writes: "the working directory, a per-user temp directory, and any directories you've added". Reads: "the entire computer, except certain denied directories. This default still allows reading credential files". Network: allowed domains "start empty"; "By default, a sandboxed command can't connect directly to a server that's running on your machine", so `allowLocalBinding` is off by default (srt README also: "default: false"). `failIfUnavailable`: off by default; "By default, when the platform doesn't support the sandbox or the dependency check at startup fails, Claude Code runs commands without sandboxing." ([sandboxing](https://code.claude.com/docs/en/sandboxing).) **Verified**, high.
- **Unattended and headless.** [sandbox-environments](https://code.claude.com/docs/en/sandbox-environments): "The sandboxed Bash tool on its own constrains only shell commands, so it is not sufficient for fully unattended runs in either mode." For `--dangerously-skip-permissions`: "Always run ... inside a container, a VM, or the sandbox runtime". For auto mode the boundary is "defense in depth ... not required". Recommended for unattended work: the reference dev container, any container or VM, or running the whole `claude` process under `srt`. In `dontAsk` mode an unsandboxed retry is denied. **Verified**, high. plotkeeper uses `-p --permission-mode dontAsk`, not skip-permissions; which of the doc's rows that falls under is not stated (**unverified**).
- **Reference devcontainer firewall.** `init-firewall.sh` (`anthropics/claude-code` at `d945a61`): accepts all traffic on the container's own `lo` (lines 36-38), the host's /24 (lines 99-104), DNS, SSH, and an allow-listed domain set; default DROP; then verifies it cannot reach example.com and can reach api.github.com, else `exit 1` (lines 121-138). So inside the container loopback is open, but it is the container's own. Note the host /24 rule, which is wider than "no host services". **Verified**, high.

### 2. Industry standards and other tools' defaults

- **OWASP Top 10 for LLM Applications 2025, LLM06 Excessive Agency** ([genai.owasp.org](https://genai.owasp.org/llmrisk/llm062025-excessive-agency/)): "Limit the permissions that LLM extensions are granted to other systems to the minimum necessary"; "Avoid the use of open-ended extensions where possible" (its example is a shell tool); "Implement authorization in downstream systems rather than relying on an LLM"; "Log and monitor the activity". It does not mention sandboxing. **Verified**, high.
- **OWASP Top 10 for Agentic Applications 2026** ([announcement](https://genai.owasp.org/2025/12/09/owasp-top-10-for-agentic-applications-the-benchmark-for-agentic-security-in-the-age-of-autonomous-ai/)): ASI05 "Unexpected Code Execution" is on the list. Its mitigations (treat generated code as untrusted, hardened sandboxes, review before execution) come only from secondary write-ups; the PDF could not be read here. **Names verified, mitigations unverified**, medium.
- **NIST.** Not read this session. **Unverified**: nothing here says whether the AI RMF or an SP speaks to agent sandboxing.
- **Codex CLI defaults.** `workspace-write` is "the default low-friction mode for local work" ([sandboxing](https://learn.chatgpt.com/docs/sandboxing)); in config, `network_access` defaults to `false` (`codex:codex-rs/config/src/types.rs:1326-1334`), and with network off the Seatbelt policy grants no loopback (problem 1 table). `/tmp` and `$TMPDIR` writable by default. **Verified**, high.
- **Gemini CLI defaults.** Sandbox is opt-in (`-s`, `GEMINI_SANDBOX`, settings); default macOS profile `permissive-open`: writes confined to the project, network allowed ([sandbox docs](https://geminicli.com/docs/cli/sandbox/)). **Verified** (docs), high.
- **Cursor defaults.** Network mode "sandbox.json + Defaults" is the default (allowlist plus package registries); loopback not documented; Linux falls back to approval prompts when the sandbox cannot start ([run modes](https://cursor.com/docs/agent/security/run-modes)). **Verified** (docs), medium-high.

### 3. Recommendation (inference, not a cited fact)

**(a) General bar for running model-written code unattended on a developer's own machine.** Drawn from the above: an OS-enforced boundary for every command the code runs (not prompts); network egress default-deny; writes limited to the work folder and a temp folder; credential folders not readable; the run fails closed if the boundary is not in force; and everything logged. The vendors' own line for fully unattended runs is a container, a VM or `srt` around the whole process, because the per-command sandbox leaves hooks and MCP servers outside.

**(b) A reasonable v1 for plotkeeper,** given one user, local macOS, runs only on Daniel's click, prompts with trusted text only (never issue or PR bodies), credential folders read-denied, no internet for tools:

| Question | Standard or default practice | v1? |
| :- | :- | :- |
| Sandbox unavailable | Vendors check at startup only (Claude Code, Cursor); stronger tools prove the fence with a probe (srt Windows, devcontainer `init-firewall.sh`) or apply it in-process (nono). Silent pass is the failure every standard warns about: an "isolated" run that was not | **Must-have.** A canary before the run plus failing the run on any `sandbox_apply` / `Sandbox is required` in its tool results. Cheap, no vendor change, and without it the other settings are not known to apply |
| Shared temp folder | Most tools share system temp (Codex, nono, Gemini tool sandbox); the careful ones give a per-run dir (Gemini CLI) or private `/tmp` (Linux tools) | **Must-have for v1, but cheap:** a short per-run `CLAUDE_CODE_TMPDIR`, after one measurement. If the measurement fails, the current deny-each-entry list is acceptable for v1, and a fuller fix is later hardening. The reason it matters: other sessions' scratchpads hold Daniel's own work, and a run that can write there can plant files another session reads |
| Loopback | Default in Claude Code, srt and Codex is closed; `allowLocalBinding` is documented as opening every host port; a private loopback needs Linux namespaces or a VM | **v1: keep closed for the agent; for the check run, either accept `allowLocalBinding` with the risk written down or allow only the ephemeral range.** With no internet and trusted prompts, the realistic threat is a careless or confused agent, not exfiltration through a host service. A private loopback (VM or container) is later hardening |

## Open questions

- **Does a short `CLAUDE_CODE_TMPDIR` move the sandboxed `$TMPDIR` in 2.1.323?** The docs say yes below a length limit; the 44-byte figure is read from minified 2.1.325 code. Needs one run that prints `$TMPDIR` from a sandboxed Bash call. Also unknown: whether an `env` block in `--settings` counts, or only the process environment.
- **How does Claude Code classify `sandbox_apply: Operation not permitted`?** Whether it counts as "failed to initialize" (commands refused with a message) or just as a failing command was not settled. Either way the session keeps running.
- **Is a port in a Seatbelt *bind* filter enforced?** srt and Codex write `(local ip "localhost:PORT")` bind rules for their proxy; nono says bind cannot be filtered by port. Not tested here.
- **Which host listeners sit on ephemeral ports?** The ephemeral-range idea only holds if Daniel's usual services are below 49152. srt's and Claude Code's own proxies bind ephemeral ports (unverified), so a test could reach those.
- **Rule-count limit.** nono's ~17,770-rule crash figure is its own claim; 16,384 ephemeral ports fit under it, but the srt profile's other rules count too.
- **Not read:** OWASP ASI05's own mitigation text (PDF unreadable here), NIST guidance, Devin, Daytona, E2B docs.

## Appendix: source notes per tool (written as read)

### srt (sandbox-runtime) and Claude Code

- **Loopback on macOS is one switch.** With `allowLocalBinding` the profile adds `(allow network-bind (local ip "*:*"))`, `(allow network-inbound (local ip "*:*"))` and `(allow network-outbound (remote ip "localhost:*"))` (`srt:src/sandbox/macos-sandbox-utils.ts:1280-1284`). Without it, the only loopback rules are for srt's own proxy ports, one rule per port, for example `(allow network-outbound (remote ip "localhost:${httpProxyPort}"))` (`:1318-1342`). **Verified**, high.
- **Seatbelt's IP filter takes only `localhost` or `*` as host, and a port number or `*`.** srt's comment says so: "Seatbelt only accepts "localhost" or "*" as the host token" (`:1266`). It also says `(remote ip "localhost:*")` matches 127.0.0.1 and ::1 but not `::ffff:127.0.0.1`, and that a `(local ip ...)` filter on outbound admits every connection, because an unbound socket's source is the any-address, which `localhost` matches (`:1271-1279`). **Verified** that srt states it; the SBPL grammar itself is undocumented by Apple, so **unverified** at the source, medium-high.
- **Per-port loopback is already how srt fences its own proxy.** That proves a static per-port allow works in a profile. There is no rule for "only ports this process bound": the profile is fixed when `sandbox-exec -p` starts (`:1590-1600`). **Verified**, high.
- **The README states the scope plainly:** `allowLocalBinding` on macOS is "bind and accept on any local port, and connect to `localhost` on any port"; on Linux it is "ignored — the sandbox has its own network namespace, whose loopback is always usable and is not the host's" (srt README, `network.allowLocalBinding`). On Windows the same setting opens "Any service listening on the machine's loopback ... This is the same as on macOS with the setting on; Linux gives the sandbox its own loopback instead" (srt README, "Local servers on Windows"). **Verified**, high.
- **Loopback through the proxy by IP literal.** The README says allow-listing an IP literal such as `127.0.0.1:3000` in `allowedDomains` is "an explicit choice", and that hostnames resolving to loopback are otherwise refused (README, "Resolved-address check"). That gives proxied HTTP access to one fixed host port. It does not let a test bind a server. **Verified** (README), high.
- **Linux uses `bwrap --unshare-net`** (`srt:src/sandbox/linux-sandbox-utils.ts:3699`), so tests get a private loopback. **Verified**, high.
- **Unavailability check on macOS is empty.** `checkDependenciesCommon` adds errors only for Linux (ripgrep, bwrap, socat) and Windows (srt-win); on macOS it returns no errors at all (`srt:src/sandbox/sandbox-manager.ts:1883-1955`). Nothing tries `sandbox-exec` before the first command. Windows is the opposite: `initialize()` runs an egress probe under the sandbox token and "refuses to start if any of them gets out" (README, Windows WFP section). **Verified**, high.
- **Claude Code's `failIfUnavailable` acts on the same check at startup.** In the 2.1.325 binary, headless start calls `getSandboxUnavailableReason()`; if a reason exists and the sandbox is required, it writes `Error: sandbox required but unavailable: <reason> / Refusing to start without a working sandbox (see sandbox.failIfUnavailable).`, emits a `sandbox_required_unavailable` result in stream-json, and exits 1 (strings of `/opt/homebrew/Caskroom/claude-code/2.1.325/claude`). Since srt reports no reason on macOS when Seatbelt cannot be applied, the run starts, and each Bash call then fails with `sandbox_apply: Operation not permitted`. This matches what plotkeeper measured. **Verified** that the code path exists (minified); **unverified** that the reason source is srt's `checkDependencies`, medium-high.
- **TMPDIR.** srt sets `TMPDIR` for the sandboxed command to `process.env.CLAUDE_CODE_TMPDIR || process.env.CLAUDE_TMPDIR || '/tmp/claude'`, read from srt's own process environment (`srt:src/sandbox/sandbox-utils.ts:794-806`). Only `/tmp/claude` and `/private/tmp/claude` are writable without being listed (`SANDBOX_OWN_WRITE_PATHS`, `:585-594`), so a custom `CLAUDE_CODE_TMPDIR` must also be put in `allowWrite`. For plotkeeper's own `srt` run of the checks this is a supported, per-run knob: set `CLAUDE_CODE_TMPDIR=<run dir>` in srt's environment and list that dir in `allowWrite`. **Verified**, high.
- **Claude Code's sandboxed TMPDIR is `/tmp/claude-<uid>`, not srt's `/tmp/claude`.** So Claude Code computes its own value and does not just pass srt's default. The binary mentions `CLAUDE_CODE_TMPDIR` only in messages about the CLI's own files (disk quota, socket path, plugin clones, `claude --cloud`). plotkeeper measured that setting it moved only the CLI's files. Superseded below: the env-vars page documents that `CLAUDE_CODE_TMPDIR` does move the sandboxed `$TMPDIR` when the path is short. **Verified** (docs), high.

### OpenAI Codex CLI

- **Seatbelt network, same shape as srt.** With `allow_local_binding`: `(allow network-bind (local ip "*:*"))`, `(allow network-inbound (local ip "localhost:*"))`, `(allow network-outbound (remote ip "localhost:*"))`. Then one `(allow network-outbound (remote ip "localhost:{port}"))` per entry in `loopback_ports`, the managed proxy's ports (`codex:codex-rs/sandboxing/src/seatbelt.rs:288-298`, inputs at `:176-180`). With network off and no proxy, the network policy is the empty string: no loopback at all (`:319-321`). **Verified**, high.
- **Linux: own network namespace.** bwrap gets `--unshare-net` (`codex:codex-rs/linux-sandbox/src/bwrap.rs:287`, `:362`), so loopback inside is private. **Verified**, high.
- **Sandbox failure is detected after the fact, by guessing.** `is_likely_sandbox_denied` says "We don't have a fully deterministic way to tell if our command failed because of the sandbox" and matches exit codes and words such as "operation not permitted" and "sandbox" (`codex:codex-rs/sandboxing/src/denial.rs:1-60`). Codex's own tests treat `sandbox-exec: sandbox_apply: Operation not permitted` as "running nested, skip" (`codex:codex-rs/sandboxing/src/seatbelt_tests.rs:2885`). No production preflight for nested Seatbelt was found by grep. **Verified** (grep), medium.
- **Linux preflight.** Before the real command, Codex runs `bwrap ... /bin/true` to learn whether `/proc` can be mounted, and matches its stderr (`codex:codex-rs/linux-sandbox/src/linux_run_main.rs:487-507`, `:625-642`). It is a capability probe, used to choose options, not a fail-closed gate. Windows has a `run_legacy_setup_preflight` (`codex:codex-rs/core/src/windows_sandbox.rs:264-301`). **Verified**, high.
- **Temp: shared.** In workspace-write mode `/tmp` and `$TMPDIR` are writable unless `exclude_slash_tmp` / `exclude_tmpdir_env_var` are set (`codex:codex-rs/protocol/src/permissions.rs:849-880`). On macOS `/private/tmp` and `/private/var/tmp` are added as scratch roots (`codex:codex-rs/sandboxing/src/seatbelt_scratch.rs:36-47`). Nothing makes a per-run temp dir. **Verified**, high.

### Google Gemini CLI

- **Whole-CLI Seatbelt (`sandbox-macos-*.sb`).** The `strict-proxied` profile allows loopback to fixed ports only: `(allow network-inbound (local ip "localhost:9229"))` for the debugger and `(allow network-outbound (remote tcp "localhost:8877"))` for its proxy (`gemini:packages/cli/src/utils/sandbox-macos-strict-proxied.sb:127-133`). The `permissive-open` profile allows all bind and outbound (`sandbox-macos-permissive-open.sb:134-142`). **Verified**, high.
- **Per-run private TMPDIR on macOS.** Each launch does `fs.mkdtempSync(path.join(realpath(os.tmpdir()), 'gemini-sandbox-'))`, `chmod 0700`, passes it to the profile as `-D TMP_DIR=...`, and sets `TMPDIR`, `TMP` and `TEMP` to it (`gemini:packages/cli/src/utils/sandbox.ts:270-295`, `:375-397`). The profile allows writes only to `TARGET_DIR`, `TMP_DIR`, `CACHE_DIR` and a few home caches (`sandbox-macos-permissive-open.sb:71-89`). This is the clearest precedent for problem 3 on macOS. **Verified**, high.
- **Tool-level sandbox (`packages/core/src/sandbox`).** On macOS network is all or nothing (`NETWORK_SEATBELT_PROFILE`: `(allow network-outbound) (allow network-inbound) (allow network-bind)`, `gemini:packages/core/src/sandbox/macos/baseProfile.ts:217-223`), and `os.tmpdir()` is made writable whole (`macos/seatbeltArgsBuilder.ts:96-97`), so shared. On Linux bwrap gets `--unshare-all`, `--share-net` only when network is on, and `--tmpfs /tmp`, "an isolated, writable /tmp directory" (`linux/bwrapArgsBuilder.ts:45-64`). **Verified**, high.

### Claude Code docs (sandboxing page)

Read at [code.claude.com/docs/en/sandboxing](https://code.claude.com/docs/en/sandboxing) on 24 November 2026.

- **Loopback.** "macOS: set `network.allowLocalBinding` to `true`. Sandboxed commands can then listen on network ports and connect to any port on localhost, which includes every other service listening there." "Linux and WSL2: a sandboxed command's `localhost` is private to that command ... `allowLocalBinding` has no effect." For reaching one host service it suggests an IP literal in `allowedDomains`, such as `"127.0.0.1:3000"`, and warns that "An IP address entry with no port lets sandboxed commands reach every service listening on that address." **Verified**, high. No per-port bind/connect pairing is offered.
- **`failIfUnavailable` covers the startup check only.** "when the platform doesn't support the sandbox or the dependency check at startup fails, Claude Code refuses to start". If the sandbox fails after start, "each sandboxed command Claude runs is refused with the message shown. The session keeps running, and the refused command doesn't run unsandboxed" (messages `Sandbox is required but failed to initialize: <reason>` and `... stopped being available in this session: <reason>`). So a clean exit 0 with every command refused is the documented design, not a bug. The `sandbox_apply` case plotkeeper measured is a third shape: the wrapper starts and `sandbox-exec` itself fails, so Claude Code may not even classify it as a sandbox failure. **Verified** (docs), high; the classification of `sandbox_apply` is **unverified**, medium.
- **Temp.** "A per-user temp directory is writable inside the sandbox by default ... Claude Code sets `$TMPDIR` to this directory for sandboxed commands." `CLAUDE_CODE_TMPDIR` is named only as what an *unsandboxed* command gets when the shell's `TMPDIR` is empty. The sandboxing page names no setting that moves the sandboxed `$TMPDIR`; the env-vars page does (see the next section). **Verified** (docs), high.
- **Whole-process boundary.** For stronger isolation the page points to running Claude Code inside `srt`, or in the dev container ([sandbox-environments](https://code.claude.com/docs/en/sandbox-environments.md)). **Verified**, high.

### Cursor

- Read at [cursor.com/docs/agent/security/run-modes](https://cursor.com/docs/agent/security/run-modes) (Sandboxing). macOS uses Seatbelt via `sandbox-exec`; Linux uses Landlock and seccomp and needs kernel 6.2. **Verified**, high.
- **Unavailable:** on Linux, "If your kernel does not meet these requirements, Cursor falls back to asking for approval before running commands." That is a fallback to prompts, not to unsandboxed auto-run. Nothing is said for macOS. **Verified**, high.
- **Temp:** with Read access set to Workspace, sandboxed commands "get a private per-project temp folder in `$TMPDIR` that keeps files between commands"; `disableTmpWrite` turns temp writes off; on Linux `privateTmp` makes `/tmp` that same folder. Per project, not per run. **Verified** (docs; condensed by the fetch tool), medium-high.
- **Loopback:** not addressed on that page. **Unverified**.

### nono (`nolabs-ai/nono` at `d0c52cd`, prefix `nono:`)

- **Per-port loopback connect on macOS, with ranges.** In network-blocked mode, each allowed localhost port becomes `(allow network-outbound (remote tcp "localhost:PORT"))`; ranges are expanded one rule per port; port 0 means `localhost:*` (`nono:crates/nono/src/sandbox/macos.rs:463-494`, `:843-870`). CLI: `--open-port PORT` ("bidirectional localhost TCP on a port: connect + listen") and `--listen-port PORT` (`nono:crates/nono-cli/src/cli.rs:1229-1235`). **Verified**, high.
- **Bind cannot be scoped by port on macOS.** When any localhost port is allowed nono adds blanket `(allow network-bind)` and `(allow network-inbound)`, commented "Seatbelt cannot filter bind/inbound by port" (`macos.rs:866-869`, `:896-903`). General per-port TCP rules error out with "Seatbelt cannot filter by TCP port" (`:921-930`). **Verified** that nono says and does this, high. Note srt and Codex do write `(local ip "localhost:PORT")` bind rules for their proxy port, so a port in a *bind* filter at least parses; whether it is enforced was not checked.
- **Rule-count ceiling.** "`sandbox_init()` crashes with SIGILL above ~17,770 rules"; nono caps ranges at `1 << 14` = 16,384 ports (`nono:crates/nono/src/capability.rs:13-16`, check at `macos.rs:504-515`). **Verified** (nono's claim), medium-high. 16,384 is exactly the size of macOS's default ephemeral port range 49152-65535 (see "What fits plotkeeper").
- **Fails closed by applying the sandbox in-process.** nono calls `sandbox_init()` itself and returns `NonoError::SandboxInit` with the kernel's message when it is non-zero, before running the child (`macos.rs:940-978`). Its `is_supported()` on macOS is a hard-coded `true` (`:162-175`), so the real check is the apply. A nested Seatbelt would fail there and nono would exit with an error instead of starting the command. **Verified** (code), high; the nested behaviour is **unverified** (not run), medium-high.
- **Temp:** the built-in policy grants the system `/tmp` (`/private/tmp`) and `$TMPDIR` (`nono:crates/nono-cli/data/policy.json:177-190`, `:296-306`). Shared, no per-run dir. **Verified**, medium.

### Claude Code: `CLAUDE_CODE_TMPDIR` does move the sandboxed TMPDIR, unless the path is long

This corrects the earlier srt/Claude Code note and probably explains plotkeeper's measurement.

- **Docs.** [env-vars](https://code.claude.com/docs/en/env-vars), `CLAUDE_CODE_TMPDIR`: "Override the temp directory used for internal temp files. Claude Code appends `/claude-{uid}/` to this path on Unix ... Default: `/tmp` on macOS ... On macOS and Linux, sandboxed Bash subprocesses receive a short fallback `$TMPDIR` under the system default when your override is a long path, since some tools fail when temp paths get too long. ... Set it in your shell, user settings, or managed settings. Ignored in project and local settings." **Verified**, high.
- **Code (2.1.325 binary, minified).** `function pf(){let e=i.CLAUDE_CODE_TMPDIR;if(e)return e;return"/tmp"}var yKr=44`, and the child-process temp dir function `Ije()` returns the per-user dir when `Buffer.byteLength(r)<=yKr`, else `iGo()`, which makes `join("/tmp", <claude-uid name>)` with mode 0700. So the limit is **44 bytes** for the full `<override>/claude-<uid>` path. **Verified** (read in strings), medium-high: the names are minified, so the reading of which function feeds the sandbox is inference.
- **Consequence for plotkeeper.** An override such as `/Users/danielrispler/.../runs/<id>/tmp` is far over 44 bytes, so sandboxed tools fell back to `/tmp/claude-501`, the shared folder. An override like `/tmp/pk-<8 hex>` gives `/tmp/pk-1a2b3c4d/claude-501` (27 bytes), under the limit, and should give each run a private sandboxed `$TMPDIR`. It must be set in `claude`'s process environment (plotkeeper spawns it, so that works), not in project settings. Whether a `--settings` `env` block counts is **unverified**. Not run here: needs a measurement. **Unverified** end to end, medium-high.

### Docker Sandboxes

- [docs.docker.com/ai/sandboxes/security](https://docs.docker.com/ai/sandboxes/security): "The primary trust boundary is the microVM." "Hypervisor isolation: separate kernel per sandbox." The sandbox "cannot access your host Docker daemon, your host network directly"; "Outbound TCP connections to destinations allowed by network policy are proxied through the host." [Architecture](https://docs.docker.com/ai/sandboxes/architecture/): "All outbound TCP traffic from the sandbox routes through a proxy on your host." **Verified** (docs, via a page-to-text fetch), high. A separate kernel means its own loopback and its own `/tmp`; the pages do not say so in words (**unverified**, high by construction).

