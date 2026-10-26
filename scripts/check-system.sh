#!/bin/bash
# Health report for a system built by this repository.
#
# Runs *on the target device*, not on the build host, so it is standalone: it
# does not source scripts/lib.sh, it needs no repository and it changes nothing.
# Every check maps to a failure this port has actually hit (see AGENTS.md), so a
# red line here is a regression, not a hypothetical.
#
# It must run on the *booted* system. Inside a chroot over a mounted image the
# /proc, /sys and dmesg readings describe the host and `systemctl is-active`
# answers "Running in chroot, ignoring command" with exit status 0 for every
# unit, so the report would be silently wrong; the script warns when PID 1 is
# not systemd.
#
# Usage:
#   scripts/check-system.sh [REPO_DIR]
#
# Run it as the desktop user: the audio checks talk to that user's PipeWire, and
# the privileged reads (dmesg, iw, bluetoothctl, journalctl) go through
# passwordless `sudo -n`. From a root shell it re-executes itself as the session
# user.
#
# With a repository checkout (REPO_DIR, default: the parent of this script) it
# also verifies that every package declared in packages/*.list,
# devices/*/packages.list and uis/*/packages.list is installed.
#
# Exit status: 0 when nothing FAILs, 1 otherwise.
#
# Pitfall, with `pipefail` set: never test the status of `producer | grep -q ...`.
# `grep -q` exits on the first match, the producer then dies of SIGPIPE (141) and
# the pipeline - and therefore the `if` - reports failure even though the pattern
# was found. Whether it bites depends on how far the producer got before the pipe
# closed, so it shows up as a random false FAIL/WARN (the firmware check below
# reported a loaded firmware as missing on the card, and not on the host, with
# the very same input). Feed `grep` a here-string of an already-captured variable
# (`grep -q PAT <<<"$var"`) or use `grep -rqs PAT <path>` instead.
set -uo pipefail

PASS=0 WARN=0 FAIL=0
if [ -t 1 ]; then
	C_OK=$'\033[1;32m' C_WARN=$'\033[1;33m' C_FAIL=$'\033[1;31m'
	C_HEAD=$'\033[1;34m' C_DIM=$'\033[1;30m' C_OFF=$'\033[0m'
else
	C_OK= C_WARN= C_FAIL= C_HEAD= C_DIM= C_OFF=
fi

ok()      { PASS=$((PASS + 1)); printf '  %sOK%s    %s\n' "$C_OK" "$C_OFF" "$*"; }
warn()    { WARN=$((WARN + 1)); printf '  %sWARN%s  %s\n' "$C_WARN" "$C_OFF" "$*"; }
fail()    { FAIL=$((FAIL + 1)); printf '  %sFAIL%s  %s\n' "$C_FAIL" "$C_OFF" "$*"; }
skip()    { printf '  %sSKIP%s  %s\n' "$C_DIM" "$C_OFF" "$*"; }
info()    { printf '  ----  %s\n' "$*"; }
section() { printf '\n%s==> %s%s\n' "$C_HEAD" "$*" "$C_OFF"; }
have()    { command -v "$1" >/dev/null 2>&1; }

# A session started by greetd does not necessarily export the sbin directories,
# and several tools this report needs live there (swapon, findfs, blkid, iw,
# tuigreet). Fix the PATH once here instead of discovering each of them as a
# false failure - the first real report from the booted card did exactly that
# with swapon (which made an active zram swap look absent).
case ":$PATH:" in
	*:/usr/sbin:*) ;;
	*) PATH="$PATH:/usr/sbin:/sbin"; export PATH ;;
esac

# ---------------------------------------------------------------------------
# Privileges: privileged reads use `sudo -n`, session commands must run as the
# desktop user (they need that user's PipeWire and Wayland sockets).
# ---------------------------------------------------------------------------
if [ "$(id -u)" -eq 0 ]; then
	_user="${SUDO_USER:-}"
	if [ -z "$_user" ]; then
		for _d in /run/user/[0-9]*; do
			[ -d "$_d" ] || continue
			_user="$(stat -c %U "$_d" 2>/dev/null)"
			[ -n "$_user" ] && break
		done
	fi
	if [ -n "$_user" ] && [ "$_user" != root ] && have runuser; then
		exec runuser -u "$_user" -- \
			env XDG_RUNTIME_DIR="/run/user/$(id -u "$_user")" \
			"$(readlink -f "$0")" "$@"
	fi
	warn "running as root: the session checks (audio, PipeWire, sway) will not work"
fi

if [ "$(id -u)" -eq 0 ]; then
	asroot() { "$@" 2>/dev/null; }
else
	asroot() { sudo -n "$@" 2>/dev/null; }
	asroot true 2>/dev/null || warn "sudo -n does not work here: privileged checks will be skipped"
fi

# `systemctl is-active` always succeeds inside a chroot ("Running in chroot,
# ignoring command 'is-active'"), so service checks need their own gate: exit
# status 2 means "cannot tell", printed as SKIP instead of OK or FAIL.
if [ -d /run/systemd/system ]; then
	svc() { systemctl is-active --quiet "$1"; }
else
	svc() { return 2; }
fi
# check_svc UNIT "message when the unit is active"
check_svc() {
	svc "$1"
	case "$?" in
		0) ok "$2" ;;
		2) skip "$2 (cannot tell: not running under systemd)" ;;
		*) fail "$2" ;;
	esac
}

# Optional repository checkout, for the declared-package check.
_self="$(cd "$(dirname "$0")" && pwd)"
REPO="${1:-$(cd "$_self/.." 2>/dev/null && pwd)}"
if [ -z "$REPO" ] || [ ! -d "$REPO/packages" ] || [ ! -d "$REPO/uis" ]; then
	REPO=""
fi

# sway -C needs a runtime dir; keep one for the whole run.
RUNTIME="$(mktemp -d)"
trap 'rm -rf "$RUNTIME"' EXIT

printf '%sdebian-aarch64 system report%s  %s\n' "$C_HEAD" "$C_OFF" "$(date '+%Y-%m-%d %H:%M:%S %Z')"

# /proc, /sys and dmesg describe the kernel this script runs under. If PID 1 is
# not systemd it is not the booted Debian image, and everything read from them
# belongs to whatever the host is (typically Nura, over a mounted image).
_init="$(cat /proc/1/comm 2>/dev/null)"
if [ "$_init" != systemd ]; then
	warn "PID 1 is '${_init:-unknown}', not systemd: this is not the booted image."
	warn "The /proc, /sys and dmesg checks below describe the host kernel."
fi

# ---------------------------------------------------------------------------
section "Boot"
# ---------------------------------------------------------------------------
info "kernel  $(uname -r)"
info "cmdline $(cat /proc/cmdline)"
CMDLINE="$(cat /proc/cmdline)"
case "$CMDLINE" in
	*kern_guid=%U*)
		fail "kern_guid=%U was not substituted: depthcharge did not boot this kpart" ;;
	*kern_guid=*)
		ok "depthcharge substituted kern_guid" ;;
	*)
		warn "no kern_guid in the kernel command line" ;;
esac
case "$CMDLINE" in
	*root=UUID=*) ok "root=UUID= present" ;;
	*)            warn "the cmdline does not select the root filesystem by UUID" ;;
esac

# The kernel does not come from an ESP or from /boot: depthcharge loads it from
# the ChromeOS kernel partition named by the kern_guid it substituted. That
# partition is also what scripts/install-kpart.sh rewrites after a kernel
# update, so verify it is really there and really holds a signed FIT.
_guid=""
for _x in $CMDLINE; do
	case "$_x" in
		kern_guid=*) _guid="${_x#kern_guid=}" ;;
	esac
done
if [ -n "$_guid" ]; then
	_kpart_part="$(findfs PARTUUID="$_guid" 2>/dev/null || true)"
	if [ -z "$_kpart_part" ]; then
		fail "kern_guid=$_guid names no partition: nothing to boot from next time"
	else
		ok "kernel partition $_kpart_part (kern_guid=$_guid)"
		if have futility; then
			# Do not pipe futility into `grep -q` (see the header).
			_fv="$(asroot futility show "$_kpart_part" 2>/dev/null || true)"
			case "$_fv" in
				*'Body verification succeeded'*)
					ok "the kernel partition holds a dev-signed FIT"
					;;
				*)
					fail "futility cannot verify a signed kpart in $_kpart_part"
					;;
			esac
		else
			skip "futility not installed (package vboot-kernel-utils): kernel partition not verified"
		fi
	fi
fi

if [ -r /sys/firmware/devicetree/base/model ]; then
	_model="$(tr -d '\0' < /sys/firmware/devicetree/base/model)"
	_compat="$(tr '\0' ' ' < /sys/firmware/devicetree/base/compatible)"
	case "$_compat" in
		*google,*) ok "device tree: $_model" ;;
		*)         fail "device tree is not a Google Chromebook DTB: $_model ($_compat)" ;;
	esac
else
	fail "/sys/firmware/devicetree/base is missing (booted without a device tree?)"
fi

if [ -d "/lib/modules/$(uname -r)" ]; then
	ok "modules for $(uname -r) installed"
else
	fail "no /lib/modules/$(uname -r): the running kernel does not match the installed modules"
fi

# ---------------------------------------------------------------------------
section "Firmware"
# ---------------------------------------------------------------------------
if have dmesg; then
	_dmesg="$(asroot dmesg 2>/dev/null)"
	_missing="$(printf '%s\n' "$_dmesg" | grep -oE 'Direct firmware load for [^ ]+ failed' |
		awk '{print $5}' | sort -u)"
	if [ -z "$_missing" ]; then
		ok "no failed firmware loads in dmesg"
	else
		# A failed direct load is not necessarily a missing firmware: a driver
		# built as a module can request its firmware from the initramfs, long
		# before the root filesystem - and therefore /lib/firmware - exists, and
		# get it seconds later when the module is loaded for good. That is what
		# mtk-scp does here (mediatek/mt8192/scp.img: two -2 failures, then
		# "Booting fw image ... SCP is ready"), so what matters is the outcome,
		# not the first attempt.
		for _fw in $_missing; do
			if grep -q "Booting fw image $_fw" <<<"$_dmesg"; then
				ok "firmware $_fw: requested before the rootfs was up, then loaded"
			elif [ -e "/lib/firmware/$_fw" ] || [ -e "/lib/firmware/$_fw.zst" ] ||
				[ -e "/lib/firmware/$_fw.xz" ]; then
				warn "firmware $_fw exists in /lib/firmware but the kernel never loaded it"
			else
				warn "firmware $_fw is not in /lib/firmware (the kernel asked for it)"
			fi
		done
	fi
	_dmesg_err="$(printf '%s\n' "$_dmesg" | grep -icE '\b(error|failed)\b.*(rtw88|bluetooth|hci|sound|asoc)' || true)"
	[ "${_dmesg_err:-0}" -eq 0 ] && ok "no errors from the wireless/Bluetooth/audio subsystems" ||
		warn "$_dmesg_err dmesg line(s) mention errors in the wireless/Bluetooth/audio subsystems"
else
	skip "dmesg not available"
fi

# ---------------------------------------------------------------------------
section "Storage"
# ---------------------------------------------------------------------------
_root_src="$(findmnt -n -o SOURCE / 2>/dev/null)"
_root_uuid="$(findmnt -n -o UUID / 2>/dev/null)"
info "root    $_root_src (UUID $_root_uuid)"
case "$_root_src" in
	*mmcblk0*) info "booted from the internal eMMC" ;;
	*mmcblk1* | *mmcblk2* | *sd[a-z]*) info "booted from removable storage" ;;
esac

# A root UUID that does not match /etc/fstab boots exactly once. blkid is the
# fallback because findmnt's UUID column needs the device node to be present.
_fstab_uuid="$(awk '$2 == "/" { print $1 }' /etc/fstab 2>/dev/null | sed -n 's/^UUID=//p')"
if [ -z "$_root_uuid" ] && have blkid; then
	_root_uuid="$(blkid -s UUID -o value "$_root_src" 2>/dev/null)"
fi
if [ -z "$_fstab_uuid" ]; then
	fail "/etc/fstab has no UUID= entry for /"
elif [ -z "$_root_uuid" ]; then
	skip "/etc/fstab not compared: the root UUID could not be read"
elif [ "$_fstab_uuid" = "$_root_uuid" ]; then
	ok "/etc/fstab matches the mounted root"
else
	fail "/etc/fstab root UUID ($_fstab_uuid) != mounted root ($_root_uuid): the next boot will fail"
fi

# Two sources, so a stripped-down PATH or an odd util-linux cannot fake a
# "no swap" report on a machine that is swapping.
_swap="$(swapon --show=NAME --noheadings 2>/dev/null || true)"
[ -n "$_swap" ] || _swap="$(awk 'NR > 1 { print $1 }' /proc/swaps 2>/dev/null || true)"
if [ -n "$_swap" ]; then
	ok "swap active: $(printf '%s' "$_swap" | tr '\n' ' ')"
else
	warn "no swap active (zram-tools should provide /dev/zram0)"
fi

# p2 is /boot (see scripts/lib-cgpt.sh). If it is missing the system still
# boots - the kpart is self-contained - but the kernel files would then live in
# the root filesystem, which is not the layout this repository installs.
_boot_fstab="$(awk '$2 == "/boot" { print $1 }' /etc/fstab 2>/dev/null | sed -n 's/^UUID=//p')"
if [ -n "$_boot_fstab" ]; then
	_boot_src="$(findmnt -n -o SOURCE /boot 2>/dev/null || true)"
	_boot_uuid="$(findmnt -n -o UUID /boot 2>/dev/null || true)"
	[ -n "$_boot_uuid" ] && [ -n "$_boot_src" ] ||
		_boot_uuid="$(blkid -s UUID -o value "$_boot_src" 2>/dev/null || true)"
	if ! mountpoint -q /boot; then
		fail "/etc/fstab mounts /boot but it is not mounted"
	elif [ -n "$_boot_uuid" ] && [ "$_boot_uuid" = "$_boot_fstab" ]; then
		ok "/boot is a separate filesystem ($_boot_src) matching /etc/fstab"
	else
		fail "/boot UUID mismatch: fstab $_boot_fstab, mounted ${_boot_uuid:-unknown}"
	fi
	if [ -f "/boot/vmlinuz-$(uname -r)" ]; then
		ok "the running kernel's vmlinuz is on /boot"
	else
		fail "/boot/vmlinuz-$(uname -r) is missing: /boot does not hold the running kernel"
	fi
else
	warn "/etc/fstab has no /boot entry (not the layout this repository installs)"
fi

_used="$(df -P / | awk 'NR == 2 { print $5 }' | tr -d '%')"
if [ -n "$_used" ] && [ "$_used" -ge 90 ]; then
	warn "root filesystem ${_used}% full"
else
	ok "root filesystem ${_used}% used"
fi

# ---------------------------------------------------------------------------
section "Display and GPU"
# ---------------------------------------------------------------------------
_connected=""
for _s in /sys/class/drm/card*-*/status; do
	[ -r "$_s" ] || continue
	[ "$(cat "$_s")" = connected ] && _connected="$(basename "$(dirname "$_s")")"
done
if [ -n "$_connected" ]; then
	ok "display connected on $_connected"
else
	fail "no connected DRM connector"
fi

if [ -d /sys/module/panfrost ]; then
	ok "panfrost loaded (GPU acceleration available)"
else
	warn "panfrost is not loaded: no GPU acceleration"
fi

# The XF86MonBrightness* bindings run brightnessctl as the session user, which
# needs write access to /sys/class/backlight/*/brightness; Debian grants none,
# so uis/sway/files/etc/udev/rules.d/90-brightnessctl.rules does.
_bl=""
for _b in /sys/class/backlight/*/brightness; do
	[ -e "$_b" ] || continue
	_bl="$_b"
	_mode="$(stat -c %a "$_b")"
	_grp="$(stat -c %G "$_b")"
	if [ "$_grp" = video ] && [ "${_mode: -2:1}" -ge 6 ]; then
		ok "backlight writable by group video ($_mode $_grp)"
	else
		fail "$_b is $_mode $_grp: brightnessctl will fail with Permission denied"
	fi
done
[ -n "$_bl" ] || warn "no backlight device"
if [ "$(id -u)" -ne 0 ]; then
	_groups="$(id -nG 2>/dev/null || true)"
	if grep -qw video <<<"$_groups"; then
		ok "session user is in group video"
	else
		fail "session user is not in group video: brightnessctl cannot write the backlight"
	fi
fi

# The external display is the DTB plus one udev one-shot.  The patch describes
# the DPI output and the second ANX7625, which is what creates a DP connector
# at all; the bridge then has one DP transmitter and two downstream Type-C
# ports, and its TCPC crosspoint picks which port it reaches.  Mainline has no
# typec-switch mux (ChromeOS's driver does), so the chip keeps its power-on
# default - the left port - and a monitor on the right one still gets HPD,
# EDID and a mode while the panel shows its no-signal pattern.
_dp=""
for _s in /sys/class/drm/card*-DP-*/status; do
	[ -e "$_s" ] || continue
	_dp="${_s%/status}"
	_dp="${_dp##*/}"
	break
done
if [ -n "$_dp" ]; then
	ok "external display connector $_dp present (DPI and ANX7625 in the DTB)"
else
	fail "no DP connector: the DTB describes no DPI output (kernels/mediatek-mt81/patches/mt8192-asurada-external-display.patch)"
fi
if [ -x /usr/local/sbin/lazyos-dp-mux ]; then
	ok "lazyos-dp-mux is executable"
else
	fail "/usr/local/sbin/lazyos-dp-mux is missing or not executable: the Type-C crosspoint is never set"
fi
if grep -qs lazyos-dp-mux /etc/udev/rules.d/90-lazyos-dp-mux.rules; then
	ok "udev runs lazyos-dp-mux when a port enters DisplayPort alt mode"
else
	fail "/etc/udev/rules.d/90-lazyos-dp-mux.rules is missing: only the left-hand port reaches the monitor"
fi
if command -v i2cset >/dev/null 2>&1; then
	ok "i2cset is installed"
else
	fail "i2cset is missing (i2c-tools): lazyos-dp-mux cannot write the crosspoint"
fi
# The crosspoint has to name the port the EC put into DisplayPort alt mode, or
# the monitor is an EDID source with no lanes behind it.  Reading it does not
# change the chip: each transaction carries its own register address, and the
# two registers live on the same page.
for _alt in /sys/class/typec/port*-partner/port*-partner.*/; do
	[ -r "$_alt/active" ] || continue
	[ "$(cat "$_alt/active" 2>/dev/null)" = yes ] || continue
	_port="${_alt#/sys/class/typec/}"
	_port="${_port%%-*}"
	case $_port in
	port0) _want=0x21 ;;   # left  / TYPEC_ORIENTATION_NORMAL
	port1) _want=0x12 ;;   # right / TYPEC_ORIENTATION_REVERSE
	*) continue ;;
	esac
	_bus=""
	for _c in /sys/bus/i2c/devices/*-0058; do
		[ -e "$_c/of_node/ports/port@0/endpoint/bus-type" ] || continue
		_bus="${_c##*/}"
		_bus="${_bus%%-*}"
		break
	done
	_got=0x
	if [ -n "$_bus" ] && command -v i2cget >/dev/null 2>&1; then
		_got="$(asroot i2cget -f -y "$_bus" 0x2c 0xb4)"
	fi
	case $_got in
	"$_want") ok "Type-C crosspoint on $_port ($_got)" ;;
	0x) skip "Type-C crosspoint on $_port (no i2c bus or i2cget here)" ;;
	*) fail "Type-C crosspoint is $_got, not $_want: a monitor on $_port gets no picture" ;;
	esac
done

# The resume half.  udev sees a plug, never a resume: the bridge re-initialises
# when the machine wakes and the crosspoint goes back to its power-on default
# (measured here: 0x12 before the sleep, 0x00 and then 0x21 after it), while the
# EC re-detects the partner without re-adding the alt-mode device the rule
# matches.  Without this hook the connector still reports connected and sway
# still has the mode, so the picture is painted into the empty port until the
# cable is replugged.  The modeset is asked for by name because the crosspoint
# write alone changes nothing - the link was already trained when it ran.
_dpr="$HOME/.config/bin/lazyos-dp-resume"
if [ -x "$_dpr" ]; then
	ok "lazyos-dp-resume is executable"
	if grep -qs 'resolution' "$_dpr"; then
		ok "lazyos-dp-resume retrains the link after writing the crosspoint"
	else
		fail "$_dpr lost its modeset: the crosspoint would be right and the link still dead"
	fi
else
	fail "$_dpr is missing or not executable: the monitor keeps its picture only until the first suspend"
fi

# The arrangement menu and the file it writes.  A runtime `output position` does
# not survive a logout, so the menu saves one; a file the session's config does
# not include is the same as none, and a Settings row that is gone leaves the
# menu with no way in.
_disp="$HOME/.config/bin/rofi-display"
if [ -x "$_disp" ]; then
	ok "rofi-display is executable"
	if grep -qs rofi-display "$HOME/.config/bin/rofi-quick"; then
		ok "Settings has the display arrangement row"
	else
		fail "rofi-quick has no display arrangement row: the menu is unreachable"
	fi
	if grep -qs 'config\.d' "$HOME/.config/sway/config"; then
		ok "the saved arrangement is read at login (config.d included)"
	else
		fail "$HOME/.config/sway/config does not include config.d/: a saved arrangement is never applied"
	fi
else
	fail "$_disp is missing or not executable: the monitor can only be arranged until the next logout"
fi
if have wdisplays; then
	ok "wdisplays is installed (manual arrangement)"
else
	fail "wdisplays is missing: the menu's manual row cannot open it"
fi

# The one state with no other exit: the panel off and no monitor.  That is a
# black screen, so the menu has to keep offering the row that brings the panel
# back - and a missing row is invisible until the day it is needed.  Exercised
# with a stub swaymsg (the real one would change the session) and a stub picker
# that keeps the rows it was offered.
if [ -x "$_disp" ] && [ -f "$HOME/.config/bin/lib-rofi.sh" ]; then
	_bt=$(mktemp -d "${TMPDIR:-/tmp}/lazyos-display.XXXXXX" 2>/dev/null || true)
	if [ -n "$_bt" ]; then
		mkdir -p "$_bt/bin"
		cp "$_disp" "$HOME/.config/bin/lib-rofi.sh" "$_bt/" 2>/dev/null || true
		cat >"$_bt/bin/swaymsg" <<'SH'
#!/bin/sh
case $1 in
-t) printf '%s\n' '[{"name":"eDP-1","active":false,"rect":{"x":0,"y":0,"width":0,"height":0},"scale":1.0,"current_mode":null,"modes":[]}]' ;;
esac
exit 0
SH
		printf '%s\n' '#!/bin/sh' 'cat >"$DISPLAY_TEST_ROWS"' 'exit 1' >"$_bt/bar-pick"
		chmod 0755 "$_bt/bin/swaymsg" "$_bt/bar-pick" "$_bt/rofi-display" 2>/dev/null || true
		_black=$(
			export PATH="$_bt/bin:$PATH" DISPLAY_TEST_ROWS="$_bt/rows"
			"$_bt/rofi-display" >/dev/null 2>&1 || true
			grep -qs 'Bring the internal panel back' "$_bt/rows" && printf 'ok '
			"$_bt/rofi-display" --status 2>/dev/null
		)
		case $_black in
		'ok internal panel off')
			ok "the display menu brings the panel back with no monitor (no dead end)" ;;
		*)
			fail "the display menu has no way back with the panel off and no monitor ('${_black:-nothing}'): that is a black screen and the power key" ;;
		esac
		rm -rf "$_bt"
	fi
fi

# The blind key.  With every output off there is nowhere to draw a menu - sway
# lets the last output be turned off (measured) - so the recovery has to be a
# command, and both halves matter: a binding whose mode is gone does nothing,
# and a mode nobody binds is unreachable with a black screen.
if grep -qs 'rofi-display --panel-on' /etc/sway/config.d/*.conf \
	"$HOME/.config/sway/config.d"/*.conf 2>/dev/null; then
	ok "a key turns the internal panel back on with no picture (blind recovery)"
else
	fail "no binding runs 'rofi-display --panel-on': a black screen has no way back"
fi
if grep -qs -- '--panel-on' "$_disp" 2>/dev/null; then
	ok "rofi-display --panel-on exists"
else
	fail "$_disp has no --panel-on mode: the recovery key would run nothing"
fi

# ---------------------------------------------------------------------------
section "Input"
# ---------------------------------------------------------------------------
if [ -r /proc/bus/input/devices ]; then
	_kb="$(grep -cE '^H: Handlers=.*kbd' /proc/bus/input/devices)"
	# Pointers and touchscreens are the devices that report absolute axes;
	# they carry no 'mouse' handler (that needs CONFIG_INPUT_MOUSEDEV).
	_pt="$(grep -cE '^B: ABS=[0-9a-f]*[1-9a-f]' /proc/bus/input/devices)"
	[ "${_kb:-0}" -ge 1 ] && ok "$_kb keyboard device(s)" || fail "no keyboard device"
	[ "${_pt:-0}" -ge 1 ] && ok "$_pt pointer/touch device(s)" ||
		fail "no pointer or touchscreen device (absolute-axis input)"
	info "devices: $(sed -n 's/^N: Name="\(.*\)"$/\1/p' /proc/bus/input/devices | tr '\n' '|')"
else
	skip "/proc/bus/input/devices not available"
fi

if have keyd || have keyd.rvaiya; then
	check_svc keyd "keyd.service is active"
	# keyd grabs the keyboard, so a `sleep = ...` mapping swallows KEY_SLEEP
	# before systemd-logind's button handling sees it and the Lock key stops
	# suspending the machine. That is exactly what happened once (AGENTS.md).
	if grep -rqsE '^[[:space:]]*sleep[[:space:]]*=' /etc/keyd/ 2>/dev/null; then
		fail "keyd maps the sleep key: logind never sees KEY_SLEEP and the Lock key cannot suspend"
	else
		ok "keyd leaves the sleep key alone"
	fi
	_match="$(asroot journalctl -u keyd -b --no-pager 2>/dev/null | grep 'DEVICE: match' | tail -1)"
	if [ -n "$_match" ]; then
		ok "keyd matched its keyboard: $(printf '%s' "$_match" | sed 's/.*DEVICE: match *//')"
	else
		warn "keyd logged no 'DEVICE: match' this boot"
	fi
else
	skip "keyd is not installed"
fi

# ---------------------------------------------------------------------------
section "Network"
# ---------------------------------------------------------------------------
if have iw; then
	_ifaces="$(asroot iw dev | awk '/Interface/ {print $2}')"
	if [ -n "$_ifaces" ]; then
		ok "wireless interface: $(printf '%s' "$_ifaces" | tr '\n' ' ')"
	else
		fail "no wireless interface (is rtw88_8822ce loading its firmware?)"
	fi
	# A world regulatory domain (country 00) is not a bug by itself: it is what
	# remains while neither NetworkManager, wpa_supplicant nor the AP's 802.11d
	# country IE has set one. What *is* a bug is wireless-regdb missing, which
	# makes cfg80211 log "failed to load regulatory.db" and pins the world domain
	# no matter what connects (AGENTS.md).
	_reg="$(asroot iw reg get | sed -n 's/^country \([^:]*\).*/\1/p' | head -1)"
	_regdb_bad="$(asroot dmesg 2>/dev/null | grep -c 'failed to load regulatory.db' || true)"
	if [ "${_regdb_bad:-0}" -gt 0 ]; then
		warn "cfg80211 could not load regulatory.db (package wireless-regdb): the world domain cannot change"
	elif [ -z "$_reg" ] || [ "$_reg" = 00 ]; then
		info "regulatory domain: world (country 00), set once NetworkManager or the AP provides one"
	else
		ok "regulatory domain: $_reg"
	fi
