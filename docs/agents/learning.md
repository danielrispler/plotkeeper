# Learning through plotkeeper

plotkeeper, like Falinks before it, is an educational project first. This workflow governs Daniel and the agent working together. Its goal is to build the understanding needed to explain the system, predict behavior, challenge decisions, and propose the next idea. Daniel has TypeScript/full-stack experience and is learning Rust.

The main risk is cognitive debt: the project advancing faster than Daniel's mental model. Agent implementation and verification can proceed autonomously within the agreed scope; explanations remain available on request and never block progress.

## Session opening

Begin each session with a brief orientation: the current state, relevant settled decisions, unresolved questions or blockers, today's goal, and its stop condition. Use the current ticket/map and required reading when working through Wayfinder; use the current task and available context for other sessions. Give enough context to resume without reconstructing the previous conversation, and distinguish recorded facts from assumptions. Briefly revisit a relevant learning gap or concept when it helps today's work.

## Planning and questions

Apply this teaching approach during grilling, Wayfinder, and ordinary planning, including before implementation exists. Establish background and intuition using the project's vocabulary before alternatives. For each decision question put to Daniel (which ones, below), explain the dilemma, why it matters, the viable options and their consequences, a concrete scenario or failure that makes the difference visible, and the recommendation with its reasoning. Make clear what the answer will decide or unblock.

**Copy the standard shell; design only the learning layer.** Sidebar hierarchy, project trees, worktree navigation, branch switching, history and other standard workspace mechanics are solved problems: copy the proven pattern instead of inventing one. Before proposing options for such a decision, check how the benchmark tools solve it (see **Prior art** in `docs/studio/CONTEXT.md`): herdr (spaces, nested worktrees, urgent-status rollup), cmux and Workmux (worktree and branch switching), AgentsRoom and T3 Code (desktop multi-session, multi-project hierarchy and archive), and Cursor's parallel background agents (worktree-per-task lifecycle and status). Zed is second, for editor chrome such as branch pickers and config-versus-state separation. Read the existing notes in `docs/studio/research/` first, and save new findings there. Options name the tool they copy; where the benchmark tools agree, recommend their pattern. Custom design is for what makes the studio unique, the learning layer: the story rail, intentions and turn summaries, and the PR briefs and the fog on the map (cards and quizzes are retired, danielrispler/plotkeeper#347 and #348; [PR briefs](../studio/behaviour/pr-briefs.md)).

**Keep a grilling on disk.** At a grilling's start, open `local-evidence/<date>-<topic>/` and add its row to the index (`AGENTS.md`, Local evidence). After each round, append Daniel's decisions to `reports/decisions.md` there before the next round begins. Brief each subagent to write its report to a file under `reports/`, saving as it goes, so a session that ends mid-run keeps the work so far. A compacted or restarted session reads these files in place of the chat, and the spec and the resolution cite them.

**Ask Daniel only what is his to decide.** By default a grilling puts to Daniel only product, UX/UI, scope, workflow, privacy/quota and roadmap questions. Settle the implementation questions yourself and record each under "Made without Daniel" in the grilling's `reports/decisions.md`: the choice, the options set aside, and why. Number them Q<n> in one series with the questions he answers. A technical question that hides a workflow or roadmap trade-off is his: ask it. When his request says "full technical grill", ask every question, the implementation ones too. When he says "explain Q<n>", open that decision in the decision-question shape above, whoever made it.

This default trades against the main risk named above, cognitive debt: grillings teach Daniel less of the technical side, since he no longer reasons through the implementation questions. Three things make that up: the veto summary shows him the consequential decisions made without him, "explain Q<n>" opens any of them, and `$dsh-explain-diff` teaches the merged result.

**Critic and veto gate.** A spec moves in this order: grill, draft spec, critic, fixes, veto summary, publish, `$to-tickets`. A spec that changes code behaviour (Rust, TypeScript, or agent and tooling scripts such as `.mjs`) gets the critic; a docs-only or research spec goes from its draft to the veto summary.

- **Critic.** One round, by a fresh-context reviewer. Until `delegate.mjs pick critic` ships, the critic is a fresh Sonnet subagent at effort `high`, never a fork, since a fork ignores a per-call model and effort. Its brief: check every claim of the draft against the repository; mark each verified, with its `file:line` or a command and its output, or unverified; flag contradictions with `AGENTS.md`, `system-map.json` and existing code; rate each finding high, medium or low. It writes its findings to `reports/critic.md` in the grilling's `local-evidence/` folder.
- **Fixes.** Answer every finding in that file, fixed or rejected with evidence, and fix the draft. The critic does not run again on the fixes.
- **Veto summary.** Give Daniel at most eight lines: the consequential decisions made without him (hard to undo, visible to him, or spending quota) and every rejected finding. Publish the spec only once he passes it. The spec lists the other decisions made without him under "Made without Daniel".

