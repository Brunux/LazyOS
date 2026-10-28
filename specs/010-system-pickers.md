# Spec 010 — System pickers

- **Status**: draft
- **Phase**: 3
- **Depends on**: 002, 003, 004, 005, 006
- **Model**: `claude-sonnet-5-5` (effort `high`) — many dialogs sharing one shape, each with a
  destructive edge and a parity test against the shell it replaces; the confirm type is the
  safety, and the tests prove it. Fallback `claude-opus-5-5` (not a cheaper model: the edges are
  format, swap and power).
- **Branch**: `spec/010-system-pickers`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §5.7-§5.10, §16 phase 3
- **Old paths**: `rofi-quick`, `rofi-power`, `rofi-display`, `rofi-swap`, `rofi-format`,
  `rofi-tz`, `rofi-dns`, `rofi-vnc`, `bar-*` panels (settings/power), `sway-hibernate`

## Problem

Nine system dialogs live in nine scripts with nine error stories. The destructive ones
(format, swap, power) each re-implement their own confirmation, and there is no single place
where "is this disk a system disk" or "is this action stoppable" is decided and tested — the
shell got these rules right only after hardware reports.

## Scope

**In (`lazyos menu settings|power|display|swap|format|tz|dns|vnc|ssh`):**

- **Settings**: the radio switches (Wi-Fi, Bluetooth, night light, DND, touchpad, tablet
  mode), each showing the real state from the same probe as today, with a task for slow
  reads (the Wi-Fi scan).
- **Power**: lock, suspend, hibernate (with the three refusal reasons), log out, reboot,
  shut down — every destructive row behind a confirm with Cancel selected; hibernate's
  preconditions are checked **before** the row is offered and explained when missing.
- **Display**: the arrangement menu (right/left/above/below/mirrored, resolution, scale),
  the saved `config.d/40-display.conf`, `wdisplays` as a row, and the blind `--panel-on` path
  as its own subcommand.
- **Swap**: auxiliary swap setup/undo with the same disk-safety function as Format, the
  labelled `fstab` block, the read-back activation.
- **Format**: only disks that pass `ui_disk_system`-equivalent rules; Cancel first; the
  candidate list's four refusal reasons; not stoppable.
- **TZ / DNS / VNC / SSH**: the existing behaviours, with the VNC eye state and the ssh
  status as read rows.

**Out:** media/notify/clip/emoji/capture (011), the launcher and reference (012), the theme
engine's own menu (004).

## Design

- **One safety module** in the crate: `disk::is_system_disk()`, `disk::candidates()`,
  `power::can_hibernate()`, `confirm::destructive()`; the shell's rules are ported as unit
  tests against the fixtures the shell left behind (the `lsblk`/`findmnt` sample outputs).
- A picker is a `List` + a `Confirm`; the confirm text names the device, model and size (the
  Format rule) and is the only path to the action.
- The menus call the **same system tools** (`nmcli`, `swaymsg`, `parted`, `mkswap`, `wdisplays`)
  through argv arrays; no shell strings, no `pkill` patterns except where the task spec says
  so.
- `--status` subcommands return the same data the row shows, so the menu and the check read
  one source (the pattern `rofi-display --status` already established).

## Acceptance criteria

- [ ] Settings reflects a real toggle within one second (nmcli/bluetoothctl/swaymsg), and each
      switch survives a reopen.
- [ ] Power: Hibernate explains a missing swap/resume instead of failing; reboot and shutdown
      require the confirm; Escape at the confirm does nothing.
- [ ] Format refuses the boot card, the eMMC, a disk carrying a kernel partition and the disk
      root/boot/swap live on — the four reasons each have a test with a fixture.
- [ ] Swap: setup writes the labelled block, activates at priority 10, and undo removes it;
      a failed activation removes the block again.
- [ ] Display: the arrangement is applied at once and saved; `--panel-on` restores the internal
      panel with no monitor and with the panel off (the blind key).
- [ ] witness: `scripts/check-system.sh` FAILs on any missing subcommand, on a missing
      destructive confirm (the table), on the swap/format guard functions, and on the display
      config include.
- [ ] witness: paragraphs in `uis/sway/files/usr/share/doc/lazyos/04-software.md` and
      `05-system.md` describe each dialog and its undo.
- [ ] witness: an `AGENTS.md` bullet says the safety rules are one tested module.
- [ ] unit tests: disk candidate rules (four fixtures), hibernation preconditions, fstab block
      replace/remove, TZ effect, DNS state read-back, display arrangement parsing/writing.

## Tests

- **Unit** (`cargo test`): the safety module against recorded `lsblk`/`findmnt`/`swapon` outputs; fstab
  editing (marker replace, no truncation); confirm text.
- **Snapshot**: each picker's empty/normal/error state; the Format confirm.
- **Integration**: a loop device with the pmOS layout (the repo's own fixture) → Format and
  Swap refuse; a spare image → both succeed and undo cleanly.
- **Hardware**: switch night light and DND, hibernate (with the swap card in), arrange the
  external monitor, then unplug it and use `--panel-on`.

## Risks

- **A destructive action reachable without the confirm** — the action type is only constructed
  by the confirm, and the check greps for direct calls.
- **A system tool's output changes** (nmcli fields) — parsing is isolated per tool with a
  fixture, and a parse failure is a visible error row, not a wrong state.
