# Plan: LazyOS

> Living roadmap. It records where the project is, the rules that do not change, the Rust-first
> language policy, the installer that comes next, and the order in which the shell tools move to
> Rust. The deep dive for the installer is [`plan-installer.md`](plan-installer.md); the measured
> device work is [`optimization.md`](optimization.md); the boot chain and the gotchas are
> [`AGENTS.md`](../AGENTS.md).

**Status: v0.4.2.** The image boots from microSD on the Hayato, the eMMC install works via
the native TUI wizard (`lazyos installer`), the session UI is 100% migrated to the Rust multi-call
binary `lazyos`, 16 themes are driven by `lazyos-theme`, `lazyos-notify` serves layer-shell
cards, and `rofi` / `lib-rofi.sh` / `bar-pick` are retired.

---

## 1. Where the project is

| Piece | State |
|---|---|
| Debian forky arm64 on the Hayato (MT8192 / `google-asurada`) | working, validated on hardware |
| Boot chain: stock depthcharge, signed FIT kpart in `p1`, `/boot` in `p2`, root in `p3` | working, reproduced with `cgpt` + `depthchargectl` |
| swayfx session, lazyos framework, waybar, foot, lazyos-notify, manual | working, 424+ OK in check-system report |
| eMMC install | `lazyos installer` (spec 018), proven on hardware |
| Backup / restore | `lazyos-profile export/import`, `scripts/backup-system.sh` |
| Installer with a UI | **completed**: `lazyos installer`, Rust end to end |
| Rust migration of the shipped tools | **completed**: 45+ subcommands in `lazyos`, rofi retired (spec 014) |

The reference machine is the one being ported; see [`recon.md`](recon.md) for the hardware table
and [`reports/`](../reports/) for the evidence.

## 2. The non-negotiables

1. **The boot chain is stock.** depthcharge loads a signed FIT kpart from the GPT partition whose
   PARTUUID is `kern_guid`; the SPI firmware is **never** written. Only GPT partitions on the
   card/eMMC are touched.
2. **The device layer stays device-agnostic.** A device is `devices/<codename>/` (deviceinfo,
   kernel flavor, cmdline, initramfs modules, packages) plus `kernels/<flavor>/` and, when it needs
   its own shell, `uis/<ui>/`. Nothing in the engine is Hayato-specific.
3. **The booted card is the installer.** There is no separate ISO, no squashfs, no Calamares, no
   second rootfs. The card that has been tested is the source it clones, exactly as pmbootstrap
   installs the rootfs it built. A separate artifact would introduce a second system that nobody
   booted before.
4. **One implementation per dangerous concept.** The ChromeOS GPT, the kpart, the identity cleanup
   and the device profile each have one owner. The shell today and the Rust crates tomorrow are
   never two live copies of the same rule.
5. **Nothing resident.** Menus and installers are one-shot processes; the engine is a child that
   ends. `cliphist`'s watcher remains the one agreed exception.
6. **A feature ships with its three witnesses**: a `check-system.sh` line that fails when its
   dependency disappears, a paragraph in the shipped manual, and a bullet in `AGENTS.md` that says
   why it is built that way.
7. **Destructive actions name what they erase, cancel first, and re-ask.** The installer's
   confirmation starts on **Cancel**; after the first write the install cannot be stopped.
8. **No in-place upgrades.** A system is replaced by installing a new image (card or eMMC), not by
   upgrading itself. This is why the card remains the rescue system.

## 3. Language policy: Rust first

The shipped tools are written in **Rust**. Shell remains where it is not a shipped program:

| Stays shell | Why |
|---|---|
| `scripts/00-deps.sh` | It installs the toolchain (including cargo); it cannot be built by the toolchain it installs. |
| `scripts/build*.sh`, `scripts/install-kernel.sh`, `scripts/install-swayfx.sh` | Host-side build orchestration: download, compile, stage, call the Rust tools. It is not on the device and it changes with upstream, not with the product. `lazyos-build` may absorb it in v0.6. |
| `~/.bashrc`, `etc/profile.d/90-cli-tools.sh` | They are sourced by the shell; a binary cannot be sourced. |
| `BUSY_STOP` in the busy protocol | The line is data that a shell executes by design (`sh -c`); the menu writer cannot know every tool's stop pattern in advance. |
| `scripts/make-icons.sh`, `make-brand.sh`, `make-emojis.sh` | Host-only asset generators whose output is committed. Porting them would add heavy image/font/Unicode dependencies to the build for no runtime gain. |
| Third-party configuration (sway, foot, waybar, rofi, mako, glow, GTK) | It is the upstream program's format, not our code. |

Everything else that ships or makes a decision **becomes Rust**:

