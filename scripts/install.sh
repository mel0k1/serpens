#!/usr/bin/env bash
set -euo pipefail

STAGE_URL_DEFAULT="https://github.com/mel0k1/serpens/releases/latest/download/serpens-stage-x86_64.tar.gz"
MNT="/mnt/serpens"

usage() {
    echo "usage: install.sh [--stage URL|путь] [--disk DEV] [--efi|--bios] [--hostname ИМЯ] [--timezone ЗОНА] [--grub] [--yes]"
    echo "  размечает диск, разворачивает stage-тарбол Serpens и готовит chroot"
    echo "  запуск от root, данные на целевом диске будут уничтожены"
    exit 1
}

die() {
    echo "ошибка: $*" >&2
    exit 1
}

log() {
    echo "install: $*"
}

ask() {
    local prompt="$1" def="${2:-}" v
    if [ "$ASSUME_YES" = 1 ]; then
        echo "$def"
        return
    fi
    read -r -p "$prompt [$def]: " v </dev/tty
    echo "${v:-$def}"
}

confirm() {
    if [ "$ASSUME_YES" = 1 ]; then
        return 0
    fi
    read -r -p "$1 [y/N]: " a </dev/tty
    [ "$a" = "y" ] || [ "$a" = "yes" ]
}

STAGE=""
DISK=""
MODE=""
HOSTNAME=""
TIMEZONE=""
GRUB=0
ASSUME_YES=0

while [ $# -gt 0 ]; do
    case "$1" in
        --stage) STAGE="$2"; shift 2 ;;
        --disk) DISK="$2"; shift 2 ;;
        --efi) MODE="efi"; shift ;;
        --bios) MODE="bios"; shift ;;
        --hostname) HOSTNAME="$2"; shift 2 ;;
        --timezone) TIMEZONE="$2"; shift 2 ;;
        --grub) GRUB=1; shift ;;
        --yes) ASSUME_YES=1; shift ;;
        -h|--help) usage ;;
        *) usage ;;
    esac
done

[ "$(id -u)" = 0 ] || die "нужен root"
command -v sfdisk >/dev/null || die "нет sfdisk (util-linux)"
command -v mkfs.ext4 >/dev/null || die "нет mkfs.ext4 (e2fsprogs)"
command -v tar >/dev/null || die "нет tar"

MODE="${MODE:-efi}"
HOSTNAME="${HOSTNAME:-serpens}"
TIMEZONE="${TIMEZONE:-Europe/Moscow}"

if [ -z "$DISK" ]; then
    log "диски в системе:"
    lsblk -d -o NAME,SIZE,MODEL 2>/dev/null | sed 's/^/  /'
    DISK="$(ask "целевой диск" "")"
    [ -n "$DISK" ] || die "диск не указан"
fi
[ -b "$DISK" ] || die "$DISK не блочное устройство"
BASE="$(basename "$DISK")"
case "$BASE" in
    nvme*|mmcblk*) P="$BASE""p" ;;
    *) P="$BASE" ;;
esac
ESP_DEV="/dev/${P}1"
ROOT_DEV="/dev/${P}2"
[ "$MODE" = "efi" ] || ROOT_DEV="/dev/${P}1"

echo "будет перезаписан $DISK: таблица $MODE, root на $ROOT_DEV"
confirm "продолжить" || die "отменено"

log "размечаю $DISK"
if [ "$MODE" = "efi" ]; then
    sfdisk --wipe always "$DISK" <<'FDISK'
label: gpt
name=esp, size=512MiB, type=uefi
name=root, type=linux
FDISK
else
    sfdisk --wipe always "$DISK" <<'FDISK'
label: gpt
name=bios, size=1MiB, type=21686148-6449-6E65-694E-656D6D617070
name=root, type=linux
FDISK
fi
partprobe "$DISK" 2>/dev/null || true
sleep 1

log "форматирую $ROOT_DEV"
mkfs.ext4 -F -L serpens "$ROOT_DEV"
if [ "$MODE" = "efi" ]; then
    command -v mkfs.vfat >/dev/null || die "нет mkfs.vfat (dosfstools)"
    mkfs.vfat -F32 -n SERPENS "$ESP_DEV"
fi

log "монтирую $MNT"
mkdir -p "$MNT/boot/efi"
mount "$ROOT_DEV" "$MNT"
[ "$MODE" = "efi" ] && mount "$ESP_DEV" "$MNT/boot/efi"

TMPDIR_DL="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_DL"' EXIT
if [ -n "$STAGE" ]; then
    case "$STAGE" in
        http://*|https://*)
            log "качаю stage $STAGE"
            curl -fL --retry 3 -o "$TMPDIR_DL/stage.tar.gz" "$STAGE" || die "не скачал stage"
            if curl -sfL -o "$TMPDIR_DL/stage.tar.gz.sha256" "$STAGE.sha256"; then
                (cd "$TMPDIR_DL" && sha256sum -c stage.tar.gz.sha256 >/dev/null) || die "сумма stage не сошлась"
            fi
            STAGE_PATH="$TMPDIR_DL/stage.tar.gz"
            ;;
        *)
            [ -f "$STAGE" ] || die "нет файла $STAGE"
            STAGE_PATH="$STAGE"
            ;;
    esac
