# AGENTS.md

## Project

Build a **Debian arm64** operating system (suite `forky`) for the **Google Hayato** Chromebook
(MediaTek MT8192, board codename `google-asurada`), reusing Nura's hardware support — kernel,
device tree, patches, modules and boot flow — instead of Alpine Linux.

This repo started empty; the scaffolding below is being created incrementally. Treat anything not
yet present in `scripts/` as not implemented.

## Language convention

- **All versioned artifacts are written in English**: source code, comments, commit messages,
  docs, `AGENTS.md`, scripts.
- Conversation with the maintainer may be Spanish and/or English. Artifacts are English only.
- Everything generated is versioned here (build scripts, kernel packaging, device package,
  installer, docs). Do not leave useful work only in `/tmp`.

## Critical upstream facts

- **Nura is postmarketOS renamed** (rebrand 2026). Old names `postmarketOS`, `pmOS`, `pmaports`,
  `pmbootstrap` refer to the same ecosystem; expect them in sources.
- The live wiki (`wiki.nura.eco`, `wiki.postmarketos.org`) is behind **Anubis** (proof-of-work
  anti-bot): `webfetch` returns a challenge page, not content. Use `git clone` from
  `gitlab.postmarketos.org` (the git protocol is **not** blocked) or the Wayback Machine.
- The `gitlab.com/postmarketOS/*` mirror is **stale**. Always prefer
  `gitlab.postmarketos.org/postmarketOS/*`.

## Target hardware (the install machine)

| Item | Value |
|---|---|
| Model | Google Hayato rev1 (ASUS Chromebook Detachable / Flip CM3 CM3200) |
| SoC | MediaTek MT8192 "Asurada" (4×A76 + 4×A55), Mali-G57 MC5 (Panfrost) |
| RAM | 4 GB |
| Storage | eMMC 58.2 GB (`mmcblk0`) + microSD (`mmcblk1`) |
| WiFi/BT | Realtek RTL8822CE (`rtw88_8822ce` + `rtl_bt`) |
| Input | `cros-ec-keyb` + Elan `04F3:2E70` (i2c-hid) |
| Firmware | stock coreboot/depthcharge, developer mode, vboot2 |

## Boot chain (do not get this wrong)

