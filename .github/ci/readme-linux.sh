#!/usr/bin/env bash
# Runs the Linux install block from README.md, with the freshly built binary
# in place of the release download. The final curl is left to verify-unix.sh.
set -euo pipefail
bin=$(realpath "$1")
script=$(mktemp)
awk '/^### Linux/ { section = 1 }
     section && /^```sh$/ { inblock = 1; next }
     inblock && /^```$/ { exit }
     inblock' README.md |
  sed -E "s|^curl -fsSL https://github\.com/.*$|cp '$bin' ipv6who|" |
  grep -v "^curl -s 'http://\[::1\]" > "$script"
grep -q "^cp '$bin' ipv6who$" "$script" || { echo "download line not found in README"; exit 1; }
cat "$script"
bash -e "$script"
