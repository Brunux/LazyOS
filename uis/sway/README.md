# uis/sway — Sway desktop

A UI is a package list plus a file tree:

```
uis/sway/
  packages.list          packages to install (Debian names)
  files/                 tree overlaid onto the rootfs
    etc/...              copied to /            (owned by root)
    home/...             merged into /home/$USERNAME (owned by the user)
```

`scripts/build-rootfs.sh` stages both into the chroot and
`scripts/rootfs-setup.sh` installs them, rewriting `/home/brunux` to the
configured `USERNAME` inside the text files.

This desktop was ported from the maintainer's Nura (postmarketOS) session, so it
keeps the solarized theme, the `Alt` modifier, the rofi launcher on the Search key
and on `Alt+D`, waybar and the foot terminal.

## Layout

| Path | What it is |
|---|---|
| `etc/sway/config` | System half of the session (replaces Debian's default file). Includes `config.d/*` at the end. |
| `etc/sway/config.d/*` | Drop-ins: logo key, bar, launcher, brightness, waybar, notifications, lock, volume, sleep, gestures, screenshots, theme, touchpad, bar panels, swayfx effects, the LazyOS idle screen, clipboard, tablet mode, terminal file search, idle inhibition. |
| `home/.config/sway/config` | Personal overlay: re-includes `/etc/sway/config`, then adds the keyboard repeat rate and the Search-key bindings (the launcher on Search, the file list on `Alt+Search`, the terminal file search on `Search+Shift`). |
| `home/.config/waybar/{config.jsonc,style.css}` | Solarized waybar theme and the bar itself (see **The bar** below). |
| `home/.config/bin/osd` | The volume and brightness overlay: the key's `pactl`/`brightnessctl` step plus one mako card with its progress bar and the matching icon (no process, no daemon). |
| `home/.config/gtk-4.0/{settings.ini,gtk.css}` | The GTK4/libadwaita half of the palette (`papers` and anything else GTK4): the dark preference the missing Settings portal would otherwise answer, and the solarized named colours. The GTK3 pair next door does not reach it. |
| `home/.config/bin/rofi-learn` | **Learn**: the manual, one page at a time. |
| `home/.config/bin/rofi-keys` | **Keybindings**: every binding, parsed from the configuration, a picture on every row (its tool, its action, or its section). |
| `home/.config/bin/dictate`, `dictate-model` | Dictation with whisper.cpp; the model is fetched (pinned SHA256) on first use. |
| `home/.config/bin/translate` | **Alt+Ctrl+T**: the text selected with the mouse (or the clipboard) into the language Settings sets - English until it is - with Google's JSON endpoint and MyMemory as the fallback; the answer goes to the clipboard and, when long, to a read window. |
| `home/.config/bin/sway-hibernate` | Hibernate, gated on a disk swap and `resume=`. |
| `home/.config/bin/rofi-agents` | **Agents**: launch a coding agent, install one, keep accounts per agent; `--default` is the root menu's one-click `Agent session` row. |
| `home/.config/bin/rofi-capture`, `rofi-qr`, `screenshot` | **Capture** (screenshot, annotate, OCR, QR, record), the Wi-Fi QR code, and the one place a screenshot's folder is decided (`~/Pictures/Screenshots/<today>/`). |
| `home/.config/bin/rofi-dev`, `lazyos-profile` | **Developer** (`Alt+Ctrl+D`): the account into one encrypted archive on a disk (keys, dotfiles, package list), the restore that backs the current profile up first, and a diagnostics bundle for a bug report. `lazyos-profile` is the engine and also runs from a terminal. |
| `usr/local/sbin/lazyos-automount`, `etc/udev/rules.d/90-lazyos-automount.rules`, `etc/systemd/system/lazyos-automount@.service`, `etc/polkit-1/rules.d/49-lazyos-automount.rules` | **A plugged USB drive mounts itself**: the udev rule wants one oneshot unit per partition, the helper mounts it through `udisksctl` as the session user (`setpriv`, because a root mount is `uid=0` on vfat) after refusing anything that is not a removable USB disk or that this system came from, and the polkit rule authorizes that session-less caller through `plugdev`. One process per plug, nothing resident. |
| `home/.config/bin/rofi-files`, `lazyos-thumbnail`, `lazyos-view` | The file browser, the thumbnail generator it calls, and the "open with LazyOS" rule (text/bat, document/papers, image/imv, video/mpv, the rest/Firefox). |
| `home/.config/bin/lazyos-find` | **Search+Shift**: `fd` streamed into `fzf` over the whole home, with a preview pane (see **Files in a terminal**). |
| `home/.config/bin/rofi-power` | **Power** menu: lock, suspend, hibernate, log out, reboot, shut down (the last three ask first). |
| `usr/share/doc/lazyos/*.md` | The seven manual pages the Learn menu opens. |
| `home/.config/rofi/config.rasi` | Launcher settings: the three modes, `terminal = foot`, matching. |
| `home/.config/rofi/solarized.rasi` | The solarized look, shared by the launcher, the window switcher and the bar's dmenu. |
| `home/.config/foot/foot.ini` | Solarized palette, and the font chain that doubles as the icon policy: JetBrains Mono, then Symbols Nerd Font Mono, Font Awesome and Material Design Icons for the session terminal and the bar's popups. |
| `home/.config/kitty/kitty.conf` | The same theme for **kitty**, if it is installed: nothing in the session starts kitty (foot is `$term`), so this file is only there to keep `apt install kitty` from adding a second, unthemed palette. |
| `home/.bashrc` | The interactive shell: `ls` is eza, `cat` is bat, `cd` is zoxide, fzf key bindings and the starship prompt (see **The CLI tools**). |
| `home/.config/starship.toml` | The powerline prompt in the session palette: user, directory, git, languages, container and time, with glyphs the image's fonts actually draw (see **The CLI tools**). |
| `etc/profile.d/90-cli-tools.sh` | The CLI environment (`MANPAGER`, `BAT_THEME`, `PATH`), shared by login shells and `~/.bashrc`. |
| `etc/yt-dlp.conf` | Names Node as yt-dlp's JS runtime (see **The CLI tools**). |
| `home/.config/foot/screensaver.ini` | The idle screen's terminal: black, no padding, 26 pt, solarized orange foreground. |
| `home/.config/mako/config` | Solarized theme for the notification daemon (`mako-notifier`). |
| `home/.config/lazyos/screensaver.txt` | The LazyOS art the idle screen loops: the wordmark, the caption and the arcade line. |
| `home/.config/bin/lazyos-screensaver` | Launches the idle screen and is also the program that runs inside it (see **The idle screen**). |
| `home/.config/bin/sway-lock` | Locks the session and waits for swaylock's readiness line; stops the idle screen first (see **The idle screen**). |
| `home/.config/bin/bar-*` | The menus the bar opens on click: `bar-menu`, `bar-net`, `bar-bt`, `bar-key`, `bar-audio`, `bar-updates`, `bar-power`, plus the shared `bar-pick`. |
| `home/.config/workstyle/config.toml` | Icon map for workstyle (see below). |
| `home/Pictures/Wallpapers/lazyos.png` | The desktop wallpaper, rendered from `brand/wallpaper.svg` and referenced by `etc/sway/config`. |
| `home/Pictures/Wallpapers/mountains.jpg` | The lock screen's photograph, referenced by `bin/sway-lock`. |
| `etc/keyd/default.conf` | Chromebook keymap for keyd: F-keys and VT switching (see below). |
| `etc/udev/rules.d/90-brightnessctl.rules` | Gives the `video` group write access to the backlight, so `brightnessctl` works for the unprivileged session (see below). |

## The compositor

The session runs **swayfx**, the wlroots fork that draws the effects Deboma gets from Hyprland:
rounded corners, shadows under windows, and a blurred backdrop under the three translucent
surfaces (the terminal, the launcher and the bar). Debian only packages
plain sway, so both the fork and its effects library are built from source
(`compositors/swayfx/`, `scripts/build-swayfx.sh`) and installed as `swayfx` + `libscenefx-0.5`,
which replace Debian's `sway` package. That is why `packages.list` does not mention `sway`: the
binary, `/etc/sway` and the greeter's `sway` command are the same either way, but
`scripts/install-swayfx.sh` has to put the fork in place.

`etc/sway/config.d/26-swayfx.conf` enables the effects and tunes the one that is not free:

| Effect | State | Why |
|---|---|---|
| `corner_radius 6` + `smart_corner_radius` | on | The corner is cut out of the window's buffer when it is committed; with the 8 px inner gap the radius is visible and nothing is done per frame. |
| `shadows` | on | One blurred texture per window, drawn once per frame. |
| `default_dim_inactive 0.0` | off | The solarized palette already dims unfocused borders; dimming a terminal turns its text grey. |
| `blur` | on | A tiled window samples a buffer blurred once from the wallpaper (one bilinear tap, 0.07-0.11 ms full screen); a floating window, and any window for the duration of its animation, is re-blurred live from what is behind it (2.1-2.2 ms, scaling with its area). |
| `blur_xray` | off | On, every surface would sample the cached wallpaper, including a floating window - cheapest, but the wallpaper shows through it instead of the window behind. |
| `blur_passes 2` / `blur_radius 4` | | A 32 px sample instead of the shipped 80 px, and the distance a live blur grows its damage by. |
| `blur_noise 0`, neutral brightness/contrast/saturation | | SceneFX skips its second full-screen pass when nothing is tinted; noise would also be visible grain. |
| `animation_duration_ms 150` | on | The only animation knob swayfx 0.6 has (no per-event directives, one fixed ease-out-cubic): open/close, tiling moves and the workspace cross-fade, ~9 frames at 60 Hz. It sends each animating window down the live blur path for that duration. |
| `layer_effects` | bar + launcher | The bar only has the wallpaper behind it and takes the cached path; rofi floats over windows and takes the live one. |

Blur is drawn *behind* a surface, so it only exists under one with alpha < 1: the terminal
(`foot.ini`, `alpha=0.87`), the launcher (`solarized.rasi`, 0.82, and 0.88 on the selected row), the
bar (`style.css`, 0.71, and 0.80 on the cards) and, if it is installed, **kitty**
(`kitty.conf`, `background_opacity 0.87`) are the carriers. Everything else in the session is
opaque on purpose. `docs/optimization.md` §7 has the measurements, the method, and why the renderer
question has only one answer here.

The drop-in is swayfx-only, so `sway -C` failing means Debian's plain sway is installed again
(`scripts/check-system.sh` fails on that, and on a `sway --version` that does not say `swayfx`).

## The launcher

Three jobs, one binary: **rofi** 2.0, themed by `~/.config/rofi/config.rasi` (settings) and
`~/.config/rofi/solarized.rasi` (the look).

| Binding | Command | What it lists |
|---|---|---|
| `$mod+d` | `rofi -show drun` | the `.desktop` entries, including the port's own session actions in `usr/share/applications/org.lazyos.*` |
| Search key | `rofi -show drun` | the applications, the same menu as `$mod+d` |
| `Alt+Search` | `rofi-files --search` | every file under `$HOME`, newest first, narrowed by typing (one bounded `fd` run; see "The file browser") |
| `$mod+Tab` | `rofi -show window` | the open windows, through wlroots' `zwlr_foreign_toplevel_manager_v1` - sway has no switcher of its own |
| `$mod+Shift+d` | `rofi -show run` | every executable in `$PATH`; **Enter** runs it, **Shift+Enter** runs it in foot, which is how the TUIs from `packages/tools.list` are started |
| any bar panel | `bar-pick` → `rofi -dmenu` | the fixed list the bar hands it |

The Search key opens the launcher, as it did before the file search borrowed it:
the magnifier is the key people press for the applications, and `$mod+d` stays as
the other way to the same menu. `Alt+Search` is the file search - the key that
used to be here - because the launcher already answers "which application" one
key away. The bindings live in `~/.config/sway/config` (not in
`19-launcher.conf`, which documents the launcher alone) and are
`--release --no-repeat`, because the top row doubles as `F1`-`F10` while Search
is held down (see *Virtual terminals*).

It replaces **tofi** (launcher) and **fuzzel** (bar popups), so the session runs one picker
instead of two. Measured on this machine, per process, in the same headless compositor the bar
was measured in:

| Launcher | PSS while open | Notes |
|---|---|---|
| **rofi**, drun, this theme | **15.7-17.1 MB** | text only |
| rofi, drun, **icons on**, ten rows | **13.2 MB** | measured again with the LazyOS icon theme: only the visible rows are decoded, and the 57 MB figure came from a run that decoded far more |
| rofi, `-dmenu` (a bar click) | 14.2-15.6 MB | |
| rofi, `-show window` | 16.7 MB | |
| tofi, drun (before) | 14.6 MB | applications only; its run mode just printed the selection |
| fuzzel, `--dmenu` (before) | 40.5 MB | what every bar click cost |
| wofi, drun | 62.1 MB | |
| walker / Wox | resident | a GTK4 service / a ~150 MB daemon |

None of these is a daemon: rofi exists while its window is on screen and nothing stays
afterwards. rofi 2.0 speaks Wayland natively - it is a layer-shell client, and the window
mode uses the foreign-toplevel protocol - so no Xwayland is involved, even though the
package still supports X11.

The theme is the session palette: base03 window, a 2 px cyan rim, JetBrains Mono 12, the
prompt in cyan, entries in base1, the typed text in base2, and the selected row a base02
card with 6 px corners. The layout drops what rofi shows by default and this session does
not need (mode switcher, message bar, counter) and the box shrinks to the number of
entries it has, up to ten, so a three-item bar menu and the eight-application list both
get a box that fits them. Matching is fzf-style fuzzy.

**Icons: font glyphs in the lists, images everywhere a name is asked for.** Every list this port
draws itself puts a Font Awesome codepoint in the row text (`~/.config/bin/lib-rofi.sh`), the same
alphabet the bar uses: it costs nothing, follows the palette and is filtered away by typing. The
launcher and the window switcher show the application's real icon, which rofi decodes with
gdk-pixbuf - about 13 MB while ten rows are on screen and nothing afterwards. The file browser is
the one list with pictures instead of glyphs: `rofi-files` puts a 32 px thumbnail on every row
that has one, and a theme icon on every row that has not (see "The file browser" below).
Notifications are a fourth consumer of the same theme, through mako's `icon-path` (see
"Notifications").