- Stock ChromeOS firmware (**depthcharge**) loads a **signed FIT "kpart"** from the GPT partition
  whose PARTUUID equals the `kern_guid` in the kernel command line (`p1` in Nura's layout).
- The kpart is built with **`depthchargectl build --board mt8192`** (`depthcharge-tools`) and
  signed with the **vboot developer keys** (`/usr/share/vboot/devkeys`).
- Partitioning uses **`cgpt`** (ChromeOS GPT). The 128 MB kernel partition is required.
- **Never flash the SPI firmware.** Only touch GPT partitions. Write-protect is disabled on this
  device, so a firmware write could brick it.
- Boot from microSD/USB requires `crossystem dev_boot_usb=1 dev_boot_signed_only=0`; the firmware
  then boots the external disk when it is picked in the developer-mode menu (see below).

Nura's proven disk layout (reuse it, and this repository reproduces it): `p1 pmOS_kernel` 128 MB
(kpart), `p2 pmOS_boot` 512 MB (ext2, where `/boot` lives), `p3 pmOS_root` root. It is created in
exactly one place, `scripts/lib-cgpt.sh`, which `write-sd.sh`, `install-emmc.sh` and
`install-kpart.sh` share; the kernel partition flags (`-S 1 -T 5 -P 10`), the partition types
(`kernel`, `efi`, `data`), the labels and the 512 MB boot size are the ones pmbootstrap uses for
ChromeOS (`pmb/install/partition.py`, whose `Config.boot_size` default is 512).

## Repository layout (device-agnostic)

The build is generic on purpose: any device supported by Nura can be ported by adding a
directory, and kernels are shared between devices.

```
devices/<codename>/        one directory per device
  deviceinfo               pmaports-style device description (sourceable shell)
  kernel                   kernel flavor to use (name under kernels/)
  kernel-cmdline.conf      base kernel command line
  modules-initfs           modules forced into the initramfs
  packages.list            device-specific Debian packages
  APKBUILD.reference       upstream device package (reference only)
kernels/<flavor>/          one directory per kernel
  version                  upstream version (e.g. 7.2.7)
  config.aarch64           kernel config (CONFIG_LOCALVERSION gives the release suffix)
  patches/                 *.patch + a `series` file with the apply order
  APKBUILD.reference       upstream kernel package (reference only)
uis/<ui>/                  one directory per graphical shell
  packages.list            UI packages (Debian names)
  files/                   tree overlaid onto the rootfs; `etc/` is copied as
                           root, `home/` is merged into /home/$USERNAME
  setup.sh                 optional; run inside the chroot after files/ is
                           overlaid, for what a plain tree cannot express
                           (seeding ~/.config from a vendored tree, symlink farms)
  README.md                what the shell ships and how it deviates upstream
packages/                  device-agnostic base package lists (base.list)
scripts/                   lib.sh (device/kernel/UI resolution), lib-cgpt.sh
                           (the ChromeOS GPT layout), lib-dist.sh (what a
                           published artifact must not carry) + build/install
                           scripts, including make-image.sh / make-release.sh
reports/                   check-system.sh output captured on real hardware, kept
                           as evidence for the checks (see reports/README.md)
docs/                      plan.md, recon.md, releases/<version>.md (release
                           notes, reused as the tag message and the GitHub
                           release body), screenshots/
```

Select a device with `DEVICE=<codename> scripts/...`; with a single device it is picked
automatically. Select the graphical shell with `UI=<name>` (default `sway`).

### The image carries this repository

`build-rootfs.sh` stages the **committed** tree (`git archive HEAD`, so never an untested working
copy) into the image at `/home/$USERNAME/Dev/<repo directory name>`, next to a `REVISION` file, and
writes the same information to `/etc/debian-aarch64-revision` (commit, device, UI, kernel, date).
It also installs `check-system.sh` as `/usr/local/sbin/check-system.sh`. This is not decoration:
`install-emmc.sh`, `install-kpart.sh` and the package checks in `check-system.sh` are meant to run
**on the device**, they source `scripts/lib.sh`, and the card that installs to the eMMC is the same
system that has to rewrite its own kernel partition later. A copy found on a card can therefore be
told apart from the checkout it came from - check `REVISION` before trusting it. It is a snapshot:
update it by rebuilding, not by editing it in place. The script warns when the working tree has
uncommitted changes, since those do not travel into the image.

## Vendored reference (pinned)

- Upstream: `pmaports` commit `0e458a2417d6f5ed61c7b70621d5632e30136091` (2026-10-15).
- `kernels/mediatek-mt81/` — mainline Linux **7.2.7** from `cdn.kernel.org` + patches + config.
  Built with `make LLVM=1` (clang/lld).
- `devices/google-asurada/` — `deviceinfo`, `kernel-cmdline.conf`, `modules-initfs`.

## Conventions and gotchas

- The shell in this environment is **ash (busybox)**: no brace expansion (`{a,b}`), no `[[ ]]`.
  Write POSIX sh, or call `bash` explicitly.
- Build/run natively on this aarch64 machine; no cross-compiler needed.
- **`/tmp` is a ~1.9 GiB tmpfs** — too small for kernel/rootfs work. Keep large artifacts on
  the real filesystem; the scripts default to `$HOME/.cache/debian-aarch64`
  (`BUILD_ROOT`, `WORK_DIR`, `SRC_DIR`). `/tmp/opencode` is fine only for small scratch files.
- **Never edit a script while it is executing.** The shell re-reads the file as it goes and can
  die with `unexpected EOF while looking for matching '"'`. Write the new version elsewhere and
  `mv` it into place.
- **`pipefail` is set in every script here, so never test the status of `producer | grep -q PAT`**:
  `grep -q` exits on the first match, the producer then dies of SIGPIPE (141) and the pipeline -
  and with it the `if` - reports failure although the pattern *was* found. It depends on how far
  the producer got before the pipe closed, so the same input passes on a fast machine and fails on
  a slow one (`check-system.sh`'s firmware check reported a loaded firmware as missing on the card
  and as present on the host). Capture the output first and use `grep -q PAT <<<"$var"`, or a
  single `grep -rqs PAT <path>`.

### Host toolchain

- `scripts/00-deps.sh` installs the same tool set from either package manager, because the
  reference machine runs this port (Debian) as well as having built it from Alpine: `apk` is used
  when present, `apt-get` otherwise. The Debian names differ in a few places (`rust-bindgen` ->
  `bindgen`, `elfutils-dev` -> `libelf-dev`, `pahole` -> `dwarves`, `vboot-utils` ->
  `cgpt` + `vboot-kernel-utils`, `dtc` -> `device-tree-compiler`, `gptfdisk` -> `gdisk`,
  `musl-dev` -> glibc via `build-essential`). On Debian the kernel's rust needs (`rustc`,
  `rust-src`, `bindgen`, `libclang-dev`) and BTF (`dwarves`) are part of that list too.
- The kernel build also needs the **`openssl`** binary (module signing) and **`findutils`** (the
  kernel's initramfs scripts use `find -printf`, which BusyBox find lacks).

### Debian naming

- `deviceinfo_arch` uses distro/Alpine names (`aarch64`), but **debootstrap requires the Debian
  name** (`arm64`). `lib.sh` maps between them; passing `aarch64` yields
  `Invalid Release file, no entry for main/binary-aarch64/Packages`.

### Kernel staging

- `make zinstall` installs the image as **`vmlinuz-<release>`** only; there is no plain `vmlinuz`
  in the staged tree.
- `CONFIG_MMC_MTK`, `CONFIG_DRM_MEDIATEK` and `CONFIG_EXT4_FS` are built-in (`=y`), so the root
  filesystem and the display work without any initramfs module.
- `CONFIG_DRM_PANFROST` (the driver this MT8192 uses) and `CONFIG_DRM_PANTHOR` (newer Mali) are both
  built-in, so every `update-initramfs` prints
  `W: Possible missing firmware /lib/firmware/arm/mali/arch14.8/mali_csffw.bin for built-in driver
  panthor`. It is noise, not a missing piece: initramfs-tools scans built-in modules for firmware
  requirements, and panthor never drives the Mali-G57 here.
- `CONFIG_EXT2_FS` is **not set**, and the `pmOS_boot` partition is formatted ext2 anyway
  (`scripts/lib-cgpt.sh`). That works because `CONFIG_EXT4_USE_FOR_EXT2=y`: the ext4 driver registers
  the `ext2` and `ext3` types, so `mount -t ext2` is served by ext4 and `/proc/filesystems` lists
  ext2/ext3/ext4. Nura's own `/boot` is mounted the same way. Do not "fix" this by enabling
  CONFIG_EXT2_FS.
- Not every `Direct firmware load ... failed` line is a missing firmware. `CONFIG_MTK_SCP=m`, and
  the SCP remoteproc asks for `mediatek/mt8192/scp.img` while only the initramfs exists, so every
  boot logs two `remoteproc remoteproc0: Direct firmware load for mediatek/mt8192/scp.img failed
  with error -2` and one `request_firmware failed: -2`; three seconds later the module is loaded
  from the real root filesystem and the log continues with
  `Booting fw image mediatek/mt8192/scp.img, size 309164` and `SCP is ready. FW version
  asurada_scp_v2.0.20536`. The file is installed (`firmware-mediatek`) and Nura loads the very same
  size, so this is the module being probed after the rootfs, not a missing piece; every function
  this port uses (display, audio, Wi-Fi, Bluetooth, suspend) works with the SCP coming up late.
  `check-system.sh` therefore reports the outcome of the load, not the first attempt.

### Userspace / UI (Debian forky)

- **greetd**: Debian's package creates the **`_greetd`** system user (upstream docs say
  `greetd`), and its unit declares `Conflicts=getty@tty7.service`, i.e. it expects `vt = 7` in
  `/etc/greetd/config.toml`. With the upstream name the service dies with
  `configured default session user 'greetd' not found` and the machine silently falls back to a
  console login on tty1.
