# CodeBoarding: what it does and what plotkeeper can take from it

CodeBoarding (codeboarding.org) draws an architecture map of a repository and reviews pull requests on that map. This note looks at how much it overlaps with plotkeeper and lists ideas worth turning into tickets. Studio terms follow `docs/studio/CONTEXT.md`.

## How this was read

- **The live demo, read on 2026-11-24.** I used Claude in Chrome on `https://app.codeboarding.org/CodeBoarding/eShop/pull/8` and `https://app.codeboarding.org/CodeBoarding/eShop`. I opened these views:
  - Overview, Flows, a flow's detail, the method diff, Risks with "How it's decided", Conversation, Checks, the commit picker, and the Changes-only and Full map views;
  - the repo's Architecture and Health tabs.
  
  These are marked **(demo)**. Nothing was signed in to, posted or changed.
- **Primary sources, read on 2026-11-24 at pinned commits:**
  - the engine, `CodeBoarding/CodeBoarding@b5341f2e`;
  - the Action, `CodeBoarding/CodeBoarding-action@1765d81e`;
  - the MCP server, `CodeBoarding/CodeBoarding-MCP@4757d652`;
  - codeboarding.org and the VS Code Marketplace listing.

  These are marked by source. A subagent read them through WebFetch and `gh`. I did not re-read each one myself, so treat line-level details as **unverified** until someone checks them.
- **Unverified throughout:** the page made no network calls I could see (`read_network_requests` caught only one `data:` request after a reload). I could not confirm how the page loads its data.

## Verdict

- **The overlap is real, and it sits in PR review.** Both tools tell a pull request as an ordered story drawn on a map of parts. CodeBoarding's version is more finished.
- **plotkeeper's core is not covered.** CodeBoarding works from the PR or analysis onward, for a team reviewing on GitHub. plotkeeper works from the agent session onward, for one person:
  - it covers each turn while the agent is still working;
  - its Evidence line comes from hooks and git, with no model involved;
  - it tracks what you personally understand (fog, staleness, explain-back);
  - its map is declared in the repo, and its Laws are enforced by tests.

  CodeBoarding has none of these.
- **Their maps are generated, ours is declared.** CodeBoarding builds its map automatically: LSP call graphs, then an LLM groups and names the components. plotkeeper's `system-map.json` is written by hand, and CI checks it.
  - Their map needs no setup.
  - Ours is the "laws" contract. Because it is declared, a part's name and boundaries never drift between two runs. Their README itself has to warn about "re-grouping" between runs (see "Engine and pipeline" below).

## What the demo does (demo)

### PR page layout

- **Header:**
  - a "MEDIUM MERGE RISK" badge;
  - the branch, with a copy button, pointing at `main`;
  - `+707 −12`.
- **Tabs:** Overview · Flows 8 · Risks · Files "0 of 14 viewed" · Conversation "2 open" · Commits 2 · Checks "8 passed".
- **Two panes:** the tab's content on the left, and the map on the right, always visible.
- **Map controls:**
  - a **Changes only 4 / Full map 15** toggle; Changes only collapses the untouched siblings into a "+2 unchanged" chip;
  - an **Edges** menu with "Changes" and "Focused";
  - Labels on/off (key `L`), zoom, and fullscreen;
  - "Take a tour", and "Press H for help".
- **Legend**, bottom right:
  - "comparing 92d85b6 → 0efa917";
  - line styles for added, modified, removed and unchanged;
  - "Warnings are solid lines".

### Overview

- **Stale-base warning:** "7 commits behind main. The diff and the diagram compare this PR against the commit it branched from… Update the branch to review against a current baseline."
- **Risks summary** with "Copy all".
- **An honesty line:** "Some architecture checks did not run on this commit, so this is not the full picture."
- **Summary:** an AI rewrite of the PR body, with "Read the whole description".
- **"Start here":** points at Flow #1 and says why.

### Flows: the PR as an ordered set of changes

Flows come in three groups, each with a one-line explanation:

- **Across components:** "Calls that go from one component of the system to another".
- **Changes within one component.**
- **Supporting changes:** "Config, tests and docs not tied to a change above".

Each flow has:

- a kind tag (NEW, UPDATE, CONFIG, REFACTOR, TESTS, DOCS);
- a one-line outcome, for example "Callers can price one basket line from the catalog";
- a progress meter, such as `0/3`;
- a cross-reference, for example "Mentions CatalogClient, which #2 changes" or "No component on the map holds its files";
- a finding count.

Opening a flow shows:

