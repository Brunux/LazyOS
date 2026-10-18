#!/bin/bash
# Build every asset of one GitHub release into a directory:
#
#   <device>.img             the disk image (make-image.sh)
#   <device>.img.xz          the same image, compressed for download
#   <device>-rootfs.tar.zst  the root filesystem as a tarball
#   <device>.vmlinuz.kpart   the kernel partition that matches the image
#   SHA256SUMS               hashes of the four above
#
# Usage: make-release.sh [OUTDIR]     (default $WORK_DIR/release-<device>)
#
# The release notes are not generated here: they are written by hand, kept in
# docs/releases/ so they are versioned like everything else, and reused for the
# annotated tag and for the GitHub release.
#
# Three things are worth knowing about the assets:
#
#   * Everything that is published is sanitized and asserted clean
#     (scripts/lib-dist.sh): a released image must not carry the maintainer's
#     Wi-Fi PSKs, host keys or machine id.
#   * The kpart embeds `root=UUID=...`, so it is the kernel partition of *this*
#     image and nothing else. It is shipped because it makes p1 reproducible from
#     the release alone; anyone writing their own card gets a kpart that matches
#     their partitions from scripts/write-sd.sh or scripts/install-kpart.sh,
#     which build it from the disk they are writing to.
#   * The tarball is the root filesystem with /boot inside it (the layout mounts
#     the boot partition there), for writing a card by hand:
#       sudo ROOTFS=<extracted> scripts/write-sd.sh <device>
#     which generates the fstab and the kpart for those partitions. Its own
#     /etc/fstab is the unconfigured one the build leaves behind.
set -euo pipefail
. "$(dirname "$0")/lib.sh"
. "$(dirname "$0")/lib-cgpt.sh"
. "$(dirname "$0")/lib-dist.sh"

OUTDIR="${1:-$WORK_DIR/release-$DEVICE}"
IMAGE_SIZE_MIB="${IMAGE_SIZE_MIB:-4096}"
export IMAGE_SIZE_MIB

# build-rootfs.sh documents the same pitfall: sudo drops the environment, so
# every resolved input is forwarded explicitly.
if [ "$(id -u)" -ne 0 ]; then
	exec sudo -n DEVICE="$DEVICE" IMAGE_SIZE_MIB="$IMAGE_SIZE_MIB" "$(readlink -f "$0")" "$OUTDIR"
fi

IMG="$OUTDIR/$DEVICE.img"
TARBALL="$OUTDIR/$DEVICE-rootfs.tar.zst"
KPART_OUT="$OUTDIR/$DEVICE.vmlinuz.kpart"
STAGE="$WORK_DIR/release-stage-$DEVICE"

mkdir -p "$OUTDIR"
[ -d "$ROOTFS" ] || die "rootfs not found: $ROOTFS (run build-rootfs.sh first)"

log "1/4 Image"
"$REPO_DIR/scripts/make-image.sh" "$IMG" "$IMAGE_SIZE_MIB"

log "2/4 Compressing the image"
xz -T0 -6 -f -k "$IMG"

log "3/4 Root filesystem tarball"
rm -rf "$STAGE"
mkdir -p "$STAGE"
# Same exclusion as the image: the profiles must not be copied and then deleted,
# or their bytes survive in the staging filesystem (scripts/lib-dist.sh).
# A leftover mount under the rootfs is refused for the same reason it is in
# make-image.sh: rsync would copy the live host's pseudo-filesystems.
_stale_mounts="$(findmnt -rn -o TARGET 2>/dev/null | grep -F "$ROOTFS/" || true)"
[ -z "$_stale_mounts" ] ||
	die "something is mounted under $ROOTFS ($_stale_mounts); unmount it first"
EXCLUDES="$WORK_DIR/dist-excludes-$DEVICE.txt"
dist_write_excludes "$EXCLUDES"
rsync -aHAX --delete --exclude-from="$EXCLUDES" "$ROOTFS"/ "$STAGE"/
dist_sanitize "$STAGE"
dist_assert_clean "$STAGE"
dist_assert_no_profile_bytes "$STAGE"
# --acls/--xattrs/--xattrs-include keep what rsync -A/-X preserved, and
# --numeric-owner because the tarball is extracted as root, where the numbers
# are what count.
tar --acls --xattrs --xattrs-include='*' --numeric-owner -cpf - -C "$STAGE" . |
	zstd -T0 -12 -q -f -o "$TARBALL"
rm -rf "$STAGE"

log "4/4 kpart of the released image"
# Read the image's root UUID back instead of assuming it: the kpart must embed
# exactly the UUID the image boots with.
LOOP="$(losetup --show -f -P "$IMG")"
ROOT_UUID_IMG="$(blkid -s UUID -o value "${LOOP}p3" 2>/dev/null || true)"
losetup -d "$LOOP"
[ -n "$ROOT_UUID_IMG" ] || die "could not read the image's root UUID back from $IMG"
"$REPO_DIR/scripts/make-kpart.sh" "$ROOTFS" "$KPART_OUT" "root=UUID=$ROOT_UUID_IMG rootwait rw"
kpart_verify "$KPART_OUT"

log "Checksums"
(
	cd "$OUTDIR"
	sha256sum "$(basename "$IMG")" "$(basename "$IMG").xz" "$(basename "$TARBALL")" \
		"$(basename "$KPART_OUT")" >SHA256SUMS
	cat SHA256SUMS
) | sed 's/^/  /'

log "Assets in $OUTDIR"
ls -lh "$OUTDIR" | sed 's/^/  /'
