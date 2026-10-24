# The system

## The disk, and why the third partition is not the whole story

The disk has three partitions, the ChromeOS layout this machine's firmware
expects:

| Partition | Size | What |
|---|---|---|
| `p1` `pmOS_kernel` | 128 MiB | the **signed FIT** ("kpart") the firmware boots: a kernel, the initramfs and the device trees in one file |
| `p2` `pmOS_boot` | 512 MiB | ext2, mounted on `/boot`: the same kernel and initramfs, unpacked |
| `p3` `pmOS_root` | the rest | the Debian root filesystem |

The firmware does not read `/boot`. It loads `p1`, which is why a kernel update
is only half done after `apt`: the new kernel lands in `/boot` and `/lib/modules`,
but `p1` still carries the old one. Finish it from the repository copy:

```sh
sudo ~/Dev/LazyOS/scripts/install-kpart.sh
```

That script finds the partition this machine booted from (it compares
`kern_guid` in `/proc/cmdline`), builds a fresh kpart, verifies it is dev-signed,
backs up what it replaces to `~/.cache/debian-aarch64/work/`, and writes it.
`CONFIRM=yes` is required.

## The kernel

Linux **7.2.7-mt81**, built here with clang and lld from
`kernels/mediatek-mt81/`. It is not a Debian kernel: the MediaTek display, MMC,
panfrost (the GPU) and ext4 are built in, and the configuration carries this
machine's tuning:

* **MGLRU** enabled by default - page reclaim by generation instead of a full
  scan, which is what 4 GB under pressure wants;
* **schedutil** as the CPU governor (`cat
  /sys/devices/system/cpu/cpufreq/policy*/scaling_governor`), instead of
  `ondemand`, which pinned both clusters at maximum;
* **1000 Hz** timers, full preemption;
* transparent huge pages set to `madvise` on the command line.

Memory: `zram` provides a **~2.5 GB** compressed swap device - two thirds of RAM,
because the pages it holds are compressed and cost about a quarter of the RAM their
size suggests (measured here: 427 MB of pages in 98 MB). The size is a deliberate
middle: the packaged half of RAM (1.9 GB) is reached while RAM is still free, and the
full-RAM cap was shipped once and dropped because a cap that never fills lets the
kernel (swappiness 150) swap out cold pages it used to keep, and every re-touch pays
an lz4 decompress - a small but real latency. Two thirds keeps roughly double the old
headroom while the cap still fills, so zram stays a burst tier and the card is
reached only after ~2.5 GB are already held compressed (zram-generator,
`/etc/systemd/zram-generator.conf`, lz4). `systemd-oomd` kills a memory hog at 50 %
pressure instead of letting the machine freeze. `swapon --show`, `zramctl` and
`cat /sys/block/zram0/mm_stat` show what is in use.

## When you leave it alone

The session sleeps in three steps, and each one exists because the previous one
left something on:

| After | What happens |
|---|---|
| 2½ minutes | the LazyOS idle screen starts (one foot window, 30 fps, nothing else) |
| 5 minutes | the session locks (`sway-lock`, the same lock every other path uses) |
| 6 minutes | the panel goes dark (`swaymsg "output * power off"`, i.e. DPMS) |

Any input ends the idle screen and brings the panel back: a key, a moving mouse
or trackpad, a tap on the touchscreen. swayidle's `resume` command turns the
panel on again, and `after-resume` does the same after a suspend, which is a
different event and would otherwise wake to a dark screen. The lock answers
typing only - `swaylock` ignores the pointer - so at five minutes a mouse move
only lights the screen and the password ends the lock. The
idle chain is `/etc/sway/config.d/23-lazyos-screensaver.conf`; `check-system.sh`
reads it and fails when a step or either wake line is missing, when the pointer
can no longer end the idle screen, and when the 150 s command stops returning at
once (swayidle waits for it, and the lock and the panel-off timer would never
arrive).

A **fullscreen video or presentation** in Firefox or `papers`, and a video in
**mpv**, are exempt: they ask the compositor to inhibit the idle timer while
they are being watched, so a film is not interrupted and the screen stays on.
The automatic half is `/etc/sway/config.d/32-idle-inhibit.conf`; the manual half
is the **coffee cup on the bar**, yellow while the screen is kept awake and dim
when it may sleep. It is waybar's own module and starts no process.

## The first launch

Two caches are built once, inside the image, instead of on your first session:

* **fontconfig's system cache** (`/var/cache/fontconfig`) - without it the first
  terminal to draw a glyph walks every font directory;
* **the icon theme's index** (`/usr/share/icons/LazyOS/icon-theme.cache`) -
  without it the first `rofi -show drun` scans 220 files before it shows a row.

Both are rebuilt by `scripts/rootfs-setup.sh` at build time. `check-system.sh`
warns when either is missing - a rebuilt theme that did not refresh its index is
the silent case, because the launcher still opens, just later.

## Health and backups

