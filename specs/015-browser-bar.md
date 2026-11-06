# Spec 015 — Browser optimization: vertical tabs and compact density

- **Status**: draft
- **Note**: The original prototype evaluated driving Firefox through Marionette (TCP port 2828)
  and hiding chrome with `userChrome.css`. That approach was **dropped** (§*Alternatives*, §*Risks*)
  because Marionette triggers an intrusive automated-testing banner, lacks multi-tab synchronization,
  exposes an unauthenticated loopback attack surface, and wastes battery polling, while hiding
  `#nav-bar` destroys extension popups, SSL certificate badges, and permission prompts. This spec
  replaces it with **Option 1**: native Firefox Enterprise Policies configuring vertical tabs and
  compact density with zero daemons and zero hacks.
- **Optional**: **no** — on the display optimization path for the Chromebook Hayato panel.
- **Phase**: after the session framework (the bar itself moves in spec 013)
- **Depends on**: 001 (`lazyos`, multi-call binary)
- **Model**: `claude-sonnet-5-5` — enterprise policies JSON, documentation, and check-system witness. Fallback `claude-opus-5-5`.
- **Branch**: `spec/015-browser-bar`

## Problem

On the Google Hayato Chromebook (MediaTek MT8192, 1366×912 resolution, 3:2 aspect ratio, 4 GB RAM),
vertical screen real estate is exceptionally scarce (912 px).

When Firefox runs in its stock desktop configuration, it stacks multiple horizontal bars on top of the
web viewport:
1. The session top bar (Waybar / LazyOS bar): ~30 px.
2. The Firefox tab strip (`#TabsToolbar`): ~38 px.
3. The Firefox navigation and URL toolbar (`#nav-bar`): ~40 px.

Over **108 px of vertical display height** is consumed by horizontal chrome before the user reads a
single line of web content.

On a 3:2 aspect ratio display (1366 px wide), **horizontal width is abundant while vertical height is
scarce**. Forcing tabs to sit horizontally above the navigation bar cuts deeply into document reading,
code review, and web browsing.

## Scope

**In** (Native Firefox Enterprise Policies + Sway integration):

- **Native Vertical Tabs** (`sidebar.revamp: true` and `sidebar.verticalTabs: true`):
  Tabs move into Firefox's native, collapsible vertical sidebar on the left. The entire top horizontal
  `#TabsToolbar` is completely eliminated from the top of the window, immediately reclaiming ~38 px of
  vertical height.
- **Compact Toolbar Density** (`browser.compactmode.show: true` and `browser.uidensity: 1`):
  Shrinks the remaining navigation and address bar down to a clean, compact ~30 px height.