- the installer engine and its TUI;
- the disk, identity, kpart and profile logic;
- the session's logic tools, bar popups and rofi menus;
- `check-system.sh`, the backup/restore tools, the Wi-Fi import/export, the release sanitizer;
- the host disk tools (`write-sd`, image/release assembly) once the engine's disk crate exists.

### The shape

```
rust/Cargo.toml            workspace, Cargo.lock committed
rust/crates/lazyos-core    deviceinfo/compatible/profile, run, paths, errors, version
rust/crates/lazyos-disk    ChromeOS GPT, mkfs, fstab, mount, kpart, verify
rust/crates/lazyos-identity clean machine identity; later the fresh user/home
rust/crates/lazyos-install probe, plan, apply, state, doctor, auto (lib + binary)
rust/crates/lazyos-installer Ratatui TUI (binary)
rust/crates/lazyos         multi-call session/system tool (installer host in v0.4)
```

- **One multi-call binary for the session.** `lazyos <subcommand>` replaces the 50 scripts under
  `~/.config/bin/`. The old paths become **symlinks** to `/usr/bin/lazyos`, so the 36 sway
  bindings, the 26 `.desktop` `Exec` lines and the manual keep working; a dangling link fails `-x`
  in `check-system.sh`.
- **Three binaries at first**: `lazyos-installer` (TUI), `lazyos-install` (root engine) and
  `lazyos` (host integration: `installer`, `installer-autostart`, later the session). The engine
  and the TUI share `serde` types, not code paths.
- **Ratatui, not layer-shell.** The installer is a fullscreen TUI in foot, hosted with
  `foot --app-id=org.lazyos.installer --fullscreen -e lazyos-installer`. It does not paint a
  Wayland surface of its own; the trade-offs (sway bindings, idle inhibitor) and their handling
  are in [`plan-installer.md`](plan-installer.md) §8.
- **Pure-Rust dependencies**, `Cargo.lock` committed, `cargo build --release --locked`, target
  directory under `$WORK_DIR`. The binary reports the version in `VERSION`; a build cannot say
  0.2 while the tag says v0.4.0.
- **Tests are part of the port**: unit tests, `TestBackend` snapshots for the TUI, a loop-device
  diff of the GPT against the shell library before the shell is deleted, a dry run on the card,
  and the hardware checklist for the first real install.
- **The session framework around the installer** — the design system, the TaskView, the theme
  engine and the OpenCode/MCP contract — is [`plan-ui.md`](plan-ui.md). The installer TUI is the
  first consumer of the same widgets.

## 4. The installer (v0.4)

The live card is the installer. It probes the running system, matches the device profile, builds
and verifies the kpart **before** erasing anything, then clones the card's tested root filesystem
onto internal storage, cleans the machine identity, writes the fstab, cmdline and kernel partition,
verifies them, and records the installation so the next boot knows it is installed.

- **v0.4.0 ships the clone path + clean identity**: the path `install-emmc.sh` already proved, with
  the card's user and applications preserved and `/etc/machine-id` emptied and the SSH host keys
  removed, so two machines never share one identity. The target's existing contents are named in
  the confirmation and are gone after the first write.
- **Fresh mode** (new user, home, hostname and password) is the first v0.4.x addition. Its screens
  and schema are designed now; the engine refuses `--mode fresh` until it has been tested on
  hardware.
- **Internal storage only by default.** A profile may override the target with
  `deviceinfo_install_target`; writing cards remains `scripts/write-sd.sh`. The running disk, the
  SPI (`mtd*`), the eMMC boot partitions (`mmcblk*boot*`, `rpmb`), zram, loop, dm and md are never
  candidates.
- **Detection without a kernel flag**: `/etc/lazyos/installation` records the root UUID; the
  installer compares it with `/` and is live when the marker is absent or belongs to another root.
- **Autoinstall** is a KEY=value file read by `lazyos-install auto`; no UI, no prompt, and the
  password (fresh mode only) arrives as a crypt hash, never in argv or in a log.
- **The TUI opens once per live card** (`exec-once lazyos installer-autostart`), then lives in the
  launcher and the root menu. A window closed mid-install does not stop the engine; reopening the
  TUI reattaches to the state under `/run/lazyos-install`.

The full design — layers, detection, phases, safety invariants, JSON protocol, screens, packaging,
tests and witnesses — is [`plan-installer.md`](plan-installer.md).

## 5. The Rust migration, in tiers

Line counts are the shell that moves (whole files unless noted), from `wc -l` at the time of this
plan.

