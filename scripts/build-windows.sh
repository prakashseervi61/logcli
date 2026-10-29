#!/usr/bin/env bash
# Cross-compile a native Windows executable from the Linux Rust container.
#
# Usage:
#   ./scripts/build-windows.sh            # writes dist/logcli.exe
#   ./scripts/build-windows.sh --install  # also copies to ~/bin
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="logcli-rust:windows"
dist="$root/dist"

mkdir -p "$dist"

docker build -q -f "$root/scripts/Dockerfile.windows" -t "$image" "$root" >/dev/null

export MSYS_NO_PATHCONV=1
volume="$(cygpath -m "$root" 2>/dev/null || echo "$root")"

docker run --rm \
    -v "${volume}:/work" \
    -v "${volume}/dist:/out" \
    -v logcli-target:/work/target \
    -v logcli-registry:/usr/local/cargo/registry \
    -w /work \
    "$image" \
    sh -c 'cargo build --release --target x86_64-pc-windows-gnu && cp target/x86_64-pc-windows-gnu/release/logcli.exe /out/'

echo "Built $dist/logcli.exe"

if [[ "${1:-}" == "--install" ]]; then
    mkdir -p "$HOME/bin"
    cp -f "$dist/logcli.exe" "$HOME/bin/logcli.exe"
    echo "Installed to $HOME/bin/logcli.exe"
fi
