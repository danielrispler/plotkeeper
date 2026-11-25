---
name: review-trial
description: Run the outside-review trial (two variants on one diff) on a branch or PR, and report it. Runs only when Daniel types it.
disable-model-invocation: true
---

Daniel typed this, so he is starting the review trial (`docs/studio/development.md`, the review trial paragraph). Run it once. `$ARGUMENTS` names the branch or PR. With no argument, use the current worktree's branch.

1. Work in the worktree that has that branch checked out (`git worktree list`). Create none, and stop if none holds the branch. Its work must be committed: the trial reads HEAD.
2. Run `node scripts/review/outside-review-trial.mjs --out <scratchpad>` from that worktree as a background task, and wait for it to finish. It takes up to 15 minutes.
3. Read each `== <variant> ==` block. When every variant is `reviewed`, merge the two findings files into one list, with each finding once and tagged with the variants that found it (for example `medium, subagents`). Check each finding against the code. Fix the valid ones, commit and push them, and give the rejected ones a reason. Don't run the trial or the outside review again on the fixes. For a variant that is `skipped` or `review unavailable`, write its line down in place of its findings.
4. When the branch has an open PR, replace its "Outside review" section with the merged list, each finding marked fixed or rejected, with the two `cost:` lines and the trial's quota cost line. Otherwise give that section in your reply.
