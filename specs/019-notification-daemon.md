# Spec 019 — The Notification Daemon: Eye-Candy Layer-Shell Cards, Actions, and Progress

- **Status**: merged (PR #44; layer-shell lazyos-notify daemon with action buttons and progress)
- **Phase**: 5
- **Depends on**: 002, 004, 006
- **Model**: `claude-opus-5-5` (effort `high`) — Wayland `wlr-layer-shell` protocol client, D-Bus `org.freedesktop.Notifications` service implementation, `tiny-skia` 2D software card rendering, interactive hit testing for action buttons, and history persistence. Fallback `claude-sonnet-5-5`.
- **Branch**: `spec/019-notification-daemon`
- **Plan**: [`docs/plan-ui.md`](../docs/plan-ui.md) §9 (The notification daemon)
- **Old paths**: `mako`, `makoctl`, `~/.config/mako/config`, `~/.config/bin/rofi-notify`

## Problem

LazyOS currently relies on `mako` for desktop notifications. While mako is lightweight, it falls short of the LazyOS design language and interactive capabilities in critical areas:
1. **No Interactive Action Buttons**: Mako parses notification actions but cannot render clickable action buttons—it requires keyboard navigation or scripts to invoke them.
2. **No Numeric Progress Bars or Task Colors**: Progress hints are crude; mako lacks smooth percentage gauges with semantic per-task colors for long-running operations (like file transfers, backups, or package installations).
3. **Private, Inflexible History**: Mako stores history in an opaque internal buffer accessible only via `makoctl history`, making it impossible for `lazyos menu notify` to directly query, search, filter, or categorize past notifications.
4. **Visual Disconnect**: Mako draws basic rectangular boxes with Pango markup that do not fully capture the polished eye-candy of the session (soft card shadows, subtle border radii matching SwayFX, and palette synchronization driven natively by `theme.toml`).

## Scope

**In (`lazyos-notify` daemon crate & `lazyos notify` integration):**

- **Wayland Layer-Shell Client**:
  - Anchors to `top-right`, positioned below the Waybar bar on the `overlay` layer.
  - Utilizes pure-Rust 2D software rendering via **`tiny-skia` + `cosmic-text`** over Wayland shared memory buffers (`wl_shm`).
  - Avoids GPU/OpenGL overhead and heavy GTK/Qt toolkits, maintaining LazyOS's low-memory footprint on the Mali-G57 GPU.
  - Zero CPU when idle: repaints only upon arrival, animation tick, progress update, or user interaction.
- **Eye-Candy Card Design & Layout**:
  - Consistent with LazyOS design tokens: rounded corners (`corner_radius 8`), subtle outer drop shadow, and 1px border matching the urgency role (`dim` for low, `accent` for normal, `danger` for critical).
  - Background surface matches `theme.toml` elevation (`bg-elevated` with translucent alpha).
  - High-resolution app icon rendering (direct PNG decoding) or fallback to single-advance Nerd Font glyphs (`Glyph`).
  - Title in bold accent, wrapped body text with automatic URL detection.
  - **Embedded Progress Bar**: Smooth horizontal gauge displaying percentage (0%–100%) with task-specific accent colors.
  - **Interactive Action Buttons**: Visual pill buttons (e.g. `[ Open ]`, `[ Dismiss ]`, `[ Retry ]`) with hover feedback and click handling.
- **D-Bus Specification Compliance**:
  - Implements `org.freedesktop.Notifications` over the session D-Bus.
  - Methods: `Notify`, `CloseNotification`, `GetCapabilities`, `GetServerInformation`.
  - Signals: `NotificationClosed`, `ActionInvoked`.
  - Supports standard hints: `urgency`, `category`, `value` (progress), `image-path`, `image-data`, `sound-file`, and synchronous tag updates (`x-canonical-private-synchronous`).
- **Interactive Gestures & Inputs**:
  - Left click on card body invokes the default action.
  - Click on specific action button emits `ActionInvoked` with that action identifier.
  - Right click dismisses the individual notification.
  - Middle click dismisses all active notifications of the same category or app.
- **Public Structured History**:
  - All received, dismissed, or expired cards are recorded to `$XDG_STATE_HOME/lazyos/notify/history.jsonl`.
  - `lazyos menu notify` reads this structured file directly, eliminating the need for `makoctl`.
- **Gentle Motion & Animations**:
  - Smooth 200 ms slide-in / fade-in using cubic bezier easing.
  - Immediately bypassed when `LAZYOS_REDUCED_MOTION=1` is set.

**Out:**

- Full sound synthesizer (audio alerts use standard system sound cues via PipeWire/libcanberra if enabled).
- In-window TUI notification viewer changes (`lazyos menu notify` already provides the viewer and will read the new history directly).

## Design

### Why Software Rendering via `tiny-skia` + `cosmic-text`

Building a custom notification surface in Wayland requires layer-shell support (`zwlr_layer_shell_v1`). Ratatui is designed for terminal character cells and cannot draw floating transparent overlay surfaces directly on the compositor without launching a foot terminal window. Conversely, GTK4/Adwaita or Qt6 would introduce 50+ MB of resident shared libraries.

`tiny-skia` provides a high-performance, pure-Rust 2D drawing pipeline:
- Antialiased paths for rounded rectangles, pill buttons, and radial badges.
- Crisp vector clipping and alpha blending.
- Blazingly fast CPU rendering on ARM64 Cortex-A76 cores (a 360x100 card renders in under 1.2 ms).
- Software buffer (`wl_shm`) transfers directly to Wayland without GPU context switching.

### State Protocol & History Architecture

```
[notify-send / Apps / Card::send()]
             │  (D-Bus: org.freedesktop.Notifications)
             ▼
     ┌────────────────────────┐
     │     lazyos-notify      │ ◄─── Reads theme.toml & icons
     │  (wlr-layer-shell)     │
     └───────────┬────────────┘
                 │
                 ├──► [Screen Output: Overlay Cards + Action Buttons]
                 │
                 └──► [Appends to ~/.local/state/lazyos/notify/history.jsonl]
                             │
                             ▼
                     [lazyos menu notify (TUI Viewer)]
```

### Replacement & Coexistence

- The daemon is named `lazyos-notify` and installed at `/usr/bin/lazyos-notify`.
- Started as a systemd user unit `lazyos-notify.service`, replacing `mako.service`.
- When `lazyos-notify` lands, `mako` is cleanly removed from `uis/sway/packages.list`.
- `lazyos theme set` signals `lazyos-notify` via D-Bus reload, atomically updating active colors.

## Acceptance criteria

- [x] `lazyos-notify` registers `org.freedesktop.Notifications` on session D-Bus; standard `notify-send "Test" "Body"` displays an eye-candy card.
- [x] Notifications with actions render interactive visual buttons; clicking a button invokes `ActionInvoked` with the corresponding action key.
- [x] Notifications with the `value` progress hint render a smooth progress bar reflecting the integer percentage.
- [x] Cards display rounded corners, drop shadows, and border colors matching the active theme's urgency role.
- [x] In-place updates (`replaces_id` or matching synchronous tags) update title, body, and progress bar without re-triggering entrance animations.
- [x] Right-clicking a card immediately dismisses it and frees the layer-shell surface.
- [x] Notifications are recorded in `$XDG_STATE_HOME/lazyos/notify/history.jsonl` with timestamp, app name, summary, body, and urgency.
- [x] `lazyos-notify` consumes ≤ 15 MB PSS when active with notifications on screen, and ≤ 8 MB PSS when idle with zero cards displayed.
- [x] compatibility: `notify-send`, `lazyos notify`, and all existing shell scripts continue working without modification.
- [x] witness: `scripts/check-system.sh` verifies `lazyos-notify` is running, asserts D-Bus ownership of `org.freedesktop.Notifications`, and fails if `mako` remains running.
- [x] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/05-system.md` § *Notifications* documents the eye-candy layer-shell daemon, action buttons, and history format.
- [x] witness: an `AGENTS.md` bullet documents the `tiny-skia` software rendering choice and D-Bus action architecture.
- [x] unit tests: D-Bus message deserializer, action parser, history serializer, and `tiny-skia` layout engine are covered by `cargo test`.

## Tests

- **Unit** (`cargo test`):
  - `parse_notification_actions`: verifies parsing of action tuples `["default", "Open", "cancel", "Cancel"]`.
  - `progress_clamping`: ensures `value` hints outside 0..=100 are safely clamped.
  - `history_jsonl_roundtrip`: validates serialization and deserialization of history records.
  - `card_layout_bounds`: verifies that long summaries and bodies wrap cleanly within maximum dimensions.
- **Snapshot / Render**:
  - Pixel snapshot test comparing rendered `tiny-skia` pixmaps across Normal, Critical, Progress, and Action card states against golden reference PNGs.
- **Integration**:
  - Spawn `lazyos-notify` in a mock Wayland/DBus environment.
  - Send notification via `notify-send -a "TestApp" -i "dialog-information" "Hello" "World"`.
  - Assert notification window surfaces via layer-shell protocol.
- **Hardware**:
  - Boot on Hayato Chromebook under SwayFX compositor.
  - Trigger volume/brightness keys to verify OSD progress bar update fidelity.
  - Trigger test action notification and verify finger touch / mouse click on action button.

## Risks

- **Layer-shell protocol version mismatch** — mitigated by negotiating `zwlr_layer_shell_v1` version 1..4 gracefully.
- **Font rendering memory bloat** — `cosmic-text` font database is initialized once from system fontconfig caches, preventing redundant font scans.