- **SSH host keys**: Debian's `sshd-keygen.service` is gated on `ConditionFirstBoot=yes`, i.e. it
  generates keys only when `/etc/machine-id` was *empty at boot*. Every system here is a *copy* of
  an image (`write-sd.sh`, `install-emmc.sh`), and the copy is written from a tree that already
  generated its id, so the condition is never true again, `ssh-keygen` never runs and
  `ssh.service` dies on every boot with `sshd: no hostkeys available -- exiting.` (hit on both the
  card and the eMMC install; the card kept failing for as long as it had been booted).
  `rootfs-setup.sh` therefore drops in
  `/etc/systemd/system/sshd-keygen.service.d/90-run-when-keys-are-missing.conf`, which resets the
  inherited condition list (`ConditionFirstBoot=`) and requires the ed25519 host key to be
  **absent** instead: the unit generates whatever key type is missing on the first boot that lacks
  them and is a single `stat` from then on. `build-rootfs.sh` ships `/etc/machine-id` empty and no
  host keys, and `install-emmc.sh` blanks the id and the keys of the copy, so each card and each
  install gets its own identity instead of sharing one journal, one DHCP identity and one SSH
  host key (`lib-dist.sh` makes the same choice for published artifacts).
- `nmtui` lives in the separate **`network-manager-tui`** package (no longer part of
  `network-manager`).
- `wireless-regdb` provides `regulatory.db`; without it cfg80211 logs
  `failed to load regulatory.db` and the WiFi stays on the world regulatory domain. `iw` is in
  `packages/base.list` for `check-system.sh`: without it the wireless interface and the regulatory
  domain are invisible to the report (it SKIPs both checks). The world domain (`country 00`) on its
  own is *not* a failure - it is what remains until NetworkManager or the access point's 802.11d
  country IE sets one (Nura reaches `country MX` from the AP), so the report says so instead of
  warning.
- `zstd` lets initramfs-tools compress the initramfs with zstd; otherwise it warns
  `No zstd in /usr/bin:/sbin:/bin, using gzip`.
- **Bluetooth**: `libspa-0.2-bluetooth` provides PipeWire's BlueZ SPA plugins, but those are
  only a D-Bus client of `org.bluez`. The `bluez` package is what supplies `bluetoothd`,
  `bluetoothctl`/`btmgmt` and the `bluetooth.service` unit; without it the HCI controller
  (`hci0`, bound to `serial0-0` via `hci_uart` in H5 mode, RTL8822CS firmware loaded) is
  never powered up and there is no Bluetooth at all — no scanning, no pairing, and the SPA
  plugins have nothing to talk to, so BT audio silently does not exist. `bluez` is therefore
  in `uis/sway/packages.list` alongside the SPA plugin. Debian 5.87 has **no `bluez-utils`**
  (the CLI tools ship in `bluez`; `bluetoothd` is `/usr/libexec/bluetooth/bluetoothd`).
  Verify with `bluetoothctl show` (*Powered: yes*) and `btmgmt info`.
- **Audio (MT8192)**: the card `mt8192_mt6359_rt1015p_rt5682` is only usable once its UCM
  **boot sequence** has run. That sequence (`MediaTek/mt8192/.../init.conf`) connects the
  front-end and back-end DAIs (`I2S3_CH1 DL1_CH1`, …); alsa-lib runs `BootSequence` **only**
  on an explicit `set _boot` — never at open or on a verb set — so something must drive it.
  `/usr/sbin/alsactl` does: on card add `90-alsa-restore.rules` and `alsa-restore.service`
  run `alsactl restore`, which for a card with no saved state falls back to the UCM init.
  Without **`alsa-utils`** the PCMs fail to open with `ASoC: no backend DAIs enabled for
  Playback_*`, ACP marks the `HiFi` profile unsupported, WirePlumber picks profile `off`, and
  the only sink is a **Dummy Output** (volume keys change nothing, no speaker sound).
  `alsa-utils` is therefore in `packages/base.list`. Verify with `pactl list cards` (Active
  Profile `HiFi`) and `wpctl status` (a *Built-in Audio Speaker* sink, not *Dummy Output*).
  Confirmed on hardware: the speaker plays a video.
  The same class of bug will hit any card whose routing lives in a UCM `BootSequence`.
- **ALSA-only applications**: Debian's stock alsa-lib configuration has no usable `default`
  PCM; that device is supplied by **`pipewire-alsa`** (in `uis/sway/packages.list`). Without
  it, `aplay`/`arecord` without an explicit `-D`, emulators and some Java/game audio fail
  with *audio open error: No such file or directory* while PipeWire clients (and therefore
  everything using PulseAudio) work fine, which makes the gap easy to overlook.
