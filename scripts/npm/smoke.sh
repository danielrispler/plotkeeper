#!/bin/sh
# Installs a packed plotkeeper with `npm i -g <tarball>`, with no Rust toolchain on the PATH, in a fresh home and npm
# prefix, and checks it as a user meets it (#368): `npx` of the tarball, which with no terminal asks nothing, wires
# nothing and serves (#434), `plotkeeper --version`, the four binaries side by side for this machine's architecture,
# `plotkeeper install`, a `plotkeeper` start that serves the page, and hooks that keep recording once the npm install
# folder is gone, as after a switch of Node version.
set -eu
tarball=${1:?usage: scripts/npm/smoke.sh <tarball>}
tarball=$(cd "$(dirname "$tarball")" && pwd)/$(basename "$tarball")
node_bin=$(dirname "$(command -v node)")
work=$(mktemp -d)
server=
cleanup() {
  [ -n "$server" ] && kill "$server" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
fail() {
  echo "smoke: $*" >&2
  exit 1
}

export HOME="$work/home"
mkdir -p "$HOME" "$work/agents"
prefix="$work/prefix"
# A stand-in `claude`, so install renders Claude Code's plugin; it records what install asks of it.
cat > "$work/agents/claude" <<'SH'
#!/bin/sh
echo "$*" >> "$HOME/claude-calls"
SH
chmod +x "$work/agents/claude"
export PATH="$prefix/bin:$node_bin:$work/agents:/usr/bin:/bin:/usr/sbin:/sbin"
export PLOTKEEPER_NO_BROWSER=1
! command -v cargo >/dev/null && ! command -v rustc >/dev/null || fail "a Rust toolchain is on the PATH"

data="$HOME/Library/Application Support/plotkeeper"
# Waits for the start running as $server to print its address into the file $1, and checks the page is served there.
serves() {
  tries=0
  until grep -q "^plotkeeper: http" "$1"; do
    tries=$((tries + 1))
    [ "$tries" -lt 600 ] || fail "plotkeeper printed no address: $(cat "$1")"
    kill -0 "$server" 2>/dev/null || fail "plotkeeper stopped: $(cat "$1")"
    sleep 0.1
  done
  url=$(sed -n 's/^plotkeeper: \(http.*\)/\1/p' "$1")
  curl -sfL -c "$work/jar" -b "$work/jar" "$url" | grep -q "<title>Plotkeeper</title>" || fail "the page is not served at $url"
}

# The one command, as a first-time user runs it. CI gives it no terminal, so it never asks or wires: it serves and
# prints the command that wires (#434), on stderr: stdout's first line is the address.
npx --yes --package "$tarball" plotkeeper < /dev/null > "$work/npx-out" 2> "$work/npx-err" &
server=$!
serves "$work/npx-out"
head -n 1 "$work/npx-out" | grep -q "^plotkeeper: http" || fail "npx plotkeeper's first line on stdout is not its address: $(cat "$work/npx-out")"
grep -q "plotkeeper install --yes" "$work/npx-err" || fail "npx plotkeeper did not print the command that wires on stderr: $(cat "$work/npx-err")"
[ ! -e "$HOME/claude-calls" ] || fail "npx plotkeeper with no terminal ran claude: $(cat "$HOME/claude-calls")"
[ ! -e "$data/install" ] || fail "npx plotkeeper with no terminal wired or stored an answer"
kill -INT "$server"
wait "$server" || fail "npx plotkeeper did not stop cleanly on Ctrl+C"
server=
[ ! -e "$data/plotkeeper.running.json" ] || fail "the npx start is still running"

npm i -g --prefix "$prefix" "$tarball"
package="$prefix/lib/node_modules/plotkeeper"

version=$(node -p "require('$package/package.json').version")
[ "$(plotkeeper --version)" = "plotkeeper $version" ] || fail "plotkeeper --version says $(plotkeeper --version)"

arch=$(uname -m)
for name in plotkeeper plotkeeper-hook plotkeeper-answer plotkeeper-run; do
  [ -x "$package/bin/$name" ] || fail "$name is not an executable in $package/bin"
  lipo "$package/bin/$name" -verify_arch "$arch" || fail "$name has no $arch build"
done

plotkeeper install --yes
for name in plotkeeper-hook plotkeeper-answer; do
  cmp "$package/bin/$name" "$data/bin/$name" || fail "install did not copy $name into the data folder's bin/"
done
grep -q "plugin install" "$HOME/claude-calls" || fail "install did not install Claude Code's plugin"
hooks="$data/plugins/claude/plotkeeper/hooks/hooks.json"
grep -q "$prefix" "$hooks" && fail "the hooks name the npm install folder"

plotkeeper < /dev/null > "$work/out" &
server=$!
serves "$work/out"
kill -INT "$server"
wait "$server" || fail "plotkeeper did not stop cleanly on Ctrl+C"
server=

# The npm install folder goes, as when Node is switched; the hooks run the data folder's copies and still record.
npm rm -g --prefix "$prefix" plotkeeper
[ ! -e "$package" ] || fail "the package is still installed"
command=$(node -p "require('$hooks').hooks.SessionStart[0].hooks[0].command")
echo '{"hook_event_name":"SessionStart","session_id":"smoke-368"}' | sh -c "$command"
grep -q "smoke-368" "$data/events.jsonl" || fail "the hook recorded nothing after the package was removed"
echo "smoke: ok"