else
	skip "iw is not installed"
fi

check_svc NetworkManager "NetworkManager is active ($(nmcli -t -f STATE,CONNECTIVITY g 2>/dev/null))"
# Profiles: ask NetworkManager when it runs, otherwise count the files.
_profiles="$(nmcli -t -f NAME,TYPE con show 2>/dev/null | grep -c ':802-11-wireless')"
[ "${_profiles:-0}" -eq 0 ] &&
	_profiles="$(ls /etc/NetworkManager/system-connections/*.nmconnection 2>/dev/null | wc -l)"
[ "${_profiles:-0}" -ge 1 ] && ok "$_profiles Wi-Fi profile(s) installed" ||
	warn "no Wi-Fi profile imported (scripts/wifi.sh import)"
# A profile copied verbatim from Nura pins interface-name=wlan0 and is never
# activated on Debian, where the interface is wlp1s0.
if grep -rqs '^interface-name=' /etc/NetworkManager/system-connections/ 2>/dev/null; then
	warn "a Wi-Fi profile still pins interface-name= (scripts/wifi.sh import strips it)"
elif [ "${_profiles:-0}" -ge 1 ]; then
	ok "no Wi-Fi profile pins an interface name"
fi
have nmtui && ok "nmtui present" || warn "nmtui missing (package network-manager-tui)"
# iproute2 (ip, ss) and iputils-ping (ping): the three commands every "is the
# network up" answer starts with.  Debian's base system does not pull iproute2
# in, and a missing `ip` turns the first troubleshooting step into "command not
# found", which reads as a broken system rather than a missing package.
_missing_net=""
for _c in ip ping ss; do have "$_c" || _missing_net="$_missing_net $_c"; done
if [ -z "$_missing_net" ]; then
	ok "network diagnostics present (ip, ping, ss)"
else
	fail "network diagnostics missing:$_missing_net (the first troubleshooting step answers 'command not found')"
fi
# Remote login is on by design: it is the maintainer's recovery path (the same
# way a card can put a working system back on the eMMC), and turning it off is a
# decision, not a cleanup.  The report states it rather than leaving it implicit,
# because a machine whose ssh is expected to answer and does not is a surprise
# either way.  The default password is documented in 04-software.md.
if systemctl is-enabled ssh >/dev/null 2>&1; then
	if systemctl is-active ssh >/dev/null 2>&1; then
		info "remote login is enabled (ssh, default password documented in 04-software.md)"
	else
		warn "ssh is enabled but not running: remote login does not answer"
	fi
fi

# ---------------------------------------------------------------------------
section "Bluetooth"
# ---------------------------------------------------------------------------
if have bluetoothctl; then
	_show="$(asroot bluetoothctl show)"
	info "controller $(printf '%s\n' "$_show" | grep -m1 'Controller' | cut -d' ' -f2-)"
	case "$_show" in
		*"Powered: yes"*) ok "controller powered" ;;
		*) fail "controller is not powered (bluetoothd? rtl_bt firmware?)" ;;
	esac
	have btmgmt && info "btmgmt: $(asroot btmgmt info | grep -m1 'current settings' | sed 's/^ *//')"
else
	skip "bluetoothctl is not installed (package bluez)"
fi

# ---------------------------------------------------------------------------
section "Audio"
# ---------------------------------------------------------------------------
if have pactl; then
	if pactl info >/dev/null 2>&1; then
		ok "audio server: $(pactl info | sed -n 's/^Server Name: //p')"
		# PipeWire names an ALSA card after the platform device
		# (alsa_card.platform-sound), not after the codec, so the codec name is
		# asked of ALSA instead. What matters is that a card exists at all: with
		# no card there is no UCM boot sequence and no sink but Dummy Output.
		_cards="$(pactl list cards short 2>/dev/null)"
		if [ -z "$_cards" ]; then
			fail "no sound card in PipeWire at all (UCM boot sequence did not run?)"
		else
			ok "sound card: $(printf '%s\n' "$_cards" | head -1 | cut -f2)"
			_codec="$(grep -oE 'mt[0-9]+[a-z0-9_]*' /proc/asound/cards 2>/dev/null | sort -u | tr '\n' ' ')"
			[ -n "$_codec" ] && ok "ALSA codec: $(printf '%s' "$_codec" | sed 's/ $//')" ||
				warn "no MediaTek codec name in /proc/asound/cards"
		fi
		# The card's routing lives in a UCM BootSequence, which alsa-lib only
		# runs on an explicit `set _boot`; alsactl does it at card add. Without
		# it ACP marks HiFi unsupported and the sink is a Dummy Output.
		_profile="$(pactl list cards 2>/dev/null | grep -m1 'Active Profile:' | sed 's/.*: //')"
		case "$_profile" in
			"" | off) fail "card active profile is '${_profile:-none}': the UCM boot sequence did not run (alsa-utils)" ;;
			*)        ok "card active profile: $_profile" ;;
		esac
		_sinks="$(pactl list short sinks 2>/dev/null | awk '{print $2}' | tr '\n' ' ')"
		case "$_sinks" in
			"" | *auto_null*) fail "no real sink (found: ${_sinks:-none}): WirePlumber fell back to Dummy Output" ;;
			*)                ok "sinks: $_sinks" ;;
		esac
	else
		fail "pactl cannot reach the session PipeWire/PulseAudio server"
	fi
else
	skip "pactl is not installed (package pulseaudio-utils)"
fi
# ALSA-only programs need a usable `default` PCM, which pipewire-alsa supplies.
if have aplay; then
	_pcm="$(aplay -L 2>/dev/null || true)"
	if grep -qx default <<<"$_pcm"; then
		ok "alsa 'default' PCM available (pipewire-alsa)"
	else
		fail "no alsa 'default' PCM: aplay/emulators fail without an explicit -D"
	fi
else
	skip "aplay is not installed (package alsa-utils)"
fi

# ---------------------------------------------------------------------------
section "Suspend and power"
# ---------------------------------------------------------------------------
grep -qw mem /sys/power/state 2>/dev/null && ok "/sys/power/state advertises suspend ($(cat /sys/power/state))" ||
	fail "no 'mem' in /sys/power/state: the kernel cannot suspend"
check_svc systemd-logind "systemd-logind is active (its button handling is the polkit-free suspend path)"
check_svc polkit "polkitd is active (session suspend via loginctl)"
[ -x "$HOME/.config/bin/sway-suspend" ] && ok "sway-suspend helper is executable" ||
	warn "~/.config/bin/sway-suspend is missing or not executable"
_susp="$(asroot journalctl -b --no-pager | grep -c 'PM: suspend entry')"
[ "${_susp:-0}" -ge 1 ] && ok "$_susp suspend/resume cycle(s) recorded in this boot" ||
	warn "no suspend recorded in this boot: press the Lock key to test it"

# ---------------------------------------------------------------------------
section "System tuning"
# ---------------------------------------------------------------------------
# Everything below is a deliberate choice documented in docs/optimization.md, and
# the failure mode is always the same: something silently takes it back (a
# package resets the governor, an overlay stops being installed) and the machine
# is slower or hungrier with no message anywhere.

_govs="$(cat /sys/devices/system/cpu/cpufreq/policy*/scaling_governor 2>/dev/null | sort -u | tr '\n' ' ')"
case "$_govs" in
	"schedutil ") ok "cpufreq governor is schedutil on every policy" ;;
	*) warn "cpufreq governor is '${_govs% }', not schedutil (ondemand pins both clusters at max)" ;;
esac

# MGLRU (CONFIG_LRU_GEN + CONFIG_LRU_GEN_ENABLED): reclaim by generation instead
# of a full scan, which is what a 4 GB machine under zram pressure needs.
if [ -r /sys/kernel/mm/lru_gen/enabled ]; then
	_lru="$(cat /sys/kernel/mm/lru_gen/enabled)"
	case "$_lru" in
		0x0000) fail "MGLRU is built in but disabled (lru_gen/enabled=$_lru)" ;;
		*) ok "MGLRU enabled ($_lru)" ;;
	esac
else
	fail "no /sys/kernel/mm/lru_gen: the kernel was built without CONFIG_LRU_GEN"
fi

# THP=madvise comes from the kernel command line: with 4 GB and zram, 'always'
# both wastes memory and adds latency spikes.
_thp="$(cat /sys/kernel/mm/transparent_hugepage/enabled 2>/dev/null)"
case "$_thp" in
	*'[madvise]'*) ok "transparent hugepages: madvise" ;;
	*) warn "transparent hugepages: ${_thp:-unknown} (expected [madvise] from the cmdline)" ;;
esac

# Swap has to be zram, and it has to be a unit: systemd-oomd's ManagedOOMSwap
# only sees units, and the generator (not a swapon in a script) is what creates
# dev-zram0.swap.
case "$(cat /proc/swaps 2>/dev/null)" in
	*/dev/zram0*) ok "swap is zram ($(awk '/zram0/ { print $4 " kB in use"; exit }' /proc/swaps))" ;;
	*) fail "no zram swap: memory pressure has nowhere to go" ;;
esac
case "$(systemctl list-units --all --no-legend 'dev-zram0.swap' 2>/dev/null)" in
	*dev-zram0.swap*) ok "zram swap is a systemd unit (systemd-oomd can manage it)" ;;
	*) warn "zram swap is not a dev-zram0.swap unit: systemd-oomd cannot manage it" ;;
esac

# The size of the zram device is the one tuning value whose regression is silent in
# either direction: half of RAM still "works" (it sends pages to the card while RAM
# is free), and full RAM works too (it swaps out cold pages and every re-touch pays
# an lz4 decompress). Both halves are asked - what the generator will do on the next
# boot, and what this boot actually got, because a machine whose file was edited in
# place keeps the device it created at boot until it reboots.
_zrc=/etc/systemd/zram-generator.conf
if [ -r "$_zrc" ]; then
	_zrsize=$(awk -F= '/^[[:space:]]*zram-size[[:space:]]*=/ { gsub(/[[:space:]]/, "", $2); print $2; exit }' "$_zrc")
	case $_zrsize in
	min\(ram\*2/3,4096\)) ok "zram-size = min(ram * 2 / 3, 4096) (a burst tier, not the full-RAM cap)" ;;
	"") warn "$_zrc sets no zram-size: the packaged min(ram/2, 4096) applies, so the card is reached with RAM still free" ;;
	*) warn "zram-size = $_zrsize, not min(ram * 2 / 3, 4096) (see docs/optimization.md, 'zram sizing')" ;;
	esac
	# A limit looks like the valve for incompressible data and is the wrong one: the
	# store fails with BLK_STS_IOERR and the allocator keeps choosing zram anyway
	# (it picks by slot exhaustion, not by write success), so the overflow never
	# reaches the card. See docs/optimization.md, "zram sizing".
	if grep -qs '^[[:space:]]*zram-resident-limit' "$_zrc"; then
		warn "$_zrc sets zram-resident-limit: a limit fails the store (I/O error) instead of handing the page to the card at priority 10"
	fi
else
	warn "no $_zrc: the packaged zram-size default (half of RAM) applies"
fi
# What this boot really has: `min(ram * 2 / 3, 4096)` is two thirds of MemTotal (or
# 4096 MiB on a machine with more than 6 GiB), so a device within 10 % of that is
# the configured size (the generator rounds to whole MiB). A smaller device spills
# to the card, and a larger one is the full-RAM cap whose latency this port rejected.
if [ -r /sys/block/zram0/disksize ]; then
	_zdisk=$(($(cat /sys/block/zram0/disksize) / 1024 / 1024))
	_zmem=$(($(awk '/^MemTotal:/ { print $2 }' /proc/meminfo) / 1024))
	_zwant=$((_zmem * 2 / 3))
	[ "$_zwant" -gt 4096 ] && _zwant=4096
	if [ "$_zdisk" -ge $((_zwant * 11 / 10)) ]; then
		warn "zram device is $_zdisk MiB, larger than the ~$_zwant MiB cap: a full-RAM device is the swap latency this port rejected (reboot after editing $_zrc)"
	elif [ "$_zdisk" -ge $((_zwant * 9 / 10)) ]; then
		ok "zram device is $_zdisk MiB, ~2/3 of $_zmem MiB of RAM"
	else
		fail "zram device is $_zdisk MiB but ~2/3 of RAM is $_zwant MiB: pages go to the card while RAM is free (rebuild, or reboot after editing $_zrc)"
	fi
fi

check_svc systemd-oomd "systemd-oomd is active (kills a hog on memory pressure)"

[ "$(cat /proc/sys/vm/swappiness 2>/dev/null || echo 0)" -ge 100 ] &&
	ok "vm.swappiness=$(cat /proc/sys/vm/swappiness) (swapping to compressed RAM is cheap)" ||
	warn "vm.swappiness=$(cat /proc/sys/vm/swappiness): too low for a zram-only system"

case "$(cat /sys/block/mmcblk0/queue/scheduler 2>/dev/null)" in
	*'[kyber]'*) ok "eMMC I/O scheduler: kyber" ;;
	*) warn "eMMC I/O scheduler: $(cat /sys/block/mmcblk0/queue/scheduler 2>/dev/null) (expected [kyber])" ;;
esac

# The kernel options the tuning depends on, read from the config the image ships
# in /boot rather than assumed: a kernel built from a different config (or the
# stock one after a bad install) is invisible otherwise.
_cfg="$(ls -1 /boot/config-* 2>/dev/null | head -1)"
if [ -r "$_cfg" ]; then
	_missing_cfg=""
	[ -n "$(grep -x 'CONFIG_HZ=1000' "$_cfg" 2>/dev/null)" ] || _missing_cfg="$_missing_cfg HZ=1000"
	[ -n "$(grep -x 'CONFIG_LRU_GEN=y' "$_cfg" 2>/dev/null)" ] || _missing_cfg="$_missing_cfg LRU_GEN"
	[ -n "$(grep -x 'CONFIG_CPU_FREQ_DEFAULT_GOV_SCHEDUTIL=y' "$_cfg" 2>/dev/null)" ] || _missing_cfg="$_missing_cfg governor=schedutil"
	[ -z "$_missing_cfg" ] && ok "kernel config carries the tuning ($(basename "$_cfg"))" ||
		warn "kernel config lacks:$_missing_cfg"
else
	skip "no /boot/config-* to check the kernel options against"
fi

# ---------------------------------------------------------------------------
section "Clock"
# ---------------------------------------------------------------------------
if have timedatectl; then
	info "$(timedatectl show -p Timezone --value 2>/dev/null) / local RTC $(timedatectl show -p LocalRTC --value 2>/dev/null)"
	[ "$(timedatectl show -p NTPSynchronized --value 2>/dev/null)" = yes ] && ok "clock synchronised" ||
		warn "clock not synchronised (systemd-timesyncd needs network)"
else
	skip "timedatectl is not installed"
fi

# ---------------------------------------------------------------------------
section "Session"
# ---------------------------------------------------------------------------
check_svc greetd "greetd is active"
if [ -r /etc/greetd/config.toml ]; then
	# Debian's greetd creates the _greetd user and its unit conflicts with
	# getty@tty7, so vt must be 7; with any other name the service dies and the
	# machine falls back to a console login (AGENTS.md).
	_vt="$(sed -n 's/^vt *= *//p' /etc/greetd/config.toml | tr -d '"')"
	_user="$(sed -n 's/^user *= *//p' /etc/greetd/config.toml | tr -d '"')"
	[ "$_vt" = 7 ] && ok "greeter vt = 7" ||
		fail "greeter vt = ${_vt:-unset} (must be 7: greetd conflicts with getty@tty7)"
	getent passwd "$_user" >/dev/null 2>&1 && ok "greeter user $_user exists" ||
		fail "greeter user '${_user:-unset}' does not exist: greetd exits and the greeter never shows"
else
	fail "/etc/greetd/config.toml is missing"
fi
have tuigreet && ok "tuigreet is installed" || warn "tuigreet is not installed"
# greetd hands the session only the PAM environment, so pam_env is what puts the
# locale there; without it every terminal warns "'C' is not a UTF-8 locale" and
# sorting/matching run on the C locale.
case "${LANG:-}" in
*UTF-8* | *utf8*) ok "session locale is UTF-8 (${LANG})" ;;
*) fail "session LANG is '${LANG:-unset}': greetd needs pam_env in /etc/pam.d/greetd (terminals warn and sorting is C)" ;;
esac
if have sway; then
	# The session runs the wlroots fork built by scripts/build-swayfx.sh, not
	# Debian's plain sway: 26-swayfx.conf needs it (corners, shadows, the blur
	# tuning and the layer_effects).  The version string is the only cheap way to
	# tell the two /usr/bin/sway binaries apart, and plain sway would start but
	# silently ignore the effects.
	_sway_version="$(sway --version 2>/dev/null | head -n1)"
	case "$_sway_version" in
	*swayfx*) ok "compositor is $_sway_version" ;;
	*) fail "compositor is '${_sway_version:-unknown}', not swayfx: rounded corners and shadows are missing (run scripts/install-swayfx.sh)" ;;
	esac
	# The renderer the session actually got. PanVK has no Mali-G57 support
	# ("vulkaninfo: Mali-G57 not supported") and SceneFX only implements a GLES
	# renderer, so panfrost through EGL is the whole stack. A forced
	# LIBGL_ALWAYS_SOFTWARE, a broken firmware load or a Mesa regression would
	# silently drop the session onto llvmpipe/swrast, where every frame and the
	# blur are drawn on the CPU - eglinfo reports the renderer without needing a
	# display, and it ships in mesa-utils (already a package here).
	if have eglinfo; then
		_gpu_renderer="$(eglinfo -B 2>/dev/null | awk -F': ' '/OpenGL ES profile renderer/ { print $2; exit }')"
		case "$_gpu_renderer" in
		*Panfrost*) ok "GPU renderer is $_gpu_renderer" ;;
		"") warn "eglinfo reported no renderer: cannot confirm the GPU driver" ;;
		*) fail "GPU renderer is '$_gpu_renderer', not Panfrost: the session is drawing on software (check LIBGL_ALWAYS_SOFTWARE and the GPU firmware)" ;;
		esac
	else
		skip "eglinfo is missing (mesa-utils): cannot confirm the GPU driver"
	fi
	if XDG_RUNTIME_DIR="$RUNTIME" WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
		sway -C -c /etc/sway/config >/dev/null 2>&1; then
		ok "sway configuration is valid"
	else
		fail "sway -C rejected /etc/sway/config"
	fi
	# sway starts Xwayland itself (it is not in the configuration), so a missing
	# package shows up only as two red lines in the journal and as X11 programs
	# that cannot launch (see AGENTS.md).
	if grep -rqsE '^[[:space:]]*xwayland[[:space:]]+disable' /etc/sway/config /etc/sway/config.d 2>/dev/null; then
		skip "Xwayland disabled in the sway configuration"
	elif [ -x /usr/bin/Xwayland ]; then
		ok "Xwayland present (X11 programs can run)"
	else
		warn "Xwayland is missing: sway logs a startup error and X11 programs cannot run"
	fi
fi
_missing_cmds=""
for _c in waybar foot rofi swaylock swaynag swaybg brightnessctl grimshot wlsunset wpctl pactl mako; do
	have "$_c" || _missing_cmds="$_missing_cmds $_c"
done
if [ -z "$_missing_cmds" ]; then
	ok "every command the sway configuration invokes is installed"
else
	warn "commands referenced by the sway configuration are missing:$_missing_cmds"
fi

# Workspace icons: workstyle renames every workspace to "<number>: <icons>" and
# the bar draws that name back ("format": "{value}"). Four things have to hold at
# once, and each one of them fails silently on its own: the binary, the exec line
# that starts it, a pill that is not trimmed back to its number, and glyphs that
# the menus' own alphabet carries (a codepoint from nowhere else is a tofu box in
# the pill, and nothing logs it).
if [ ! -x /usr/bin/workstyle ]; then
	fail "/usr/bin/workstyle is missing: every pill keeps its bare number and the workspace names stay bare (run scripts/install-workstyle.sh)"
else
	ok "workstyle installed ($(/usr/bin/workstyle --version 2>/dev/null | head -n1))"
	if grep -rqsE '^[[:space:]]*exec(_always)?[[:space:]]+.*workstyle' \
		/etc/sway/config /etc/sway/config.d 2>/dev/null; then
		ok "the sway configuration starts workstyle"
	else
		fail "no exec line starts workstyle (/etc/sway/config.d/02-workstyle.conf): the workspace names are never rewritten"
	fi
	if pgrep -x workstyle >/dev/null 2>&1; then
		ok "workstyle is running"
	else
		warn "workstyle is not running: the workspace names stay as they are until it starts (swaymsg reload starts it again)"
	fi
	_bar_cfg="$HOME/.config/waybar/config.jsonc"
	if grep -A 14 '"sway/workspaces"' "$_bar_cfg" 2>/dev/null | grep -q 'persistent-workspaces'; then
		warn "the bar's workspaces module still carries persistent-workspaces: it is matched by workspace *name*, so every renamed workspace draws a duplicate pill beside it"
	fi
	if grep -A 14 '"sway/workspaces"' "$_bar_cfg" 2>/dev/null | grep -q '"format": "{value}"'; then
		ok "the bar draws the whole workspace name (number and icons)"
	elif [ -r "$_bar_cfg" ]; then
		warn "the bar's workspaces format is not \"{value}\": {name} keeps only what is after the colon, so the number disappears from the pill"
	fi
	_ws_conf="$HOME/.config/workstyle/config.toml"
	if [ ! -r "$_ws_conf" ]; then
		fail "$_ws_conf is missing: workstyle writes its own default there, which knows none of this session's apps"
	else
		# The glyphs are \uXXXX escapes, so the file stays ASCII (a literal
		# private-use character does not survive every editor, which is how an
		# earlier glyph table in this repository shipped as empty spans). Each one
		# has to be a codepoint ~/.config/bin/lib-rofi.sh already carries: the font
		# check below is what proves some installed font draws those.
		_ws_hexes="$(sed -n 's/.*"\\u\([0-9a-fA-F]\{4\}\)".*/\1/p' "$_ws_conf" | tr 'A-F' 'a-f' | sort -u)"
		_ws_alphabet="$(sed -n 's/^UI_I_[A-Z_]*=.*#[[:space:]]*\([0-9a-f][0-9a-f]*\)$/\1/p' \
			"$HOME/.config/bin/lib-rofi.sh" 2>/dev/null | sort -u)"
		_ws_stray=""
		for _h in $_ws_hexes; do
			grep -qx "$_h" <<<"$_ws_alphabet" || _ws_stray="$_ws_stray U+$_h"
		done
		if [ -z "$_ws_hexes" ]; then
			fail "no glyph parsed from $_ws_conf (every pill would show a bare number)"
		elif [ -n "$_ws_stray" ]; then
			fail "workstyle uses glyph(s) the menu alphabet does not carry:$_ws_stray (nothing checks whether a font draws them, and a pill with no glyph is a hole)"
		else
			ok "workspace icons use $(printf '%s\n' $_ws_hexes | wc -l) codepoints from the menu alphabet"
		fi
		# A line this check cannot read is either a typo or a construct the daemon
		# parses differently. A TOML parse error is loud in the journal (one retry
		# per second) and silent in the bar (the old names stay), so it is worth
		# saying once here.
		_ws_bad="$(grep -vE '^[[:space:]]*(#|$)' "$_ws_conf" |
			grep -cvE '^("[^"]*"[[:space:]]*=[[:space:]]*"\\u[0-9a-fA-F]{4}"|\[other\]|[a-z_]+[[:space:]]*=[[:space:]]*("[^"]*"|true|false))[[:space:]]*(#.*)?$' ||
			true)"
		[ "${_ws_bad:-0}" -eq 0 ] ||
			warn "$_ws_bad line(s) of $_ws_conf do not look like \"pattern\" = \"\\uXXXX\": a parse error leaves workstyle retrying once a second"
	fi
fi
# workstyle renames workspaces, so a command that names one ("workspace 3")
# targets a name that is about to change; `workspace number 3` is the form that
# keeps working, and it is what this configuration already uses everywhere.
_ws_named="$(grep -rnE '^[[:space:]]*[^#[:space:]][^#]*workspace[[:space:]]+[0-9]' \
	/etc/sway/config /etc/sway/config.d 2>/dev/null | head -n 3 || true)"
if [ -z "$_ws_named" ]; then
	ok "sway addresses workspaces by number, so renaming them does not break a binding"
else
	fail "a command names a workspace instead of its number: $(printf '%s' "$_ws_named" | tr '\n' ' ')(workstyle renames workspaces, so that name will not exist)"
fi

# kitty is the optional second terminal: nothing in the session starts it (foot is
# $term, the bar's popups, and every menu that needs a terminal) and no package
# list installs it, but the image ships its configuration so that `apt install
# kitty` does not put a second, unthemed palette on the wallpaper.  A kitty with a
# broken configuration looks *fine*: an unknown key is only a stderr line
# ("Ignoring unknown config key") and a value that does not parse - a mistyped
# colour, a font size that is not a number - is dropped in favour of the default.
# So the check asks kitty's own parser what the file resolves to, with no window
# (kitty +runpy, ~0.1 s, writes nothing), and compares that with the theme rather
# than grepping the file for the keys it is supposed to have.
if ! have kitty; then
	skip "kitty is not installed (the session terminal is foot)"
elif [ ! -r "$HOME/.config/kitty/kitty.conf" ]; then
	fail "$HOME/.config/kitty/kitty.conf is missing: kitty opens on its own palette beside a solarized foot"