Debian's Adwaita 51 ships only monochrome *symbolic* icons and not one of this session's
names - a drun row for "Files" had a hole where its icon should be - and the one full-colour
theme in the archive (`papirus-icon-theme`) is 190 MB installed. So `scripts/make-icons.sh`
renders the same Font Awesome codepoints into 48x48 PNGs and they ship as a small **LazyOS**
icon theme (`usr/share/icons/LazyOS`), whose `index.theme` inherits Adwaita and hicolor: an
application that brings its own icon (foot, btop, htop, anything installed later) still shows
it in colour. The bar's popups pass `-no-show-icons` because they draw their own glyph.

The theme holds **three tables**, because three different things ask it for a name:

| Table | Who asks | Example |
|---|---|---|
| `.desktop` names | the launcher (`rofi -show drun`) | `org.lazyos.files` |
| binary names | the command palette (`rofi -show run`) | `foot.png`, `htop.png` |
| freedesktop names | `rofi-files` rows and mako notifications | `folder`, `text-x-generic`, `application-pdf`, `dialog-error` |

The command palette needed its own table for a reason that is not obvious: rofi's **run** mode
asks the icon theme for an icon *named after the binary* it would run
(`rofi_icon_fetcher_query(str[0], …)`), not for a `.desktop` icon, so without an icon called
`foot.png` a palette row has a blank 26 px column. The fixed column keeps the rows without one
aligned, and `check-system.sh` warns (never fails) when they are missing. The third table is the
one that grows with a feature: a new row icon or a new notification icon is a name added to
`make-icons.sh` and a PNG regenerated on the build host (see the icon rule in `AGENTS.md`).

