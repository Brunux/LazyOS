# uis/omarchy — the Omarchy v4 desktop on Debian

A UI is a package list plus a file tree:

```
uis/<ui>/
  packages.list          packages to install (Debian names)
  files/                 tree overlaid onto the rootfs
    etc/...              copied to /            (owned by root)
    home/...             merged into /home/$USERNAME (owned by the user)
```

`scripts/build-rootfs.sh` stages both into the chroot and `scripts/rootfs-setup.sh`
installs them, rewriting `/home/brunux` to the configured `USERNAME` inside the
text files. Select this UI with `UI=omarchy`; `uis/sway` remains the other one.

This desktop is a port of **[Omarchy v4](https://omarchy.org)** — Hyprland, the
Quickshell shell, the `omarchy-*` tooling and themes — onto this repository's
Debian `forky`/`arm64` base, reusing the kernel, device tree and boot chain
already proven for the Google Hayato.

- **Upstream:** [`omacom/omarchy`](https://github.com/omacom/omarchy), branch
  **`quattro`**, pinned at **`da2db1a2fe022089165b9edca3326886ad78dae2`**
  (2026-10-18), `version` = `4.0.0.alpha`.
- **Plan:** [`docs/plan-omarchy.md`](../../docs/plan-omarchy.md) — decisions,
  phases and the full Arch→Debian mapping rationale.

## Scope

**The desktop only.** There is no Arch base: no `pacman`/`yay`, no ISO builder,
no Omarchy installer/provisioner, no btrfs/snapper/LUKS, and none of the
`omarchy-pkgs` applications (`omawrite`, `omacalc`, `omasnap`, `owe`, …) in this
milestone. The distribution stays Debian; the port contributes the desktop
session, its shell, its configuration, its themes and the tooling the desktop
itself uses.

## Layout

| Path | What it is |
|---|---|
| `packages.list` | The Debian package set (this file's companion), the Arch→Debian map in table form below. |
| `PROVENANCE.md` | The pinned upstream commit and exactly what was vendored or excluded. |
| `patches/` | `*.patch` + `series` against the pinned tree; empty until the port needs one. |
| `files/usr/share/omarchy/` | The curated vendored Omarchy tree (`config/`, `shell/`, `applications/`, `etc/`, `bin/`, `default/`, `themes/`, `version`) at upstream's runtime path. |
| `files/usr/share/wayland-sessions/omarchy.desktop` | The session entry tuigreet lists; runs UWSM → Hyprland. |
| `files/usr/local/bin/omarchy-session` | The command greetd runs by default: `uwsm start … hyprland.desktop`, as one argv. |
| `files/etc/systemd/user/wayland-wm@hyprland.desktop.service.d/10-keep-shell.conf` | Upstream's `KillMode=mixed` drop-in for the UWSM unit. |
| `files/etc/profile.d/omarchy.sh` | Puts `$OMARCHY_PATH` in login shells. |
| `setup.sh` | Runs in the chroot: symlinks the shipped `omarchy-*` commands into `/usr/bin`, seeds `~/.config` from the vendored tree, installs the UWSM env file and applies the Tokyo Night theme. **Landed (F3).** |

The vendored tree is what upstream expects at `$OMARCHY_PATH`, so the shell and the
`omarchy-*` commands find their files unchanged. The port's job is the *edges*: the
package list (done), the Debian-ised system/user overlay (pending), the session entry
(greetd, pending) and the device quirks (pending).

## What changed from Arch/Omarchy

### Session and login

Omarchy starts Hyprland through **UWSM** behind **SDDM**. This port keeps the
Debian side's **`greetd` + `tuigreet`** and starts the same **Hyprland via
UWSM** session from there (see `docs/plan-omarchy.md` §2). SDDM is not installed
and upstream's `etc/sddm.conf.d/*` and `install/login/sddm.sh` do not ship.

The chain on this port is: `greetd` → `tuigreet` (which lists
`/usr/share/wayland-sessions/omarchy.desktop`) → `/usr/local/bin/omarchy-session`
→ `uwsm start -g -1 -e -D Hyprland hyprland.desktop` → Hyprland.
`scripts/rootfs-setup.sh` became UI-aware and writes `--cmd omarchy-session` into
`/etc/greetd/config.toml` (`--cmd sway` for the sway UI), enabling `greetd` for
either UI.

Upstream ships the `omarchy-*` commands as `/usr/bin/omarchy-*` (they come in the
Arch `omarchy` package). Here `setup.sh` symlinks them from the vendored
`$OMARCHY_PATH/bin` into `/usr/bin`, and `/etc/profile.d/omarchy.sh` plus
`/usr/share/uwsm/env.d/10-omarchy` set `$OMARCHY_PATH`. The Arch-only command
families (package management and updates, the installer/provisioner,
AUR/web-app/gaming/AI installers, and helpers that call `pacman`/`yay`) are left
off PATH rather than shipped broken: that list is the `EXCLUDE` block in
`setup.sh`, and every other command is linked.

### Device quirks and system files

The Hayato quirks the sway UI already solved are reused here:

- `files/etc/keyd/default.conf` — the Chromebook top-row remap (VTs, F1–F10); the
  Lock key stays unmapped so systemd-logind still suspends on `KEY_SLEEP`.
- `files/etc/udev/rules.d/90-brightnessctl.rules` — gives the `video` group write
  access to the backlight, which `omarchy-brightness-display` calls.
- `files/etc/udev/rules.d/60-omarchy-io-scheduler.rules`,
  `files/etc/modprobe.d/omarchy-usb-autosuspend.conf`,
  `files/etc/NetworkManager/conf.d/omarchy-wifi-powersave.conf` — upstream's.

Omarchy defines its own XKB option (`shift_caps_compose`) in `/etc/xkb`, which
libxkbcommon reads before the system rules, so `files/etc/xkb/rules/evdev` and
`files/etc/xkb/symbols/omarchy` are installed, alongside upstream's logind,
systemd (`nofile`, shutdown), `systemd-oomd`, `sysctl.d`, `plocate` and `zswap`
drop-ins. The ALSA UCM boot sequence comes from `alsa-utils` (base) and
`alsa-ucm-conf` (this list), as in the sway UI. Upstream's Arch-only system files
— mkinitcpio, limine, plymouth, sddm, docker, mise, the `sudoers.d/omarchy-*`
grants and its `nsswitch.conf` — are deliberately not installed.

### Hyprland configuration is Lua

Since 0.55 Hyprland reads `hyprland.lua` and `hypr/autostart.lua`,
`bindings.lua`, `input.lua`, `looknfeel.lua`, `monitors.lua` rather than the
legacy `hyprland.conf`. Debian `forky` ships **hyprland 0.56.2**, so the pinned
tree's Lua configuration is used as-is; v3-era `hyprland.conf` material is not
ported.

### The shell (Quickshell)

The shell's QML imports (`Quickshell*`, `qs.Commons`, `qs.Ui`, `QtQuick*`,
`QtQml.Models`) all resolve against Debian's `quickshell` 0.3.2 plus the
`qml6-module-qtquick*` set in `packages.list`; the `Quickshell.Services.*`
modules ship inside the binary and use PipeWire, UPower, polkit, Pam, BlueZ and
NetworkManager, all installed. One import, `Owe.LockFeed`, comes from the
`omarchy-pkgs` app `owe` and is deferred with the rest of that set (F8) — the lock
screen may fall back until then. This is the F4 gate: it is a **shape check on
the imports**, not a boot.

### Package-manager and installer tooling

Everything tied to Arch's package flow is dropped: `pacman`, `yay`,
`pacman-contrib`, `expac`, `kernel-modules-hook`, the mkinitcpio/limine/plymouth
integration, and the `omarchy-pkg-*` / `omarchy-update-*` commands that call
them. Debian's `apt`, this repository's `install-kernel.sh` + `install-kpart.sh`
flow and `initramfs-tools` replace all of it. The `omarchy-*` subset that ships
is the part the desktop itself reaches (theme switching, OSD, capture, audio,
network, power, session, lock); the rest is omitted rather than stubbed, and
`docs/plan-omarchy.md` F2 pins that boundary down.

### Package substitutions

| Omarchy (Arch) | Debian (forky) | Why |
|---|---|---|
| `hyprland-guiutils` | `hyprland-qtutils` | Debian's name for the Qt GUI helpers and share picker. |
| `quickshell` | `quickshell` | Present (0.3.2); the pinned shell must run against it, or be trimmed (F4). |
| `hyprland-preview-share-picker` | `hyprland-qtutils` | Ships inside the Qt utils package. |
| `python-gobject` | `python3-gi` | Debian's name. |
| `nss-mdns` | `libnss-mdns` | Debian's name. |
| `tldr` | `tealdeer` | Debian's tldr client. |
| `evince` | `papers` | GNOME's successor viewer; `evince` is a transitional name in forky. |
| `gpu-screen-recorder` | `wf-recorder` | Not packaged; `omarchy-capture-screenrecording` falls back to wf-recorder. |
| `satty` | `swappy` | Not packaged; screenshot annotation uses swappy. |
| `ghostty` | `foot` / `alacritty` / `kitty` | Not packaged; the port ships the three terminals Debian has. |
| `docker-buildx` / `docker-compose-v2` | `docker-buildx` / `docker-compose` | Debian's names (only if `docker.io` is wanted). |
| `zram-generator` | `systemd-zram-generator` | The aarch64 addition; `zram-tools` is already in `packages/base.list`. |
| `qemu-user-static-binfmt` | `qemu-user-binfmt` | Debian's name. |
| `woff2-font-awesome` | `fonts-font-awesome` | Debian's name. |
| `ttf-jetbrains-mono-nerd-basic` | `fonts-jetbrains-mono` + `fonts-nerd-symbols` + `fonts-font-awesome` + `fonts-materialdesignicons-legacy` | Debian ships no Nerd-patched Mono face, and `fonts-nerd-symbols` is DFSG-stripped of Font Awesome and Material Design Icons — the same fix `uis/sway` uses. |
| `ttf-ia-writer` | (dropped) | Not packaged; only vendor it if a theme link needs it. |
| `libvips`, `mariadb-libs`, `postgresql-libs` | `libvips42t64`, `libmariadb3`, `libpq5` | Debian's names. |

### Not needed on Debian

`walker`, `mako` and the v3 `swayosd` stack were Omarchy v3's launcher,
notification daemon and OSD. Omarchy v4's **Quickshell shell** owns the bar,
launcher, notifications and OSD, so none of them are installed. `swayosd` is in
Debian if a fallback OSD is ever wanted.

### Dropped for this milestone

Arch/mise-only tooling (`mise-bin`, `ttfx`, `superwhisper-bin`), proprietary or
unpackaged apps (`localsend`, `moonlight-qt`, `obsidian`, `pinta`,
`wl-screenrec`), `dotnet-runtime`, and the entire `omarchy-pkgs` application set.
They are recorded in `docs/plan-omarchy.md` §4 and deferred to F8 rather than
silently forgotten.

## Installed package list

`packages.list` holds 128 Debian names, every one verified to resolve on
`forky`/`arm64`. It does not repeat what `packages/base.list` already provides
(`network-manager`, `wireless-regdb`, `alsa-utils`, `rtkit`, `dosfstools`,
`bash-completion`, `less`). The compositor, Quickshell and its Qt6/QML runtime,
the portals, the terminals, the fonts and the capture/audio/power CLI the shell
and key bindings invoke are all listed there with a comment saying why.

## Status

| Phase (`docs/plan-omarchy.md`) | State |
|---|---|
| F0 recon + pin | done — this README and the plan record it |
| F1 `packages.list` | done — 128 names, all resolved on forky/arm64 |
| F2a vendor the pinned tree (`files/usr/share/omarchy/`, curated) | done — 24 MB, see `PROVENANCE.md` |
| F2b seed `~/.config` + link the `bin/` subset (`setup.sh`) | done — Arch-only commands excluded, rest linked |
| F3 session/login (Hyprland via UWSM under greetd) | wired — greetd is UI-aware, the session entry and env are shipped; **not yet booted** |
| F4 Quickshell shell | imports resolve against Debian's quickshell 0.3.2 (shape check, not a boot); `Owe.LockFeed` deferred |
| F5 Hayato quirks (keyd, ALSA UCM, backlight, RAM) | device files + the system subset installed; **not yet booted** |
| F6 build + off-device validation | done — `UI=omarchy` rootfs builds; see the validation notes below |
| F7 hardware validation + release | card written 2026-10-18 from this build; **awaiting boot + report** |

Nothing here claims to boot yet: the wiring is in place and a card carries it,
but the port is only real once that card boots it (F7).

> The card was written from a rootfs stamped `commit 5fd928c` whose setup hook was
> then re-run by hand with the later EXCLUDE fix, so its `omarchy-*` links are the
> corrected set while its `/etc/debian-aarch64-revision` still says `5fd928c`, and
> its `/usr/local/sbin/check-system.sh` is the older sway-oriented report. A clean
> rebuild + re-write makes the stamp match the content.

### What was actually validated off-device

`UI=omarchy scripts/build-rootfs.sh` builds the rootfs and its hook runs:

- the vendored tree lands at `/usr/share/omarchy`, the packages resolve, and
  `~/.config` is seeded (`hypr/`, `omarchy/`, `uwsm/`, `environment.d/`, …);
- the hook links **309** commands into `/usr/bin` and leaves the 223 Arch-only
  ones out (verified inside the built rootfs: `omarchy-theme-set` and
  `omarchy-launch-shell` are present, `omarchy-pkg-add` and
  `omarchy-update-available` are absent);
- `omarchy-theme-set "Tokyo Night"` runs headless and leaves
  `~/.local/state/omarchy/current/theme.name` = `tokyo-night`;
- `Hyprland --verify-config` loads `~/.config/hypr/hyprland.lua` and the Omarchy
  defaults through Lua with no config errors (the one line it reported,
  `hl.env(... HYPRLAND_INSTANCE_SIGNATURE)`, only fires when the compositor runs
  outside its own session; the later segfault is the chroot having no `/dev`/DRM).

The compositor, the shell and the hardware paths cannot be judged in a chroot;
that is F7.
