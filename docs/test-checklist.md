# Hardware test checklist

What to try on a booted card, one item per feature that has not run on the
device yet, with the exact behaviour that means it works. `scripts/check-system.sh`
covers everything a script can see; this file is the half only a person can
judge - whether the glyph appears in the cursor, whether the screen turns the
right way, whether the notification showed up in time.

Run it after `check-system.sh`, and write both results to `reports/` (see
`reports/README.md`). A **FAIL** from `check-system.sh` is a regression, not a
question: every one of its checks maps to a bug this port has already hit.

## Before anything else

```sh
cat /etc/debian-aarch64-revision ~/Dev/LazyOS/REVISION ~/Dev/LazyOS/VERSION
/usr/local/sbin/check-system.sh ~/Dev/LazyOS
```

* the revision must be the build you meant to test. The tree at
  `~/Dev/LazyOS` is a snapshot (`git archive HEAD`), not a checkout, so compare
  that hash with `git log --oneline -1` on the machine that wrote the card;
* the snapshot and the system must be one build: `~/Dev/LazyOS/REVISION` carries
  the same lines as `/etc/debian-aarch64-revision` (commit, device, UI, kernel,
  version, date), `~/Dev/LazyOS/VERSION` is the `VERSION_ID` in
  `/etc/os-release` and the version in `/etc/issue`, and on the build host
  `git rev-parse v<version>^{commit}` is the commit `REVISION` names. A
  disagreement means the card was assembled from two builds - stop here;
* `check-system.sh` must end with **0 FAIL**. WARN and SKIP lines are expected
  and explained in `reports/README.md`; anything else stops the run here.

## The first launch - the caches built at build time

Two caches are built inside the image so the first session does not pay for
them. Both are invisible when present and only noticeable as a slow first
terminal or a slow first launcher, which is why they are a step of their own.

| Step | Expected |
|---|---|
| `ls -l /usr/share/icons/LazyOS/icon-theme.cache` | the file exists and is newer than the PNGs beside it (it is built after the theme is overlaid) |
| `ls -l /var/cache/fontconfig \| head` | files, not an empty directory: `fc-cache -f -s` ran in the chroot |
| `time rofi -show drun` (Escape at once) | the list draws without a visible scan of the theme; a card that takes a beat on first open is the cache missing |
| `sudo gtk-update-icon-cache -q -t -f /usr/share/icons/LazyOS` | regenerates the index (this is the command the build runs); `check-system.sh` stops warning |

A missing cache is a `WARN` in `check-system.sh` (the image is still correct,
just slower on first use). `rootfs-setup.sh` is the place both are built; a
missing `gtk-update-icon-cache` in a hand-built chroot is the same warning, not a
broken image. The manual is `05-system.md` § *The first launch*.

## First boot - the welcome card

| Step | Expected |
|---|---|
| log in on a card that has never been logged into | one foot window (`--app-id=lazyos-welcome`) with nine keys: `Alt+Space` menu, `Search`/`Alt+D` launcher, `Alt+Search` file search, `Alt+Ctrl+F` file list, `Alt+Ctrl+T` translate, `Alt+Return` terminal, `Alt+Q` close, `Alt+Escape` power, `Alt+Ctrl+K` bindings. Enter closes it |
| log out and back in | no window: it is once per install, not once per login |
| `cat ~/.local/state/lazyos/welcome` | the content of `/etc/machine-id` |
| delete that file, log out and back in | the window returns |
| `sudo -n rm /etc/machine-id; sudo -n systemd-machine-id-setup` (or a rebuilt card) | a new id, so the window appears again - which is what makes it per install |

A window that never appears on a fresh card is `lazyos-welcome` without its
executable bit (it is started by `config.d/30-welcome.conf`); one that appears on
**every** login is the marker being written before the window could be read
rather than after it closes. `check-system.sh` runs the script against a stub
`foot` in a temporary state directory to prove the marker is not written on
failure.

## Workspace pills

| Step | Expected |
|---|---|
| look at the bar | the pills are the workspace number plus the glyph of what is open there - **1: >_** for a terminal, **2: 🌐** for Firefox - and a workspace with nothing in it is its bare number |
| open a second terminal in the same workspace | the pill keeps **one** terminal glyph (`deduplicate_icons`): one glyph per kind of window, not per window |
| open an app with no line in `~/.config/workstyle/config.toml` (`libreoffice --writer`) | a plain window outline (U+F2D0) in the pill, and **no** workstyle line in `journalctl -b` - an unmapped window is a debug message (see `tools/workstyle/patches/`) |
| `swaymsg -t get_workspaces \| jq -r '.[] \| "\(.num) \(.name)"'` | the name carries the glyphs and `num` stays the number: anything scripted reads `num`, not `name` |
| `swaymsg reload`, then `pgrep -c -x workstyle` | `1` - the lockfile keeps a reload from leaving a second copy, and the extra one exits without a log line |
| `swaymsg exit` from a tty, wait a minute, `pgrep -x workstyle` | nothing: the reconnect loop gives up after 60 tries instead of retrying once a second for the rest of the boot |

A FAIL here means `scripts/check-system.sh` reports it: the binary
(`scripts/install-workstyle.sh`), the `exec_always` line in
`/etc/sway/config.d/02-workstyle.conf`, a workspace binding that names a workspace instead of
`workspace number N`, or a glyph outside the `UI_I_*` alphabet.

## The desktop wallpaper, the About card, the greeter and the console

