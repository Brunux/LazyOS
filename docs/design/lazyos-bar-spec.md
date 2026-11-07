# LazyOS Native Bar — Product & Interaction Specification

**Status:** proposed implementation brief  
**Reference implementation target:** SwayFX / Wayland on ARM64 (Google Hayato / MT8192)  
**Primary constraint:** low idle CPU and memory use; no continuous animation or high-frequency polling.

## 1. Product intent

Build a native graphical Wayland status bar that visually belongs to LazyOS: a terminal-inspired shell surface with monospaced typography, command-like panel titles, concise status output, sharp geometry, semantic color, and keyboard-first interaction.

This is **not** a terminal emulator in a panel and is **not** a Ratatui application. Ratatui remains the rendering system for terminal windows managed by `lazyos-ui`. The bar is a separate Wayland layer-shell client.

The bar replaces Waybar only after feature parity and hardware validation. Waybar remains installable and selectable as a fallback during development.

## 2. Design references

- [Bar overview](lazyos-bar-overview.svg): provisional top-level composition, spacing and state colors.
- [Panel and menu map](lazyos-bar-panels.svg): initial panel inventory, not a complete visual specification.
- [Complete menu tree](lazyos-bar-menu-tree.md): canonical root categories, nested routes, stable IDs, availability rules and indicator context menus.
- [Menu tree diagram](lazyos-bar-menu-tree.svg): graphical overview of all root categories and major nested routes.
- [Panel visual contracts](lazyos-bar-panel-spec.md): detailed layout, states, controls and interaction behavior for every required panel.
- [Panel gallery](lazyos-bar-panel-gallery.svg): provisional graphical wireframes for the palette, network, audio, calendar, task, confirmation and shared panel frame.
- **Reference-image fidelity is not yet validated.** The earlier SVGs are assistant-authored provisional diagrams, not faithful reproductions of the original generated concept image. Do not treat their geometry or styling as pixel-accurate source material. The original image remains the visual source of truth once reattached.

## 3. Visual system

### 3.1 Identity
- Wordmark: `z_` followed by `LazyOS`; launcher opens the root command palette.
- Typeface: JetBrains Mono first choice; monospace fallback.
- Default theme: existing Solarized Dark tokens from `lazyos-theme`.
- Corners: square or very small radius (0–4 px); no pill-shaped UI.
- Borders: thin, low-contrast separators; focused control uses accent border/foreground.
- Transparency: restrained translucent background; optional SwayFX blur must degrade gracefully.
- Icons: existing LazyOS glyph/icon mapping where available; always retain textual/accessibility labels.

### 3.2 Palette semantics
Consume semantic roles from `lazyos-theme`; do not hard-code a second palette:
- background / surface / elevated surface
- text / muted text
- accent / accent foreground
- info, success, warning, danger
- disabled / unavailable

Solarized Dark reference values: base03 `#002b36`, base02 `#073642`, base01 `#586e75`, base00 `#657b83`, base0 `#839496`, base1 `#93a1a1`, base2 `#eee8d5`, yellow `#b58900`, orange `#cb4b16`, red `#dc322f`, magenta `#d33682`, violet `#6c71c4`, blue `#268bd2`, cyan `#2aa198`, green `#859900`.

### 3.3 Geometry (initial targets; tune on device)
- Top layer, anchored top, exclusive zone enabled.
- Bar height 30–34 logical px; horizontal outer margin 6 px; module gap 4–6 px.
- Left group: launcher, workspaces, compositor mode.
- Center group: clock/date, centered independently of left/right widths.
- Right group: busy, updates, tray, keyboard layout, Bluetooth, VNC, network, audio, brightness, idle inhibitor, battery.
- Indicators collapse when genuinely irrelevant (e.g. no pending updates or only one keyboard layout); persistent toggles such as idle inhibitor remain visible.
- Avoid clipping on narrow outputs and fractional scaling; define overflow strategy (compact labels then overflow menu).

## 4. Interaction model

