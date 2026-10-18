#!/bin/bash
# Partition and populate a microSD/USB device so it boots via depthcharge (pick
# the external disk in the firmware's developer-mode menu).
#
# The layout is Nura's (see scripts/lib-cgpt.sh):
#   p1  kernel  128 MiB  signed FIT kpart
#   p2  boot    512 MiB  kernel, initramfs, DTBs
#   p3  root    the rest Debian root filesystem
#
# Usage: write-sd.sh [DEVICE]     (default: /dev/mmcblk1)
set -euo pipefail
. "$(dirname "$0")/lib.sh"
. "$(dirname "$0")/lib-cgpt.sh"

BLOCK="${1:-/dev/mmcblk1}"

if [ "$(id -u)" -ne 0 ]; then
	exec sudo -n DEVICE="$DEVICE" "$(readlink -f "$0")" "${1:-}"
fi

[ -b "$BLOCK" ] || die "not a block device: $BLOCK"
[ -d "$ROOTFS" ] || die "rootfs not found: $ROOTFS (run build-rootfs.sh first)"

# Safety: never touch the internal eMMC or the running root device.
ROOT_SRC="$(findmnt -n -o SOURCE / 2>/dev/null || true)"
case "$BLOCK" in
	*/mmcblk0|*/mmcblk0p*) die "refusing to write to internal eMMC: $BLOCK" ;;
esac
case "$ROOT_SRC" in
	"$BLOCK"|"$BLOCK"p*|"$BLOCK"[0-9]*) die "refusing to write to the running root device: $BLOCK" ;;
esac

log "Device: $DEVICE -> $BLOCK ($(blockdev --getsize64 "$BLOCK") bytes)"
disk_release "$BLOCK"

log "Creating ChromeOS GPT on $BLOCK (kernel $((KPART_SIZE_SECTORS * 512 / 1024 / 1024)) MiB, boot ${BOOT_SIZE_MIB} MiB)"
disk_layout_create "$BLOCK"
disk_layout_parts "$BLOCK"

MNT="$WORK_DIR/sd-mnt-$DEVICE"
disk_layout_mount "$BLOCK" "$MNT"

log "Copying root filesystem"
# A leftover mount under the rootfs (a chroot /proc or /sys someone forgot to
# unmount) is never wanted: rsync would copy the live host's pseudo-filesystems
# and fail on their unreadable entries - which is exactly what aborted a card
# write once - and on a slower medium it could write some of them first.
_stale_mounts="$(findmnt -rn -o TARGET 2>/dev/null | grep -F "$ROOTFS/" || true)"
[ -z "$_stale_mounts" ] ||
	die "something is mounted under $ROOTFS ($_stale_mounts); unmount it first"
rsync -aHAX --delete "$ROOTFS"/ "$MNT"/
disk_layout_fstab "$MNT"

log "Building kpart (root=UUID=$ROOT_UUID)"
"$REPO_DIR/scripts/make-kpart.sh" "$MNT" "$KPART" "root=UUID=$ROOT_UUID rootwait rw"
kpart_fits "$KPART" "$KPART_PART"
kpart_verify "$KPART"

log "Writing kpart to $KPART_PART"
dd if="$KPART" of="$KPART_PART" bs=1M conv=fsync
sync
disk_layout_umount "$MNT"

log "Done. Remove $BLOCK, insert it and pick the external disk in the firmware menu."
cgpt show "$BLOCK" || true