```sh
check-system.sh ~/Dev/LazyOS    # OK / WARN / FAIL, one line per check
```

Every check corresponds to a bug this port has already hit, so a `FAIL` is a
regression, not a theory. The image ships the script as
`/usr/local/sbin/check-system.sh` too.

The session half of the report is worth knowing about: it reads the **idle chain**
(150 s screen, 300 s lock, 360 s panel off, both wake lines and the lock before
sleep), the **resident set** (a declined daemon such as `udiskie`, `swaync` or an
indexer is a `FAIL`; a user unit outside PipeWire/WirePlumber/portals/gvfs is a
`WARN`), the **two caches** above, the **non-Latin font**, the **GTK4 files**, the
**overlay** behind the volume and brightness keys, and the **`ip`/`ping`/`ss`**
trio. A `WARN` there usually names a missing piece of the look rather than a
broken machine.

Backups are made of the *parts* a disk cannot be regenerated from:

```sh
sudo ~/Dev/LazyOS/scripts/backup-system.sh /media/you/BACKUP
sudo ~/Dev/LazyOS/scripts/restore-system.sh /media/you/BACKUP /dev/mmcblk0
```

The kernel partition, the partition table, `/boot` and the root filesystem go
into tars (ACLs and xattrs preserved) plus a `README.md` with the restore steps
and a `SHA256SUMS`. `restore-system.sh` wants a disk of the same size and
`CONFIRM=yes`, and refuses to run from the disk it would overwrite.

## Remote login (ssh)

`openssh-server` is installed and `ssh.service` is **enabled on every boot**, on
purpose: it is the recovery path. A machine whose session will not start, or
whose screen is not usable, can still be reached from another computer, and a
card that installs the eMMC is the same system that can rebuild its own kernel
partition - over ssh if that is the only way in.

That also means the machine answers a login prompt on whatever network it joins.
The build's default credentials are **`brunux` / `debian`** with passwordless
`sudo`; change the password on the first day:

```sh
passwd
```

`check-system.sh` prints the fact (`---- remote login is enabled (ssh, …)`) so the
state is never a surprise, and to turn it off:

```sh
sudo systemctl disable --now ssh
```

## Looking at the screen from another machine

`wayvnc` serves this session as VNC, so a screen that is misbehaving can be
looked at from a bigger machine — the other half of what ssh gives you, which is
a shell but not the picture. A rotated screen, a window that will not draw, a
lock screen that came up wrong: all of those are one glance away instead of one
photograph.

**Settings** (Alt+Ctrl+O) → **Share this screen** starts it and says where it is
serving; the same list is in the root menu (Alt+Space), and it is where **Stop
sharing** and **Copy the tunnel command** live. While it is on, an eye appears on
the bar (right click stops it).

It listens on **127.0.0.1:5900 and nothing else**, and it is **read-only**. Both
are decisions, not limitations:

* wayvnc 0.10 authenticates only through `~/.config/wayvnc/config`, where
  `enable_auth` wants a certificate, a private key and a password. This image
  ships no such file, so a server on the network address would be an
  unauthenticated desktop on every network the machine joins.
* the loopback address is reached through the **ssh login the image already
  answers** (`openssh-server` is installed and `ssh.service` enabled on every
  boot), so the tunnel *is* the authentication — using the part that is already
  configured, tested and documented.
* read-only (`--disable-input`) means the viewer cannot type, click or move the
  pointer here. Remote input would arrive in this session as a keyboard nobody
  can attribute, and "see what is on the screen" never needs to touch it.

From the other machine:

```sh
ssh -L 5900:127.0.0.1:5900 brunux@lazyos
```

and then point a VNC viewer at `localhost:5900`: Remmina, TigerVNC's
`vncviewer`, `gvncviewer`, or on macOS the built-in Screen Sharing app
(`vnc://localhost:5900`). The **Copy the tunnel command** row copies that line
with this machine's user and host name already in it. Keep the ssh session open
for as long as you are watching.

A few things follow from the rules above and are worth knowing before debugging
them:

* **No audio.** VNC carries pixels; the sound stays on this machine.
* **Nothing runs until the row is used**, and nothing stays after **Stop
  sharing**. There is no unit and no autostart: `wayvnc` is one process started
  by the menu, and the bar's eye is a state file being read (one `sh` every ten
  seconds), not a daemon.
* **A session that ends takes the state with it** (it lives in
  `$XDG_RUNTIME_DIR/lazyos-vnc`), so a logout cannot leave a stale "on".
* **Port 5900 is the only port.** If something else already listens there, the
  menu says so — with wayvnc's own message — instead of starting half a server.
* `~/.config/bin/rofi-vnc status` prints the state in one line, which is the
  quickest answer to "is this machine being watched right now".

## Suspend and hibernate

Suspend-to-RAM works and resumes cleanly. Two paths lead to it, and they are
different on purpose:

