#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/mel0k1/serpens.git"
PROFILE="default/x86-64"
DEFAULT_PKGS="serpens-base busybox bash coreutils ncurses zlib readline"

usage() {
    echo "usage: mkstage.sh <output.tar.gz> [--pkgs \"список\"] [--profile ИМЯ] [--repo URL] [--jobs N] [--keep]"
    echo "  бутстрапит корень Serpens, ставит базовые пакеты и пакует stage-тарбол"
    echo "  пакеты собираются тулчейном текущего хоста, нужен gcc, make, tar"
    exit 1
}

log() {
    echo "mkstage: $*"
}

SRC_DIR="$(cd "$(dirname "$0")/.." && pwd)"
OUT=""
PKGS="${SERPENS_STAGE_PKGS:-$DEFAULT_PKGS}"
JOBS=""
KEEP=0

while [ $# -gt 0 ]; do
    case "$1" in
        --pkgs) PKGS="$2"; shift 2 ;;
        --profile) PROFILE="$2"; shift 2 ;;
        --repo) REPO_URL="$2"; shift 2 ;;
        --jobs) JOBS="$2"; shift 2 ;;
        --keep) KEEP=1; shift ;;
        -h|--help) usage ;;
        *) if [ -z "$OUT" ]; then OUT="$1"; shift; else usage; fi ;;
    esac
done

[ -z "$OUT" ] && usage
command -v cargo >/dev/null 2>&1 || export PATH="$HOME/.cargo/bin:$PATH"

FLEXURA="$SRC_DIR/target/release/flexura"
if [ ! -x "$FLEXURA" ] || [ "$SRC_DIR/flexura/src/main.rs" -nt "$FLEXURA" ]; then
    log "собираю flexura"
    cargo build --release --manifest-path "$SRC_DIR/Cargo.toml" >/dev/null 2>&1 || cargo build --release --manifest-path "$SRC_DIR/Cargo.toml"
fi

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/serpens-stage.XXXXXX")"
cleanup() {
    if [ "$KEEP" = 1 ]; then
        log "корень оставлен в $STAGE"
    else
        rm -rf "$STAGE"
    fi
}
trap cleanup EXIT

log "готовлю корень $STAGE"
for d in bin boot dev etc/flexura home mnt opt proc root run srv sys tmp usr/bin usr/sbin usr/lib var/cache/flexura/distfiles var/cache/flexura/packages var/db/flexura var/log/flexura var/tmp/flexura; do
    mkdir -p "$STAGE/$d"
done
chmod 1777 "$STAGE/tmp"

install -m 755 "$FLEXURA" "$STAGE/usr/bin/flexura"

write_conf() {
    {
        echo "[main]"
        echo "profile = \"$PROFILE\""
        echo "repos = [\"$1\"]"
        if [ -n "$JOBS" ]; then
            echo "jobs = $JOBS"
        fi
        echo ""
        echo "[flags]"
        echo "use = []"
    } > "$STAGE/etc/flexura/flexura.conf"
}

write_conf "$SRC_DIR"
log "ставлю пакеты: $PKGS"
if ! FLEXURA_ROOT="$STAGE" "$FLEXURA" install $PKGS; then
    mkdir -p flexura-logs
    cp "$STAGE/var/log/flexura/"*.log flexura-logs/ 2>/dev/null || true
    log "сборка сорвалась, логи в $PWD/flexura-logs/"
    exit 1
fi

write_conf "$REPO_URL"
log "ставлю исполнимый конфиг с репозиторием $REPO_URL"

rm -rf "$STAGE/var/log/flexura/"*.log "$STAGE/var/tmp/flexura/"* "$STAGE/var/cache/flexura/distfiles/"* 2>/dev/null || true

log "пакую $OUT"
tar -czf "$OUT" \
    -C "$STAGE" \
    --numeric-owner \
    --owner=0 --group=0 \
    --exclude='./dev/*' \
    --exclude='./proc/*' \
    --exclude='./sys/*' \
    --exclude='./run/*' \
    --exclude='./tmp/*' \
    --exclude='./var/cache/flexura/distfiles/*' \
    --exclude='./var/tmp/flexura/*' \
    --exclude='./var/log/flexura/*' \
    --exclude='./var/db/repos/*' \
    .

SIZE="$(du -h "$OUT" | cut -f1)"
COUNT="$(FLEXURA_ROOT="$STAGE" "$FLEXURA" list -i | wc -l)"
log "готово: $OUT ($SIZE), пакетов: $COUNT"