- **Auto-Hide Fullscreen** (`browser.fullscreen.autohide: true`):
  Ensures `F11` (or Sway's `Mod+F` fullscreen toggle) gives 100% of the 1366×912 panel to the web page,
  with the navigation bar smoothly dropping down when the pointer touches the top edge.
- **Enterprise Policy Packaging**:
  Shipped as `/usr/lib/firefox-esr/distribution/policies.json` in the rootfs.
- **User Customization Intact**:
  Preferences are configured with `"Status": "default"` rather than `"locked"`. The user can adjust,
  revert, or customize sidebar tabs in `about:preferences` or `about:config` without system lockouts.
- **Zero Resident Processes**:
  Zero background daemons, zero TCP ports, zero network listeners, and zero polling loops.

**Out**:

- **Marionette protocol client and TCP 2828 listener** (dropped: security risk, battery waste, and
  intrusive test banner).
- **Hiding `#nav-bar` via `userChrome.css`** (dropped: destroys extensions like uBlock Origin and
  Bitwarden, hides SSL certificates, and blocks webcam/microphone permission dialogs).
- **Custom Waybar browser controls**:
  The browser provides its own mature, accessible address bar, URL autocomplete, search engine
  integration, and history. Duplicating these controls in Waybar is unnecessary and counterproductive.

## Design

### Screen geometry analysis (1366×912, 3:2)

Standard 16:9 widescreen displays have surplus horizontal space and low vertical height (e.g. 1366×768).
The Hayato Chromebook panel is 3:2 (1366×912), offering 144 extra vertical pixels over a standard 768p
panel, but every pixel counts when running side-by-side terminal windows or reading documentation.

Horizontal tab bars scale poorly as tab count increases: tab titles truncate into illegible ~20 px
stubs. Vertical tabs scale naturally down the full 912 px height of the panel, showing complete page
titles and favicons. Furthermore, the sidebar can be collapsed to show only icons when reading,
maximizing viewport width while keeping the top of the page free from redundant tab strips.

### Policy configuration (`policies.json`)

Debian's `firefox-esr` package reads `/usr/lib/firefox-esr/distribution/policies.json` on startup.
The shipped configuration defines default preferences:

```json
{
  "policies": {
    "DisableAppUpdate": true,
    "Preferences": {
      "browser.compactmode.show": {
        "Value": true,
        "Status": "default"
      },
      "browser.uidensity": {
        "Value": 1,
        "Status": "default"
      },
      "sidebar.revamp": {
        "Value": true,
        "Status": "default"
      },
      "sidebar.revamp.round-content-area": {
        "Value": true,
        "Status": "default"
      },
      "sidebar.verticalTabs": {
        "Value": true,
        "Status": "default"
      },
      "browser.fullscreen.autohide": {
        "Value": true,
        "Status": "default"
      }
    }
  }
}
```

Because `Status` is set to `default`, these settings apply automatically to new profiles and daily
usage while allowing the user to customize them freely.

## Acceptance criteria

- [ ] Firefox launches out of the box with vertical tabs active, eliminating the top horizontal tab strip.
- [ ] Compact toolbar density is active by default, reducing toolbar height to ~30 px.
- [ ] Browser extensions (uBlock Origin, password managers) and SSL site permission badges remain visible and fully functional in the toolbar.
- [ ] Preferences are set with `Status: "default"`, allowing user customization or rollback without admin overrides.
- [ ] Zero background daemons or TCP ports (such as port 2828) are started; idle CPU consumption remains at 0%.
- [ ] The policy file is cleanly deployed at `/usr/lib/firefox-esr/distribution/policies.json`.
- [ ] witness: `scripts/check-system.sh` verifies that `/usr/lib/firefox-esr/distribution/policies.json` exists, is valid JSON, specifies vertical tabs and compact density, and verifies that no Marionette port 2828 is listening; SKIPs when Firefox is not installed.
- [ ] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/04-software.md` § The browser explains the vertical tabs layout, compact density, and how to customize or collapse the sidebar.
- [ ] witness: an `AGENTS.md` bullet records why Marionette and `userChrome.css` were dropped in favor of native distribution policies, the zero-daemon rule, and the 3:2 screen geometry rationale.
- [ ] unit tests: `cargo test -p lazyos-cli` validates the JSON syntax and required keys of the policy configuration.

## Tests

- **Unit** (`cargo test`): JSON syntax validation test ensuring the policy template parses cleanly and contains the required vertical tabs and compact density preferences.
- **Integration** (`check-system.sh`): verifies `/usr/lib/firefox-esr/distribution/policies.json` on the target system, checks JSON schema validity, ensures `sidebar.verticalTabs` and `browser.uidensity` are configured, and verifies that no process is listening on `127.0.0.1:2828`.
- **Hardware**: visual verification on the Hayato Chromebook display confirming the top tab bar is gone, tabs appear in the collapsible vertical sidebar, and web pages gain ~38 px of vertical viewport space.

## Alternatives, and why native policies won

| Approach | Screen space gained | Maintenance & reliability | Extension & SSL support | Battery & CPU impact | Verdict |
|---|---|---|---|---|---|
| **Marionette TCP 2828 + `userChrome.css`** | ~75 px | **Fails**: breaks on ESR DOM changes; triggers "controlled by automated software" banner | **Fails**: `#nav-bar` removal destroys extensions and SSL badges | **Fails**: 1,800 TCP round-trips/hr poll CPU constantly | **DROPPED** (Spec 015 prototype) |
| **Native WebExtension + Native Messaging** | ~38 px | Medium: requires maintaining an out-of-tree extension | Supported | Good (event-driven) | Viable, but redundant when native policies exist |
| **Native Policies (`policies.json`)** | **~68 px** (vertical tabs + compact) | **Optimal**: officially supported Mozilla Enterprise API; 0 custom code | **Full support**: toolbar, extensions, and SSL dialogs intact | **Zero impact**: no daemons, no background polling | **SELECTED** (Spec 015) |

## Risks

- **Mozilla renames sidebar revamp preferences**: Firefox ESR maintains enterprise stability for extended periods. If Mozilla promotes vertical tabs from experimental/revamp to stable default settings in a future ESR release, the preferences can be updated via the policy file without requiring binary or shell script recompilation. `scripts/check-system.sh` catches missing keys.
- **User prefers horizontal tabs**: Because `Status` is `"default"` rather than `"locked"`, any user who prefers traditional horizontal tabs can simply right-click the sidebar and select horizontal tabs without restriction.
