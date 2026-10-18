#!/bin/bash
# Build a Debian rootfs for the selected device using debootstrap.
#
# Device: env DEVICE (default: the only one under devices/).
# Requires root (re-execs itself through sudo).
set -euo pipefail
. "$(dirname "$0")/lib.sh"

UI="${UI:-sway}"
[ -d "$UI_DIR" ] || die "unknown UI '$UI' (see $UIS_DIR)"
TARGET_HOSTNAME="${TARGET_HOSTNAME:-$(echo "$DEVICE" | sed 's/.*-//')}"
USERNAME="${USERNAME:-brunux}"
USERPASS="${USERPASS:-debian}"

if [ "$(id -u)" -ne 0 ]; then
	# Forward every resolved build input: sudo does not preserve the caller's
	# environment, so an override passed to this script (USERNAME, TIMEZONE, …)
	# would otherwise be silently dropped and replaced by the default.
	exec sudo -n \
		BUILD_ROOT="$BUILD_ROOT" WORK_DIR="$WORK_DIR" SRC_DIR="$SRC_DIR" \
		DEVICE="$DEVICE" UI="$UI" WIFI_DIR="$WIFI_DIR" \
		LOCALE="$LOCALE" TIMEZONE="$TIMEZONE" \
		TARGET_HOSTNAME="$TARGET_HOSTNAME" USERNAME="$USERNAME" USERPASS="$USERPASS" \
		DEBIAN_SUITE="$DEBIAN_SUITE" DEBIAN_MIRROR="$DEBIAN_MIRROR" \
		DEBIAN_COMPONENTS="$DEBIAN_COMPONENTS" ROOTFS="$ROOTFS" \
		"$(readlink -f "$0")" "$@"
fi

cleanup_mounts() {
	for m in dev proc sys; do
		umount -lf "$ROOTFS/$m" 2>/dev/null || true
	done
}
trap cleanup_mounts EXIT

log "Device : $DEVICE ($DEVICE_ARCH)"
log "Kernel : $KERNEL_FLAVOR $KERNEL_RELEASE"
log "Rootfs : $ROOTFS"

log "debootstrap $DEBIAN_SUITE/$DEBIAN_ARCH"
rm -rf "$ROOTFS"
mkdir -p "$ROOTFS"
debootstrap --arch="$DEBIAN_ARCH" --variant=minbase \
	--components="$(echo $DEBIAN_COMPONENTS | tr ' ' ',')" \
	"$DEBIAN_SUITE" "$ROOTFS" "$DEBIAN_MIRROR"

log "Mounting virtual filesystems"
mount --bind /dev "$ROOTFS/dev"
mount -t proc proc "$ROOTFS/proc"
mount -t sysfs sys "$ROOTFS/sys"
cp /etc/resolv.conf "$ROOTFS/etc/resolv.conf"

