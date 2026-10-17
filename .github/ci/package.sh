#!/usr/bin/env bash
# Packages a built binary with docs and service files.
# Usage: package.sh <path to binary> <label, e.g. linux-x86_64>
set -euo pipefail
bin=$1
label=$2
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
name="ipv6who-${version}-${label}"
mkdir -p "out/$name"
cp "$bin" LICENSE README.md README.zh-CN.md "out/$name/"
cp -r dist "out/$name/dist"
rm "out/$name/dist/vendor.sh"
cd out
if [[ $label == windows-* ]]; then
  7z a -tzip -bso0 "$name.zip" "$name"
  archive="$name.zip"
else
  tar -czf "$name.tar.gz" "$name"
  archive="$name.tar.gz"
fi
if command -v sha256sum >/dev/null; then
  sha256sum "$archive" > "$archive.sha256"
else
  shasum -a 256 "$archive" > "$archive.sha256"
fi
rm -r "$name"
ls -l
