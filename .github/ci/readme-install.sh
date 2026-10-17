#!/usr/bin/env bash
# Runs an install block from README.md verbatim, except that the release
# download line installs the freshly built binary instead. The final curl is
# left to verify-unix.sh.
# Usage: readme-install.sh '<section heading prefix>' <binary> '<install path>'
set -euo pipefail
heading=$1
bin=$(realpath "$2")
dest=$3
script=$(mktemp)
awk -v h="$heading" 'index($0, h) == 1 { section = 1 }
     section && /^```sh$/ { inblock = 1; next }
     inblock && /^```$/ { exit }
     inblock' README.md |
  sed -E "s|^curl -fsSL https://github\.com/.*$|install -m 755 '$bin' $dest|" |
  grep -v "^curl -s 'http://\[::1\]" > "$script"
grep -q "^install -m 755 '$bin' " "$script" || { echo "download line not found in README"; exit 1; }
cat "$script"
bash -e "$script"
