#!/usr/bin/env bash
set -euo pipefail

DIST="${1:-/tmp/flexura-test}"
mkdir -p "$DIST"

WORK="$(mktemp -d)"
NAME="hello"
VERSION="1.0"
DIR="$WORK/$NAME-$VERSION"
mkdir -p "$DIR"

cat > "$DIR/build.sh" <<'EOF'
mkdir -p "$D/usr/bin"
printf '#!/bin/sh\necho Serpens %s\n' "$(cat version.txt)" > "$D/usr/bin/hello"
chmod +x "$D/usr/bin/hello"
EOF
echo "1.0" > "$DIR/version.txt"
chmod +x "$DIR/build.sh"

tar -czf "$DIST/$NAME-$VERSION.tar.gz" -C "$WORK" "$NAME-$VERSION"
SUM="$(sha256sum "$DIST/$NAME-$VERSION.tar.gz" | cut -d' ' -f1)"

cat > "$DIST/$NAME.flx" <<EOF
[package]
name = "$NAME"
version = "$VERSION"
description = "Тестовый пакет для проверки flexura"
homepage = "https://github.com/mel0k1/serpens"
license = "MIT"

[source]
url = "file://$DIST/$NAME-$VERSION.tar.gz"
sha256 = "$SUM"

[build]
install = ["./build.sh"]
EOF

rm -rf "$WORK"
echo "готово: $DIST/$NAME-$VERSION.tar.gz"
echo "рецепт: $DIST/$NAME.flx"
