# The Omarchy gap (October 2026)

What the Omarchy v4 desktop has that this port does not, what is covered by something else, and
what is deliberately absent. It exists because "are we missing anything?" is asked often enough to
deserve a checked answer instead of a feeling.

## How this was computed

* **Packages**: the Omarchy-v4 Debian port (Deboma, `uis/omarchy/packages.list`) is the app/tool
  half of upstream v4 mapped onto Debian names. Diffed against the four lists this port installs
  (`packages/base.list`, `packages/tools.list`, `uis/sway/packages.list`,
  `devices/google-asurada/packages.list`): **86 names in Omarchy are not in this tree**. Most (45)
  belong to the Qt6/Quickshell/Hyprland shell and its look-and-feel helpers - the desktop itself is
  the difference (swayfx + waybar + rofi + mako + foot), not a missing package - and three of those
  names (`hyprpicker`, `yaru-theme-icon`, `libqalculate23`) are standalone tools that are
  reclassified in the tables below.
* **Features**: the inventory in [`plan-rofi.md`](plan-rofi.md) and the manual's own "not built yet"
  list (`uis/sway/files/usr/share/doc/lazyos/03-shortcuts.md`).
* **Section 5 is not Omarchy parity**: it is what this port's own plans (`plan-rofi.md`'s batches B
  and C, its tablet section, `plan-agents.md`) already specify and has not been built. It is here so
  the two questions - "what does Omarchy have that we do not" and "what have we planned and not
  done" - do not require two documents.
* Sizes are `Installed-Size` from forky, uncompressed.

## 1. Missing, and worth adding

Grouped by what the machine cannot do today. Sizes are per package, for a 4 GB device where the
whole image is ~4 GiB.

