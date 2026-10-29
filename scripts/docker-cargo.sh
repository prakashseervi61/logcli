#!/usr/bin/env bash
# Run cargo inside a Rust container (for hosts without a Rust toolchain).
#
# The image is rebuilt from scripts/Dockerfile.dev on demand (cached after the
# first run) so rustfmt and clippy are always available.
#
# Usage:
#   ./scripts/docker-cargo.sh build
#   ./scripts/docker-cargo.sh test
#   ./scripts/docker-cargo.sh run -- tests/fixtures/simple.jsonl
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
image="logcli-rust:dev"

docker build -q -f "$root/scripts/Dockerfile.dev" -t "$image" "$root" >/dev/null

# Docker Desktop expects Windows-style paths on Windows; keep them as-is.
if command -v cygpath >/dev/null 2>&1; then
    volume="$(cygpath -m "$root")"
else
    volume="$root"
fi

# Git Bash rewrites POSIX paths in arguments (e.g. -w /work); disable that.
export MSYS_NO_PATHCONV=1

docker run --rm \
    -v "${volume}:/work" \
    -v logcli-target:/work/target \
    -v logcli-registry:/usr/local/cargo/registry \
    -w /work \
    "$image" \
    cargo "$@"
