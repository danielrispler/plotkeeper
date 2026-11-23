# Load the plugin

Sessions the studio starts load the plugin on their own. For a `claude` you start yourself, in a studio shell or any other terminal, add the plugin to `CLAUDE_CODE_PLUGIN_DIRS` in your shell profile, through the link in the studio's data folder:

    export CLAUDE_CODE_PLUGIN_DIRS="$HOME/Library/Application Support/plotkeeper/checkout/plugin"

At each start the app points `checkout` there at its own checkout, so a moved checkout is picked up by the next start (#21). It also asks a login shell for the variable: when no entry is this checkout's plugin, the bar over the terminal says so, and "Fix it" writes the line into `~/.zshrc`, after a copy to `~/.zshrc.studio-backup` that a later fix keeps. It drops entries that are another studio plugin and keeps the rest as written, missing ones too, since Claude Code skips those.

Only zsh's `~/.zshrc` is fixed, and a line that builds the value with `${`, `$(` or a backtick is left alone; then the bar names the line to add. A shell that gives no answer within 10 s, or `exec`s another program such as tmux, counts as loading the plugin. Claude Code skips a missing plugin folder with no error, so before #21 a stale path showed only as sessions with no row.

Each `claude` session then loads the plugin in place, as `falinks-studio@inline` in `claude plugin list`. Nothing is written to `~/.claude/settings.json`. `claude plugin install` would write `enabledPlugins` and `extraKnownMarketplaces` there.

The hooks run the `studio-hook` built in this checkout, so build before you start a session. Edits to the plugin take effect at the next session start or `/reload-plugins`.

Two hooks carry a blocked command (spec #170). On `PermissionDenied` for Bash, `studio-answer` leaves a request for the card and waits up to 600 s for Allow or Decline (timeout 610). On `PreToolUse` for Bash it claims the one-time pass Allow left and returns `"ask"`, or returns at once with no output when no pass matches. `PermissionDenied` is also logged by `studio-hook` like every other event.
