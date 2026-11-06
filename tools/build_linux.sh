#!/bin/sh
# Builds the Linux release and packages it as dist/rotfall-linux-x64.tar.gz.
set -e
cd "$(dirname "$0")/.."
cargo build --release "$@"
DIST=dist/rotfall-linux-x64
rm -rf "$DIST" "$DIST.tar.gz"
mkdir -p "$DIST"
cp target/release/rotfall LICENSE README.md "$DIST/"
tar -C dist -czf "$DIST.tar.gz" rotfall-linux-x64
ls -la "$DIST.tar.gz"
