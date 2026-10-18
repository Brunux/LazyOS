#!/bin/bash
# Build a distributable disk image from the built rootfs, without a card.
#
# Usage: make-image.sh [OUTPUT.img] [SIZE_MIB]
#
#   OUTPUT   destination, default $WORK_DIR/<device>.img
#   SIZE_MIB total size, default $IMAGE_SIZE_MIB or 4096
#
# The image carries the same ChromeOS GPT and the same signed kpart as
# scripts/write-sd.sh (both come from scripts/lib-cgpt.sh), so flashing it is
# plain dd:
#
#   xz -dc nureb-<device>.img.xz | sudo dd of=/dev/mmcblk1 bs=4M conv=fsync
#
# and the card boots from the firmware's external-disk entry. Two things differ
# from write-sd.sh, and both are about the file being *published*:
#
#   * the size is fixed by argument instead of taken from a device, so the same
#     build can be written to any card that is at least this big;
#   * the copy is sanitized (scripts/lib-dist.sh) and asserted clean, because
#     the build rootfs deliberately knows the maintainer's Wi-Fi networks.
#
# A card larger than the image keeps the unused space: growing the root
# partition is `cgpt add -i 3 -s <sectors>` plus a resize2fs, documented in the
# release notes. This script does not do it, because it does not know the card.
set -euo pipefail
. "$(dirname "$0")/lib.sh"
. "$(dirname "$0")/lib-cgpt.sh"
. "$(dirname "$0")/lib-dist.sh"

IMG="${1:-$WORK_DIR/$DEVICE.img}"
SIZE_MIB="${2:-${IMAGE_SIZE_MIB:-4096}}"
KPART_IMG="$WORK_DIR/kpart-image-$DEVICE.img"

if [ "$(id -u)" -ne 0 ]; then
	exec sudo -n DEVICE="$DEVICE" "$(readlink -f "$0")" "$IMG" "$SIZE_MIB"
fi

[ -d "$ROOTFS" ] || die "rootfs not found: $ROOTFS (run build-rootfs.sh first)"
[ -f "$ROOTFS/boot/vmlinuz-$KERNEL_RELEASE" ] ||
	die "the rootfs has no /boot/vmlinuz-$KERNEL_RELEASE: run install-kernel.sh first"

log "Image: $IMG ($SIZE_MIB MiB, device=$DEVICE)"
rm -f "$IMG"
truncate -s "${SIZE_MIB}M" "$IMG"

# Populate through a loop device, so the layout code (which is written against a
# disk and its /dev/<disk>p<n> partitions) is reused unchanged instead of being
# reimplemented with partition offsets.
MNT="$WORK_DIR/image-mnt-$DEVICE"
LOOP="$(losetup --show -f -P "$IMG")"
trap 'disk_layout_umount "$MNT" 2>/dev/null || true; losetup -d "$LOOP" 2>/dev/null || true' EXIT
log "Loop device: $LOOP"

disk_release "$LOOP"
disk_layout_create "$LOOP"
disk_layout_parts "$LOOP"
disk_layout_mount "$LOOP" "$MNT"

# The maintainer's state is excluded from the copy, not deleted after it: a
# deleted file is still in the image's free blocks (scripts/lib-dist.sh).
log "Copying root filesystem"
# A leftover mount under the rootfs (e.g. a chroot /proc or /sys someone forgot
# to unmount) is never wanted: rsync would copy the live host's pseudo-filesystems
# and fail on their unreadable entries (hit while writing a card). Refuse.
_stale_mounts="$(findmnt -rn -o TARGET 2>/dev/null | grep -F "$ROOTFS/" || true)"
[ -z "$_stale_mounts" ] ||
	die "something is mounted under $ROOTFS ($_stale_mounts); unmount it first"
EXCLUDES="$WORK_DIR/dist-excludes-$DEVICE.txt"
dist_write_excludes "$EXCLUDES"
rsync -aHAX --delete --exclude-from="$EXCLUDES" "$ROOTFS"/ "$MNT"/
disk_layout_fstab "$MNT"
dist_sanitize "$MNT"
dist_assert_clean "$MNT"

CMDLINE_EXTRA="root=UUID=$ROOT_UUID rootwait rw"
log "Building kpart (cmdline: $KERNEL_CMDLINE$CMDLINE_EXTRA)"
"$REPO_DIR/scripts/make-kpart.sh" "$MNT" "$KPART_IMG" "$CMDLINE_EXTRA"
kpart_fits "$KPART_IMG" "$KPART_PART"
kpart_verify "$KPART_IMG"

log "Writing kpart to $KPART_PART"
dd if="$KPART_IMG" of="$KPART_PART" bs=1M conv=fsync
sync
disk_layout_umount "$MNT"
losetup -d "$LOOP"
trap - EXIT
# The filesystem being clean is not enough: the image is what gets published.
dist_assert_no_profile_bytes "$IMG"

log "Done: $IMG ($(du -h "$IMG" | cut -f1) of data, $(du -h --apparent-size "$IMG" | cut -f1) apparent)"
log "Flash it with: xz -dc ${IMG##*/}.xz | sudo dd of=<card> bs=4M conv=fsync"
cgpt show "$IMG" || true
