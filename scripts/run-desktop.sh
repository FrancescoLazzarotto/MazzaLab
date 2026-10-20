#!/usr/bin/env sh
# Builds the web UI (first run or with REBUILD=1) and starts the MazzaLab desktop app.
# Usage: scripts/run-desktop.sh [path/to/show.mazzalab]   (RELEASE=1 for an optimized build)
set -e
root="$(cd "$(dirname "$0")/.." && pwd)"
web="$root/apps/mazzalab-web"

if [ -n "$REBUILD" ] || [ ! -f "$web/dist/index.html" ]; then
  (cd "$web" && { [ -d node_modules ] || npm ci; } && npm run build)
fi

cd "$root"
if [ -n "$RELEASE" ]; then
  cargo run -p mazzalab-desktop --release -- "$@"
else
  cargo run -p mazzalab-desktop -- "$@"
fi