### 4.1 Global behavior
- Left click opens the relevant panel or performs the indicator's primary safe toggle.
- Right click exposes a secondary action or opens a terminal diagnostic only where useful.
- Scroll is reserved for continuous controls (volume, brightness, calendar month); never hijack workspace scrolling.
- Panels open anchored to their indicator when possible; otherwise centered below the bar.
- One panel is active at a time; opening another replaces it or explicitly nests a child view.
- Escape closes current panel; Backspace returns one level; Enter activates; arrows navigate; Tab changes pane/control; printable keys filter lists.
- Mouse hover/click/wheel supported, but no action may require a mouse.
- Focus must be restored to the prior surface on close.
- Every action has a visible loading, success, failure, or unavailable state. Long-running work stays in its originating task view, with live progress/logs and a safe cancel action when supported.
- Destructive actions require a confirmation view; power-off/reboot must never happen from a single ambiguous click.

### 4.2 Bar controls and submenus

| Control | Primary interaction | Panel / submenu |
|---|---|---|
| `z_ / LazyOS` | Open root menu | Command palette, categorized menu, search |
| Workspaces | Switch workspace | Workspace overview; optional move-window-to-workspace actions |
| Sway mode | Show active mode | Mode help / keybinding reference |
| Clock | Open calendar | Month grid, today, previous/next month, optional agenda if a provider exists |
| Busy | Show current task; click stop only if task is cancellable | Task detail, progress, log tail, cancel confirmation/result |
| Updates | Open updates view | Check, list packages, changelog, install, progress, result |
| Tray | Activate application item | Native StatusNotifierItem menu/action |
| Keyboard | Cycle layout | Available layouts, current layout, set default |
| Bluetooth | Open device panel | Radio toggle, scan, paired devices, available devices, connect/disconnect, trust/pair/remove, device details |
| VNC | Open sharing panel | Start/stop read-only sharing, endpoint/status, security warning |
| Network | Open network panel | Wi-Fi/Ethernet/VPN, radio toggle, scan, saved networks, connect/disconnect, forget, details, diagnostics |
| Audio | Left click mute; right click devices | Output/input device selection, volume, mute, per-device details; scroll volume |
| Brightness | Scroll adjust; click opens display panel | Brightness slider, displays, night light/gamma schedule if supported |
| Idle inhibitor | Toggle keep-awake | Current state, duration presets (optional), turn off |
| Battery | Open power panel | Charge/state, estimate if available, power profile, battery health if exposed |

### 4.3 Root command palette taxonomy

The launcher is a searchable palette, not a static oversized menu. Categories and initial entries:

- **System:** Overview, Processes, Resource monitor, Disks & mounts, Services, Logs, Updates, About, Power/session.
- **Connectivity:** Wi-Fi, Ethernet, VPN, Bluetooth, Screen sharing, Network diagnostics.
- **Personalization:** Themes, Wallpapers, Bar layout, Workspaces, Keyboard layouts, Display, Brightness/night light, Notifications, Sound, Idle inhibitor.
- **Applications:** Application launcher, Run command, Recent applications, Default applications.
- **Files & tools:** Search files, File manager, Terminal, Clipboard/history if installed, Help, Keybindings.
- **Session:** Lock, Logout, Restart compositor/session, Reboot, Shutdown (confirm).
- **Developer (conditional):** LazyOS status, system diagnostics, logs, package/build tools; hide when not installed or not applicable.

Do not create dead menu items. Entries must be capability-discovered or visibly disabled with a reason.

## 5. Panel specifications

### 5.1 Shared panel anatomy
Every panel has: header (glyph + command-like title + state/breadcrumb), body (rows/forms/graphs), footer (context-sensitive key hints), and statusline (live context). Match the existing `lazyos-ui` anatomy without importing Ratatui as the renderer.

Panel states: idle, loading, ready, empty, error, confirming, running, completed, cancelled. Async results must not block the Wayland event loop.

