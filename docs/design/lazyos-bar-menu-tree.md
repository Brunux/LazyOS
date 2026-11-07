# LazyOS Bar — Complete Menu Tree

**Status:** implementation-ready functional taxonomy; visual styling remains provisional until checked against the original concept image.

This document defines every menu route. It is the canonical source for menu IDs, parent/child relationships, capability gates, and actions. Labels may be localized; IDs must remain stable.

## Navigation contract

- The root palette is opened by clicking `z_` or the configured launcher shortcut.
- The root palette has a search field, category list, recent actions, and favorites.
- Selecting a category opens its child list in the same panel. Selecting a child either opens a deeper list or opens a dedicated panel.
- Breadcrumbs always show the current route. Backspace and Left Arrow return one level; Escape closes the panel (Escape from a confirmation returns to the action's parent, not directly to the desktop).
- Up/Down moves selection; Home/End moves to list boundaries; Right Arrow enters a submenu; Left Arrow goes back; Enter activates; printable characters filter the current list; Ctrl+K focuses search; Tab moves between search, list, and footer actions.
- Search is global from the root and scoped to the current list after entering a category. Results show category, description, and shortcut where available.
- Disabled items remain discoverable only when useful and explain why they are unavailable. Never show a clickable dead end.
- Long-running actions transition to a task view with progress, logs, result, and cancellation only when the backend can safely cancel.
- Destructive actions always pass through a confirmation view with a clear target and consequence.

## Root tree

```text
LAZYOS (root palette)
├── System
│   ├── Overview
│   ├── Resources
│   │   ├── CPU
│   │   ├── Memory & swap
│   │   ├── Storage & mounts
│   │   ├── Temperature & sensors
│   │   └── Processes
│   │       ├── Process details
│   │       ├── Show process in terminal
│   │       └── Terminate → confirm → result
│   ├── Devices
│   │   ├── Displays → Display panel
│   │   ├── Audio devices → Audio panel
│   │   ├── Input devices (read-only unless backend supports configuration)
│   │   └── Battery & power → Power panel
│   ├── Services
│   │   ├── Service list
│   │   ├── Service details → status / logs
│   │   └── Start / Stop / Restart → confirm when disruptive → task result
│   ├── Logs
│   │   ├── System journal (if available)
│   │   ├── LazyOS bar log
│   │   └── Service log → filter / follow / copy
│   ├── Updates → Updates panel
│   ├── Disks & mounts
│   │   ├── Filesystem usage
│   │   ├── Mount details
│   │   └── Open mount in file manager (if available)
│   ├── About
│   │   ├── LazyOS version
│   │   ├── Kernel / architecture
│   │   ├── Hardware
│   │   └── Licenses / build information
│   └── Power & session → Session menu
│
├── Connectivity
│   ├── Network → Network panel
│   │   ├── Wi-Fi
│   │   │   ├── Radio on/off
│   │   │   ├── Scan networks → scan results
│   │   │   │   └── Network details → Connect / Password prompt / Connecting / Result
│   │   │   ├── Saved networks
│   │   │   │   └── Profile details → Connect / Disconnect / Forget → confirm
│   │   │   └── Current connection → details / disconnect
│   │   ├── Ethernet → interfaces / connection details
│   │   ├── VPN
│   │   │   ├── Configured profiles
│   │   │   └── Connect / Disconnect / details
│   │   └── Diagnostics
│   │       ├── Interface and route
│   │       ├── DNS lookup
│   │       ├── Gateway reachability
│   │       └── Copy diagnostic report
│   ├── Bluetooth → Bluetooth panel
│   │   ├── Adapter on/off
│   │   ├── Scan → nearby devices
│   │   │   └── Device details → Pair → PIN/passkey confirmation → Connect
│   │   ├── Paired devices
│   │   │   └── Device details → Connect / Disconnect / Trust / Untrust / Remove → confirm
│   │   └── Adapter details
│   ├── Screen sharing / VNC → Sharing panel
│   │   ├── Current status and endpoint
│   │   ├── Start sharing → security confirmation → task result
│   │   ├── Stop sharing → confirmation if clients are connected
│   │   └── Access policy / read-only status
│   └── Network diagnostics → Diagnostics panel
│
├── Personalization
│   ├── Appearance
│   │   ├── Themes → Theme selector
│   │   ├── Theme preview → Apply / Cancel
│   │   ├── Bar density → Comfortable / Compact
│   │   ├── Bar position → Top (other positions only if implemented)
│   │   └── Transparency / blur (only if supported)
│   ├── Bar layout → Bar settings
│   │   ├── Left / center / right modules
│   │   ├── Module visibility
│   │   ├── Overflow and compact labels
│   │   ├── Output selection
│   │   └── Reset to defaults → confirm
│   ├── Workspaces
│   │   ├── Workspace names and ordering
│   │   └── Workspace overview → move window
│   ├── Keyboard
│   │   ├── Current layout
│   │   ├── Available layouts → set active
│   │   └── Layout indicator visibility
│   ├── Display → Display panel
│   │   ├── Brightness
│   │   ├── Connected displays
│   │   ├── Scale / resolution (only when safely supported)
│   │   └── Night light / gamma schedule (only when backend exists)
│   ├── Sound → Audio panel
│   ├── Notifications
│   │   ├── Notification history → Notifications panel
│   │   └── Notification settings (Mako configuration link/help; daemon remains Mako)
│   └── Keep awake → Idle inhibitor panel
│
├── Applications
│   ├── Application launcher
│   ├── Recent applications
│   ├── Favorites
│   ├── Default applications (read-only until a supported setter exists)
│   └── Run command → command entry → execute / cancel
│
├── Files & tools
│   ├── Search files
│   ├── File manager
│   ├── Terminal
│   ├── Clipboard history (conditional on provider)
│   ├── Help
│   │   ├── LazyOS help
│   │   ├── Bar help
│   │   └── Keybindings → searchable reference
│   └── Diagnostics
│       ├── Collect system report
│       ├── Copy report
│       └── Open report in terminal/editor
│
├── Session
│   ├── Lock
│   ├── Logout → confirmation
│   ├── Restart Sway/SwayFX → confirmation
│   ├── Reboot → confirmation
│   └── Shutdown → confirmation
│
└── Developer (conditional)
    ├── LazyOS status
    ├── Package manager
    │   ├── Check updates
    │   ├── Installed package lookup
    │   └── Package operation → task view
    ├── Build / deployment tools (only if installed)
    ├── GPU status
    ├── Logs & traces
    └── Collect debug bundle
```

## Indicator-specific menus

These are compact context menus attached to the bar, not duplicate copies of the root tree.

| Indicator | First-level menu | Nested routes |
|---|---|---|
| Workspace | Workspace 1…N; overview | Window list → Focus / Move to workspace |
| Sway mode | Current mode; mode description | Keybindings for this mode |
| Clock | Calendar month | Date details; previous/next month; Today |
| Busy | Current task and progress | Task details → log tail → cancel confirmation (if cancellable) |
| Updates | Pending count; Check now | Package list → package details → install task |
| Tray | App-provided menu | Preserve StatusNotifierItem menu hierarchy and activation semantics |
| Keyboard | Active layout | Available layouts → select |
| Bluetooth | Radio state; adapter | Paired / nearby → device detail → actions |
| VNC | Sharing state | Start/stop → security confirmation → endpoint/details |
| Network | Connection state | Wi-Fi/Ethernet/VPN → profile/device detail → actions |
| Audio | Mute; volume | Output/input device list → select; per-device details |
| Brightness | Current brightness | Slider; display list; night-light controls if available |
| Idle inhibitor | Active/inactive | Toggle; optional duration presets |
| Battery | Charge and state | Power profile; battery details |

## Menu item data contract

Each menu entry should be represented as typed data, not a hand-built widget:

- `id`: stable identifier (e.g. `connectivity.network.wifi.scan`)
- `label`: visible localized label
- `description`: one-line explanation
- `icon`: semantic icon key (optional)
- `parent_id`: parent route
- `kind`: category, action, toggle, value, navigation, separator, or status
- `availability`: available, unavailable(reason), or hidden
- `shortcut`: optional key hint
- `danger_level`: safe, disruptive, destructive
- `action_id`: backend command/event identifier (never shell text from a label)
- `children`: child IDs or lazy provider
- `state`: current checked/value/progress state where relevant

Never execute arbitrary shell strings stored in configuration. Actions must map to allow-listed backend operations with typed arguments.

## Route IDs

Use dotted IDs with stable nouns and verbs: `system.resources.memory`, `connectivity.network.wifi.scan`, `personalization.appearance.theme.apply`, `session.shutdown`. Do not derive IDs from translated labels.

## Completion checklist

- [ ] Every visible root item has a real route or an explicit unavailable reason.
- [ ] Every route has a parent, stable ID, and back path.
- [ ] Every destructive route has a confirmation state.
- [ ] Async actions have task IDs and a terminal result state.
- [ ] Search finds actions in nested routes and displays their breadcrumb.
- [ ] Context menus and root palette route to the same underlying action IDs.
- [ ] Keyboard-only navigation can reach every action and every confirmation.