- **Clock**: Debian enables no NTP client, so a fresh image never synchronises
  (`timedatectl` says *System clock synchronized: no*, *NTP service: n/a*) and only the RTC
  keeps time. `systemd-timesyncd` is in `packages/base.list` for this.
- **Battery gauge**: the fuel gauge (`sbs-battery 8-000b`) registers fine (`battery gas gauge
  device registered`) and its capacity is readable, but it reports `Unknown chemistry: OTC0` and
  the kernel repeatedly logs `power_supply sbs-8-000b: driver failed to report
  'status'/'health'/'time_to_empty_now' property: -5/-6`. This is **upstream, not this port**:
  Nura's own log carries the same messages (1296 of them). What waybar shows is the capacity,
  which works, because its `battery` module reads `/sys/class/power_supply` directly
  (`"bat": "sbs-8-000b"`, see `waybar-battery(5)`) — `upower` is *not* what draws it.
  `upower` is in `uis/sway/packages.list` anyway: WirePlumber asks it for the battery
  percentage, and without the daemon every start logs
  `default: Failed to get percentage from UPower: NameHasNoOwner`.
- **Backlight keys**: `brightnessctl` needs write access to
  `/sys/class/backlight/*/brightness`. Debian's `brightnessctl` ships no udev rule and
  nothing else in the system grants it, so `/sys/class/backlight` stays root-only (0644) and
  the sway `XF86MonBrightness*` bindings fail with *Permission denied* for the session user.
  `uis/sway/files/etc/udev/rules.d/90-brightnessctl.rules` (group `video`, `g+w`) fixes it.
- **Lock key**: the right-most key of the top row emits `KEY_SLEEP`, and that is precisely the
  key systemd-logind turns into a suspend on its own — its button handling consults no polkit and
  no session, which makes it the only suspend path that always works.
  `uis/sway/files/etc/keyd/default.conf` therefore leaves the key alone, and
  `uis/sway/files/etc/sway/config.d/21-lock-screen.conf` binds the keysym the same keycode
  produces (**`XF86Sleep`**) to `swaylock`: the key locks the screen and logind suspends the
  machine ~0.3 s later.
  Nura instead maps `sleep = coffee` — `KEY_COFFEE`, the kernel's lock/screensaver key, whose
  xkb keysym is `XF86ScreenSaver`. Because keyd grabs the keyboard, logind then never sees
  `KEY_SLEEP`, and since nothing consumes `KEY_COFFEE` either, the Lock key there does
  *nothing* (elogind handles only `KEY_SLEEP`, `KEY_SUSPEND`, `KEY_POWER`, `KEY_POWER2`,
  `KEY_RESTART` and `KEY_ESC`). **That remap is what broke suspend on this image**: the first
  boot suspended on the key, and it stopped the moment keyd started grabbing the keyboard
  (`journalctl` shows `Suspend key pressed short.` in the early boots and not one line of it
  afterwards, while `keyd.rvaiya` logs `DEVICE: match ... (cros_ec)`), so the mapping is left
  out.
  Confirmed on hardware: the first report from a booted card records `5 suspend/resume cycle(s)`
  and the Lock key locks the screen and suspends the machine.
  The session can still suspend by itself — `Alt+Shift+Escape` (lock, then sleep; Nura's
  shortcut) and `Alt+Ctrl+Escape` (sleep without locking) run `~/.config/bin/sway-suspend` —
  but that goes through `loginctl suspend`, a D-Bus method guarded by polkitd (stock
  `org.freedesktop.login1.suspend` has `allow_active=yes`), which fails with *Access denied*
  and no output from a session logind does not consider active. The helper therefore walks a
  fallback chain (`sudo -n loginctl suspend`, then `echo mem > /sys/power/state`) and logs
  every attempt to `~/.cache/sway-suspend.log`. Suspending through logind needs the daemon
  (D-Bus activated; Debian keeps `systemd-logind` inside the `systemd` package).
  Reading a keymap dump is misleading here: `xkbcli compile-keymap` prints **X** keycodes,
  which are evdev + 8, so `KEY_SLEEP` (evdev 142) shows up as keycode 150 and
  `KEY_BRIGHTNESSDOWN` (evdev 224) as keycode 232.

- **`uis/sway/`** ports the maintainer's Nura session (solarized theme, Alt as `$mod`, Search
  key opens bemenu, waybar, Alacritty). `rootfs-setup.sh` rewrites `/home/brunux` inside the
  shipped text files to `$USERNAME`, and merges `uis/sway/files/home/` into that home.
- **tuigreet**: Debian ships the *original* tuigreet **0.9.1**, which has no `--config`/TOML
  support and no background animations (Nura runs the fork, which has both). The greeter's
  personalisation is therefore expressed as flags in `/etc/greetd/config.toml`; the `matrix`
  backdrop and the `custom_title`/widget positions have no 0.9.1 equivalent. Unknown
  components or colour names in `--theme` are silently ignored (they do not break the
  greeter), and `--remember*` needs `/var/cache/tuigreet` owned by the greeter user.
- **The Chromebook keyboard has no F1-F10**, and that is what breaks virtual terminals:
  `mt8192-asurada-hayato-r1.dts` maps the top row to action keys (`KEY_BACK`, `KEY_FORWARD`,
  `KEY_REFRESH`, `KEY_FULL_SCREEN`, `KEY_SCALE`, brightness, mute, volume). Switching VTs needs a
  real F-key keycode: the kernel maps `Ctrl+Alt+F<n>` to `Console_<n>`, and sway reacts to the
  `XF86Switch_VT_<n>` keysym that the same keycode produces (`srvr_ctrl(fkey2vt)` is part of the
  default xkb keymap). `uis/sway/files/etc/keyd/default.conf` (package `keyd`) supplies them:
  `Ctrl+Alt`+top row = VT 1-10, `Search`+top row = F1-F10. Debian names the binary
  **`/usr/bin/keyd.rvaiya`** (the package enables `keyd.service`, and `/etc/keyd/` ships empty).
  keyd reads its config **only at startup** — `keyd.rvaiya reload` after editing — and
  `backspace+escape+enter` makes it quit and release the keyboard.