- "0 of 3 reviewed", a **Reviewed** checkbox, and a "1 of 8" pager;
- **Risks** ("No findings on this change");
- **How it works:** one plain paragraph that walks the call path;
- **Steps:** numbered calls that match numbered badges on the map edges. "Point at a step, a number or its line to light it there";
- **What changed where:** grouped by component, each changed method listed with:
  - its signature;
  - a one-line purpose, for example "Returns a basket line estimate";
  - its line number;
  - its own reviewed checkbox.

On the map, everything outside the flow dims, and the flow's path is highlighted.

### Method-scoped diff

- Clicking a method opens the file diff under the map. The URL becomes `?f=<file>&l=244`, so the view can be linked.
- The diff is folded by **symbol**: `CatalogApi class · modified +38 11–507` › `GetBasketPreview(...) method · added +33 244–276`. Untouched lines between them sit behind "191 hidden lines · Expand down / Expand up".
- Controls: Unified/Split, "Viewed", open on GitHub.

### Risks

- **Findings are architectural**, each pinned to a file and line:
  - "Basket.API now depends on Catalog.API directly — Extensions.cs:19";
  - "Catalog.API now depends on Basket.API at compile time — Catalog.API.csproj:41".

  Each finding sits under the flow that caused it.
- **"How it's decided"** opens the rule in a few words: "The level is the highest finding. High: a customer action can now fail, or a sensitive change is failing checks or very large. Medium: a sensitive area, or a decision that slows the team. Notes never change it."
- **On the map,** a new dependency edge carries a warning marker, and the legend says warnings are solid lines.

### Conversation

- Threads can be anchored to **code lines**, with an inline snippet and "Show in code", or to a **component**, with "Show on the map".
- **Each thread shows who it is waiting on,** for example "Waiting on ivanmilevtues".
- **Filters:** "Waiting on you (0)", Open/All, search, and sort by recent activity.
- **Clicking a component on the map filters the threads to it** ("Only Customer Basket Domain Models ×").

### Commits picker

- "Show changes from": **All changes**, **Since your review** ("There is no review to count from yet"), or pick commits with shift-click for a range.
- One commit carries the tag **"map drawn from this"**.

### Checks

- GitHub checks are listed with their durations: their own `CodeBoarding / review` (3m 53s), `CodeBoarding / sync` (skipped), and the repo's tests.

### Repo view

- **Tabs:** Architecture "10 components", Pull requests "4 open", **Health "164 warnings"**, History "50 commits".
- **Actions:** Clone · Export · Share.
- **About panel:** a one-paragraph description of the system, plus the "Largest components" by files and methods.
- **Map:** each component has a `+` to expand its sub-diagram and an `i` for info.
- **Health:** warnings grouped by component, such as unused imports and long functions with their line counts. Each warning links to its line.
  - Most of the 164 are noise: unused `using` lines in generated EF migrations.

## How it is built (primary sources)

### Where it runs

The engine README ("Where to use it") lists five surfaces:

- **CLI:** Python 3.12, `pipx install codeboarding`, then `codeboarding full|incremental|partial --local <repo>`.
- **Web app**, with Explore and Review views. Public maps need a committed `.codeboarding/analysis.json`.
- **VS Code extension,** also on Open VSX for Cursor and Windsurf.
- **GitHub Action,** with a `review` mode and a `sync` mode.
- **A small MCP server** (`CodeBoarding-MCP` README).
  - **Unverified:** it may be legacy. It points at the old "GeneratedOnBoardings" repo.

### Engine and pipeline

- **Stages** (engine `AGENTS.md` §4): ProjectScanner → StaticAnalyzer → DiagramGenerator → output generators.
- **Static analysis** uses real language servers, downloaded to `~/.codeboarding/servers/`. It also uses tree-sitter grammars, libclang and networkx.
  - It builds a call graph (`static_analyzer/cfg/call_graph.py`) and clusters it (`static_analyzer/clustering/`).
- **LangChain and LangGraph agents** turn the clusters into named, described components (`agents/planner_agent.py`, `tree_planner_agent.py`, `scope_analysis_agent.py`).
  - **Providers:** OpenAI, Anthropic, Google, Bedrock, Ollama, OpenRouter, LiteLLM, and any OpenAI-compatible endpoint (`agents/llm_config.py`).
  - The README says: "Analysis can send code excerpts to that provider".
- **Output:** `analysis.json`, holding components, relations, source references and nested sub-diagrams.
  - It can also be rendered as Markdown with Mermaid, HTML, MDX or RST (`--render`).
  - The VS Code extension writes one Markdown file per component under `.codeboarding/components/` (Marketplace).
- **Incremental runs** use a baseline in `.codeboarding/` (`analysis.json` + `fingerprint.json`).
  - `partial --component-id` re-runs a single component.
  - With no baseline, it fails fast instead of silently running a full analysis (engine `AGENTS.md` §5).
  - Since extension 0.10.0, only modified methods are re-analysed (Marketplace).

