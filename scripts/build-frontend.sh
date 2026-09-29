#!/usr/bin/env bash
# Rebuilds public/ from the doodle-duo-space frontend.
#
#   scripts/build-frontend.sh                 # clone the default branch
#   scripts/build-frontend.sh ../doodle-duo-space   # use a local checkout
#   FRONTEND_REF=some-branch scripts/build-frontend.sh
set -euo pipefail

REPO_URL="${FRONTEND_REPO:-https://github.com/abhishek-shivale/doodle-duo-space}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/public"

if [[ $# -ge 1 ]]; then
  SRC="$(cd "$1" && pwd)"
else
  SRC="$(mktemp -d)"
  trap 'rm -rf "$SRC"' EXIT
  git clone --depth 1 ${FRONTEND_REF:+--branch "$FRONTEND_REF"} "$REPO_URL" "$SRC"
fi

cd "$SRC"
if command -v bun >/dev/null; then
  bun install --frozen-lockfile
else
  npm ci
fi

BUILD="$(mktemp -d)"
npx vite build --outDir "$BUILD" --emptyOutDir
rm -rf "$OUT"
mv "$BUILD" "$OUT"
chmod -R u=rwX,go=rX "$OUT"
echo "frontend $(git -C "$SRC" rev-parse --short HEAD) built into $OUT"