else
	_kitty_out=$(LZY_KITTY_CONF="$HOME/.config/kitty/kitty.conf" kitty +runpy '
import os
from kitty.config import load_config
bad = []
o = load_config(os.environ["LZY_KITTY_CONF"], accumulate_bad_lines=bad)
print("bad=%d" % len(bad))
print("bg=%06x" % o.background.rgb)
print("fg=%06x" % o.foreground.rgb)
print("opacity=%s" % o.background_opacity)
# `font_family` resolves to a FontSpec namedtuple (family, system, postscript
# name, ...), so ask it for .system: a bare %s of the spec is a TypeError and the
# report then says the font is missing when it is right there.
print("font=%s" % o.font_family.system)
print("symbols=%d" % len(o.symbol_map))
for b in bad:
    print("badline=%d %s: %s" % (b.number, b.line, b.exception))
' 2>"$RUNTIME/kitty.err")
	_kitty_rc=$?
	_kitty_unknown=$(sed -n 's/.*Ignoring unknown config key: *//p' "$RUNTIME/kitty.err" | paste -sd, -)
	_kitty_badlines=$(sed -n 's/^bad=//p' <<<"$_kitty_out")
	_kitty_first=$(sed -n 's/^badline=[0-9]* //p' <<<"$_kitty_out" | head -n 1)
	if [ "$_kitty_rc" -ne 0 ] && [ -z "$_kitty_out" ]; then
		warn "kitty +runpy failed, cannot confirm the configuration: $(head -n 1 "$RUNTIME/kitty.err")"
	elif [ "${_kitty_badlines:-?}" != 0 ] || [ -n "$_kitty_unknown" ]; then
		fail "kitty ignores part of its configuration:${_kitty_unknown:+ unknown key(s) $_kitty_unknown;} ${_kitty_first:-no value reported} (the default is used instead, so the palette or the alpha is not what the theme says)"
	else
		_kitty_bg=$(sed -n 's/^bg=//p' <<<"$_kitty_out")
		_kitty_fg=$(sed -n 's/^fg=//p' <<<"$_kitty_out")
		_kitty_op=$(sed -n 's/^opacity=//p' <<<"$_kitty_out")
		_kitty_font=$(sed -n 's/^font=//p' <<<"$_kitty_out")
		_kitty_sym=$(sed -n 's/^symbols=//p' <<<"$_kitty_out")
		_kitty_off=
		[ "$_kitty_bg" = 002b36 ] ||
			_kitty_off="$_kitty_off background=#$_kitty_bg (not solarized base03)"
		[ "$_kitty_fg" = 93a1a1 ] ||
			_kitty_off="$_kitty_off foreground=#$_kitty_fg (not base1)"
		awk -v o="$_kitty_op" 'BEGIN { exit !(o > 0 && o < 1) }' ||
			_kitty_off="$_kitty_off opacity=$_kitty_op (opaque: swayfx has no blur to draw behind it)"
		[ "$_kitty_font" = "JetBrains Mono" ] ||
			_kitty_off="$_kitty_off font=$_kitty_font (not the theme's type)"
		[ "${_kitty_sym:-0}" -ge 2 ] ||
			_kitty_off="$_kitty_off symbol_maps=$_kitty_sym (Nerd Font glyphs render double width)"
		if [ -z "$_kitty_off" ]; then
			ok "kitty carries the session palette (solarized base03/base1, alpha $_kitty_op, $_kitty_font, $_kitty_sym symbol map(s))"
		else
			fail "kitty is not themed like the session:$_kitty_off"
		fi
	fi
fi

# Notifications: mako owns org.freedesktop.Notifications on the session bus. If
# the daemon is not running (crashed, or never started) nothing logs it and
# every application's notifications silently disappear, so ask the bus.
if have busctl; then
	_notify_name="$(busctl --user list --no-legend 2>/dev/null | awk '$1 == "org.freedesktop.Notifications" { print $1 }')"
	if [ -n "$_notify_name" ]; then
		ok "a notification daemon owns org.freedesktop.Notifications (mako)"
	else
		warn "no notification daemon on the session bus: mako is not running, notifications are lost"
	fi
else
	skip "busctl is not installed"
fi

# A notification icon is a name mako looks up on its icon-path, and one it cannot
# find draws nothing at all: the card is text-only, with no log line and no error.
# The names are lib-rofi.sh's UI_ICON_* constants.
if [ -r "$HOME/.config/bin/lib-rofi.sh" ] && [ -d /usr/share/icons/LazyOS ]; then
	_missing_icon=
	for _n in $(sed -n 's/^UI_ICON_[A-Z]*=\([a-z-]*\)$/\1/p' "$HOME/.config/bin/lib-rofi.sh"); do
		ls /usr/share/icons/LazyOS/*/apps/"$_n".png >/dev/null 2>&1 ||
			_missing_icon="$_missing_icon $_n"
	done
	[ -z "$_missing_icon" ] &&
		ok "notification icons resolve in the LazyOS theme" ||
		warn "notification icon(s) missing from /usr/share/icons/LazyOS:$_missing_icon (scripts/make-icons.sh)"
fi

# The other half of the same rule: a name only draws if the call asks for one.
# A `notify-send -a APP` without `-i` is a text-only card with no log line, and
# the file browser's eight messages were the last ones without a picture.
_notify_calls=$(grep -rn 'notify-send .*-a [A-Za-z]' "$HOME/.config/bin" 2>/dev/null || true)
_notify_noicon=$(printf '%s\n' "$_notify_calls" | grep -v -- ' -i ' || true)
if [ -z "$_notify_noicon" ]; then
	ok "every notification in ~/.config/bin carries an icon"
else
	warn "notification(s) without -i, so a text-only card: $(printf '%s\n' "$_notify_noicon" | sed 's#.*/bin/##' | tr '\n' ' ')"
fi

# mako reads icon-path itself (no theme metadata), so both halves have to be there
# for any picture to appear. The config has no --check, and a name it cannot find
# is silent, which is why this is asked of the configuration directly.
if [ -r "$HOME/.config/mako/config" ]; then
	_mako_icons=$(sed -n 's/^icons=//p' "$HOME/.config/mako/config" | head -1)
	_mako_path=$(sed -n 's/^icon-path=//p' "$HOME/.config/mako/config" | head -1)
	if [ "$_mako_icons" = 1 ] && [ -n "$_mako_path" ] && [ -d "$_mako_path" ]; then
		ok "notifications can carry a picture (icons=1, icon-path=$_mako_path)"
	else
		warn "mako draws no picture: icons=1 and an existing icon-path are missing from ~/.config/mako/config"
	fi
fi

# The bar's menus are the whole port of Deboma's panels: each indicator draws
# from waybar and runs ~/.config/bin/bar-* on click.  A missing script or a
# missing tool does not log anything - the indicator still appears and every
# click is a silent no-op - so this is a FAIL, not a warning.
_missing_bar=""
for _c in rofi jq nmcli bluetoothctl pactl brightnessctl foot; do
	have "$_c" || _missing_bar="$_missing_bar $_c"
done
for _s in bar-pick bar-menu bar-net bar-bt bar-key bar-audio bar-updates bar-power \
	bar-busy busy lib-rofi.sh; do
	[ -r "$HOME/.config/bin/$_s" ] || _missing_bar="$_missing_bar bin/$_s"
done
if [ -z "$_missing_bar" ]; then
	ok "bar menus complete (scripts plus rofi, nmcli, bluetoothctl, jq)"
else
	fail "bar menus broken:$_missing_bar (clicking those indicators does nothing)"
fi

# The bar's busy card is also the stop button: config.jsonc's custom/busy
# on-click runs `bar-busy stop`, and a lost key leaves a spinner that draws and
# does nothing when clicked - which is indistinguishable from "this action
# cannot be stopped", the answer the card gives on purpose. Read the module
# object out of the file (it is JSON with comments, so jq cannot parse it): the
# block starts at its key and ends at the first line that is only `}`.
_busy_mod=""
[ -r "$HOME/.config/waybar/config.jsonc" ] &&
	_busy_mod=$(awk '/"custom\/busy"[[:space:]]*:[[:space:]]*\{/ { f = 1 }
		f && /^[[:space:]]*\}/ { print; exit }
		f' "$HOME/.config/waybar/config.jsonc" 2>/dev/null || true)
case $_busy_mod in
*'on-click'*bar-busy*stop*)
	ok "the bar's busy card is the stop button (custom/busy on-click)" ;;
'')
	warn "custom/busy is not readable in ~/.config/waybar/config.jsonc: the spinner may not be clickable" ;;
*)
	warn "custom/busy has no on-click stop: the spinner cannot be clicked" ;;
esac

# Both ends share one protocol, exercised here for real because it is invisible
# until it fails: `busy start` records BUSY_STOP - a shell line, not a pid - in
# <tag>.stop (0600, since something executes it), `bar-busy stop` removes the
# card and runs it, and `busy cancelled` then answers 0 so the producer reports
# nothing instead of half a result. The probe is one file in the runtime
# directory and it is removed before this block ends.
_busy_rt=${XDG_RUNTIME_DIR:-/tmp}/lazyos-busy
_busy_bin="$HOME/.config/bin/busy"
if [ ! -d "${XDG_RUNTIME_DIR:-/tmp}" ] || [ ! -w "${XDG_RUNTIME_DIR:-/tmp}" ]; then
	skip "no writable runtime directory: the busy/stop protocol cannot be exercised here"
else
	if BUSY_STOP=: "$_busy_bin" start check-system Probe 'probe' >/dev/null 2>&1 &&
		[ -f "$_busy_rt/check-system.stop" ] &&
		[ "$(stat -c %a "$_busy_rt/check-system.stop" 2>/dev/null)" = 600 ]; then
		ok "busy records the stop command (mode 0600) the spinner runs"
		if "$_busy_bin" cancelled check-system >/dev/null 2>&1; then
			warn "busy cancelled says a running card is already gone (a stopped lookup looks finished)"
		else
			ok "busy cancelled tells a running task from a stopped one"
		fi
	else
		warn "busy does not record a stop command in $_busy_rt: the spinner cannot stop its task"
	fi
	"$_busy_bin" stop check-system >/dev/null 2>&1 || true
	if "$_busy_bin" cancelled check-system >/dev/null 2>&1; then
		ok "after a stop, busy says the task was cancelled (the producer reports nothing)"
	else
		warn "busy cancelled still sees the card after busy stop (the producer would report a half result)"
	fi
fi

# The idle screen is a foot window running ~/.config/bin/lazyos-screensaver: a
# loop of tte effects over the LazyOS wordmark, started by swayidle at 150 s.
# Each piece fails silently on its own: without tte or the art the static frame
# stays (the loop, which is the point of the entry, is gone), without the
# launcher the screen never appears, without the lock helper the 300 s timer
# does nothing, and without swayidle nothing starts at all.
_saver=""
have tte || _saver="$_saver tte"
have swayidle || _saver="$_saver swayidle"
[ -s "$HOME/.config/lazyos/screensaver.txt" ] || _saver="$_saver .config/lazyos/screensaver.txt"
[ -x "$HOME/.config/bin/lazyos-screensaver" ] || _saver="$_saver bin/lazyos-screensaver"
[ -x "$HOME/.config/bin/sway-lock" ] || _saver="$_saver bin/sway-lock"
[ -r "$HOME/.config/foot/screensaver.ini" ] || _saver="$_saver .config/foot/screensaver.ini"
if [ -z "$_saver" ]; then
	ok "idle screen complete (tte, LazyOS art, launcher, sway-lock, terminal profile)"
else
	fail "idle screen incomplete:$_saver (the idle screen is blank or never starts)"
fi

# And the pointer has to end it.  A pty forwards the mouse only when the program
# inside asks for it, so the dismissal on a mouse move is one escape: DECSET 1003
# (all motion) with 1006 (the SGR encoding it is read in).  Without the first,
# the loop sees key bytes only - motion and clicks stay foot's own business (a
# click starts a selection) and the wordmark keeps animating with no log line
# anywhere, which is what the card reported.  Leaving the mode on is the other
# half: a `--run` in a real shell would then write escape sequences into the
# prompt for the rest of the session.
_saver_bin="$HOME/.config/bin/lazyos-screensaver"
if [ -x "$_saver_bin" ]; then
	_saver_ptr=""
	grep -q '?1003h' "$_saver_bin" || _saver_ptr="no-1003h"
	grep -q '?1003l' "$_saver_bin" || _saver_ptr="$_saver_ptr no-1003l"
	case $_saver_ptr in
	'') ok "idle screen ends on the pointer too (mouse reporting on, and off again on exit)" ;;
	*no-1003h*)
		fail "idle screen does not ask the terminal for the mouse (?1003h): motion and clicks cannot end it" ;;
	*) warn "idle screen leaves mouse reporting on (?1003l): --run in a shell writes escapes into the prompt" ;;
	esac
fi

# Hiding the pointer while the idle screen is up is a session-wide seat setting,
# not a property of the window: sway offers no IPC read for it (get_seats
# reports only name, capabilities, focus and devices), so the value in effect
# before the saver is recovered from the merged config and the EXIT trap puts it
# back.  Without the restore, the seat keeps hiding the pointer after foot
# closes - the desktop pointer vanishing after every movement is the report this
# port got ("the mouse pointer disappears quickly").  Note sway clamps any
# non-zero timeout under 100 up to 100 ms, so `hide_cursor 1` is really 100.
if [ -x "$_saver_bin" ]; then
	_saver_cur=""
	grep -q 'hide_cursor 1' "$_saver_bin" || _saver_cur="$_saver_cur no-hide"
	grep -q 'prev_hide_cursor=' "$_saver_bin" || _saver_cur="$_saver_cur no-capture"
	grep -qF 'hide_cursor "$prev_hide_cursor"' "$_saver_bin" || _saver_cur="$_saver_cur no-restore"
	case $_saver_cur in
	'') ok "idle screen restores the pointer on exit (captures the prior hide_cursor and puts it back)" ;;
	*no-restore*)
		fail "idle screen hides the seat cursor and never resets it: the desktop pointer disappears after the saver" ;;
	*) warn "idle screen pointer-hide/restore drifted (capture or restore line changed)" ;;
	esac
fi

# The 150 s command has to return at once, or the rest of the chain never runs.
# swayidle runs it with `-w`, which blocks its whole event loop until the command
# exits (main.c, cmd_exec: waitpid), and a saver held in the foreground would
# keep the 300 s lock, the 360 s DPMS, the resume lines and before-sleep waiting
# for as long as the wordmark animated: the session would never lock, the panel
# would never go dark, and a lid close would sleep after logind's inhibitor
# timeout without the lock in front of it.  The stub foot sleeps, so a launcher
# that detaches returns in milliseconds and a foreground one cannot.  (Measured,
# not assumed: with `-w` a `timeout 1 'sleep 100'` kept a 2 s timeout from firing
# within 5 s; without `-w` it fired at 2 s.)
_saver_tmp=$(mktemp -d "${TMPDIR:-/tmp}/lazyos-saver.XXXXXX" 2>/dev/null || true)
if [ -n "$_saver_tmp" ] && [ -x "$_saver_bin" ]; then
	if pgrep -x swaylock >/dev/null 2>&1 || pgrep -f '[o]rg.lazyos.screensaver' >/dev/null 2>&1; then
		# Both make the launcher return before it reaches foot, which would be
		# an OK for the wrong reason.
		warn "idle screen launcher not tested: a lock or an idle screen is already up"
	else
		printf '%s\n' '#!/bin/sh' 'sleep 30' >"$_saver_tmp/foot"
		chmod +x "$_saver_tmp/foot"
		if PATH="$_saver_tmp:$PATH" timeout 5 "$_saver_bin" >/dev/null 2>&1; then
			ok "idle screen launcher returns at once (swayidle -w reaches the 300 s lock)"
		else
			fail "idle screen launcher stays in the foreground: with swayidle -w the lock, the panel-off timer and resume never run"
		fi
	fi
	pkill -f "$_saver_tmp/foot" 2>/dev/null || true
	rm -rf "$_saver_tmp"
fi

# The chain itself, read from the drop-in that owns it: a missing line means the
# screen never appears (150 s), the session never locks (300 s), the panel stays
# lit after the lock (360 s), the panel never comes back (resume) or the machine
# suspends before the lock is covered (before-sleep).  The numbers are compared,
# because a different timeout is the same as none.  The resume half is asked for
# twice on purpose: swayidle's own `resume` covers an idle period, and
# `after-resume` covers a suspend, which is not an idle timeout and would
# otherwise wake to a black panel that nothing turns back on.
_idle_cfg=/etc/sway/config.d/23-lazyos-screensaver.conf
_idle=""
[ -r "$_idle_cfg" ] || _idle=" $_idle_cfg"
if [ -r "$_idle_cfg" ]; then
	grep -q "timeout 150 .*lazyos-screensaver" "$_idle_cfg" || _idle="$_idle no-150s-screen"
	grep -q "timeout 300 .*sway-lock" "$_idle_cfg" || _idle="$_idle no-300s-lock"
	grep -q "timeout 360 .*power off" "$_idle_cfg" || _idle="$_idle no-360s-dpms"
	grep -q "^[[:space:]]*resume .*power on" "$_idle_cfg" || _idle="$_idle no-dpms-resume"
	grep -q "after-resume .*power on" "$_idle_cfg" || _idle="$_idle no-dpms-after-resume"
	grep -q "after-resume .*lazyos-dp-resume" "$_idle_cfg" || _idle="$_idle no-external-display-after-resume"
	grep -q "before-sleep .*sway-lock --wait" "$_idle_cfg" || _idle="$_idle no-lock-before-sleep"
fi
pgrep -x swayidle >/dev/null 2>&1 || _idle="$_idle swayidle-not-running"
if [ -z "$_idle" ]; then
	ok "idle chain armed (150 s idle screen, 300 s lock, 360 s panel off, lock before sleep)"
else
	fail "idle chain broken:$_idle"
fi

# A fullscreen film or presentation must not be interrupted by the lock, and
# mpv asks the compositor for that itself (zwp_idle_inhibitor_v1); Firefox and
# papers ask org.freedesktop.portal.Inhibit instead, which this image has no
# backend for, so the compositor holds the inhibitor on their behalf.  Without
# those lines the screen locks in the middle of a video, with no error anywhere.
# The bar's coffee cup is the manual override (waybar's own idle_inhibitor
# module); a missing module leaves the "keep awake" toggle unreachable, which is
# a warning rather than a failure because the automatic rules still work.
_inhib_cfg=/etc/sway/config.d/32-idle-inhibit.conf
_inhib=""
if [ -r "$_inhib_cfg" ]; then
	grep -qs 'firefox.*inhibit_idle fullscreen' "$_inhib_cfg" ||
		_inhib="$_inhib no-firefox"
	grep -qs 'Papers.*inhibit_idle fullscreen' "$_inhib_cfg" ||
		_inhib="$_inhib no-papers"
	grep -qs 'mpv.*inhibit_idle visible' "$_inhib_cfg" ||
		_inhib="$_inhib no-mpv"
else
	_inhib="$_inhib $_inhib_cfg"
fi
if [ -z "$_inhib" ]; then
	ok "fullscreen video inhibits the idle chain (Firefox, papers, mpv)"
else
	fail "idle inhibition incomplete:$_inhib (the lock interrupts a film)"
fi
if ! grep -qs '"idle_inhibitor"' "$HOME/.config/waybar/config.jsonc"; then
	warn "the bar has no idle_inhibitor module: there is no way to keep the screen awake by hand"
elif ! grep -qs 'idle_inhibitor\.activated' "$HOME/.config/waybar/style.css"; then
	warn "style.css has no #idle_inhibitor.activated rule: the coffee cup does not show whether it is on"
else
	ok "the bar carries a keep-awake toggle (idle_inhibitor, yellow when on)"
fi

# The on-screen display the volume and brightness keys draw.  The keys used to
# change the value and say nothing, and the only feedback was the bar's 12 px
# number - which a fullscreen window covers.  Three things break it silently: the
# helper loses its executable bit (the key presses and nothing appears), a
# binding goes back to the raw pactl/brightnessctl call (the value changes with
# no card), or the theme loses one of the three volume icons (the card is
# text-only, with no log line).  The mako side is asked too: without `icons=1`
# the icon is not drawn at all, and without a progress-color the `value` hint
# draws no bar.
_osd=""
[ -x "$HOME/.config/bin/osd" ] || _osd="$_osd bin/osd"
have notify-send || _osd="$_osd notify-send"
grep -qs 'bin/osd volume' /etc/sway/config.d/21-volume.conf ||
	_osd="$_osd config.d/21-volume.conf"
grep -qs 'bin/osd brightness' /etc/sway/config.d/20-brightness.conf ||
	_osd="$_osd config.d/20-brightness.conf"
for _i in audio-volume-muted audio-volume-low audio-volume-high display-brightness; do
	[ -r "/usr/share/icons/LazyOS/48x48/apps/$_i.png" ] || _osd="$_osd icon:$_i"
done
if [ -z "$_osd" ]; then
	ok "volume and brightness keys draw an overlay (bin/osd, three volume icons, the theme's brightness icon)"
else
	fail "the volume/brightness overlay is incomplete:$_osd"
fi
if [ -r "$HOME/.config/mako/config" ]; then
	if grep -qs '^icons=1' "$HOME/.config/mako/config" &&
		grep -qs '^progress-color=' "$HOME/.config/mako/config"; then
		ok "the overlay can carry its icon and its level bar (mako icons=1, progress-color)"
	else
		warn "mako/config loses icons=1 or progress-color: the overlay shows text only"
	fi
fi

# Both steps have to be in a form brightnessctl accepts.  A delta written with a
# leading hyphen is read by its getopt as an option ("invalid option -- '5'"), so
# `osd brightness down` returns 1, changes nothing and raises the error card while
# `up` works: one direction only, and the helper reads perfectly.  The stub
# records the arguments the helper really passes, so nothing is written and the
# helper's own code is what runs.
_osd_tmp=$(mktemp -d "${TMPDIR:-/tmp}/lazyos-osd.XXXXXX" 2>/dev/null || true)
if [ -n "$_osd_tmp" ] && [ -x "$HOME/.config/bin/osd" ]; then
	printf '%s\n' '#!/bin/sh' 'printf "%s\n" "$*" >>"$OSD_TEST_LOG"' >"$_osd_tmp/brightnessctl"
	printf '%s\n' '#!/bin/sh' 'exit 0' >"$_osd_tmp/notify-send"
	chmod +x "$_osd_tmp/brightnessctl" "$_osd_tmp/notify-send"
	(
		export PATH="$_osd_tmp:$PATH" OSD_TEST_LOG="$_osd_tmp/args"
		: >"$_osd_tmp/args"
		"$HOME/.config/bin/osd" brightness down >/dev/null 2>&1 || true
		"$HOME/.config/bin/osd" brightness up >/dev/null 2>&1 || true
	)
	_osd_steps=$(grep -- '^set ' "$_osd_tmp/args" | tr '\n' ' ')
	_osd_steps=${_osd_steps% }
	if grep -q -- '^set -' "$_osd_tmp/args"; then
		fail "bin/osd passes brightnessctl a step starting with a hyphen ($_osd_steps): getopt reads it as an option, so that direction does nothing"
	elif [ "$(grep -c -- '^set ' "$_osd_tmp/args")" -lt 2 ]; then
		fail "bin/osd asks brightnessctl for fewer than two steps (${_osd_steps:-none}): a key changes nothing"
	else
		ok "both brightness directions pass a step brightnessctl accepts ($_osd_steps)"
	fi
	rm -rf "$_osd_tmp"
fi

# The night light is one helper now: the login line, the Settings switch and
# the after-resume hook all ask it, so a raw `exec wlsunset` and the switch's own
# copy of the coordinates are both regressions. The resume has to run after the
# screensaver kill, and that kill has to be bracketed: swayidle runs the chain
# with `/bin/sh -c`, whose own command line contains the pattern, so
# `pkill -f org.lazyos.screensaver` kills the shell before `night-light resume`
# runs and the screen wakes at daylight colour while the switch says on.
_nl_cfg=/etc/sway/config.d/23-lazyos-screensaver.conf
_night_light=""
[ -x "$HOME/.config/bin/night-light" ] || _night_light="$_night_light bin/night-light"
grep -qs '^exec .*/night-light login' /etc/sway/config ||
	_night_light="$_night_light config(night-light-login)"
grep -qs 'after-resume .*night-light resume' "$_nl_cfg" ||
	_night_light="$_night_light config.d/23-lazyos-screensaver.conf(resume)"
grep -qs 'night-light" start' "$HOME/.config/bin/rofi-quick" ||
	_night_light="$_night_light bin/rofi-quick(start)"
if [ -x "$HOME/.config/bin/night-light" ]; then
	case $("$HOME/.config/bin/night-light" status 2>/dev/null || true) in
	on\ *|off|off\ *) ;;
	*) _night_light="$_night_light bin/night-light(status)" ;;
	esac
fi
if [ -z "$_night_light" ]; then
	ok "night light wired (helper, login line, after-resume, Settings switch)"
else
	fail "night light incomplete:$_night_light (the screen never warms, or it stays at daylight after a resume)"
fi
if grep -qs '^exec .*wlsunset' /etc/sway/config; then
	fail "/etc/sway/config starts wlsunset directly (the night-light helper is bypassed: the switch and the after-resume hook do not know the mode)"
fi
if grep -q 'after-resume .*pkill -f org\.lazyos\.screensaver' "$_nl_cfg" 2>/dev/null; then
	fail "idle chain uses an unbracketed pkill -f org.lazyos.screensaver (it matches its own sh -c and kills the chain before night-light resume)"
fi

# The bug the guard in lazyos-screensaver exists for: a saver left running under
# an active lock animates to nobody and spends CPU (it used to do exactly that
# for the 150 s between the two timers).
if pgrep -x swaylock >/dev/null 2>&1 && pgrep -f org.lazyos.screensaver >/dev/null 2>&1; then
	warn "the idle screen is running under swaylock (invisible CPU; it should have refused)"
fi

# The CLI tools of packages/tools.list, wired by /etc/profile.d/90-cli-tools.sh
# and ~/.bashrc.  A missing binary is the quietest kind of breakage: the alias is
# simply not defined and `ls` silently falls back to coreutils, so this is a FAIL
# and not a warning.
_missing_tools=""
for _c in eza bat fd rg fzf zoxide starship tldr btop fastfetch inxi tmux \
	lazygit gum curl unzip whois socat inotifywait qrencode zbarimg tesseract \
	ffmpeg node npm col man; do
	have "$_c" || _missing_tools="$_missing_tools $_c"
done
[ -r "$HOME/.bashrc" ] || _missing_tools="$_missing_tools .bashrc"
[ -r /etc/profile.d/90-cli-tools.sh ] || _missing_tools="$_missing_tools profile.d/90-cli-tools.sh"
[ -r "$HOME/.config/starship.toml" ] || _missing_tools="$_missing_tools .config/starship.toml"
if [ -z "$_missing_tools" ]; then
	ok "CLI tools complete (eza, bat, fd, rg, fzf, zoxide, starship, btop, ...)"
else
	fail "CLI tools incomplete:$_missing_tools"
fi

# The prompt is a page of private-use codepoints, and a glyph no installed font
# draws is a missing-glyph box in the terminal - no log line, no error, and the
# same prompt can look whole in one emulator and broken in another.  That is not
# hypothetical: the `$os` block of the upstream prompt asked for Nerd Fonts v3
# `nf-md` glyphs (Linux U+F033D, Debian U+F08DA, ...), which live in plane 15 and
# which no package in forky provides because fonts-nerd-symbols is DFSG-stripped;
# 14 of its 44 codepoints had no font at all.  The quieter half is the second
# question: a codepoint that only some *other* family has is drawn from
# fontconfig's dynamic fallback at that family's size, so the icon comes out
# smaller than the cell next to it (foot draws the listed fonts first, in order).
# Both are asked of fontconfig, per codepoint, against the chain foot is given.
if [ ! -r "$HOME/.config/starship.toml" ]; then
	: # the CLI tools check above already FAILs on the missing file
elif ! have fc-list || ! have perl; then
	skip "fc-list or perl is missing: cannot confirm that every prompt glyph has a font"
else
	_star_cps=$(perl -CSD -ne 'while (/([\x{E000}-\x{F8FF}\x{F0000}-\x{FFFFD}])/g) { printf "%X\n", ord($1) }' \
		"$HOME/.config/starship.toml" 2>/dev/null | sort -u)
	_star_footfonts=$(sed -n 's/^[[:space:]]*font[[:space:]]*=[[:space:]]*//p' "$HOME/.config/foot/foot.ini" 2>/dev/null |
		tr ',' '\n' | sed 's/:.*//; s/^[[:space:]]*//; s/[[:space:]]*$//')
	_star_nofont= _star_offchain= _star_count=0
	for _cp in $_star_cps; do
		_star_count=$((_star_count + 1))
		_star_fams=$(fc-list ":charset=$_cp" family 2>/dev/null | cut -d, -f1 | sort -u)
		if [ -z "$_star_fams" ]; then
			_star_nofont="$_star_nofont U+$_cp"
			continue
		fi
		_star_listed=no
		while IFS= read -r _f; do
			[ -n "$_f" ] || continue
			if grep -qxF -- "$_f" <<<"$_star_fams"; then
				_star_listed=yes
				break
			fi
		done <<<"$_star_footfonts"
		[ "$_star_listed" = yes ] ||
			_star_offchain="$_star_offchain U+$_cp($(paste -sd/ - <<<"$_star_fams"))"
	done
	if [ -n "$_star_nofont" ]; then
		fail "the prompt asks for glyph(s) no installed font draws:$_star_nofont (fc-list :charset= answers nothing, so the terminal can only draw a missing-glyph box)"
	elif [ -n "$_star_offchain" ]; then
		warn "prompt glyph(s)$_star_offchain come from a family $HOME/.config/foot/foot.ini does not list: foot draws them from fontconfig's fallback at that family's size, so they render small (add the family to font=)"
	else
		ok "all $_star_count private-use glyphs of the prompt are drawn by fonts foot is given"
	fi
