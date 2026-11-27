#!/bin/sh
# Packs plotkeeper's npm packages for this machine into <out-dir> (#368, #706): the launcher package `npm/`
# (`plotkeeper`, whose `launcher.js` is the command), and the platform package it runs, with `plotkeeper` (the page
# embedded), `plotkeeper-hook`, `plotkeeper-answer` and `plotkeeper-run` side by side in its `bin/`.
# On macOS that is `plotkeeper-darwin`, universal binaries (Apple silicon and Intel), which needs the Rust targets
# aarch64-apple-darwin and x86_64-apple-darwin. On Linux it is `plotkeeper-linux-x64` or `plotkeeper-linux-arm64` for
# this machine's architecture, static musl binaries that need no particular glibc, which needs the Rust target
# x86_64-unknown-linux-musl or aarch64-unknown-linux-musl and musl-gcc (Debian's and Ubuntu's `musl-tools`).
set -eu
out=${1:?usage: scripts/npm/pack.sh <out-dir>}
mkdir -p "$out"
out=$(cd "$out" && pwd)
cd "$(dirname "$0")/../.."

case "$(uname -s)-$(uname -m)" in
  Darwin-*) platform=darwin targets="aarch64-apple-darwin x86_64-apple-darwin" ;;
  Linux-x86_64) platform=linux-x64 targets=x86_64-unknown-linux-musl ;;
  Linux-aarch64) platform=linux-arm64 targets=aarch64-unknown-linux-musl ;;
  *)
    echo "pack: plotkeeper has no npm package for $(uname -s) on $(uname -m)" >&2
    exit 1
    ;;
esac
# ring, under ureq's TLS, compiles C; for a musl target cc-rs would look for a cross compiler named after it.
export CC_x86_64_unknown_linux_musl=musl-gcc CC_aarch64_unknown_linux_musl=musl-gcc

npm run build
for target in $targets; do
  cargo build --locked --release --target "$target" -p studio-server -p studio-hook
done
package=target/npm/plotkeeper-$platform
rm -rf "$package"
mkdir -p "$package/bin"
# All side by side in one real folder: each start copies the hook binaries beside `plotkeeper` into the data
# folder's `bin/` (#356), `plotkeeper install` needs them there too (#357), and a background run's launcher,
# `plotkeeper-run`, is started from beside `plotkeeper` (#390).
for name in plotkeeper plotkeeper-hook plotkeeper-answer plotkeeper-run; do
  if [ "$platform" = darwin ]; then
    lipo -create -output "$package/bin/$name" \
      "target/aarch64-apple-darwin/release/$name" "target/x86_64-apple-darwin/release/$name"
    # Apple silicon refuses an unsigned binary; the linker's ad-hoc signature is enough, and lipo keeps it. The x86_64
    # slice is unsigned by design (the linker does not sign x86_64 output, and macOS does not require it), so it is
    # not checked (#440).
    if ! codesign -v -a arm64 "$package/bin/$name"; then
      echo "pack: the arm64 slice of $package/bin/$name is not validly signed" >&2
      exit 1
    fi
  else
    cp "target/$targets/release/$name" "$package/bin/$name"
  fi
done
node scripts/npm/release.mjs platform "$platform" "$package"
cp LICENSE "$package/LICENSE"
node scripts/npm/prepare-readme.mjs
npm pack ./npm --pack-destination "$out"
npm pack "./$package" --pack-destination "$out"
