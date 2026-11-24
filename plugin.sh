#!/bin/sh
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
sys="$(uname -s)"
case "$sys" in
MINGW* | MSYS* | CYGWIN*) rel="x86_64-pc-windows-msvc/context.exe" ;;
Darwin*)
    case "$(uname -m)" in
    arm64) rel="aarch64-apple-darwin/context" ;;
    *) rel="x86_64-apple-darwin/context" ;;
    esac ;;
*)
    case "$(uname -m)" in
    aarch64 | arm64) rel="aarch64-unknown-linux-musl/context" ;;
    *) rel="x86_64-unknown-linux-musl/context" ;;
    esac ;;
esac
if [ -x "$here/$rel" ]; then exec "$here/$rel" "$@"; fi
for cand in "$here/target/release/context.exe" "$here/target/release/context" "$here/target/debug/context.exe" "$here/target/debug/context"; do
    if [ -x "$cand" ]; then exec "$cand" "$@"; fi
done
echo "context binary not found in $here" >&2
exit 1