fi

# Non-Latin coverage in one question.  A file name, a web page or an EPUB in
# Arabic is a row of boxes when no installed font claims the script, and nothing
# logs it.  fonts-noto-core is the package that answers for Arabic, Hebrew,
# Devanagari, Thai and the rest of the non-CJK set; CJK is a documented
# `apt install` in 04-software.md (fonts-noto-cjk is 91 MB), so its absence is
# not a failure here.
if have fc-list; then
	_nl=$(fc-list :lang=ar 2>/dev/null)
	if [ -n "$_nl" ]; then
		ok "non-Latin text has a font (Arabic, Hebrew, Indic; CJK is an install away)"
	else
		warn "fc-list :lang=ar finds nothing: Arabic, Hebrew and Indic text render as boxes (package fonts-noto-core)"
	fi
fi

# The editors. /usr/bin/vi is the part that goes missing on its own: it is an
# update-alternatives link that vim's postinst registers, so a `vim` installed
# without its postinst running leaves `vim` working and `vi` absent, which only
# the script that calls vi notices.
_editors=""
for _c in nvim vim vi nano; do
	have "$_c" || _editors="$_editors $_c"
done
if [ -z "$_editors" ]; then
	ok "editors installed (nvim, vim, vi, nano)"
else
	fail "editor(s) missing:$_editors"
fi
# $EDITOR is what eff, `git commit` and `crontab -e` open, and it comes from
# /etc/profile.d/90-cli-tools.sh - which a session started by greetd does not
# source, so the question is asked of a login shell instead of this environment.
_editor=$(bash -lc 'printf %s "${EDITOR:-}"' 2>/dev/null || true)
if [ "$_editor" = nvim ]; then
	ok "EDITOR is nvim (eff, git commit, crontab -e)"
else
	warn "a login shell has EDITOR='${_editor:-unset}': eff and git open nano instead of nvim"
fi

# `ls` is the one people notice.  Ask an interactive bash what the alias is -
# that also proves ~/.bashrc is read at all - and match with a case statement:
# `producer | grep -q` is the pipefail trap this script documents at the top.
_ls_alias="$(bash -ic 'alias ls' 2>/dev/null || true)"
case "$_ls_alias" in
	*eza*) ok "ls is eza in an interactive shell" ;;
	*) warn "ls is not eza (alias: ${_ls_alias:-none}); ~/.bashrc is not being read" ;;
esac
# zoxide appends its hook to PROMPT_COMMAND and starship replaces it, so the
# init has to come last or the first `cd -` prints "zoxide: detected a possible
# configuration issue" and no jump is recorded. awk exits on the first match
# instead of feeding a pipe, so the script's pipefail rule is not at risk.
_zhook=$(awk '/zoxide init bash/ { print NR; exit }' "$HOME/.bashrc" 2>/dev/null || true)
_zstarship=$(awk '/starship init bash/ { print NR; exit }' "$HOME/.bashrc" 2>/dev/null || true)
if [ -n "$_zhook" ] && [ -n "$_zstarship" ] && [ "$_zhook" -gt "$_zstarship" ] &&
	grep -qs 'builtin cd -' "$HOME/.bashrc"; then
	ok "zoxide initialises after starship and cd - stays bash's (hook survives)"
else
	warn "~/.bashrc initialises zoxide before starship or lost zd's cd -: zoxide's hook is dropped"
fi

# The launcher is rofi, themed by two files that the launcher, the window
# switcher and the bar's dmenu all read. Without them rofi still opens, in its
# default colours, which looks like a themed launcher that lost its theme rather
# than a missing file.
if have rofi && [ -r "$HOME/.config/rofi/config.rasi" ] && [ -r "$HOME/.config/rofi/solarized.rasi" ]; then
	ok "launcher complete (rofi, config.rasi, solarized.rasi)"
else
	fail "launcher incomplete: rofi, ~/.config/rofi/config.rasi or solarized.rasi missing"
fi
# rofi is also the bar's dmenu: without it every bar click is a silent no-op.
[ -x "$HOME/.config/bin/bar-pick" ] && ok "bar-pick (rofi -dmenu) is executable" ||
	fail "bin/bar-pick is missing or not executable"

# Every Alt+1 second action in these menus (delete a clipboard entry, unmount a
# disk, act on a file row) is rofi's -kb-custom-1, and sway would answer Alt+1
# itself with "workspace number 1" before rofi ever saw the key: a layer surface
# with exclusive keyboard interactivity does not stop compositor bindings, only
# an active zwp_keyboard_shortcuts_inhibitor does, and rofi asks for one only
# when `global-kb` is on. Off, the menu switches workspace and does nothing -
# there is no error anywhere, which is why it is checked and not assumed.
if [ -r "$HOME/.config/rofi/config.rasi" ]; then
	if grep -qs '^[[:space:]]*global-kb:[[:space:]]*true' "$HOME/.config/rofi/config.rasi"; then
		ok "rofi inhibits the compositor's shortcuts while a menu is open (global-kb)"
	else
		warn "rofi/config.rasi does not set global-kb: true - Alt+1 inside a menu is taken by sway's workspace binding"
	fi
fi

# The Search key is the launcher and Alt+Search is the file search; they used to
# be the other way around, so a key that goes back to the other menu no longer
# does what the manual says. The file search needs all three halves asked for:
# the --search mode of bin/rofi-files (without it the key exits with a usage
# error and looks dead) and both press paths of the chord (keyd turns Alt-first
# into F20; the release binding catches Search-first).
if grep -qs 'Super_L exec .*\$launcher' "$HOME/.config/sway/config"; then
	ok "the Search key opens the applications launcher (\$launcher)"
else
	fail "the Search key does not run '\$launcher' in ~/.config/sway/config (it opens the file search, or nothing)"
fi
if grep -qs 'F20 exec .*rofi-files --search' "$HOME/.config/sway/config" &&
	grep -qs 'Super_L+Mod1 exec .*rofi-files --search' "$HOME/.config/sway/config"; then
	ok "Alt+Search opens the file search (rofi-files --search)"
else
	fail "Alt+Search is missing its F20 press binding or its Super_L+Mod1 release fallback in ~/.config/sway/config"
fi
grep -qs -- '--search)' "$HOME/.config/bin/rofi-files" ||
	fail "bin/rofi-files has no --search mode: Alt+Search exits with a usage error"

# Alt+D stays the launcher even though the Search key opens the same menu: an
# overlay that unbinds it takes away the launcher key that does not go through
# keyd, and it reads as a key that does nothing.
if grep -qs '^unbindsym Mod1+d' "$HOME/.config/sway/config"; then
	fail "~/.config/sway/config still unbinds Alt+d: the launcher is gone"
else
	ok "Alt+d is not unbound (the launcher stays on \$mod+d)"
fi
grep -qs 'bindsym \$mod+d exec' /etc/sway/config.d/19-launcher.conf ||
	fail "/etc/sway/config.d/19-launcher.conf has no \$mod+d binding: Alt+d has nothing to run"

# The file search needs fd; without it rofi-files says so and browses $HOME instead,
# which looks like a search that found nothing.
if have fd || have fdfind; then
	ok "file search available (fd), which rofi-files --search runs"
else
	fail "fd is missing (package fd-find): rofi-files --search falls back to browsing"
fi

# The terminal file search (Search+Shift) asks the same question with fzf:
# lazyos-find streams fd into fzf and hands the choice to lazyos-view. There are
# four separate ways for that key to look dead - no binding, a script without its
# executable bit, no fzf, no opener - and the fzf one is a FAIL rather than a
# warning because a missing filter is an *empty list*: the window opens, prints
# nothing, and reads as "no such file" instead of as a missing package.
if grep -qs 'Super_L+Shift exec .*lazyos-find' "$HOME/.config/sway/config"; then
	ok "Search+Shift opens the terminal file search (lazyos-find)"
else
	fail "no 'Super_L+Shift exec ...lazyos-find' binding in ~/.config/sway/config: the key does nothing"
fi
if grep -qs '^leftmeta = f20$' /etc/keyd/default.conf; then
	ok "keyd maps Alt+Search to F20"
else
	fail "/etc/keyd/default.conf does not map leftmeta = f20: holding Alt and tapping Search does nothing"
fi
if [ -x "$HOME/.config/bin/lazyos-find" ]; then
	ok "terminal file search (lazyos-find) is executable"
else
	fail "bin/lazyos-find is missing or not executable: Search+Shift opens a foot window that closes at once"
fi
if have fzf; then
	ok "fzf present, which the terminal file search filters with"
else
	fail "fzf is missing (package fzf): Search+Shift lists nothing"
fi
# Its notifications carry the search glyph, and a name mako cannot resolve draws
# no picture at all: the card is text-only with no log line.
[ -r /usr/share/icons/LazyOS/48x48/apps/system-search.png ] ||
	warn "the LazyOS theme has no system-search icon: lazyos-find's notifications are text-only"

# Search inside files (Alt+Ctrl+G) is the other half of the file search (Alt+Search):
# rofi-files finds a file by its name, rofi-grep by what is written in it. Two
# things take it away and neither says so - a missing binding (the key does
# nothing) or a missing ripgrep (the menu opens, refuses, and reads as "no
# matches").
if grep -rqs 'Ctrl+g exec .*rofi-grep' /etc/sway/config.d/; then
	ok "Alt+Ctrl+G searches inside the files (rofi-grep)"
else
	fail "no 'Ctrl+g exec ...rofi-grep' binding in /etc/sway/config.d/: the key does nothing"
fi
have rg && ok "ripgrep present, which the search inside files runs" ||
	fail "ripgrep is missing (package ripgrep): Alt+Ctrl+G finds nothing"
# Enter opens the match at its line. less cannot seek a pipe, so bat writes the
# page to a real file; piping bat into less answered "Cannot seek to line number
# N" and left the match below the screen.
if grep -qs 'lazyos-grep.XXXXXX' "$HOME/.config/bin/rofi-grep" &&
	grep -qs 'less -R "+$1" "$2"' "$HOME/.config/bin/rofi-grep"; then
	ok "a match opens at its line (less seeks a real page file)"
else
	warn "bin/rofi-grep no longer gives less a seekable file: the page starts at the top"
fi

# Download (the root menu and the launcher) is yt-dlp plus the two things that
# make it useful: ffmpeg to merge a video stream with its audio, and node as the
# JavaScript runtime /etc/yt-dlp.conf names for YouTube's challenges. Without node
# yt-dlp still runs and warns that "some formats may be missing", which reads as a
# site problem instead of a missing package.
_missing_dl=""
for _c in yt-dlp ffmpeg node; do have "$_c" || _missing_dl="$_missing_dl $_c"; done
[ -x "$HOME/.config/bin/rofi-download" ] || _missing_dl="$_missing_dl bin/rofi-download"
if [ -z "$_missing_dl" ]; then
	ok "download complete (yt-dlp, ffmpeg, node, rofi-download)"
else
	fail "download incomplete:$_missing_dl (the row opens a menu that cannot fetch)"
fi
grep -qs 'js-runtimes node' /etc/yt-dlp.conf && ok "yt-dlp is told to use node (JS runtimes)" ||
	warn "/etc/yt-dlp.conf does not name node: yt-dlp logs 'JS runtimes: none' and YouTube may withhold formats"

# The home folders the manual and the menus name (Documents, Downloads, Music,
# Videos) are created by rootfs-setup.sh rather than shipped in the tree, because
# git cannot carry an empty directory. A missing Downloads only moves the download
# (rofi-download creates it), so this warns instead of failing.
_missing_home=""
for _d in Documents Downloads Music Videos; do
	[ -d "$HOME/$_d" ] || _missing_home="$_missing_home $_d"
done
[ -z "$_missing_home" ] && ok "the home folders are there (Documents, Downloads, Music, Videos)" ||
	warn "home folders missing:$_missing_home (rootfs-setup.sh creates them; a download recreates Downloads)"

# Alt+1 in the file browser is rofi's -kb-custom-1 and needs two halves: the flag
# in rofi-files and bin/rofi-file-actions behind it. Without the flag rofi exits 0
# and Enter's action runs instead - the row opens when the person asked to act on
# it - which is silent.
if grep -qs -- '-kb-custom-1 Alt+1' "$HOME/.config/bin/rofi-files"; then
	ok "the file browser passes Alt+1 through to rofi (-kb-custom-1)"
else
	warn "bin/rofi-files no longer passes -kb-custom-1 Alt+1: Alt+1 opens the row instead of acting on it"
fi
# The browser remembers the last directory in $XDG_STATE_HOME/lazyos/files-last.
# Two edits make it silently forget: dropping the state path (every Alt+Ctrl+F
# starts in $HOME again) or removing the --search guard (a search overwrites the
# memory, so the browser goes home after it).
if grep -qs 'files-last' "$HOME/.config/bin/rofi-files" &&
	grep -qs 'XDG_STATE_HOME' "$HOME/.config/bin/rofi-files" &&
	grep -qs 'from_search' "$HOME/.config/bin/rofi-files"; then
	ok "the file browser remembers its directory (\$XDG_STATE_HOME/lazyos/files-last)"
else
	warn "bin/rofi-files has no files-last memory: every Alt+Ctrl+F starts in \$HOME"
fi
_missing_actions=""
for _c in tar unzip ffmpeg udisksctl; do have "$_c" || _missing_actions="$_missing_actions $_c"; done
[ -x "$HOME/.config/bin/rofi-file-actions" ] || _missing_actions="$_missing_actions bin/rofi-file-actions"
if [ -z "$_missing_actions" ]; then
	ok "the file actions have their tools (tar, unzip, ffmpeg, udisksctl)"
else
	fail "file actions incomplete:$_missing_actions (Alt+1 offers rows that cannot run)"
fi

# The Developer menu (Alt+Ctrl+D, rofi-dev, the root menu's Developer row and
# org.lazyos.dev.desktop) exports ~/.ssh, ~/.gnupg and the dotfiles into an
# encrypted archive, restores one, and writes a diagnostics bundle. Every piece
# is needed at a different moment: a missing gpg fails after the passphrase, a
# missing tar/zstd fails before it, and a missing udisksctl means a stick that is
# not mounted yet cannot be offered.
_missing_dev=""
for _c in tar zstd gpg udisksctl; do have "$_c" || _missing_dev="$_missing_dev $_c"; done
for _s in rofi-dev lazyos-profile; do
	[ -x "$HOME/.config/bin/$_s" ] || _missing_dev="$_missing_dev bin/$_s"
done
if [ -z "$_missing_dev" ]; then
	ok "developer menu complete (tar, zstd, gpg, udisksctl, rofi-dev, lazyos-profile)"
else
	fail "developer menu incomplete:$_missing_dev (Alt+Ctrl+D loses those actions)"
fi

# The passphrase pipeline is the one part that can break without a binary going
# missing: gpg 2.1 and later ignore --passphrase-fd unless the pinentry is told
# to loop back, and the symptom is a gpg that waits for an agent dialog that
# nobody is there to answer. This runs the exact command the menu runs - AES256,
# passphrase through fd 3 - on a throwaway passphrase and two bytes, so the
# encryption is proven without touching the account's keys.
if have gpg && _dev_tmp=$(mktemp -d "${TMPDIR:-/tmp}/lazyos-gpg.XXXXXX"); then
	_dev_agent=
	if have systemctl; then
		[ "$(systemctl --user is-active gpg-agent.service 2>/dev/null)" = active ] && _dev_agent=was
	fi
	(umask 077; printf 'lazyos-check\n' >"$_dev_tmp/pass")
	printf 'lazyos' >"$_dev_tmp/plain"
	_dev_gpg=ok
	gpg --batch --yes --pinentry-mode loopback --passphrase-fd 3 \
		--cipher-algo AES256 --s2k-digest-algo SHA512 -c \
		-o "$_dev_tmp/plain.gpg" "$_dev_tmp/plain" 3<"$_dev_tmp/pass" 2>/dev/null || _dev_gpg=
	gpg --batch --yes --pinentry-mode loopback --passphrase-fd 3 \
		-d -o "$_dev_tmp/plain.out" "$_dev_tmp/plain.gpg" 3<"$_dev_tmp/pass" 2>/dev/null || _dev_gpg=
	[ "$(cat "$_dev_tmp/plain.out" 2>/dev/null)" = lazyos ] || _dev_gpg=
	if [ -n "$_dev_gpg" ]; then
		ok "gpg round-trips the Developer menu's passphrase pipeline (AES256, fd 3)"
	else
		fail "gpg cannot round-trip with --pinentry-mode loopback --passphrase-fd: the profile export would hang"
	fi
	rm -rf "$_dev_tmp"
	# Only when this check started it: an agent the user was already running
	# (a signed commit, an ssh-agent session) is theirs to keep.
	if [ -z "$_dev_agent" ] && have gpgconf; then
		gpgconf --kill gpg-agent >/dev/null 2>&1 || :
	fi
fi

# The two lists the export is built from are the feature: the paths travel in
# the tar, and the dry run prints them without writing anything. A path that
# leaves the list stops being saved, silently.
if [ -x "$HOME/.config/bin/lazyos-profile" ]; then
	_dev_dry=$("$HOME/.config/bin/lazyos-profile" export --dry-run 2>/dev/null || :)
	# Both names have to be in it, not just one of them.
	_dev_paths=ok
	case $_dev_dry in *".ssh"*) ;; *) _dev_paths= ;; esac
	case $_dev_dry in *".gnupg"*) ;; *) _dev_paths= ;; esac
	if [ -n "$_dev_paths" ]; then
		ok "the profile export still includes ~/.ssh and ~/.gnupg (export --dry-run)"
	else
		fail "bin/lazyos-profile export --dry-run no longer names ~/.ssh and ~/.gnupg: the keys would stop travelling"
	fi
	# The agents are half of what "the account" means now: the menu's account
	# directories and the CLIs' own homes. The inclusion test is a whole-line
	# fixed-string match, because the exclusions below it also name the path and a
	# substring test would pass with the entry gone.
	_dev_agents=ok
	for _p in .claude .codex .gemini .copilot .config/github-copilot \
		.local/share/lazyos/agents .config/opencode; do
		grep -qsxF -- "$_p" "$HOME/.config/bin/lazyos-profile" || _dev_agents=
	done
	grep -qsF -- "--exclude='.claude/projects'" "$HOME/.config/bin/lazyos-profile" ||
		_dev_agents=
	if [ -n "$_dev_agents" ]; then
		ok "the profile export covers the agents' homes and account directories (transcripts excluded)"
	else
		fail "bin/lazyos-profile lost an agent path or its transcript excludes: sign-ins or 100s of MB of history"
	fi
	# Firefox is the other half of "the account": bookmarks, passwords and
	# add-ons travel, the site data (202 MB here), the session and the SQLite
	# checkpoints do not. Both roots are in ENTRIES because the XDG path is what
	# Debian's Firefox uses here and MOZ_LEGACY_HOME=1 moves it to ~/.mozilla; one
	# `*mozilla/firefox/*` exclude pattern covers either.
	_dev_ff=ok
	for _p in .config/mozilla .mozilla; do
		grep -qsxF -- "$_p" "$HOME/.config/bin/lazyos-profile" || _dev_ff=
	done
	grep -qsF -- "--exclude='*mozilla/firefox/*/storage'" "$HOME/.config/bin/lazyos-profile" || _dev_ff=
	if [ -n "$_dev_ff" ]; then
		ok "the profile export covers the Firefox profile (site data, session and caches excluded)"
	else
		fail "bin/lazyos-profile lost the Firefox profile or its storage/cache excludes: bookmarks and passwords or 200 MB of site data"
	fi
	# A passphrase prompt that lands on the VT is worse than no prompt at all:
	# the session shows a busy card and nothing ever asks. `[ -t 0 ]` alone was
	# that bug - sway's exec inherits /dev/tty7 - so the foreground-group test
	# and the rofi fallback are part of the feature, not an implementation
	# detail.
	if grep -qs 'tpgid' "$HOME/.config/bin/lazyos-profile" &&
		grep -qs 'in_session' "$HOME/.config/bin/lazyos-profile"; then
		ok "the passphrase prompt cannot land on an invisible terminal (tpgid test, rofi fallback)"
	else
		fail "bin/lazyos-profile asks the passphrase with a bare [ -t 0 ]: from a key the prompt goes to the VT and the export hangs"
	fi
fi

# Three places reach the menu and one is enough to lose: the root menu's row
# (bar-menu), the key (24-keys.conf) and the launcher entry (whose Exec is a
# $HOME script, so its execute bit is also part of it).
_dev_reach=ok
grep -qs 'rofi-dev' "$HOME/.config/bin/bar-menu" || _dev_reach=
grep -rqs 'Ctrl+d exec .*rofi-dev' /etc/sway/config.d/ || _dev_reach=
[ -r /usr/share/applications/org.lazyos.dev.desktop ] || _dev_reach=
if [ -n "$_dev_reach" ]; then
	ok "the Developer menu is reachable three ways (root menu, Alt+Ctrl+D, launcher)"
else
	fail "the Developer menu lost the root-menu row, the Alt+Ctrl+D binding or org.lazyos.dev.desktop"
fi
grep -qs 'Icon=applications-development' /usr/share/applications/org.lazyos.dev.desktop 2>/dev/null &&
	[ -r /usr/share/icons/LazyOS/48x48/apps/applications-development.png ] &&
	ok "the Developer launcher entry asks for a picture the theme carries" ||
	warn "org.lazyos.dev.desktop has no applications-development icon: the launcher row is a hole"

# The manual is where the passphrase recipe and the restore steps live; the
# section is one heading, and losing it makes the feature undiscoverable from a
# terminal.
grep -qs 'Your profile, on a stick' /usr/share/doc/lazyos/04-software.md &&
	ok "the manual has the profile section (04-software.md)" ||
	warn "04-software.md lost 'Your profile, on a stick': the export and restore are undocumented"

# Format a disk writes a partition table and an exFAT or ext4 filesystem, so both
# halves have to be there before the menu is allowed to list anything - and the
# menu is reached from Places, where a missing script is a row that does nothing.
# The list is also the safety (ui_disk_system in lib-rofi.sh), so the guard is
# witnessed below: without it the internal eMMC is offered again.
_missing_fmt=""
for _c in parted mkfs.exfat mkfs.ext4; do have "$_c" || _missing_fmt="$_missing_fmt $_c"; done
[ -x "$HOME/.config/bin/rofi-format" ] || _missing_fmt="$_missing_fmt bin/rofi-format"
if [ -z "$_missing_fmt" ]; then
	ok "format a disk complete (parted, mkfs.exfat, mkfs.ext4, rofi-format)"
else
	fail "format a disk incomplete:$_missing_fmt (the Places row does nothing)"
fi

# A fixed disk and the disk the system runs from must never be offered, and that
# is one function plus one last gate in each of the two menus that repartition a
# disk. The eMMC is the case that matters: it is not mounted as a whole and, with
# its partition table wiped, it carries no ChromeOS kernel partition either, so
# without this guard it would be offered for formatting.
_fmt_guard=ok
grep -qs 'ui_disk_system()' "$HOME/.config/bin/lib-rofi.sh" || _fmt_guard=
grep -qs 'device/removable' "$HOME/.config/bin/lib-rofi.sh" || _fmt_guard=
grep -qs 'kern_guid' "$HOME/.config/bin/lib-rofi.sh" || _fmt_guard=
grep -qs 'ui_disk_system "\$name"' "$HOME/.config/bin/rofi-format" || _fmt_guard=
grep -qs 'ui_disk_system "\$name"' "$HOME/.config/bin/rofi-swap" || _fmt_guard=
if [ -n "$_fmt_guard" ]; then
	ok "format a disk refuses fixed storage and the disk this system runs from (ui_disk_system)"
else
	fail "the disk guard is gone from lib-rofi.sh or from one of the two disk menus: the internal eMMC can be offered for formatting"
fi

# The two disk menus test their tools with `command -v` before they draw a row,
# and greetd's session does not export the sbin directories: without the PATH
# line in lib-rofi.sh, parted, mkswap, swapon, swapoff and blkid are all
# reported missing on a machine that has every one of them (the card answered
# "Missing: parted mkswap swapon swapoff blkid", and Format a disk had the same
# hole). This asks the shared file in a session-shaped environment, which is the
# thing that breaks, instead of trusting that the line is still there; a tool
# that is really absent is the FAIL below.
if ! have mkswap; then
	skip "disk menu PATH not asked (mkswap is not installed; the next check fails)"
elif env -i HOME="$HOME" PATH=/usr/local/bin:/usr/bin:/bin \
	sh -c 'cd "$HOME/.config/bin" && . ./lib-rofi.sh && command -v mkswap' >/dev/null 2>&1; then
	ok "Auxiliary swap and Format a disk resolve the sbin tools (lib-rofi.sh adds /usr/sbin:/sbin)"
else
	fail "bin/lib-rofi.sh lost the /usr/sbin:/sbin PATH line: both disk menus report installed tools as missing"
fi

# Auxiliary swap (rofi-swap) writes a swap partition and the fstab block that
# activates it at boot. parted, mkswap, swapon/swapoff and blkid are the commands
# it runs; a missing one is a menu that fails halfway, and the two rows that reach
# it (Places and the root menu) are plain text that would still be drawn.
_missing_aux=""
for _c in parted mkswap swapon swapoff blkid; do
	have "$_c" || _missing_aux="$_missing_aux $_c"
done
[ -x "$HOME/.config/bin/rofi-swap" ] || _missing_aux="$_missing_aux bin/rofi-swap"
if [ -z "$_missing_aux" ]; then
	ok "auxiliary swap complete (parted, mkswap, swapon, blkid, rofi-swap)"
else
	fail "auxiliary swap incomplete:$_missing_aux (the menu cannot prepare or activate a card)"
fi

# The card is optional, so no block is OK, not a FAIL. When there is one, the
# options that keep a missing card from holding the boot are witnessed here:
# nofail and a device timeout. The priority has to stay below zram's 100, or the
# kernel round-robins between compressed RAM and the much slower card.
_aux_opts=$(awk '$1 == "LABEL=lazyos_swap" && $3 == "swap" { print $4; exit }' /etc/fstab 2>/dev/null)
if [ -z "$_aux_opts" ]; then
	ok "no auxiliary swap configured"
else
	_aux_bad=""
	case ",$_aux_opts," in *,nofail,*) ;; *) _aux_bad="$_aux_bad nofail" ;; esac
	case ",$_aux_opts," in
	*,x-systemd.device-timeout=*) ;;
	*) _aux_bad="$_aux_bad x-systemd.device-timeout" ;;
	esac
	_aux_pri=
	case ",$_aux_opts," in
	*,pri=*)
		_aux_pri=${_aux_opts#*pri=}
		_aux_pri=${_aux_pri%%,*}
		;;
	esac
	case ${_aux_pri:-} in
	'' | *[!0-9]*) _aux_bad="$_aux_bad pri=" ;;
	*) [ "$_aux_pri" -lt 100 ] || _aux_bad="$_aux_bad pri<100" ;;
	esac
	if [ -z "$_aux_bad" ]; then
		ok "auxiliary swap fstab block is safe for a missing card (nofail, device timeout, priority $_aux_pri < 100)"
	else
		warn "auxiliary swap fstab options unsafe:$_aux_bad (a boot without the card waits, or zram stops winning)"
	fi
	_aux_dev=$(asroot blkid -L lazyos_swap 2>/dev/null)
	if [ -z "$_aux_dev" ]; then
		skip "auxiliary swap configured, card not inserted (nofail keeps the boot clean)"
	else
		# One line per device and zram comes first, so join them before `case`:
		# a newline is not the space the pattern needs (this reported an active
		# card as inactive).
		_aux_swaps=$(swapon --show=NAME --noheadings 2>/dev/null | tr '\n' ' ' || true)
		case " $_aux_swaps " in
		*" $_aux_dev "*) ok "auxiliary swap active on $_aux_dev" ;;
		*) warn "auxiliary swap configured on $_aux_dev but not active: open Auxiliary swap and set it up again" ;;
		esac
	fi
