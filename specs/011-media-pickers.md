# Spec 011 — Media and utility pickers

- **Status**: draft
- **Phase**: 3
- **Depends on**: 002, 003, 005, 006
- **Model**: `claude-haiku-5-5` — a fixed shape repeated over ten surfaces, with fixtures and a
  handful of small parsers, after 009/010 have set the pattern; spec 006 showed Haiku 5.5 carries
  a well-shaped surface in this repository. Fallback `claude-sonnet-5-5`.
- **Branch**: `spec/011-media-pickers`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §5.8-§5.10, §16 phase 3
- **Old paths**: `rofi-media`, `rofi-notify`, `rofi-clip`, `rofi-emoji`, `rofi-capture`,
  `rofi-qr`, `rofi-remind`, `rofi-calc`, `rofi-color`, `bar-*` audio/bluetooth/mpris panels

## Problem

Ten small menus each repeat the same rofi plumbing and the same three bugs: a slow action
without a stop, a destructive action without a confirm, and a card without an icon. The
calculator is a one-shot rofi prompt, the emoji picker inserts through `wtype`, the clipboard
is a dmenu over `cliphist`, and the reminder menu has its own calendar recursion.

## Scope

**In (`lazyos menu media|notify|clip|emoji|capture|qr|remind|calc|audio|bluetooth`):**

- **Media**: player state (playerctl), play/pause/next/prev, the current track's art as a
  sixel card when available, seek and volume rows.
- **Notify history**: the last N mako cards (read from mako's own history if enabled, else the
  in-memory log) with dismiss/clear.
- **Clipboard**: `cliphist` rows by kind (text/link/image), copy back, save the newest image,
  clear the database; the secret rule (spec 006) applies.
- **Emoji**: the committed `emojis.tsv` table, fuzzy filter, insert through `wtype` (Alt+1 =
  clipboard only); the font pin is checked.
- **Capture**: screenshot (screen/area/window), annotate (swappy), OCR (tesseract), QR read
  (zbarimg), record (wf-recorder start/stop with a task + pid), each ending in a card with the
  path/thumbnail.
- **QR**: the Wi-Fi payload with the escaped fields, rendered in-window as a QR (no foot
  popup, no temp PNG).
- **Remind**: the title → note → when flow, `systemd-analyze calendar` validation, the repeat
  groups, the list/cancel view — as a real menu instead of nested rofi prompts.
- **Calc**: an expression parser (no rofi prompt), history, a copy action.
- **Audio/Bluetooth**: the bar panels as menus, with the device lists and connect/disconnect.

**Out:** the capture post-actions popup beyond a card (a later polish), the media bar module
(014), the notification daemon (013), dictation (its own spec in the plan, later).

## Design

- The ten surfaces share `List`/`Task`/`Card`; the only new logic is per-surface parsing and
  the data each reads (`playerctl -F` once per open, `cliphist list`, the emoji TSV, the
  calendar parser).
- Capture's recorder is the one long-lived child: pid + file in `$XDG_RUNTIME_DIR/lazyos-capture`,
  start/stop from the same list, SIGINT to finalise — already the shell's contract, kept.
- The calculator parser rejects rather than guesses (`2+` is an error with the position), and
  never evaluates a function name it does not know.
- The QR renderer draws in half-block cells, so it needs no sixel and no PNG.

## Acceptance criteria

- [ ] Media shows the current track and art, and the transport keys work; with no player, an
      empty state names the command.
- [ ] Clipboard lists text/link/image rows, copies back, and clears; an image row saves to
      `~/Pictures/clipboard/<date>` and opens it.
- [ ] Emoji inserts into the focused surface; a glyph the font does not draw is absent from the
      list (the pin is compared).
- [ ] Capture: a region screenshot lands in the dated folder, the card carries the thumbnail,
      OCR copies the text, a recording starts and stops into a playable file.
- [ ] Remind: an invalid calendar expression shows systemd's own message and returns to the
      When menu with the title intact; a valid one lists with its next occurrence.
- [ ] Calc: a valid expression evaluates, an invalid one reports the position; history copies.
- [ ] witness: `scripts/check-system.sh` FAILs on a missing tool per surface (playerctl,
      cliphist, wtype, grim/swappy/tesseract/zbarimg/wf-recorder, qrencode, systemd-analyze),
      on the emoji font pin mismatch, and on a missing subcommand.
- [ ] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/04-software.md` per group
      (media, clipboard, emoji, capture, reminders, calculator).
- [ ] witness: an `AGENTS.md` bullet says the ten surfaces are one shape and the recorder is
      the only long-lived child.
- [ ] unit tests: emoji TSV filter/pin, QR payload escaping, calculator parser (valid, invalid,
      precedence), calendar validation wrapper, cliphist row classification, track metadata.

## Tests

- **Unit** (`cargo test`): `qr_escapes_the_four_characters`, `calc_precedence_and_errors`,
  `emoji_rows_match_the_pin`, `cliphist_classifies_image`, `calendar_rejects_25_00`.
- **Snapshot**: each surface's normal/empty/error; the QR card.
- **Integration**: a fake `playerctl`/`cliphist`/`wtype` on `PATH` asserts the exact argv;
  the calculator is driven by a key sequence.
- **Hardware**: take a screenshot, OCR a page, scan the Wi-Fi QR with a phone, set a reminder
  one minute out, and paste an emoji into foot.

## Risks

- **A tool's CLI changes** (playerctl fields, cliphist columns) — each parse has a fixture and
  a visible error row.
- **The recorder survives a crashed menu** — the pid file is checked with `kill -0` and the
  file is the notification's, so a stale state is reported, never silently reused.
