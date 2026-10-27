#!/bin/bash
# Install the session framework binary into a root filesystem (or `/`).
#
# Usage:
#   install-lazyos.sh [ROOTFS]
#
# Without an argument the rootfs built by build-rootfs.sh is used;
# `install-lazyos.sh /` installs it on the running system, which is the
# counterpart of install-kpart.sh when the binary changes.
#
# Two things land: /usr/bin/lazyos and the symlinks /usr/bin/lazyos-menu and
# /usr/bin/lazyos-task -> lazyos. The links are the "migrated set"
# check-system.sh validates: a name whose `check-system.sh` line must resolve,
# and whose command must be in `--help`.
# The ~64 old names under ~/.config/bin join it one spec at a time, because each
# one replaces a script that still has to work until its surface is ported.
set -euo pipefail
. "$(dirname "$0")/lib.sh"

[ -n "${1:-}" ] && ROOTFS="$1"
ROOTFS="$(readlink -f "$ROOTFS" 2>/dev/null || printf '%s' "$ROOTFS")"

[ -x "$LAZYOS_STAGE/lazyos" ] ||
	die "no lazyos binary in $LAZYOS_STAGE (run build-lazyos.sh first)"
[ -d "$ROOTFS/usr/bin" ] || die "not a root filesystem: $ROOTFS/usr/bin is missing"

if [ "$(id -u)" -ne 0 ]; then
	exec sudo -n \
		BUILD_ROOT="$BUILD_ROOT" WORK_DIR="$WORK_DIR" SRC_DIR="$SRC_DIR" \
		DEVICE="$DEVICE" ROOTFS="$ROOTFS" \
		"$(readlink -f "$0")"
fi

if [ "$ROOTFS" = "/" ]; then
	log "Installing lazyos on the running system"
else
	log "Installing lazyos into $ROOTFS"
fi

install -m 0755 "$LAZYOS_STAGE/lazyos" "$ROOTFS/usr/bin/lazyos"
ln -sfn lazyos "$ROOTFS/usr/bin/lazyos-menu"
ln -sfn lazyos "$ROOTFS/usr/bin/lazyos-task"

# Read back what was written: a binary that does not answer is a broken copy,
# and the version is what check-system.sh compares with the image's record.
REPORTED="$("$ROOTFS/usr/bin/lazyos" --version 2>/dev/null || true)"
[ "$REPORTED" = "$(cat "$REPO_DIR/VERSION")" ] ||
	die "$ROOTFS/usr/bin/lazyos does not report $(cat "$REPO_DIR/VERSION") (got '$REPORTED')"
[ -x "$ROOTFS/usr/bin/lazyos-menu" ] ||
	die "$ROOTFS/usr/bin/lazyos-menu does not resolve to the binary"
[ -x "$ROOTFS/usr/bin/lazyos-task" ] ||
	die "$ROOTFS/usr/bin/lazyos-task does not resolve to the binary"

log "lazyos $REPORTED installed; its menu opens with 'lazyos menu' from the session"