fi

# The two entry points are static rows, so they can outlive the script. Places is
# the documented way in (the root menu's row is the second copy).
if [ -r "$HOME/.config/bin/rofi-places" ]; then
	grep -qs 'rofi-swap' "$HOME/.config/bin/rofi-places" &&
		ok "Places lists the Auxiliary swap row" ||
		fail "bin/rofi-places has no Auxiliary swap row: the disk menu is unreachable from Places"
fi

# Install LazyOS (the root menu's red row, the launcher entry, bin/rofi-install)
# is a front-end for scripts/install-emmc.sh: the menu asks the questions, the
# detached runner does the write by calling that script with CONFIRM=yes, and the
# copy that lands on the internal disk is the one this port has proven boots.
# Three things can therefore go wrong invisibly - a tool the installer needs is
# missing (the menu then shows a red row instead of starting a doomed write, but
# that row is only as good as this list), the runner grows a partitioning or
# filesystem command of its own (the second implementation this port must never
# have), and the typed confirmation turns back into a single Enter.
_install=""
for _c in rsync parted cgpt depthchargectl mkfs.ext2 mkfs.ext4 blkid blockdev \
	partx flock setsid; do
	have "$_c" || _install="$_install $_c"
done
for _s in rofi-install lazyos-install-run lazyos-install-log; do
	[ -x "$HOME/.config/bin/$_s" ] || _install="$_install bin/$_s"
done
if [ -z "$_install" ]; then
	ok "install to the internal disk complete (rsync, parted, cgpt, depthchargectl, and the menu with its runner)"
else
	fail "install to the internal disk incomplete:$_install (the menu refuses, or the write stops halfway)"
fi

# The delegation is the safety, so it is asked of the file and not assumed: the
# runner must call the installer with the CONFIRM the menu collected, and it must
# not carry a partitioning, filesystem or copy command of its own - it is allowed
# to prepare (stop udisks2, unmount) and to read the disk back, nothing else.
if [ -r "$HOME/.config/bin/lazyos-install-run" ]; then
	if grep -qs 'install-emmc.sh' "$HOME/.config/bin/lazyos-install-run" &&
		grep -qs 'CONFIRM=yes' "$HOME/.config/bin/lazyos-install-run"; then
		ok "the install runner delegates to scripts/install-emmc.sh with CONFIRM=yes"
	else
		fail "bin/lazyos-install-run no longer calls scripts/install-emmc.sh with CONFIRM=yes: the copy is not the one that is known to boot"
	fi
	_verbs=""
	for _v in parted mkfs cgpt rsync dd; do
		grep -qsE "(^|[^[:alnum:]_])$_v([^[:alnum:]_]|$)" "$HOME/.config/bin/lazyos-install-run" &&
			_verbs="$_verbs $_v"
	done
	if [ -z "$_verbs" ]; then
		ok "the install runner carries no partitioning, filesystem or copy command of its own"
	else
		fail "bin/lazyos-install-run contains$_verbs: the install has to be scripts/install-emmc.sh's, not a second one"
	fi
fi

# The last gate is typed, and the order of the two lines is what says so: Enter
# on a list row is exactly the gesture that must not erase a disk, so the menu
# has to ask for the target's name (bar-pick -free) and compare it before it
# starts the runner.
_gate=missing
if [ -r "$HOME/.config/bin/rofi-install" ]; then
	_gate=$(awk '
		/bar-pick/ && !asked { asked = NR }
		/setsid.*lazyos-install-run/ && !started { started = NR }
		END {
			if (!asked || !started) print "missing"
			else print (asked < started ? "before" : "after")
		}' "$HOME/.config/bin/rofi-install")
fi
case $_gate in
before) ok "the install menu asks for the disk's name typed in full before it starts the runner" ;;
*) fail "bin/rofi-install: the typed confirmation is $_gate the runner start (or one of the two is gone)" ;;
esac

# The root menu is the documented way in (the launcher entry is the second copy),
# and the log window is the only place the write's record can be read: the runner
# is detached, so no terminal outlives the menu that started it.  The row *text*
# is what is asked for, not the `case` arm: an arm whose row is gone leaves a menu
# that looks unchanged and an install nobody can reach (the dead-row check below
# covers the opposite direction).
grep -qs "'Install LazyOS'" "$HOME/.config/bin/bar-menu" &&
	ok "root menu has the Install LazyOS row" ||
	fail "bin/bar-menu has no Install LazyOS row: the install menu is only reachable by typing its name"
if grep -qs -- '--app-id=lazyos-install' "$HOME/.config/bin/lazyos-install-log" 2>/dev/null; then
	if grep -qs 'app_id="lazyos-install"' /etc/sway/config.d/31-install.conf 2>/dev/null; then
		ok "the install log opens in a floated foot window (app-id lazyos-install)"
	else
		warn "config.d/31-install.conf is missing or does not match app_id lazyos-install: the log opens tiled"
	fi
else
	fail "bin/lazyos-install-log lost --app-id=lazyos-install: its window cannot be matched or floated"
fi
[ -r /usr/share/icons/LazyOS/48x48/apps/system-software-install.png ] &&
	ok "the install icon is in the LazyOS theme (system-software-install)" ||
	warn "the LazyOS theme has no system-software-install.png: the launcher row and the install card have no picture"

# The Learn menu's second row is the machine's own documentation: apropos searches
# the whatis database man-db builds, and man renders the page. Without man-db both
# are absent and the row reports a search that found nothing.
if have apropos && have man; then
	ok "manual pages searchable (apropos, man), which the Learn menu's second row opens"
else
	fail "man-db is missing (apropos/man): the Learn menu cannot show a program's own manual"
fi

# Password (the root menu) is openssl's random bytes, and a missing openssl is a
# row that answers "openssl is not installed" instead of a password.
have openssl && ok "openssl present, which the Password row generates with" ||
	fail "openssl is missing (package openssl): the Password row cannot generate anything"

# dig is the first step of every DNS problem in the manual, and there is no
# resolvectl here (the resolver is NetworkManager's, not systemd-resolved's), so
# without bind9-dnsutils that section begins with "command not found".
if have dig; then
	ok "dig present (bind9-dnsutils), which DNS troubleshooting asks for"
else
	fail "dig is missing (package bind9-dnsutils): 06-troubleshooting.md's DNS section cannot run"
fi

# The manual, the key view, dictation and hibernate. The first two are pure
# scripts (a missing one is invisible: the menu entry just does nothing); the
# last two depend on hardware and packages, so they warn instead of failing.
_learn_pages=$(ls /usr/share/doc/lazyos/*.md 2>/dev/null | wc -l)
if [ "$_learn_pages" -ge 6 ] && [ -x "$HOME/.config/bin/rofi-learn" ]; then
	ok "manual installed ($_learn_pages pages) and rofi-learn present"
else
	fail "manual incomplete: $_learn_pages page(s) in /usr/share/doc/lazyos, rofi-learn $( [ -x "$HOME/.config/bin/rofi-learn" ] && echo present || echo missing)"
fi
[ -x "$HOME/.config/bin/rofi-keys" ] && ok "keybindings view (rofi-keys) is executable" ||
	fail "bin/rofi-keys is missing: the key view has no script"
[ -x "$HOME/.config/bin/rofi-files" ] && ok "file browser (rofi-files) is executable" ||
	fail "bin/rofi-files is missing: Alt+Ctrl+F and the Files menu entry do nothing"
# The translate key is taught in one place, the shortcuts page, and the Settings
# row only says what it does - not that it exists.  A manual that lost the row
# is a key nobody finds.
grep -qs 'Alt+Ctrl+T' /usr/share/doc/lazyos/03-shortcuts.md ||
	fail "03-shortcuts.md does not name Alt+Ctrl+T (the only place the translate key is taught)"

# Every menu behind the root menu is a rofi-* script. A missing one is the
# quietest failure in the session: the row is still listed (bar-menu draws it
# from a static list) and choosing it does nothing at all.
_missing_menus=""
for _s in rofi-quick rofi-notify rofi-clean rofi-kill rofi-tz rofi-places \
	rofi-media rofi-calc rofi-layout rofi-power rofi-about rofi-dns \
	rofi-pkg rofi-color rofi-remind lib-rofi.sh; do
	[ -r "$HOME/.config/bin/$_s" ] || _missing_menus="$_missing_menus bin/$_s"
done
if [ -z "$_missing_menus" ]; then
	ok "session menus complete (settings, notifications, free space, processes, time zone, places, media, calculator, layout, power, about, DNS, packages, colour, reminders)"
else
	fail "session menus missing:$_missing_menus (those root-menu rows do nothing)"
fi

# Presence is not enough: a script in bin/ without the execute bit makes its key
# do nothing at all, and nothing logs it (sway's `exec` gets EACCES and the line
# goes to the journal). Three menus of the batch that answered the card's report
# shipped that way - including this menu list, which passed because it asked for
# -r - so every file in bin/ is asked for -x here, once, instead of in each list
# above. lib-rofi.sh is sourced by the others, not run.
_noexec=""
for _s in "$HOME"/.config/bin/*; do
	[ -f "$_s" ] || continue
	case ${_s##*/} in lib-rofi.sh) continue ;; esac
	[ -x "$_s" ] || _noexec="$_noexec ${_s##*/}"
done
if [ -z "$_noexec" ]; then
	ok "every script in ~/.config/bin is executable"
else
	fail "not executable:$_noexec (the key or menu row that runs them does nothing)"
fi

# Reminders (Alt+Space > Reminders) are systemd transient timers, so what can
# silently take half the menu away is a missing systemd-analyze (every custom
# date is then refused while the five delays keep working) and a missing
# notification icon (the card has no picture and nothing logs it).
_reminder=""
have systemd-analyze || _reminder="$_reminder systemd-analyze"
have notify-send || _reminder="$_reminder notify-send"
have jq || _reminder="$_reminder jq"
[ -r /usr/share/icons/LazyOS/48x48/apps/preferences-system-time.png ] ||
	_reminder="$_reminder preferences-system-time.png"
[ -x "$HOME/.config/bin/rofi-remind" ] || _reminder="$_reminder bin/rofi-remind"
if [ -z "$_reminder" ]; then
	ok "reminders complete (systemd-analyze, notify-send, jq, the clock icon)"
else
	fail "reminders incomplete:$_reminder (custom dates or the notification icon disappear)"
fi
# The formats are what people ask about in the When menu, so it ends with a help
# row. Both halves are needed: the row in the list and the arm that answers it -
# a row whose arm is gone reads as a dead key.
if grep -qs 'How do I fill these in?' "$HOME/.config/bin/rofi-remind" &&
	grep -qs '^[[:space:]]*help)' "$HOME/.config/bin/rofi-remind"; then
	ok "the reminder When menu carries its format help row"
else
	warn "rofi-remind lost the 'How do I fill these in?' row or its help arm"
fi

# A repeat is a calendar expression handed to the timer itself, so the parser that
# has to read it is worth one call: this is the exact expression the menu writes
# for "every weekday", and a systemd without calendar support is a menu that
# refuses every date.
if have systemd-analyze; then
	if LC_ALL=C systemd-analyze calendar --iterations=1 'Mon..Fri *-*-* 08:30:00' >/dev/null 2>&1; then
		ok "systemd parses the calendar expressions the reminder menu writes"
	else
		fail "systemd-analyze cannot read 'Mon..Fri *-*-* 08:30:00' (repeating reminders would be refused)"
	fi
fi

# 02-rofi.md sends a reader to the root menu for DNS, Packages, Colour, About and
# Reminders. A row that goes missing is a documented keypress that answers
# nothing - Colour was the one that had neither the row nor the `case` arm.
if [ -r "$HOME/.config/bin/bar-menu" ]; then
	_root_missing=
	for _m in rofi-dns rofi-pkg rofi-color rofi-about rofi-remind rofi-swap; do
		grep -qs "$_m" "$HOME/.config/bin/bar-menu" || _root_missing="$_root_missing ${_m#rofi-}"
	done
	[ -z "$_root_missing" ] &&
		ok "root menu reaches DNS, Packages, Colour, About, Reminders and Auxiliary swap" ||
		fail "root menu has no entry for:$_root_missing (bar-menu)"
fi

# A row and its `case` arm are two halves of one entry; a row whose key has no
# arm closes the list and does nothing. Keys are the last word of a ui_row line,
# and the arms are indented inside the `case`. A row whose key is followed by a
# colour passes `"$UI_RED"` as its last word, so that one is read from the field
# before it - the earlier version took $NF always, which reported the Power row
# as dead, and matched arms only at column 0, which reported every row as dead.
if [ -r "$HOME/.config/bin/bar-menu" ]; then
	_dead_rows=$(awk '
		/^[[:space:]]*ui_row / {
			k = $NF
			if (k ~ /^\$|^"\$/) k = $(NF - 1)
			gsub(/"/, "", k)
			rows[k] = 1
		}
		/^[[:space:]]*[a-z][a-z-]*\)/ { k = $1; sub(/\).*/, "", k); arms[k] = 1 }
		END { for (k in rows) if (!(k in arms)) printf "%s ", k }' \
		"$HOME/.config/bin/bar-menu")
	[ -z "$_dead_rows" ] && ok "every root-menu row has an action" ||
		warn "root-menu rows with no action: $_dead_rows"
fi

# The key view draws one picture per row by name, and a dropped flag makes the
# list silently text-only - the same class of bug as the file browser's.
if [ -r "$HOME/.config/bin/rofi-keys" ] &&
	grep -qs -- '-show-icons' "$HOME/.config/bin/rofi-keys" &&
	grep -qs -- '-icon-theme LazyOS' "$HOME/.config/bin/rofi-keys"; then
	ok "rofi-keys asks the theme for a picture per row (-show-icons, LazyOS)"
else
	warn "rofi-keys no longer passes -show-icons/-icon-theme LazyOS: the key view shows no pictures"
fi
# And a name the theme does not carry is worse than a missing picture: rofi drops
# the icon column for that row alone, so its text starts 34 px to the left of its
# neighbours (that is what the key view looked like when only its section lines had
# one).  Every name the script can emit is asked of the theme: the pictures its
# icon() returns and the tool names it recognises in a command, both read out of the
# script itself - its awk returns them as `return "name"` and as one list in
# split("...", tl).
if [ -r "$HOME/.config/bin/rofi-keys" ]; then
	_keys_pics=$(sed -n 's/.*return "\([a-z0-9.-]*\)".*/\1/p' "$HOME/.config/bin/rofi-keys"
		sed -n 's/.*split("\([a-z0-9 .-]*\)", tl.*/\1/p' "$HOME/.config/bin/rofi-keys" | tr ' ' '\n')
	_keys_missing=""
	for _k in $_keys_pics; do
		[ -r "/usr/share/icons/LazyOS/48x48/apps/$_k.png" ] || _keys_missing="$_keys_missing $_k"
	done
	if [ -z "$_keys_pics" ]; then
		warn "rofi-keys names no pictures at all: the key view is text-only"
	elif [ -z "$_keys_missing" ]; then
		ok "the key view's pictures all resolve in the LazyOS theme (sections, actions and tools)"
	else
		fail "LazyOS theme has no key-view icon for:$_keys_missing (those rows lose the icon column)"
	fi
fi

# fastfetch exits when it has drawn, so without --hold the About window flashes
# and closes - which reads as "the row does nothing".
if [ -r "$HOME/.config/bin/rofi-about" ] && grep -qs -- '--hold' "$HOME/.config/bin/rofi-about"; then
	ok "rofi-about holds its fastfetch window open (--hold)"
else
	warn "rofi-about no longer passes --hold: the fastfetch picture flashes and closes"
fi
# The About windows stay open until dismissed, and the footer is the only
# instruction on the screen. The key it names must be one this keyboard
# answers: Ctrl+Shift+W (foot's built-in close) did not reach them; Alt+Q is
# sway's own `kill` and does.
_about_footer=$(grep -c 'close this window with Alt+Q' "$HOME/.config/bin/rofi-about" 2>/dev/null || true)
if [ "${_about_footer:-0}" -eq 3 ] && ! grep -qs 'Ctrl+Shift+W' "$HOME/.config/bin/rofi-about"; then
	ok "the About windows name a close key this keyboard answers (Alt+Q)"
else
	warn "rofi-about footers do not all say 'close this window with Alt+Q' (found ${_about_footer:-0}/3)"
fi

# The capture menu (Alt+Ctrl+C) is a list over external tools: grimshot for the
# screenshot, swappy to annotate, tesseract and zbarimg to read a region, and
# wf-recorder for the one action that outlives the menu. A missing binary is a
# row that does nothing when chosen, and a missing script is a key that does
# nothing at all, so both are FAILs.
_missing_capture=""
for _c in grimshot grim slurp tesseract zbarimg qrencode swappy wf-recorder; do
	have "$_c" || _missing_capture="$_missing_capture $_c"
done
for _s in rofi-capture rofi-qr screenshot; do
	[ -x "$HOME/.config/bin/$_s" ] || _missing_capture="$_missing_capture bin/$_s"
done
if [ -z "$_missing_capture" ]; then
	ok "capture menu complete (grimshot, swappy, tesseract, zbarimg, wf-recorder, qrencode)"
else
	fail "capture menu incomplete:$_missing_capture (Alt+Ctrl+C loses those rows)"
fi

# Screenshots land in ~/Pictures/Screenshots/<date>/, and grimshot only knows that
# when it is called through bin/screenshot: grimshot itself writes to $HOME on this
# image (there is no user-dirs.dirs), so one key that calls it directly drops the
# picture among the dotfiles again - which was the report.
grep -qs 'bin/screenshot' /etc/sway/config.d/90-screenshot-shortcuts.conf &&
	ok "the screenshot keys go through bin/screenshot" ||
	fail "90-screenshot-shortcuts.conf does not call bin/screenshot: the files land in \$HOME"
[ -z "$(grep -rs '^[[:space:]]*bindsym.*exec grimshot' /etc/sway/config.d/)" ] &&
	ok "no key calls grimshot directly" ||
	fail "a key calls grimshot directly: that screenshot lands in \$HOME"
grep -qs -- '"$dir/screenshot"' "$HOME/.config/bin/rofi-capture" &&
	ok "the capture menu saves through bin/screenshot" ||
	fail "rofi-capture calls grimshot directly: its screenshots land in \$HOME"
if [ -r /usr/share/applications/org.lazyos.screenshot-area.desktop ]; then
	grep -qs 'Exec=.*bin/screenshot' /usr/share/applications/org.lazyos.screenshot-area.desktop &&
		ok "the Screenshot (area) launcher entry uses bin/screenshot" ||
		fail "org.lazyos.screenshot-area.desktop still runs grimshot: that entry saves into \$HOME"
fi

# Screen sharing (Settings > Share this screen, and the root menu) is wayvnc
# served on the loopback address.  Three things fail quietly and one fails
# loudly, and only the last is visible: a missing binary (the menu says so when
# it is opened), a script without its execute bit (the row closes and nothing
# happens), and - the one that matters - a script that starts listening on the
# network with remote input enabled, which is an unauthenticated desktop on
# every network the machine joins.  `--disable-input` is the read-only decision
# and 127.0.0.1 is the authentication (wayvnc 0.10 only authenticates through a
# certificate and a password file this image does not ship), so both are asked
# of the file itself instead of being trusted to a comment.
_missing_vnc=""
have wayvnc || _missing_vnc="$_missing_vnc wayvnc"
[ -x "$HOME/.config/bin/rofi-vnc" ] || _missing_vnc="$_missing_vnc bin/rofi-vnc"
if [ -z "$_missing_vnc" ]; then
	ok "screen sharing complete (wayvnc, bin/rofi-vnc)"
else
	fail "screen sharing incomplete:$_missing_vnc (wayvnc is the server, bin/rofi-vnc is the menu)"
fi
if [ -r "$HOME/.config/bin/rofi-vnc" ]; then
	if grep -qs -- '--disable-input' "$HOME/.config/bin/rofi-vnc"; then
		ok "screen sharing is read-only (wayvnc --disable-input)"
	else
		fail "rofi-vnc no longer passes --disable-input: a viewer could type into this session"
	fi
	# A bare 0.0.0.0 (or a bind to the interface address) with no auth configured
	# is the failure this feature must not have.  Matching the literal address is
	# enough: the script has exactly one wayvnc invocation and check 1 fails
	# without the binary.
	if grep -qs '127\.0\.0\.1' "$HOME/.config/bin/rofi-vnc" &&
		! grep -qs '0\.0\.0\.0' "$HOME/.config/bin/rofi-vnc"; then
		ok "screen sharing listens on the loopback address only"
	else
		fail "rofi-vnc does not bind 127.0.0.1 only: that is an unauthenticated desktop on the network"
	fi
	# The tunnel command is the only way in, so it is not decoration: the row
	# copies what the manual prints, and both have to name the same port.
	if grep -qs 'ssh -L' "$HOME/.config/bin/rofi-vnc" &&
		grep -qs 'ssh -L 5900:127.0.0.1:5900' /usr/share/doc/lazyos/05-system.md; then
		ok "the ssh tunnel command is on the menu row and in the manual (5900, loopback)"
	else
		warn "the tunnel command is missing from rofi-vnc or from 05-system.md: the way in is undocumented"
	fi
fi
# A row and its arm, in both menus: a row whose key has no arm closes the list
# and does nothing, which is the quietest failure a menu can have.
grep -qs 'rofi-vnc' "$HOME/.config/bin/rofi-quick" &&
	ok "Settings carries the Share this screen row" ||
	fail "rofi-quick has no Share this screen row: the only way in would be the root menu"
grep -qs 'rofi-vnc' "$HOME/.config/bin/bar-menu" &&
	ok "the root menu reaches Share this screen" ||
	fail "bar-menu has no Share this screen row"
# The bar's eye is a module over a state file, so the two halves that can be
# dropped are its place in modules-right (the module then never runs) and the
# .empty rule (a card stays on the bar saying nothing).  A missing .empty is a
# cosmetic warning; a module that does not run is the indicator that never
# appears while the screen is being served.
if [ -r "$HOME/.config/waybar/config.jsonc" ]; then
	grep -qs '"custom/vnc"' "$HOME/.config/waybar/config.jsonc" &&
		ok "the bar has the screen-sharing indicator (custom/vnc)" ||
		fail "waybar config.jsonc has no custom/vnc: nothing on the bar says the screen is being shared"
fi
if [ -r "$HOME/.config/waybar/style.css" ]; then
	grep -qs '#custom-vnc.empty' "$HOME/.config/waybar/style.css" &&
		ok "the sharing indicator collapses when nothing is shared" ||
		warn "style.css has no #custom-vnc.empty: an empty card is left on the bar"
fi
# Nothing starts it by itself.  A wayvnc in the session's startup - or one left
# behind by a script - is a screen served to whoever asks, and the menu would
# then offer to start a second one on a port already taken.  `status` is the
# same read the Settings row makes, so this compares the process with the state
# the menu reports instead of guessing.
_vnc_pid=$(pgrep -x wayvnc 2>/dev/null || true)
if [ -n "$_vnc_pid" ]; then
	_vnc_state=$("$HOME/.config/bin/rofi-vnc" status 2>/dev/null | head -n 1 || true)
	if [ "$_vnc_state" = off ]; then
		warn "wayvnc is running but rofi-vnc does not own it: the menu says off while the screen is being served (pid $_vnc_pid)"
	else
		info "the screen is being shared: $_vnc_state"
	fi
fi

# Clipboard history (Alt+Ctrl+V) and the emoji picker (Alt+Ctrl+E): each is a
# script over one tool and one file, and every half of it fails quietly. Without
# cliphist the list says so; without the watcher the list is simply empty and
# reads as "nothing was copied"; without the emoji table the picker refuses to
# open; without wtype the glyph reaches the clipboard but never the cursor.
_missing_clip=""
for _c in cliphist wl-paste wl-copy wtype; do
	have "$_c" || _missing_clip="$_missing_clip $_c"
done
[ -r "$HOME/.config/emojis.tsv" ] || _missing_clip="$_missing_clip .config/emojis.tsv"
for _s in rofi-emoji rofi-clip; do
	[ -x "$HOME/.config/bin/$_s" ] || _missing_clip="$_missing_clip bin/$_s"
done
if [ -z "$_missing_clip" ]; then
	ok "clipboard history and emoji picker complete (cliphist, wl-clipboard, wtype, emojis.tsv)"
else
	fail "clipboard/emoji incomplete:$_missing_clip (Alt+Ctrl+V and Alt+Ctrl+E fail or insert nothing)"
fi
# The watcher is the one part a binary cannot prove: it either is running or
# nothing is being stored.  `-fx` and not `-f`: pgrep -f matches this script's
# own command line, which contains the pattern, and would always answer "yes".
if pgrep -fx 'wl-paste --watch cliphist store' >/dev/null 2>&1; then
	ok "clipboard watcher running (wl-paste --watch cliphist store)"
else
	warn "the cliphist watcher is not running: the clipboard list stays empty (config.d/27-clipboard.conf starts it with the session)"
fi
# "Save the newest image" names the file after the type `file` reports, and
# without it the picture is written with no extension at all - the viewer then
# refuses it, which reads as a broken clipboard rather than a missing package.
if have file; then
	ok "file(1) is installed (a clipboard image is saved with its extension)"
else
	warn "file(1) is missing (package file): an image saved from the clipboard has no extension"
fi
# An inserted emoji needs a colour font; fc-match says what would draw it.
if have fc-match; then
	_emoji_font=$(fc-match -f '%{family}' 'emoji' 2>/dev/null || true)
	case ${_emoji_font:-} in
	*Emoji*) ok "a colour emoji font is installed ($_emoji_font)" ;;
	*) fail "no colour emoji font (fc-match emoji -> '${_emoji_font:-none}'): every inserted emoji renders as tofu" ;;
	esac
else
	skip "fc-match is not installed"
fi
# The table is filtered against the font that draws it, and ~/.config/emojis.font
# pins the font it was built from. A font upgrade does not break an existing row,
# but the table stops offering the glyphs the new font added, and the pin is the
# only place that difference is visible.
if have fc-match && have sha256sum; then
	if [ ! -r "$HOME/.config/emojis.font" ]; then
		warn "$HOME/.config/emojis.font is missing: the emoji table cannot be compared with the font (regenerate it with scripts/make-emojis.sh)"
	else
		_emoji_file=$(fc-match -f '%{file}' emoji 2>/dev/null || true)
		_emoji_pin=$(sed -n 's/^sha256=//p' "$HOME/.config/emojis.font" 2>/dev/null || true)
		_emoji_sum=""
		if [ -n "$_emoji_file" ] && [ -r "$_emoji_file" ]; then
			_emoji_sum=$(sha256sum "$_emoji_file" 2>/dev/null || true)
			_emoji_sum=${_emoji_sum%% *}
		fi
		if [ -n "$_emoji_sum" ] && [ "$_emoji_sum" = "$_emoji_pin" ]; then
			ok "the emoji table matches the font it was filtered against"
		else
			warn "the emoji font changed since ~/.config/emojis.font was written: re-run scripts/make-emojis.sh on the build host to pick up its new glyphs"
		fi
	fi
else
	skip "fc-match or sha256sum is not installed"
fi

