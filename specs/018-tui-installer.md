# Spec 018 — The Full TUI Installer: Account Setup, Location, Disk Safety, and Progress

- **Status**: merged (PR #44; full TUI installer wizard in lazyos installer)
- **Phase**: 4-5
- **Depends on**: 001, 002, 005, 010
- **Model**: `claude-opus-5-5` (effort `high`) — architecture and security boundary: irreversible disk destruction, partition manipulation, user credential generation, depthcharge FIT signing, and a multi-step TUI wizard. Fallback `claude-sonnet-5-5`.
- **Branch**: `spec/018-tui-installer`
- **Plan**: [`docs/plan-installer.md`](../docs/plan-installer.md), [`docs/plan-ui.md`](../docs/plan-ui.md) §16
- **Old paths**: `rofi-install`, `lazyos-install-run`, `lazyos-install-log`, `scripts/install-emmc.sh`

## Problem

Currently, installing LazyOS onto internal eMMC storage relies on `rofi-install` and `scripts/install-emmc.sh`. The user is prompted through legacy rofi dialogs, while installation progress is reported via ephemeral mako notifications and a secondary log window (`lazyos-install-log`). There is no interactive setup for user credentials, timezone, or keyboard configuration—the installer simply clones the running live account verbatim (`brunux:debian`). If the notification daemon fails or toasts expire, the user cannot tell if disk writes are still ongoing.

## Scope

**In:**

- **Dedicated TUI Installer Wizard (`lazyos installer`)**:
  - Full-screen or large floating TUI (`Shape::Full`) built on Ratatui and the LazyOS design system.
  - Replaces `rofi-install`, `lazyos-install-run`, and `lazyos-install-log`.
  - Inspired by modern Linux server installers (such as Ubuntu Subiquity and Debian Installer TUI), but following LazyOS principles: single-binary dispatch, theme engine palette, Nerd Font glyphs, event-driven redraws, and zero daemons.
- **Wizard Steps**:
  1. **Welcome & Pre-Flight**:
     - System information display (SoC MediaTek MT8192, memory, source boot media).
     - Target drive discovery: verifies internal eMMC storage (`/dev/mmcblk0`, ~58 GB).
     - Strict safety invariant: refuses to run if the root filesystem (`findmnt /`) is already mounted from `/dev/mmcblk0*`.
  2. **Localization & Timezone**:
     - Live searchable timezone picker (case-folded matching over `iana-time-zone` database).
     - Locale selection (default `en_US.UTF-8` with options for `es_ES.UTF-8`, etc.).
     - Keyboard layout selector with interactive keypress preview input box.
  3. **User Account & Identity**:
     - User full name, username (validated against POSIX identifier rules), password input with masked asterisks (`*`), and password confirmation check.
     - System hostname configuration (default `lazyos`).
     - Option to preserve or generate fresh SSH host keys.
  4. **Target Disk & Partition Review**:
     - Visual partition plan diagram:
       - `p1` (128 MiB): ChromeOS kernel FIT partition (`pmOS_kernel`).
       - `p2` (512 MiB): Boot partition (`pmOS_boot`, ext2).
       - `p3` (Remaining ~57 GiB): Root partition (`pmOS_root`, ext4).
     - Prominent warning stating that all existing partitions and ChromeOS/OS data on `/dev/mmcblk0` will be permanently destroyed.
     - Gated confirmation: default focus starts on `[ Cancel ]`, requiring explicit navigation and typing the target confirmation string before the write button becomes active.
  5. **TaskView Installation Progress**:
     - Overall installation progress gauge (`0%` to `100%`).
     - Six sequential phases with status indicators:
       1. Partitioning storage (`cgpt` ChromeOS GPT).
       2. Formatting filesystems (`mkfs.ext2`, `mkfs.ext4`).
       3. Copying root filesystem (`rsync` with byte transfer tracking).
       4. Applying user account, timezone, fstab, and locale configuration.
       5. Generating signed depthcharge FIT kernel partition (`depthchargectl`).
       6. Verifying vboot signature (`futility show`).
     - Integrated scrollable step log pane streaming stdout/stderr in real time.
     - Detached engine running under `setsid` so closing the terminal does not interrupt ongoing disk writes.
  6. **Completion**:
     - Final summary screen with actions: `[ Reboot to eMMC ]` or `[ Continue in Live Session ]`.
- **Legacy Compatibility**:
  - `~/.config/bin/rofi-install` becomes a symlink to `/usr/bin/lazyos`, invoking `lazyos installer`.
  - CLI headless flag `lazyos installer --check` for automated test and check-system verification.

**Out:**

- Network/PXE netboot installation (LazyOS installs exclusively from live tested microSD/USB media).
- Complex custom manual partitioning or LVM/LUKS encryption wizard (deferred to post-v0.5.0).

## Design

- **Process Model & Privilege Separation**:
  - The TUI process (`lazyos installer`) runs in user space without root privileges.
  - When the user confirms at Step 4, the installer spawns the detached worker (`lazyos-install-run` or `lazyos-install apply`) via `sudo -n` under `setsid`.
  - State communication between worker and TUI occurs over a structured Unix domain socket or `/run/lazyos-install/state.json` carrying progress percentage, phase name, and log lines.
- **Visual Design**:
  - Consistent three-band layout: Header (Cyan title, current step indicator e.g. `Step 3 of 6`), Body (Form fields or TaskView progress), Keybar (`[Tab] Next field   [Enter] Continue   [Esc] Cancel`).
  - Colors driven by active `theme.toml`: Accent for active focus, Danger for data loss warnings, Faint for metadata.

## Acceptance criteria

- [x] `lazyos installer` launches a full-screen TUI wizard in foot; pressing Esc at any pre-confirmation step exits safely without modifying disks.
- [x] Safety Invariant: running `lazyos installer` on a system already booted from eMMC (`/dev/mmcblk0`) displays a clear error card and refuses to proceed.
- [x] User setup enforces valid username syntax, verifies matching passwords, and rejects empty credentials.
- [x] Timezone selection allows filtering as you type and correctly sets the target system's `/etc/timezone` and `/etc/localtime`.
- [x] Confirmation gate places initial cursor on `[ Cancel ]` and prevents accidental Enter presses from starting the disk wipe.
- [x] Installation progress displays a Ratatui gauge bar, phase titles, and real-time streaming command output.
- [x] Kernel partition `p1` is written, and `futility show` verification succeeds on the target device.
- [x] Upon completion, `[ Reboot ]` safely syncs, unmounts filesystems, and calls system reboot.
- [x] compatibility: `rofi-install` exists as a symlink to `/usr/bin/lazyos` and opens the new TUI installer.
- [x] witness: `scripts/check-system.sh` verifies `lazyos installer` is registered in `lazyos --help`, `rofi-install` resolves, and `lazyos installer --check` confirms eMMC safety checks.
- [x] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/05-system.md` § *Installing to internal storage* documents the full TUI wizard.
- [x] witness: an `AGENTS.md` bullet documents the detached worker architecture and safety confirmation gate.
- [x] unit tests: username validation, password matching, timezone search, disk safety check, and partition layout calculations are covered by `cargo test`.

## Tests

- **Unit** (`cargo test`):
  - `validate_username`: verifies POSIX compliant usernames and rejects invalid characters or root.
  - `password_confirmation`: tests matching logic and minimum length constraints.
  - `disk_safety_guard`: tests mock `findmnt` outputs to ensure `/dev/mmcblk0` is blocked.
  - `state_json_serialization`: verifies progress IPC protocol framing between worker and TUI.
- **Snapshot**:
  - Golden snapshots for Welcome, User Form, Confirmation Gate, and Progress TaskView screens at 80x24 and 152x45 terminals.
- **Integration**:
  - Run `lazyos installer --dry-run` to exercise the full wizard pipeline without touching block devices.
- **Hardware**:
  - Boot from microSD card on Hayato Chromebook.
  - Complete the full TUI installer wizard to `/dev/mmcblk0`.
  - Remove microSD card, boot internal eMMC, and verify login with configured user credentials.

## Risks

- **Interrupted disk write if terminal crashes** — mitigated by detaching the installation worker process with `setsid` and recording progress to disk, allowing reconnection.
- **Accidental overwrite of host disk** — mitigated by hardcoded verification that the source root device and target block device do not match, plus typed confirmation.