* **The Lock key** is `KEY_SLEEP`, and that is exactly the key systemd-logind
  turns into a suspend on its own. It needs no polkit, no session and no helper,
  which is why it always works. The same key runs `~/.config/bin/sway-lock`, so
  the screen is locked a fraction of a second before the machine sleeps.
* `sway-suspend` (Alt+Shift+Escape locks first, Alt+Ctrl+Escape suspends
  directly) goes through `loginctl suspend`, a D-Bus method guarded by polkit. It
  has a fallback chain and logs to `~/.cache/sway-suspend.log`, so if the session
  is in a state logind does not consider active, it still works.

`sway-lock` is the lock every path goes through, including the 300 s idle timer:
it stops the idle screen first, waits (up to 4 s before a suspend) for
`swaylock`'s own readiness signal, and puts the session's wallpaper behind the
lock instead of a flat colour.

**Hibernate** is offered when it can actually resume:

* a swap on disk (not zram) big enough for the image - the kernel targets 2/5 of
  RAM, so 2 GB is the practical minimum here;
* `resume=` on the kernel command line, so the next boot restores the image
  instead of starting fresh. Because `resume=` is resolved through udev, a
  **label** is enough: a swap partition labelled `lazyos_swap` plus
  `resume=LABEL=lazyos_swap` in `devices/google-asurada/kernel-cmdline.conf`.

The **Auxiliary swap** row in Places (and in the root menu) writes that partition
and the `/etc/fstab` line that activates it on every boot, with priority 10 so
zram keeps being used first. It does not add `resume=`: that is a kernel command
line change and needs the kernel partition rebuilt.

`sway-hibernate` (in the root menu, and as a launcher entry) checks both and says
which one is missing. It never hibernates into a swap the machine cannot resume
from.

## The microSD and the firmware

The internal eMMC and the microSD are written the same way
(`scripts/write-sd.sh`, `scripts/install-emmc.sh`), so a card is a full rescue
system: the same three partitions, the same signed kpart, the same session. The
stock firmware's developer-mode menu picks **internal disk** (the eMMC) or
**external disk** (the card), so a broken install on the eMMC is recoverable by
booting the card and running the installer again.

## Install LazyOS

There is no separate installer program: `scripts/install-emmc.sh` copies **the
system that is running** onto the internal disk, which is why the card is what
you install from. The menu (*Install LazyOS* in the root menu and in the
launcher) is its front end, so what lands on the eMMC is the system that was
tested, not a second implementation of it.

Open it from the microSD. When the machine is running from the disk it would
erase, it says so and offers nothing to press: that is the one mistake nothing
can undo.

The menu is a report before it is a button. It checks the target and the room
the copy needs, the charger and the battery (below half it says so, below 20% on
battery it refuses), the running kernel's own files - the kernel partition is
rebuilt from them *after* the copy, and without them the install would fail with
the disk already formatted - the installer's tools, and that `sudo` does not ask
for a password, because the copy runs detached and nothing can answer a prompt.

**Cancel is the first row**, and it is selected, so a stray Enter writes
nothing. Choosing *Install LazyOS* then asks for the disk's name typed in full
(`mmcblk0` for the eMMC): a list cannot ask that, because Enter on a row is
exactly the gesture that must not erase a disk.

After that it runs the documented install unchanged - `install-emmc.sh` with
`CONFIRM=yes` - and does not partition or format anything itself. Around that
call it does the two things the installer refuses to start with: it stops
**udisks2** and masks it for the length of the run, so nothing can auto-mount
the new partitions mid-write, and it unmounts any partition of the target. Both
are put back afterwards on every exit path, and a failure there stops before a
byte is written. `systemd-inhibit` holds a sleep lock for the length of the copy.

The copy takes minutes, so it is **not stoppable**: a killed `rsync` or `mkfs`
leaves a disk that is neither the old system nor the new one. A notification
card that does not expire and the bar's spinner follow the installer's own
steps, and the card says why it cannot be stopped when it is clicked.

The end is read from the disk and not from the exit code: the three partitions
must carry `pmOS_kernel`, `pmOS_boot` and `pmOS_root`, and the kernel partition
must verify with `futility show`. Only then does the card say to reboot and pick
**internal disk** in the firmware menu.

Every run writes `~/.local/state/lazyos/install-<date>.log`. **Show the install
log** opens it in a window with `less`, following it live while the copy runs
and at the end afterwards; the menu also keeps the last run's outcome under
*last time*, and a `running` line with no process behind it means the run was
killed. When something fails, the card carries the installer's own last `ERROR:`
line and the path of the log.

## Updating

```sh
sudo apt update && sudo apt full-upgrade
sudo ~/Dev/LazyOS/scripts/install-kpart.sh   # if the kernel changed
check-system.sh ~/Dev/LazyOS
```

The image itself is a snapshot: it is rebuilt from the repository, not upgraded
in place forever. Your home and your `apt` changes survive; the shipped
`/etc` files and the session scripts are replaced by the next image, which is why
personal settings belong in `~/.config/`.
