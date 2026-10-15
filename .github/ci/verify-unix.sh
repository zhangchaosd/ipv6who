#!/usr/bin/env bash
# Checks an installed ipv6who service on Linux or macOS.
# Usage: verify-unix.sh <expected service user> <this host's LAN IPv4>
set -euo pipefail
expected_user=$1
own_ip=$2

body=
for _ in $(seq 1 30); do
  body=$(curl -s 'http://[::1]:8066/v1/ipv6' || true)
  if python3 -c 'import json,sys; sys.exit(json.loads(sys.argv[1]).get("address") != "2001:db8::1")' "$body" 2>/dev/null; then
    break
  fi
  sleep 1
done
echo "$body"
python3 -c '
import json, sys
d = json.loads(sys.argv[1])
assert d["address"] == "2001:db8::1" and d["type"] == "static", d
' "$body"

echo "IPv4 through the dual-stack socket:"
curl -sf http://127.0.0.1:8066/healthz

user=$(ps -o user= -p "$(pgrep -x ipv6who)" | tr -d ' ')
echo "service user: $user"
[ "$user" = "$expected_user" ]

echo "connecting from $own_ip, which is not allowlisted:"
set +e
curl -s -m 5 "http://$own_ip:8066/healthz"
rc=$?
set -e
echo "curl exit $rc"
[ "$rc" = 52 ] || [ "$rc" = 56 ]
