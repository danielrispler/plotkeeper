#!/bin/bash
# The status line of sessions the studio starts (danielrispler/Falinks#521; danielrispler/Falinks#517, decision 5), set by `--settings`. It saves
# stdin as the session's usage snapshot first, then hands the same stdin to the user's own status line
# command, read anew on every run, so its output and exit status pass through and edits apply at once.
# shortcut: reads only the user settings file, not project or local ones; read those too if a
# project ever sets its own status line.
input=$(cat)
source "$(dirname "$0")/statusline-usage.sh"
command=$(jq -r '.statusLine.command // empty' "$HOME/.claude/settings.json" 2>/dev/null)
[[ -n "$command" ]] || exit 0
exec bash -c "$command" <<<"$input"