install -Dm755 "$REPO_DIR/scripts/rootfs-setup.sh" "$ROOTFS/root/rootfs-setup.sh"
rm -rf "$ROOTFS/tmp/packages"
mkdir -p "$ROOTFS/tmp/packages"
cp "$PACKAGES_DIR"/*.list "$ROOTFS/tmp/packages/"
[ -f "$DEVICE_DIR/packages.list" ] && cp "$DEVICE_DIR/packages.list" "$ROOTFS/tmp/packages/device.list"
# The UI package list is staged under the name rootfs-setup.sh expects.
cp "$UI_DIR/packages.list" "$ROOTFS/tmp/packages/$UI.list"
# The UI file tree is staged aside; rootfs-setup.sh decides where it goes.
# The staged copy is normalised to root:root because `cp -a` would otherwise
# carry the host owner (uid 10000 on postmarketOS hosts) into the image.
rm -rf "$ROOTFS/tmp/ui-files"
if [ -d "$UI_DIR/files" ]; then
	cp -a "$UI_DIR/files" "$ROOTFS/tmp/ui-files"
	chown -R root:root "$ROOTFS/tmp/ui-files"
fi
# A UI may ship an optional setup.sh, run by rootfs-setup.sh inside the chroot
# after its files are overlaid. It exists for work that cannot be expressed as a
# plain file tree: seeding ~/.config from a vendored tree, or creating symlink
# farms, without duplicating those files in files/.
rm -f "$ROOTFS/tmp/ui-setup.sh"
[ -f "$UI_DIR/setup.sh" ] && install -Dm755 "$UI_DIR/setup.sh" "$ROOTFS/tmp/ui-setup.sh"
# The image carries this repository, because the scripts that partition the
# internal storage, rewrite the ChromeOS kernel partition and report on the
# system are all meant to run *on* the device (scripts/install-emmc.sh,
# scripts/install-kpart.sh, scripts/check-system.sh).  What is staged is the
# committed tree (git archive HEAD), so the image never carries an untested
# working copy; rootfs-setup.sh decides its final home.
command -v git >/dev/null || die "git is needed to stage the repository into the image"
# This script runs as root while the repository belongs to the invoking user, so
# git needs the checkout whitelisted (safe.directory); without it every command
# below dies with "detected dubious ownership in repository".
GIT="git -c safe.directory=$REPO_DIR -C $REPO_DIR"
REPO_REVISION="$($GIT rev-parse --short HEAD)"
if ! $GIT diff-index --quiet HEAD -- 2>/dev/null; then
	warn "the working tree has uncommitted changes: the image gets HEAD ($REPO_REVISION), not them"
fi
rm -rf "$ROOTFS/tmp/repo"
mkdir -p "$ROOTFS/tmp/repo"
$GIT archive HEAD | tar -x -C "$ROOTFS/tmp/repo"
# git archive stores 0664/0775, and tar restores the stored mode verbatim when it
# runs as root, so normalize: directories and scripts 755, everything else 644.
chmod -R u=rwX,go=rX "$ROOTFS/tmp/repo"
# Which revision an image was built from, so a copy found on a card can be told
# apart from the checkout it came from.
{
	printf 'commit %s\n' "$REPO_REVISION"
	printf 'device %s\n' "$DEVICE"
	printf 'ui %s\n' "$UI"
	printf 'kernel %s\n' "$KERNEL_RELEASE"
	printf 'built %s\n' "$(date -Is)"
} >"$ROOTFS/etc/debian-aarch64-revision"

log "Running rootfs-setup.sh in chroot"
chroot "$ROOTFS" env \
	HOSTNAME="$TARGET_HOSTNAME" \
	USERNAME="$USERNAME" \
	USERPASS="$USERPASS" \
	DEBIAN_SUITE="$DEBIAN_SUITE" \
	DEBIAN_MIRROR="$DEBIAN_MIRROR" \
	DEBIAN_COMPONENTS="$DEBIAN_COMPONENTS" \
	LOCALE="$LOCALE" \
	TIMEZONE="$TIMEZONE" \
	UI="$UI" \
	REPO_NAME="$(basename "$REPO_DIR")" \
	/bin/bash /root/rootfs-setup.sh

if [ -d "$WIFI_DIR" ]; then
	log "Installing Wi-Fi profiles from $WIFI_DIR"
	"$REPO_DIR/scripts/wifi.sh" import "$WIFI_DIR" "$ROOTFS"
fi

rm -rf "$ROOTFS/tmp/packages" "$ROOTFS/tmp/ui-files" "$ROOTFS/tmp/ui-setup.sh" \
	"$ROOTFS/tmp/repo" "$ROOTFS/root/rootfs-setup.sh"

# Whatever is built here ships as a disk (write-sd.sh, make-image.sh) or is
# cloned onto one (install-emmc.sh), so it has to leave as a first boot: from an
# empty /etc/machine-id systemd generates a fresh id and the sshd-keygen
# drop-in (scripts/rootfs-setup.sh) the host keys. This is the same choice
# lib-dist.sh makes for published artifacts, for the same reasons. Left as the
# build left it, every card and every clone shares one identity - one journal,
# one dhcp identity - and ssh.service dies with "no hostkeys available".
: >"$ROOTFS/etc/machine-id"
rm -f "$ROOTFS"/etc/ssh/ssh_host_*

log "Rootfs ready: $ROOTFS"