# The touchpad's three-finger swipes. The two vertical ones are the only place
# this port *types for the person*: sway has no command that hands a key to the
# focused client, so `wtype` drives the virtual keyboard protocol and the window
# receives a real Ctrl+Tab. Without it the swipe completes, sway logs
# `Command not found` and the tab does not change - the gesture looks broken
# while the binding is exactly as written. The two workspace swipes are checked
# with it because one file holds all four.
#
# The vertical two also carry the Firefox filter, and that half is checked too:
# `con_id="__focused__"` is what keeps the keys out of every other window (a
# criteria without it matches a window of that app even when it is not focused,
# measured), and it is sway's own check in-process - nothing is started when
# another application is focused. A binding that loses the criteria would type
# Ctrl+Tab into whatever is focused and look like a broken gesture everywhere
# else, so this FAILs when it goes missing.
_gest_cfg=/etc/sway/config.d/22-workspace-gestures.conf
_gest=""
[ -r "$_gest_cfg" ] || _gest="$_gest config.d/22-workspace-gestures.conf"
for _g in 'swipe:3:left workspace next_on_output' \
	'swipe:3:right workspace prev_on_output' \
	'swipe:3:up [app_id="(?i)^firefox" con_id="__focused__"] exec wtype -M ctrl -k Tab -m ctrl' \
	'swipe:3:down [app_id="(?i)^firefox" con_id="__focused__"] exec wtype -M ctrl -M shift -k Tab -m shift -m ctrl'; do
	grep -qsF "$_g" "$_gest_cfg" 2>/dev/null || _gest="$_gest ${_g%% *}"
done
have wtype || _gest="$_gest wtype"
if [ -z "$_gest" ]; then
	ok "touchpad swipes wired (3-finger left/right workspaces, up/down Ctrl+Tab / Ctrl+Shift+Tab in Firefox)"
else
	fail "touchpad swipes incomplete:$_gest (the swipe completes and nothing happens)"
fi

# A duplicate bindgesture is *not* a silent override the way a duplicate bindsym
# is. Measured on the maintainer's session: the same gesture in a user drop-in
# and in the system one still applies, and away from a running session nothing
# complains (`sway -C` accepts the files, and so does a fresh sway) - but a
# `swaymsg reload` counts it as a config error and puts the red "There are
# errors in your config file" bar across the top of the screen, where the only
# visible cause is a line that looks correct. That is how the temporary
# diagnostic behind the vertical swipes was found. It is a WARN and not a FAIL:
# the bindings work, the bar is the whole symptom.
_user_gestures=""
for _f in "$HOME/.config/sway/config" "$HOME"/.config/sway/config.d/*.conf; do
	[ -r "$_f" ] || continue
	while read -r _k; do
		[ -n "$_k" ] || continue
		grep -rqsF "bindgesture $_k " /etc/sway/config.d/ &&
			_user_gestures="$_user_gestures $_k(${_f##*/})"
	done <<<"$(sed -n 's/^[[:space:]]*bindgesture[[:space:]]\{1,\}\([^[:space:]]*\).*/\1/p' "$_f")"
done
if [ -z "$_user_gestures" ]; then
	ok "no user drop-in repeats a system bindgesture (a duplicate raises the red config-error bar on reload)"
else
	warn "a user drop-in repeats a system bindgesture:$_user_gestures - sway counts that as a config error and shows 'There are errors in your config file' on reload while 'sway -C' accepts it; edit the system file instead"
fi

# Tablet mode: the keyboard folded back. Four pieces have to be there for it to
# do anything - the switch, the script, the on-screen keyboard and the
# accelerometer - and each missing half is silent: no bindswitch means detaching
# the keyboard changes nothing, no wvkbd means the screen has no way to type, no
# accelerometer means the screen stays sideways. The accelerometer is read the
# same way the poller reads it (cros-ec-accel, in_accel_*_raw), so a kernel that
# renames or reorganises the devices is noticed here.
_tablet=""
have wvkbd-mobintl || _tablet="$_tablet wvkbd-mobintl"
[ -x "$HOME/.config/bin/tablet-mode" ] || _tablet="$_tablet bin/tablet-mode"
grep -rqs 'bindswitch tablet:' /etc/sway/config.d/ ||
	_tablet="$_tablet config.d/28-tablet.conf(bindswitch)"
if [ -z "$_tablet" ]; then
	ok "tablet mode wired (wvkbd-mobintl, tablet-mode, bindswitch tablet:on/off)"
else
	fail "tablet mode incomplete:$_tablet (detaching the keyboard does nothing, or the screen cannot type)"
fi
_accel=""
for _d in /sys/bus/iio/devices/iio:device*; do
	[ -r "$_d/name" ] || continue
	[ "$(cat "$_d/name" 2>/dev/null)" = cros-ec-accel ] || continue
	[ -r "$_d/in_accel_x_raw" ] && _accel=$_d && break
done
if [ -n "$_accel" ]; then
	ok "accelerometer present ($_accel, cros-ec-accel) for tablet rotation"
else
	fail "no cros-ec-accel with in_accel_*_raw: tablet mode starts but the screen never turns"
fi
# The two physical inputs are switched off with the mode, and the on-screen
# keyboard comes up first: a card whose keyboard was disabled before wvkbd proved
# alive would be a touchscreen with no way to type. The mode is the only thing in
# the session that disables a keyboard, so the pattern is a fair test of the wiring.
#
# The keyboard is never switched off as `type:keyboard`: cros_ec_buttons is a
# keyboard (the power and volume keys) *and* the device that reports
# SW_TABLET_MODE, wlroots builds both halves over one libinput handle, and
# send-events belongs to that handle - so a type-wide disable suspends the switch
# too, `tablet:off` never arrives and the mode cannot be left by folding the
# keyboard back (the 2026-10-22 card). The script names each safe identifier
# instead and skips every identifier a switch also carries. The value is `$1` of
# inputs() and not the word `disabled`, so what is tested is the call
# (`inputs disabled`) and the order of the last one against `start_kbd`: `on`
# answers an already-running mode before it starts the keyboard, and that early
# `inputs disabled` is the one a plain line-order test would find.
_tm="$HOME/.config/bin/tablet-mode"
_tm_code=$(grep -v '^[[:space:]]*#' "$_tm" 2>/dev/null || true)
_tm_ids=$(sed -n '/^keyboard_ids() {/,/^}/p' "$_tm" 2>/dev/null || true)
_tm_inputs=$(sed -n '/^inputs() {/,/^}/p' "$_tm" 2>/dev/null || true)
_tm_ids_ok=0
_tm_inputs_ok=0
_tm_typewide=""
while IFS= read -r _line; do
	case $_line in
	*'input type:keyboard events enabled'*) ;;
	*'input type:keyboard events'*)
	_tm_typewide=${_line#"${_line%%[![:space:]]*}"}
	;;
	esac
done <<<"$_tm_code"
case $_tm_ids in
*'type == "switch"'*) _tm_ids_ok=1 ;;
esac
case $_tm_inputs in
*'keyboard_ids'*'events "$1"'*) _tm_inputs_ok=1 ;;
esac
if [ -n "$_tm_typewide" ]; then
	fail "bin/tablet-mode switches type:keyboard as a whole ($_tm_typewide): cros_ec_buttons is a keyboard and the tablet switch on one libinput device, so the switch is suspended and tablet:off never arrives (the mode cannot be left by folding the keyboard back)"
elif [ "$_tm_ids_ok" = 1 ] && [ "$_tm_inputs_ok" = 1 ]; then
	_after_kbd=$(grep -n 'inputs disabled' "$_tm" | tail -1 | cut -d: -f1)
	_start_kbd=$(grep -n 'start_kbd ||' "$_tm" | head -1 | cut -d: -f1)
	if [ -n "$_after_kbd" ] && [ -n "$_start_kbd" ] && [ "$_after_kbd" -gt "$_start_kbd" ]; then
		ok "tablet mode switches the physical keyboard off per identifier, leaving the tablet switch enabled (after wvkbd is up)"
	else
		fail "bin/tablet-mode disables the keyboard before wvkbd is confirmed alive: the touchscreen can be left unable to type"
	fi
else
	fail "bin/tablet-mode does not enumerate keyboards per identifier: disabling a keyboard by type can take the tablet switch with it"
fi
# The switch is only useful while it is enabled. `swaymsg -t get_inputs` reports
# libinput's send-events mode, which is what the switch half shares with the
# keyboard half of cros_ec_buttons, so this reads the state the next fold depends
# on. Outside a sway session there is nothing to ask.
_tm_sock=${SWAYSOCK:-}
if [ -z "$_tm_sock" ] || [ ! -S "$_tm_sock" ]; then
	for _s in "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"/sway-ipc."$(id -u)".*.sock; do
		[ -S "$_s" ] || continue
		_tm_sock=$_s
		break
	done
fi
_tm_devs=""
if [ -n "$_tm_sock" ] && [ -S "$_tm_sock" ] && have swaymsg && have jq; then
	_tm_devs=$(SWAYSOCK="$_tm_sock" swaymsg -t get_inputs -r 2>/dev/null || true)
fi
if [ -z "$_tm_devs" ]; then
	skip "no sway session to ask which input devices are enabled (a suspended tablet switch cannot deliver tablet:off)"
else
	_tm_suspended=$(printf '%s\n' "$_tm_devs" |
		jq -r '.[] | select(.type == "switch" and .libinput.send_events == "disabled") | .identifier' 2>/dev/null || true)
	if [ -z "$_tm_suspended" ]; then
		ok "every switch device is enabled (the tablet switch is not suspended with a keyboard)"
	else
		fail "switch device(s) $_tm_suspended have send_events disabled: folding the keyboard back cannot deliver tablet:off"
	fi
fi
# Folding the keyboard back has to leave the screen upright even when the mode's
# state directory is gone: a session killed in tablet mode takes it with it, and
# the old off() answered that case first and returned before the rotation was
# undone. off() now asks the compositor for the focused output, so the undo needs
# nothing from $state; extract the function because the same words appear in
# comments and in on().
_tablet_off=$(sed -n '/^off() {/,/^}/p' "$HOME/.config/bin/tablet-mode" 2>/dev/null || true)
case $_tablet_off in
*'apply_transform normal'*)
	if grep -q '\[ -d "\$state" \] ||' <<<"$_tablet_off"; then
		fail "tablet-mode off() returns before restoring the rotation when the state is gone (the screen can stay sideways after a killed session)"
	else
		ok "tablet mode off restores the rotation without the state directory"
	fi
	;;
*)
	fail "tablet-mode off() does not apply 'transform normal': folding the keyboard back can leave the screen turned"
	;;
esac
if grep -q -- '--alpha' "$HOME/.config/bin/tablet-mode" 2>/dev/null; then
	warn "tablet-mode still passes --alpha to wvkbd (the keyboard is meant to be opaque: with transparency the window shows through the keys)"
fi
# The rotation reference is optional - the built-in -Y table is the default - but a
# file that does not parse is ignored in silence by orient(), which reads as "the
# rotation keeps turning back to the old way" after a calibration that claimed to work.
_calib_pose="${XDG_CONFIG_HOME:-$HOME/.config}/lazyos/tablet.conf"
if [ ! -f "$_calib_pose" ]; then
	skip "no rotation calibration ($_calib_pose): the built-in -Y reference is in use"
else
	_calib_line=$(sed -n '1s/[[:space:]]*$//;1s/^[[:space:]]*//;1p' "$_calib_pose" 2>/dev/null || true)
	case $_calib_line in
	normal=[xXyY][+-]) ok "rotation calibration is valid ($_calib_line in $_calib_pose)" ;;
	*) fail "rotation calibration is malformed ('${_calib_line:-empty}' in $_calib_pose): orient() ignores it and the screen turns the built-in way (delete the file to say so out loud)" ;;
	esac
fi
# The keyboard belongs to tablet mode and nowhere else. One that is already
# running while the mode is off is either a leftover from a session that was
# killed mid-mode or something starting it at login; either way it covers the
# bottom of the screen for no reason.
if [ ! -d "${XDG_RUNTIME_DIR:-/tmp}/lazyos-tablet" ]; then
	if pgrep -x wvkbd-mobintl >/dev/null 2>&1; then
		warn "wvkbd-mobintl is running outside tablet mode (config.d/28-tablet.conf starts it on the switch only)"
	fi
fi
# The first boot's card is once per *install*: the marker holds /etc/machine-id,
# it is written after the window closes, and a foot that never opened must not
# write it (a window nobody saw would then never be tried again). The stub-foot
# test uses only a temp directory, so it puts nothing on the real desktop.
_welcome=""
[ -x "$HOME/.config/bin/lazyos-welcome" ] || _welcome="$_welcome bin/lazyos-welcome"
[ -r "$HOME/.config/sway/config.d/30-welcome.conf" ] ||
	_welcome="$_welcome .config/sway/config.d/30-welcome.conf"
grep -qs '^exec .*/lazyos-welcome' "$HOME/.config/sway/config.d/30-welcome.conf" 2>/dev/null ||
	_welcome="$_welcome 30-welcome.conf(no-exec)"
if [ -z "$_welcome" ]; then
	_welcome_text=$("$HOME/.config/bin/lazyos-welcome" --print 2>/dev/null || true)
	_welcome_missing=""
	for _k in 'Alt+Space' 'Alt+D' 'Search' 'Alt+Ctrl+F' 'Alt+Ctrl+T' 'Alt+Return' \
		'Alt+Q' 'Alt+Escape' 'Alt+Ctrl+K'; do
		grep -qF -- "$_k" <<<"$_welcome_text" || _welcome_missing="$_welcome_missing $_k"
	done
	grep -qF -- 'once per install' <<<"$_welcome_text" ||
		_welcome_missing="$_welcome_missing once-per-install"
	if [ -z "$_welcome_missing" ]; then
		ok "welcome window lists the nine keys and says it is once per install"
	else
		fail "welcome window text lacks:$_welcome_missing"
	fi
else
	fail "welcome window incomplete:$_welcome (the first boot shows no card)"
fi

_welcome_tmp=$(mktemp -d "${TMPDIR:-/tmp}/lazyos-welcome.XXXXXX" 2>/dev/null || true)
if [ -n "$_welcome_tmp" ] && [ -x "$HOME/.config/bin/lazyos-welcome" ]; then
	printf '%s\n' '#!/bin/sh' 'printf "call\n" >>"$WELCOME_TEST_LOG"' 'exit "${WELCOME_TEST_RC:-0}"' >"$_welcome_tmp/foot"
	chmod +x "$_welcome_tmp/foot"
	_welcome_res=$(
		export XDG_STATE_HOME="$_welcome_tmp/state" PATH="$_welcome_tmp:$PATH"
		export WELCOME_TEST_LOG="$_welcome_tmp/calls"
		WELCOME_TEST_RC=1 "$HOME/.config/bin/lazyos-welcome" >/dev/null 2>&1 || true
		[ -f "$_welcome_tmp/state/lazyos/welcome" ] && printf 'fail-marker '
		WELCOME_TEST_RC=0 "$HOME/.config/bin/lazyos-welcome" >/dev/null 2>&1 || true
		[ "$(cat "$_welcome_tmp/state/lazyos/welcome" 2>/dev/null)" = "$(cat /etc/machine-id)" ] &&
			printf 'marker-ok '
		_welcome_before=$(wc -l <"$_welcome_tmp/calls")
		"$HOME/.config/bin/lazyos-welcome" >/dev/null 2>&1 || true
		[ "$(wc -l <"$_welcome_tmp/calls")" = "$_welcome_before" ] &&
			printf 'no-third-call'
	)
	case $_welcome_res in
	'marker-ok no-third-call')
		ok "welcome marker works (failure leaves none, success writes the machine id, third run stays quiet)" ;;
	*)
		fail "welcome marker misbehaves ('${_welcome_res:-empty}'): the card can repeat on every login or never appear again" ;;
	esac
	rm -rf "$_welcome_tmp"
fi

# The manual is rendered with glow, and both the binary and the glamour style
# are needed: without the style glow prints an error and exits, and rofi-learn
# opens a foot window that closes immediately, which reads as "nothing happened".
_glow=""
have glow || _glow="$_glow glow"
[ -r "$HOME/.config/glow/solarized.json" ] || _glow="$_glow .config/glow/solarized.json"
[ -r "$HOME/.config/glow/glow.yml" ] || _glow="$_glow .config/glow/glow.yml"
if [ -z "$_glow" ]; then
	ok "manual renderer complete (glow with the solarized glamour style)"
else
	fail "manual renderer incomplete:$_glow (Learn opens a window that closes at once)"
fi
# And the style has to be readable by glamour: a colour written without its "#"
# is taken as a 256-colour index, so glow emits 38;5;839496, the terminal
# ignores it and the page falls back to its own colours. Nothing errors, which is
# why the first version of this style shipped 23 bare values and only looked
# wrong (inline code as a blank block). Cosmetic, so it warns.
if [ -r "$HOME/.config/glow/solarized.json" ] &&
	grep -qE '":[[:space:]]*"[0-9a-fA-F]{6}"' "$HOME/.config/glow/solarized.json"; then
	warn "glow style: a colour is missing its # (glamour reads it as a 256-colour index; the page loses its palette)"
fi

# The browser is the image's reader for what chafa cannot draw: a PDF, the HTML
# file, a URL. rofi-files and /usr/local/bin/xdg-open are the two things that
# start it; without it they answer with a notification instead, which is easy to
# miss.
if have firefox-esr; then
	ok "browser installed (firefox-esr), used by rofi-files and xdg-open"
else
	fail "firefox-esr is missing: Files cannot open an image or a PDF and xdg-open fails"
fi
if [ -x /usr/local/bin/xdg-open ]; then
	ok "xdg-open shim is installed (URLs and paths go to the browser)"
else
	fail "/usr/local/bin/xdg-open is missing: ctrl+click in foot and URL-printing tools fail"
fi
# Thumbnails in the file browser: rofi draws a row's picture through
# lazyos-thumbnail, which is ffmpeg for images and video and pdftocairo for a
# PDF's first page. A missing binary is silent - the row simply has no picture -
# so the two tools are a FAIL; the cache directory (rofi's own, under
# ~/.cache/thumbnails) only warns, because it is created on first use.
if have ffmpeg && have pdftocairo; then
	ok "thumbnails can be generated (ffmpeg for images/video, pdftocairo for PDF page 1)"
else
	fail "thumbnails cannot be generated:$(have ffmpeg || echo ' ffmpeg')$(have pdftocairo || echo ' pdftocairo') (Files rows lose their pictures)"
fi
if have chafa; then
	ok "picture viewer present (chafa draws sixels in foot)"
else
	fail "chafa is missing: Files cannot show an image without starting Firefox"
fi
# The three programs the file browser hands a file to, each with a package of its
# own: imv for a picture (chafa above is its fallback when the GPU gives it no
# EGL context), papers for a document, mpv for a container. Without one of them
# the branch falls through to the browser, which is 280 MB and cannot play a
# video at all - and nothing logs it, which is why this is a FAIL.
_viewers=""
for _c in imv papers mpv; do
	have "$_c" || _viewers="$_viewers $_c"
done
if [ -z "$_viewers" ]; then
	ok "viewers installed (imv for a picture, papers for a document, mpv for a video)"
else
	fail "missing viewer(s):$_viewers (that kind of file falls back to the browser)"
fi
# imv is not a package binary: Debian ships imv-wayland/imv-x11 and a
# /usr/libexec/imv/imv dispatcher, and rootfs-setup.sh puts that behind the short
# name. Without the symlink every `command -v imv` fails and a photo goes to
# chafa's sixels, which looks like "the new viewer does nothing".
if [ -x /usr/local/bin/imv ]; then
	ok "the imv short name resolves (/usr/local/bin/imv -> the package dispatcher)"
else
	fail "/usr/local/bin/imv is missing: rofi-files and lazyos-view cannot start the viewer"
fi
# imv has no on-screen help: the first picture of a session says its keys in one
# notification. A missing executable bit or a dropped call is silent - the picture
# simply opens with no hint.
if [ -x "$HOME/.config/bin/lazyos-image-hint" ]; then
	ok "the viewer keys are hinted once per session (lazyos-image-hint)"
else
	fail "bin/lazyos-image-hint is missing or not executable: imv opens with no key hint"
fi
for _f in rofi-files lazyos-view; do
	grep -qs 'lazyos-image-hint' "$HOME/.config/bin/$_f" ||
		warn "bin/$_f no longer calls lazyos-image-hint: pictures opened that way get no hint"
done
# mpv-mpris is a plugin file, not a program, so nothing else notices it: without
# it the film plays and playerctl, the Media menu and the bar see no player.
if [ -r /etc/mpv/scripts/mpris.so ]; then
	ok "mpv is an MPRIS player (mpv-mpris at /etc/mpv/scripts/mpris.so)"
else
	fail "mpv-mpris is missing: playerctl, the Media menu and the bar cannot see mpv"
fi
if [ -d "$HOME/.cache/thumbnails" ] && [ ! -w "$HOME/.cache/thumbnails" ]; then
	warn "$HOME/.cache/thumbnails is not writable: every picture is decoded again on every listing"
fi
# And the browser has to keep asking: an edit that drops the flag or the
# thumbnail:// suffix makes the list silently text-only.
if [ -r "$HOME/.config/bin/rofi-files" ] &&
	grep -qs -- '-show-icons' "$HOME/.config/bin/rofi-files" &&
	grep -qs 'thumbnail://' "$HOME/.config/bin/rofi-files"; then
	ok "rofi-files asks rofi for thumbnails (-show-icons and thumbnail://)"
else
	warn "rofi-files no longer passes -show-icons/thumbnail://: the file list shows no pictures"
fi

# A video is mpv's job: chafa only draws stills and answers a container with
# "unknown file format", which is the error the report carried. Without mpv the
# row falls back to Firefox (an .mkv then asks to be downloaded), so this is a
# FAIL - the row claims to play something.
if have mpv; then
	ok "media player installed (mpv), what rofi-files opens a video with"
else
	fail "mpv is missing: a video in Files falls back to Firefox (chafa cannot decode a container)"
fi
# A document is papers' job. Without it a PDF goes to Firefox, which is a
# 280 MB browser for a page, and an EPUB still needs it (papers does not read
# EPUB, whatever the comparison table once said).
if have papers; then
	ok "document reader installed (papers), what a double click on a PDF uses"
else
	warn "papers is missing: a PDF opens in Firefox instead of the reader (the fallback works, it is just the browser)"
fi

