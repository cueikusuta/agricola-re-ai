#!/usr/bin/env bash
# Local, self-contained browser E2E run: starts the loopback web server, runs
# the requested node test file, and always shuts the server down again.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

# Use a dedicated port so a leftover server from another session is never
# mistaken for the binary under test.
PORT="${AGRICOLA_WEB_PORT:-8799}"
LOG="artifacts/agricola-web-local.log"
mkdir -p artifacts
AGRICOLA_WEB_PORT="$PORT" ./target/debug/agricola-re-ai.exe --web >"$LOG" 2>&1 &
SERVER=$!
cleanup() { kill "$SERVER" 2>/dev/null; wait "$SERVER" 2>/dev/null; }
trap cleanup EXIT

ready=0
for _ in $(seq 1 80); do
  if curl -fsS "http://127.0.0.1:${PORT}/" >/dev/null 2>&1; then ready=1; break; fi
  sleep 0.25
done
if [ "$ready" != 1 ]; then
  echo "server did not become ready"
  cat "$LOG"
  exit 1
fi

export AGRICOLA_WEB_URL="http://127.0.0.1:${PORT}/"
node --test --test-concurrency=1 "$@"
status=$?
cleanup
trap - EXIT
exit "$status"
