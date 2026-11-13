# Triage Labels

The skills speak in terms of five canonical triage roles. This file maps those roles to the actual label strings used in this repo's issue tracker.

| Label in mattpocock/skills | Label in our tracker | Meaning                                  |
| -------------------------- | -------------------- | ---------------------------------------- |
| `needs-triage`             | `needs-triage`       | Maintainer needs to evaluate this issue  |
| `needs-info`               | `needs-info`         | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`    | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`    | Requires human implementation            |
| `wontfix`                  | `wontfix`            | Will not be actioned                     |

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), use the corresponding label string from this table.

## Before publishing

Once per session before publishing a labelled issue, list GitHub's labels with `gh api --paginate repos/danielrispler/plotkeeper/labels --jq '.[].name'` and compare them with all five names in the tracker column above. Create only missing configured labels with `gh label create <name> --repo danielrispler/plotkeeper`, preserving unrelated labels. If verification or creation fails, resolve it before publishing. Keep this live tracker preflight out of offline CI.