That rule is also an **order**, and the order is what keeps it from being forced: an existing
`UI_I_*` glyph, then a name an existing table already carries, then a new name plus a regenerated
PNG, then **text alone**. The last step is a real answer, not a failure to try - the bar's popups
pass `-no-show-icons` because they draw their own glyph, an fzf row cannot carry a Font Awesome
codepoint (foot's fallback is Symbols Nerd Font Mono, the row would be a tofu box, and that
feature's picture is its preview pane), and `check-system.sh`'s report stays plain because other
scripts grep it. Nothing is installed and nothing stays resident just to draw.

That was measured, not assumed: the first attempt at this theme shipped SVGs with a literal
private-use character, which does not survive every editor or transport, and the first
`make-icons.sh` ran `printf` under dash, which has no `\u` escape - both produced a row of
empty spans, which is what the check for an empty canvas in the script now prevents. The theme also
has to offer the icons through a *scalable* directory: a theme that shipped only `26x26` resolved
nothing in rofi, while `48x48` plus a scalable entry does.

![The launcher: the session's entries with LazyOS icons, and foot's own hicolor icon](../../docs/screenshots/hayato-r1-launcher.png)

Two behaviours were verified rather than assumed: `rofi -dmenu` exits 1 on Escape, which is
what `bar-pick`'s callers' `|| exit 0` relies on, and drun honours `Terminal=true` through
the `terminal` setting (`foot`) - run mode needs Shift+Enter for that, rofi's own "run in
terminal" binding.

To check the theme after an edit, run `rofi -show drun` in the session: a parse error is a
dialog naming the file, line and column. `rofi -dump-config` does **not** report theme parse
errors.

## The menus

Everything that is not the launcher or a bar panel is a `rofi-*` script in
`~/.config/bin/`, built on three helpers in `lib-rofi.sh` (`ui_row` for the one row style,
`ui_pick` for the table-and-key round trip, `ui_say` for the result):

| Script | Key / entry | What it does |
|---|---|---|
| `rofi-quick` | `Alt+Ctrl+O`, Settings | the switches of the session in one list: Wi-Fi and Bluetooth radios (with the pickers one row below), night light, do-not-disturb, touchpad, tablet mode (the on-screen keyboard and the rotation, see the tablet section) and **Calibrate rotation** while the mode is on, rotation, keyboard layout, what **Alt+Ctrl+T** translates into, audio, media, battery. It redraws after every switch, so the state it shows is never stale, and its probes run in parallel (two `bluetoothctl` clients, `nmcli` and `swaymsg` in sequence made it feel broken) |
| `rofi-power` | `Alt+Escape`, `Alt+Ctrl+P`, Power menu | lock, suspend, hibernate, log out, reboot, shut down; the last three ask once more through `bar-pick` |
| `rofi-notify` | `Alt+Ctrl+N`, Notifications | mako's history (`makoctl history -j`), Enter copies one, plus restore, dismiss-all and do-not-disturb |
| `rofi-clean` | Free space | `df` plus what grows (trash, thumbnails, apt cache, journal, `~/.cache`), one action per row, re-measured afterwards |
| `rofi-kill` | End a process | the heaviest twenty by resident memory; SIGTERM, then an explicit offer of SIGKILL |
| `rofi-tz` | Time zone | `timedatectl list-timezones` with the current one marked, and the network-clock switch. It restarts waybar afterwards: `timedatectl` only rewrites `/etc/localtime`, and a running process keeps the zone glibc cached the first time it called `localtime()` |
| `rofi-places` | Places | home, documents, downloads, pictures, every mounted filesystem, and a **Mount** row for each unmounted one (an SD card in the internal slot, or a USB stick with auto-mount off) through `udisks2`; Enter browses there with `rofi-files`, Alt+1 unmounts the disk under the row |
| `rofi-swap`, `rofi-format` | Places (swap is also in the root menu) | the two disk actions, both drawn from `ui_disks` in `lib-rofi.sh` (nothing mounted, no ChromeOS kernel partition, no SPI flash or eMMC boot partition): **swap** turns a card into a `lazyos_swap` partition and the fstab block that activates it on every boot (priority 10, below zram; `nofail` and a 15 s timeout), with **Turn it off** as the undo; **format** wipes a disk clean as FAT32 or ext4 |
| `rofi-media` | Media | the current track and play/pause/next/previous through `playerctl` |
| `rofi-calc` | `Alt+Ctrl+Q`, Calculator | awk, one prompt; Enter copies the result |
| `rofi-layout` | Settings → Keyboard layout | every layout in `evdev.lst`, applied to all keyboards and kept in `~/.config/sway/config.d/90-keyboard-layout.conf` |
| `rofi-emoji` | `Alt+Ctrl+E`, Emoji | the table `scripts/make-emojis.sh` generates (`~/.config/emojis.tsv`, ~2000 rows); Enter inserts at the cursor, Alt+1 copies only |
| `rofi-clip` | `Alt+Ctrl+V`, Clipboard | the `cliphist` history: Enter copies an entry back, Alt+1 deletes it, plus "save the newest image" and "clear the history" |
| `rofi-dns` | DNS | the resolver of the connection that is up: Cloudflare, Google, Quad9, AdGuard, typed servers, or back to DHCP |
| `rofi-vnc` | Settings → Share this screen, root menu, and the bar's eye | `wayvnc` serves this session **read-only** (`--disable-input`) on **127.0.0.1:5900**, started and stopped from the list, with the `ssh -L` tunnel command on a row that copies it; the loopback bind is what makes the ssh login the authentication (wayvnc only authenticates through a certificate and a password file this image does not ship), and `custom/vnc` puts an eye on the bar while it runs - a state file being read, not a daemon |
| `rofi-pkg` | Packages | `apt-cache search`, then `apt-get install -y` behind a busy card; installed packages are marked before they are picked |
| `rofi-dev` | `Alt+Ctrl+D`, Developer | export the profile to a disk (mounted, mountable through udisks, or `~/Downloads`) as `lazyos-profile-<host>-<date>.tar.zst.gpg` with a plaintext manifest beside it, passphrase typed into rofi; restore an archive after backing the current profile up to `~/.cache/lazyos/backups`; write a diagnostics bundle (system report, this boot's journal, dmesg, hardware and packages). A tar because exFAT drops modes and symlinks, encrypted because it carries `~/.ssh` and `~/.gnupg` |
| `rofi-color` | Colour | a pixel (`slurp -p` + `grim` + `ffmpeg`), and the sixteen solarized colours as swatch rows |
| `rofi-remind` | Reminders | transient `systemd-run --user --collect` timers, listed back from systemd, cancelled by choosing one. A title, an **optional** note, then when: five delays or a repeat (a date and time, daily, weekdays, weekly, monthly on a day 1-28). Every custom expression is validated with `systemd-analyze calendar` and the card shows systemd's own next occurrence; title and note ride in the timer's `Description` |
| `rofi-about` | About | model, revision, kernel, uptime, memory, disk, battery, network, compositor, processes |
| `rofi-learn` | Learn | the manual, rendered with glow, with a search across every page |
| `rofi-files`, `rofi-keys`, `rofi-agents` | see below | the file browser, the key view, the agents menu (`lazyos-thumbnail` is the helper the first one calls for its row pictures) |

![Settings: the session's switches in one list, each row with its state](../../docs/screenshots/hayato-r1-settings.png)

Each one is a single process while its window is open and nothing afterwards. They all end in
`ui_pick` (`ui_pick_alt` when a second action sits on Alt+1), which is `bar-pick` (rofi's dmenu)
over a table: rofi hands back the *rendered* row, so the key of the chosen row travels in a second
column that markup cannot be parsed out of.

## Clipboard history and emoji

These two are the batch that took the session its first resident process and its first generated
data file, and both are worth the exception:

* **`rofi-clip`** (`Alt+Ctrl+V`) is a list over `cliphist`, which is fed by
  `wl-paste --watch cliphist store` (one `exec` line in
  `etc/sway/config.d/27-clipboard.conf`). About 2 MB, and there is no history without it:
  `wl-clipboard` alone only knows the current selection. Rows are classified as text, link or
  image from what `cliphist` keeps; an image is listed by its size and dimensions, not by a
  thumbnail, because a rofi row is one line. "Save the newest image" decodes it into
  `~/Pictures/clipboard/` with the extension its MIME type implies and opens it. The database is
  `~/.cache/cliphist/db` and holds everything copied, secrets included; the menu's last row
  clears it.
* **`rofi-emoji`** (`Alt+Ctrl+E`) reads `~/.config/emojis.tsv`, which `scripts/make-emojis.sh`
  generates on the build host from Python's Unicode database (name, glyph, block: 1988 rows, the
  emoji blocks of the standard plus Dingbats and Miscellaneous Symbols). The image needs neither
  Python nor a package for it — the file is committed and copied with the home directory. Insertion
  is Omarchy's: the glyph goes to the clipboard and `wtype -M shift -k Insert -m shift` types a
  virtual Shift+Insert, which every toolkit here understands as paste; `wtype -t "$glyph"` would
  need a keysym for the codepoint, which most emoji do not have. Alt+1 is the clipboard write
  alone. `fonts-noto-color-emoji` draws the glyph, and `check-system.sh` asks `fc-match emoji`.

The other five lists (`rofi-dns`, `rofi-pkg`, `rofi-color`, `rofi-remind`, `rofi-about`) are plain
scripts with no such trade-off: DNS is two `nmcli` properties plus a re-activation, packages are
`apt-cache`/`apt-get` behind the busy card, colour is `slurp` + `grim` + `ffmpeg -f rawvideo`
(no ImageMagick), reminders are transient timers that fire `notify-send` once, and about reads
procfs and sysfs.

## Something is happening

A scan or a connection takes seconds; a menu that answers only when it is done reads as a
click that was never registered. `~/.config/bin/busy` is the one place that says otherwise, and
it says it twice:

* a **mako toast** that does not expire while the action runs and then *becomes* the result -
  the same card reads "Looking for networks…", "9 networks in range", "Connected to Home", or
  the error in red. Every message carries the same `x-canonical-private-synchronous` tag, so
  mako replaces the card instead of stacking one per step;
* a **spinner on the bar** (`custom/busy` → `bar-busy`), a braille frame with the task name in
  yellow, left of the updates indicator - and it is also a button.

`busy start|update|stop|end|fail TAG LABEL MESSAGE [ICON]` writes one file per task in
`$XDG_RUNTIME_DIR/lazyos-busy` and signals waybar (signal 8, the same mechanism the other `bar-*`
modules use). `bar-busy` prints `{"class":"empty"}` when there is nothing to show, which
`style.css` collapses to zero width, and it drops a state file older than five minutes so a
script killed mid-action cannot leave a spinner on the bar forever.

**Clicking the spinner stops what it names.** The producer registers the command in `BUSY_STOP`
as it starts (`pkill -f "[n]mcli.*device wifi list"` - a pattern, not a pid guessed from the
label), `busy start` writes that line to `<tag>.stop` (mode 0600), and `bar-busy stop` - the
module's `on-click` - turns the card into `Stopped Wi-Fi` and runs it. The producer then asks
`busy cancelled wifi`, which answers 0 once the card is gone, and stays quiet: a killed `nmcli`
would otherwise be read as an empty network list, or as a failure. A `.stop` from a previous run
of the same tag is never inherited, so a producer that registers nothing has no button rather
than the last one's. The tooltip of a stoppable card ends with *click to stop*; clicking one that
cannot be interrupted safely shows "… cannot be stopped from here" and leaves it running:

| Stoppable | Not stoppable, and why |
|---|---|
| the Wi-Fi scan and join, the Bluetooth scan, the `du` walk in Free space, the dictation recording | an `apt` install (dpkg's lock outlives it), the DNS switch (the profile is already written), the radio toggles (milliseconds) |

![A Wi-Fi scan in progress: the mako card and the bar spinner](../../docs/screenshots/hayato-r1-busy.png)

Wired into: the Wi-Fi rescan, connect, disconnect and radio switch; the Bluetooth scan (three
seconds), connect, disconnect and radio; the `du` walk in Free space; the dictation recording.
Two lines per action, plus a `BUSY_STOP` when it can be interrupted:

```sh
BUSY_STOP=nmcli busy start wifi Wi-Fi 'Looking for networks…' network-wireless
scan=$(nmcli --terse --fields IN-USE,SSID,SECURITY,SIGNAL device wifi list --rescan yes)
busy stop wifi
```

## The file browser

`rofi-files` (`Alt+Ctrl+F`, "Files" in the root menu and the launcher) is a shell script around
`rofi -dmenu`. rofi 2.0 *does* have a `filebrowser` mode (with sorting, hidden files and a
`recursivebrowser`), but with `-show-icons` it drew no thumbnails here at all - the XDG cache was
never written and every row kept its mimetype icon - and it would replace the markup rows and the
`bat`/popup behaviour below, so the dmenu path is what this port uses:

* the rows are Pango markup, directories first (cyan, trailing `/`), then files with the size
  and the date dimmed; `find -L` decides the type, so a symlink to a directory navigates and a
  broken link says so;
* the name column is padded to the longest name in the listing, which works because the theme
  is monospace, and dmenu matches the text **with the markup stripped** (verified: a filter on
  a tag name finds no row), so typing a name is not disturbed by the styling;
* **every row carries an icon** (32 px here only - the launcher and the bar popups keep the
  theme's 26 px). A row is `text\0icon\x1fthumbnail://<path>` when a picture can be decoded, and
  `text\0icon\x1f<theme name>` when it cannot: `folder`, `go-up`, `view-hidden`,
  `dialog-warning`, `text-x-generic`, `text-html`, `application-pdf`, `image-x-generic`,
  `video-x-generic`, `audio-x-generic`, `x-office-document`, `package-x-generic`,
  `application-x-executable`. The name comes from the extension plus the executable bit, both
  already in the `find` listing, so a directory of source files is a list with a column of icons
  instead of bare text - the report's "no icons in Files". `-show-icons` with `-preview-cmd`
  makes rofi call `~/.config/bin/lazyos-thumbnail` for the rows it is about to draw, all at once,
  and only when that picture is not in `~/.cache/thumbnails` yet. The script is ffmpeg for an
  image or a video frame, `pdftocairo` for a PDF's first page, and nothing at all for a format it
  does not know (the row then keeps its theme icon);
* Enter opens a directory and reads a text file with `bat --paging=always` (`BAT_THEME=ansi`)
  inside the floated `popup` foot window. Anything else goes where its kind belongs, which is
  the extension's decision: a **document** (PDF, DjVu, a comic) is opened by **papers**, a reader
  with search, zoom and annotations instead of a 280 MB browser tab (printing needs `cups`, which
  the image does not ship, so "Print" says it cannot find a service); a **picture** is
  drawn by **imv** on the GPU (zoom, pan, rotation, a slideshow of the folder), with **chafa**'s
  sixels in one foot window as the fallback for a GPU where imv gets no EGL context (`-f sixels`
  is a protocol foot renders natively, 16 MB measured for a 24 MP photo); a **video** is played
  by **mpv** in its own window (`setsid`, seek, subtitles, sound, and `mpv-mpris` loaded so
  `playerctl` and the Media menu see it); an HTML file opens as text because it *is* text, and a
  `.deb` is Firefox's download prompt. The image family is decided **before** the text test, or
  an SVG - a text file - would open as source in bat and never as a picture. The first 4 kB are
  checked for a NUL to tell text from binary, because bat would show a binary as an empty page;
* `↑ parent directory` is the first row (and the one that is *not* preselected), the dotfiles
  are toggled by the last row, and a typed path that matches nothing is taken literally, which is
  what makes Ctrl+Return useful;
* the line under the prompt carries the path, the counts and the free space.

**`rofi-files --search` is Alt+Search's mode**: one bounded `fd` run over `$HOME`
(`--type f --max-results 500`, with `.cache`, `.git`, `node_modules` and
`.local/share/containers` excluded), one `stat` for the metadata of what it found, newest first,
each row named by its `~/relative/path` and carrying the same icon the browser would draw.
Enter opens the file exactly as a browser row does; the list is bounded by construction (501
rows of a 700-file home in 92 ms here, no subprocess per row), so a very broad query is answered
by typing one more letter. Escape closes either mode with nothing, and a missing `fd` says so
and opens the browser instead. Debian's binary is `fdfind`; `/usr/local/bin/fd` is the symlink
the key's command uses.

Thumbnails are generated **lazily and in parallel, which is rofi's doing**: it asks for the rows
it is about to show and starts one process per row at the same time (8 started within 30 ms), so a
screenful of ten 1280x800 PNGs takes 0.97 s through the gate against 2.0 s one after another, and
scrolling asks for the next screenful only. Because a 24 MP PNG decodes into ~131 MB, the script
gates itself to four decoders with `flock` slots and passes `-threads 1 -filter_threads 1`, which
keeps the parallelism across files; `-lowres 2` decodes a JPEG at a quarter of its size in one step
(53 ms and 50 MB against 179 ms and 83 MB for 24 MP). rofi owns the cache
(`~/.cache/thumbnails/normal/<md5 of the path>.png`, 3-8 kB a picture) and skips
the command when the file is there, so a picture is decoded once, ever. The cost is 23 MB of
PSS for a window of fourteen thumbnail rows against 17 MB for the same window without them, and
nothing afterwards; the numbers are in `docs/optimization.md`.

Nothing is resident: the listing is built per run, and the viewer is imv's, papers', mpv's,
foot's or Firefox's own window. `check-system.sh` fails when the script, the browser, `imv`,
`papers`, `mpv`, `mpv-mpris`'s plugin, `ffmpeg`, `pdftocairo` or `chafa` is missing, because
Alt+Ctrl+F and the menu entry would silently do nothing (or answer "no browser is installed"),
and warns when the thumbnail cache is not writable or the script stops asking for pictures.

`usr/local/bin/xdg-open` is two dozen lines around the same question: a URL goes to Firefox as it
is, and a path goes to **`lazyos-view`** - the same rules a double click in favnyr applies - with
the browser as the answer for a user who has no session `bin` directory (root, an ssh session).
That is what foot's ctrl+click, `yt-dlp --print-trailer` and any script that shells out to
`xdg-open` expect, and there is no desktop portal in this image.
A file opened from **outside** the session - a download manager, a terminal, a `file://` link -
goes through `~/.config/bin/lazyos-view` (`etc/xdg/mimeapps.list` and `org.lazyos.view.desktop`),
which applies the same rules as the browser: text in `bat`, a document in `papers`, a picture in
`imv` (chafa when imv gets no EGL context), a video in `mpv`, and an HTML page, an EPUB or a
`.deb` in Firefox - so a double click on a video does not open it in the browser.

![rofi-files over ~/Documents, with the sizes and dates dimmed](../../docs/screenshots/hayato-r1-files.png)

## Files in a window (favnyr)

The mouse half of the same story is **favnyr**: `uis/sway/files/usr/bin/favnyr`, one statically
linked Slint binary (Brunux/Favnyr-aarm64 v0.3.6, 36 MB) committed in the tree. It is a second
entry, not a replacement: `Alt+Search` and `Alt+Ctrl+F` keep opening `rofi-files` (one process, no
window, a list a key opens and a key closes), and `Alt+Ctrl+Shift+F` opens the window for what a
list cannot do - drag and drop, several files at once, rename in place, the trash. The launcher
carries both rows (**Files**, **Files (window)**) and so does the root menu.

* **Why it is a binary in the tree and not a package**: it is not packaged for Debian, and the two
  alternatives were both expensive. nautilus is a **504-package** closure (GTK4 + libadwaita, 38 MB
  of ICU, GStreamer, 31 MB of `samba-libs`), needs a **resident indexer** (`localsearch` crawling
  `$HOME`) and decodes thumbnails through glycin's sandboxed helpers; the GTK3 manager this image
  used before it was 229 packages and 4.9 MB for a window. favnyr links neither GTK nor libgio, so
  it pulls in nothing - and nothing can upgrade it behind the image's back, which is why the
  version and the SHA are the commit that carries it. The measurements of the rejected alternatives
  are in `docs/omarchy-gap.md` §1.
* **A double click opens what the Files list opens.** `/etc/xdg/mimeapps.list` sends the types to
  `org.lazyos.view.desktop`, whose `Exec` is `~/.config/bin/lazyos-view` - the same rule table as
  `rofi-files` (which reads an HTML file as text): text in `bat`, a document in `papers`, a
  picture in `imv` (or `chafa`'s sixels when imv gets no EGL context), a video in `mpv`, and an
  HTML page, an EPUB or a `.deb` in Firefox. `inode/directory` names `pcmanfm.desktop` on purpose:
  the *name* is what GIO and the launcher know, and the file that carries it is the user-level
  shadow `~/.local/share/applications/pcmanfm.desktop` whose `Exec` is `favnyr` and whose `Name`
  is *Files (window)*. There is no desktop portal here, so that pair *is* the "open with"
  mechanism for every GIO program.
  **A trap worth knowing**: GIO silently drops a `.desktop` whose `Exec` program does not exist,
  so a missing executable bit on `lazyos-view` - or on `favnyr` itself - reads as "no default
  application" rather than as an error, which is why `check-system.sh` asks `gio mime image/png`
  *and* checks both modes. No `mimeinfo.cache` regeneration is needed for the associations to
  resolve (verified).
* **Its icons come from the session's theme, not from its own resources.** favnyr reads
  `gtk-icon-theme-name=` out of `home/.config/gtk-3.0/settings.ini` (it parses that file) and asks
  the named theme for the folder, trash, drive and mimetype pictures it draws. Adwaita 51 ships no
  full-colour `folder`, `user-home` or `drive-harddisk` at all, so `scripts/make-icons.sh` gained a
  **fourth table** (the places, the actions and the mimetypes the sidebar and the icon view ask for
  by name) and the LazyOS theme carries them in `48x48/apps` + `scalable/apps`. That one key is
  also why the GTK3 pair matters to a window that links no GTK.
* **A terminal from inside it is foot, through `$TERMINAL`.** favnyr has no `terminal=` setting: it
  runs `$TERMINAL` and then a built-in list of emulators (kgx, konsole, gnome-terminal, kitty,
  alacritty, wezterm, xfce4-terminal, lxterminal, tilix, xterm) that does **not** include foot.
  `/etc/environment` therefore carries `TERMINAL=foot`: the window is started by the compositor, so
  `/etc/profile.d` is never read on that path, while `pam_env` reads `/etc/environment` in greetd's
  session stack and in sudo's. Without it the menu item answers *no terminal detected* and starts
  nothing.
* **A disk it cannot unmount says why.** The removal action goes through `udisksctl`, and when
  udisks refuses because something holds the mount, the window runs `fuser -m` to fill its
  *In use by: …* sentence - so `psmisc` is in the package list. `eject` is not involved; it was the
  GTK3 manager's dependency and nothing in this image execs it any more.
* **`Alt+Ctrl+Shift+F` is the Shift variant of the Files key** (`Alt+Ctrl+F`), which is the key
  scheme's pattern: the plain chord opens the menu, the Shift chord the window it stands for. It
  keeps the Search key for lists and filters (launcher, file list, terminal search), and it is the
  same shape Omarchy uses for its file manager.
* **The settings are TOML under `~/.config/favnyr/`** (`config.toml`, `openers.toml`,
  `favorites.toml`, `workspace.toml`; `favorites.toml` and `workspace.toml` live in
  `~/.local/share/favnyr/`), the log is `~/.cache/favnyr/favnyr.log`, and both roots are in
  `lazyos-profile`'s list so a profile export carries them. The window follows the palette by
  itself - it draws its own widgets - and previews come from tools the image already has
  (`ffmpeg`, poppler's `pdftoppm`/`pdftocairo`, the same ones `lazyos-thumbnail` uses).
* **Never run it to ask its version.** `favnyr --version` parses the flag as a startup location
  (*WARN favnyr: invalid startup location location=--version*) and then opens its event loop;
  `--help` does the same. `check-system.sh` hung on exactly that line, which is why it now asks for
  the exec bit instead.

## Files in a terminal (lazyos-find)

**Search+Shift** opens `lazyos-find`: `fd` streamed into **fzf** in a floated foot window, one row
per file under `$HOME`, a preview pane on the right, and Enter handing the choice to `lazyos-view`
- the same opener a double click in favnyr reaches. It is the third answer to "where is that
file", and the three are one feature with three shapes: `rofi-files --search` is a *menu* (rows
with icons and thumbnails, open before you type, one bounded `fd` run, a list you scroll),
favnyr is the *window* (drag and drop, the trash), and this is the *filter* - uncapped, live
while the walk is still running, with the file's contents beside it.

**It is a release binding**, like the launcher on Search and the file list on `Alt+Search`: sway
matches a release binding against the modifiers held when the key comes up, so Search released
with Shift still down lands here and Search alone falls through to the launcher. The top row
doubles as F1-F10 while Search is held (keyd), which is why none of the three fires on the press.

**Why fzf and nothing new.** It is already in the image for the shell's `Ctrl-R`/`Ctrl-T`/`Alt-C`
bindings, it matches *while* the producer streams instead of after it, and `--read0`/`--print0`
make a name with a space, a tab or a newline one row and one argument. Measured on the build host
(19.4k files after the excludes), prompt to a keystroke, through a pty:

| | first paint | keystroke | PSS while open |
|---|---|---|---|
| **fzf 0.74.4** over `fd` | **24 ms** | **5-35 ms** | **35.6 MB** |
| fzf's own `--walker`, same tree | 9-16 ms | 7-21 ms | the same process |
| the menu, for comparison | rofi's own start | rofi filters a fixed list | 17-23 MB (`docs/optimization.md` §8) |

* **`skim`** (Debian has it) is the same program in Rust with no advantage here: it would be a
  second fuzzy finder in a 4 GB image, against one that is already wired into the shell.
* **`television`/`tv`** is the fast new one and is **not packaged in forky**, so it would mean
  `cargo` on the device or a vendored binary of its own. (workstyle *is* built from a pinned
  crates.io tarball - `tools/workstyle/`, `scripts/build-workstyle.sh` - because the bar's
  workspace icons have no other source; a second finder does not earn the same exception.)
* **`fzy`** is tiny and slower than fzf on big lists, with no preview and no bindings to build on.
* **fzf's built-in `--walker`** would drop the `fd` dependency for this one feature and is 8 ms
  faster to first paint, but it does **not** read `.gitignore` - on a home full of repos that is
  build output and vendor trees in the list. `fd` is already installed and already the tree walker
  of every other file feature, so the list is one tool's answer everywhere.
* Memory is twice the menu's (17.1 MB of rofi against 35.6 MB of fzf), and that is the trade: 35.6
  MB of fzf plus foot's ~25 MB while the window is open, nothing afterwards. The menu stays the
  cheap answer.

Four things that are not obvious, each of them a bug that was caught:

* **`--print0` makes `fd` prefix every relative path with `./`** (it does not in newline mode), so
  every row and every path handed to the opener carried two characters nobody types.
  `--strip-cwd-prefix` is what removes them.
* **Dotfiles are included here and hidden in the menu.** The menu's pool is capped, so a dotfile
  takes a slot a likely answer could use; here the pool is the whole tree and `~/.config` is a
  place people look. The four excludes (`.cache`, `.git`, `node_modules`, `.local/share/containers`)
  are the same ones both use - `.cache` alone is 330k of the 350k files in this home.
* **The preview draws pictures with `chafa -f symbols`, not sixels.** The preview is a pane of the
  same terminal, not a window of its own, so a sixel picture would paint over fzf's next redraw.
  The picture itself is `lazyos-thumbnail`'s (the generator the menu uses, into
  `~/.cache/thumbnails/normal/`), bounded by `timeout 8`: a preview that waits is worse than a
  preview that says nothing. Text is `bat --line-range=:300` at the pane's own width, taken from
  the `FZF_PREVIEW_COLUMNS`/`LINES` fzf exports.
* **`set -e` does not see fzf's Escape.** The status of `fd | fzf | xargs` is `xargs`'s, so
  cancelling is not an error - and `--no-multi` keeps a Tab that marked three rows from opening
  one and saying nothing about the others.

The window is floated and centred by `etc/sway/config.d/29-file-search.conf` (`app_id=lazyos-find`,
1200x760), and the binding is the `Super_L+Shift` release line in `home/.config/sway/config` next
to the launcher's.
`check-system.sh` asks for the binding, the
executable bit, `fzf`, the `system-search` notification icon and (through `lazyos-view`) the
opener, because each one on its own makes the key look dead: a missing filter is an *empty list*,
not an error message.

## The key scheme

The session speaks Omarchy's keys with `Alt` in place of `Super`, so an action keeps the
same letter and the same Ctrl/Shift combination wherever a sway default does not already
own it. The whole set is the *Shortcuts* page of the manual
(`usr/share/doc/lazyos/03-shortcuts.md`) and, generated from the configuration,
`rofi-keys`; this is the shape of it:

| Group | Keys | Where it lives |
|---|---|---|
| launcher, switcher, commands | `Alt+D` and the Search key, `Alt+Tab`, `Alt+Shift+D` | `19-launcher.conf`, `home/.config/sway/config` |
| file search | `Alt+Search` (`rofi-files --search`, a list), `Search+Shift` (`lazyos-find`, a terminal filter) | `home/.config/sway/config` |
| root menu | `Alt+Space` (Omarchy: `Super+Space`) | `24-keys.conf` |
| panels | `Alt+Ctrl+A/B/W` → audio, Bluetooth, network (Omarchy: `Super+Ctrl+A/B/W`) | `24-keys.conf` |
| system | `Alt+Escape` and `Alt+Ctrl+P` power menu (`rofi-power`), `Alt+Ctrl+L` lock, `Alt+Shift+Escape` lock+sleep, `Alt+Ctrl+Escape` sleep | `21-lock-screen.conf`, `22-sleep.conf`, `24-keys.conf` |
| browser | `Alt+Shift+Return` opens `firefox-esr` | `19-launcher.conf` |
| windows | `Alt+Q` close, `Alt+T` float, `Alt+F` fullscreen, `Alt+grave` scratchpad, `Alt+R` resize | `config`, `24-keys.conf` |
| views | `Alt+Ctrl+K` every binding, `Alt+Ctrl+X` dictation, `Alt+Ctrl+Shift+A` agents | `24-keys.conf` |
| files | `Alt+Ctrl+F` (`rofi-files`), `Alt+Ctrl+Shift+F` (favnyr, the window) | `24-keys.conf` |

The three places where the letter differs are deviations with a reason, and each is written
next to the binding: close is `Alt+Q` because `Alt+W` is sway's tabbed layout, the key view is
`Alt+Ctrl+K` because `Alt+K` is vim focus up and `Alt+Shift+K` moves the window, and there is no "cycle layout" key because
sway's layouts are stacking/tabbed/split rather than Hyprland's dwindle/master/scrolling.
The panel keys run the very scripts the bar's indicators run (`bar-audio`, `bar-bt`,
`bar-net`), so a panel has one implementation and two ways to reach it. The power menu is the
one exception: the key opens `rofi-power` (actions, and a yes/no on the three that cannot be
undone) while the bar's battery indicator still opens `bar-power` (numbers), because those are
two different questions and one script answering the wrong one is what made both keys look dead.

![The key view: categories, keys and the description from the comment above each binding](../../docs/screenshots/hayato-r1-keys.png)

**Every binding carries its description on a comment line above**, and the description
covers the bindings below it until a blank line or another comment, because `rofi-keys` reads
those comments as the text it shows; a binding without one appears as its raw command in the
key view. The `Alt+minus`/`Alt+Shift+minus` pair stays as the sway-default alias of the two
scratchpad actions, and sway's `focus mode_toggle` was dropped from `Alt+Space` in favour of
the menu (floating itself is still `Alt+Shift+Space`).

## The manual, the key view, dictation and hibernate

Four features that exist because rofi and the terminal can carry them without a resident process;
`usr/share/doc/lazyos/` holds the manual they are documented in.

**Learn** (`Alt+Space` root menu → Learn, or the launcher entry) lists the pages in
`/usr/share/doc/lazyos/` and opens the chosen one with **glow** inside foot, so the markdown is
*rendered* (headings, tables, inline code, lists) instead of shown as source, in the palette of
`~/.config/glow/solarized.json`. The first row is a search: ripgrep over every page, one row per
match, Enter opens the page that contains it. There is no browser in the image, and a terminal page
costs 25 MB while it is open.

![A manual page rendered by glow in foot](../../docs/screenshots/hayato-r1-manual.png)

`glow` is `packages/tools.list` (a single binary, no runtime); its style is a glamour description,
which is why it lives next to `glow.yml` and not in the rofi theme - the two formats are unrelated.
`glow.yml` has to name the style by **absolute path**: a bare name is looked up among glow's
built-ins, and `-s solarized` fails with *specified style does not exist*. Every colour in that
JSON carries its `#`, because glamour reads a bare `839496` as a 256-colour index, emits
`38;5;839496`, and the terminal then falls back to its own colours with no error - the page losing
its palette, inline code included, while still "rendering".

**Keybindings** (`Alt+Ctrl+K`, matching Omarchy's `Super+K`) is generated, not written: `rofi-keys`
parses every `bindsym`/`bindcode`/`bindswitch`/`bindgesture` line from `/etc/sway/config`,
`/etc/sway/config.d/*.conf` and `~/.config/sway/config`, expands the `set $var` definitions, and
uses the comment above each binding as its description (the files are written that way). It is shown
with `rofi -dmenu -markup-rows`, so keys are bold and descriptions are dim; Enter copies the command
to the clipboard. A binding added anywhere shows up on the next run, which is the point: a separate
list would drift the first time someone edits the configuration.

**Dictation** (`Alt+Ctrl+X`, matching Omarchy's `Super+Ctrl+X`) is `parecord` at 16 kHz mono while
it records, then `whisper-cli` for the transcription, then `wtype` into the focused window (and the
clipboard as a fallback). The level is normalised with `ffmpeg -af dynaudnorm` before whisper: the
internal microphone records quietly and whisper is sensitive to that. The model is fetched on first
use by `dictate-model` - 78 MB for the
default `ggml-tiny.en.bin`, 148 MB for `base.en` - verified against a pinned SHA256 and kept in
`~/.local/share/whisper/`. Recording stops itself after 60 seconds and the state is a pid file in
`$XDG_RUNTIME_DIR`, so nothing survives a crash. Both silent phases report through `busy`: the
notification says it is recording ("Alt+Ctrl+X stops it"), becomes "Transcribing…" and ends as the
text it typed or the reason it failed, and the bar shows its spinner. Pressing the key again - or
the root menu's Dictation row - stops it (SIGINT, which is what finalises the wav). Omarchy uses
voxtype (a Rust daemon with a state file); this is the same interaction without the daemon.

**Agents** (`Alt+Ctrl+Shift+A`, Omarchy's `Super+Shift+Ctrl+A`) is `rofi-agents`: it lists the
coding-agent CLIs it knows (Claude Code, Codex, OpenCode, Gemini, Copilot), launches the installed
one in foot from `~/Work`, offers `npm install -g --prefix ~/.local <package>` for the rest (so npm
never wants root), and keeps **accounts** for the two that support an alternative config directory -
Claude Code (`CLAUDE_CONFIG_DIR`) and Codex (`CODEX_HOME`) - as one directory per account under
`~/.local/share/lazyos/agents/`. **OpenCode is the one the image ships** (`tools/opencode/` pins the
arm64 tarball; `scripts/rootfs-setup.sh` installs it into `~/.opencode/bin`), because it is the agent
this port is built with and a fresh card should not have to fetch it first: that layout is the one
its own `opencode upgrade` recognises and rewrites without root, and the image ships it signed out.
The root menu's AI group opens with **Agent session** rather than with the list: that row runs
`rofi-agents --default`, which starts the agent marked `●` (OpenCode until *Set the default agent…*
names another) in `~/Work`, falls back to OpenCode when the recorded default is not installed, and
raises a card instead of an empty terminal when no agent CLI is present - so the section is an
action and a list, and the common case costs one click.
Omarchy's usage meters are deliberately absent: they read provider
APIs that change shape, and a broken meter is worse than none; if they return, the fitting shape is a
`python3` script printing one line for a waybar `custom` module, not a daemon.

**Hibernate** is only offered when it can resume: `/sys/power/state` must contain `disk`, a swap on
disk (not zram) must be at least ~2/5 of RAM, and the kernel command line must carry `resume=`
(`resume=LABEL=lazyos_swap`, resolved through udev, so a labelled SD swap partition is enough). Each
missing condition gets its own message; it never hibernates into a swap the machine cannot come back
from. The default image has no disk swap, but **Auxiliary swap** (Places and the root menu) prepares
the partition and the fstab entry that activates it on every boot; it does not add `resume=`, which
is a kernel command line change and a rebuilt kpart.

## The bar

The bar is the Deboma (Omarchy) bar, ported: the same left-to-right reading (menu, workspaces,
clock, **busy**, updates, tray, layout, bluetooth, network, volume, brightness, battery) and the same
interactions.  The busy card only exists while an action runs (see *Something is happening*); it is
one shell per second while it is on screen, and a zero-width module the rest of the time.  What differs is what happens on a click.  Deboma answers with a Quickshell panel, and
the shell process that draws those panels costs **361 MB** of PSS: more than half of the whole
Deboma session and several times a booted LazyOS.  Here every click runs a short-lived process
instead, and nothing survives the click:

| Indicator | Left click | Right click | Scroll |
|---|---|---|---|
| menu | root menu (terminal, Wi-Fi, Bluetooth, audio output, dictation, capture, lock, suspend, the power menu) | terminal | – |
| workspaces | focus / move (sway) | – | disabled |
| clock | switch format (date + time / seconds) | – | previous / next month (hover shows the month grid) |
| updates | apt updater in a floating terminal (the module only exists while something is pending) | – | – |
| tray | application StatusNotifier items | – | – |
| layout | next xkb layout (the module hides itself when only one layout is configured) | – | – |
| bluetooth | device list from a 3 s scan; picking one connects it, picking a connected one disconnects it | `bluetoothctl` in a floating terminal | – |
| network | Wi-Fi list, strongest first, the active one marked `*`; a new secured network asks for its password in a masked prompt | `nmtui` in a floating terminal | – |
| volume | mute / unmute | output list (speakers, headphones, a Bluetooth headset): switches the default and moves the playing streams | volume, 5% steps |
| brightness | – | – | backlight, 5% steps |
| battery | power panel: charge, state, draw, time left, charger, uptime, memory, swap, kernel | root menu | – |

`rofi` in dmenu mode draws the menus - the same binary that is the launcher - `foot
--app-id=popup` (floated by `etc/sway/config.d/25-bar-panels.conf`) runs what needs a real
terminal, `nmtui` and `bluetoothctl`; `swaynag` shows the power panel.  All of those are already
in the image for other reasons, so the bar needed no package of its own once rofi replaced the
launcher.  The bar is resident; the only other thing that stays is `workstyle`, which renames the
workspaces it displays (see below).

The scripts are POSIX `sh`, they live in `home/.config/bin/`, and they share `bar-pick`, which holds
the look of every popup through `~/.config/rofi/solarized.rasi`.  Each one
documents at the top of the file which Deboma widget it reproduces.  Icons are written in
`config.jsonc` as `\uXXXX` escapes so the codepoint is readable and survives any editor; they are
Font Awesome (see the font table below).

### Workspace icons

Each pill is the workspace number plus one glyph per *kind* of window in it - **3: 🖥 🌐** - so
"where did I leave that window" is answered by the bar. The renaming is **workstyle**'s
(`tools/workstyle/`, built from its pinned crate because Debian packages no workspace renamer and
Nura's binary was linked against musl): the session's second resident process after the clipboard
watcher, and for the same reason - only something watching can say which windows are where. It
costs about 1 MB (2.4 MB of RSS, 4.7 MB at peak) and no CPU while idle, and it re-reads its config on every workspace or window
event, so an edit lands on the next switch.

`config.jsonc` draws `"{value}"` - the whole name. `{name}` is only the part after the colon, so it
would drop the number and leave the glyphs; `{icon}` behaves the same. `persistent-workspaces` is
deliberately gone: this module matches that list by workspace *name*, so a renamed workspace would
get a duplicate pill beside it (and in the `"*": [1, 2, 3, 4, 5]` shape it carried before - the
*Hyprland* module's format - it drew nothing at all).

`home/.config/workstyle/config.toml` is the table: `"pattern" = "\uXXXX"` per application, the
glyph being a `UI_I_*` codepoint from `lib-rofi.sh` so the pills use the same alphabet as the
menus. Patterns match a case-insensitive substring of the title, then the app_id, then the class,
and the first match in the file wins - `firefox` sits above `foot` so a browser tab titled after a
terminal is still a browser, and the foot windows with their own app_id (`popup`, `lazyos-find`,
`lazyos-welcome`) sit above it too. `deduplicate_icons = true` makes three terminals one glyph, and
a window nobody mapped draws a plain window outline instead of nothing.

Renaming is what breaks anything that addresses a workspace by *name*, so the configuration says
`workspace number 3` everywhere and `check-system.sh` fails on a line that does not. The `swaymsg
-t get_workspaces` name carries the glyphs; `num` is the field to read.

### Refreshing

waybar polls `custom` modules on an `interval`, so a click that changes something would stay
invisible until the next tick.  Every bar script therefore ends with `pkill -RTMIN+8 waybar` and
every `custom` module declares `"signal": 8`, so one signal re-runs all of them at once.  The
intervals are long on purpose (`bar-updates` every 6 hours, Bluetooth every 30 s, the layout every
10 s) to keep the fork count at zero in the steady state; the signal covers the interactive case.

Two modules collapse to zero width when they have nothing to say, using the CSS class the script
prints in its JSON (`"class": "empty"`, matched by `#custom-updates.empty` in `style.css`): no
upgrades pending, and a single keyboard layout.  Without that they would leave an empty card on the
bar.

### What was left out of the Deboma bar

* **weather**, **elsewhen** (time until the next calendar event), **indicators** and **agents**:
  none of them is a control, and each one is another process or network call.
* **Power profiles** in the power panel need `power-profiles-daemon`, a resident daemon and a
  D-Bus service for a setting that is not exposed through sysfs on this SoC.  The panel shows the
  readings instead; add the package to `packages.list` if the profiles matter more than the memory.
* **The calendar opens on hover, not on click.**  waybar's clock shows `{calendar}` in its
  tooltip, which costs no process; a click switches the clock format instead.
* **No CPU / memory / temperature / disk modules.**  The Deboma bar does not show them and this port
  follows that layout.  They are one line each in `config.jsonc` (plus their card in `style.css`)
  if they are wanted back.

`nmcli`, `bluetoothctl`, `pactl` and `brightnessctl` come with packages that were already listed
(`network-manager`, `bluez`, `pulseaudio-utils`); the additions are `rofi` (also the launcher and
the window switcher) and `jq` (which parses `swaymsg -t get_inputs` for the layout indicator).  `scripts/check-system.sh` verifies all of
it on the device: every bar script executable, those tools present, and the backlight device named
in `config.jsonc` existing under `/sys/class/backlight` (a wrong name leaves waybar's module empty
with no error anywhere).

## Notifications

Deboma's notifications are a Quickshell panel; here they are **mako** — the daemon sway starts from
`config.d/20-notifications.conf`, themed in `home/.config/mako/config` with the bar's palette (base03
background, base1 text, cyan border, 8 px radius, hanging 40 px from the top so it never covers the
30 px bar). `group-by=app-name` and `sort=-time` keep the stack readable, critical notifications stay
until dismissed and turn red, and icons are capped at 48 px.

Debian's package is **`mako-notifier`**, not `mako`: the name `mako` belongs to a Python template
library, while the binary is still `/usr/bin/mako`. The config parses with `mako -c <file>` — a bad
key prints `Failed to parse option '<key>'`, a good file goes on to fail on acquiring the bus name
when another daemon is already running.

`check-system.sh` asks the session bus for `org.freedesktop.Notifications`: a daemon that died
leaves every application's notifications silently lost, with nothing in the journal.

## The idle screen

Deboma runs a screensaver 150 s after the last input and locks at 300 s
(`shell.json`: `"idle": {"screensaver": 150, "lock": 300}`). The same chain runs here on `swayidle`
(`config.d/23-lazyos-screensaver.conf`), with two differences:

* **The loop runs at 30 fps, not 120.** Deboma's `omarchy-screensaver` loops `ttfx` at 120 fps;
  measured here, that is 78 MB of Python plus a full 1920x1200 repaint 120 times a second on the
  Mali-G57, for as long as the screen is idle. `lazyos-screensaver` loops Debian's `tte` with
  `--reuse-canvas` over a curated list (`decrypt middleout print sweep wipe slide expand`), so the
  wordmark never blinks away between effects. One full cycle of those seven effects measures 70 s of
  wall time and 8.5 s of CPU — about **12 % of one core**, peaking at 49 MB in `tte` on top of foot's
  ~25 MB, and nothing at all once the screen ends. The same cycle at 120 fps takes 20 s and ~6.4 s of
  CPU, i.e. ~32 % of a core, for an animation three times faster than a screensaver needs.
* **No per-monitor launcher.** Deboma's launcher walks `hyprctl monitors`, opens a terminal per
  monitor in a special workspace and listens on Hyprland's socket; here the session has one output
  and the window rule in `23-lazyos-screensaver.conf` fullscreens
  `[app_id="org.lazyos.screensaver"]` on the current workspace.

The art is `~/.config/lazyos/screensaver.txt`: the **z_** mark in block characters, `lazyOS_`,
`:: lazy by design ::` and the arcade line `» press any key to play «`, drawn in the brand's cyan
(`foreground=2aa198` in `screensaver.ini`, and its own SGR in the static frame, which also turns
`play` cursor orange after centring). The
art, the fastfetch logo, the wallpaper and the launcher icon all come from `scripts/make-brand.sh`;
`brand/README.md` has the rules.

Any key, or any movement of the pointer, dismisses it (`read -rsn1 -t 1` polls while the effect runs,
so the dismissal does not wait for the effect to end; `tte` gets stdin on `/dev/null`, or it would
swallow the key). The pointer needs the terminal's own mouse reporting (`ESC [ ? 1003 h` with the SGR
encoding `ESC [ ? 1006 h`, turned off on the way out): a pty hears nothing about the mouse unless the
program inside asks, which is why the idle screen used to answer keys only. The launcher also returns
as soon as the window is up, because `swayidle` runs the chain with `-w` and waits for each timed
command to exit — a foreground saver would keep the 300 s lock and the 360 s panel-off from ever
running. Three details cost a round each and are commented at the line that does them: a terminal
allocates an 80x24 pty and resizes it only after the window is mapped, so `tte` has to wait for `stty
size` to change; `tte`'s **stdout** is the animation (the first version sent it to `/dev/null` and
the screen stayed black); and the art is measured in characters with `LC_ALL=C awk` after stripping
UTF-8 continuation bytes, which is the same answer in mawk (the image) and gawk (a build host).

**The lock is `~/.config/bin/sway-lock`.** The idle timer, `Alt+Ctrl+L`, the Lock key and the two
suspend shortcuts call it instead of `swaylock`: it stops the idle screen first (a lock put up with
the shortcut at 100 s used to leave the 150 s timer free to start the animation underneath it, where
it burned CPU invisibly until the 300 s timer), refuses to stack a second surface,
puts the session's own wallpaper on base03 (`-i …/mountains.jpg -s fill -c 002b36`), and waits for
swaylock's **readiness line** (`-R, --ready-fd`, written once the compositor guarantees that nothing
sensitive is visible) through a FIFO — 2 s for an ordinary lock, 4 s for `before-sleep`, inside
logind's 5 s inhibitor budget. If swaylock does not come up at all, a critical notification says so.

The bar menu has a **Screensaver** entry that starts it on demand, and `check-system.sh` FAILs when
`tte`, `swayidle`, the art, the launcher, `sway-lock` or `screensaver.ini` is missing, reads the
drop-in for the 150 s / 300 s / `before-sleep` lines, and WARNs when an idle screen is found running
under `swaylock`.

## The CLI tools

Deboma ships an opinionated terminal and this port keeps it: the tools are listed in
`packages/tools.list` (a third list next to `packages/base.list` and `uis/sway/packages.list`,
installed by `rootfs-setup.sh`) and wired by two files — `etc/profile.d/90-cli-tools.sh` for the
environment and `home/.bashrc` for the interactive shell.

| Instead of | Tool | Notes |
|---|---|---|
| `ls` | **eza** | `ls -lh --group-directories-first --icons=auto`, plus `lsa`, `lt`, `lta`. |
| `cat` | **bat** | `--style=plain --paging=never`. Debian's binary is `batcat`; `rootfs-setup.sh` adds `/usr/local/bin/bat`. |
| `find` | **fd** | Debian's binary is `fdfind`, same symlink. It is also `FZF_DEFAULT_COMMAND`. |
| `grep` | **ripgrep** | Not aliased: `rg` is not a drop-in for every `grep` flag, and a quietly different grep is a trap. |
| `cd` | **zoxide** | A `cd` wrapper tries the real path first, so only non-paths jump by frecency. |
| the prompt | **starship** | The powerline prompt in `~/.config/starship.toml`, recoloured to the session palette. |
| `top` | **btop** | |
| `man` pager | **bat** | `MANPAGER='sh -c "col -bx | bat -l man -p"'`; `bsdextrautils` supplies `col`, `man-db` supplies `man` itself. |

Also present: `fzf` (key bindings only — its completion wrappers cost ~25 ms per shell start), `tldr`
(`tealdeer`), `fastfetch`, `inxi`, `tmux`, `lazygit`, `gum`, `yt-dlp`, `ffmpeg`, `curl`, `unzip`,
`whois`, `socat`, `inotify-tools`, `qrencode`, `zbar-tools`, `tesseract-ocr` and `nodejs`/`npm`.

**`/etc/yt-dlp.conf` names Node as yt-dlp's JavaScript runtime.** yt-dlp 2026 needs one to solve
YouTube's JS challenges and enables only `deno` by default, which Debian does not ship, so a stock
install logs `JS runtimes: none` and warns that *some formats may be missing*. `nodejs` is already
installed (for npm), so the config only has to say `--js-runtimes node`. Verify with
`yt-dlp -v --simulate <url>`: the debug line becomes `JS runtimes: node-24.21.0`.

`ff` and `eff` are the fzf helpers: `ff` previews with bat, `eff` opens the pick. Nothing is a
service or a daemon — `plocate` is deliberately absent because it is `locate` *plus*
`plocate-updatedb.timer`, and `tree-sitter-cli`/`lua5.4`/`wtype`/`gawk` are absent because they only
feed Omarchy's own components, which this port does not have. The full rationale is in
`packages/tools.list`.

**The prompt's glyphs come from fonts `foot.ini` names, in order.** `font=` is also the icon
policy: foot searches the listed fonts and draws the first one that has the glyph *at that font's
size*, and only then asks fontconfig's dynamic list. So the chain is JetBrains Mono (the cell),
then `Symbols Nerd Font Mono` (the DFSG-stripped Nerd Fonts set: eza's icons, fastfetch's logo, the
starship branch/clock/character glyphs), then `Font Awesome 6 Free Solid` and `Material Design
Icons version 6` for what that set leaves out (the prompt's folder substitutions), with the three
icon fonts one point larger than the text because otherwise they sit short of the line. The
prompt's separators are the triangles `JetBrains Mono` draws itself for the same reason: a glyph
from the primary font always tiles with the cell, and the rounded caps from the symbol font do not
(it is `~/.config/starship.toml` that asks for triangles). `check-system.sh` asks fontconfig, per
codepoint of the prompt, whether the font that draws it is one of these, and FAILs when no font has
it at all. waybar needs none of this: it is a Pango program.

## Deviations from the Nura original

* **Fonts.** The theme asks for `AdwaitaMono Nerd Font` / `JetBrainsMono Nerd Font`, which Debian
  does not ship (its `fonts-adwaita` has no Mono variant), so `style.css` and the Pango spans in
  `config.jsonc` use `JetBrains Mono`. The icons come from fontconfig fallback and need two
  packages people usually miss:

  | Glyphs | Package |
  |---|---|
  | Font Awesome, `U+F000`-`U+F2FF` | `fonts-font-awesome-legacy` |
  | Material Design Icons, plane 15 | `fonts-materialdesignicons-legacy` (the *older* MDI numbering: it has `U+F0219`, not the Nerd Fonts v3 `nf-md` set) |
  | Octicons, Codicons, Devicons, Powerline | `fonts-nerd-symbols` |

  `fonts-nerd-symbols` alone is not enough: Debian stripped Font Awesome and Material Design
  Icons from it for DFSG reasons, so every waybar icon would render as a tofu box with the hex
  codepoint inside. Nothing else in the configuration changes.
* **workstyle renames the workspaces, and is built here.** Debian packages no workspace
  renamer and Nura's binary was linked against musl (it cannot run against glibc), so
  `tools/workstyle/` pins the 0.8.2 crate, `scripts/build-workstyle.sh` builds it into a `.deb`
  and `scripts/install-workstyle.sh` installs it (`scripts/build.sh` does both). The session
  starts it from `etc/sway/config.d/02-workstyle.conf`, the bar draws the renamed name with
  `"format": "{value}"`, and `home/.config/workstyle/config.toml` is the icon table - one
  `UI_I_*` codepoint per app, written as a `\uXXXX` escape. See **Workspace icons** (under *The
  bar*) and `tools/workstyle/README.md`.

* **Bluetooth needs `bluez`, not just the PipeWire plugin.** `libspa-0.2-bluetooth` only gives
  PipeWire a D-Bus client for `org.bluez`; the daemon that actually powers the controller up
  (and provides `bluetoothctl`) is in `bluez`, which is now listed next to it in
  `packages.list` (see `AGENTS.md`).

* **ALSA-only programs need `pipewire-alsa`.** Debian's alsa-lib configuration has no usable
  `default` PCM by itself, so `aplay`/`arecord` without an explicit `-D`, emulators and some
  games fail with *audio open error: No such file or directory* even though PipeWire (and
  therefore PulseAudio clients) works. `pipewire-alsa` is listed in `packages.list`.

* `etc/sway/config.d/01-status-bar.conf` disables swaybar (waybar is used
  instead); the block is kept commented out for reference.

## Brightness, volume and lock keys

The top row is handled by sway: `XF86MonBrightness{Down,Up}` run `brightnessctl`
(`etc/sway/config.d/20-brightness.conf`), `XF86Audio{Raise,Lower}Volume` /
`XF86AudioMute` run `pactl` (`etc/sway/config.d/21-volume.conf`), and the Lock key locks
the screen (`etc/sway/config.d/21-lock-screen.conf`). Some of the pieces are
easy to miss:

* **Backlight permissions.** Debian's `brightnessctl` ships no udev rule and nothing
  else in the system touches `/sys/class/backlight`, so the brightness file stays
  root-only (0644) and `brightnessctl set` fails with *Permission denied* for the
  session user. `etc/udev/rules.d/90-brightnessctl.rules` puts it in the `video` group
  and makes it group-writable (this is the rule upstream brightnessctl installs).
* **A real audio sink.** `pactl` acts on `@DEFAULT_SINK@`. If the MT8192 card's UCM boot
  sequence has not run, the card exposes no sink and WirePlumber falls back to a *Dummy
  Output*, so the volume keys "work" but nothing is audible. The `alsa-utils` package in
  `packages/base.list` fixes this (see `AGENTS.md`).
* **The Lock key sleeps by itself.** The right-most key of the top row emits `KEY_SLEEP`, the
  one key systemd-logind suspends on without consulting polkit or even the session, so
  `etc/keyd/default.conf` deliberately leaves it alone (Nura's keymap maps `sleep = coffee`
  instead, which swallows the key: logind stops seeing `KEY_SLEEP` and nothing consumes
  `KEY_COFFEE`, so the key is dead there — and that remap is what killed suspend on this image
  as soon as keyd was configured). `21-lock-screen.conf` binds `XF86Sleep`, the keysym that
  keycode produces, to `swaylock`: the key locks the screen and the machine sleeps ~0.3 s
  later, with no shortcut and no polkit in the path.
* **Locking and sleeping from the session** is what the two Escape shortcuts in `22-sleep.conf`
  are for — `Alt+Shift+Escape` locks first, `Alt+Ctrl+Escape` only sleeps. Being plain
  commands, they call `loginctl suspend`, a D-Bus method that needs polkitd to allow the active
  session (Debian's stock `org.freedesktop.login1.suspend` policy has `allow_active=yes`) and
  that otherwise fails with *Access denied* — silently. Both go through
  `~/.config/bin/sway-suspend`, which tries `loginctl suspend`, then `sudo -n loginctl suspend`,
  then `echo mem > /sys/power/state`, appends every attempt to `~/.cache/sway-suspend.log` and
  shows a swaynag pointing at that file when all three fail.
* **tuigreet is the original 0.9.1**, not the fork Nura runs, so it has no `--config`: the
  greeter's personalisation lives as flags in `/etc/greetd/config.toml` (theme, greeting,
  time format, padding, F-key menus). The `matrix` background, the custom title and the
  widget positions have no equivalent and are dropped.

## Virtual terminals and the missing F-keys

The hayato device tree (`mt8192-asurada-hayato-r1.dts`) maps the top row to
*action* keys (`KEY_BACK`, `KEY_FORWARD`, `KEY_REFRESH`, `KEY_FULL_SCREEN`,
`KEY_SCALE`, `KEY_BRIGHTNESS{DOWN,UP}`, `KEY_MUTE`, `KEY_VOLUME{DOWN,UP}`), so
this keyboard never emits `F1`-`F10`. That breaks virtual-terminal switching,
which needs a real F-key keycode:

* the kernel maps `Ctrl+Alt+F<n>` to `Console_<n>` (its `defkeymap` has
  `control alt keycode 59 = Console_1`), and
* sway reacts to the `XF86Switch_VT_<n>` keysym, which the default xkb keymap
  produces from that same keycode (`srvr_ctrl(fkey2vt)` puts `XF86Switch_VT_1`
  on `<FK01>` at the `CTRL+ALT` level).

`keyd` supplies the missing F-keys. `etc/keyd/default.conf` is the maintainer's
Nura keymap, moved out of `/tmp` (where it was a symlink target that vanished on
reboot: keyd only reads its configuration at startup, so a config written after
boot is never picked up).

| Combination | Result |
|---|---|
| `Ctrl+Alt` + top row, left to right | `Ctrl+Alt+F1`-`F10` → virtual terminals 1-10 |
| `Search` + top row | `F1`-`F10` |
| top row alone | ChromeOS action keys (back, brightness, volume, ...) |
| `Alt+Backspace` | `Delete` |
| `Ctrl+Scale` / `Ctrl+F5` | `SysRq` |
| `AltGr` + arrows | `Home` / `End` / `PgUp` / `PgDn` |
| Lock key | *not* remapped: systemd-logind suspends on `KEY_SLEEP` |

### Which devices keyd grabs

`[ids]` lists the internal cros-ec keyboard (`k:0000:0000`; keyd matches ids as
prefixes) plus the Google USB keyboards of the original config, and it *excludes*
two devices that share the `0000:0000` id but are not keyboards:

* `cros_ec_buttons` — it carries the power button, the volume buttons and the
  lid and tablet-mode switches. keyd's heuristic counts it as a keyboard because
  it reports `KEY_VOLUMEUP/DOWN`, but keyd's virtual device does not forward
  `EV_SW`, so grabbing it would break suspend-on-lid-close and take the power
  button away from logind.
* the headset jack — same problem with the headphone/microphone insertion
  switches.

Exclusions have to come *before* the `k:0000:0000` line (keyd returns on the
first matching entry) and hold a hash of the device name, key count and abs/rel
masks. If a kernel update ever changes either device, read the ids back with
`keyd.rvaiya monitor`, or with `journalctl -u keyd`, which logs what it matched.

Notes:

* Debian's package installs the daemon as `/usr/bin/keyd.rvaiya` and enables
  `keyd.service` on install; `/etc/keyd/` ships empty, so this file is the only
  config. Reload it with `sudo keyd.rvaiya reload` after editing.
* keyd grabs the keyboard and re-emits everything through a virtual device
  (`kbd_connect()` in the kernel accepts uinput devices, so VT switching works).
  If it is not running the keyboard still works, just without the remapping.
* Emergency exit if a config misbehaves: **`backspace`+`escape`+`enter`** makes
  keyd quit and release the keyboard.
* Without keyd a console is still reachable from a terminal with `chvt 2`
  (`chvt` needs root or a controlling tty on the active VT), and you come back
  with `chvt 7`.

## Tablet mode

The keyboard folds all the way back and the machine is then a tablet, which the
kernel already reports: `cros_ec_buttons` carries `SW_LID` and
`SW_TABLET_MODE` (`SW=3`), and sway has `bindswitch`/`unbindswitch` for exactly
that. `etc/sway/config.d/28-tablet.conf` binds the two states to
`~/.config/bin/tablet-mode on|off`, which:

| Step | What it does | Why |
|---|---|---|
| keyboard | `wvkbd-mobintl`, solarized, `JetBrains Mono`, 300 px portrait / 240 px landscape, radius 8, alpha 242 | the panel is the only input once the keyboard is folded |
| physical keyboard | `swaymsg input type:keyboard events disabled` (re-enabled by `off`) | the folded keys are under the screen, and a keystroke there is a keystroke. It is switched off **only after the on-screen keyboard is confirmed alive**: from that moment the touchscreen is the only input, so a wvkbd that failed must leave the physical keyboard on. The Lock key still suspends, because logind reads `/dev/input` itself and the switch is a separate device |
| touchpad | `swaymsg input type:touchpad events disabled` | it is on the back of the screen now, and pressure on it is input nobody meant |
| rotation | a 1 Hz poller over `cros-ec-accel`'s `in_accel_{x,y,z}_raw` applying `swaymsg output <focused> transform <t>` | the accelerometer is in the kernel; no package, no D-Bus service |

`off` undoes the three in reverse (keyboard gone, both inputs back, `transform
normal`), and `status` prints `on · screen <transform>` or `off` - which is what
the Settings row shows. `toggle` is the Settings row itself, for a mode that did
not follow the switch. `off` also restores the inputs when the state directory is
gone, which is the path a person cannot reach without a mouse.

The mapping is a table in `orient()`, and **the card's first report said the screen
came up upside down**: the accelerometer in this panel sits 180° around from the
frame the first table assumed, so an upright device reads `-Y`, not `+Y`, and the
four in-plane rules are the mirror image of the assumption (the flat rule is
unchanged). Screen-up gives `+Z`:

| Device | Reading | `transform` |
|---|---|---|
| flat, screen up | `+Z` | `normal` |
| upright, landscape (facing you) | `-Y` | `normal` |
| upside down | `+Y` | `180` |
| turned clockwise | `+X` | `90` |
| turned counter-clockwise | `-X` | `270` |
| no axis dominant (in the hand, mid-turn) | - | keep the current one |

The thresholds are `0.85` for the flat check and `0.75` for an axis (about 41°
off vertical), so tilting the device does not flip the screen; only a real turn
does. **A sign in a table is a guess until it is checked on a unit**, which is why
there is a calibration instead of a hardcoded frame: `tablet-mode calibrate`
(Settings → **Calibrate rotation**, only listed while the mode is on) reads the
accelerometer in the pose the person is holding - upright, screen facing them -
and writes the direction that must look upright to
`~/.config/lazyos/tablet.conf`, one line, `normal=y-`. `orient()` reads that file on
every pass and derives all four positions from the one reference: the readings run
around the device in the counter-clockwise order `+X, +Y, -X, -Y`, so each 90° the
reading turns is 90° of transform, the reference getting `normal` and the rest
90/180/270 in that order. The file is **optional** (no file means the table
above), a malformed one is ignored silently (`orient()` runs once a second, where a
notification would be the wrong answer), and deleting it is the way back.
`tablet-mode calibration` prints which mapping is in use, `calibrate` refuses to
store anything while the device is flat or halfway between two positions (both say
"hold the screen upright"), and it applies `normal` at once, because the pose that
was just calibrated is the one that has to look upright.

Nothing about this is resident in laptop mode: the state is a directory in
`$XDG_RUNTIME_DIR/lazyos-tablet` holding the two pids and the output name, so a
session that dies takes the keyboard and the poller with it, and the next
`tablet:off` still restores the touchpad (`off` runs its undo even when the
state is gone). `wvkbd` 0.15 from Debian is the version that ships the flags this
uses (`-H`, `-L`, `-R`, `--fn`, the colours, `--alpha`); it has no `--dock` or
`--auto`, which arrived later, and the keyboard is placed by the compositor as a
layer-shell surface at the bottom edge, which is what `--dock` would do.

Settings (`Alt+Ctrl+O`) carries **Tablet mode** next to **Rotation**: the second
is the manual turn (`rofi-quick --rotation-menu`), the first is this script, and
**Calibrate rotation** (only while the mode is on) is the pose the person is
holding. The
on-screen keyboard's layout is the mobile-international one compiled into
`wvkbd-mobintl`, where each key's variants are one `Cmp` away: `Cmp` then `a`
opens `á à â ä …` and the one you tap is inserted (`Cmp` then `?` reaches `¿`).
It is a variant layer, not a dead-key compose sequence, which is why clicking is
part of typing an accent.
