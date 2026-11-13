#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "usage: mkstage.sh <root-dir> <output.tar.gz>"
    echo "  упаковывает готовый корень Serpens в stage-тарбол"
    exit 1
}

ROOT="${1:-}"
OUT="${2:-}"
[ -z "$ROOT" ] && usage
[ -z "$OUT" ] && usage
[ -d "$ROOT" ] || { echo "нет каталога $ROOT"; exit 1; }

tar -czf "$OUT" \
    -C "$ROOT" \
    --numeric-owner \
    --exclude='./dev/*' \
    --exclude='./proc/*' \
    --exclude='./sys/*' \
    --exclude='./tmp/*' \
    --exclude='./run/*' \
    --exclude='./var/cache/flexura/distfiles/*' \
    --exclude='./var/tmp/flexura/*' \
    .

echo "готово: $OUT"