Support deeper discovery through predictions, experiments, and hints. Give Daniel an opportunity to reason about a scenario before revealing its outcome when useful; provide a direct explanation when requested. Build shared understanding before asking Daniel to choose. Batch independent questions with sufficient context for each; questions dependent on unanswered choices wait for a later round. Find environmental facts yourself rather than turning them into questions for Daniel.

## Ticket workflow

Design together → Agent implements → Tests → Resolve ticket → Next ticket. Suggest an optional explanation in the final message.

Apply this to every meaningful ticket, rather than every small commit. A meaningful ticket introduces or changes a mechanism, consequential design decision, failure behavior, or substantial body of knowledge. Suggest the explanation for a ticket that merges a diff. A research, design, grilling or prototype ticket gets one when Daniel asks. Combine routine edits into the relevant ticket's explanation.

1. **Design together.** Use the planning approach above to agree on the problem, the product design, scope, and relevant learning goals; settle the implementation design as "Ask Daniel only what is his to decide" says.
2. **Implement and verify.** Work independently within the agreed design. Run checks appropriate to the change, and distinguish verified behavior from assumptions and remaining uncertainty. For research/design tickets, check the evidence and consistency of the decision instead of inventing implementation or test results.
3. **Suggest an explanation.** After a meaningful ticket that merges a diff, suggest `$dsh-explain-diff` in the final message. Continue authorized work and resolve completed tickets without waiting for an invocation or readiness confirmation. Load and run the global explanation skill only when Daniel explicitly invokes it. When invoked, follow steps 4–6 and the coverage guidance below; explain the full ticket outcome, including its surrounding system and relevant changes across commits. Save one self-contained HTML page at `~/code-explanations/plotkeeper/YYYY-MM-DD-<ticket-or-topic>.html` and return its absolute path as a clickable link.
4. **Read the core.** Identify the 3–5 most important code passages, ordered by how the mechanism works. Give verified file/line references, explain why each matters, and state what to notice. Use fewer when the work has fewer relevant passages. For tickets without code changes, identify the relevant decision/evidence passages and explain that distinction.
5. **Use the HTML knowledge check.** Put open-ended questions about explanation, prediction, failure cases, trade-offs, and a possible modification in the explanation HTML, with reasoning available to reveal. Daniel works through these there; do not repeat them or require a second quiz in chat. Discuss answers or offer a new scenario only when Daniel requests it or raises an understanding gap.
6. **Explore a micro-world when useful.** Use a small simulator or explorable trace for concurrency, locks, revisions, recovery, event ordering, and other behavior that remains hard to visualize. Let Daniel change inputs/order, inspect state, and predict outcomes. Name the model's simplifications and check its examples against the actual mechanism or documented design. Introduce it earlier when it would help the design or explanation.

## What the explanation covers

- The problem and prior behavior, with enough background to understand the change.
- The architecture, component relationships, and how the outcome connects to the rest of the system.
- Key decisions, alternatives, and consequences, including why the chosen approach fits.
- Invariants: what must remain true, where that is enforced, and what happens if it is violated.
- Failure cases and recovery, with concrete inputs and event sequences.
- Important Rust concepts used by the change, taught in context with TypeScript comparisons when helpful.
- The ordered walkthrough, core reading passages, validation evidence, and unresolved limitations.

Ground claims in inspected code, tests, current decisions, and primary sources where needed. Label a toy model, proposed behavior, or inference explicitly so a teaching diagram cannot silently become a specification.

## Continuity and ticket resolution

When an explanation is requested, record its path, core reading pointers, concepts explored, and any understanding gaps Daniel raises with the relevant ticket's session outcome. Local HTML paths are for Daniel's machine; the ticket's durable decision and evidence must remain understandable without access to those files. At the next session, briefly recap the model and any gap Daniel raised; use the existing HTML check rather than repeating its questions in chat.

Resolve a meaningful ticket when its agreed work and verification are complete, then follow `docs/agents/ticket-authoring.md` for closure, reading-link updates, and the map pointer. Keep canonical decisions in their ticket resolutions, domain vocabulary in `docs/studio/CONTEXT.md`, and architecture records in `docs/adr/` when warranted.

This guide is the source of the learning defaults. The global explanation skill supplies the reusable artifact format.
