#!/bin/sh
# Builds the npm package in `npm/` with the page embedded and universal macOS binaries (Apple silicon and Intel),
# `plotkeeper`, `plotkeeper-hook`, `plotkeeper-answer` and `plotkeeper-run` side by side in its `bin/`, and packs it into <out-dir> (#368).
# Needs the Rust targets aarch64-apple-darwin and x86_64-apple-darwin.
set -eu
out=${1:?usage: scripts/npm/pack.sh <out-dir>}
mkdir -p "$out"
out=$(cd "$out" && pwd)
cd "$(dirname "$0")/../.."

npm run build
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --locked --release --target "$target" -p studio-server -p studio-hook
done
rm -rf npm/bin
mkdir npm/bin
# All side by side in one real folder: each start copies the hook binaries beside `plotkeeper` into the data
# folder's `bin/` (#356), `plotkeeper install` needs them there too (#357), and a background run's launcher,
# `plotkeeper-run`, is started from beside `plotkeeper` (#390).
for name in plotkeeper plotkeeper-hook plotkeeper-answer plotkeeper-run; do
  lipo -create -output "npm/bin/$name" \
    "target/aarch64-apple-darwin/release/$name" "target/x86_64-apple-darwin/release/$name"
done
npm pack ./npm --pack-destination "$out"