| Theme | Package | Size | What it buys |
|---|---|---|---|
| Media | `foliate` | 12.4 MB | An EPUB reader that is not a browser. `papers` is now the document viewer and it reads **no EPUB at all** (`org.gnome.Papers.desktop`'s `MimeType=` has no `application/epub+zip`), so an EPUB still opens in Firefox. |
| Fonts | ~~`fonts-noto-core`~~ | — | **done**: 42.5 MB, in `uis/sway/packages.list`. Arabic, Hebrew, Devanagari, Thai and the rest of the non-CJK Noto set now have a face (the image used to ship DejaVu + JetBrains + the icon fonts, i.e. Latin/Cyrillic/Greek only). |
| Fonts | `fonts-noto-cjk` | 91 MB | Chinese/Japanese/Korean. Optional and still out: it is the single biggest item here, documented as an `apt install` in `04-software.md`. |
| Storage | `gnome-disk-utility` | 7.2 MB | Formatting/partitioning with a GUI. Today it is `parted`/`cgpt` in a terminal. |
| Secrets | `gnome-keyring` + `libsecret-1-0` | 4.6 MB + 414 kB | Somewhere for applications to store a password/token (Git, Nextcloud, Firefox Sync). |
| Portals | `xdg-desktop-portal-gtk` | 776 kB | The backend for GTK file choosers and the screenshot dialog. With only `xdg-desktop-portal-wlr`, an application that asks the portal for a file has no implementation. The two interfaces it would also answer are handled without it: **Settings** by `gtk-4.0/` (libadwaita's colour scheme) and **Inhibit** by `config.d/32-idle-inhibit.conf` (the compositor holds the inhibitor for Firefox and papers), so the remaining reason to install it is a file chooser. |
| Network | `nm-connection-editor` | 5.5 MB | VPN and 802.1x editing. `bar-net` lists and connects Wi-Fi; it cannot author a VPN profile. |

**The file manager is `pcmanfm` + `gvfs`, not `nautilus`** (measured on forky: `pcmanfm 1.4.0-1+b1`
against `nautilus 51.0.1-3`). The four names Omarchy installs (`nautilus`, `python3-nautilus`,
`gnome-sushi`, `gvfs-backends`) are 4.6 MB of binaries and **504 packages** of closure: GTK4 +
libadwaita, `libicu78` (38 MB), GStreamer, `gexiv2` and `samba-libs` (31 MB, a hard dependency of
`gvfs-backends`), plus a **resident indexer** - the binary queries
`org.freedesktop.Tracker3.Miner.Files`, so one opened window leaves `localsearch` crawling `$HOME`
on the eMMC - and glycin's sandboxed per-thumbnail decoders, which `plan-rofi.md` already declined.
`pcmanfm` is 229 packages, is GTK3 (the image already has it for waybar), has no indexer
(`search:///` is a plain walk, provided by `libfm-modules`), no glycin and no ICU. It ships in
`uis/sway/packages.list` with `gvfs` (431 kB: the trash and the local volume monitor) and without
`gvfs-backends`, which is where `samba-libs` comes from. It is the *windowed* half of the file
story, not a replacement for the menu: `Alt+Search` and `Alt+Ctrl+F` still open `rofi-files` (one
process, no window), while `Alt+Ctrl+Shift+F` and the root menu's **Files (window)** row open
pcmanfm.
Three limits stay documented at the setting: its thumbnails come from libfm's own gdk-pixbuf
loader, so they are **images only** (the freedesktop `.thumbnailer` files are invisible to it); its
Devices pane is whatever `udisks2` reports through gvfs' volume monitor (`udisks2` is in
`uis/sway/packages.list`; `udiskie`, the resident tray daemon, is not), with `autorun` off so a
card never runs its own contents; and `pcmanfm --desktop` is X11-only, i.e. nothing on sway.

Adopted since this list was written: `swappy` and `wf-recorder` (the capture menu, `rofi-capture`),
`cliphist` (clipboard history, `rofi-clip`: the one resident process, ~2 MB),
`fonts-noto-color-emoji` (the emoji picker's glyphs, `rofi-emoji`), `pcmanfm` + `gvfs` (the
windowed file manager: `Alt+Ctrl+Shift+F`, the launcher's **Files (window)**; the decision is section
1), and the five names the file story was waiting for: **`mpv` + `mpv-mpris`** (a video plays in a
real player, and `playerctl`, `rofi-media` and the bar see it because `lazyos-view` passes
`--script=/etc/mpv/scripts/mpris.so` - `--no-config` empties the directory the plugin installs
into), **`imv`** (the picture viewer, with `chafa` kept as the fallback for a GPU without an EGL
context), **`papers`** (PDF, DjVu and comics in a reader instead of a browser tab), **`udisks2`**
(mounting a stick from Places and from pcmanfm's Devices pane; **`udiskie` stays out on purpose** -
it is a resident tray daemon and this bar has no tray), and **`neovim` + `vim`** (so `EDITOR` and
`VISUAL` name `nvim` instead of leaving `eff` on nano, and `vim` brings `/usr/bin/vi` with it).
The colour picker needed no package at all: `slurp` + `grim` + `ffmpeg` already read a pixel - and
neither did `ffmpegthumbnailer`: `lazyos-thumbnail` makes a video's thumbnail with the `ffmpeg` the
image already has.

## 2. Covered by something else

| Omarchy | Here |
|---|---|
| `chromium` (280 MB) | `firefox-esr` (in `uis/sway/packages.list`), the image's one browser: documents, pictures and video have viewers of their own now (`papers`, `imv`, `mpv`) |
| `alacritty`, `kitty` | `foot` - half the memory of an alacritty window on a 4 GB machine |
| `yaru-theme-icon` | the **LazyOS** icon theme, rendered by `scripts/make-icons.sh` from the Font Awesome codepoints used in the menus |
| `libxkbcommon-tools` (`xkbcli list`) | `keyd` for the keymap and `swaymsg -t get_inputs` + `jq` for the layout indicator |
| `libqalculate23` | `rofi-calc` (an `awk` expression, no library) |
| `hyprsunset` | `wlsunset` + the night-light row in `rofi-quick` |
| `pamixer` | `pactl` from `pulseaudio-utils` |
| `xdg-utils`, `xdg-terminal-exec` | the shipped `/usr/local/bin/xdg-open` shim (what foot's ctrl+click calls) and rofi's `terminal: foot` |
| `xdg-desktop-portal-hyprland` | `xdg-desktop-portal-wlr` |
| `desktop-file-utils`, `gtk-update-icon-cache` | build-time only: nothing at runtime needs `update-desktop-database`, and GTK scans a theme directory when its cache is absent. A future `make-icons.sh` could build the cache. |
| `fish`, `zsh` (upstream) | bash with the `90-cli-tools.sh` environment and the aliases in `~/.bashrc` |

## 3. Deliberately not here

| Package | Why not |
|---|---|
| `gawk` | Omarchy's theme renderer needs awk's `RT`; this port's theme is a static file, and mawk is faster. |
| `lua5.4`, `tree-sitter-cli` | Editor and keybindings-view tooling for Omarchy's own components; `rofi-keys` parses sway's config with `awk` instead. |
| `plocate` | `locate` **plus** `plocate-updatedb.timer` - a systemd timer that indexes the disk. Not on a card that is rewritten on every install. |
| `nautilus` (+ `gnome-sushi`, `gvfs-backends`) | GTK4 + libadwaita + 38 MB of ICU + GStreamer + 31 MB of `samba-libs`, and a `localsearch` indexer, for a file manager `pcmanfm` does on the GTK3 the image already has (section 1). |
| `libnotify-bin` | `notify-send` is already pulled in by `grimshot`/mako's client tools. |
| `power-profiles-daemon` | Omarchy ships it, but `plan-rofi.md` declines it on purpose (a daemon) and the kernel is pinned to `schedutil` (`optimization.md` item 2). |
| `fwupd`, KDE Connect / GSConnect | `plan-rofi.md` excludes them: this device's firmware is ChromeOS's and is never flashed, and both are resident daemons. |
| Qt6, `qml6-module-*`, `quickshell`, `uwsm`, `hyprland*`, `gir1.2-*`, `liblayershellqt*`, `lxqt-qtplugin`, `qt6ct`, `nwg-look` | The Quickshell shell and its Qt runtime. This port's shell is waybar + rofi + mako, and its theme is static, so there is nothing to configure with a Qt/GTK appearance tool. |
| `xdg-desktop-portal-hyprland` | Hyprland-specific portal; `-wlr` is the wlroots one. |

## 4. Features, not packages

The menus are one-shot `rofi` scripts (`uis/sway/files/home/.config/bin/rofi-*`). The ones Omarchy
has that are still missing:

| Omarchy feature | Today | What it would take |
|---|---|---|
| **Emoji picker** (`Super+Ctrl+E`) | **done**: `rofi-emoji` (Alt+Ctrl+E) over `~/.config/emojis.tsv` (generated by `scripts/make-emojis.sh`), `wl-copy` + `wtype -M shift -k Insert`, `fonts-noto-color-emoji` draws it | — |
| **Clipboard history** (`Super+Ctrl+V`) | **done**: `rofi-clip` (Alt+Ctrl+V) over `cliphist` + `wl-paste --watch`, Enter copies back, Alt+1 deletes, the newest image is saved to `~/Pictures/clipboard`. The only resident process (~2 MB) | — |
| **Capture menu** (`OCR`, `QR`, `record`) | **done**: `rofi-capture` (Alt+Ctrl+C) does screenshot, annotate, OCR (`tesseract`), QR (`zbarimg`) and start/stop recording (`wf-recorder`); `rofi-qr` draws a Wi-Fi QR | — |
| **Wi-Fi QR** | **done**: `rofi-qr` (root menu, and the last row of the Wi-Fi panel) | — |
| **Package search/install** | **done**: `rofi-pkg` searches with `apt-cache`, marks what is installed, installs with `apt-get -y` behind the busy card, and its last row runs `apt-get update` | — |
| **DNS switcher** | **done**: `rofi-dns` sets `ipv4.dns`/`ipv4.ignore-auto-dns` on the connection that is up and brings it back, with common resolvers, typed servers or DHCP | — |
| **Reminders** | **done**: `rofi-remind` starts transient `systemd-run --user` timers that fire `notify-send` once, listed back from systemd and cancelled by choosing one | — |
| **Theme switcher, appearance, animations, monitor layout** | fixed solarized theme | A real project: the palette lives in waybar CSS, foot.ini, rofi's rasi, mako, sway `client.*` and swaybg. Not a menu. |
| **OSD (volume/brightness overlay)** | **done**: `~/.config/bin/osd` (Alt+volume/brightness) draws a mako card with the `value` hint as a progress bar and the matching Font Awesome icon; no process, no package. `swayosd` **is** packaged in forky (`0.3.0-4`, 7.7 MB), and the reason it is not used is the daemon: a resident GTK4 server for the same picture. | — |

Not portable from Omarchy at all: its **Install/Remove trees** (pacman/AUR/mise for Arch packages -
on Debian they are `apt` one-liners with different names), its **Quickshell plugins/dock/agents
panel**, and the **fingerprint/FIDO2/NVIDIA/hybrid-GPU** items, since this Chromebook has none of
that hardware.

## 5. What the port's own plans call for (not Omarchy parity)

`plan-rofi.md` (batches B/C and the tablet section) and `plan-agents.md` specify work that is not a
package Omarchy installs, so it did not appear in the diff above. It is still missing, and it is
listed here so the two lists do not have to be read together to know what is left.

| Feature | Package | Size | Note |
|---|---|---|---|
| Tablet mode: on-screen keyboard | `wvkbd` (`wvkbd-mobintl`) | 826 kB | `bindswitch tablet:on` starts it, disables the touchpad and starts a 1 Hz accelerometer poller; `tablet:off` reverses it. `libwlroots` already exposes the virtual keyboard. |
| Tablet mode: rotation service | `iio-sensor-proxy` | 236 kB | Optional: the poller can read `iio:device1/2` sysfs directly instead, with no package and no daemon. |
| Manual rotation | `wlr-randr` | 81 kB | The "Rotate screen" row in `rofi-quick`; today it only works through `swaymsg output … transform`. |
| Snippets | — | — | `~/.config/snippets.tsv` → rofi → `wtype`/`wl-copy`; `wtype` is installed. |
| Password store | `pass` (or `rbw`) | 129 kB (15.3 MB) | `passmenu`-style, one row per entry; `gnupg` is installed. The choice between the two is still open in the plan. |
| Unit / currency conversion | `units` (or a python table) | 917 kB | A script mode beside `rofi-calc`. |
| Translate | — (`curl` + `jq`) | — | **done**: `Alt+Ctrl+T` translates the mouse selection (or the clipboard) with Google's JSON endpoint and MyMemory as the fallback - `translate-shell` is contrib and its engine answers this machine with a captcha page. Result to the clipboard and a card; long text to a read window. |
| Disk usage | `ncdu` | 156 kB | `ncdu` in foot for `/`, `/home` and the card (the Free space menu walks `du`). |
| Mirror ranking | `netselect-apt` | 35 kB | A system task, last in the plan. |
| Weather | — (`curl` installed) | — | `wttr.in` in foot. |
| World clock | — | — | `TZ=… date` list. |
| About / system | — | — | **done**: `rofi-about` (the read-only list plus `fastfetch` in foot). |
| Agents usage (local logs) | — | — | `plan-agents.md` P3: a `python3` one-liner over the CLIs' own session files, on demand. P4 (provider APIs) is deliberately not planned. |
| Share (clipboard/file/folder) | — | — | `localsend` is not packaged in forky; a decision of its own. |

Not in forky at all, and therefore not a package to add: `rofi-emoji`, `rofi-calc`, `rofi-file-browser`
(all three are script modes here), `satty` (substituted by `swappy`) and `localsend`.

## 6. What this port has and Omarchy does not

Worth saying, because the gap list is one-directional by nature: `sway-hibernate` gated on a real
disk swap and `resume=`; `dictate` (whisper.cpp, a pinned model, `wtype`); the agents menu with
per-account `CLAUDE_CONFIG_DIR`/`CODEX_HOME` homes; `rofi-files` as a script instead of a file
manager; and a session whose entire resident set is the compositor, waybar, mako, pipewire and the
bar scripts.

## 7. Omarchy 4.5 (release candidate, 20 October 2026)

4.5 is the first feature release since Quattro (v4), the release sections 1-4 were diffed
against, and it is a **release candidate**: the notes say to expect rough edges. It is read here
for the same reason the v4 list was - "is this something we should have?" - not to follow a
moving target. Sources: the `v4.5.0-rc1` release notes and the manual pages they link, read on
26 October 2026. One detail matters for the comparison itself: **4.5 ships an aarch64 ISO**, so
this is the first release where the two desktops are on the same architecture rather than
different ones.

### 7.1 Already at parity (the headline is something this port does)

| 4.5 headline | Here |
|---|---|
| Notifications stacked by sender | mako's `group-by=app-name` in `~/.config/mako/config`; the card's header is the application name. |
| Notification history and Do Not Disturb | `rofi-notify` (Alt+Ctrl+N) lists mako's `max-history=20`; DND is a row in `rofi-quick` and a `makoctl mode`. |
| Apps opaque, terminals transparent | the carriers are already split that way: foot 0.87, rofi 0.82, waybar 0.71, mako 0.88, and the GTK3/GTK4 fills are opaque base03. |
| Scratchpad console on `Super+Grave` | `Alt+grave` (`Alt+minus` as the alias). |
| `Super+Shift+S` for a screenshot | `Alt+Shift+S` (area), `Alt+Shift+F` (full), both through `screenshot` + `grimshot -n`. |
| A second close chord | `Alt+Q` is the only close, and it is the documented one: foot's `Ctrl+Shift+W` never reached the held About windows on this keyboard. |
| Agent account selection applied to subprocesses | that is the design here: `CLAUDE_CONFIG_DIR` and `CODEX_HOME` are set per account (`rofi-agents`), so every process an agent starts inherits the choice. |
| No-animations mode, on by default in VMs | `LAZYOS_REDUCED_MOTION=1` skips the framework's one 300 ms sweep (`app.rs`); there is no toggle in the session yet (7.2). |

### 7.2 Worth taking (and which spec would carry it)

Each row names the smallest thing that makes it real, because "add an Appearance menu" is a
project and "one more row in the network panel" is a commit.

| 4.5 feature | Today | What it takes |
|---|---|---|
| **Captive portal status and a sign-in action** | a hotel or airport network shows "connected, no internet" and nothing offers the page | `nmcli networking connectivity check` answers `full`/`limited`/`portal`/`none` (verified here: `full`), and `nmcli -g GENERAL.STATE` names the connection. One row in `bar-network`/`rofi-quick` that reads the state and opens `http://nmcheck.gnome.org/` in Firefox, where the portal's own redirect takes over. No package, no daemon. **The best-value item in this list**, and the only one that fixes a situation where the machine is otherwise unusable. |
| **Actions, codes and links on the card** | mako 1.11 is installed and **supports it** (`actions=1`, `on-button-left`, `on-notify` in its own man page and binary); the config does not set it | `actions=1` in the mako template plus a `default` action on the call sites that have one ("open the folder", "show the log"). Belongs to spec 006, whose `Card` already carries an `action` field. |
| **Snooze** | dismissing a card loses it (it stays in `makoctl`'s history, not on screen) | no mako option; the shape already exists in `rofi-remind` - `systemd-run --user --collect` a one-shot that re-sends the card. A row on the card's menu, not a daemon. Spec 006. |
| **An Appearance menu** | theme (`Alt+Shift+T`), wallpaper (`Alt+Shift+W`), night light and the radio switches are four separate places; the root menu's *Settings* row says "Wi-Fi, Bluetooth, night light, tablet mode, rotation" | a group, not a daemon: theme, wallpaper, night light, reduced motion, bar position, and (once it exists) density. Spec 013 is where the theme engine's user-facing surface lands, so the grouping is its job. |
| **A short intro when the theme loads** | `theme next` rewrites eleven files and prints a line | the framework's animation budget already allows **one** sweep of at most 300 ms, skipped by `LAZYOS_REDUCED_MOTION=1`; the surface that draws it is the framework window, so this is spec 013/014 work, not a `swaybg` trick. |
| **Remembering the wallpaper per theme** | one wallpaper for the session; `theme next` keeps it | `wallpaper next` already chooses; the per-theme memory is one file under `~/.local/state/lazyos/theme/` keyed by theme name. Spec 013. |
| **Dimming installed software instead of hiding it** | `rofi-pkg` marks what is installed; `rofi-install` lists rows | one `ui_row` for the installed state with the dim colour, i.e. the shape `rofi-pkg` already uses. Spec 012 (the launcher/reference surfaces). |
| **Scrolling through workspaces from the bar** | already on: waybar 0.15's `sway/workspaces` scrolls unless `disable-scroll: true`, which this config does not set (verified in `waybar-sway-workspaces(5)`), so the pills cycle on a scroll today. | nothing; noted so it is not "added" twice. |
| **A clock with live seconds** | `sway/clock` with a one-minute interval | `interval: 1` costs a wakeup a second all day on a battery device: **opt-in**, and the honest default stays minutes. |
| **`Shift+Caps Lock` as the compose key** | neither Compose nor a dead-key layer is set; accents come from `wvkbd`'s `Cmp` key on the touch keyboard | xkb's `compose:caps` option is available in this tree's `evdev` rules (verified); it is one line in the keyd/xkb setup and it returns Caps Lock to normal. Worth doing for typing on a physical keyboard, and it has to be checked against keyd's own layer. |
| **Keeping an update safe from a logout mid-transaction** | `lazyos-install-run` and `sway-suspend` already use `systemd-inhibit`; `rofi-pkg` and the update row do not | the same `systemd-inhibit --what=shutdown:sleep` wrapper around the `apt-get` call, plus the busy card it already opens. |
| **BBR and the kyber I/O scheduler** | `optimization.md` tunes sysctls; the scheduler is the kernel's default | both are measurable here (`sysctl net.ipv4.tcp_congestion_control=bbr`, `echo kyber > /sys/block/mmcblk0/queue/scheduler`), and both belong in `optimization.md` with numbers **before** they become a drop-in. |
| **Hardware video decoding in mpv** | `lazyos-view` and `rofi-files` run `mpv --no-config` with **no `--hwdec`**, so playback is software everywhere today | the MT8192 has a stateless `mtk-vcodec` decoder; whether `--hwdec=v4l2request` opens a real node here has not been measured, and a codec that silently falls back is not a saving. One `mpv --hwdec=v4l2request --msg-level=vd=v` against a known file answers it, then `optimization.md`. A `fix:` commit only after the measurement. |

### 7.3 Deliberately not

| 4.5 feature | Why not |
|---|---|
| **Floating workspaces with a dock** | sway has no dock layer and no floating workspace: its layouts are stacking/tabbed/split, and the bar already carries the workspace pills with `workstyle`'s icons. A dock is a window manager feature, not a bar module. |
| **Workspaces remembered per monitor** | sway assigns workspaces to outputs by rule (`workspace N output DP-1`), and the display menu writes positions rather than a workspace map; the normal case here is one internal panel. Revisit if a docked desk becomes common. |
| **Video wallpapers** | a resident video decoder for as long as the desktop is up, on a 4 GB battery device whose idle screen already costs a measured 12 % of a core at 30 fps. `mpvpaper` is not in forky either. `swaybg` draws one image and stops. |
| **50 languages, switched live** | the image is built with one locale and one keymap; live layout switching is a separate decision because keyd owns the keyboard and remaps the top row. The fonts for the scripts are already there (`fonts-noto-core`), which is the part that matters for reading, not for the UI language. |
| **The Quickshell plugin surface, dock app grid, tray drawer** | that is the Qt6/Quickshell shell, which section 3 declines wholesale; this session's bar is waybar's own modules. |
| **Agent usage meters and provider spend limits** | `plan-agents.md` P4 declines the provider APIs on purpose; a meter that changes shape every release is worse than none. The local, on-demand shape (P3) is still open. |
| **Microsoft web apps, Slack, T3 Code, Hype, Monologue, Signed, Disktree** | an application catalogue, not a desktop feature. Two of them have a cheaper answer here already (`papers` for documents; `ncdu` for disk usage), and the rest are one `apt` or one browser tab away. |
| **NVIDIA N1x, Snapdragon X, Apple Silicon** | not this hardware. |
| **Japanese/Chinese/Korean input methods from the installer** | `fcitx5` is packaged in forky (`5.1.23-1`, verified), but an IME is a resident daemon plus engines plus a per-language configuration, and this port's rule is that nothing stays resident unless only a watcher can answer the question. It is an `apt install` documented in `04-software.md`, not part of the image. |