# The webcam. The camera is a USB UVC device - the internal one sits on the hub
# at 1-1.4, and it is the only one this tablet has: the device tree carries no
# MIPI sensor, so there is nothing for libcamera to drive and no image pipeline
# to miss. uvcvideo is a module of this kernel, so the *device* half needs no
# package at all; what can disappear is the window, and each of the failures
# below is silent on the machine - a row that opens nothing, or a window that
# never appears - which is why they are FAILs and not warnings.
_cam_sys= _cam_node=
for _v in /sys/class/video4linux/video*; do
	[ -e "$_v" ] || continue
	_drv=$(readlink -f "$_v/device/driver" 2>/dev/null) || continue
	[ "${_drv##*/}" = uvcvideo ] || continue
	_cam_sys=$_v
	_cam_node=/dev/${_v##*/}
	break
done
if [ -n "$_cam_node" ]; then
	ok "webcam found ($_cam_node, uvcvideo: $(cat "$_cam_sys/name" 2>/dev/null))"
elif [ -d /sys/module/uvcvideo ]; then
	fail "uvcvideo is loaded but no /dev/video* node is bound to it: the camera cannot be opened (sudo dmesg | grep -i uvc)"
else
	skip "no webcam (uvcvideo is not loaded: no UVC device on the bus)"
fi
if have guvcview; then
	ok "camera application installed (guvcview $(dpkg-query -W -f='${Version}' guvcview 2>/dev/null))"
else
	fail "guvcview is missing: the Camera row opens nothing - there is a camera and no window for it"
fi
if [ -x "$HOME/.config/bin/lazyos-camera" ]; then
	ok "the camera wrapper runs (bin/lazyos-camera, executable)"
else
	fail "bin/lazyos-camera is missing or not executable: the Camera row and the launcher entry do nothing"
fi
if [ -r /usr/share/applications/org.lazyos.camera.desktop ]; then
	ok "Camera is in the launcher (org.lazyos.camera.desktop)"
else
	fail "org.lazyos.camera.desktop is missing: the launcher has no Camera entry"
fi
if [ -r /usr/share/icons/LazyOS/48x48/apps/camera-web.png ]; then
	ok "the Camera entry has its picture (camera-web in the LazyOS theme)"
else
	fail "the LazyOS theme has no camera-web.png: the Camera launcher row is a hole"
fi
if grep -qs 'lazyos-camera' "$HOME/.config/bin/bar-menu"; then
	ok "the root menu carries the Camera row (Alt+Space)"
else
	fail "bar-menu lost its Camera row: Alt+Space cannot open the webcam"
fi
# The camera portal is a different road to the same device, and this image has
# no backend that implements it: the only portal backend here is the wlr one
# (Screenshot and ScreenCast), so xdg-desktop-portal logs "The camera portal
# requires an access impl" on every session while guvcview, ffmpeg and Firefox
# open /dev/video* directly. It is a stated limit (04-software.md), not a
# regression - but it is the answer to "why does this Flatpak see no camera",
# so the report says it instead of leaving the journal line to be found.
if [ -S "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/bus" ] && have busctl; then
	if ! _portal=$(busctl --user call org.freedesktop.portal.Desktop \
			/org/freedesktop/portal/desktop \
			org.freedesktop.portal.Camera IsCameraPresent 2>&1); then
		info "no camera portal (${_portal#Call failed: }): a Flatpak app cannot use the camera; guvcview, ffmpeg and Firefox read ${_cam_node:-the device} directly"
	fi
fi

# Every row of the file browser carries an icon *name* the LazyOS theme has to
# resolve; a missing PNG is a blank 32 px column with no log line anywhere.
_missing_row_icons=""
for _i in folder go-up view-hidden dialog-warning text-x-generic text-html \
	application-pdf image-x-generic video-x-generic audio-x-generic \
	application-x-executable package-x-generic x-office-document; do
	[ -r "/usr/share/icons/LazyOS/48x48/apps/$_i.png" ] || _missing_row_icons="$_missing_row_icons $_i"
done
if [ -z "$_missing_row_icons" ]; then
	ok "file browser icons present (the LazyOS theme has every row name)"
else
	warn "LazyOS theme is missing row icons:$_missing_row_icons (those rows lose the icon column)"
fi

# The GTK3 half of the palette. One window in this image is GTK3 (favnyr, the file manager), and
# without these two files it renders in Adwaita's light grey - a bright rectangle
# in a session whose every other pixel is base03. Both failures are silent: GTK
# drops a stylesheet rule it cannot parse with a line on stderr nobody reads, and
# an icon name the theme does not carry draws nothing at all.
if [ -r "$HOME/.config/gtk-3.0/gtk.css" ]; then
	ok "GTK3 windows use the solarized palette (gtk-3.0/gtk.css)"
else
	warn "$HOME/.config/gtk-3.0/gtk.css is missing: favnyr renders in Adwaita's light grey"
fi
if grep -qs '^gtk-icon-theme-name=LazyOS' "$HOME/.config/gtk-3.0/settings.ini" &&
	grep -qs '^gtk-application-prefer-dark-theme=1' "$HOME/.config/gtk-3.0/settings.ini"; then
	ok "GTK3 windows use the LazyOS icons on a dark base (gtk-3.0/settings.ini)"
else
	warn "gtk-3.0/settings.ini does not select LazyOS + dark: favnyr looks like a foreign program"
fi
# The GTK4 half is a different settings store, so the two files above reach
# nothing here.  `papers` is the one GTK4/libadwaita window in the image, and it
# opened white: libadwaita asks the settings portal for the colour scheme, and
# this image runs no Settings portal (the only portal backend is the wlr one,
# which implements Screenshot and ScreenCast).  With no portal it falls back to
# Adwaita's light default unless gtk-4.0/settings.ini prefers dark.
if [ -r "$HOME/.config/gtk-4.0/settings.ini" ] &&
	grep -qs '^gtk-application-prefer-dark-theme=1' "$HOME/.config/gtk-4.0/settings.ini"; then
	ok "GTK4 windows prefer the dark palette (gtk-4.0/settings.ini)"
else
	warn "gtk-4.0/settings.ini is missing or not dark: papers renders in libadwaita's light default"
fi
if [ -r "$HOME/.config/gtk-4.0/gtk.css" ] && [ -s "$HOME/.config/gtk-4.0/gtk.css" ]; then
	ok "GTK4 windows use the solarized palette (gtk-4.0/gtk.css)"
else
	warn "$HOME/.config/gtk-4.0/gtk.css is missing or empty: papers keeps libadwaita's own colours"
fi

# The windowed file manager is favnyr (replaced pcmanfm with Brunux/Favnyr-aarm64 v0.3.6 binary at /usr/bin/favnyr);
# Alt+Search finds files). What fails here does so silently: the terminal comes
# from $TERMINAL with a fallback list foot is not in (the check below), a double
# click on a photo goes through the association, and the icons it draws come from
# the theme rather than from the binary, so a lost PNG is a hole in the sidebar
# and the icon view.
if have favnyr; then
	# **Never run it to ask its version.** This build parses `--version` as a
	# startup location ("WARN favnyr: invalid startup location
	# location=--version"), then opens its Slint event loop and never exits:
	# the report stopped on this line, and every check after it - the whole
	# framework section included - never ran. `--help` opens the window too.
	# Presence and the exec bit are what can be asked without starting one.
	ok "file manager installed (favnyr is executable)"
else
	fail "favnyr is missing: the Files (window) entry and 'Open containing folder' do nothing"
fi
# The window has a key of its own (Alt+Ctrl+Shift+F, the Shift variant of the
# Files menu's Alt+Ctrl+F): without the binding the window is only reachable
# from the launcher, which reads as "the shortcut stopped working".
if grep -rqs 'Ctrl+Shift+f exec .*favnyr' /etc/sway/config.d/; then
	ok "Alt+Ctrl+Shift+F opens the file manager (favnyr)"
else
	fail "no 'Ctrl+Shift+f exec ...favnyr' binding in /etc/sway/config.d/: Alt+Ctrl+Shift+F does nothing"
fi
# Mounting removable media uses udisks2; both the file manager (favnyr) and rofi-places
# through /usr/bin/udisksctl to udisksd on the system bus. The check is the client
# talking to the daemon (udisksctl status activates the D-Bus service), because
# the package can be installed with the service missing from the bus.
if have udisksctl; then
	if _udisks=$(udisksctl status 2>&1); then
		ok "removable media can be mounted (udisks2 answers on the system bus)"
	else
		fail "udisksctl cannot reach udisksd: $_udisks (Places cannot mount a stick)"
	fi
else
	fail "udisks2 is missing: an unprivileged session cannot mount a USB stick at all"
fi
# A plugged USB drive mounts itself: the udev rule wants
# lazyos-automount@<dev>.service, and /usr/local/sbin/lazyos-automount mounts it
# as the desktop user through udisksd, then says so in a notification. Nothing
# stays resident, so the chain *is* the feature and every link fails silently:
# the stick simply never appears, and there is no journal line anybody would
# think to look for. The two halves that are easy to lose are the polkit rule (a
# session-less caller is auth_admin, not allow_active, so udisksd denies the
# mount) and the plugdev membership that rule matches by group - which is why
# they are checked together.
_am_missing=
for _am_f in /etc/udev/rules.d/90-lazyos-automount.rules \
	/etc/systemd/system/lazyos-automount@.service \
	/usr/local/sbin/lazyos-automount; do
	[ -r "$_am_f" ] || _am_missing="$_am_missing $_am_f"
done
# /etc/polkit-1/rules.d is 0750 root:polkitd, so the rule is read through sudo
# like every other privileged file in this report (captured first: a
# `producer | grep -q` here would report a match as a failure whenever grep
# exits before the producer finishes).
_am_polkit=$(sudo -n cat /etc/polkit-1/rules.d/49-lazyos-automount.rules 2>/dev/null || true)
[ -n "$_am_polkit" ] || _am_missing="$_am_missing /etc/polkit-1/rules.d/49-lazyos-automount.rules"
[ -x /usr/local/sbin/lazyos-automount ] || _am_missing="$_am_missing lazyos-automount(!exec)"
if [ -z "$_am_missing" ]; then
	ok "a plugged USB drive mounts itself (udev rule, unit, helper, polkit rule)"
else
	fail "auto-mount is incomplete:$_am_missing (a plugged stick stays unmounted)"
fi
# The rule must stay USB-only: the eMMC and the microSD slot are the two disks
# this machine boots from, and a rule that matched them would mount a partition
# of the running system under /run/media.
if grep -qs 'SUBSYSTEMS=="usb"' /etc/udev/rules.d/90-lazyos-automount.rules; then
	ok "auto-mount matches USB disks only (never the eMMC or the SD slot)"
else
	fail '90-lazyos-automount.rules lost SUBSYSTEMS=="usb": it would mount the internal disks too'
fi
if grep -qs 'SYSTEMD_WANTS}+="lazyos-automount@%k.service"' /etc/udev/rules.d/90-lazyos-automount.rules; then
	ok "the udev rule wants the auto-mount unit"
else
	fail "the auto-mount udev rule does not want lazyos-automount@%k.service: a plug starts nothing"
fi
# A root mount of a vfat/exFAT filesystem is /run/media/root/<label> with uid=0,
# i.e. a stick the person at the screen cannot write to: setpriv is the line that
# makes the mount the user's.
if grep -qs setpriv /usr/local/sbin/lazyos-automount &&
	grep -qs 'udisksctl mount' /usr/local/sbin/lazyos-automount; then
	ok "the auto-mount helper mounts as the session user (setpriv + udisksctl)"
else
	fail "lazyos-automount no longer mounts as the user: a vfat stick would be uid=0"
fi
# The helper's own tools: setpriv drops to the account, findfs resolves the
# UUID=/LABEL= specs it refuses to mount over, and udisksctl is the mount.
_missing=
for _c in setpriv findfs udisksctl lsblk findmnt; do have "$_c" || _missing="$_missing $_c"; done
if [ -z "$_missing" ]; then
	ok "the auto-mount helper has its tools (setpriv, findfs, udisksctl, lsblk, findmnt)"
else
	fail "the auto-mount helper is missing:$_missing (a plugged stick fails to mount)"
fi
if grep -qs 'filesystem-mount' <<<"$_am_polkit" &&
	grep -qs "isInGroup('plugdev')" <<<"$_am_polkit"; then
	ok "polkit lets the auto-mount helper mount a removable disk (plugdev)"
else
	fail "49-lazyos-automount.rules does not grant filesystem-mount to plugdev: udisksd answers auth_admin"
fi
# The account, not this process: the helper mounts with setpriv --init-groups,
# which reads the group database, so a shell that predates the membership is not
# what the question is about.
case " $(id -nG "$(id -un 2>/dev/null)" 2>/dev/null) " in
*" plugdev "*)
	ok "the session user is in plugdev (the group the auto-mount rule names)"
	;;
*)
	fail "the session user is not in plugdev: the auto-mount polkit rule matches nobody"
	;;
esac
if systemctl is-active --quiet polkit 2>/dev/null; then
	ok "polkitd is running (the auto-mount rule is in force)"
else
	warn "polkitd is not running: a plugged stick will not mount (the rule is not read)"
fi
if grep -qs 'Auto-mount USB drives' "$HOME/.config/bin/rofi-quick"; then
	ok "Settings has the auto-mount switch"
else
	fail "rofi-quick lost the 'Auto-mount USB drives' row: the switch is unreachable"
fi
# favnyr's Unmount (if supported) runs through eject, not udisksctl.
# without it the action answers `Failed to execute child process "eject"` and the
# stick stays mounted.
if have eject; then
	ok "eject is available for favnyr's Unmount"
else
	fail "eject is missing: favnyr's Unmount may fail with missing child process"
fi
_xdg_dirs=${XDG_CONFIG_DIRS:-/etc/xdg}
# favnyr has no `terminal=` setting to read: it runs $TERMINAL and then a list of
# known emulators (kgx, konsole, gnome-terminal, kitty, alacritty, wezterm,
# xfce4-terminal, lxterminal, tilix, xterm) that does not include foot, so the
# image exports TERMINAL=foot in /etc/environment - the file pam_env reads in
# both greetd's and sudo's session stacks, verified on this machine with
# `sudo -n env`. Without it "Open terminal here" answers
# "no terminal detected ($TERMINAL or known candidates)" and starts nothing.
if grep -qx 'TERMINAL=foot' /etc/environment 2>/dev/null; then
	ok "favnyr opens a terminal with foot (TERMINAL=foot in /etc/environment)"
else
	fail "no TERMINAL=foot in /etc/environment: 'Open terminal here' in favnyr finds no terminal"
fi
# The names it draws with come from the theme, not from the package: Adwaita 51
# ships no full-colour "folder" at all, so a PNG that did not make it into the
# image is a hole in the places sidebar and in the icon view.
_missing=
for _icon in folder user-home user-trash text-x-generic application-pdf \
	image-x-generic drive-harddisk view-refresh edit-copy folder-new; do
	[ -r /usr/share/icons/LazyOS/48x48/apps/"$_icon".png ] || _missing="$_missing $_icon"
done
if [ -z "$_missing" ]; then
	ok "the LazyOS theme carries every icon the file manager asks for"
else
	warn "the LazyOS theme has no icon for:$_missing (holes in favnyr's sidebar and icon view)"
fi
if [ -x "$HOME/.config/bin/lazyos-view" ]; then
	ok "open-with handler installed (lazyos-view: bat, imv, papers, mpv, firefox-esr)"
else
	fail "bin/lazyos-view is missing or not executable: a double click in favnyr opens nothing"
fi
# The launcher entry is a shadow of the package's (same file name, so it replaces
# Debian's "PCMan File Manager"): without it the launcher shows "Files" (the
# menu) next to "PCMan File Manager", which reads as two copies of one thing.
if grep -qs '^Name=Files (window)' "$HOME/.local/share/applications/pcmanfm.desktop"; then
	ok "the launcher names the window 'Files (window)'"
else
	warn "$HOME/.local/share/applications/pcmanfm.desktop is missing or has wrong name: the launcher shows the wrong file manager name"
fi
# `gio mime` resolves the association exactly like the application does, and it
# is also a test of the .desktop entry itself: GIO drops an entry whose Exec
# program does not exist, so a lazyos-view without its executable bit reads as
# "no default application" instead of as an error. LC_ALL=C so the label is the
# one this greps for, not a translation.
#
# The image ships `gio` (libglib2.0-bin) so this is the real question, but the
# file is the answer that does not depend on a package: the fallback used to
# read `[Default Applications]` and then compare it with gio's own wording, so
# every association warned even when the file was right. Both shapes now feed
# the same variable: the desktop id, nothing else.
_mime_default() { # MIME-TYPE
	if have gio; then
		# gio's line is `Default application for “image/png”: org.lazyos.view.desktop`
		# (typographic quotes, and the type before the colon), or
		# `No default applications for “...”` when there is none - the field
		# after the last `: ` is the answer in the first shape and absent in
		# the second, which is why the old `^Default application: ` prefix
		# matched nothing at all.
		LC_ALL=C gio mime "$1" 2>/dev/null |
			awk 'NR == 1 && /: / { sub(/^.*: /, ""); print; exit }'
	else
		sed -n "s|^$1=||p" "${_xdg_dirs%%:*}/mimeapps.list" 2>/dev/null | head -1
	fi
}
_mime=$(_mime_default image/png)
if [ "$_mime" = org.lazyos.view.desktop ]; then
	ok "a double click on an image reaches lazyos-view (mimeapps.list)"
else
	warn "image/png defaults to '${_mime:-(nothing)}', not org.lazyos.view.desktop: a photo opens in the browser or nowhere"
fi
# A document is papers' job, and the association is the half a package cannot
# carry: papers ships org.gnome.Papers.desktop, and this file is what points
# application/pdf at it. GIO drops an entry whose Exec does not exist, so the
# same question also proves the .desktop is installed.
_mime_pdf=$(_mime_default application/pdf)
if [ "$_mime_pdf" = org.gnome.Papers.desktop ]; then
	ok "a double click on a PDF reaches papers (mimeapps.list)"
else
	warn "application/pdf defaults to '${_mime_pdf:-(nothing)}', not org.gnome.Papers.desktop: a document opens in the browser"
fi
_dir=$(_mime_default inode/directory)
if [ "$_dir" = "pcmanfm.desktop" ]; then
	ok "'Open containing folder' opens favnyr via pcmanfm.desktop shadow entry (inode/directory)"
else
	warn "inode/directory defaults to '${_dir:-(nothing)}', not pcmanfm.desktop (favnyr shadow): 'Open containing folder' does nothing"
fi

# Dictation: the shortcut records with parecord and transcribes with whisper-cli.
# The model is fetched on first use, so its absence is not a failure.
if have whisper-cli && have wtype && have parecord; then
	ok "dictation tools present (whisper-cli, wtype, parecord)"
	have ffmpeg && ok "dictation normalises the level (ffmpeg dynaudnorm)" ||
		warn "ffmpeg is missing: the internal microphone records quietly and the wav is transcribed raw"
else
	warn "dictation incomplete:$(have whisper-cli || echo ' whisper-cli')$(have wtype || echo ' wtype')$(have parecord || echo ' parecord')"
fi
_model="$(ls "$HOME"/.local/share/whisper/ggml-*.bin 2>/dev/null | head -1)"
[ -n "$_model" ] && ok "speech model present ($(basename "$_model"))" ||
	skip "no speech model yet: the dictation shortcut fetches tiny.en (78 MB) on first use"

# Translate (Alt+Ctrl+T): the selection goes to one endpoint with a fallback.
# What a report can see is the tools, the binding and the Settings row - the
# network is asked, never required, because a card without Wi-Fi translates
# nothing and says so.
_missing_translate=""
for _t in curl jq wl-paste; do
	have "$_t" || _missing_translate="$_missing_translate $_t"
done
[ -x "$HOME/.config/bin/translate" ] ||
	_missing_translate="$_missing_translate bin/translate(not executable)"
grep -qs 'bindsym $mod+Ctrl+t exec .*/translate' \
	/etc/sway/config.d/24-keys.conf 2>/dev/null ||
	_missing_translate="$_missing_translate 24-keys.conf(no-binding)"
grep -qs 'translate" config' "$HOME/.config/bin/rofi-quick" 2>/dev/null ||
	_missing_translate="$_missing_translate rofi-quick(no-row)"
[ -r /usr/share/icons/LazyOS/48x48/apps/globe.png ] ||
	_missing_translate="$_missing_translate globe.png"
if [ -z "$_missing_translate" ]; then
	ok "translate ready (Alt+Ctrl+T: curl, jq, wl-paste, the binding, the Settings row, the globe icon)"
else
	fail "translate incomplete:$_missing_translate (Alt+Ctrl+T does nothing, or its card has a hole)"
fi
# The target is one line a person may edit, so a code the table does not know is
# a typo that would silently translate into the wrong language: it is asked of
# the script itself, which is the same table the Settings menu draws.
_target_code=$(sed -n 's/^[[:space:]]*target[[:space:]]*=[[:space:]]*//p' \
	"$HOME/.config/lazyos/translate.conf" 2>/dev/null | tail -n 1)
# The list is captured first: `translate --languages | cut | grep -q` would kill
# the producer with SIGPIPE under pipefail and report a good code as unknown.
_translate_langs=$("$HOME/.config/bin/translate" --languages 2>/dev/null |
	cut -f1 || true)
if [ -z "$_target_code" ]; then
	ok "translate target is the default (English; Settings → Translate changes it)"
elif grep -qx -- "$_target_code" <<<"$_translate_langs"; then
	ok "translate target is '$_target_code', a language both engines take"
else
	fail "translate target '$_target_code' is not in the language table (~/.config/lazyos/translate.conf): the script falls back to English, so the setting does nothing"
fi
# The endpoint is unofficial, so it is asked and not required: offline is SKIP,
# and any other answer is a WARN because MyMemory may still be the answer.
_translate_code=$(curl -sS --max-time 5 -o /dev/null -w '%{http_code}' \
	'https://clients5.google.com/translate_a/t?client=dict-chrome-ex&sl=auto&tl=en&q=ok' \
	2>/dev/null || true)
case $_translate_code in
200) ok "the translation endpoint answers (Google first, MyMemory as the fallback)" ;;
'' | 000) skip "no network: the translation endpoint was not reachable (Alt+Ctrl+T says so instead of translating)" ;;
*) warn "the translation endpoint answered $_translate_code, not 200: Google is out and MyMemory is the fallback here" ;;
esac

# Hibernate needs both a disk swap and resume=; without them the helper explains
# what is missing, which is the correct behaviour, not a failure.
_hib_swap=$(awk 'NR > 1 && $1 !~ /zram/ { print $1 }' /proc/swaps 2>/dev/null | head -1)
if [ -n "$_hib_swap" ]; then
	case " $(cat /proc/cmdline) " in
	*" resume="*) ok "hibernation possible (disk swap $_hib_swap, resume= present)" ;;
	*) warn "disk swap $_hib_swap is active but the kernel command line has no resume=: hibernate would not resume" ;;
	esac
else
	skip "no disk swap: hibernation needs a swap labelled lazyos_swap (see sway-hibernate)"
fi
[ -x "$HOME/.config/bin/sway-hibernate" ] || fail "bin/sway-hibernate is missing"

# Agents: the menu is a script; whether a CLI is installed is informational,
# because installing one is the menu's own job.
[ -x "$HOME/.config/bin/rofi-agents" ] && ok "agents menu (rofi-agents) is executable" ||
	fail "bin/rofi-agents is missing: Alt+Ctrl+Shift+A does nothing"
# The agents menu draws the session's glyph rows. Without lib-rofi.sh the rows are
# text-only; without -markup-rows rofi prints the <span> tags instead of drawing.
if grep -qs 'lib-rofi.sh' "$HOME/.config/bin/rofi-agents" &&
	grep -qs 'ui_row' "$HOME/.config/bin/rofi-agents"; then
	ok "the agents menu draws the session's glyph rows (lib-rofi.sh, ui_row)"
else
	warn "bin/rofi-agents no longer uses lib-rofi.sh/ui_row: its rows are text only"
fi
grep -qs -- '-markup-rows' "$HOME/.config/bin/rofi-agents" &&
	ok "the agents menu asks rofi to render markup rows" ||
	fail "bin/rofi-agents lost -markup-rows: every row shows its <span> tags"
_agent_bins=""
for _a in claude codex opencode gemini copilot; do have "$_a" && _agent_bins="$_agent_bins $_a"; done
[ -n "$_agent_bins" ] && ok "coding agent CLI(s) installed:$_agent_bins" ||
	skip "no coding-agent CLI installed yet (the Agents menu installs one with npm)"
# OpenCode is the exception to that: the image ships it (tools/opencode/ pins the
# tarball, scripts/rootfs-setup.sh installs it into ~/.opencode/bin), so it is a
# promise and not a choice. A missing binary turns its row into an install row,
# which reads as the feature being gone, and nothing logs it. `--version` creates
# the agent's data directories and a log inside $HOME, hence the throwaway home:
# this report changes nothing.
_oc_bin="$HOME/.opencode/bin/opencode"
if [ -x "$_oc_bin" ]; then
	_oc_home="$(mktemp -d 2>/dev/null || true)"
	_oc_ver="$(HOME="${_oc_home:-/tmp}" \
		XDG_CONFIG_HOME="${_oc_home:-/tmp}/config" \
		XDG_DATA_HOME="${_oc_home:-/tmp}/data" \
		XDG_CACHE_HOME="${_oc_home:-/tmp}/cache" \
		XDG_STATE_HOME="${_oc_home:-/tmp}/state" \
		"$_oc_bin" --version 2>/dev/null || true)"
	[ -z "$_oc_home" ] || rm -rf "$_oc_home"
	case "$_oc_ver" in
	opencode*) ok "OpenCode ships with the image (${_oc_ver#opencode })" ;;
	*) fail "~/.opencode/bin/opencode does not run: reinstall it from tools/opencode/" ;;
	esac
else
	fail "OpenCode is missing from ~/.opencode/bin: the image ships it (tools/opencode/)"
fi
if grep -qs '\.opencode/bin' "$HOME/.config/bin/rofi-agents"; then
	ok "the Agents menu puts ~/.opencode/bin on PATH"
else
	fail "bin/rofi-agents no longer adds ~/.opencode/bin: OpenCode's row becomes an install row"
fi
# The root menu's AI group opens with the action an agent is for - a session -
# so the section is an action and a list, not only a list. It calls
# `rofi-agents --default`, which starts the recorded default and falls back to
# OpenCode; both halves are asked because either one missing makes the row a
# no-op: a flag the script does not have exits 2 into the journal, and a row
# nobody draws is simply not there.
if grep -qs -- '--default' "$HOME/.config/bin/rofi-agents"; then
	ok "the agents menu starts a session with the default agent (rofi-agents --default)"
else
	fail "bin/rofi-agents lost --default: the AI group's Agent session row does nothing"
fi
grep -qs 'rofi-agents" --default' "$HOME/.config/bin/bar-menu" &&
	ok "the root menu's AI group starts a session with the default agent" ||
	fail "bar-menu lost the Agent session row: its AI group can only open the list"

# The session's own launcher entries. They are the only way to reach the bar's
# panels and the session actions by typing, and a missing one is invisible: the
# list simply has fewer rows and nobody notices. The idle screen's entry is
# checked apart because it is the one the brand owns: its name is the mark's.
_missing_desktop=""
for _d in wifi wifi-nmtui bluetooth audio-output power lock suspend \
	screenshot-area capture lazygit tmux files emoji clipboard about menu install; do
	[ -r "/usr/share/applications/org.lazyos.$_d.desktop" ] || _missing_desktop="$_missing_desktop $_d"
done
[ -r /usr/share/applications/org.lazyos.screensaver.desktop ] ||
	_missing_desktop="$_missing_desktop lazyos-screensaver"
if [ -z "$_missing_desktop" ]; then
	ok "launcher entries present (session actions, nmtui, lazygit, tmux, about)"
else
	fail "launcher entries missing:$_missing_desktop"
fi
# The root menu's entry is the one row whose Exec is a script in $HOME, not a
# binary in $PATH. A .desktop whose Exec does not exist is silently dropped, so
# the row is simply absent and nothing logs it.
if [ -r /usr/share/applications/org.lazyos.menu.desktop ] &&
	grep -qs '^Exec=.*bin/bar-menu$' /usr/share/applications/org.lazyos.menu.desktop &&
	[ -x "$HOME/.config/bin/bar-menu" ]; then
	ok "the root menu has a launcher entry (LazyOS Menu -> bar-menu)"
else
	warn "org.lazyos.menu.desktop is missing or does not run an executable bar-menu"
fi

# waybar reads the backlight by device name and shows an empty module (with no
# error anywhere) when the name does not exist.  Read it with bash's own regex
# rather than a pipe: `set -o pipefail` plus an early-exiting filter is the
# pitfall this script already documents at the top.
_bl=""
_cfg="$HOME/.config/waybar/config.jsonc"
if [ -r "$_cfg" ]; then
	while IFS= read -r _line; do
		if [[ $_line =~ \"device\":\ *\"([^\"]*)\" ]]; then
			_bl=${BASH_REMATCH[1]}
			break
		fi
	done <"$_cfg"
fi
if [ -n "$_bl" ]; then
	if [ -d "/sys/class/backlight/$_bl" ]; then
		ok "waybar backlight device $_bl exists"
	else
		fail "waybar names backlight device '$_bl'; /sys/class/backlight has: $(ls /sys/class/backlight 2>/dev/null | tr '\n' ' ')"
	fi
fi

# The caches the first launch would otherwise build.  rootfs-setup.sh runs both
# commands inside the chroot, where the committed theme already is, so the first
# terminal and the first launcher do not walk the trees.  A rebuilt theme that
# did not regenerate its cache is the silent half: the launcher still opens, it
# just scans 220 files before it draws.
if [ -s /usr/share/icons/LazyOS/icon-theme.cache ]; then
	ok "the icon theme ships its cache (the first launcher scan is a read)"
else
	warn "/usr/share/icons/LazyOS/icon-theme.cache is missing: the first icon scan walks the theme (gtk-update-icon-cache -f /usr/share/icons/LazyOS)"
fi
if [ -d /var/cache/fontconfig ] && [ -n "$(ls -A /var/cache/fontconfig 2>/dev/null)" ]; then
	ok "the system font cache exists (the first font lookup does not walk every face)"
else
	warn "/var/cache/fontconfig is empty or missing: the first font lookup walks every font directory (fc-cache -s)"
fi

# The resident set is the session's strongest rule and had no test: the bar, the
# notification daemon, the idle chain and sway itself are expected, workstyle
# and the cliphist watcher are the two agreed exceptions, and everything else
# should not be running.  The declined daemons are named because each one is a
# package the comments and docs/omarchy-gap.md already answered "no" to, and a
# running one means the rule was broken without anything saying so.
_declined=""
for _p in udiskie swaync dunst picom blueman-applet packagekitd localsearch-3; do
	pgrep -x "$_p" >/dev/null 2>&1 && _declined="$_declined $_p"
done
# `pgrep -x` compares the 15-character comm, and this one's name is longer:
# tracker-miner-fs-3 would never match. The bracketed pattern matches the full
# command line and does not match pgrep's own.
pgrep -f "[t]racker-miner-fs-3" >/dev/null 2>&1 &&
	_declined="$_declined tracker-miner-fs-3"
if [ -z "$_declined" ]; then
	ok "no declined daemon is running (udiskie, swaync, dunst, picom, blueman, packagekit, indexers)"
else
	fail "declined daemon(s) running:$_declined (each is a package this port answered no to; it spends battery and nothing says why)"
fi
# The session's own units (PipeWire, WirePlumber, the portals, gvfs while the
# file manager has a window, dconf) are expected, and so is gpg-agent: gpg 2.x
# routes the passphrase of a symmetric archive through the agent, so the
# Developer menu's export starts it on demand and it idles out by itself (there
# is no enabled unit; the packaged socket activates it).  An unknown one is a warning
# rather than a failure: a distribution update can add one, and the report
# should name it instead of hiding it.  The two agreed residents are `exec`'d by
# the sway configuration, not units, which is why the process checks above
# (workstyle, the cliphist watcher) are where they are seen.
if have systemctl; then
	_unknown_units=""
	for _u in $(systemctl --user list-units --type=service --state=running --no-legend 2>/dev/null | awk '{ print $1 }'); do
		case $_u in
		dbus.service|dconf.service|gvfs-*.service|mpris-proxy.service|\
		pipewire.service|pipewire-pulse.service|filter-chain.service|\
		wireplumber.service|wireplumber@*.service|xdg-desktop-portal*.service|\
		xdg-document-portal.service|xdg-permission-store.service|\
		gpg-agent.service|keyboxd.service) ;;
		*) _unknown_units="$_unknown_units $_u" ;;
		esac
	done
	if [ -z "$_unknown_units" ]; then
		ok "the user units are the expected ones (PipeWire, WirePlumber, portals, gvfs)"
	else
		warn "unexpected user unit(s):$_unknown_units (the resident set grew; AGENTS.md says nothing resident)"
	fi
fi

