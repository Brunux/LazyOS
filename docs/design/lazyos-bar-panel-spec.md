# LazyOS Bar — Panel Visual & Interaction Contracts

**Status:** detailed provisional wireframe specification. These contracts are ready for implementation, but the final pixel styling must be reconciled with the original concept image when it is available.

## Shared panel frame

Every native panel uses the same frame and state language:

```text
┌─ [glyph] panel.command ───────────────────────── [state] ─┐
│ path: parent / current                                    │
├───────────────────────────────────────────────────────────┤
│                                                           │
│                     panel content                         │
│                                                           │
├───────────────────────────────────────────────────────────┤
│ [↑↓] navigate  [enter] select  [backspace] back  [esc] close│
└─ status: contextual message / result / service availability ┘
```

- Panel title is a short command-like noun (e.g. `networkctl`, `audioctl`, `calendar`), not a fake terminal prompt that implies a command was executed.
- Breadcrumb sits below the title when nested. The title remains stable for the panel family.
- Header height, row height, border, padding, and footer are shared tokens. Start with a compact 8 px content inset, 24–28 px rows, 1 px separators, and 30–34 px bar height; tune after the source image is available and on-device scaling is tested.
- Use a two-column layout only when it improves scanning: list left, details right. Collapse to a single column on narrow widths.
- Focused rows use an accent left marker and accent foreground/background; selection must not rely on color alone.
- Disabled rows use muted text plus a short reason on focus.
- Graphs are optional, low-resolution sparklines with labels and numeric values; never use continuously animated charts.
- All panel controls are keyboard reachable, have accessible names, and support pointer interaction.
- Panel dimensions are content-driven with safe maximum width/height. Long lists scroll internally; footer remains visible.

## Reusable visual components

| Component | Appearance | Behavior |
|---|---|---|
| Command header | glyph + monospaced panel name + state at right | stable across panel updates |
| Breadcrumb | muted route text | back navigation |
| Menu row | icon, label, short description/value, right chevron or shortcut | focus, activate, toggle |
| Toggle row | label + explicit ON/OFF state | Enter/Space toggles; state changes only after backend confirms |
| Slider row | label + value + horizontal track | arrows adjust by small step; PageUp/PageDown by larger step |
| Device row | name + type + connection/status | opens device detail |
| Section label | uppercase/muted, letter spacing | visual grouping only |
| Progress row | determinate bar or indeterminate text | updates on backend events |
| Log viewer | timestamp + severity + wrapped message | pause/follow/copy; bounded buffer |
| Confirmation | target + consequence + Cancel/Confirm | default focus is Cancel |
| Empty state | concise explanation + valid next action | never a blank panel |
| Error state | short cause + retry/details action | preserve last known good state where safe |
| Footer | key hints, context-sensitive | always visible when panel open |

## Panel layouts

### 1. Root command palette — `launcher`

- Width: 560–720 logical px on desktop; clamp to available output width.
- Header: `z_ / command`, search input, result count.
- Body: left category rail (System, Connectivity, Personalization, Applications, Files & tools, Session, Developer when available); right result list.
- Rows: icon, action label, one-line description, shortcut/right chevron.
- Initial focus: search field; typing immediately filters all available commands.
- Empty state: “No matching commands” plus clear-search hint.
- Footer: arrows, Enter, Backspace, Esc, Ctrl+K.
- Narrow mode: category rail becomes a compact category selector above the result list.

### 2. Calendar — `calendar`

- Header: month/year with previous/next controls and Today action.
- Body: 7-column month grid, weekday labels, current day marker, selected day marker.
- Footer: Left/Right month, T Today, Enter select, Esc close.
- No network dependency; agenda is an optional provider and must not appear as an empty placeholder.

### 3. Network — `networkctl`

- Header: connection status + radio state.
- Left/list region: Wi-Fi, Ethernet, VPN sections and profiles; scan results appear under Wi-Fi.
- Right/details region: selected network name, security, signal, interface, IP, gateway, DNS when supplied by backend.
- Connect flow: select SSID → details → password form when needed → connecting progress → success/error.
- Forget flow: profile detail → confirmation naming profile → result.
- Radio toggle is explicit and reports pending/failure states; never optimistically display success.
- Diagnostics are a nested page, not an unlabelled button cluster.

### 4. Bluetooth — `bluetoothctl`

- Header: adapter name + powered state.
- Sections: paired devices, nearby devices (only during/after scan).
- Device row: device name, type, connected/paired state, signal when available.
- Device detail: address (copyable), trusted state, connection state, supported actions.
- Pairing: explicit pairing state; show passkey/PIN and require user confirmation where BlueZ requests it.
- Remove device requires confirmation.

### 5. Audio — `audioctl`

- Top summary: output name, volume percentage, mute state.
- Main control: slider with numeric percentage; mute toggle.
- Device selectors: output and input as separate lists, each with active marker.
- Optional per-application streams are hidden unless the backend exposes them reliably.
- Scroll on the indicator adjusts default output volume; keyboard arrows adjust slider.
- Clamp at backend limits; do not assume 100% is the maximum if amplification is supported.

### 6. Display — `displayctl`