else
    log "качаю свежий stage с $STAGE_URL_DEFAULT"
    curl -fL --retry 3 -o "$TMPDIR_DL/stage.tar.gz" "$STAGE_URL_DEFAULT" || die "не скачал stage"
    if curl -sfL -o "$TMPDIR_DL/stage.tar.gz.sha256" "$STAGE_URL_DEFAULT.sha256"; then
        (cd "$TMPDIR_DL" && sha256sum -c stage.tar.gz.sha256 >/dev/null) || die "сумма stage не сошлась"
    fi
    STAGE_PATH="$TMPDIR_DL/stage.tar.gz"
fi

log "распаковываю stage в $MNT"
tar -xzf "$STAGE_PATH" -C "$MNT" --numeric-owner
mkdir -p "$MNT/dev" "$MNT/proc" "$MNT/sys" "$MNT/run"

ROOT_UUID="$(blkid -s UUID -o value "$ROOT_DEV")"
printf 'UUID=%s\t/\text4\tdefaults,noatime\t0 1\ntmpfs\t/tmp\ttmpfs\tnosuid,nodev,mode=1777\t0 0\n' "$ROOT_UUID" > "$MNT/etc/fstab"
log "fstab записан, root=UUID=$ROOT_UUID"

log "настраиваю chroot"
mount --rbind /dev "$MNT/dev"
mount --make-rslave "$MNT/dev"
mount -t proc none "$MNT/proc"
mount --rbind /sys "$MNT/sys"
mount --make-rslave "$MNT/sys"
cp -L /etc/resolv.conf "$MNT/etc/resolv.conf" 2>/dev/null || true

echo "$HOSTNAME" > "$MNT/etc/hostname"
printf '127.0.0.1\tlocalhost\n::1\t\tlocalhost\n' > "$MNT/etc/hosts"

if [ -e "$MNT/usr/share/zoneinfo/$TIMEZONE" ]; then
    ln -sfn "/usr/share/zoneinfo/$TIMEZONE" "$MNT/etc/localtime"
    echo "$TIMEZONE" > "$MNT/etc/timezone"
else
    log "zoneinfo для $TIMEZONE нет в stage, поправь время после установки"
fi

log "синхронизирую рецепты"
chroot "$MNT" /usr/bin/flexura sync || log "sync не удался, поправь network и повтори chroot-ом"

if [ "$ASSUME_YES" = 0 ]; then
    chroot "$MNT" passwd root || log "пароль не задан, задай его вручную"
fi

if [ "$GRUB" = 1 ]; then
    log "ставлю grub"
    if [ -x "$MNT/usr/bin/grub-install" ] || [ -x "$MNT/usr/sbin/grub-install" ]; then
        log "grub из stage, ставлю через chroot"
        if [ "$MODE" = "efi" ]; then
            chroot "$MNT" /usr/bin/grub-install --target=x86_64-efi --efi-directory=/boot/efi --boot-directory=/boot --removable --no-nvram \
                || chroot "$MNT" /usr/sbin/grub-install --target=x86_64-efi --efi-directory=/boot/efi --boot-directory=/boot --removable --no-nvram
        else
            chroot "$MNT" /usr/bin/grub-install --boot-directory=/boot "$DISK" \
                || chroot "$MNT" /usr/sbin/grub-install --boot-directory=/boot "$DISK"
        fi
    else
        command -v grub-install >/dev/null || die "нет grub-install ни в stage, ни на хосте"
        log "grub с хоста"
        if [ "$MODE" = "efi" ]; then
            grub-install --target=x86_64-efi --efi-directory="$MNT/boot/efi" --boot-directory="$MNT/boot" --removable --no-nvram
        else
            grub-install --boot-directory="$MNT/boot" "$DISK"
        fi
    fi
    KERNELS="$(ls "$MNT"/boot/vmlinuz-* 2>/dev/null || true)"
    if [ -n "$KERNELS" ]; then
        ROOT_UUID="$(blkid -s UUID -o value "$ROOT_DEV")"
        {
            echo "set timeout=5"
            echo "set default=0"
            echo "menuentry \"Serpens Linux\" {"
            for k in $KERNELS; do
                echo "    linux /boot/$(basename "$k") root=UUID=$ROOT_UUID rw console=tty0 console=ttyS0,115200"
                break
            done
            echo "}"
        } > "$MNT/boot/grub/grub.cfg"
        log "grub.cfg записан, root=UUID=$ROOT_UUID"
    else
        log "в stage нет ядра: recipe linux ещё не в дереве, добавь ядро и напиши grub.cfg руками"
    fi
else
    log "grub пропущен: загрузчик ставится отдельно, см. README"
fi

log "отмонтирую"
umount -R "$MNT/dev" 2>/dev/null || umount -l -R "$MNT/dev" || true
umount -R "$MNT/sys" 2>/dev/null || umount -l -R "$MNT/sys" || true
umount "$MNT/proc" 2>/dev/null || umount -l "$MNT/proc" || true
[ "$MODE" = "efi" ] && umount "$MNT/boot/efi" 2>/dev/null || true
umount "$MNT" 2>/dev/null || umount -l "$MNT" || true

log "готово. далее:"
log "  1) задай пароль root: chroot $MNT passwd (если пропустил)"
if ls "$MNT"/boot/vmlinuz-* >/dev/null 2>&1; then
    log "  2) ядро уже в stage, grub.cfg записан — можно перезагружаться"
else
    log "  2) собери ядро: chroot $MNT flexura install linux"
fi
log "  3) перезагрузка: reboot"
