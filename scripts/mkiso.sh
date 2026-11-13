#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "usage: mkiso.sh <stage.tar.gz> <output.iso> [--kernel ФАЙЛ]"
    echo "  собирает загрузочный live-ISO: ядро, initramfs со static busybox"
    echo "  и stage-тарбол; при загрузке stage разворачивается в RAM"
    echo "  нужны: cpio, gzip, grub-mkrescue, xorriso, mtools, grub-pc-bin и grub-efi-amd64-bin"
    exit 1
}

die() {
    echo "mkiso: $*" >&2
    exit 1
}

STAGE=""
OUT=""
KERNEL=""

while [ $# -gt 0 ]; do
    case "$1" in
        --kernel) KERNEL="$2"; shift 2 ;;
        -h|--help) usage ;;
        *) if [ -z "$STAGE" ]; then STAGE="$1"; shift; elif [ -z "$OUT" ]; then OUT="$1"; shift; else usage; fi ;;
    esac
done

[ -n "$STAGE" ] && [ -n "$OUT" ] || usage
[ -f "$STAGE" ] || die "нет файла $STAGE"

for t in tar gzip cpio grub-mkrescue xorriso mformat; do
    command -v "$t" >/dev/null || die "нет $t"
done

WORK="$(mktemp -d "${TMPDIR:-/tmp}/serpens-iso.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

ISO="$WORK/iso"
INITFS="$WORK/initfs"
mkdir -p "$ISO/boot/grub" "$INITFS/bin" "$INITFS/dev" "$INITFS/proc" "$INITFS/sys" "$INITFS/tmp" "$INITFS/mnt" "$INITFS/newroot" "$INITFS/run"
chmod 1777 "$INITFS/tmp"

if [ -n "$KERNEL" ]; then
    [ -f "$KERNEL" ] || die "нет файла $KERNEL"
    cp "$KERNEL" "$ISO/boot/vmlinuz-serpens"
else
    ENTRY="$(tar -tzf "$STAGE" | grep -m1 '/boot/vmlinuz-' || true)"
    [ -n "$ENTRY" ] || die "в stage нет /boot/vmlinuz-*, собери stage с пакетом linux"
    tar -xzf "$STAGE" -C "$WORK" "$ENTRY"
    cp "$WORK/$ENTRY" "$ISO/boot/vmlinuz-serpens"
fi

tar -xzf "$STAGE" -C "$WORK" ./usr/bin/busybox
file "$WORK/usr/bin/busybox" | grep -q 'statically linked' \
    || die "busybox в stage динамический, собери stage с флагом static у busybox"
cp "$WORK/usr/bin/busybox" "$INITFS/bin/busybox"
ln -sf busybox "$INITFS/bin/sh"

cat > "$INITFS/init" <<'INIT_EOF'
#!/bin/busybox sh
export PATH=/bin
/bin/busybox mkdir -p /proc /sys /dev /tmp /mnt /newroot /run
/bin/busybox mount -t proc none /proc
/bin/busybox mount -t sysfs none /sys
/bin/busybox mount -t devtmpfs devtmpfs /dev 2>/dev/null
/bin/busybox mdev -s 2>/dev/null

SRC=""
n=0
while [ -z "$SRC" ] && [ $n -lt 15 ]; do
    for d in /dev/sr0 /dev/sr1 /dev/sd[a-z] /dev/vd[a-z] /dev/nvme0n1; do
        [ -b "$d" ] || continue
        if /bin/busybox mount -t iso9660 -o ro "$d" /mnt 2>/dev/null; then
            if /bin/busybox ls /mnt/serpens-stage-*.tar.gz >/dev/null 2>&1; then
                SRC="$d"
                break
            fi
            /bin/busybox umount /mnt 2>/dev/null
        fi
    done
    [ -n "$SRC" ] || { n=$((n+1)); /bin/busybox sleep 1; }
done

if [ -z "$SRC" ]; then
    echo "serpens-iso: носитель со stage не найден, отдаю shell"
    exec /bin/busybox sh
fi

echo "serpens-iso: живой корень с $SRC"
/bin/busybox mount -t tmpfs -o size=80% tmpfs /newroot
STAGE=$(/bin/busybox ls /mnt/serpens-stage-*.tar.gz | /bin/busybox head -n 1)
/bin/busybox tar -xzf "$STAGE" -C /newroot
/bin/busybox mkdir -p /newroot/media/serpens
/bin/busybox mount -o move /mnt /newroot/media/serpens
exec /bin/busybox switch_root /newroot /sbin/init
INIT_EOF
chmod +x "$INITFS/init"

( cd "$INITFS" && find . -print0 | cpio --null -o -H newc --quiet | gzip -9 ) \
    > "$ISO/boot/initramfs.cpio.gz"

cat > "$ISO/boot/grub/grub.cfg" <<'GRUB_EOF'
serial --unit=0 --speed=115200
terminal_input serial console
terminal_output serial console
set timeout=3
set default=0
echo "grub: старт Serpens live"
menuentry "Serpens live" {
    echo "grub: загружаю ядро"
    linux /boot/vmlinuz-serpens console=tty0 console=ttyS0,115200 earlyprintk=serial,ttyS0,115200,keep
    echo "grub: загружаю initramfs"
    initrd /boot/initramfs.cpio.gz
    echo "grub: передаю управление ядру"
    boot
}
GRUB_EOF

cp "$STAGE" "$ISO/"

grub-mkrescue -o "$OUT" "$ISO" -- -volid SERPENS_LIVE
echo "mkiso: готово: $OUT ($(du -h "$OUT" | cut -f1))"
