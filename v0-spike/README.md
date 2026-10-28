# v0 guess-and-card loop (#315), throwaway

Not the studio's stack: the frontend is plain HTML and JS with no build step, and the hooks are shell scripts that need `jq`.

Run the panel:

    cd apps/studio/v0-spike/panel && cargo run                      # fake card
    cd apps/studio/v0-spike/panel && CARD_WRITER=claude cargo run   # real `claude -p` card writer

Run Claude Code with the hooks, from any project:

    claude --settings /Users/danielrispler/work/Falinks/apps/studio/v0-spike/hook/settings.json