| Step | Expected |
|---|---|
| look at the desktop | the keyboard wallpaper on base03: one lit key carrying the cyan z_ mark, and `lazyOS_`, `lazy by design` and the arcade line bottom left; the bar, foot, rofi and the menus are still solarized (the brand did not repaint the session) |
| `Alt+Ctrl+L`, then unlock | the **mountain photograph**, not the brand wallpaper - the lock is deliberately the other surface |
| `Alt+d`, type `About LazyOS` | the **About LazyOS** row has the mark as its icon (not a hole); Enter opens the card with the block-character z_ (cyan z, orange cursor) and `lazyOS_` on the left, `OS · LazyOS <version>` among the facts, and the 16-colour palette at the bottom |
| `Alt+d`, type `LazyOS Menu` | the row with the same mark; Enter opens the root menu - the list `Alt+Space` shows, reachable by typing (`org.lazyos.menu.desktop`) |
| `cat /etc/os-release` | `ID=lazyos`, `PRETTY_NAME="LazyOS <version>"`, and `VERSION_ID` equal to the `version` line of `/etc/debian-aarch64-revision` |
| `hostname` | `lazyos` |
| log out to the greeter | the greeting is `LazyOS · lazy by design`, not "Welcome back to Debian" |
| `Ctrl+Alt+F2` (keyd's VT keys), log in on the text console | the banner is `LazyOS <version> · lazy by design` and `lazyos · tty2` |

The launcher's **About LazyOS** and the root menu's **About** open the same card;
the launcher entry is the one that carries `Icon=lazyos`. A flat colour instead of
the keyboard wallpaper is `Pictures/Wallpapers/lazyos.png` missing (or `swaybg`
failing), which `scripts/make-brand.sh` renders from `brand/wallpaper.svg`; a flat
colour instead of the photograph at the lock is `mountains.jpg` missing, and that
file ships in the repository. A hole
where the About row's icon should be is `lazyos.png` missing from
`/usr/share/icons/LazyOS/48x48/apps/`.
Debian's swirl in the About card is `~/.config/fastfetch/config.jsonc` missing.
`check-system.sh` FAILs on the first two, on an `os-release` that is Debian's
again, and on an `org.nureb.*` entry that survived the rename; it WARNs when the
greeter, `/etc/issue` or the fastfetch files lose the brand, because those are a
hand-edit away.

## Night light, and what a suspend does to it

| Step | Expected |
|---|---|
| Settings (`Alt+Ctrl+O`) -> **Night light** at any hour | the screen turns warm at once, not only after sunset: the switch runs `night-light start`, which forces the warm end (`-T 4000 -t 4000`) |
| suspend (`Alt+Shift+Escape`, or close the lid) and wake it | the screen comes back **still warm**. Daylight on the first frame and warm a moment later is the `after-resume` hook restarting `wlsunset`, which is correct |
| `night-light status` after the resume | `on warm` (the mode file survived and `pgrep -x wlsunset` answers) |
| turn the switch off, suspend and resume | daylight, and it stays daylight: with no state file `resume` does nothing |
| log out and back in before sunset | daylight: `login` starts the **sunset schedule**, not the forced warm end. Between sunset and sunrise (the hours wlsunset computes for those coordinates) it ramps to 4000 K with no key pressed |
| `pgrep -x wlsunset` | one process, never two |

The whole cause of the bug this covers: `wlsunset` is a long-running client that
takes the compositor's gamma table **once**, and the panel comes back from a
suspend with its own, so a client that survived the sleep no longer has any
effect on the screen - warm at 22:00, daylight at 22:05. `check-system.sh` FAILs
when `bin/night-light` is missing and WARNs when the idle drop-in loses
`night-light resume`; whether the screen is warm is only the eye's answer.

## An external monitor (USB-C)

The display is a device-tree patch (the DPI output and the second ANX7625) plus
one register write per plug: the bridge has one DP transmitter and two Type-C
ports, and the crosspoint that picks the port is what mainline does not set.
A card that only works on the left port has the udev rule, the helper or
`i2c-tools` missing.

| Step | Expected |
|---|---|
| plug a DisplayPort monitor into the **left** port | the desktop extends onto it; `swaymsg -t get_outputs` names `DP-1` with its mode |
| move the cable to the **right** port, replug twice, then plug it in after login | a picture every time. `sudo i2cget -f -y 7 0x2c 0xb4` reads `0x21` on the left and `0x12` on the right, and `journalctl -t lazyos-dp-mux` is empty |
| with a window on the monitor, suspend (`Alt+Shift+Escape`) and wake it | the monitor comes back **on its own**, showing the lock screen: no replug, no command. `journalctl -t lazyos-dp-resume` has one line - the crosspoint write plus the 720p round trip that retrains the link |
| the same, after the wake | the window is still on the monitor. A window that came back on the internal panel means the hook disabled the output instead of changing its mode |
| Settings (`Alt+Ctrl+O`) -> **Display arrangement** -> **Monitor on the left** | the picture moves at once, `swaymsg -t get_outputs` shows `DP-1` at a negative x, and the Settings row now reads "left of the panel" |
| log out and back in | the monitor is still on the left: `~/.config/sway/config.d/40-display.conf` holds its resolution, scale and position, and `sway -C -c ~/.config/sway/config` accepts the file |
| **Monitor above**, **Mirrored**, then **Only the internal panel** and **Bring the monitor back** | each applies at once; the one-screen rows say they are not remembered and their notification gives the `swaymsg output … enable` command back |
| **Open wdisplays** | one window alone on workspace 11, filling it; closing it and choosing the row again starts it again, not a second copy |
| **Only the monitor** (monitor still plugged in), then `Alt+Ctrl+O` -> **Display arrangement** | the menu draws on the monitor and carries **Bring the internal panel back**; choosing it turns the panel on again |
| **Only the monitor**, then unplug the monitor, then `Alt+Ctrl+Shift+D` | the panel comes back from nothing on screen. This is the state a menu cannot cover - with every output off there is nowhere to draw it - so try the key once on purpose and learn it |
| turn the night light on with the monitor connected | the internal panel warms, the monitor keeps its own colour: the external CRTC has no `GAMMA_LUT` (`modetest -M mediatek -p` shows one on the 1366x912 CRTC only). The monitor's own colour setting is the answer |

`check-system.sh` FAILs on a missing DP connector, helper, udev rule,
`i2cset`, resume hook or modeset step, the arrangement menu, its Settings row,
the `config.d` include or `wdisplays`, and on a crosspoint that does not name
the port whose alt mode is active.

## The idle chain - the screen, the lock, and the panel

Three timers, each with a job the previous one does not do. The last one is the
one a person notices on the battery.

| Step | Expected |
|---|---|
| leave the machine alone for 2½ minutes | the **LazyOS** wordmark starts animating; nothing else changes |
| move the mouse or drag a finger on the trackpad | the wordmark goes away **at once**, without a key, and the 150 s starts over; the pointer is hidden again a moment later |
| tap the touchscreen while the wordmark runs | the same - the tap is a click, and a click ends the screen |
| leave it for five minutes | the session is **locked** (the mountain photograph), and the idle screen is gone (not animating underneath) |
| leave it for six minutes | the **panel goes dark** (`swaymsg "output * power off"`); look at it from the side - the backlight is off, not a black image |
| press a key or touch the touchpad | the panel comes back on **and** the lock is still up: one input, both effects, no second press |
| move the mouse, then type the password | the move only lights the panel; nothing on the lock reads the pointer, so the password is what ends it |
| `pgrep -af swayidle` during all of it | one process, with the 150/300/360 timeouts and both wake lines |
| `pgrep -x swaylock` at five minutes | one process: with `swayidle -w` the rest of the chain only runs if the 150 s command returned |
| play a video in mpv, fullscreen or in a window | no idle screen, no lock, no panel off **while the window is visible** (the mpv rule is `visible`, not `fullscreen`, so a paused film also holds it); close the player and the timers start over |
| fullscreen a YouTube page in Firefox for six minutes | the same: the screen stays awake (the `fullscreen` scope is deliberate - the browser merely open does not inhibit) |
| open a PDF in `papers` and go fullscreen | the same |
| leave a page playing in a Firefox **background** tab | it does **not** inhibit: the screen sleeps. That is deliberate |
| click the **coffee cup** on the bar | it turns yellow and the tooltip says the screen is kept awake; wait past six minutes - no idle screen, no lock, no panel off |
| click it again | it turns dim; the timers run again from the next idle period |
| `systemctl suspend`, then wake | the panel is on at the lock: `after-resume` turned it on (a suspend is not an idle timeout) |

What a failure means:

* **the wordmark ignores the mouse**: `~/.config/bin/lazyos-screensaver` lost its
  `?1003h`/`?1006h` escapes (a terminal reports the pointer only when the
  program asks), so only keys end the screen. `check-system.sh` FAILs on the
  missing enable; the restore (`?1003l`) is the other half, because `--run` in a
  shell would otherwise leave the prompt writing escapes.
* **the session never locks while the wordmark is up**: the 150 s command is
  still staying in the foreground, and `swayidle -w` waits for it, so the 300 s
  lock, the 360 s DPMS and `resume` never run. `check-system.sh` runs the
  launcher against a stub terminal and FAILs when it does not return at once.
  `pgrep -ax swaylock` at five minutes is the check that matters.
* **the mouse does nothing at the lock**: it is not meant to. `swaylock` ignores
  the pointer and the password ends it; a move only turns the panel back on.
* **the lock appears but the panel stays lit**: the `timeout 360` line is missing
  from `/etc/sway/config.d/23-lazyos-screensaver.conf`; `check-system.sh` FAILs
  on it. `swaymsg -t get_outputs` still reports the output as on.
* **the panel stays dark after a key**: the `resume` line (or the
  `after-resume` one for a suspend) is missing; the first is inside the swayidle
  chain and the second is the logind hook. Both are checked.
* **a fullscreen video locks anyway**: the application has no Wayland app id
  (Firefox under Xwayland, `MOZ_ENABLE_WAYLAND=0`), or
  `/etc/sway/config.d/32-idle-inhibit.conf` is missing; `check-system.sh` FAILs
  on the rules. The coffee cup is the manual override for anything else.
* **the coffee cup does nothing**: `idle_inhibitor` is not in waybar's
  `modules-right` (FAIL) or `style.css` lost `#idle_inhibitor.activated` (WARN -
  then it stays dim, because both states draw the same glyph). The module holds
  the inhibitor on the bar's own surface; there is no separate process and no
  `swaymsg` command to check.

The manual is `03-shortcuts.md` § *Panels, lock and power* and `05-system.md`
§ *When you leave it alone*; the symptom list is `06-troubleshooting.md` § *The
screen turns off while you are watching something* and § *The screen stays lit
after the lock*.

## Clipboard history - `Alt+Ctrl+V`

| Step | Expected |
|---|---|
| copy a line in a terminal (`echo hello-paste \| wl-copy`), then `Alt+Ctrl+V` | a row with the text and its kind (`text`) in the dim column |
| Enter on that row, then paste in foot (`Ctrl+Shift+V`) | `hello-paste` |
| copy a region to the clipboard (`Alt+Ctrl+S`), reopen the list | an image row, plus **Save the newest image** |
| Enter on **Save the newest image** | `~/Pictures/clipboard/clip-<date>.<ext>`, opened in **imv** (a picture is the viewer's job now); the notification names the file. A foot window drawing the picture in sixels is `chafa`'s fallback for a GPU with no EGL context, not a failure |
| `Alt+1` on a row | "Entry deleted from the history" and the row is gone when the list reopens |
| last row, **Clear the history** | the list says nothing has been copied yet |

An **empty** list is the watcher, not the menu: `pgrep -fx 'wl-paste --watch cliphist store'`
(`etc/sway/config.d/27-clipboard.conf` starts it once per session). An image
that does not open is `~/.config/bin/lazyos-view` (not executable) or `imv`; a
saved file ending in `.bin` means `file` is missing and the extension could not
be read from the contents.

## Emoji - `Alt+Ctrl+E`

| Step | Expected |
|---|---|
| type `sun` | the list narrows as you type (the row shows the glyph, its name and its group) |
| Enter with foot focused | the glyph is typed at the cursor, **in colour** |
| `Alt+1` on a row | "… copied to the clipboard", nothing typed |
| Enter with an XWayland or Java window focused | the notification says it was copied because the window takes no virtual keyboard |

Tofu boxes instead of glyphs mean `fonts-noto-color-emoji` is missing (a FAIL in
`check-system.sh`); an empty list means `~/.config/emojis.tsv` is missing
(regenerate it with `scripts/make-emojis.sh` on the build host).

## Translate - `Alt+Ctrl+T`

| Step | Expected |
|---|---|
| select a Spanish sentence with the mouse in foot, press `Alt+Ctrl+T` | a `Translating…` card, then `Spanish → English` with the translation; the whole answer is on the clipboard (`Ctrl+Shift+V` in foot) |
| `Alt+Ctrl+T` with nothing selected, after copying a sentence | the clipboard's sentence is the one translated (the clipboard is the second source) |
| select an English sentence | the card says **Already English** and the clipboard is untouched |
| select a long paragraph (half a screen) | the card and a floated `foot` read window with the translation and the selection underneath; Alt+Q closes it |
| **Settings** (`Alt+Ctrl+O`) → **Translate** | the list opens with the current language marked and the how-to rows; pick **Spanish**, Escape, open Settings again, and the row says `into Spanish` |
| select English text and press `Alt+Ctrl+T` | `English → Spanish`, which is the configured direction |
| `cat ~/.config/lazyos/translate.conf` | `target=es` |
| disconnect Wi-Fi, select text, press the key | one critical card, *No answer from the translator: is the network up?*, and nothing on the clipboard |
| press the key, then click the bar's spinner while `Translating…` is up | the card disappears and nothing is reported (a stopped translation is not a failure) |

A card titled `… (MyMemory)` means Google was unreachable and the fallback
answered (the guess is `es` for an English target unless `fallback_source=`
says otherwise). A card that never appears at all is the binding
(`/etc/sway/config.d/24-keys.conf`) or the script's executable bit;
`check-system.sh` reports both, and SKIPs its network probe when the card is
offline instead of failing.

## DNS - root menu (`Alt+Space`) -> **DNS**

| Step | Expected |
|---|---|
| open it | the servers in use, marked `in use`, plus **Automatic (the network)** and **Type the servers** |
| choose **Type the servers**, enter `1.1.1.1 9.9.9.9` | the busy card, then a notification; reopening shows them `in use` |
| `nmcli -f ipv4.dns,ipv4.ignore-auto-dns connection show <name>`, and `iw dev wlp1s0 link` for the interface's own name | the same servers; the interface is `wlp1s0` here (Nura calls it `wlan0`) |
| choose **Automatic (the network)** | back to what DHCP hands out |

Changing a connection needs root: without a passwordless `sudo` the menu says so
and changes nothing, which is the intended behaviour.

## The toolbox - `ip`, `ping`, `ss`, `zip`, `strace`, `ncdu`

These answer the questions an image without them answers with `command not
found`, which reads as a broken system instead of a missing package.

| Step | Expected |
|---|---|
| `ip -br addr` | the interfaces and their addresses (`wlp1s0`, `lo`); a missing command is the `iproute2` package |
| `ip route` | a default via the gateway; `ping -c3 1.1.1.1` answers after it |
| `ping -c3 deb.debian.org` | answers: DNS works (the previous row is the link, this one is the resolver) |
| `ss -tunlp` | what is listening - `sshd` on `:22` is the expected one; `127.0.0.1:5900` appears only while screen sharing is on |
| `echo test \| zip -q /tmp/t.zip - && unzip -p /tmp/t.zip` | `test`: `zip` is the writing half of `unzip` |
| `strace -V` and `strace -e trace=openat true` | the version and one traced call; the debugger a device with no gdb reaches for |
| `ncdu /tmp` then `q` | the disk walk; a full eMMC is what this is for, and `du` prints the same numbers as a wall of text |

`check-system.sh` FAILs on a missing `ip`/`ping`/`ss` and on a declared package
that is not installed. The manual route is `04-software.md` § *Working in the
terminal all day*, and `06-troubleshooting.md` § *Is the network up?* is the
ordered list to follow when the second `ping` is the one that fails.

## Packages - root menu -> **Packages**

| Step | Expected |
|---|---|
| type `htop` | package names with their one-line description |
| Enter on `htop` | busy card, then the result; `htop` exists afterwards |
| search a package that is installed | the row is dim and says `installed` |
| Escape | nothing is installed |
| **Update the package lists** (last row) | `apt-get update` runs behind the same card, then the search repeats |

## Colour - root menu -> **Colour**

| Step | Expected |
|---|---|
| Enter on **Pick a pixel**, click anywhere | a notification with the hex, and `wl-paste` holds it |
| Enter on one of the Solarized swatch rows | the hex is copied (the launcher lists them by name) |

## Reminders - root menu -> **Reminders**

| Step | Expected |
|---|---|
| **New reminder**, type `tea`, Enter, leave the note empty, choose **In 5 minutes** | a notification stating when systemd will fire it, in systemd's own words |
| the same flow, but type a note at the second prompt | the note is part of the card the reminder fires later |
| reopen the list | the reminder with its title, and the note (if any) on a second, dimmer line |
| Enter on it | "Cancelled …" and the row is gone |
| **At 09:00** | the notification names a **concrete date**, and it does not repeat the next morning |
| a **repeat** choice (every day, every weekday, every week, every month) | the card names the next occurrence; the reminder stays in the list after it fires |
| type `*-*-* 25:00` where a date is asked for | systemd's own rejection (`Failed to parse calendar specification …`, `Invalid argument`) and the When menu again, title and note still known |
| `systemctl --user list-timers` | the pending reminder is there, and its `Description` carries the title (nothing on disk: `systemd-run --user --collect`) |

Reminders die with the session's user manager, which is the design. A reminder
that is **not** in `list-timers --all` was never created: `systemd-run` printed
the reason, and the menu shows it instead of claiming success.

## About - root menu -> **About**

| Step | Expected |
|---|---|
| open it | model, revision, kernel, uptime, memory, disk, battery, network, session, processes |
| **Copy this summary** | the same lines are on the clipboard |
| **The full picture** | fastfetch in a foot window that **stays open** and ends with `fastfetch has finished · close this window with Alt+Q`; a window that flashed for a tenth of a second is `--hold` missing from `rofi-about` |
| **Errors this boot** | `journalctl -p err -b` in a foot window (the same lines the report calls a problem) |
| **Boot time** | `systemd-analyze blame` in a foot window, longest first |
| **Devices** | `lspci` then `lsusb` in a foot window |

## Agents - `Alt+Ctrl+Shift+A`, and the root menu's `ai` group

| Step | Expected |
|---|---|
| open the root menu (`Alt+Space`) | an `ai` heading with **Agents** and **Dictation** together, above `system`; **Agents** opens its menu |
| `Alt+Ctrl+Shift+A` with no agent installed | the five agents, each row reading `Name — install <npm>`, then **Set the default agent…** and **Open a terminal in ~/Work**; Escape leaves nothing running |
| from a terminal, `rofi-agents --new claude`, then the name `work` | `~/.local/share/lazyos/agents/claude/work` exists and is empty, and `~/.local/share/lazyos/agents/claude/default` holds `work` |
| with Claude Code or Codex installed, its row grows **accounts…** | `● work  (default)` is listed; `rofi-agents --env claude work` prints `CLAUDE_CONFIG_DIR` at the directory |
| launch the installed Claude Code row | foot opens in `~/Work` (or `$HOME` when `~/Work` does not exist) with `CLAUDE_CONFIG_DIR` pointing at `agents/claude/work` |

An account is a directory, not a login: the first launch in a new one runs the
CLI's own sign-in, so two subscriptions never share a token. The menu installs
with an explicit `--prefix ~/.local`, so npm never needs root and never touches
`/usr/local`.

## Files and thumbnails - `Alt+Ctrl+F`

| Step | Expected |
|---|---|
| open `~/Pictures` | rows with a **32 px** thumbnail for images, videos and PDFs; the icon is the picture, not a mimetype glyph |
| open a directory, then Escape, then `Alt+Ctrl+F` again | it reopens that directory, not `$PWD`; a fresh home with no state file starts in `$HOME` (`cat ~/.local/state/lazyos/files-last`) |
| press `Alt+Search` (the file search) from another directory, then `Alt+Ctrl+F` | still the directory the list last had: `--search` writes nothing to that state file |
| open a directory of source files or dotfiles | **every** row still has an icon (a page, an executable, a folder), so the names line up in one column - no bare text list |
| reopen the same directory | instant: the thumbnails come from `~/.cache/thumbnails/normal/` and are not decoded again |
| Enter on an image | **imv** in its own window (zoom, pan, rotate; the arrow keys walk the folder). The first picture of a session also raises a notification naming those keys (`lazyos-image-hint`), and only the first. A foot window drawing sixels means imv could not get an EGL context, which is the fallback |
| Enter on an SVG | **imv** as well, not `bat`: an SVG is a text file, but the image family is decided before the text test |
| Enter on a **video** (`.mp4`, `.mkv`) | **mpv** in its own window with sound; `q` closes it. Firefox means mpv is missing |
| Enter on a PDF, a DjVu or a comic | **papers**, with search, zoom and print. Firefox means papers is missing |
| Enter on an EPUB | Firefox: `papers` reads no EPUB |
| Enter on an HTML file | `bat` (it is text), not Firefox |
| Enter on a `.deb` | Firefox, as a download prompt: the browser is what a package is for here |
| Enter on a text file | `bat` in the popup window (`BAT_THEME=ansi`, solarized) |
| open a deep path (`~/Dev/LazyOS/uis/sway/files/home/.config/bin`) | the window is drawn wide enough that the **end** of the name is readable: rofi cannot scroll a row sideways, so `lib-rofi.sh`'s `ui_width` sizes the list instead of letting rofi cut it with `…` |
| spam `Alt+Ctrl+F` on a directory of large photos | the application keeps responding while the rows fill in (four decoders at a time) |
| **Alt+1** on a directory | the actions for it: disk usage, compress, copy the path, a terminal here. Enter still *opens* the row; only Alt+1 asks |
| **Alt+1** → **Compress** on a folder | a `busy` card, then `folder.tar.xz` beside it, and the bar spinner while it packs (click the spinner to stop it) |
| **Alt+1** → **Extract** on that archive | a folder named after the archive. Run it again: it refuses ("already exists") instead of merging into it |
| **Alt+1** on a video → **Convert** → MP3 | `name.mp3` beside it, with a card. On an `.mp4` → MP4 the output is `name-converted.mp4`, never the source |
| **Alt+1** on an `.iso` | it mounts through `udisksctl` and the browser opens where it landed |
| **Alt+1** on the dotfiles row | a notification saying it has nothing to act on, and nothing else happens |

An Alt+1 that switches to workspace 1 means rofi is not inhibiting the
compositor's keys (`global-kb: true` in `~/.config/rofi/config.rasi`); an Alt+1
that opens the row means `-kb-custom-1 Alt+1` was dropped from `rofi-files`.
`check-system.sh` warns about both.

A list with no pictures at all means `-show-icons`/`thumbnail://` was dropped from
`rofi-files`, or `lazyos-thumbnail` is not executable; `check-system.sh` warns
about both. A row with **no** icon in a text-only directory means the theme is
missing that name (`check-system.sh` checks the table against the PNGs).

## Search inside the files - `Alt+Ctrl+G`

| Step | Expected |
|---|---|
| press **Alt+Ctrl+G**, type a word you know is in a file | a `busy` card ("Looking for …") and then a menu of matches, the matching **text** on each row and `path:line` dimmed |
| Enter on a match | the file in `bat`, scrolled to that line and with the line highlighted |
| a word that appears 40 times in one file | at most three rows from that file |
| a word inside `~/.cache` or `node_modules` | not listed (the same four excludes as the other searches) |
| type `a(b` (a broken regex) | a critical card quoting ripgrep's own parse error, not an empty list |
| a word that is nowhere | "Nothing matches" |
| press it again while it is searching | the first card's spinner in the bar can be clicked to stop the run |

## Download - root menu -> **Download**

| Step | Expected |
|---|---|
| Alt+Space → Download → **Paste a link**, a short video URL, **Video** | a `busy` card, then "Saved `<name>.mp4`", and the file in `~/Downloads` |
| the same with **Audio only** | a `.m4a` in `~/Downloads`, no video stream |
| copy a URL first, then open Download | a second row, **the link in the clipboard**, which uses it without a prompt |
| a link to a video inside a playlist | one file, not the playlist |
| a link to something unavailable | a critical card with yt-dlp's own words ("Video unavailable"), not a traceback |
| click the bar's spinner mid-download | the card says the `.part` file is still there, and no half file is presented as finished |

## Format a disk - root menu -> **Places** -> **Format a disk**

| Step | Expected |
|---|---|
| open it with only the eMMC and the boot card attached | "Nothing to format": the eMMC is fixed storage and the card is the disk this system runs from, so neither is offered |
| boot from the card and wipe the eMMC's partition table (`sudo parted -s /dev/mmcblk0 mktable gpt`), then open it | still "Nothing to format": the eMMC is fixed storage and the disk this system depends on, which is the case the kernel-partition rule alone missed; `scripts/install-emmc.sh` is how it is written again |
| insert a USB stick, leave it unmounted | the stick, with its model and size |
| pick it → **Cancel** | nothing at all is written |
| pick it → **exFAT** → the confirmation | a card, then "is empty (exfat, label LAZYOS)"; the stick mounts again from Places |
| the same with **ext4** | `mkfs.ext4`, and the disk mounts on a Linux machine |

Neither menu may ever list `mtdblock0` (the SPI flash) or `mmcblk0boot0`/`mmcblk0boot1`
(the eMMC's hardware boot partitions): `lsblk -d` shows all three as small disks,
none mounted and none carrying a kernel partition, and writing any of them is how
this machine gets bricked or loses its boot blocks. Neither may list the eMMC or the
card the system is running from either, whatever their partition tables say.

## Auxiliary swap - root menu -> **Auxiliary swap**, or **Places** -> **Auxiliary swap**

| Step | Expected |
|---|---|
| open it | the disk list, never `Missing:`: `parted`, `mkswap`, `swapon`, `swapoff` and `blkid` live in `/sbin`, which the session's `PATH` does not carry, and `~/.config/bin/lib-rofi.sh` is the line that adds it |
| open it with only the eMMC and the boot card attached | "Nothing to offer": the eMMC is the disk this system runs from and the card carries a bootable kernel partition, and the sentence says so |
| boot from the eMMC with the card that installed it inserted | "Nothing to offer ... carries a bootable kernel partition (the card that installed this system)": the card is removable and unmounted, and only its kernel partition excludes it |
| open it with the boot card and a spare, unmounted card/SD | the spare card only, with model and size |
| pick it → **Cancel** | no partition table is written |
| pick it → **The whole card** → the confirmation | a busy card, then a notification with the partition and "priority 10"; `swapon --show` lists it, `blkid -L lazyos_swap` resolves it |
| `grep lazyos_swap /etc/fstab` | one marked block: `LABEL=lazyos_swap none swap sw,nofail,x-systemd.device-timeout=15s,pri=10 0 0` |
| **Hibernate** from the power menu (`Alt+Escape`) on the default card (swap on, no `resume=`) | a notification naming the missing `resume=` and the kpart rebuild it needs; the machine stays up, it does not suspend |
| `sway-hibernate --status` with that fresh swap (nothing has been swapped to it yet) | `swap_kb` is the partition's size in KiB (about 8.4 million for a 8 GiB card), never `0`, and `hibernate=no` only because `resume=no` |
| rebuild the kpart with `CONFIRM=yes RESUME=yes ~/Dev/LazyOS/scripts/install-kpart.sh` (`05-system.md` § Suspend and hibernate), reboot, **Hibernate** | `--status` says `hibernate=yes`; the card names the partition, the machine powers off, and powering it back on restores the same session |
| run `CONFIRM=yes ~/Dev/LazyOS/scripts/install-kpart.sh` again without `RESUME=` and reboot | `cat /proc/cmdline` still carries `resume=LABEL=lazyos_swap`: a kernel update does not silently stop the machine resuming |
| reboot with the card in | `swapon --show` lists it again without touching the menu (priority 10, below zram's 100) |
| remove the card and reboot | the boot still reaches the session (a swap unit may show as failed only if `nofail` was lost); it waits at most 15 s for the card |
| run the menu again | the status row (device, size, priority) and **Turn it off**, not the disk picker |
| **Turn it off** → confirm | the partition is swapped off, the fstab block is gone, the partition is still in `lsblk` with `FSTYPE=swap`, and the next boot does not try to use it |
| after turning it off, open **Format a disk** | the card is offered again (it is no longer active swap), and formatting erases the old swap partition |

Never pull the card while the swap is active: the pages living on it are lost. Turn
it off from the same menu first. Hibernate additionally needs
`resume=LABEL=lazyos_swap` on the kernel command line - the swap row does not edit
the kernel partition, so the default card refuses with that message. A partition
labelled `nureb_swap` (an old card) is still usable as swap, but that `resume=`
will not find it: re-label it (`sudo swaplabel -L lazyos_swap <device>`) or let
the swap menu recreate it.

## Install LazyOS - root menu -> **Install LazyOS**

Boot from the card; the menu writes `/dev/mmcblk0`, the internal disk.

| Step | Expected |
|---|---|
| open it on the eMMC install (the system runs from the target) | "Cannot install yet": the target is the disk this system is running from, and the Install row is not offered |
| boot from the card and open it | the machine report - target, room needed, charger and battery, the three kernel files, the installer's tools, a `sudo` that will not prompt - and **Cancel** selected; **Install LazyOS** offered only if every gate passed |
| press Enter on a report row (they carry no action) | the menu re-opens and re-checks; it does not close |
| pick **Install LazyOS** and answer the typed name wrongly | a critical toast, nothing written; the menu is still there |
| answer `mmcblk0` | a toast that the copy started and the bar spinner; open the menu again and pick the log row for the live log. `systemctl is-active udisks2` says inactive while it runs |
| while it runs, `systemctl show -p LoadState udisks2` and `findmnt -rn -S /dev/mmcblk0p3` | `masked-runtime` and no mount: the two states `disk_release()` refuses to start with |
| when it finishes | the card says it finished; `lsblk -o NAME,PARTLABEL /dev/mmcblk0` names `pmOS_kernel`, `pmOS_boot` and `pmOS_root`; `sudo futility show /dev/mmcblk0p1` says *Body verification succeeded*; `udisks2` is `active`/`enabled` again with no runtime mask |
| power off, remove the card, pick **internal disk** in the firmware menu | the eMMC boots the system that ran from the card (the same files, the same `REVISION`) |
| open the same menu there | it refuses for the same reason as the first row |
| kill the copy by hand mid-run (`sudo pkill -f '[i]nstall-emmc.sh'`) | the next menu reads `failed`/`did-not-finish` with the installer's last `ERROR:` line; the disk is half written and a new run is the way out - there is no stop button, on purpose |

The four report rows are gates, not decoration: below 20 % on battery the install
is refused, and a `sudo` that would prompt makes the Install row disappear rather
than fail after the disk has been formatted. The copy is not interruptible (a
killed `rsync` or `mkfs` leaves a disk that looks written and is not) and the log
stays in `~/.local/state/lazyos/install-<timestamp>.log`.

## Search key - the launcher

| Step | Expected |
|---|---|
| press **Search** (nothing else) | the applications menu, the same one as `Alt+D`: one row per `.desktop` entry, narrowed as you type |
| type `fire` and press Enter | `firefox-esr` starts |
| hold **Search**, tap a top-row key | an F-key reaches the focused window; no launcher opens on the way |
| press **Search+Shift** | the terminal file search window (see below), not the launcher |

Both keys are one rofi process while the menu is open and nothing afterwards.

## Alt+Search - the file search

| Step | Expected |
|---|---|
| hold **Alt**, tap **Search**, release Search then Alt | a list of files from all over `$HOME`, newest first, each with the same icon the browser draws |
| hold **Search**, tap **Alt**, then release Search | the same list: the chord works in either press order |
| type `conf` | the list narrows as you type; a path is shown as `~/relative/path` |
| Enter on a text file | it opens exactly as it does from `Alt+Ctrl+F` |
| Escape | nothing opens |
| press Alt+Search twice in a row | two lists, no leftover window and no second process after both close |

`rofi-files --search` is the same mode from a terminal. An empty list on a home
with files means `fd`/`fdfind` is missing (`check-system.sh` FAILs).

## Search+Shift - the terminal file search

| Step | Expected |
|---|---|
| press **Search+Shift** | a floated, centred foot window with an `fzf` prompt showing `~ > ` and rows filling in as the walk runs |
| type part of a path, two segments | the list narrows while you type, with no pause between keystrokes (the whole tree is the pool: nothing is capped at 500 rows) |
| put the cursor on a text file | the right 55 % shows its first lines through `bat` (`BAT_THEME=ansi`, solarized) |
| put it on a photo or a PDF | the preview draws a picture in the pane (symbols, not sixels - it survives the next redraw) |
| put it on a `.zip` or a binary | the pane names the type and the size instead of trying to draw it |
| **Ctrl+Y** | `copied the path` in the header; `wl-paste` is `~/relative/path` |
| Enter on a text file / a photo / a video | opens exactly as a double click in pcmanfm does (`bat`, `imv`, `mpv`) |
| type `config` | dotfiles are found here (`~/.config/...`), unlike the file list (Alt+Search) |
| Escape | the window closes and nothing opens |
| press Search+Shift twice in a row | two windows, and no `fzf` process left after both close |
| `lazyos-find` from a foot prompt | the same search runs *in that terminal* instead of a new window |

A window that flashes and closes means `bin/lazyos-find` lost its executable
bit; an empty list on a home with files means `fzf` is missing
(`check-system.sh` FAILs both). `fd`'s `.cache`, `.git`, `node_modules` and
`.local/share/containers` are excluded, so a file inside one of them is
correctly absent.

## Files (window) - Alt+Ctrl+Shift+F

| Step | Expected |
|---|---|
| press **Alt+Ctrl+Shift+F** | pcmanfm opens on `$HOME`, in the solarized palette (base03 window, cyan selection), JetBrains Mono, LazyOS icons |
| press **Alt+Ctrl+F** | the file list (`rofi-files`), not the window |
| tap **Search** on its own | the launcher, not the window |
| press **Alt+Search** | the file search, not the window |
| hold **Search**, tap a top-row key | an F-key reaches the focused window; no menu opens on the way |
| right-click a file -> **Open With** | the first entry is what a double click does: text in `bat`, a document in `papers`, a picture in `imv` (or `chafa`), a video in `mpv`. Two deliberate exceptions: an HTML page is text to the Files list (`bat`) and a page to a double click (**Firefox**), and an EPUB goes to Firefox because `papers` reads none |
| press **F4** | `foot` opens in the current directory (not an error about `x-terminal-emulator`) |
| delete a file | it goes to the Trash and **Restore** puts it back (`gvfsd-trash`) |
| look at a folder of photos | thumbnails; a PDF or a video keeps its mimetype icon (libfm decodes images only - that is not a bug) |
| insert a USB stick | it appears in the sidebar already mounted (`udisks2`); nothing on it runs by itself |
| `dpkg -l pcmanfm gvfs` | both installed; `gvfs-backends` absent on purpose |
| `ls ~/.config/mimeapps.list` | no such file: only `/etc/xdg/mimeapps.list` decides, so `org.lazyos.view.desktop` is the system default. A file left from a pre-v0.3.0 card keeps the old viewer for every association it names |

A window that looks like Adwaita's light grey means `~/.config/gtk-3.0/gtk.css` is
missing; icons that are empty squares mean a name the LazyOS theme does not carry
(`check-system.sh` warns with the list). A double click that does nothing means
`~/.config/bin/lazyos-view` is not executable: `gio mime image/png` then answers
"no default application" instead of naming `org.lazyos.view.desktop`.

## The document reader - GTK4, the palette and printing

`papers` is GTK4/libadwaita, so the GTK3 files above do not reach it. It was the
one white window in the session.

| Step | Expected |
|---|---|
| open a PDF (Alt+Ctrl+F, Enter on a `.pdf`) | the reader opens **dark**: base03 page background, base1 text, the headerbar in base02 and the cyan accent on the selected control |
| `papers`, then **Print** | the print dialog opens with no printer available and says so (no `cups` in the image): not a crash, and not a silent no-op |
| `ls ~/.config/gtk-4.0/` | `settings.ini` and `gtk.css` |
| `grep prefer-dark ~/.config/gtk-4.0/settings.ini` | `gtk-application-prefer-dark-theme=1` |
| move `gtk-4.0/gtk.css` away (put it back afterwards), reopen a document | the window falls back to libadwaita's dark default (grey cards, blue accent) - proof the first row is the CSS, not the preference |
| move `gtk-4.0/settings.ini` away too (put it back), reopen | the light default comes back: that is the state the card shipped in |

A white window means `gtk-4.0/settings.ini` is missing or lost its dark line
(`check-system.sh` WARNs); that grey-but-dark look means the preference is there
and `gtk.css` is not. Do **not** "fix" it with `xdg-desktop-portal-gtk`: the
Settings portal is a resident daemon for one boolean, and the two files are read
by GTK4 without it. Printing needs `sudo apt install cups` and one configured
queue; the manual is `04-software.md` § *Files in a window* and
`06-troubleshooting.md` § *A GTK4 window is white*.

## USB sticks and SD cards - Places

| Step | Expected |
|---|---|
| **insert a USB stick** | a mako card `USB drive: <label> mounted at /run/media/<you>/<label>` within a second, and no process left behind (`systemctl status 'lazyos-automount@*'` is `inactive (dead)`) |
| open **Places** (root menu, or `rofi-places`) with the stick in | the stick is a **disk** row (already mounted) and there is **no Mount row for it**; an SD card in the internal slot still shows a **Mount** row, because auto-mount is USB only |
| Settings -> **Auto-mount USB drives** -> off, unplug and replug the stick | no notification, no mount, and Places shows a **Mount** row for it; turn the switch back on and replug |
| `cat ~/.config/lazyos/automount.conf` | `enabled=no` or `enabled=yes` - one line, and a missing file means on |
| `ls -ld /run/media/*/` and `findmnt -rn -S /dev/sdX1 -o OPTIONS` | the directory is under your name and the options carry `uid=<you>`, not `uid=0`: the helper mounts as the account, not as root |
| insert a stick formatted without a partition table (a bare `mkfs.vfat /dev/sdX`) | it mounts too: the rule has a `DEVTYPE=="disk"` line for exactly that shape |
| **insert the rescue card into the internal microSD slot** | nothing mounts by itself, and the eMMC is never a candidate: the udev rule matches `SUBSYSTEMS=="usb"` and `check-system.sh` fails if that line goes |
| plug a stick while the switch is on but with `plugdev` missing (`sudo gpasswd -d "$USER" plugdev`, then log out and back in) | the notification says `Not authorized` in udisks' words - the polkit rule matches by group, and `check-system.sh` fails on the membership |
| `systemctl status udisks2` after all of it | *inactive (dead)* again - udisks2 is D-Bus activated, and the helper is one process per plug, not a watcher |
| Enter on a **Mount** row | a mako card while it mounts, then `rofi-files` opens that directory. A trailing `.` in the path is the `udisksctl` message not being stripped |
| Enter on a mounted disk row | `rofi-files` browses it |
| **Alt+1** on a mounted disk row | "Unmounted `<label>`" and the row is gone from the next menu; pulling a stick that is still mounted is how a filesystem is lost |
| **Alt+1** on a folder row (Home, Documents) | "Only a disk can be unmounted", and nothing else happens |
| a swap partition or a LUKS one on the stick | no **Mount** row for it (a swap has nothing to show and LUKS needs a passphrase prompt this menu does not have) |
| unplug and replug the stick | it mounts again (or is listed again with auto-mount off); a stick that stays mounted after the unplug is a sign the filesystem is dirty |
| right-click a mounted stick in **pcmanfm** (Alt+Ctrl+Shift+F) -> **Unmount** | it unmounts and the icon goes away, with no `Failed to execute child process "eject"`: pcmanfm unmounts through `/usr/bin/eject`, which is its own package (`gvfs` and `udisks2` do not depend on it) |
| `systemctl status udisks2` | *inactive (dead)* until something mounts - it is D-Bus activated, nothing runs resident, and `udiskie` is deliberately absent |

A **Mount** row that fails says why in a critical notification, in udisks' own
words, and so does an auto-mount that fails; an auto-mount that decided not to act
(a swap, a LUKS volume, the disk this system came from) says nothing at all,
because none of those is an error. `vfat`/`exFAT`/`NTFS` need no helper package:
the kernel has the drivers.

## Developer - `Alt+Ctrl+D`

The rows carry `~/.ssh` and `~/.gnupg`, so the test is as much about what the
archive refuses as about what it saves.

| Step | Expected |
|---|---|
| `Alt+Ctrl+D` | three rows: **Export my profile**, **Restore a profile**, **Save a diagnostics bundle**, and the dim line saying the archive carries the keys |
| **Export my profile** with a stick inserted | the target list offers its partitions (a mounted one by label, an unmounted one as **Mount LABEL**) and `~/Downloads`; the eMMC and the card this system boots from are never in it |
| pick the stick | the busy card turns to **Waiting for the passphrase…** and a masked rofi prompt appears - launched from the key, never a prompt on the VT (that was the bug: `[ -t 0 ]` is true for tty7 in a sway `exec`) |
| type the passphrase twice | a busy card, then the path and size of `lazyos-profile-<host>-<date>.tar.zst.gpg`; a `.txt` manifest sits beside it |
| `ls -l` on the stick | both files are `0600` (on exFAT the bits are a no-op, which is why the archive is encrypted) |
| `sed -n '1,12p'` the manifest | date, host, image revision, the paths and `apt-mark showmanual` - and no key material |
| `gpg -d <archive> \| tar --zstd -t` with the passphrase | `.ssh/`, `.gnupg/`, `.bashrc`, `.config/...`, the agents' homes and `.config/mozilla/firefox/<profile>/` with `places.sqlite`, `logins.db` and `key4.db`; `tar -tv` shows `-rw-------` on `id_ed25519` |
| `gpg -d <archive> \| tar --zstd -t \| grep -E 'storage/\|security_state\|cache2\|sqlite-wal'` | nothing: the Firefox site data and the caches never travel (the profile is 238 MB, the archive a few) |
| export again with a different passphrase at the second prompt | "The two passphrases differ", and the prompt asks again: nothing is written |
| export with no stick inserted, pick `~/Downloads` | the archive lands there; copying it to a stick later is the same file |
| **Restore a profile**, pick the archive | the passphrase, then the count of files and **Cancel as the first row** |
| **Escape** at the confirmation | "Nothing was restored", and the files are untouched |
| **Restore** over a file you changed | the file comes back, the card says to log out and back in, and `~/.cache/lazyos/backups/profile-before-restore-<date>.tar.zst` holds the version from before |
| `~/.config/bin/lazyos-profile import <archive>` from ssh | the same work with a terminal prompt (typed `restore`), no rofi |
| **Save a diagnostics bundle** | `lazyos-report-<host>-<date>.tar.zst` with `report.txt`, `journal.txt`, `dmesg.txt`, `hardware.txt`, `sysinfo.txt` and a `README.txt`; `report.txt` has the `check-system.sh` summary and no colour escapes |
| `check-system.sh` | `developer menu complete`, the gpg round-trip, `--dry-run` naming `.ssh`/`.gnupg`, the agents' homes, the Firefox profile and the three ways in, all OK; no `unexpected user unit` after the round-trip |
| after a report run | `systemctl --user is-active gpg-agent.service` is `inactive` if it was not running before the check |

## The shell - `cd -`, the aliases and the prompt

| Step | Expected |
|---|---|
| `cd /tmp`, then `cd -` | back to the previous directory, with **no** `zoxide: detected a possible configuration issue` |
| `echo $PROMPT_COMMAND` | starship's hook **and** zoxide's, both of them: zoxide's init is the last line of `~/.bashrc` because starship replaces that variable wholesale |
| `zd -` | the previous directory, printed by `builtin cd -` (that argument is not handed to zoxide) |
| `ls`, `lt`, `cat <file>` | `eza --icons`, `eza --tree --icons`, `bat --style=plain --paging=never` |
| `ff` | the fzf picker; the choice opens in `$EDITOR` (`nvim`) |
| open a new terminal | the prompt draws with no missing-glyph box: foot needs the `Symbols Nerd Font Mono` fallback listed after `JetBrains Mono` in `~/.config/foot/foot.ini` |

## Editors

| Step | Expected |
|---|---|
| `echo $EDITOR $VISUAL` in a new terminal | `nvim nvim` |
| `nvim file`, `:q` | Neovim opens; no configuration is shipped, so it starts plain |
| `vim file`, `vi file` | the same editor both ways (`vi` is the alternative `vim` registers) |
| `eff` | the fzf picker, and the choice opens in Neovim |
| `EDITOR=nano eff` | nano wins: the profile only sets the variable when nothing has |
| `nano file` | still there, and still what `git commit` falls back to without a session profile |

A `vi` that is missing while `vim` works means the `vim` postinst never ran
(`update-alternatives --display vi` says so); `check-system.sh` asks for `vi` by
name for that reason.

## Text in other languages

| Step | Expected |
|---|---|
| `fc-list :lang=ar` | several faces (`fonts-noto-core`), not an empty list |
| `echo 'مرحبا بالعالم'` in foot, and the same text in Firefox | Arabic letters, joined and right-to-left; not a row of boxes |
| `echo 'नमस्ते दुनिया'`, `echo 'שלום עולם'`, `echo 'สวัสดี'` | Devanagari, Hebrew and Thai text in the same way |
| `fc-list :lang=ja` | **empty**: CJK is deliberately not installed |
| `echo '日本語'` before installing CJK | boxes - the documented gap, not a fault |
| `sudo apt install fonts-noto-cjk`, then `echo '日本語'` | the glyphs appear without a logout (fontconfig picks the new face up); `fc-list :lang=ja` is no longer empty |

Boxes for Arabic, Hebrew or an Indic script mean `fonts-noto-core` is missing
(`check-system.sh` WARNs with `fc-list :lang=ar`). There is nothing to configure:
fontconfig's fallback finds the family in the terminal, the browser and waybar.
The manual is `04-software.md` § *Text in other languages* and
`06-troubleshooting.md` § *Icons render as boxes*, which carries the same
`fc-list` questions.

## Screenshots

| Step | Expected |
|---|---|
| `Alt+Shift+F` | the notification names a file under `~/Pictures/Screenshots/<today>/`, and carries the image |
| `Alt+Shift+S`, drag a region | the same, one area |
| `Alt+Ctrl+C` -> annotate | `swappy` opens on the newest file and writes the edits back over it |
| `Alt+Ctrl+C` -> the last row | opens the screenshots folder (or `~/Pictures` before the first one) |
| `ls -d ~/Pictures/Screenshots/*/` | one directory per day, no `*.png` left in `$HOME` |

## Volume and brightness - the overlay

The keys used to change the value and say nothing; the only feedback was the
bar's 12 px number, which a fullscreen window covers.

| Step | Expected |
|---|---|
| press volume up / down | one card under the bar: the speaker icon, the level as a **cyan progress bar** and `NN%`; the value changes by 5% |
| hold volume up | one card that moves, not a stack of cards (the synchronous tag replaces it) |
| press Mute | the mute glyph and **Muted**, with an empty bar; press it again and the level comes back |
| press brightness up / down | the sun icon and `NN%`; the panel changes at the same time |
| a fullscreen video, then a volume key | the card is drawn **over** it: this is the case the bar cannot answer |
| lock the screen (`Alt+Ctrl+L`) and press volume | the value changes (the bindings are `--locked`); the card is behind the lock, which is expected |
| scroll the volume or brightness module on the bar | the value changes with **no** card: the bar already shows the number and a toast per scroll would be noise |
| `Alt+Ctrl+A` -> **Volume per application** | `pavucontrol` opens: one slider per application, plus the output's port/profile |
| `Settings` (`Alt+Ctrl+O`) -> **Volume per application** | the same window |

A card with text and no picture is a name mako cannot resolve - the theme needs
`audio-volume-{muted,low,high}` and `display-brightness` (FAIL in
`check-system.sh` when one is missing). No card at all is `bin/osd` without its
executable bit, a binding that went back to raw `pactl`/`brightnessctl`, or no
notification daemon; `check-system.sh` FAILs on the first two and WARNs on the
mako side (`icons=1`, `progress-color`). The manual is `03-shortcuts.md`
§ *Sound, light and screenshots* and `06-troubleshooting.md` § *The volume or
brightness keys change the value but show nothing*.

## Share this screen - Settings -> **Share this screen**

| Step | Expected |
|---|---|
| `Alt+Ctrl+O` | a **Share this screen** row, `off` until it is used |
| Enter on it | the menu reopens saying `localhost:5900 · read-only`, with **Stop sharing**; a notification names the port and the `ssh -L` command; an **eye appears on the bar** |
| `~/.config/bin/rofi-vnc status` | `localhost:5900 · read-only` |
| `ss -ltn \| grep 5900` | `127.0.0.1:5900` and **not** `0.0.0.0:5900` |
| from the laptop: `ssh -L 5900:127.0.0.1:5900 brunux@lazyos`, then a VNC viewer on `localhost:5900` | the screen, live. Typing in the viewer does **nothing** (it is read-only), and the pointer in the viewer does not move anything here |
| right click on the bar's eye | stopped: the notification says so, the eye is gone, `ss` shows nothing, `rofi-vnc status` answers `off` |
| start it twice | the second says *Already sharing* instead of starting a second server |
| `kill -9` the wayvnc process, then open the menu | the menu says `off` (a stale pid is not trusted) and offers to share again - never *Not serving* |
| occupy the port first (`python3 -m http.server 5900 --bind 127.0.0.1`), then start | the notification quotes wayvnc's bind error instead of claiming success |
| log out with it on, log in again | nothing is shared and `rofi-vnc status` says `off`: the state lives in `$XDG_RUNTIME_DIR/lazyos-vnc` |

What a failure means:

* **the menu says wayvnc is not installed**: the package is missing from the image;
  `check-system.sh` FAILs on it.
* **the row is there but Enter does nothing**: the script lost its execute bit, or the
  row has no `case` arm; `check-system.sh` FAILs on both.
* **the eye never appears**: `custom/vnc` is missing from `modules-right` in
  `~/.config/waybar/config.jsonc` (FAIL) or `style.css` lost `.empty` (WARN - then the
  card stays on the bar saying nothing). `pkill -RTMIN+8 waybar` forces the poll.
* **`ss` shows `0.0.0.0:5900`**: the script was edited; that is an unauthenticated
  desktop on the network and `check-system.sh` FAILs on it.
* **the viewer cannot connect**: the tunnel is closed (it has to stay open), or the
  viewer was pointed at `lazyos:5900` instead of `localhost:5900` - the port is on the
  loopback address by design. `06-troubleshooting.md` has the ordered list.

## Remote login - ssh

`sshd` is enabled on every boot on purpose: it is the recovery path, and the
same login carries the VNC tunnel above. The image's default credentials are
`brunux` / `debian`.

| Step | Expected |
|---|---|
| `systemctl is-enabled ssh` and `systemctl is-active ssh` | `enabled` and `active` |
| `ss -ltn \| grep ':22 '` | sshd listening on `0.0.0.0:22` (or `[::]:22`) |
| from another computer: `ssh brunux@lazyos`, password `debian` | a shell; `sudo -n true` answers without a prompt |
| `passwd` there, then reconnect with the new password | the old one is refused |
| `check-system.sh \| grep ssh` | the `---- remote login is enabled` info line (a WARN if enabled but not running) |
| as a test: `sudo systemctl disable --now ssh`, then `ss` again | no port 22; re-enable it with `sudo systemctl enable --now ssh` |
| after a fresh card, if ssh refuses the connection | `sudo ssh-keygen -A && sudo systemctl restart ssh`: the host keys are generated on a first boot that lacks them (`sshd-keygen.service` is adjusted for a copied image), and `journalctl -u ssh` names the reason |

Turning it off is a decision, not a cleanup: `sudo systemctl disable --now ssh`.
The manual is `05-system.md` § *Remote login (ssh)*.

## Touchpad swipes

| Step | Expected |
|---|---|
| three fingers to the left / to the right | next / previous workspace, the same as `Alt+1..0` |
| three fingers up with two tabs open in `firefox-esr` | the browser moves to the next tab, as if Ctrl+Tab had been pressed |
| three fingers down | the previous tab (Ctrl+Shift+Tab) |
| the same in foot, or in an editor with two files open | **nothing**, in any window that is not Firefox - and `pgrep -a wtype` while swiping shows no process: the filter runs inside sway, so the gesture costs nothing there |
| three fingers up on the bare desktop | nothing, and nothing in `journalctl --user -b` |

What a failure means:

* **the swipe does nothing at all, for all four directions**: `/etc/sway/config.d/22-workspace-gestures.conf`
  is not in the image; `check-system.sh` FAILs on the four bindings.
* **left/right work, up/down do not in Firefox**: `wtype` is missing (`check-system.sh` FAILs
  on it), or that Firefox runs under Xwayland (`MOZ_ENABLE_WAYLAND=0`): it then has no Wayland
  app id, which is what the binding filters on.
* **up/down also type into another window**: the `[app_id="(?i)^firefox" con_id="__focused__"]`
  prefix is gone from the binding; `check-system.sh` FAILs on it.
* **the tab changes on the *press* of another key instead**: something is bound to
  Ctrl+Tab in `~/.config/sway/config`; sway sees the injected keys first, and a
  session binding there swallows them before the window gets them.

## Tablet mode

Fold the keyboard all the way back.

| Step | Expected |
|---|---|
| fold it back | the on-screen keyboard appears at the bottom (solarized, `JetBrains Mono`), the touchpad **and the physical keyboard** do nothing, one notification: "Tablet mode: the keyboard is up and the screen follows the device" |
| type on the folded keys | nothing arrives in the focused window (the keys are under the screen now) |
| type in foot on the on-screen keyboard | letters land in the focused window; `Cmp` then `a` opens `á à â ä …` and tapping one inserts it; `Cmp` then `?` reaches `¿` |
| turn the device 90° clockwise, 90° back, upside down, and lay it flat | the screen follows within about a second, and the flat reading puts it back to `normal` |
| the pose you would type in (screen upright, facing you) | the screen is upright, **not** upside down - if it is not, see Calibrate below |
| tilt it, do not turn it | the screen does **not** flip |
| press the Lock key | the screen locks and the machine suspends (logind reads /dev/input directly, so a disabled sway keyboard does not matter) |
| press the volume keys | the volume still changes: the power/volume keyboard shares its libinput device with the tablet switch, so it is left enabled |
| fold it back into a laptop | keyboard gone, touchpad **and keyboard** back (type), screen upright, "Tablet mode off" |
| `pgrep -x wvkbd-mobintl` and `pgrep -af 'tablet-mode poller'` afterwards | nothing |
| Settings (`Alt+Ctrl+O`) -> **Tablet mode** | toggles by hand; the row reads `on · screen 90` or `off` |
| Settings -> **Calibrate rotation** (listed only in tablet mode) | hold it upright and it says so; `~/.config/lazyos/tablet.conf` then holds `normal=<axis><sign>` |
| Settings -> **Rotation** | still turns the screen by hand, independently |

What a failure means:

* **no keyboard on the fold, but it works by hand** (`~/.config/bin/tablet-mode on`):
  the switch did not arrive. `grep -A5 cros_ec_buttons /proc/bus/input/devices`
  must show `SW=3`, and `bindswitch tablet:` must be in
  `/etc/sway/config.d/28-tablet.conf`.
* **keyboard, no rotation**: the accelerometer (`/sys/bus/iio/devices/iio:device1`,
  `cros-ec-accel`); `check-system.sh` FAILs when it is gone.
* **rotation inverted or turned the wrong way** (the 2026-10-21 card came up
  upside down): run **Settings → Calibrate rotation** while holding the screen
  upright, or `~/.config/bin/tablet-mode calibrate`. It writes the reference to
  `~/.config/lazyos/tablet.conf` and applies it at once, and it needs no rebuild.
  `tablet-mode calibration` says `calibrated …` or `default …`; deleting the file
  goes back to the built-in mapping. The four `transform` values in `orient()`
  are now derived from that one reference.
* **the physical keyboard still types**: `swaymsg -t get_inputs -r` lists the
  keyboards, and `tablet-mode status` says whether the mode is on; `tablet-mode
  off` restores it. It is switched off only after the on-screen keyboard is
  confirmed alive, so a wvkbd that failed leaves the physical keyboard on and the
  notification says so.
* **folded back into a laptop and nothing happens** (the mode stays on, the
  on-screen keyboard stays up): the tablet switch was suspended with a keyboard.
  `swaymsg -t get_inputs -r` must list `cros_ec_buttons` as a `switch` with
  `"send_events": "enabled"` even while the mode is on; the script switches the
  typing keyboards off one identifier at a time and leaves any identifier a
  switch also carries alone, and `check-system.sh` FAILs when a switch is
  suspended. `~/.config/bin/tablet-mode off` (or Settings → Tablet mode) is the
  way out without folding, and the on-screen keyboard can type it.
* **keyboard appears but types nowhere**: the focused surface refuses virtual
  keyboards; the emoji picker's notification says the same thing, and pending
  `zwp_virtual_keyboard_manager_v1` support is in wlroots, not wvkbd.

## Nothing resident - the agreed set

The session starts nothing that stays, with two agreed exceptions (the
clipboard's watcher and workstyle). This is the test of that sentence.

| Step | Expected |
|---|---|
| `pgrep -x waybar`, `pgrep -x mako`, `pgrep -x swayidle` | one pid each: the bar, the notification daemon, the idle chain |
| `pgrep -x workstyle` | one pid: the workspace icons need something watching the tree |
| `pgrep -fx 'wl-paste --watch cliphist store'` | one pid: no clipboard history without a watcher |
| `pgrep -x 'udiskie\|swaync\|dunst\|picom\|blueman-applet\|packagekitd'` | **nothing**: each one is a package this port answered "no" to |
| `pgrep -f '[t]racker-miner-fs-3\|[l]ocalsearch-3'` | nothing: the file manager is pcmanfm, which starts no indexer |
| `systemctl --user list-units --type=service --state=running --no-legend` | PipeWire, WirePlumber, the portals, gvfs (while pcmanfm has a window), dconf, mpris - and `gpg-agent` only while gpg is using it, which idles out on its own (`gpgconf --kill gpg-agent` ends it now) - and nothing else |
| `systemctl status udisks2` | *inactive (dead)* until a stick is mounted: it is D-Bus activated |
| open `Alt+Ctrl+O`, then close it | the menu is one rofi process that ends with the window: `pgrep -x rofi` is empty afterwards |
| open and close every menu once, then check | no leftover `rofi` or `foot --app-id=popup`; `pgrep -c -x swayidle` is still `1`, and no menu has left a helper behind |

`check-system.sh` FAILs on a declined daemon and WARNs on an unexpected user
unit; the two agreed residents are exec'd by the sway configuration, which is why
they are process checks and not units. A process that is here but is not in the
list above is the thing to write down in `reports/` before deleting it: the
allowlist is the agreement, not the names.

## Regressions this round could have touched

| Step | Expected |
|---|---|
| `Alt+Ctrl+K` (key view) | the Clipboard and Emoji bindings with their descriptions, and **a picture on every row**: the tool (power, files, clipboard, capture…), the action (an arrow for focus and move, a cross for close, a refresh arrow for reload) or the section (windows, gear, lock, sun, speaker…). Every row's text starts in the same column |
| **Search**, **Alt+Search**, **Search+Shift** | the applications menu, then the file list (`rofi-files --search`), then the fzf window in foot - and Search alone does not open either file view |
| `Alt+Escape` | the power menu (not the battery banner) |
| a slow action (Wi-Fi scan, DNS change) | a mako card that becomes the result, and the bar's spinner while it runs |
| **click the spinner** during a Wi-Fi scan | the card turns into "Stopped Wi-Fi" (or whatever it was), no second toast, and the Wi-Fi menu opens nothing instead of half a list |
| click the spinner during an apt install | it says the task cannot be stopped, and the install finishes |
| any notification that has one (`sudo`, a file copy) | its picture is drawn; a text-only card means mako's `icon-path` |
| `Alt+Ctrl+V` and press `Alt+1` | the entry is deleted (the second action still works after the `-kb-custom-1` fix) |
| root menu (`Alt+Space`) | **Colour**, **DNS**, **Packages**, **Reminders**, **About**, **Download**, **Password** and **Search inside files** are all there, and the `ai` heading has **Agents** and **Dictation** together, above `system` |
| `Alt+Ctrl+G` and then `Alt+1` inside that menu | nothing is taken by sway: the menu answers, the workspace does not change |
| `Alt+Ctrl+F`, then `Alt+1` | the actions menu, not workspace 1 (the `global-kb` fix reached both menus) |
| `gio mime image/png`, `gio mime application/pdf`, `gio mime inode/directory` | `org.lazyos.view.desktop`, `org.gnome.Papers.desktop`, `pcmanfm.desktop`. This is GIO's own answer, not the file read back: a `lazyos-view` without its executable bit makes GIO say "no default application" |
| `dig +short example.com` and `host example.com` | an address. `dig` comes from `bind9-dnsutils`; `resolvectl` is deliberately absent because NetworkManager owns `resolv.conf`, and its absence is not a gap |
| a menu whose rows are long (Files on a deep path, a grep match with a long line) | the window is wide enough for the end of the path; a row cut with `…` mid-detail means `ui_width` stopped sizing that list |
| lock (`Alt+Ctrl+L`, the Lock key, and the 300 s idle timer) | the **mountain photograph** on base03, then back to the session; no idle screen left behind (it is stopped before the lock) |
| idle for 150 s without touching anything | the **LazyOS** wordmark, the caption and the arcade line, with an effect rebuilding them and another one after it (a still frame means `tte` did not start) |
| press any key while the idle screen animates | it closes at once, not at the end of the effect |
| stay idle past the lock and then a minute more | the panel goes dark (360 s) and the first input brings it back with the lock still up (see *The idle chain*) |
| volume, mute and brightness keys | the value changes and the overlay appears; the bar's scroll still changes the value with no card |
| `Alt+Ctrl+A` and `Alt+Ctrl+O` | both carry **Volume per application**, and it opens `pavucontrol` |
| a fullscreen video left alone past five minutes | it is not locked (the inhibit rule); leaving fullscreen lets the next idle period lock the screen |
| `Alt+Shift+Escape` | the screen locks, *then* the machine suspends; on resume the lock is still up |
| shut down from the power menu | the machine powers off (the question is asked once) |

## Writing it down

`reports/check-system-<date>-card.txt` gets the script's output verbatim (MAC
masked to the OUI), and `reports/feedback-<date>-<what>.txt` gets what only you
saw: which row was wrong, which notification never came, which key did nothing.
Both are what the next round's checks are written from - a FAIL here becomes a
line in `check-system.sh`, not a note in a commit message.
