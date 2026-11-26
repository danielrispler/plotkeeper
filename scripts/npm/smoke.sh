#!/bin/sh
# Installs the packages scripts/npm/pack.sh packed into <dir>, the launcher `plotkeeper` and this machine's platform
# package, with no Rust toolchain on the PATH, in a fresh home and npm prefix, and checks them as a user meets them
# (#368, #706): `npx` of the tarballs, which with no terminal asks nothing, wires nothing and serves (#434), then
# `npm i -g`, `plotkeeper --version` through the launcher, the four binaries side by side in the platform package for
# this machine's architecture, `plotkeeper install`, a `plotkeeper` start that serves the page and stops on Ctrl+C,
# and hooks that keep recording once the npm install folder is gone, as after a switch of Node version.
# The registry has no platform package at an unreleased version, so each is installed beside the launcher from its
# tarball, where the launcher finds it as it finds the one npm nests in its own node_modules.
set -eu
script_dir=$(cd "$(dirname "$0")" && pwd)
dir=${1:?usage: scripts/npm/smoke.sh <dir>}
dir=$(cd "$dir" && pwd)
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
case "$(uname -s)-$(uname -m)" in
  Darwin-*) platform=plotkeeper-darwin ;;
  Linux-x86_64) platform=plotkeeper-linux-x64 ;;
  Linux-aarch64) platform=plotkeeper-linux-arm64 ;;
  *) fail "plotkeeper has no npm package for $(uname -s) on $(uname -m)" ;;
esac
version=$(node -p "require('$script_dir/../../npm/package.json').version")
launcher_tarball=$dir/plotkeeper-$version.tgz
platform_tarball=$dir/$platform-$version.tgz
[ -f "$launcher_tarball" ] && [ -f "$platform_tarball" ] || fail "$dir lacks plotkeeper-$version.tgz or $platform-$version.tgz"
! command -v cargo >/dev/null && ! command -v rustc >/dev/null || fail "a Rust toolchain is on the PATH"

# The data folder by OS (#700): macOS keeps it in Library, Linux in XDG's data folder, here with no XDG_DATA_HOME.
unset XDG_DATA_HOME XDG_CACHE_HOME
case "$(uname -s)" in
  Darwin) data="$HOME/Library/Application Support/plotkeeper" ;;
  *) data="$HOME/.local/share/plotkeeper" ;;
esac
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
npx --yes --package "$platform_tarball" --package "$launcher_tarball" plotkeeper < /dev/null > "$work/npx-out" 2> "$work/npx-err" &
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

npm i -g --prefix "$prefix" "$platform_tarball" "$launcher_tarball"
package="$prefix/lib/node_modules/plotkeeper"
bin=$(dirname "$(node -p "require.resolve('$platform/package.json', { paths: ['$package'] })")")/bin

[ -s "$package/README.md" ] || fail "the packed package has no README.md"
[ -s "$package/LICENSE" ] || fail "the packed package has no LICENSE"
grep -q "https://raw.githubusercontent.com/danielrispler/plotkeeper/main/docs/studio/assets/readme/hero.png" "$package/README.md" \
  || fail "the packed README.md lacks the rewritten hero image URL"
node "$script_dir/prepare-readme.mjs" --check "$package/README.md" \
  || fail "the packed README.md still contains relative links"

[ "$(plotkeeper --version)" = "plotkeeper $version" ] || fail "plotkeeper --version says $(plotkeeper --version)"

# macOS's are universal; Linux's are static, so they run whatever the glibc.
arch=$(uname -m)
for name in plotkeeper plotkeeper-hook plotkeeper-answer plotkeeper-run; do
  [ -x "$bin/$name" ] || fail "$name is not an executable in $bin"
  case "$platform" in
    plotkeeper-darwin) lipo "$bin/$name" -verify_arch "$arch" || fail "$name has no $arch build" ;;
    *)
      described=$(file -b "$bin/$name")
      case "$described" in
        *"static-pie linked"* | *"statically linked"*) ;;
        *) fail "$name is not statically linked: $described" ;;
      esac
      case "$arch-$described" in
        x86_64-*x86-64* | aarch64-*aarch64*) ;;
        *) fail "$name is not an $arch build: $described" ;;
      esac
      ;;
  esac
done

plotkeeper install --yes
for name in plotkeeper-hook plotkeeper-answer; do
  cmp "$bin/$name" "$data/bin/$name" || fail "install did not copy $name into the data folder's bin/"
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
npm rm -g --prefix "$prefix" plotkeeper "$platform"
[ ! -e "$package" ] && [ ! -e "$bin" ] || fail "the packages are still installed"
command=$(node -p "require('$hooks').hooks.SessionStart[0].hooks[0].command")
echo '{"hook_event_name":"SessionStart","session_id":"smoke-368"}' | sh -c "$command"
grep -q "smoke-368" "$data/events.jsonl" || fail "the hook recorded nothing after the package was removed"
echo "smoke: ok"
