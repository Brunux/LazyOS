# Spec 015 — The browser bar

- **Status**: draft
- **Optional**: **yes** — not on the critical path. The session works with Firefox's own
  toolbar; this spec exists because the maintainer asked for the idea to be written down and
  validated, and the decision to build it is theirs. Nothing depends on it.
- **Phase**: after the session framework (the bar itself moves in spec 013)
- **Depends on**: 001 (`lazyos`, the multi-call binary), 004 (the palette the row is drawn in).
  The row only needs *a* bar that can run a command: waybar today, the framework's bar after 013
- **Model**: `claude-sonnet-5-5` — a Marionette client, a bar row and a pinned profile;
  the loopback-only rule is a test, not a judgement call. Fallback `claude-opus-5-5`.
- **Branch**: `spec/015-browser-bar`
- **Plan**: not in `docs/plan-ui.md` §16 (the plan's fourteen specs are the port of the session
  as it is). This is a new idea, recorded here after the maintainer's request of 2026-10-23.

## Problem

The browser is the one window in the session that carries its own chrome *twice*: Firefox draws a
tab strip and a toolbar over a page, and waybar draws the session's bar over the same screen. On
this panel (1366x912, 4 GB) the tab strip and toolbar are the difference between reading a page
and reading 90 % of it, and nothing in them is something the session does not already draw
somewhere: the title is a window title, the URL is a line of text, and back/forward/reload are
three buttons.

The idea is therefore to let the page use the window, and to put those five things in the bar the
session already draws. It is Omarchy-shaped: the screen belongs to the content, the chrome is one
row, and there is one place to look.

## Scope

**In** (`lazyos browser` + one bar row + the profile that hides the chrome):

- **`lazyos browser url | title | go URL | back | forward | reload | focus | status`** — a client
  for **Marionette**, Firefox's own protocol (TCP 2828, protocol 3, length-prefixed JSON). One
  process per call and no daemon: reading the bar is a poll, and a click is one call.
- **The bar row** (`custom/browser` in waybar): the site's icon or the theme's globe, a truncated
  title (the URL on hover), `on-click` opens `bar-pick -free` for an address, `on-click-right`
  reloads, the scroll wheel walks history. It prints `{"class":"empty"}` and collapses when
  Firefox is not running, the way `custom/vnc` and `custom/updates` already do.
- **The launcher starts Firefox with `--marionette`**, bound to loopback, and the binary refuses
  to talk to anything but `127.0.0.1`.
- **A pinned profile** (`~/.config/mozilla/firefox/lazyos.default-release`, named by a shipped
  `profiles.ini`) and the `chrome/userChrome.css` in it that hides `#TabsToolbar` and `#nav-bar`,
  plus `/usr/lib/firefox-esr/distribution/policies.json` setting
  `toolkit.legacyUserProfileCustomizations.stylesheets=true`. Pinning is not optional: without a
  named profile, `userChrome.css` has no stable home and a browser reset loses the whole design.
- **A key** for the address: `Alt+L` focuses the row's entry (today `Ctrl+L` is the browser's, and
  it is hidden with the toolbar).

**Out**:

- **A second browser engine.** Measured in forky and rejected in §*Alternatives*: `cog` is 622 KB
  but brings the WPE WebKit engine (a different browser: no tabs, no extensions), `epiphany` is
  7 MB plus WebKitGTK 6, `qutebrowser` 5.5 MB plus Qt6 WebEngine, and none of them has an API
  that drives navigation except `cog`'s kiosk D-Bus. Firefox already ships, is already trusted
  with the user's logins, and answers a documented protocol.