| Tier | Shell today | Rust | Lines |
|---|---|---|---|
| **v0.4.0** | `scripts/lib-cgpt.sh` | `lazyos-disk`: geometry, `cgpt`/`parted`, mkfs, fstab, mount, kpart | 246 |
| | `scripts/install-emmc.sh` | `lazyos-install apply`: clone, excludes, mount dirs, identity cleanup, boot, verify | 107 |
| | `scripts/install-kpart.sh` + `scripts/make-kpart.sh` | `lazyos-disk::kpart` + engine: cmdline, size check, `futility`, backup | 181 |
| | `scripts/rootfs-setup.sh` (machine identity) | `lazyos-identity`: machine-id, SSH keys, marker | part of 348 |
| | `scripts/lib.sh` (device side) | `lazyos-core`: deviceinfo, `compatible`, profile | part of 188 |
| | layout calls of `scripts/write-sd.sh` / `make-image.sh` | `lazyos-disk` (copy glue stays shell for now) | part of 159 |
| **v0.4.1** | `scripts/rootfs-setup.sh` (user/home/locale/timezone) | `lazyos-identity`: user, sudoers, password, home template, locale, timezone | part of 348 |
| **v0.4.x** | 12 session logic tools (`sway-*`, `screenshot`, `dictate`, `tablet-mode`, `lazyos-view`, …) | `lazyos` subcommands | 1,624 |
| | `lib-rofi.sh` | `lazyos::ui` (row, pick, pick-alt, busy, notifications) | 412 |
| | 9 `bar-*` popups | `lazyos bar-*` | 970 |
| | 28 `rofi-*` menus | `lazyos rofi-*` | 4,701 |
| **v0.5.0** | `scripts/check-system.sh` | `lazyos check`, same report, golden diff | 1,982 |
| | `backup-system.sh` + `restore-system.sh` | `lazyos backup` / `lazyos restore` | 519 |
| | `wifi.sh` | `lazyos wifi` | 82 |
| | `lib-dist.sh` + `make-release.sh` | `lazyos release` (sanitizer asserted) | 248 |
| | host disk glue (`write-sd.sh`, `make-image.sh`) | `lazyos image`, `lazyos sd` | 159 |
| **v0.6.0** (optional) | `build-kernel.sh`, `build-rootfs.sh`, `build-swayfx.sh`, `build.sh`, `install-kernel.sh`, `install-swayfx.sh` | `lazyos-build` or keep shell | ~6 files |

Rules for every tier:

- **Port behind the old path.** The new binary is installed, the old file is replaced by a symlink,
  and the old path is deleted only after `check-system.sh` and the hardware checklist are green.
- **No new shipped shell.** A temporary adapter is allowed only if it is deleted in the same
  release; the goal is no shell surface left on the device.
- **The witnesses move with the tool.** A ported script's `check-system` line now checks the
  symlink and the binary's subcommand; the manual paragraph and the `AGENTS.md` bullet say the new
  name.
- **The dangerous ones go first.** The disk layout, the kpart and the identity are ported before
  the menus: they are the code where a bug costs a disk, and they get the loop test and the dry
  run.

## 6. Roadmap

- **v0.4.0 — Rust installer, clone path.**
  `lazyos-core` / `lazyos-disk` / `lazyos-identity` / `lazyos-install` / `lazyos-installer`;
  probe → plan → kpart → partition → format → clone → clean identity → boot → verify → marker;
  the Ratatui TUI (Welcome, Device, Target, Confirm, Install, Done/Failed); autostart once per live
  card; launcher and root-menu entries; autoinstall; check-system witnesses; manual page; tests
  (unit, `TestBackend`, loop diff, dry run, card checklist). **No bash engine.**
- **v0.4.1 — Fresh identity and the kiosk option.**
  New user/home/hostname/password; the Mode and Identity screens; home template; `USERPASS_HASH`
  autoinstall; the password on stdin, never argv; optionally a dedicated greetd "Installer"
  session (minimal sway, no bindings, no idle chain). tuigreet 0.9.1 has no default-session flag,
  so it is a manual pick, not the auto-open.
- **v0.4.x — Rust session.**
  The 12 logic tools, `lib-rofi.sh`, the 9 bar popups and the 28 rofi menus become `lazyos`
  subcommands; the old paths become symlinks. This is the bulk of the line count and is done one
  commit per tool/menu, with the key view and the manual regenerated from the same paths. The
  framework they are hosted in (design system, TaskView, themes, AI/MCP) is
  [`plan-ui.md`](plan-ui.md).
- **v0.5.0 — System tools and many devices.**
  `lazyos check` (the same report, with a golden diff against `check-system.sh`), backup/restore,
  Wi-Fi import/export, release/sanitizer, the host disk tools, and the first non-Hayato device
  ported with the same installer. The new profile repeats the `compatible` file and the target hint
  the Hayato profile already carries.
