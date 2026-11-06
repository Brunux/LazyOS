#!/bin/bash
# What a *published* artifact must not carry, shared by the scripts that produce
# one (make-image.sh, make-release.sh).
#
# Sourced after lib.sh, which provides die()/log()/warn() and $ROOTFS/$USERNAME.
#
# The build rootfs keeps state that belongs to the maintainer's machine on
# purpose: build-rootfs.sh imports the Wi-Fi profiles from $WIFI_DIR so the
# maintainer's own cards come up already knowing the networks. That state must
# never leave the machine it was built on:
#
#   * /etc/NetworkManager/system-connections/*.nmconnection holds the Wi-Fi PSK
#     in clear. Shipping one publishes the maintainer's Wi-Fi password.
#   * /var/lib/NetworkManager is the state NetworkManager built from them.
#   * /etc/ssh/ssh_host_* makes every installation of the image share one host
#     identity, so it stops identifying a machine at all.
#   * /etc/machine-id and /var/lib/dbus/machine-id are the same problem for
#     systemd, DHCP leases and journal files.
#   * the shipped agent's state: /home/*/.local/share/opencode holds OpenCode's
#     database, and its credential table holds the provider sign-ins of whoever
#     ran it. The image ships OpenCode signed out (tools/opencode/), so a release
#     built from a tree that has been used must not ship it signed in.
#   * the build root itself ($HOME/.cache/debian-aarch64) contains the exported
#     Wi-Fi profiles, so a copy of it inside an artifact leaks them too.
#
# Two consequences shape this file:
#
# 1. Removing the state afterwards is NOT enough. Deleting a file inside an image
#    leaves its data in the filesystem's free blocks, and that is measurable: the
#    first v0.1.0 image had no *.nmconnection anywhere, `find` over the mounted
#    root found nothing, and `grep -a` over the .img still printed both SSIDs and
#    the PSK. So these paths are *excluded from the copy* (DIST_EXCLUDES) and the
#    finished artifact is searched for their contents
#    (dist_assert_no_profile_bytes), which is the check that would have caught
#    it.
# 2. /etc/machine-id is left empty rather than absent: systemd-machine-id-setup
#    generates a fresh one on first boot. The SSH host keys are simply not copied
#    because the image ships sshd-keygen.service (wanted by ssh.service), which
#    generates the ones it is missing.

