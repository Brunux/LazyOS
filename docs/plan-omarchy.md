# Plan: the Omarchy desktop on Debian (branch `omarchy`)

> Goal: bring the **Omarchy v4 desktop** — Hyprland, the Quickshell shell, the `omarchy-*`
> tooling, its themes and its configuration — to the Debian arm64 system this repository already
> builds for the **Google Hayato** (MediaTek MT8192, `google-asurada`).
>
> Scope: **the desktop only**. No Arch base: no `pacman`/`yay`, no ISO builder, no Omarchy
> installer, no btrfs/snapper/LUKS provisioning, and no `omarchy-pkgs` applications in this first
> milestone. The distribution stays Debian (`forky`, `arm64`), the boot chain, kernel and device
> support stay the ones already proven here.
>
> Foundation: `main` at `db676cf` (**v0.1.1**), the port that boots from the eMMC and the microSD
> and whose install path (`write-sd.sh`, `install-emmc.sh`, `install-kpart.sh`) is already verified
> on hardware.

---

## 1. What Omarchy v4 is

Pinned upstream: [`omacom/omarchy`](https://github.com/omacom/omarchy), branch **`quattro`**,
commit **`da2db1a2fe022089165b9edca3326886ad78dae2`** (2026-10-18), `version` =
**`4.0.0.alpha`**, MIT. (The project was previously hosted as `basecamp/omarchy`; `omacom/omarchy`
is the current home.)

The desktop is:

| Piece | What it is | Where it lives upstream |
|---|---|---|
| **Hyprland** | the Wayland compositor | package |
| **UWSM** | the session manager that starts Hyprland as a systemd user session | package |
| **Quickshell shell** | the desktop shell (bar, launcher, notifications, OSD, lock screen...) written in QML; replaces v3's waybar/mako/walker | `shell/shell.qml`, `shell/Commons`, `shell/Ui`, `shell/plugins`, `shell/services` (281 files) |
| **`omarchy-*` CLI** | 532 commands: theme switching, capture, audio, network, power, updates... | `bin/` |
| **Configuration** | Hyprland, autostart, terminals, btop, git, wireplumber, portals, fcitx5 | `config/` (82 files) |
| **Base filesystem** | systemd units, sysctl, udev, sudoers, xkb | `etc/` (96 files) |
| **Themes** | 30+ colour themes with wallpapers | `themes/` (340 files) |
| **Session defaults** | the seed tree overlaid into `$HOME` on first install | `default/` (585 files) |
| **Applications** | `.desktop` entries and icons | `applications/` (35 files) |
| **Installer** | per-platform package lists + provisioning scripts | `install/` (138 files) |
| **Login** | upstream runs **SDDM**; this port keeps **greetd + tuigreet** | `install/login/` |
| **Upstream apps** | `omawrite`, `omacalc`, `omasnap`, `owe`, `herdr`, ... | `omarchy-pkgs` (separate, out of scope here) |

Its package set is composed from `install/omarchy-base.packages` (162 packages), plus
`install/omarchy-aarch64.packages` (`zram-generator`, `rtkit`), plus `install/omarchy-other.packages`
(the ISO builder's list: firmware, GPU drivers, laptop-specific packages — most of it is already
handled by this port's kernel/device work or is x86/NVIDIA/Apple-specific and irrelevant here).

**Hyprland configuration is Lua.** Since 0.55 Hyprland reads `hyprland.lua` (loaded through
`hypr/autostart.lua`, `bindings.lua`, `input.lua`, `looknfeel.lua`, `monitors.lua`) instead of the
legacy `hyprland.conf`; the old syntax is deprecated. Debian forky ships **hyprland 0.56.2**, so the
Lua configuration is the one to use — do not port Omarchy v3's `hyprland.conf`.

## 2. Decisions taken for this branch

| Question | Decision |
|---|---|
| Branch | `omarchy`, off `main@db676cf` |
| Login / session | **keep `greetd` + `tuigreet`**; start **Hyprland through UWSM**. No SDDM port in this milestone. |
| Scope | **desktop shell only**: Hyprland + Quickshell + `config/`, `themes/`, `applications/`, `etc/` + the core `omarchy-*` CLI. Arch-only pieces are substituted or dropped; `omarchy-pkgs` apps are deferred. |
| Upstream pin | `quattro` @ `da2db1a2` (4.0.0.alpha) |
| Where it plugs in | the existing device-agnostic **`uis/<ui>/`** axis: add `uis/omarchy/`, selected with `UI=omarchy` (`uis/sway/` stays as the other choice) |
| Plan location | this file |

The UI axis already does what is needed: `scripts/lib.sh` resolves `UI`, and
`scripts/build-rootfs.sh` / `scripts/rootfs-setup.sh` stage `uis/<ui>/packages.list` and overlay
`uis/<ui>/files/` (`etc/` copied as root, `home/` merged into `/home/$USERNAME`). Nothing in the
kernel, boot chain, GPT layout or installer has to change.

## 3. Deliverable layout

```
uis/omarchy/
  packages.list          # Debian package names (the Arch->Debian map, documented inline)
  files/                 # overlay: etc/, home/ (Hyprland, Quickshell, omarchy CLI, themes)
  upstream/              # vendored omarchy checkout (pinned) + patches/ + PROVENANCE.md
  README.md              # what ships and every deliberate deviation from upstream
docs/plan-omarchy.md     # this plan
```

- `uis/omarchy/upstream/` will vendor only the trees this port actually uses (`config/`, `shell/`,
  `themes/`, `applications/`, `etc/`, the `bin/` subset, `version`) rather than the whole 1.2 GB
  checkout (which also carries `manual/`, `test/`, `migrations/`, screenshots...). A
  `PROVENANCE.md` records the pinned commit and what was trimmed, in the style of the existing
  `devices/google-asurada/PROVENANCE.md`.
- Anything patched is kept as a `*.patch` against the pinned tree with a `series` file, the same
  convention as `kernels/<flavor>/patches/`.
- `uis/omarchy/files/etc/` carries the system-level files (units, sysctl, udev, xkb, sudoers,
  portals, `greetd` session), `uis/omarchy/files/home/` the user-level ones (`.config/hypr`,
  `.config/omarchy`, the shell, `.local/bin` for the CLI).

## 4. Arch → Debian package mapping

Verified against this host's `forky`/`arm64` indexes. `uis/omarchy/packages.list` is the Debian
column; this table is the justification, to be kept in `uis/omarchy/README.md`.

### Available as-is (or with a rename)

| Omarchy (Arch) | Debian (forky) |
|---|---|
| hyprland | `hyprland` 0.56.2 |
| hyprland-guiutils | `hyprland-qtutils` 0.2.2 |
| quickshell | `quickshell` 0.3.2 (+ `qml6-module-*`, `libqt6qml6`, `libqt6quick6`, `libqt6multimedia6`, `libqt6svg6`, `qt6-svg-plugins`, `qt6-image-formats-plugins`) |
| uwsm | `uwsm` 0.27 |
| xdg-desktop-portal-hyprland | `xdg-desktop-portal-hyprland` 1.4.1 |
| xdg-desktop-portal-gtk | `xdg-desktop-portal-gtk` 1.15.3 |
| hyprpicker, hyprsunset | `hyprpicker` 0.4.7, `hyprsunset` 0.4.0 |
| hyprland-preview-share-picker | `hyprland-qtutils` (picker ships with the Qt utils) |
| chromium | `chromium` 154 |
| foot, kitty, alacritty | `foot` 1.28, `kitty` 0.48, `alacritty` 0.17 |
| nvim / omarchy-nvim | `neovim` 0.12 (+ vendored Omarchy nvim config) |
| starship, zoxide, eza, bat, fd, ripgrep, fzf, lazygit, btop, fastfetch, jq, less, tmux, gum, man-db, tree-sitter-cli, unzip, vim, whois, inxi, plocate, inotify-tools, socat | same names |
| networkmanager | `network-manager` |
| bluez, bluez-tools, bolt, avahi | `bluez`, `bluez-tools`, `bolt`, `avahi-daemon` |
| python-gobject | `python3-gi` |
| nss-mdns | `libnss-mdns` |
| noto-fonts, noto-fonts-cjk, noto-fonts-emoji | `fonts-noto-core`, `fonts-noto-cjk`, `fonts-noto-color-emoji` |
| ttf-jetbrains-mono-nerd-basic | `fonts-jetbrains-mono` + `fonts-nerd-symbols` (+ `fonts-font-awesome`, `fonts-materialdesignicons-legacy` for the glyphs `fonts-nerd-symbols` is DFSG-stripped of) |
| yaru-icon-theme | `yaru-theme-icon` |
| tldr | `tealdeer` |
| woff2-font-awesome | `fonts-font-awesome` |
| zbar | `zbar-tools` |
| tesseract, tesseract-data-eng | `tesseract-ocr`, `tesseract-ocr-eng` |
| gvfs-mtp/nfs/smb | `gvfs-backends` |
| gnome-disk-utility, nautilus, sushi | `gnome-disk-utility`, `nautilus`, `gnome-sushi` |
| evince | `papers` (GNOME's successor) |
| qt6-base/declarative/multimedia/svg/wayland/imageformats | `libqt6*` + `qml6-module-*` + `qt6-wayland` |
| python-poetry-core | `python3-poetry-core` |
| mariadb-libs, postgresql-libs, libvips, libyaml | `libmariadb3`, `libpq5`, `libvips42t64`, `libyaml-0-2` |
| cups, cups-filters, cups-pk-helper, system-config-printer | same names |
| ffmpeg, ffmpegthumbnailer, imagemagick, mpv, mpv-mpris, imv | same names |
| udiskie, wl-clipboard, wtype, qrencode, yt-dlp, dots... | same names |
| gpu-screen-recorder | `wf-recorder` (substitute) |
| qemu-user-static-binfmt | `qemu-user-binfmt` |
| base-devel, fakeroot | `build-essential`, `fakeroot` |
| zram-generator | `systemd-zram-generator` |

### Not in Debian — decide per item in Phase 1

| Omarchy (Arch) | Disposition |
|---|---|
| walker, mako, swayosd v3 stack | **not needed** — v4's shell (Quickshell) owns the launcher, notifications and OSD |
| satty | substitute `swappy` (screenshot annotation) |
| ghostty | substitute `foot`/`alacritty`/`kitty` |
| hyprland-guiutils (as such) | `hyprland-qtutils` provides the GUI utils |
| fcitx5-module-emoji | drop or substitute another fcitx5 emoji module |
| ttf-ia-writer | drop (vendor the font only if a theme link needs it) |
| docker-buildx / docker-compose-v2 | `docker-buildx` / `docker-compose` |
| dotnet-runtime | drop (not needed for the desktop) |
| mise-bin, ttfx, superwhisper-bin | drop — Arch/mise-tooling; Debian ships the tools directly where needed |
| localsend, moonray-qt, obsidian, pinta, gpu-screen-recorder, wl-screenrec | drop for the first milestone |
| pacman, yay, pacman-contrib, expac, kernel-modules-hook, initramfs/mkinitcpio hooks, limine, plymouth packages | drop — Debian's own package manager, `initramfs-tools` and this repo's kernel partition flow replace them |
| omacalc, omacut, omawrite, omasnap, owe, owe-lockfeed, herdr, hype, aether, gliff, monologue, signed, tobi-try, usage, cliamp, disktree-bin | **deferred** — `omarchy-pkgs` applications, out of scope for the first milestone (Phase 8) |

### Build from source (only if a core piece needs it)

Deferred to Phase 1/4 decisions; candidates: a newer **Quickshell** than 0.3.2 if the pinned shell
needs QML APIs it lacks, and the few Rust/Qt `omarchy-pkgs` apps that later become in-scope. The
default is to make the pinned shell run on the packaged Quickshell rather than to build one.

## 5. Phases

Each phase says what verifies it; nothing is "done" because it was written.

**F0 — Recon and pin (this branch's first commit).**
Freeze the upstream commit, inventory the trees against what the UI axis needs, and land this
plan. *Verify:* this document plus the `recon` output it cites; the pin is a commit hash, not
"latest".

**F1 — `uis/omarchy/packages.list`.**
Turn §4 into the list, one Debian name per line, with the deviations documented in
`uis/omarchy/README.md`. *Verify:* every name resolves on `forky/arm64`; `build-rootfs.sh` installs
the list without `apt` errors; nothing drops out of the base list.

**F2 — The overlay: `config/`, `themes/`, `applications/`, `etc/`.**
Vendor the pinned trees under `upstream/` and lay them into `files/`, rewriting Arch paths and
package calls. Port the `bin/` subset the desktop itself needs (theme switching, OSD, capture,
audio, network, power, session, lock) and stub or omit the rest. *Verify:* `sh -n`/`bash -n` on
every shipped script; no shipped script references `pacman`/`yay`/`omarchy-pkg-*` on a code path
the desktop reaches.

**F3 — Session and login.**
A Hyprland session through UWSM, selected by `greetd` + `tuigreet` (a `hyprland.desktop` wayland
session; `uwsm start`; the `greetd` session file). Portals and their `hyprland-portals.conf`.
Environment for **Panfrost** (`MOZ_ENABLE_WAYLAND`, Qt/Wayland, EGL/GLES renderer hints,
`WLR_*`/`AQ_*` as needed). *Verify:* `greetd` starts the session on a card; `hyprctl version`,
`hyprctl monitors`, `hyprctl clients` answer.

**F4 — The Quickshell shell.**
Ship the pinned `shell/` plus `config/omarchy/` hooks/extensions and the QML module set; wire
`omarchy-launch-shell`/`omarchy-shell`. Confirm the shell runs on **Quickshell 0.3.2** — if it
needs newer QML API, decide between packaging a newer Quickshell and trimming the shell. *Verify:*
the shell draws the bar on a real session; `qs -c <config>` reports no fatal QML errors; no
unhandled import.

**F5 — Hayato quirks.**
The device-specific half, mirroring what `uis/sway/files/` already solved: `keyd` exclusions for
`cros_ec_buttons`/jack and the `KEY_SLEEP` lock/suspend behaviour, the missing F-row and the
`Ctrl+Alt`/`Search` VT bindings, the ALSA UCM boot sequence, the backlight udev rule, 4 GB RAM
tuning (zram, animation/idle defaults), Wi-Fi regulatory DB. *Verify:* the same checks
`check-system.sh` already encodes for the sway UI, re-pointed at Omarchy.

**F6 — Build and validate off-device.**
`UI=omarchy scripts/build-rootfs.sh`, then a card via `write-sd.sh`. Headless checks before boot:
`sway -C` is meaningless here, so validate Hyprland's config with its own tooling
(`hyprctl`/`Hyprland` config errors), `Quickshell` offline where possible, and every config file
for the tools shipped. *Verify:* the rootfs contains the overlay, the package list is installed,
the staged revision is the branch's commit.

**F7 — Hardware and release.**
Boot the card, run `check-system.sh`, iterate on real findings only, and when the session is
stable cut a release the usual way (`make-release.sh`, notes in `docs/releases/`, `lib-dist.sh`
sanitising the artifact). *Verify:* a report in `reports/` from the booted card, and a release
whose notes do not claim more than that report shows.

**F8 — (deferred) the `omarchy-pkgs` applications.**
`omawrite`, `omacalc`, `omasnap`, `owe`, ... built for Debian, once the desktop shell itself is
proven. Not part of this milestone.

## 6. Risks

- **Hyprland + Quickshell on Panfrost / 4 GB RAM.** The headline unknown. Mali-G57 via Mesa
  (GLES3) should satisfy Hyprland's EGL/DRM needs, but performance and memory headroom on 4 GB with
  a Quickshell QML shell have to be measured, not assumed. Mitigations: keep animations/idle
  defaults modest, confirm the renderer early (F3), treat F4 as a gate.
- **Quickshell version drift.** Pinned shell against Debian's 0.3.2. Resolve by trimming the shell
  or packaging a newer Quickshell; decide in F4 with evidence.
- **Volume of Arch-only tooling.** Of 532 `omarchy-*` commands, the desktop needs a subset; the
  rest are Arch-installer/AUR/update paths. The port's job is a clean subset, not a rewrite — but
  the subset boundary must be explicit and tested (F2).
- **Two UIs to maintain.** `uis/sway` and `uis/omarchy` share the same device quirks. Keep the
  device-level files single-sourced where possible (lib/overlay) rather than copied twice.
- **Upstream is `4.0.0.alpha`.** Expect the pinned tree to move under us; the pin and the patch
  series are what make the port reproducible.

## 7. Open questions (to close in F0/F1)

- Which exact subset of `bin/` ships, and how the omitted commands fail (clean "not available" vs
  broken call).
- Whether `greetd` + `tuigreet` is enough to pick the Hyprland/UWSM session, or a small
  `greetd`/session tweak is needed.
- Whether `uis/omarchy` reuses `uis/sway`'s device files or gets its own copies.
- The first release number for a released Omarchy UI (likely `v0.2.0`, decided at F7).
