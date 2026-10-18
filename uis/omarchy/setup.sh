#!/bin/bash
# uis/omarchy/setup.sh — runs inside the chroot, as root, after uis/omarchy/files
# has been overlaid. This is the Debian counterpart of the Omarchy installer's
# config/user steps, reduced to what a Debian base actually needs:
#
#   * the omarchy-* commands on PATH, which upstream's Arch package provides as
#     /usr/bin/omarchy-* but a checkout keeps under $OMARCHY_PATH/bin;
#   * ~/.config seeded from the vendored tree, which is what omarchy-refresh-config
#     and the install scripts do on Arch.
#
# It deliberately does not run upstream's install/ scripts: those are pacman,
# SDDM, LUKS and platform provisioning for Arch (docs/plan-omarchy.md).
set -euo pipefail

: "${USERNAME:=brunux}"
HOME="/home/$USERNAME"
OMARCHY_PATH=/usr/share/omarchy

log() { printf '\n\033[1;34m[omarchy] ==> %s\033[0m\n' "$*"; }
warn() { printf '\n\033[1;33m[omarchy] WARN: %s\033[0m\n' "$*" >&2; }

[ -d "$OMARCHY_PATH/bin" ] || { warn "$OMARCHY_PATH/bin is missing, nothing to do"; exit 0; }

# Arch-only command families left off PATH: package management and updates,
# the installer/provisioner, AUR/web-app/gaming/AI installers, and the helpers
# that call pacman, yay or systemd units Debian does not have. The desktop does
# not offer features it cannot perform here; the rest of bin/ is linked.
#
# An array, matched with [[ ]], because a multi-line string in a `case` pattern
# list is not word-split by bash and would silently match nothing.
EXCLUDE=(
	'omarchy-update*'
	'omarchy-pkg-*'
	'omarchy-install-*'
	'omarchy-remove-*'
	'omarchy-migrate*'
	'omarchy-upgrade-to-quattro'
	'omarchy-reinstall*'
	'omarchy-dev-*'
	'omarchy-refresh-pacman'
	'omarchy-refresh-limine'
	'omarchy-refresh-plymouth'
	'omarchy-refresh-sddm'
	'omarchy-plymouth-*'
	'omarchy-snapshot'
	'omarchy-boot-direct'
	'omarchy-setup-direct-boot'
	'omarchy-mise-*'
	'omarchy-agent*'
	'omarchy-voxtype*'
	'omarchy-dictation*'
	'omarchy-tailscale-*'
	'omarchy-openclaw*'
	'omarchy-webapp*'
	'omarchy-games-*'
	'omarchy-drive-*'
	'omarchy-hibernation-*'
	'omarchy-cmd-*'
	'omarchy-security-*'
	'omarchy-setup-security-*'
	'omarchy-remove-security-*'
	'omarchy-tui-*'
	'omarchy-locale-generate'
	'omarchy-sudo-docker'
	'omarchy-sudo-keepalive'
	'omarchy-sudo-passwordless'
	'omarchy-windows-*'
	'omarchy-upload-log'
	'omarchy-transcode'
	'omarchy-transcode-ascii'
	'omarchy-capture-webcam-list'
	'omarchy-capture-webcam-resize'
	'omarchy-apply-hardware'
	'omarchy-apply-lock'
	'omarchy-apply-system'
	'omarchy-apply-n1x-gpu-memory'
	'omarchy-gpu-screen-recorder'
	'omarchy-provision-*'
	'omarchy-system-factory-reset'
	'omarchy-system-factory-reset-finish'
	'omarchy-n1x-probe'
	'omarchy-cliamp'
)

log "Putting the omarchy-* commands on PATH (/usr/bin)"
linked=0
skipped=0
for f in "$OMARCHY_PATH"/bin/*; do
	[ -f "$f" ] || continue
	n="${f##*/}"
	skip=0
	for pat in "${EXCLUDE[@]}"; do
		# shellcheck disable=SC2254  # the RHS is meant to be a glob
		if [[ $n == $pat ]]; then skip=1; break; fi
	done
	if [ "$skip" = 1 ]; then
		skipped=$((skipped + 1))
		continue
	fi
	ln -sf "$f" "/usr/bin/$n"
	linked=$((linked + 1))
done
log "  $linked command(s) linked, $skipped Arch-only command(s) left out"

log "Seeding ~/.config from $OMARCHY_PATH"
install -d -m 0755 -o "$USERNAME" -g "$USERNAME" "$HOME/.config"

# config/ is the packaged user configuration; omarchy-refresh-config copies it
# file by file, so the initial seed is the same tree. The Arch-only autostart
# entry is dropped (there is no limine/snapper here).
if [ -d "$OMARCHY_PATH/config" ]; then
	for entry in "$OMARCHY_PATH"/config/*; do
		[ -e "$entry" ] || continue
		cp -a "$entry" "$HOME/.config/"
	done
	rm -f "$HOME/.config/autostart/limine-snapper-notify.desktop"
fi

# The parts of default/ that seed ~/.config (the rest is read by path at runtime,
# e.g. default/hypr/bootstrap.lua).
for entry in omarchy uwsm environment.d; do
	[ -d "$OMARCHY_PATH/default/$entry" ] || continue
	install -d -m 0755 -o "$USERNAME" -g "$USERNAME" "$HOME/.config/$entry"
	cp -a "$OMARCHY_PATH/default/$entry/." "$HOME/.config/$entry/"
done

chown -R "$USERNAME:$USERNAME" "$HOME/.config"

# UWSM is what starts the session, and it sources /usr/share/uwsm/env.d, so the
# Omarchy env bootstrap lives there rather than only in ~/.config.
if [ -f "$OMARCHY_PATH/default/uwsm/env.d/10-omarchy" ]; then
	install -d -m 0755 /usr/share/uwsm/env.d
	install -m 0644 "$OMARCHY_PATH/default/uwsm/env.d/10-omarchy" /usr/share/uwsm/env.d/10-omarchy
fi

# Materialise the default theme (Tokyo Night), which upstream's installer does at
# install time. Headless, because there is no session in the chroot; if a tool it
# needs is missing the shell falls back at first login and the warning says so.
log "Applying the default theme (Tokyo Night)"
env HOME="$HOME" \
	PATH="$OMARCHY_PATH/bin:/usr/local/bin:/usr/bin:/bin" \
	OMARCHY_PATH="$OMARCHY_PATH" \
	OMARCHY_THEME_HEADLESS=1 \
	omarchy-theme-set "Tokyo Night" ||
	warn "omarchy-theme-set failed; the theme is applied on first login"
chown -R "$USERNAME:$USERNAME" "$HOME/.config" "$HOME/.local" 2>/dev/null || true

exit 0