# rsync patterns, anchored at the transfer root. Also excludes the build
# leftovers that would otherwise be deleted after being copied (see above).
DIST_EXCLUDES='
/etc/NetworkManager/system-connections/
/var/lib/NetworkManager/
/etc/ssh/ssh_host_*
/etc/machine-id
/var/lib/dbus/machine-id
/root/.ssh/
/home/*/.cache/debian-aarch64/
/home/*/.local/share/opencode/
/home/*/.local/state/opencode/
/home/*/.cache/opencode/
/home/*/.config/opencode/
/var/log/bootstrap.log
/var/cache/apt/archives/*.deb
/var/cache/apt/archives/partial/
/tmp/repo/
'

# Write the patterns where rsync can read them: rsync -aHAX --delete
# --exclude-from=<file> <rootfs>/ <destination>/
dist_write_excludes() {
	[ -n "${1:-}" ] || die "dist_write_excludes: no destination file given"
	printf '%s\n' "$DIST_EXCLUDES" | sed '/^$/d' >"$1"
}

# Make sure the state above is absent from a rootfs tree (an image mount or a
# staging tree), for callers that did not use DIST_EXCLUDES. Destructive by
# design: it runs on a copy, never on $ROOTFS.
dist_sanitize() {
	_d="${1%/}"
	[ -d "$_d/etc" ] || die "dist_sanitize: $_d does not look like a root filesystem"

	log "Sanitizing $_d for publication"
	rm -rf "$_d/etc/NetworkManager/system-connections"
	rm -rf "${_d:?}"/var/lib/NetworkManager
	# NetworkManager and dbus recreate these; keeping them (empty, 0700) leaves
	# an image that looks like a normal system.
	install -d -m700 -o root -g root "$_d/etc/NetworkManager/system-connections"
	install -d -m700 -o root -g root "$_d/var/lib/NetworkManager"
	rm -rf "${_d:?}"/home/*/.cache/debian-aarch64
	# The shipped agent's state, sign-ins included (see the header).
	rm -rf "${_d:?}"/home/*/.local/share/opencode "${_d:?}"/home/*/.local/state/opencode
	rm -rf "${_d:?}"/home/*/.cache/opencode "${_d:?}"/home/*/.config/opencode

	rm -f "$_d"/etc/ssh/ssh_host_*
	: >"$_d/etc/machine-id"
	rm -f "$_d/var/lib/dbus/machine-id"

	rm -rf "$_d/root/.ssh"
	rm -rf "${_d:?}"/var/log/journal/*
	rm -f "$_d/var/log/bootstrap.log"
	rm -rf "$_d/tmp/repo"
}

# Refuse to publish <dir> if any of the state above is still in it, as files.
dist_assert_clean() {
	_d="${1%/}"
	_bad=""

	_bad="$_bad$(find "$_d" -xdev \( -name '*.nmconnection' -o -name '*.lease' \) -print 2>/dev/null | head -5)"
	_bad="$_bad$(find "$_d/etc/ssh" -name 'ssh_host_*' -print 2>/dev/null | head -3)"
	if [ -s "$_d/etc/machine-id" ]; then
		_bad="$_bad$_d/etc/machine-id (not empty)"
	fi
	if [ -e "$_d/var/lib/dbus/machine-id" ]; then
		_bad="$_bad$_d/var/lib/dbus/machine-id"
	fi
	if find "$_d/root/.ssh" "$_d"/home/*/.cache/debian-aarch64 -mindepth 1 -print -quit 2>/dev/null | grep -q .; then
		_bad="$_bad$_d/root/.ssh or $_d/home/*/.cache/debian-aarch64 (not empty)"
	fi
	# The agent the image ships keeps its sign-ins in a database; an artifact that
	# still has one was made from a used tree.  `|| true` is load-bearing: find
	# exits 1 for a path that does not exist, which is exactly the state
	# dist_sanitize leaves this one in, and with the message discarded by
	# 2>/dev/null and `set -e` in the callers the assignment would end the run
	# with no output at all (hit cutting v0.4.1).
	_oc_left="$(find "$_d"/home/*/.local/share/opencode -mindepth 1 -print -quit 2>/dev/null || true)"
	if [ -n "$_oc_left" ]; then
		_bad="$_bad$_d/home/*/.local/share/opencode (not empty)"
	fi

	if [ -n "$_bad" ]; then
		printf '%s\n' "$_bad" | sed 's/^/  /' >&2
		die "refusing to publish: $_d still carries state that is not meant to leave this machine"
	fi
	log "Clean: no Wi-Fi profile, host key or machine id in $_d"
}

# Search the artifact itself (an image file, a staging tree) for the *contents*
# of the maintainer's profiles: their SSIDs and their PSKs, one pattern per line.
# The patterns come from the build rootfs, which is where those profiles live -
# the artifact is not supposed to contain them at all, in a file or in the free
# space of a filesystem.
dist_assert_no_profile_bytes() {
	_d="${1%/}"
	[ -e "$_d" ] || die "dist_assert_no_profile_bytes: $_d does not exist"
	# The profiles live in a 0700 root-only directory, so as an ordinary user the
	# glob below matches nothing and the check would pass without looking at
	# anything. Refuse the ambiguous case instead of trusting it.
	if [ -d "$ROOTFS/etc/NetworkManager/system-connections" ] &&
		! ls -A "$ROOTFS/etc/NetworkManager/system-connections" >/dev/null 2>&1; then
		die "dist_assert_no_profile_bytes: cannot read $ROOTFS/etc/NetworkManager/system-connections (run as root), so the check would be empty"
	fi
	_pat="$WORK_DIR/dist-profile-patterns.txt"
	: >"$_pat"
	for _p in "$ROOTFS"/etc/NetworkManager/system-connections/*.nmconnection; do
		[ -f "$_p" ] || continue
		basename "$_p" .nmconnection >>"$_pat"
		sed -n 's/^psk=//p' "$_p" >>"$_pat"
	done
	sed -i '/^$/d' "$_pat"

	if [ ! -s "$_pat" ]; then
		log "No Wi-Fi profile in $ROOTFS: nothing to look for in $_d"
		rm -f "$_pat"
		return 0
	fi
	# -l: report the file that matched and stop, not the matching bytes (the
	# artifact is 4 GiB of mostly zeros).
	_hits="$(grep -raF -l -f "$_pat" "$_d" 2>/dev/null | head -3 || true)"
	rm -f "$_pat"
	if [ -n "$_hits" ]; then
		printf '%s\n' "$_hits" | sed 's/^/  /' >&2
		die "$_d still contains Wi-Fi credentials: deleting a profile after copying it leaves its bytes in the image"
	fi
	log "Byte-checked: no SSID and no PSK anywhere in $_d"
}