- **keyd must not grab `cros_ec_buttons` or the headset jack**: both share the keyboard's
  `0000:0000` id, and keyd counts them as keyboards because they report `KEY_VOLUMEUP/DOWN` (its
  heuristic for laptops whose hotkeys live on a separate node). keyd's virtual device does not
  forward `EV_SW`, so grabbing them would silently kill suspend-on-lid-close, the tablet-mode
  switch and the power button (logind loses it). `default.conf` therefore excludes them; the
  exclusions must precede `k:0000:0000`, because keyd returns on the first matching entry, and
  they are hashes of the device name, its key count and the abs/rel masks (`keyd.rvaiya monitor`
  prints them, and `journalctl -u keyd` logs what got matched).
- **Nerd Fonts / waybar icons**: Debian has no Nerd-patched Adwaita Mono, so the theme uses
  `JetBrains Mono` for the text. Beware `fonts-nerd-symbols`: it is **DFSG-stripped** and only
  ships Octicons, Codicons, Devicons, Seti and Powerline. It has **no Font Awesome
  (U+F000-U+F2FF) and no Material Design Icons (plane 15)** — which is exactly what the theme's
  icons are — so on its own every icon renders as a *tofu* box with the hex codepoint inside.
  `fonts-font-awesome-legacy` and `fonts-materialdesignicons-legacy` supply those two sets and
  fontconfig's fallback then resolves every glyph, with no change to the configuration.
  Verify with the engine waybar actually uses:
  `pango-view --no-display --font="JetBrains Mono 13" --output=/tmp/i.png <text>` (package
  `pango1.0-tools`) and look at the PNG.
- **Wi-Fi profiles**: Nura and Debian name the wireless interface differently (Nura: `wlan0`,
  Debian/systemd: `wlp1s0`), so a NetworkManager profile copied verbatim is never activated.
  `scripts/wifi.sh import` drops `interface-name=` to bind the profile to any Wi-Fi device. The
  profiles hold the PSK in clear, so they live in `$WIFI_DIR` (`$BUILD_ROOT/wifi`) and are
  git-ignored; `build-rootfs.sh` imports them automatically when that directory exists.
- **sway `-C` validates headlessly** even inside a chroot (useful before booting):
  `WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 sway -C -c /etc/sway/config`.