- Brightness summary and slider at top.
- Connected display list with active output marker.
- Resolution/scale controls appear only if the compositor/backend exposes safe supported modes.
- Night light/gamma schedule appears only when a working provider exists; otherwise omit it.
- Brightness changes are debounced and committed through the backend; show failure feedback.

### 7. Power — `powerctl`

- Battery summary: percentage, charging/discharging state, estimate only if available.
- Power profile selector only when supported by the platform.
- Idle inhibitor state and optional duration presets.
- Session actions are links to the Session menu; do not mix shutdown into a battery detail view.
- Never fabricate battery health or remaining-time estimates.

### 8. Updates — `updates`

- Header: repository/package backend status and pending update count.
- Actions: Refresh, Review, Install.
- Package list: name, installed version, candidate version, size only if known.
- Install opens an owned task state: queued → downloading → installing → completed/failed.
- Logs are bounded and scrollable; Cancel appears only when the package manager supports safe cancellation.
- Do not start installation from a single accidental row click.

### 9. Busy task — `taskctl`

- Header: task label + running/paused/failed/completed state.
- Body: progress bar or indeterminate activity text, elapsed time, current phase.
- Expandable log tail and optional copy-log action.
- Stop action shown only if the existing busy protocol/backend says the task is cancellable.
- Preserve the current `$XDG_RUNTIME_DIR/lazyos-busy` contract as an adapter; do not change its semantics.

### 10. Notifications — `notifyctl`

- Read existing Mako notification state only if a supported IPC/history provider exists.
- Show app/source, summary, body preview, timestamp, urgency.
- Actions: dismiss one, clear visible history, open source app if supplied.
- Do not claim persistent history if Mako/backend only exposes live notifications.
- Notification settings route to Mako configuration/help; this project does not replace Mako.

### 11. Workspace overview — `workspaces`

- Workspace strip/list with current, occupied, urgent and empty states.
- Window list grouped by workspace; each row shows app/title and focused marker.
- Actions: focus workspace, focus window, move window to workspace.
- Confirm only actions that would destroy state; moving/focusing is safe.
- Do not use scroll on the workspace strip to change workspaces unless explicitly configured.

### 12. System overview — `sysinfo`

- Compact summary tiles: CPU, memory, swap, root/storage, temperature, battery, uptime.
- Each value includes units and freshness; unavailable sensors display “n/a”.
- Sampling starts on open and stops or slows when hidden.
- No animated graphs; optional short history sparkline uses bounded samples.

### 13. Sharing / VNC — `sharectl`

- Status: stopped/running, endpoint, active clients if the server exposes them.
- Start requires a clear security explanation and explicit confirmation.
- Stop warns if active clients will be disconnected.
- Never expose credentials in the bar; endpoint visibility and copy action follow existing security policy.
- Clearly label read-only vs interactive access.

### 14. Theme selector — `themes`

- List installed themes with name and semantic preview swatches.
- Preview is temporary; Apply commits; Cancel restores previous theme.
- Theme changes propagate through `lazyos-theme` without restarting the session.
- Missing theme files produce an error row, not a crash.

### 15. Bar settings — `barsettings`

- Sections: modules, left/center/right placement, visibility, compact mode, output, density.
- Changes preview live where safe; Apply/Reset semantics are explicit.
- Reset requires confirmation.
- Invalid configuration shows field-level error and retains last valid config.

### 16. Help and keybindings — `help`

- Searchable keybinding list grouped by Sway mode and LazyOS action.
- Show key chord, action, context/mode, and source.
- Do not infer bindings from defaults if the user's actual Sway config can be parsed.
- Copy binding/action and open source config in editor when available.

### 17. Session — `sessionctl`

- Actions: Lock, Logout, Restart compositor, Reboot, Shutdown.
- Safe action may execute directly only when policy permits; disruptive/destructive actions show a dedicated confirmation.
- Confirmation layout: action name, target, consequence, Cancel (default focus), Confirm.
- Require an explicit second activation; no countdown default.

## Visual state matrix

| State | Visual treatment | Required behavior |
|---|---|---|
| Ready | normal text and semantic status | actions available |
| Focused | accent marker + contrast | keyboard focus obvious |
| Disabled | muted + reason | cannot activate |
| Loading | progress text/indicator | event-driven; no spinner loop required |
| Empty | explanatory copy + next action | no blank surface |
| Warning | warning token + concise cause | action remains explicit |
| Error | danger token + retry/details | do not discard last good data |
| Confirming | distinct confirmation frame | Cancel initially focused |
| Running | progress + task label | safe cancel only |
| Completed | success state + result | dismiss/back |
| Cancelled | neutral result | preserve logs/result |

## Provisional sizing and typography

These are implementation starting points, not final pixel measurements from the missing reference:
- Bar height: 30–34 logical px.
- Panel width: palette 560–720 px; utility panels 320–520 px; details panel may reach 680 px.
- Content padding: 8–12 px.
- Row height: 24–30 px; touch targets should grow when touch input is detected.
- Border: 1 px semantic outline; focus accent 1–2 px.
- Typeface: JetBrains Mono with a monospace fallback; use a separate glyph font only if required.
- Avoid rounded cards, large shadows, decorative gradients, and continuous motion.
