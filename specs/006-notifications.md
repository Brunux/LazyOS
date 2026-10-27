# Spec 006 — Cards: notifications and in-window messages

- **Status**: merged (PR #19)
- **Phase**: 2
- **Depends on**: 002, 003
- **Model**: `opencode-go/claude-haiku-5-5` — a small, well-shaped surface over an existing
  daemon (mako stays), mostly fixtures and icon resolution. Fallback
  `opencode-go/minimax-m3.1-flash`.
- **Branch**: `spec/006-notifications`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §5.6 (notifications), §13 (the AI surface)
- **Old paths**: `notify-send` call sites in `~/.config/bin/*` keep working until spec 013

## Problem

Notifications are already `mako` and work, but every script has to remember the house rules:
an icon name the LazyOS theme draws, the `x-canonical-private-synchronous` tag for OSD/busy,
a progress hint, the right urgency, and never a secret in the body. Today a text-only card is
invisible in review ("the row worked"), and the AI/desktop surfaces need the same card as an
in-window panel, not a toast.

This spec builds one card vocabulary used by both: `Card::send()` (through mako today) and
`Card::render()` (inside a `lazyos` window), with the same fields and the same icon lookup.

## Scope

**In (`lazyos-ui::card` + `lazyos notify`):**

- **Fields**: title, body, urgency (low/normal/critical), icon (a `Glyph` or a theme PNG name),
  progress (0-100), group tag, action (a command to run on click), timeout.
- **Icon resolution**: glyph → spec 003 table; name → `/usr/share/icons/LazyOS/...` (the same
  roots mako's `icon-path` uses). A missing icon logs one line and renders text — it never
  silently drops the picture (the rule that made `notify-send -i folder` look correct while
  the card stayed blank).
- **Secrets rule**: a helper `Card::secret()` that copies to the clipboard and sends a card
  whose body says only that it is copied; the value never enters the body (and therefore never
  mako's history).
- **In-window**: `Card` as a widget for results and errors inside a task or a menu; same
  urgency colours, same icon.
- **`lazyos notify`**: `send`, `say` (a one-line result), `progress` (the OSD path: one card,
  updated in place by tag).

**Out:** the notification daemon (`mako` stays until spec 013), notification history beyond
mako's own, sound.

## Design

- The mako backend writes a `notify-send`-compatible call (a small D-Bus message via
  `zbus`/`notify-rust`), not a shell-out; the tag and hints are the same strings the shell
  menus use, so the bar and OSD behaviour does not change.
  - *Deviation*: the transport is `notify-send` with an argument vector, not a bus client in the binary. The `zbus` client was built and measured first (+42 crates, +2.6 MB, about 1m40s more on a clean build) and was not adopted for a call the image makes through `notify-send` already (`docs/optimization.md` §14). The tag and hints are the same strings, sent by the same program.
- The card's style comes from the theme (spec 004): role colours, not hex.
- `--icon=auto` derives the icon from the command/result type, so a new call site cannot
  forget it.

## Acceptance criteria

- [ ] `lazyos notify send "Saved" --icon image --body <path>` shows a card with the theme's
      image icon; the same call without an icon shows a text card and logs the fallback.
  - *Observed*: the argument vector the CLI hands `notify-send` carries `-i image-x-generic` (`lazyos-cli/tests/notify.rs::a_glyph_name_is_the_theme_picture_and_auto_is_a_folder_for_a_directory`); a name the theme lacks is sent without `-i` and the reason is printed (`a_picture_the_theme_lacks_is_sent_as_text_and_reported`).
  - *Deviation*: a call with no `--icon` flag is `auto`, which picks a picture from the body, because a card has a picture by default (`Card::new`). The "without an icon" case is therefore tested as "with a name the theme lacks", the only way a picture can be missing.
  - *Pending on the Hayato*: the card visibly showing the image icon.
- [ ] the OSD group (`x-canonical-private-synchronous:lazyos-osd`) still updates one card in
      place, with the progress bar, when called in a loop.
  - *Observed*: each call carries `string:x-canonical-private-synchronous:lazyos-osd` and `int:value:N` (`the_osd_card_asks_for_its_group_its_bar_and_its_picture`; `tag_is_stable`, `progress_is_clamped_and_carried_to_the_hint`).
  - *Pending on the Hayato*: mako replacing the card in place while the key is held (mako's behaviour on that hint, which the shell OSD already relies on).
- [ ] `Card::secret()` puts the value on the clipboard and the card names no part of it.
  - *Observed*: the copy receives the value, and the title, body, `Debug` output, the prepared message and the rendered card contain none of it (`secret_never_in_body`, with an injected copy).
  - *Pending on the Hayato*: the real `wl-copy` putting the value on the Wayland clipboard (the build host has no Wayland display).
- [x] an in-window card renders the same icon and colours; a long body truncates with an
      ellipsis instead of resizing the window.
  - *Observed*: golden snapshots at 80x24 and 152x45 (`card-urgency`, `card-icon`, `card-progress`, `card-long` in `lazyos-ui/tests/snapshots.rs`); the critical frame is the danger colour cell by cell (`a_critical_card_is_drawn_in_the_danger_role`); a 200-character body stays inside 40 columns and ends in an ellipsis (`body_truncates_to_width`).
- [x] witness: `scripts/check-system.sh` FAILs when a `notify-send` call in `~/.config/bin`
      names an application but no icon, when an icon name used by the binary is absent from
      the LazyOS theme, and when mako's `icon-path` loses the theme root.
  - *Observed*: the block was extracted from `check-system.sh` and run against a scratch `HOME` with a `lazyos` on the PATH. The shipped tree passes all five lines; a `notify-send -a LazyOS` without `-i` appended to a copy of `~/.config/bin` FAILs and names `zz-probe:1`; a mako config whose `icon-path` lost the LazyOS root FAILs; a theme copy without `image-x-generic.png` FAILs and names it. On the device the theme is `/usr/share/icons/LazyOS`.
- [x] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/05-system.md`
      § *Notifications* says where they appear, how to dismiss them, and that history is kept.
  - *Observed*: the section says the card is top right under the bar, that a right-click dismisses it, that `Alt+Ctrl+N` keeps the last twenty (`max-history=20`), and that a secret is never in a card. The right-click default is mako's (`mako(5)`, `on-button-right=dismiss`).
- [x] witness: an `AGENTS.md` bullet says the card is one vocabulary for toast and window.
  - *Observed*: the bullet "A card is one vocabulary for the toast and for the message inside a window" in the feature rules.
- [x] unit tests: icon resolution order (glyph, theme, fallback), secret body, truncation,
      group tag propagation, urgency mapping.
  - *Observed*: `icon_resolution_prefers_glyph`, `missing_icon_is_text_and_logs`, `secret_never_in_body`, `body_truncates_to_width`, `tag_is_stable`, `urgency_maps_to_mako_and_to_a_role` (`lazyos-ui/src/card.rs`), and `the_arguments_carry_the_hints_the_shell_sends` (`lazyos-ui/src/card/send.rs`).

## Tests

- **Unit** (`cargo test`): `icon_resolution_prefers_glyph`, `missing_icon_is_text_and_logs`,
  `secret_never_in_body`, `body_truncates_to_width`, `tag_is_stable`.
- **Snapshot**: card at three urgencies, with/without icon, with progress.
- **Integration**: a fake notification service on a private bus (a stub over the D-Bus name)
  - *Deviation*: no owning fake daemon. `notify-send` stops after `GetServerInformation` when nothing owns the name, so the bus cannot show a `Notify` without a stub that answers, and no bus-client crate is in the binary (`docs/optimization.md` §14). The hint payload is pinned instead by the exact argument vector (`card::send::tests::the_arguments_carry_the_hints_the_shell_sends`, and `tests/notify.rs` through a stub `notify-send` on the PATH); the bus route is observed with `dbus-monitor` (`the_card_is_addressed_to_the_notification_service_on_the_session_bus`).
  - *Observed*: `mako -c` on `uis/sway/files/home/.config/mako/config` prints no `Failed to parse option` line, while a file with an unknown key does (`[mako-bad:2] Failed to parse option 'not-a-key=1'`).
  receives exactly the expected hints; the mako config parses the same file.
- **Hardware**: the volume/brightness OSD still replaces one card, a critical card shows over
  a fullscreen video, and the icon is visible (not blank).

## Risks

- **A `notify-send` call kept in a script but bypassed by the new path** — the check greps
  both the scripts and the Rust call sites for an application name without an icon.
- **mako's history keeps a secret** — the secret helper is the only allowed password path and
  it is unit-tested.