### 5.2 Required panel set
1. **Root palette:** fuzzy filter, category grouping, recent/favorite commands, keyboard navigation.
2. **Calendar:** month navigation, today marker, selected date; no network dependency.
3. **Network:** connection state, signal, IP/gateway, radio controls, scan results, saved profiles, connect/disconnect/forget, diagnostics.
4. **Bluetooth:** adapter state, paired/available devices, scan progress, pair/trust/connect/disconnect/remove.
5. **Audio:** default sinks/sources, mute, volume, device switching; optional per-app streams only if backend is available.
6. **Display:** brightness, connected displays, scale/resolution only where safely supported, night-light/gamma status.
7. **Power:** battery state/capacity/charging, estimate when available, idle inhibitor, power profile; avoid fabricating estimates.
8. **Updates:** pending count, package list, check/download/install task states, logs and result.
9. **Busy task:** current operation, progress/indeterminate state, elapsed time, log output, cancellation capability and result.
10. **Bluetooth/network device details:** device-specific metadata and safe actions.
11. **Notifications:** notification list/history, dismiss, clear, per-app controls only if notification backend supports them. Existing Mako remains notification daemon until a separate replacement is explicitly scoped.
12. **Workspace overview:** current/focused/urgent workspaces, window list and move actions.
13. **System overview:** CPU, memory, swap, storage, temperature, battery, kernel, uptime; sample at a conservative rate and suspend expensive sampling when closed.
14. **VNC/share:** read-only sharing state, start/stop, endpoint, explicit security state.
15. **Theme selector:** installed theme list, preview, apply/reload; use `lazyos-theme` as source of truth.
16. **Bar settings:** left/center/right layout, visibility, compact mode, output placement, height, theme, reload/reset.
17. **Help/keybindings:** searchable Sway/LazyOS shortcuts, mode explanation, discoverable actions.
18. **Session/power confirmation:** lock/logout/reboot/shutdown; typed confirmation only if required by existing system safety policy.

Optional panels (not phase-one blockers): clipboard history, per-app audio, calendar agenda, network traffic graph, process kill UI, notification rules.

## 6. Architecture boundaries

Proposed crate: `rust/crates/lazyos-bar` (standalone binary). Keep the following responsibilities separated:

- **Wayland shell:** layer-shell surface, output discovery, exclusive zone, keyboard focus, pointer input, scale handling.
- **Renderer/widgets:** bar layout, panel surfaces, text, glyphs, progress bars, sparklines; no system command execution in paint code.
- **State model:** typed state/events and panel navigation; rendering is a pure projection of state.
- **Adapters:** Sway IPC, NetworkManager/nmcli, BlueZ/bluetoothctl, PipeWire/PulseAudio pactl, brightness/sysfs, UPower/sysfs, StatusNotifierItem, update backend, existing LazyOS busy protocol.
- **Theme adapter:** consume `lazyos-theme` semantic tokens and reload on theme change.
- **Configuration:** TOML under XDG config; validate, preserve defaults, tolerate missing optional modules.
- **Diagnostics:** structured logs to XDG state/runtime paths; no noisy logs by default.

Keep existing shell commands as compatibility adapters initially. Port behavior to Rust only when it removes meaningful process overhead or improves state/event handling. Preserve current `bar-busy` state contract and cancellation semantics.

### Renderer decision
Do a short hardware spike before committing to a toolkit:
- Compare GTK4 + gtk4-layer-shell against a lean Smithay Client Toolkit / Wayland client implementation.
- Measure resident memory, idle CPU, wakeups, startup time, frame time, input latency, fractional scaling, SwayFX blur compatibility, and tray support on the target Chromebook.
- Choose the simplest renderer that meets the budget. Do not select a stack on language preference alone.
- Ratatui is not a candidate for the graphical layer surface.

### Event and performance rules
- Event-driven updates where possible (Sway IPC, D-Bus, file watches, netlink/backend events).
- Poll only when unavoidable; use per-source intervals and stop timers when panel hidden.
- No one-second shell process loop for an idle indicator.
- No animation loop. Transitions may repaint briefly (target ≤150–200 ms); disable effects under load or reduced-motion preference.
- Cache static labels/icons; do not rebuild layout on every unchanged sample.
- No network calls in the bar process unless an explicit feature requires them.
- Graceful degradation when optional services are absent.

## 7. Compatibility and migration

- Keep Waybar config/scripts intact during prototype.
- Add a separate service/launcher and explicit config switch; never replace the default silently.
- Ensure bar crash/restart does not affect Sway session.
- Feature parity checklist against current Waybar controls before default switch.
- Provide a one-command rollback to Waybar.
- Verify multi-output behavior even if Hayato currently uses one output.

## 8. Delivery phases