- **v0.6.0 — Optional build tool, LUKS2, A/B, recovery.**
  `lazyos-build` if the host orchestration earns it; LUKS2 and an A/B (two kernel/root) layout are
  designed only after the single-slot clone is proven on more than one device; a recovery story
  is written before it is needed.
- **Not planned:** a separate installer ISO/squashfs, Calamares/GTK/Qt/web UIs, in-place upgrades,
  and a package manager of our own.

## 7. Multi-device

The installer is device-agnostic by construction. A new port is:

1. `devices/<codename>/` — `deviceinfo`, `kernel`, `kernel-cmdline.conf`, `modules-initfs`,
   `packages.list`, and a new **`compatible`** file listing the exact strings from
   `/proc/device-tree/compatible`, most specific first.
2. `kernels/<flavor>/` — only if the kernel is not already shared.
3. `uis/<ui>/` — only if the device needs a different shell.

`build-rootfs.sh` installs the profile at `/usr/share/lazyos/` (`deviceinfo`, `compatible`,
`kernel-cmdline.conf`, `modules-initfs`, `home-template` for fresh mode). The engine matches the
running DT and **fails closed** when no profile matches, showing both lists. The kpart is built
from the profile; the kernel partition geometry, board and DTBs come from `deviceinfo`, exactly as
the shell does it today.

Two constraints until they are verified:

- **One FIT config per device.** With `--board mt8192` no FIT config matches by `compatible`, so
  depthcharge falls back to the first DTB; hayato must stay first. A multi-device FIT needs a
  verified config selection before it ships.
- **The installer targets internal storage by default.** A device whose "internal" storage is
  removable declares `deviceinfo_install_target`; a card install is an explicit profile choice,
  not a default.

The first non-Hayato port is the test that the device layer is real.

## 8. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Writing the wrong disk | The running disk, `mtd*`, `mmcblk*boot*`/`rpmb`, zram, loop, dm and md are excluded by construction; the plan names model/size/serial; the token revalidates before the first write; the rescue card is the running disk. |
| A half-written target | The kpart is built and `futility`-verified **before** erasing; the plan's UUIDs are fixed before mkfs; the install cannot be cancelled after the first write; the UI says which step is running. |
| The marker on old images | v0.3.0 has no `/etc/lazyos/installation`; there are no in-place upgrades, so a v0.4 image is installed by the v0.4 engine, which writes it. `doctor` reports a legacy install as such. |
| TUI key collisions with sway | No Alt chords in the TUI; the engine is detached (`setsid`) and survives a closed window; the idle chain is paused/resumed; a kiosk session is the clean option later. |
| Build host vs target | Native aarch64; pure-Rust deps; the binary is checked in the chroot. A glibc/musl mismatch is a build error, not a silent one. |
| The shell and Rust drift | The shell is the reference until its port's witnesses are green; after that the shell path is a symlink, and the loop/golden diffs catch drift. |
| Multi-device FIT selection | One FIT config per device until verified; the profile's DTBs are the only ones embedded. |

## 9. Open questions

- **Kiosk installer session**: a minimal greetd session would remove the sway bindings and the idle
  chain from the installer's world. tuigreet 0.9.1 cannot preselect it, so it is a manual entry;
  v0.4.1 evaluates it after the fullscreen-foot path is used on hardware.
- **`lazyos-build`**: the build orchestration is host-only and changes with upstream; it moves only
  if the shell there becomes a real maintenance cost.
- **LUKS2 / A/B**: after the single-slot clone is proven on more than one device. The installer's
  plan/apply boundary is designed so a second slot is a plan change, not an engine rewrite.
- **Fresh-mode defaults**: locale/timezone from the live card or a fixed default; the
  `USERPASS_HASH` format; whether the home template carries the manual or only the session files.

## 10. References

- [`plan-installer.md`](plan-installer.md) — the installer design: layers, phases, safety, protocol,
  screens, tests, packaging.
- [`plan-ui.md`](plan-ui.md) — the session UI framework: design system, TaskView, theme engine,
  and the OpenCode/MCP contract that lets the AI drive the desktop visually. Its §16 roadmap is
  carried out as the specs in [`../specs/`](../specs/README.md), one branch and one PR each.
- [`plan-rofi.md`](plan-rofi.md) — what of the session can move onto rofi (the menus being ported).
- [`plan-agents.md`](plan-agents.md) — the agents feature, phased.
- [`optimization.md`](optimization.md) — the measured device and kernel tuning.
- [`recon.md`](recon.md) — the reference machine.
- [`AGENTS.md`](../AGENTS.md) — the contract: boot chain, gotchas, commands, witnesses.
- Upstream: Nura / postmarketOS, pinned `pmaports` commit
  `0e458a2417d6f5ed61c7b70621d5632e30136091` (2026-10-15).
