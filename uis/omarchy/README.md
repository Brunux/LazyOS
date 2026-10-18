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
| `files/etc/...` | System overlay (ported): units, sysctl, udev, xkb, portals, the `greetd` session entry. **Not landed yet.** |
| `files/home/...` | User overlay (ported): `~/.config`, `~/.local`, the home seed. **Not landed yet.** |

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

### Hyprland configuration is Lua

Since 0.55 Hyprland reads `hyprland.lua` and `hypr/autostart.lua`,
`bindings.lua`, `input.lua`, `looknfeel.lua`, `monitors.lua` rather than the
legacy `hyprland.conf`. Debian `forky` ships **hyprland 0.56.2**, so the pinned
tree's Lua configuration is used as-is; v3-era `hyprland.conf` material is not
ported.

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
| F2b port the system/user overlay and the `bin/` subset | not started |
| F3 session/login (Hyprland via UWSM under greetd) | not started |
| F4 Quickshell shell | not started |
| F5 Hayato quirks (keyd, ALSA UCM, backlight, RAM) | not started |
| F6 build + off-device validation | not started |
| F7 hardware validation + release | not started |

Nothing here claims to boot yet: the vendored tree is upstream's, and the port is only
real once a card boots it (F7).
