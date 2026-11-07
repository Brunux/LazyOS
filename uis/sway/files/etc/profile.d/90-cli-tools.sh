# Environment for the CLI tools in packages/tools.list.
#
# This is the one copy of these settings: /etc/profile.d runs it for login
# shells (SSH, `bash -l`), and ~/.bashrc sources it for the terminal, so the two
# cannot drift apart.
#
# Debian installs bat and fd-find as batcat and fdfind; scripts/rootfs-setup.sh
# adds the /usr/local/bin/bat and /usr/local/bin/fd symlinks that these settings
# and the aliases call.

# bat as the manual pager. col -bx does what man's own pager does: strip the
# overstrike (bold is "b\bbo\bold") and expand tabs, so bat receives plain text
# (bat -l man says "this is a man page", -p drops the header, line numbers and
# git status). MANROFFOPT=-c is man's side of it: pass -c to nroff, which keeps
# the overstrike this pipeline expects instead of producing SGR sequences.
export MANROFFOPT="-c"
export MANPAGER="sh -c 'col -bx | bat -l man -p'"

# `ansi` makes bat paint with the terminal's own 16 colours instead of one of its
# own truecolour themes; those 16 are the solarized palette in
# ~/.config/foot/foot.ini, so a file previewed by bat matches the session.
export BAT_THEME=ansi

# /usr/local/bin is where rootfs-setup.sh puts the `bat` and `fd` symlinks, and
# it is not guaranteed to be on PATH by the time this runs: it comes from
# /etc/profile for a login shell, but a terminal opened from the session inherits
# whatever the session manager exported, which can be just
# /usr/sbin:/usr/bin:/sbin:/bin (measured inside a chroot, where neither the
# alias nor `command -v bat` saw the binary). Prepending is safe - the directory
# is root-owned - and it is what Debian's own ENV_PATH/login.defs puts first.
case ":$PATH:" in
	*":/usr/local/bin:"*) ;;
	*) PATH="/usr/local/bin${PATH:+:$PATH}" ;;
esac

# Session scripts and helpers live here (~/.config/bin).
# Prepended so user scripts and ported symlinks are discoverable.
if [ -n "${HOME:-}" ]; then
	case ":$PATH:" in
		*":$HOME/.config/bin:"*) ;;
		*) PATH="$HOME/.config/bin${PATH:+:$PATH}" ;;
	esac
fi

# User-installed binaries (cargo, pipx, a user-level npm prefix) live here.
# Appended, so a system package always wins over a user copy of the same name.
if [ -n "${HOME:-}" ]; then
	case ":$PATH:" in
		*":$HOME/.local/bin:"*) ;;
		*) PATH="${PATH:+$PATH:}$HOME/.local/bin" ;;
	esac
fi

# OpenCode is the one agent the image ships (tools/opencode/, installed by
# scripts/rootfs-setup.sh). It lives in the upstream installer's layout,
# ~/.opencode/bin, because that is where its own `opencode upgrade` rewrites it;
# the Agents menu adds the same directory to its PATH, where sway's environment
# has neither this nor ~/.local/bin.
if [ -n "${HOME:-}" ]; then
	case ":$PATH:" in
		*":$HOME/.opencode/bin:"*) ;;
		*) PATH="${PATH:+$PATH:}$HOME/.opencode/bin" ;;
	esac
fi
export PATH

# The editor. Deboma exports EDITOR=omarchy-launch-editor here, a wrapper that
# decides between a TUI and a GUI; there is no such wrapper in this image, so
# EDITOR and VISUAL name the TUI editor directly. That is nvim (packages/
# tools.list, Omarchy's editor), and it is what eff, `git commit` and
# `crontab -e` open. Only when nothing has set them already - an EDITOR from an
# ssh session or a user's own export wins - and only when the binary is there,
# because an EDITOR naming a program that does not exist makes those three fail
# instead of falling back to nano.
if [ -z "${EDITOR:-}" ] && command -v nvim >/dev/null 2>&1; then
	export EDITOR=nvim
	export VISUAL=nvim
fi