# ---------------------------------------------------------------------------
section "The framework"
# ---------------------------------------------------------------------------
# The session's Rust framework (docs/plan-ui.md): one `lazyos` binary that the
# rofi/bars menus become symlinks to. Spec 001 lands the binary, the dispatch,
# the host and the terminal; every check below is what would otherwise fail
# silently - a key that opens nothing because the binary is missing, a symlink
# that points at a name the table does not carry, or a window that tiles because
# its shape lost the rule that floats it.
if have lazyos; then
	ok "lazyos is installed ($(command -v lazyos))"
	_fw_version=$(lazyos --version 2>/dev/null)
	_rev_version=$(sed -n 's/^version //p' /etc/debian-aarch64-revision 2>/dev/null)
	if [ -n "$_fw_version" ] && [ -n "$_rev_version" ]; then
		if [ "$_fw_version" = "$_rev_version" ]; then
			ok "lazyos reports the version the image was built from ($_fw_version)"
		else
			fail "lazyos reports $_fw_version but the image was built from $_rev_version (a stale /usr/bin/lazyos)"
		fi
	else
		skip "cannot compare the framework version (/etc/debian-aarch64-revision has no version)"
	fi
	_fw_help=$(lazyos --help 2>/dev/null)
	if grep -q '^  menu' <<<"$_fw_help"; then
		ok "lazyos --help lists the menu"
	else
		fail "lazyos --help does not list 'menu': the command table and the binary disagree"
	fi
	# The migrated set: every name the installer creates today must resolve and
	# must belong to a command the help lists. This is the check that notices a
	# renamed subcommand leaving a symlink pointing at nothing, which fails as
	# "the key does nothing" in the session.
	_fw_links=$(lazyos --symlinks 2>/dev/null)
	if [ -n "$_fw_links" ]; then
		while IFS='	' read -r _fw_name _fw_path; do
			[ -n "$_fw_name" ] || continue
			if [ -L "/usr/bin/$_fw_name" ] && [ -x "/usr/bin/$_fw_name" ]; then
				if grep -q "^  $_fw_path" <<<"$_fw_help"; then
					ok "/usr/bin/$_fw_name -> lazyos $_fw_path"
				else
					fail "/usr/bin/$_fw_name points at '$_fw_path', which --help does not list"
				fi
			else
				fail "/usr/bin/$_fw_name is missing or does not resolve to the lazyos binary"
			fi
		done <<<"$_fw_links"
	fi
	# The other direction: a link someone made by hand, pointing at the
	# framework but absent from the table, is drift - the table is the source of
	# truth and `--symlinks` is what the installer and this check both read.
	for _fw_link in "$HOME"/.config/bin/*; do
		[ -L "$_fw_link" ] || continue
		[ "$(readlink -f "$_fw_link" 2>/dev/null)" = "/usr/bin/lazyos" ] || continue
		_fw_base=$(basename "$_fw_link")
		if grep -q "^$_fw_base	" <<<"$_fw_links"; then
			ok "~/.config/bin/$_fw_base is a framework symlink the table knows"
		else
			warn "~/.config/bin/$_fw_base points at lazyos but the command table does not list it"
		fi
	done
else
	fail "lazyos is missing: every menu that becomes a symlink to it does nothing"
fi

# The window shapes: the binary asks foot for its pixels, and only the
# compositor can float and centre it (docs/plan-ui.md §3). Without these rules a
# menu opens as a tiled terminal that looks like the shell was replaced.
_fw_sway=$(cat /etc/sway/config.d/*.conf "$HOME"/.config/sway/config.d/*.conf 2>/dev/null)
for _fw_shape in pick task; do
	if grep -q "for_window \[app_id=\"lazyos-$_fw_shape\"\] floating enable" <<<"$_fw_sway" &&
		grep -q "for_window \[app_id=\"lazyos-$_fw_shape\"\] move position center" <<<"$_fw_sway"; then
		ok "sway floats and centres lazyos-$_fw_shape"
	else
		fail "sway has no floating/centre rule for app_id lazyos-$_fw_shape (the window tiles)"
	fi
done

# The widgets draw two alphabets, and they come from two different fonts: the
# frame's box-drawing and block characters are text (JetBrains Mono, first in
# foot's chain), and a row's picture is a Nerd Font codepoint from the symbol
# font (spec 003).  A codepoint no font carries is a hole in the window that logs
# nothing, which is the failure the rofi alphabet already shipped once in another
# surface.
#
# `lazyos --glyphs` prints both lists from the binary, one `U+XXXX<TAB>name` per
# line, and `--glyph-font` prints the pin the alphabet was filtered against, so
# neither can drift from what a window actually draws.  Three things are
# checked, and each reads as something else when it breaks:
#
#  1. the font named by the pin is installed.  fontconfig answers a missing
#     family with a substitute, which renders half the codepoints and not the
#     other half - a window of boxes with no error anywhere;
#  2. the file is the one the table was generated from.  A font upgrade that
#     drops a codepoint turns that row into a hole, and regenerating the table
#     (`scripts/make-glyphs.sh`) is the fix;
#  3. every alphabet codepoint is in that font's own character map, asked once
#     with `fc-query` (a call per glyph cost two seconds of the report on a 4 GB
#     device).  The frame's characters are asked of the union of the installed
#     fonts instead: they are text, and any font that draws them will do.
#
# The eight names `fonts-nerd-symbols` (DFSG-stripped) does not draw are text on
# purpose - `bt`, `wifi`, `bat`, `kbd`, `pad`, `=`, `qr`, `crop` - so the pin is
# also asked which ones those are; a name that answered with a box would be the
# bug this alphabet exists to prevent.
if have lazyos && have fc-list; then
	_fw_glyphs=$(lazyos --glyphs 2>/dev/null)
	_fw_font=$(lazyos --glyph-font 2>/dev/null)
	_fw_total=$(printf '%s\n' "$_fw_glyphs" | awk 'NF { n++ } END { print n + 0 }')
	_fw_family=$(sed -n 's/^family=//p' <<<"$_fw_font")
	_fw_pin=$(sed -n 's/^sha256=//p' <<<"$_fw_font")
	_fw_nerd=$(sed -n 's/^nerd=//p' <<<"$_fw_font")
	_fw_fallback=$(sed -n 's/^fallback=//p' <<<"$_fw_font")
	_fw_text=$(sed -n 's/^text=//p' <<<"$_fw_font")
	if [ "$_fw_total" -eq 0 ] || [ -z "$_fw_family" ] || [ -z "$_fw_pin" ]; then
		fail "lazyos --glyphs/--glyph-font printed nothing: the binary predates the alphabet (spec 003)"
	else
		_fw_path=$(fc-match -f '%{file}' "$_fw_family" 2>/dev/null) || _fw_path=
		_fw_families=$(fc-query --format='%{family}' "$_fw_path" 2>/dev/null | tr ',' '\n' | sed 's/^ *//')
		if [ -z "$_fw_path" ] || [ ! -r "$_fw_path" ]; then
			fail "fontconfig resolves no font for '$_fw_family': the rows have nothing to draw (apt install fonts-nerd-symbols)"
		elif ! grep -qxF "$_fw_family" <<<"$_fw_families"; then
			fail "fontconfig resolves '$_fw_family' to $_fw_path, which is not that family: install fonts-nerd-symbols"
		else
			_fw_now=$(sha256sum "$_fw_path" 2>/dev/null)
			_fw_now=${_fw_now%% *}
			if [ "$_fw_now" = "$_fw_pin" ]; then
				ok "the alphabet's font is the one it was generated from ($_fw_family, Nerd Fonts $_fw_nerd)"
			else
				fail "the alphabet's pin is for another font ($_fw_now != $_fw_pin): a font upgrade needs scripts/make-glyphs.sh, or the rows it changed are empty boxes"
			fi
			_fw_symbol=$(fc-query --format='%{charset}' "$_fw_path" 2>/dev/null)
			# The union of the installed character maps is far too big for an
			# awk -v argument (ARG_MAX said so the first time this ran): it
			# travels on stdin, one range per line.
			_fw_union=$(fc-list -f '%{charset}\n' 2>/dev/null | tr ' ' '\n' | sort -u)
			if [ -z "$_fw_union" ]; then
				fail "fc-list reports no character maps at all: the glyph check cannot answer"
			else
				_fw_missing=$(printf '%s\n' "$_fw_union" | awk -v glyphs="$_fw_glyphs" -v symbol="$_fw_symbol" '
					function dec(h,   i, v) {
						v = 0
						for (i = 1; i <= length(h); i++)
							v = v * 16 + index("0123456789abcdef", tolower(substr(h, i, 1))) - 1
						return v
					}
					function add(token, lo, hi, index_,   range, a, b) {
						if (token == "") return index_
						if (index(token, "-")) {
							split(token, range, "-")
							a = dec(range[1])
							b = dec(range[2])
						} else {
							a = b = dec(token)
						}
						lo[index_] = a
						hi[index_] = b
						return index_ + 1
					}
					function within(cp, lo, hi, count,   i) {
						for (i = 0; i < count; i++)
							if (cp >= lo[i] && cp <= hi[i]) return 1
						return 0
					}
					BEGIN {
						ucount = 0
						scount = 0
						n = split(symbol, token, /[ \t\n]+/)
						for (i = 1; i <= n; i++) scount = add(token[i], slo, shi, scount)
					}
					{ ucount = add($0, ulo, uhi, ucount) }
					END {
						n = split(glyphs, line, "\n")
						for (i = 1; i <= n; i++) {
							if (line[i] == "") continue
							split(line[i], field, "\t")
							cp = dec(tolower(substr(field[1], 3)))
							if (field[2] == "frame") {
								if (!within(cp, ulo, uhi, ucount)) frame = frame " " field[1]
							} else if (!within(cp, slo, shi, scount)) {
								alphabet = alphabet " " field[2] " " field[1]
							}
						}
						print "frame" frame
						print "alphabet" alphabet
					}')
				_fw_miss_frame=$(sed -n 's/^frame //p' <<<"$_fw_missing")
				_fw_miss_alpha=$(sed -n 's/^alphabet //p' <<<"$_fw_missing")
				if [ -z "$_fw_miss_frame" ]; then
					ok "the frame's characters render (no installed font is missing one)"
				else
					fail "no installed font renders:$_fw_miss_frame (a hole in every window that draws them, and nothing logs it)"
				fi
				if [ -z "$_fw_miss_alpha" ]; then
					ok "the rows' glyphs render ($_fw_total codepoints, every one in $_fw_family)"
				else
					fail "$_fw_family does not carry:$_fw_miss_alpha (a hole in every window that draws them, and nothing logs it)"
				fi
			fi
			_fw_bad_text=$(printf '%s\n' "$_fw_text" | sed -n '/^[a-z_]* [A-Za-z0-9=]\{1,4\}$/!p')
			if [ "${_fw_fallback:-0}" -eq 0 ] || [ -z "$_fw_text" ]; then
				fail "--glyph-font names no text fallbacks: the names the stripped symbol set lacks would draw boxes"
			elif [ -n "$_fw_bad_text" ]; then
				fail "a glyph name falls back to something that is not short text: $(printf '%s' "$_fw_bad_text" | tr '\n' ' ')"
			else
				ok "$_fw_fallback names the symbol set does not draw render text, not boxes"
			fi
		fi
	fi
fi

# A command that draws its own frame is the first step back to thirty windows
# with thirty shapes; the shell in lazyos-ui owns the header, the body and the
# key bar.  This is the same grep the crate's own test runs, so a checkout
# edited without the test suite is still caught on the device.
if [ -n "$REPO" ] && [ -d "$REPO/rust/crates/lazyos-cli/src" ]; then
	if grep -rqs 'ratatui::Frame\|&mut Frame' "$REPO/rust/crates/lazyos-cli/src"; then
		fail "a command in lazyos-cli draws its own Frame (the App shell owns the window)"
	else
		ok "every command draws through the App shell (no Frame in lazyos-cli)"
	fi
else
	skip "no checkout: cannot confirm the commands draw through the shell"
fi

# ---------------------------------------------------------------------------
section "The theme engine"
# ---------------------------------------------------------------------------
# The palette lives in theme.toml and every surface is generated from it (spec
# 004).  Four things are checked, and each is a failure that reads as something
# else: no active theme means `lazyos theme next` has no place to start; a
# regeneration diff means a hand edit or a stale image, and the surface will
# drift on the next switch; a missing header means the engine does not own that
# file and would refuse to replace it; a missing binding means the keys that
# switch the palette do nothing.
if command -v lazyos >/dev/null 2>&1; then
	_theme_current=$(lazyos theme current 2>/dev/null || true)
	if [ -z "$_theme_current" ]; then
		fail "no active theme (\`lazyos theme set solarized\` sets one)"
	else
		_theme_slug=${_theme_current%%	*}
		_theme_name=${_theme_current#*	}
		ok "the active theme is $_theme_slug ($_theme_name)"

		_theme_missing=
		for _theme_file in \
			.config/foot/foot.ini .config/waybar/style.css .config/mako/config \
			.config/rofi/solarized.rasi .config/starship.toml \
			.config/btop/themes/lazyos.theme .config/lazygit/config.yml \
			.config/nvim/colors/lazyos.vim .config/lazyos/fzf.env \
			.config/swaylock/config .config/sway/config.d/35-theme.conf
		do
			if [ ! -r "$HOME/$_theme_file" ]; then
				_theme_missing="$_theme_missing $_theme_file"
			elif ! head -n 1 "$HOME/$_theme_file" | grep -q "LazyOS theme:"; then
				_theme_missing="$_theme_missing $_theme_file(no header)"
			fi
		done
		if [ -n "$_theme_missing" ]; then
			fail "a generated surface is missing or was not generated:$_theme_missing"
		else
			ok "every surface is generated and carries the theme header"
		fi

		# The regeneration witness: rendering again must reproduce the files
		# that are installed, byte for byte.  The exit status is the answer;
		# `--quiet` prints only the surfaces that differ.
		if _theme_diff=$(lazyos theme diff --quiet 2>&1); then
			ok "every surface is byte-identical to a fresh render"
		else
			fail "regeneration differs on: $(printf '%s' "$_theme_diff" | head -n 1) (run \`lazyos theme set $_theme_slug\`)"
		fi
	fi

	if lazyos theme validate >/dev/null 2>&1; then
		ok "every built-in theme parses and clears its contrast floor"
	else
		fail "a built-in theme does not parse or fails its contrast floor (\`lazyos theme validate\`)"
	fi
else
	skip "no lazyos binary: the theme engine is not installed"
fi

# The two keys, parsed out of the configuration the way rofi-keys parses them.
_theme_keys=$(grep -rhs '^bindsym .*exec lazyos theme ' /etc/sway/config.d/*.conf "$HOME/.config/sway/config.d/"*.conf 2>/dev/null || true)
for _theme_key in "theme next" "theme wallpaper next"; do
	if printf '%s' "$_theme_keys" | grep -qF "$_theme_key"; then
		ok "the binding for \`lazyos $_theme_key\` is present"
	else
		fail "no binding runs \`lazyos $_theme_key\` (Alt+Shift+T and Alt+Shift+W)"
	fi
done

# ---------------------------------------------------------------------------
section "The brand"
# ---------------------------------------------------------------------------
# /etc/os-release is Debian's symlink to /usr/lib/os-release, and base-files
# owns that file: a base-files upgrade can put the symlink back and the image
# then silently calls itself Debian.  The build writes a regular file; this is
# what notices when it is gone.
if [ -r /etc/os-release ]; then
	_os_id=$( (. /etc/os-release 2>/dev/null; printf '%s' "${ID:-}") )
	_os_name=$( (. /etc/os-release 2>/dev/null; printf '%s' "${PRETTY_NAME:-}") )
	_os_ver=$( (. /etc/os-release 2>/dev/null; printf '%s' "${VERSION_ID:-}") )
	if [ "$_os_id" = lazyos ] && [[ $_os_name == LazyOS* ]]; then
		ok "os-release says $_os_name (ID=$_os_id)"
	else
		fail "/etc/os-release is not LazyOS (ID='${_os_id}', PRETTY_NAME='${_os_name}'): base-files may have restored its symlink"
	fi
	# The image's own record of what it was built from.  A hand edit of
	# os-release is allowed; saying a different version than the build did is
	# what this catches.
	_rev_ver=$(sed -n 's/^version //p' /etc/debian-aarch64-revision 2>/dev/null)
	if [ -n "$_rev_ver" ] && [ -n "$_os_ver" ] && [ "$_rev_ver" != "$_os_ver" ]; then
		warn "os-release says VERSION_ID=$_os_ver, but the image was built from version $_rev_ver"
	fi
else
	fail "/etc/os-release is missing"
fi

# The desktop wears the brand's keyboard wallpaper and the lock screen the
# session's own mountain photograph; a missing file leaves sway painting a flat
# colour with no message.
if [ -r "$HOME/Pictures/Wallpapers/lazyos.png" ]; then
	ok "desktop wallpaper present"
else
	fail "missing Pictures/Wallpapers/lazyos.png: the desktop falls back to a flat colour"
fi
if grep -qs 'Pictures/Wallpapers/lazyos.png' /etc/sway/config "$HOME/.config/sway/config" 2>/dev/null; then
	ok "sway paints the brand wallpaper"
else
	fail "sway does not name Pictures/Wallpapers/lazyos.png (output * bg)"
fi
if [ -r "$HOME/Pictures/Wallpapers/mountains.jpg" ]; then
	ok "lock screen wallpaper present"
else
	fail "missing Pictures/Wallpapers/mountains.jpg: the lock falls back to a flat colour"
fi
if grep -qs 'mountains.jpg' "$HOME/.config/bin/sway-lock" 2>/dev/null; then
	ok "lock screen keeps the mountain wallpaper"
else
	warn "sway-lock no longer names mountains.jpg: the lock falls back to a flat colour"
fi

# The mark itself: the About entry asks for Icon=lazyos, and a theme without it
# draws a hole where a picture should be, quietly.
_missing_mark=""
for _f in 48x48/apps/lazyos.png scalable/apps/lazyos.svg; do
	[ -r "/usr/share/icons/LazyOS/$_f" ] || _missing_mark="$_missing_mark $_f"
done
if [ -z "$_missing_mark" ]; then
	ok "the theme carries the mark (lazyos.png, lazyos.svg)"
else
	fail "the theme is missing the mark:$_missing_mark (About LazyOS shows a hole)"
fi

# The card the launcher's About row opens, and the logo fastfetch draws there.
if [ -r "$HOME/.config/fastfetch/config.jsonc" ]; then
	ok "fastfetch config present"
else
	warn "no ~/.config/fastfetch/config.jsonc: About shows Debian's swirl"
fi
if [ -r "$HOME/.config/lazyos/wordmark.txt" ]; then
	ok "fastfetch wordmark present"
else
	warn "no ~/.config/lazyos/wordmark.txt: fastfetch's logo has nothing to draw"
fi

# The greeter and the virtual terminals are the first and the cheapest surfaces
# a person sees, and both are files a later edit can quietly lose.
grep -qs 'lazy by design' /etc/greetd/config.toml 2>/dev/null &&
	ok "the greeter greets with lazy by design" ||
	warn "the greeter no longer greets with lazy by design"
grep -qs 'LazyOS' /etc/issue 2>/dev/null &&
	ok "virtual terminals greet LazyOS" ||
	warn "/etc/issue does not name LazyOS"

# The rename was a clean break, so a surviving old name is a reference that was
# missed.  Scoped to the shipped surfaces on purpose: ~/Dev/LazyOS carries the
# frozen release notes of the past, and reports/ is hardware evidence.
_stale=$(ls /usr/share/applications/org.nureb.*.desktop 2>/dev/null || true)
if [ -n "$_stale" ]; then
	fail "old desktop entries survive: $(printf '%s ' $_stale)"
else
	ok "no org.nureb.* entries survive"
fi
_stale=$(grep -rlis 'nureb' /usr/share/applications /usr/share/icons/LazyOS \
	/usr/share/doc/lazyos "$HOME/.config/bin" 2>/dev/null || true)
if [ -n "$_stale" ]; then
	warn "the old name still appears in: $(printf '%s\n' "$_stale" | sed -n '1,3p' | tr '\n' ' ')"
fi

# ---------------------------------------------------------------------------
section "Fonts and icons"
# ---------------------------------------------------------------------------
if have fc-list; then
	_families="$(fc-list : family 2>/dev/null | tr ',' '\n' | sed 's/^ *//' | sort -u)"
	for _f in "JetBrains Mono" "FontAwesome" "Material Design Icons" "Symbols Nerd Font"; do
		if grep -qF "$_f" <<<"$_families"; then
			ok "font family present: $_f"
		else
			fail "font family missing: $_f (waybar icons render as tofu boxes)"
		fi
	done
else
	skip "fc-list is not installed"
fi

# The menus draw a Font Awesome codepoint from the alphabet in lib-rofi.sh, and a
# name is only a picture while some installed font still carries that codepoint:
# a name nobody renders draws nothing at all (a hole on the row, a text-only
# card) and logs nothing.  One fontconfig call answers for every glyph at once
# (the union of the charsets installed fonts cover), because a call per glyph
# cost two seconds of the report on a 4 GB device.
_alphabet="$HOME/.config/bin/lib-rofi.sh"
if have fc-list && [ -r "$_alphabet" ]; then
	_glyphs="$(sed -n 's/^\(UI_I_[A-Z_]*\)=.*#[[:space:]]*\([0-9a-f][0-9a-f]*\)$/\1 \2/p' "$_alphabet")"
	_glyph_total="$(printf '%s\n' "$_glyphs" | awk 'NF { n++ } END { print n + 0 }')"
	if [ "$_glyph_total" -eq 0 ]; then
		fail "no UI_I_* glyphs parsed from $_alphabet (the menus would draw empty spans)"
	else
		_missing_glyph="$(fc-list -f '%{charset}\n' 2>/dev/null | tr ' ' '\n' | sort -u |
			awk -v glyphs="$_glyphs" '
			function dec(h,   i, v) {
				v = 0
				for (i = 1; i <= length(h); i++)
					v = v * 16 + index("0123456789abcdef", substr(h, i, 1)) - 1
				return v
			}
			BEGIN {
				n = split(glyphs, line, "\n")
				for (i = 1; i <= n; i++) {
					if (line[i] == "") continue
					split(line[i], f, " ")
					want[i] = dec(f[2])
					name[i] = f[1]
					hex[i] = f[2]
				}
			}
			{
				lo = $1
				if (index(lo, "-")) {
					split(lo, r, "-")
					a = dec(r[1])
					b = dec(r[2])
				} else {
					a = b = dec(lo)
				}
				for (i = 1; i <= n; i++)
					if (!seen[i] && want[i] >= a && want[i] <= b) seen[i] = 1
			}
			END {
				for (i = 1; i <= n; i++)
					if (want[i] && !seen[i]) out = out " " name[i] "(U+" hex[i] ")"
				print out
			}')"
		if [ -z "$_missing_glyph" ]; then
			ok "menu alphabet renders ($_glyph_total glyphs, every codepoint covered by a font)"
		else
			fail "no installed font renders:$_missing_glyph (a hole on every row that uses it, and nothing logs it)"
		fi
	fi
fi

# The launcher shows application *images*, and Debian's Adwaita has none of this
# session's names, so the image ships the small LazyOS theme (48x48 PNGs rendered
# from the Font Awesome codepoints). Without it every org.lazyos.* row is a hole.
_icons=""
[ -d /usr/share/icons/LazyOS ] || _icons="$_icons /usr/share/icons/LazyOS"
[ -r /usr/share/icons/LazyOS/index.theme ] || _icons="$_icons LazyOS/index.theme"
_iconless=""
# The session's own entries, plus the viewer packages': their Icon= is not their
# binary name (imv asks for multimedia-photo-viewer, papers for org.gnome.Papers)
# and Adwaita 51 has neither, so the same hole appears on those rows.
for _d in /usr/share/applications/org.lazyos.*.desktop \
	/usr/share/applications/org.gnome.Papers.desktop \
	/usr/share/applications/imv.desktop \
	/usr/share/applications/mpv.desktop; do
	[ -r "$_d" ] || continue
	_icon=""
	while IFS= read -r _line; do
		case $_line in Icon=*) _icon=${_line#Icon=}; break ;; esac
	done <"$_d"
	[ -n "$_icon" ] || continue
	[ -r "/usr/share/icons/LazyOS/48x48/apps/$_icon.png" ] && continue
	# An application's own icon (hicolor, pixmaps) is fine too.
	[ -n "$(find /usr/share/icons /usr/share/pixmaps -name "$_icon.*" 2>/dev/null | head -1)" ] && continue
	_iconless="$_iconless $(basename "$_d" .desktop)=$_icon"
done
if [ -z "$_icons" ] && [ -z "$_iconless" ]; then
	ok "launcher icons complete (LazyOS theme, every session entry resolves)"
elif [ -z "$_icons" ]; then
	fail "launcher entries without an icon:$_iconless (a hole in the launcher row)"
else
	fail "icon theme incomplete:$_icons (the launcher shows empty spaces)"
fi
# rofi's run mode (Alt+Shift+D) asks the icon theme for an icon *named after the
# binary* it would run, not for a .desktop icon (source/modes/run.c), so the
# command palette only has icons because make-icons.sh renders them under those
# names too.  Missing ones are a blank 26 px column, not an error.
_runless=""
for _c in foot btop htop lazygit tmux nvim fzf rg bat eza mpv imv firefox-esr; do
	[ -r "/usr/share/icons/LazyOS/48x48/apps/$_c.png" ] || _runless="$_runless $_c"
done
if [ -z "$_runless" ]; then
	ok "command palette icons present (the run mode resolves binaries by name)"
else
	warn "command palette icons missing for:$_runless (blank column, not a failure)"
fi

if have grep; then
	_cfg_rasi="$HOME/.config/rofi/config.rasi"
	if [ -r "$_cfg_rasi" ]; then
		_show=$(grep -c 'show-icons: *true' "$_cfg_rasi" 2>/dev/null || true)
		_theme=$(grep -c 'icon-theme: *"LazyOS"' "$_cfg_rasi" 2>/dev/null || true)
		if [ "${_show:-0}" -ge 1 ] && [ "${_theme:-0}" -ge 1 ]; then
			ok "rofi shows icons from the LazyOS theme (show-icons, icon-theme)"
		else
			warn "rofi/config.rasi does not enable show-icons and icon-theme \"LazyOS\""
		fi
	fi
fi

# ---------------------------------------------------------------------------
section "Declared packages"
# ---------------------------------------------------------------------------
if [ -n "$REPO" ] && have dpkg-query; then
	_missing=""
	_count=0
	for _list in "$REPO"/packages/*.list "$REPO"/devices/*/packages.list "$REPO"/uis/*/packages.list; do
		[ -f "$_list" ] || continue
		while read -r _pkg; do
			case "$_pkg" in '' | '#'*) continue ;; esac
			_count=$((_count + 1))
			_st="$(dpkg-query -W -f='${Status}' "$_pkg" 2>/dev/null || true)"
			grep -q 'install ok installed' <<<"$_st" || _missing="$_missing $_pkg"
		done <"$_list"
	done
	if [ -z "$_missing" ]; then
		ok "all $_count declared package(s) are installed"
	else
		fail "declared but not installed:$_missing"
	fi
else
	skip "no repository checkout: run scripts/check-system.sh <repo> to check declared packages"
fi

# ---------------------------------------------------------------------------
section "Journal"
# ---------------------------------------------------------------------------
_errs="$(asroot journalctl -b -p err --no-pager | grep -vc '^-- No entries --')"
if [ "${_errs:-0}" -eq 0 ]; then
	ok "no error-priority messages this boot"
else
	warn "$_errs error-priority line(s) this boot; the last ones:"
	asroot journalctl -b -p err --no-pager | tail -6 | sed 's/^/          /'
fi

# ---------------------------------------------------------------------------
printf '\n%s==> Summary%s  %d OK, %d WARN, %d FAIL\n' "$C_HEAD" "$C_OFF" "$PASS" "$WARN" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
