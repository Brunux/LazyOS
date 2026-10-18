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
# devices/*/packages.list and the running UI's uis/<ui>/packages.list is
# installed.
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

# Which UI this system was built with. build-rootfs.sh records it in
# /etc/debian-aarch64-revision, and the UI-specific checks key on it: running the
# sway checks (or validating uis/sway/packages.list) on an Omarchy image reports
# the *other* UI's packages as missing, which is noise, not a regression.
_UI="$(sed -n 's/^ui //p' /etc/debian-aarch64-revision 2>/dev/null)"
if [ -z "$_UI" ]; then
	if have Hyprland; then
		_UI=omarchy
	elif have sway; then
		_UI=sway
	fi
fi

# sway -C needs a runtime dir; keep one for the whole run.
RUNTIME="$(mktemp -d)"
trap 'rm -rf "$RUNTIME"' EXIT

printf '%sdebian-aarch64 system report%s  %s\n' "$C_HEAD" "$C_OFF" "$(date '+%Y-%m-%d %H:%M:%S %Z')"
[ -n "$_UI" ] && info "UI: $_UI"

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
if [ "$_UI" = sway ]; then
	[ -x "$HOME/.config/bin/sway-suspend" ] && ok "sway-suspend helper is executable" ||
		warn "~/.config/bin/sway-suspend is missing or not executable"
fi
_susp="$(asroot journalctl -b --no-pager | grep -c 'PM: suspend entry')"
[ "${_susp:-0}" -ge 1 ] && ok "$_susp suspend/resume cycle(s) recorded in this boot" ||
	warn "no suspend recorded in this boot: press the Lock key to test it"

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
if [ "$_UI" = sway ] && have sway; then
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
case "$_UI" in
sway)
	_missing_cmds=""
	for _c in waybar alacritty bemenu swaylock swaynag swaybg brightnessctl grimshot wlsunset wpctl pactl; do
		have "$_c" || _missing_cmds="$_missing_cmds $_c"
	done
	if [ -z "$_missing_cmds" ]; then
		ok "every command the sway configuration invokes is installed"
	else
		warn "commands referenced by the sway configuration are missing:$_missing_cmds"
	fi
	;;
omarchy)
	_missing_cmds=""
	for _c in Hyprland hyprctl quickshell uwsm brightnessctl pamixer wpctl; do
		have "$_c" || _missing_cmds="$_missing_cmds $_c"
	done
	if [ -z "$_missing_cmds" ]; then
		ok "every command the Omarchy session invokes is installed"
	else
		fail "commands the Omarchy session invokes are missing:$_missing_cmds"
	fi
	;;
esac

# ---------------------------------------------------------------------------
if [ "$_UI" = omarchy ]; then
section "Omarchy"
	[ -d /usr/share/omarchy ] && ok "/usr/share/omarchy is present" ||
		fail "/usr/share/omarchy is missing: the shell and the omarchy-* commands have nothing to run"
	[ -f /usr/share/wayland-sessions/omarchy.desktop ] &&
		ok "the omarchy Wayland session entry is installed" ||
		fail "the omarchy Wayland session entry is missing: tuigreet has no session to pick"
	grep -qs 'omarchy-session' /etc/greetd/config.toml &&
		ok "greetd starts the Omarchy session (omarchy-session)" ||
		warn "greetd does not run omarchy-session: the session menu entry still works"
	[ -f "$HOME/.config/hypr/hyprland.lua" ] &&
		ok "~/.config/hypr/hyprland.lua is present" ||
		fail "~/.config/hypr/hyprland.lua is missing: Hyprland has no user config"
	_theme="$(head -n1 "$HOME/.local/state/omarchy/current/theme.name" 2>/dev/null)"
	[ -n "$_theme" ] && ok "current theme: $_theme" ||
		warn "no current theme recorded (the theme is applied on first login)"
	_ncmd="$(find /usr/bin -maxdepth 1 -name 'omarchy-*' 2>/dev/null | wc -l)"
	[ "$_ncmd" -gt 0 ] && ok "$_ncmd omarchy-* command(s) on PATH" ||
		fail "no omarchy-* command on PATH: the shell and the key bindings cannot call them"
	# Hyprland's own view of the config: ask the running compositor when there is
	# one, otherwise parse it offline. HYPRLAND_INSTANCE_SIGNATURE is pinned when
	# unset so the offline parse does not trip on Omarchy reading it back.
	if have hyprctl && hyprctl -j monitors >/dev/null 2>&1; then
		_cfgerr="$(hyprctl -j configerrors 2>/dev/null)"
		case "$_cfgerr" in
			'' | '[]' | 'null') ok "the running Hyprland reports no configuration error" ;;
			*) fail "Hyprland reports configuration errors: $_cfgerr" ;;
		esac
	elif have Hyprland; then
		_hout="$(OMARCHY_PATH=/usr/share/omarchy \
			HYPRLAND_INSTANCE_SIGNATURE="${HYPRLAND_INSTANCE_SIGNATURE:-check-system}" \
			XDG_RUNTIME_DIR="$RUNTIME" Hyprland --verify-config 2>&1 || true)"
		if grep -qi 'lua error' <<<"$_hout"; then
			fail "Hyprland reported a Lua configuration error:"
			grep -i 'lua error' <<<"$_hout" | tail -3 | sed 's/^/          /'
		else
			ok "Hyprland parsed the Lua configuration without a Lua error"
		fi
	fi
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

# ---------------------------------------------------------------------------
section "Declared packages"
# ---------------------------------------------------------------------------
if [ -n "$REPO" ] && have dpkg-query; then
	_missing=""
	_count=0
	# Only the UI this system booted; the other uis/*/packages.list belongs to a
	# desktop that is not installed here (see the _UI note above).
	_ui_lists="$REPO"/uis/*/packages.list
	if [ -n "$_UI" ] && [ -f "$REPO/uis/$_UI/packages.list" ]; then
		_ui_lists="$REPO/uis/$_UI/packages.list"
	fi
	for _list in "$REPO"/packages/*.list "$REPO"/devices/*/packages.list $_ui_lists; do
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
