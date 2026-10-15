#!/bin/sh
# Builds the vendored-dependency tarball used as Source1 in ipv6who.spec.
# Usage: dist/vendor.sh   (run from the repository root)
set -eu
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
out="ipv6who-${version}-vendor.tar.xz"
rm -rf vendor
cargo vendor --quiet --locked --versioned-dirs vendor >/dev/null
tar -cJf "$out" vendor
rm -rf vendor
echo "$out"