1. **Design + renderer spike:** panel/bar prototypes, theme adapter, benchmark GTK4 layer-shell vs lean Wayland client.
2. **Shell MVP:** bar surface, launcher, workspaces, mode, clock/calendar, output/scaling, keyboard navigation.
3. **Core status:** busy, updates, tray, keyboard, Bluetooth, VNC, network, audio, brightness, idle inhibitor, battery.
4. **Panels:** root palette and required panels; asynchronous task lifecycle.
5. **Polish:** theme switching, settings, notifications bridge, accessibility, multi-output, recovery/fallback.
6. **Migration:** on-device profiling, parity tests, documentation, opt-in default, eventual Waybar deprecation only after acceptance.

## 9. Acceptance criteria

- [ ] Matches the source-controlled visual references and existing LazyOS theme tokens.
- [ ] Top layer reserves the correct exclusive zone and behaves correctly under SwayFX.
- [ ] Left/center/right layout remains balanced at target and narrow resolutions.
- [ ] All current Waybar interactions have parity or an explicitly documented replacement.
- [ ] Every required panel has keyboard and mouse operation, visible focus, contextual footer, empty/error/loading states.
- [ ] Slow actions are asynchronous and show progress/logs/results in the panel that started them.
- [ ] Destructive actions are confirmed; cancellation is exposed only when safe.
- [ ] Missing services do not crash the bar or show misleading state.
- [ ] Theme changes propagate without restarting the session.
- [ ] No continuous animation; idle CPU and memory are measured on target hardware.
- [ ] Multi-output and fractional scaling are tested.
- [ ] Waybar fallback remains documented and functional.
- [ ] Automated tests cover state transitions, config validation, adapters and panel navigation.

## 10. Non-goals for the first release

- Replacing `lazyos-ui` / Ratatui.
- Replacing Mako's notification daemon.
- Building a general-purpose terminal emulator.
- Reimplementing every existing shell command before parity is proven.
- Adding always-on graphs, telemetry, network services, or decorative animation.
- Removing Waybar before the native bar is stable on the actual Chromebook.

## 11. Repository touchpoints

- `rust/Cargo.toml` — add the new crate when implementation begins.
- `rust/crates/lazyos-theme` — shared palette and theme reload.
- `uis/sway/files/home/.config/waybar/config.jsonc` and `style.css` — current behavior and visual baseline.
- `uis/sway/files/home/.config/bin/bar-*` — compatibility adapters and existing interaction contracts.
- `docs/plan-ui.md` — Ratatui terminal UI framework; keep responsibilities distinct.
- `docs/design/lazyos-bar-overview.svg` and `docs/design/lazyos-bar-panels.svg` — source-controlled graphical references.


## 12. Complete nested navigation and panel contracts

The canonical menu hierarchy and stable route model are defined in [lazyos-bar-menu-tree.md](lazyos-bar-menu-tree.md). The detailed panel geometry, reusable components, visual states, and behavior are defined in [lazyos-bar-panel-spec.md](lazyos-bar-panel-spec.md). These are normative for functional scope; the SVGs are explanatory wireframes.

Implementation requirements:
- Build menu entries from typed route data with stable dotted IDs; never use translated labels as IDs or execute arbitrary shell strings from menu configuration.
- Root palette and indicator context menus must dispatch the same action IDs.
- Every nested view must expose a breadcrumb and a deterministic back route.
- Search must index nested actions and show their breadcrumb.
- Capability-gated features must be hidden or disabled with an explicit reason.
- Confirmation, progress, error, empty, and completed states are part of each panel contract, not optional polish.
- The panel gallery is a provisional layout reference, not the final visual target. Reconcile exact proportions, spacing, typography, glyphs, and color treatment with the original concept image before declaring visual sign-off.

## 13. Design sign-off status

- [x] Root category taxonomy and full nested menu routes documented.
- [x] Indicator context menus and shared action routing documented.
- [x] Shared panel frame and reusable graphical components documented.
- [x] Required panel layouts and visual state matrix documented.
- [x] Provisional SVG diagrams added for menu tree and representative panels.
- [ ] Original generated concept image reattached and inspected.
- [ ] SVGs and dimensions reconciled against the original image.
- [ ] Visual sign-off on target Chromebook at native and fractional scaling.