### PR review

- **How the diff is made:** the Action analyses both the PR head and the merge base, then diffs the two `analysis.json` files (Action README).
- **What it posts:** one sticky comment with an inline Mermaid graph. Added components are green, modified yellow, and deleted red with a dashed outline (`scripts/diff_to_mermaid.py`).
- **Re-grouping flag:** the comment flags a PR where the components changed but no analysed file did, since the change is only the LLM grouping the code differently.
- **Paid server-side extras:** the "review guide" and "architecture checks" run on CodeBoarding's service (`POST /v1/review-guide`, `/v1/architecture`). Size-capped source files are sent there (Action README).
  - The demo's Flows and Risks are most likely these extras. **Unverified.**

### Agent integration

- **The website** says: "Copy a component as context for your agent".
- **The Marketplace listing:**
  - "Monitor your agent's actions by seeing which components have file edits in them";
  - since 0.12.3, "MCP Activity in the Diagram" and "MCP plan impact data", which show which parts a plan is expected to touch.
- **Claude Code:** no specific integration is documented.

### Licence, pricing, languages

- **Licence:** the engine, Action and MCP repos are all MIT.
- **Pricing** (codeboarding.org):
  - Free: metered, maps 3 levels deep;
  - Pro: $9.99/month, unmetered, 10 levels, health monitoring;
  - Enterprise: custom.
- **Languages** (README "Supported stack"): Python, TS/JS, Java, Kotlin, Go, PHP, Rust, C#, and Dart. C++ and CUDA are in preview.
- **Telemetry** is on by default and can be opted out of.

## Side by side

| | CodeBoarding | plotkeeper |
|---|---|---|
| Map source | Generated: LSP call graph, then an LLM groups and names the components | Declared: `system-map.json`, with Laws tied to tests by `@law:` tags and checked in CI |
| When it runs | On a PR (Action) or an analysis run (CLI/extension) | Each agent turn, live, through hooks |
| Who it serves | A team reviewing on GitHub | One developer running agents locally |
| PR story | Flows (across / within / supporting), steps, Risks, method diff | PR brief: a stepped story drawn on the map |
| Your understanding | Per-flow and per-method "Reviewed" checks; "Since your review" | Fog, staleness bands, part catch-ups, explain-back |
| Trust signal | "Some checks did not run… not the full picture" | Evidence line from hooks and git with no model; "Agent says:" for unbacked claims |
| Where code goes | The model provider, plus CodeBoarding's service for the paid extras | `claude -p` on your own subscription |

## Ideas worth a ticket

These are ordered by how well each fits what plotkeeper already has. Each one still needs `$grilling` before it becomes a spec.

1. **Architecture risks from new Links.**
   - When a diff adds a dependency between two parts that has no Link in `system-map.json`, flag it as a risk. The flag carries the file and line, and goes in the PR brief and on the map edge. This is CodeBoarding's "X now depends on Y directly".
   - Our declared map makes this a deterministic check, not an LLM guess. It fits the Evidence-without-a-model rule.
2. **"How it's decided", one click away,** on any level or badge we compute, such as a merge-risk level or a fog state. Their rule fits in three sentences.
3. **Sort supporting changes apart from real changes.** Their three groups put config, tests, docs and mechanical moves behind the real changes. Each group carries a link such as "Mentions X, which #1 changes", or "No part on the map holds its files".
   - The second case is free for us: files no part owns are a map gap worth surfacing.
4. **"What changed where", at method level, inside a step.** Per part, list each changed symbol with its signature, a one-line purpose and its line. Clicking one opens a diff folded by class or method, with hidden runs between them.
5. **A "Changes only / Full map" toggle,** where Changes only collapses untouched siblings into a "+N unchanged" chip. This might help the map-usage issues in `map-usage-prior-art.md`.
6. **Reviewed checkboxes on steps and symbols,** plus a "Since your review" commit range. This would make fog clear when you tick a box, not only when you look.
7. **Warn when the base is stale:** "N commits behind main; this compares against the branch point".
8. **Threads anchored to a part** ("Show on the map"), with "waiting on" and a filter that follows the part you clicked on the map.
9. **A "not the full picture" line** whenever a check or a writer did not run. This matches our "Agent says:" honesty.
10. **Smaller polish:**
    - deep-linkable diff URLs (`?f=&l=`);
    - "Copy all" on the risks;
    - "Start here" plus why;
    - an `L` key to toggle labels.

**Not worth copying:** a per-component Health tab of lint warnings. In the demo, it is mostly noise from generated EF migrations.