- **Xwayland**: sway starts it on its own (it is not in the sway configuration, so the "every
  command the configuration invokes is installed" check does not see it). Without the `xwayland`
  package each session logs exactly two red lines —
  `[wlr] cannot find xwayland binary "/usr/bin/Xwayland"` and
  `[sway] Failed to start Xwayland` — and X11-only programs (emulators, Wine, some Java/Steam
  games) cannot run at all, while everything Wayland-native works. Nura ships Xwayland, so
  `xwayland` is in `uis/sway/packages.list` (25 extra packages: the X libraries);
  `check-system.sh` warns when it is missing, unless the session deliberately disables it with
  `xwayland disable`.
- **workstyle** (renames workspaces to icons) is not packaged in Debian; Nura ran a cargo build
  linked against musl, which cannot run against glibc. Its drop-in is not shipped, but
  `~/.config/workstyle/config.toml` is kept so `cargo install workstyle` is enough to enable it.
- `bemenu` in Debian pulls one backend through an alternative dependency; add
  **`libbemenu-wayland`** explicitly for the Wayland session. `grimshot` is a package of its own.
  `pactl` (the XF86Audio* keys) comes from `pulseaudio-utils`, not from `pipewire-pulse`.

### ChromeOS GPT (`cgpt`)

- **Do not pass `-z` to `cgpt create`**: it zeroes the GPT areas *after* writing them, so the
  following `cgpt add` fails with `GptSanityCheck() returned 2: GPT_ERROR_INVALID_HEADERS`.
- **`cgpt` does not write a protective MBR**, and the Linux kernel refuses to parse a GPT without
  one (`0xAA55` signature plus a `0xEE` entry). Run `parted -s <dev> mktable gpt` first — this is
  why pmbootstrap does it too — then `cgpt create` / `cgpt add` on top of it.
- After writing the table, refresh the kernel's view with `partx -d` + `partx -a`
  (`blockdev --rereadpt` returns `EBUSY` on this microSD card).
- `lsblk -nro PATH <disk>` lists the **disk first**, then its partitions: skip the first line when
  resolving `p1`/`p2`/`p3`. A wrong index here formats the whole disk instead of a partition.
- The tools needed **on the device** are three separate Debian packages and it is easy to install
  the wrong one: `cgpt` provides `cgpt`, **`vboot-kernel-utils`** provides `futility` (the
  similarly named `vboot-utils` ships `crossystem` and symlinks to `futility` but not `futility`
  itself), and `depthcharge-tools` provides `depthchargectl`/`mkdepthcharge`. All three are in
  `devices/google-asurada/packages.list`, together with `rsync`, which `install-emmc.sh` needs.
- **`depthcharge-tools`' system hooks are disabled in the image** by
  `/etc/depthcharge-tools/config` (`[depthchargectl/write] enable-system-hooks = False`, written by
  `rootfs-setup.sh`). The package ships `/etc/kernel/postinst.d/zz-depthcharge-tools` and
  `/etc/initramfs/post-update.d/depthcharge-tools`, which build a depthcharge image and write it to
  a kernel partition on every kernel or initramfs change. They know nothing about this port (the
  board `mt8192` and the command line come from our `deviceinfo`, applied by `make-kpart.sh`, and
  the write is `install-kpart.sh`), and enabled they also break the **build**: `update-initramfs`
  runs in the chroot, where `/proc/cmdline` is the host's — Nura's, which contains `cros_secure` —
  so the hook thinks it is on a depthcharge system, finds no kernel partition and exits 1, and
  `run-parts` then fails `install-kernel.sh`. The `enable-system-hooks` check reads section
  `depthchargectl/write`, not the `[depthcharge-tools]` section the package's own `config.ini` uses.
- A kernel that boots from a ChromeOS kernel partition lives **inside** that partition, so
  installing a new kernel is not finished until p1 is rewritten: `scripts/install-kpart.sh` does
  that (the Debian counterpart of boot-deploy's `flash_updated_depthcharge_kernel`, which reads
  `kern_guid` from `/proc/cmdline`, resolves `findfs PARTUUID=<guid>`, checks the image fits the
  partition and `dd`s it). On the installed system the sequence is
  `scripts/install-kernel.sh /` (staged kernel into the running rootfs) followed by
  `scripts/install-kpart.sh`; without the second step the machine keeps booting the old kernel
  while `/boot` and `/lib/modules` move on.
  Its chroot guard must keep `stat -L` **and** the comparison in that sense: without `-L`, `stat`
  reports the procfs symlink itself (device `0:28`), which can never equal `/`, so the guard passed
  silently; and *equal* is the state of a real system (this root is PID 1's root), so a chroot is when
  the two **differ**. As it was written the guard never fired, and a chroot would have rewritten the
  *host's* kernel partition from the host's `kern_guid`. Both sides are now read before refusing, so a
  system where `/proc` hides PID 1 is not mistaken for a chroot.

### pmbootstrap parity (the install method that is known to boot here)

`scripts/lib-cgpt.sh` reproduces pmbootstrap's ChromeOS install path part by part, because that
method is the one proven on this Chromebook. Read the upstream before changing it:
`pmb/install/partition.py` (`partition_cgpt`, `sanity_check_boot_size`), `pmb/install/format.py`
(`format_partition_with_filesystem`, `format_and_mount_*`), `pmb/install/_install.py`
(`create_fstab`, `copy_files_from_chroot`, `write_cgpt_kpart`, `partprobe`), `pmb/install/blockdevice.py`
(`mount_disk`) and `boot-deploy-functions.sh` (`create_depthcharge_kernel_image`,
`flash_updated_depthcharge_kernel`, `is_in_chroot`, `check_image_size`). Verified on a loop device:
the resulting geometry is identical (`p1 8192+262144`, `p2 270336+1048576`, `p3 1318912+rest-33`,
PMBR present, kernel/efi/data types, `pmOS_*` labels, `-S 1 -T 5 -P 10`).

- The **root partition size is `dev_size - root_start - 33`**, not `- 2048`: those 33 sectors are
  the secondary GPT (32) plus its header, and pmbootstrap sizes the partition so the backup table
  survives. `partprobe` runs after the `cgpt add`s, as in `install_system_image()`.
- **`mkfs.ext2 -F -q -L pmOS_boot` and `mkfs.ext4 -F -q -L pmOS_root`**, the lines
  `format_partition_with_filesystem()` builds for this device (its schema default for
  `boot_filesystem` is ext2, so `BOOT_FS=ext2` is compliance, not a quirk).
- **`/etc/fstab` options are pmbootstrap's**: `defaults` for the root and `nodev,nosuid,noexec` for
  `/boot` — which is also how the running Nura mounts its `/boot`. Do not "improve" them without a
  reason; the `pmos_rootfsopts`-equivalent a pmOS user would see comes from that column.
- **`disk_release` refuses when a partition of the target is mounted**, like `mount_disk()`, rather
  than unmounting it silently. Active swap is turned off (pmbootstrap does not check it, but `mkfs`
  on a live swap partition fails) and no `wipefs` is done: parted writes both GPT tables and
  `mkfs -F` overwrites signatures anyway.
- **`mountpoint -q <device>` does not test whether a device is mounted** — it answers whether that
  *path* is a mount point, which a device node never is. It answered "no" for a mounted partition, so
  the old release step never unmounted anything and the new refusal would never have fired. Test with
  `findmnt -rn -S <device>` (or `/proc/mounts`), never with `mountpoint`, when the argument is a
  device; `mountpoint -q <directory>` (as in `disk_layout_umount`) is fine.
- **Partitions are resolved by number, never by their position in `lsblk`.** `lsblk` lists a disk's
  partitions in the order the kernel handed out their minors, and the `partx -d`/`-a` round that
  `disk_layout_create()` runs to make the kernel re-read the table it was just given reorders them.
  Measured over 16 attaches of a 2048 MiB loop: 3 came back as `3 1 2` or `2 1 3` with no line
  missing, so the positional code would format `p2` as the root filesystem and write the kpart into
  `p3` — a card written wrong a fifth of the time, silently. It is not `partprobe`: in an A/B of 8
  attaches each, the order drifted once with it and twice without it. `disk_layout_parts()` uses
  `PATH,PARTN` and asserts the three `pmOS_*` labels, so a wrong resolution dies instead of touching
  the wrong partition: 16 of 16 attach correctly, including the two that came back reordered.

Intentional deviations, all of them at the line that makes them: the GPT areas are zeroed before
`parted` (pmbootstrap leaves it to parted; it costs nothing and makes `cgpt create` immune to a stale
header), `mkfs` does not get `-i 8192` (that is for pmbootstrap's image builds; our images are 4 GiB
with ~90k files), the fstab pass column is `0 1`/`0 2` instead of `0 0` (systemd-fsck orders them),
the kernel command line carries `root=UUID=... rootwait rw` instead of pmOS's `pmos_root_uuid=...`
(Debian resolves its root from `root=`), and `install-emmc.sh` copies `/home` with the rest of the
running system instead of seeding it from `/etc/skel`, because the point of that script is to clone
the card that has already been tested.

### depthcharge FIT

- Build with `deviceinfo_depthcharge_board="mt8192"` (not `asurada`: the hayato DTB has no
  `google,asurada` compatible). `mkdepthcharge` prepends `kern_guid=%U` by itself, so the cmdline
  passed to `depthchargectl` must not add it again.
- Verify with `futility show <kpart>`: expect a dev-signed key block, preamble 2.2 and
  `Body verification succeeded`.
- The FIT carries one config per DTB. With `--board mt8192` no config matches by `compatible`, so
  depthcharge falls back to the **default config, which is the first DTB**: hayato must stay
  first (the alphabetical order of `deviceinfo_dtb="mediatek/mt8192-asurada*"` does that).
- The output is silent by design (`quiet loglevel=2`). For a verbose boot without touching
  `kernel-cmdline.conf`:
  `KERNEL_CMDLINE_OVERRIDE="console=tty0 ignore_loglevel loglevel=7" scripts/make-kpart.sh`
  and write the resulting kpart to the kernel partition (or `scripts/install-kpart.sh` on the
  running system). Expect the `sbs-8-000b` gauge spam on screen while verbose. Never append a
  bare `debug`: initramfs-tools then redirects its own script output to
  `/run/initramfs/initramfs.debug` (a file *inside* the initramfs) instead of the console, so the
  messages that identify a boot failure (`run-init: ...`, `Target filesystem doesn't have ...`,
  `ALERT! ...`) never reach the screen. `fbcon` is built into this kernel, which is what makes
  `console=tty0` useful here.

## Commands

All scripts live in `scripts/` and read shared config from `scripts/lib.sh`.
Large artifacts go to `$HOME/.cache/debian-aarch64` (`WORK_DIR`, `SRC_DIR`).

```sh
scripts/00-deps.sh          # install host build deps (apk or apt), re-execs via sudo
scripts/build-kernel.sh     # mainline 7.2.7 + patches + config  -> $WORK_DIR/kernel-stage
scripts/build-rootfs.sh     # debootstrap Debian forky rootfs    -> $WORK_DIR/rootfs
scripts/install-kernel.sh   # copy staged kernel into the rootfs, depmod, initramfs
scripts/make-kpart.sh       # signed depthcharge FIT kpart       -> $WORK_DIR/vmlinuz.kpart
scripts/write-sd.sh [dev]   # partition+populate a microSD/USB (default /dev/mmcblk1)
scripts/make-image.sh       # distributable disk image from the rootfs (no card needed)
scripts/make-release.sh     # every release asset + SHA256SUMS -> $WORK_DIR/release-<device>
scripts/install-emmc.sh     # install the running system onto /dev/mmcblk0 (CONFIRM=yes)
scripts/install-kpart.sh    # rewrite the kernel partition this system booted from
scripts/backup-system.sh    # kernel partition + GPT + /boot + rootfs tars into a directory
scripts/restore-system.sh   # put such a backup back onto a disk (root only, from the live SD)
scripts/wifi.sh export      # saved NetworkManager Wi-Fi profiles -> $WIFI_DIR
scripts/wifi.sh import      # $WIFI_DIR -> a rootfs (drops interface-name=)
scripts/check-system.sh     # (on the booted image) hardware + configuration report

scripts/build.sh            # full pipeline (kernel + rootfs + kernel + kpart)
```

`scripts/lib-cgpt.sh` holds the one copy of the ChromeOS GPT layout, used by `write-sd.sh`,
`install-emmc.sh`, `install-kpart.sh`, `make-image.sh` and `make-release.sh`; `BOOT_SIZE_MIB` (512)
and `BOOT_FS` (ext2) override its boot partition.

`scripts/make-image.sh` reuses that layout on a loop device, so an image is built exactly like a
card is written, with one difference: its size is an argument (`IMAGE_SIZE_MIB`, default 4096)
instead of the device's, and the copy it publishes is sanitized. `scripts/lib-dist.sh` is the only
place that decides what a published artifact may contain, and `dist_assert_clean` refuses the
artifact while anything is left; the release rule is:

- **Never publish an artifact straight out of `$ROOTFS`.** The build rootfs carries the
  maintainer's Wi-Fi profiles (PSKs in clear, imported from `$WIFI_DIR`), the SSH host keys and
  the machine id. `make-image.sh` and `make-release.sh` sanitize a copy and assert it clean;
  anything that publishes a rootfs has to go through `lib-dist.sh` too.
- The released image therefore comes up with no known Wi-Fi network, `ssh` host keys are generated
  on first boot (`sshd-keygen.service`) and `systemd` gets a fresh `machine-id`.
- A release is: `scripts/make-release.sh`, notes in `docs/releases/<version>.md` (committed, and
  reused as the annotated tag message and the GitHub release body), and the assets attached to the
  tag. `docs/releases/v0.1.0.md` is the first one and shows the shape.

`scripts/backup-system.sh` is the counterpart of `install-emmc.sh`: it writes the kernel partition
the system booted from (resolved from `kern_guid`, like `install-kpart.sh`), the GPT, `/boot` and
the root filesystem into a directory, the filesystems as gzip tars with owners, ACLs and xattrs
preserved, plus a `README.md` with the restore steps and a `SHA256SUMS`. The destination is
normally a removable disk with **exFAT**, the one filesystem both macOS and Linux read; exFAT
cannot hold a Linux root filesystem by itself, which is why the trees travel as tars. Caches,
build directories and the agent's session data are excluded by pattern (`EXCLUDES`), and nothing
in the backup needs them: they are downloaded or rebuilt again.

`scripts/restore-system.sh <backup-directory> [disk]` is its other half, and the one script that
insists on being run as root: it does not re-exec through sudo, because silently escalating to
rewrite a disk is the wrong default. It refuses to run from the disk it would overwrite (that is
the "live SD" requirement), refuses a disk of a different size than the backup came from (the raw
GPT dumps are exact byte copies, and the same eMMC is the case that matters), checks SHA256SUMS
before writing anything, and takes the filesystem UUIDs from the archive's own `/etc/fstab`, so the
restored system boots with the fstab and kernel command line it had. `CONFIRM=yes` is required;
`VERIFY=no` skips the checksums and `DEEP=yes` adds a full `tar -d` comparison afterwards.

`build-rootfs.sh` installs `$WIFI_DIR` into the image when that directory exists, so the machine
comes up already knowing the Wi-Fi networks (see `scripts/wifi.sh`).

Its rootfs inputs are environment variables: `TARGET_HOSTNAME`, `USERNAME`, `USERPASS`,
`LOCALE` (default `en_US.UTF-8`) and `TIMEZONE` (default `America/Mexico_City`, e.g.
`TIMEZONE=UTC`). The script re-execs itself through `sudo`, which does not preserve
the environment, so it forwards every resolved input explicitly — anything not forwarded is
silently replaced by its default. A locale that is not an entry of `/etc/locale.gen`, or a
timezone that is not under `/usr/share/zoneinfo`, is reported and replaced by the default.

`write-sd.sh` and `install-emmc.sh` are destructive to block devices and refuse to touch the
internal eMMC from `write-sd.sh` or the running root from either. Both build the same three
partitions (see `scripts/lib-cgpt.sh`) and both write a signed kpart into p1;
`install-emmc.sh` copies the **running** system (rsync) instead of `$ROOTFS`, which is why it
needs the card to be a system worth cloning. That copy excludes the *contents* of `/proc`,
`/sys`, `/dev`, `/run`, `/mnt`, `/media`, `/tmp`, `/var/cache` and `/var/log/journal` — but
`rsync` also drops the excluded directories themselves, and a root filesystem needs them: they
are mount points, and the initramfs's `run-init` moves its own `devtmpfs` `/dev` (plus `/proc`
and `/sys`) into them before handing over to systemd. A copy without them boots the kernel,
mounts the root filesystem read-write and then stops at `run-init: opening console: No such file
or directory` / `Target filesystem doesn't have requested /sbin/init` / `No init found. Try
passing init= bootarg.`; with `quiet` on the command line that is a black screen, and since the
kpart and its payload are byte-identical to the card's it reads as a firmware or kernel problem.
The script now recreates `dev:755 proc:555 sys:555 run:755 mnt:755 media:755` after the copy and
refuses to write p1 unless `/dev` exists and `/sbin/init` resolves.

`install-kpart.sh` is the third one: it does not partition anything, it only rewrites the kernel
partition the running system was booted from (resolved from `kern_guid` in `/proc/cmdline`), so
that a kernel update does not leave p1 behind. It backs up what it replaces to
`$WORK_DIR/kpart-backup-<timestamp>.img` and requires `CONFIRM=yes`.

`check-system.sh` is their counterpart: it runs **on the booted image**, not on the build host, so
it sources nothing and changes nothing, and the image also ships it as
`/usr/local/sbin/check-system.sh` so no checkout is needed to run it. With a checkout it also
verifies that every package declared in `packages/*.list`, `devices/*/packages.list` and
the **running UI's** `uis/<ui>/packages.list` is installed (`check-system.sh ~/Dev/<repo>`); the
UI comes from `/etc/debian-aarch64-revision`, so the other desktop's list is never checked against
this system. Run it as the desktop user;
from a root shell it re-executes
itself as that user, and the privileged reads go through `sudo -n`. Every check maps to a bug this
port has already hit — keyd swallowing `KEY_SLEEP`, greetd's `vt`/user, the UCM boot sequence, ALSA's
missing `default` PCM, the backlight udev rule, the font fallback, `interface-name=` in the Wi-Fi
profiles, the clock, the `fstab` UUID, the kernel partition holding a signed FIT — so a FAIL there is
a regression, not a hypothetical. It
exits non-zero when something FAILs.

### Boot from microSD

1. `crossystem dev_boot_usb=1 dev_boot_signed_only=0` (from ChromeOS developer mode; already set
   on this machine).
2. `scripts/write-sd.sh /dev/mmcblk1`
3. Insert the card, power on and pick **external disk** in the firmware menu.

### The firmware menu (this machine)

In developer mode the stock firmware on this Chromebook does not wait for the Ctrl+D / Ctrl+U
screens the older documentation describes: it presents a **menu** on its own. Its entries were
verified on hardware: **internal disk = the eMMC, external disk = the microSD**. That is the
mechanism to boot the eMMC after `install-emmc.sh`; the microSD stays the rescue system (the card
is untouched by an eMMC install, so it can always be selected again).

Default credentials of the built image: user `brunux` / password `debian`, passwordless sudo.