- **Tabs in the bar.** One window, one page. The tab strip is hidden, and `Ctrl+T` still opens a
  tab (it shows Firefox's strip again only if the user asks for it).
- **Bookmarks, history, extensions in the bar.** The DSL of a bar row is a label and a click;
  reproducing the library pop-up is not what this buys.
- **The bar itself.** What draws the row (waybar now, `lazyos-ui` later) belongs to 013; this spec
  adds a command and one row, so the port costs nothing when the bar changes.

## Design

### The mechanism, measured

Marionette is the protocol Selenium drives Firefox with. On this card, headless, separate
profile, `firefox-esr 153.5.0esr`:

```
handshake: {'applicationType': 'gecko', 'marionetteProtocol': 3}
WebDriver:NewSession   -> {'sessionId': 'b3a1ab50-…'}
WebDriver:Navigate     -> example.com
WebDriver:GetCurrentURL -> 'https://example.com/'
WebDriver:GetTitle      -> 'Example Domain'
```

That is the whole feature's dependency, and it works. **Chrome DevTools' HTTP endpoints are not
there**: `127.0.0.1:9222/json/version` and `/json/new` answer `404` on this build, so the design
must not be a Chrome-shaped one. The wire format is `<length>:<json>`, the payload is
`[0, id, method, params]`, and the reply comes back as `[1, id, error, result]` - about 100 lines
of Rust with `TcpStream` and `serde_json`, both already in the tree.

### Why the bar calls a command and not a socket

The rule of this port is *nothing resident*. A row that held a socket open would be a daemon with
a nice label: it would need a reconnect policy, a lifecycle tied to Firefox's, and a way to die
with the session. Instead:

- **poll**: waybar runs `lazyos browser status` every two seconds; each run connects, asks
  `GetCurrentURL` + `GetTitle`, prints one line and exits;
- **click**: `bar-pick -free` for the address, then `lazyos browser go <url>`;
- **history**: `back`/`forward`/`reload` are one call each.

The cost is a TCP connect and two JSON round trips per tick, on loopback, and a process that
lives milliseconds. The spec's first measurement is that number, because a bar that costs 1 % of a
core to say "example.com" is not worth its row.

### Degrading, not lying

Three states, and each has an answer:

| state | the row shows | the commands do |
|---|---|---|
| Firefox not running | nothing (the row collapses) | exit 1, no output |
| Firefox running, Marionette silent (flag lost in an upgrade, port taken) | the window title from `swaymsg -t get_tree` and a `?` class | display works, navigation exits 3 with the reason |
| Marionette answers | title and URL | everything |

The middle state is the one that matters: a Firefox upgrade that drops `--marionette` must leave a
bar that still says what the page is, not a row that silently lies about the URL.

### The chrome that is hidden, and why it can come back

`userChrome.css` is unsupported by Mozilla and its selectors are Firefox's own. Hiding
`#TabsToolbar` and `#nav-bar` is the whole CSS; the mitigation for the day an ESR upgrade renames
one is structural: **navigation does not depend on it**. `lazyos browser go` works through
Marionette whatever the chrome looks like, so a broken stylesheet shows the toolbar again over a
page that still navigates, and `check-system.sh` says so.

The address has to move with it: hiding `#nav-bar` also hides the box `Ctrl+L` focuses, which is
why `go` exists and why the row's click is the way to type an address.

## Acceptance criteria

- [ ] `lazyos browser go`, `back`, `forward`, `reload` move the page, and `url`/`title`/`status`
      print what the browser is showing - observed against Firefox on this card, and against a
      fake server in `cargo test`.
- [ ] the bar row appears while Firefox runs, collapses when it does not, and a click on it opens
      the address entry - observed on the device.
- [ ] the page gains the tab strip and the toolbar's height, measured before and after, with the
      two screenshots in `reports/`.
- [ ] a `userChrome.css` that Firefox rejects leaves the toolbar visible **and** the row's
      navigation working - observed by pointing the profile at a deliberately broken stylesheet.
- [ ] the launcher's `--marionette` is loopback-only, and `lazyos browser` refuses a socket
      address that is not `127.0.0.1` - observed by connecting from another host (refused) and
      from the machine (answered).
- [ ] witness: `scripts/check-system.sh` FAILs when Firefox is running and Marionette does not
      answer `getCurrentURL` within one second (a dead row), when the generated row has lost its
      `on-click`, when `--marionette` is missing from the launcher, when `policies.json` is
      absent, and when the pinned profile has no `chrome/userChrome.css`; SKIPs when Firefox is
      not installed.
- [ ] witness: a paragraph in `uis/sway/files/usr/share/doc/lazyos/04-software.md` § The browser
      bar says what the row does, that the tab strip and toolbar are hidden on purpose, and how
      to get them back (`lazyos browser chrome on`, or deleting the file).
- [ ] witness: an `AGENTS.md` bullet records the protocol choice, the loopback-only port, the
      unsupported-but-recoverable `userChrome.css` and the three states of the row.
- [ ] unit tests: `cargo test -p lazyos-cli` - the framing (`<len>:<json>`), the reply parser, a
      timeout that becomes exit 3, and the whole conversation against a `TcpListener` that plays
      Firefox.

## Tests

- **Unit** (`cargo test`): the length-prefixed framing in both directions (a message split across
  reads, a message with a multibyte UTF-8 title), the `[1, id, error, result]` reply parser, the
  1 s timeout, and the refusal of a non-loopback address.
- **Integration**: a fake Marionette server in the test (`std::net::TcpListener` on `127.0.0.1:0`,
  the socket path injectable through `LAZYOS_MARIONETTE`), answering the handshake, `NewSession`,
  `Navigate`, `GetCurrentURL` and `GetTitle`; the command must complete each verb against it.
  CI has no Firefox, so this is the test that runs there.
- **Snapshot**: none - the row is a waybar module, not a frame the crate draws. Its `class` states
  are the contract, and `check-system.sh` reads them.
- **Hardware**: the before/after screenshots and the measurement of what the page gained; the
  broken-stylesheet case; a session restart (the row must come back with the browser, and cost
  nothing while it is closed).

## Alternatives, and why Firefox wins

Measured in forky on this machine (`apt-cache show`), because "simpler" has a number:

| option | installed size | engine | can the bar drive it? |
|---|---|---|---|
| **Firefox + Marionette** | 0 extra (280 MB already in the image) | Gecko | **yes** - `WebDriver:*`, measured above |
| `cog` (WPE kiosk shell) | 622 KB + `libwpewebkit-2.0-1` | WPE WebKit (a second engine) | yes, by D-Bus - but it is a different browser: no tabs, no extensions, no Firefox's codec story |
| `epiphany --application-mode` | 7 MB + `libwebkitgtk-6.0-4` + GTK4 | WebKitGTK | **no** remote API: the row could show a title and nothing else |
| `qutebrowser` | 5.5 MB + Qt6 WebEngine | Qt WebEngine (Chromium, a third engine) | partially; it also replaces the browser |

`cog` is the only one that is *architecturally* closer to the idea (a kiosk shell with no chrome
to hide). It is rejected on what it is, not on what it costs: it is a second browser with a
different login jar, a different rendering engine and no extensions, and this port's browser is
the one the user already trusts with their accounts.

## Risks

- **A Firefox ESR upgrade renames a chrome id** and the toolbar returns. Structural mitigation
  above (navigation is independent); `check-system.sh` reports the row, not the CSS, and the
  manual names the file.
- **Port 2828 is a full-control interface on loopback.** It is the same trust decision as
  `wayvnc`: any local process could drive the browser while it runs, so it binds `127.0.0.1`
  only, it exists only while Firefox runs, and both the manual and `AGENTS.md` say so. The
  alternative - a browser nobody can drive - is the toolbar we are trying to remove.
- **Hiding `#nav-bar` hides `Ctrl+L`.** The address moves to the row; a user who relies on
  `Ctrl+L` gets an invisible box. The manual says which key replaced it, and `chrome on` restores
  the toolbar in one command.
- **The pinned profile is a promise about a third-party directory layout.** If Firefox changes
  where it reads `profiles.ini`, the browser starts with a fresh profile and the user's logins
  look gone. `check-system.sh` FAILs when the pinned profile is not the running one.
- **The measurement may not pay.** If the browser bar costs more attention than the 75 px it
  gives back, the spec is dropped and the reason is written here - that is what `dropped` is for.
