//! The card: one vocabulary for a notification and for a message in a window.
//!
//! A card is what the session says when something finished, failed or changed.
//! It is built once, with the same fields, and it does two jobs:
//!
//! - [`Card::send`] posts it to the notification daemon (mako stays until spec
//!   013) as the `Notify` call `notify-send` makes, with the same hints the
//!   shell menus use: the synchronous tag that makes one card replace the
//!   previous one (the OSD and the busy card), the `value` hint that mako draws
//!   as a bar, and the urgency that mako draws as the border colour;
//! - [`Card::render`] draws it inside a window as a framed panel with the same
//!   urgency colour, the same glyph and the same truncation.
//!
//! The rules the tests hold:
//!
//! - **The picture, in order.** A glyph from the alphabet (spec 003) is drawn
//!   as its codepoint in a window. A notification cannot draw a codepoint, so
//!   it asks the theme for the PNG of the same concept ([`theme_name`]). Only
//!   when neither exists is the card text, and [`Sent::fallback`] names what
//!   was missing: a picture is never dropped in silence (the rule that made
//!   `notify-send -i folder` look correct while the card stayed blank).
//! - **`Kind` decides the icon** when a call site asks for `auto`, so a new
//!   call site cannot forget it.
//! - **A secret never reaches a card's text.** [`Card::secret`] copies the
//!   value and names only the label, so mako's history cannot keep it.
//! - **Text is text.** mako reads summary and body as markup (`markup=1`), so
//!   `&`, `<` and `>` are escaped on the way out; a window draws them as they
//!   are.
//!
//! Transport decision (spec 006, measured in `docs/optimization.md` §14): the
//! card goes out through `notify-send`, the program every shell card already
//! uses, with an argument vector and no shell. A bus client in the binary
//! (`zbus`) was built and measured first - 42 crates, +2.6 MB of release binary,
//! about 1m40s more on a clean build - and was not adopted for a call the image
//! already makes. `notify-rust` would be the same bus client behind a second
//! feature matrix, so it was not considered further.

mod send;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_width::UnicodeWidthStr;

use crate::design::{ColorRole, Design, TextStyle};
use crate::glyphs::Glyph;
use crate::widgets::{draw_card_with, draw_progress, put, put_trunc};

/// How loud a card is. mako's three levels, and the three roles a window draws
/// them in: low is the dim grey, normal the accent, critical the danger red -
/// the same colours `~/.config/mako/config` gives the borders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "low" => Some(Self::Low),
            "normal" => Some(Self::Normal),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }

    /// The word `notify-send -u` takes.
    pub fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::Critical => "critical",
        }
    }

    /// The `urgency` hint: 0, 1 or 2, which is how `notify-send -u` sends it.
    pub fn level(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Normal => 1,
            Self::Critical => 2,
        }
    }

    pub fn role(self) -> ColorRole {
        match self {
            Self::Low => ColorRole::Dim,
            Self::Normal => ColorRole::Accent,
            Self::Critical => ColorRole::Danger,
        }
    }

    fn title_style(self) -> TextStyle {
        match self {
            Self::Low => TextStyle::Dim,
            Self::Normal => TextStyle::Title,
            Self::Critical => TextStyle::Danger,
        }
    }
}

/// What a card is about, for `Icon::Name` when a call site asks for `auto`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind<'a> {
    Info,
    Error,
    Url,
    Path { dir: bool, name: &'a str },
}

/// The picture a card asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Text only, on purpose (`--icon none`).
    None,
    /// A name from the alphabet of spec 003.
    Glyph(Glyph),
    /// A freedesktop name the LazyOS theme draws (or that a window maps to a
    /// glyph, see [`window_glyph`]).
    Name(String),
}

/// How long a notification stays up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timeout {
    /// mako's own default for the urgency.
    Default,
    /// Until it is dismissed.
    Never,
    Millis(u32),
}

/// One card. Its text is built when it is made, never per frame: rendering
/// copies cells (`tests/no_alloc.rs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    title: String,
    body: String,
    urgency: Urgency,
    icon: Icon,
    progress: Option<u8>,
    progress_label: String,
    group: Option<String>,
    timeout: Timeout,
}

impl Card {
    /// A normal card with the information icon: a card is never picture-less
    /// by accident, and `no_icon` is the way to ask for text.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: String::new(),
            urgency: Urgency::Normal,
            icon: Icon::Name(kind_name(Kind::Info).to_string()),
            progress: None,
            progress_label: String::new(),
            group: None,
            timeout: Timeout::Default,
        }
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = icon;
        self
    }

    /// The icon the result is about (see [`kind_of`]).
    pub fn kind(mut self, kind: Kind<'_>) -> Self {
        self.icon = Icon::Name(kind_name(kind).to_string());
        self
    }

    pub fn no_icon(mut self) -> Self {
        self.icon = Icon::None;
        self
    }

    /// The bar under the text, 0 to 100 (anything above is 100).
    pub fn progress(mut self, percent: u8) -> Self {
        let percent = percent.min(100);
        self.progress = Some(percent);
        self.progress_label = format!("{percent}%");
        self
    }

    /// The group tag. A card with the same tag replaces the one before it
    /// instead of stacking, which is how the OSD moves one card while a key is
    /// held and how `busy` turns a spinner into its result.
    pub fn group(mut self, tag: impl Into<String>) -> Self {
        self.group = Some(tag.into());
        self
    }

    pub fn timeout(mut self, timeout: Timeout) -> Self {
        self.timeout = timeout;
        self
    }

    /// A value the person needs to paste, copied and named but never shown: the
    /// card says only that it is on the clipboard, so the value cannot reach
    /// mako's history. `name` is a label ("password"), never the value.
    pub fn secret(name: &str, value: &str) -> Self {
        Self::secret_with(name, value, copy_to_clipboard)
    }

    /// [`Card::secret`] with the copy injected, so a test can see what would
    /// have been copied without a clipboard.
    pub fn secret_with(name: &str, value: &str, copy: impl FnOnce(&str) -> io::Result<()>) -> Self {
        match copy(value) {
            Ok(()) => Self::new(format!("Copied {name}")).body(format!(
                "{name} is on the clipboard. Its value is not shown."
            )),
            Err(_) => Self::new("Clipboard")
                .body(format!("could not copy {name}: wl-copy did not take it"))
                .urgency(Urgency::Critical)
                .kind(Kind::Error),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn body_text(&self) -> &str {
        &self.body
    }

    pub fn urgency_level(&self) -> Urgency {
        self.urgency
    }

    /// What the daemon will be asked, decided without touching the bus.
    /// `roots` is where the theme's pictures are (see [`default_roots`]).
    pub fn prepare(&self, roots: &[PathBuf]) -> Prepared {
        let (app_icon, fallback) = match toast_picture(&self.icon, roots) {
            Ok(Some(name)) => (name, None),
            Ok(None) => (String::new(), None),
            Err(reason) => (String::new(), Some(reason)),
        };
        Prepared {
            message: Message {
                app_icon,
                summary: escape_markup(&self.title),
                body: escape_markup(&self.body),
                urgency: self.urgency,
                expire_ms: match self.timeout {
                    Timeout::Default => -1,
                    Timeout::Never => 0,
                    Timeout::Millis(ms) => i32::try_from(ms).unwrap_or(i32::MAX),
                },
                group: self.group.as_ref().map(|tag| format!("lazyos-{tag}")),
                value: self.progress,
            },
            fallback,
        }
    }

    /// Post the card to the notification daemon (through `notify-send`). A refusal
    /// still says whether the picture was missing: that is a reason to fix the
    /// call, and it is not lost because the bus is down.
    pub fn send(&self) -> Result<Sent, Refused> {
        let prepared = self.prepare(&default_roots());
        match send::notify(&prepared.message) {
            Ok(id) => Ok(Sent {
                id,
                fallback: prepared.fallback,
            }),
            Err(reason) => Err(Refused {
                reason,
                fallback: prepared.fallback,
            }),
        }
    }

    /// Draw the card inside `area` (which it fills): the frame in the urgency's
    /// role, the title on the top border, the glyph and the body in the cells
    /// inside it. A body line that does not fit is cut with an ellipsis; the
    /// window never grows to hold it.
    pub fn render(&self, area: Rect, buf: &mut Buffer, design: &Design) {
        let role = self.urgency.role();
        let inner = draw_card_with(
            buf,
            area,
            design,
            Some(self.title.as_str()),
            role,
            design.style(self.urgency.title_style()),
        );
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let mut text_x = inner.x;
        if let Some(glyph) = window_glyph(&self.icon) {
            let cell = glyph.cell();
            let width = UnicodeWidthStr::width(cell) as u16;
            let style = Style::default()
                .fg(design.color(role))
                .bg(design.color(ColorRole::Bg));
            put(buf, inner.x, inner.y, cell, width, style);
            text_x = inner.x.saturating_add(width).saturating_add(1);
        }
        let text_width = inner.right().saturating_sub(text_x);

        let progress_row = self.progress.map(|_| inner.bottom() - 1);
        let body_end = progress_row.unwrap_or(inner.bottom());
        let body_style = design.style(TextStyle::Body);
        for (y, line) in (inner.y..body_end).zip(self.body.lines()) {
            put_trunc(buf, text_x, y, line, text_width, body_style);
        }

        if let (Some(value), Some(row)) = (self.progress, progress_row) {
            let bar = Rect {
                x: text_x,
                y: row,
                width: text_width,
                height: 1,
            };
            draw_progress(
                buf,
                bar,
                design,
                f64::from(value) / 100.0,
                &self.progress_label,
                role,
            );
        }
    }
}

/// What [`Card::send`] reports back.
#[derive(Debug, PartialEq, Eq)]
pub struct Sent {
    /// The id mako assigned.
    pub id: u32,
    /// Set when a picture was asked for and not found: the card went out as
    /// text, and this names what was missing.
    pub fallback: Option<String>,
}

/// A card the daemon did not take, with the same picture report as [`Sent`].
#[derive(Debug, PartialEq, Eq)]
pub struct Refused {
    pub reason: String,
    pub fallback: Option<String>,
}

/// The `Notify` call, decided: every field the daemon is given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    /// -1 is the daemon's default, 0 is never.
    pub expire_ms: i32,
    /// The `x-canonical-private-synchronous` hint, already prefixed `lazyos-`.
    pub group: Option<String>,
    /// The `value` hint, 0 to 100.
    pub value: Option<u8>,
}

/// A [`Message`] and the reason its picture, if one was asked for, is missing.
#[derive(Debug, PartialEq, Eq)]
pub struct Prepared {
    pub message: Message,
    pub fallback: Option<String>,
}

/// The app name every card carries, as the shell's `notify-send -a LazyOS`.
pub const APP_NAME: &str = "LazyOS";

/// The hint key mako reads a group tag from.
pub const GROUP_HINT: &str = "x-canonical-private-synchronous";

/// Where the theme's pictures are, in the order mako searches them. The first
/// root is the one `~/.config/mako/config` names in `icon-path`.
pub const THEME_ROOTS: &[&str] = &["/usr/share/icons/LazyOS", "/usr/share/icons/hicolor"];

/// The roots a notification checks for its picture: the installed theme, or
/// `LAZYOS_THEME_ROOTS` (colon-separated) when a test or a checkout names
/// another one. The override is read here and nowhere else.
pub fn default_roots() -> Vec<PathBuf> {
    match std::env::var_os("LAZYOS_THEME_ROOTS") {
        Some(list) if !list.is_empty() => std::env::split_paths(&list).collect(),
        _ => THEME_ROOTS.iter().map(PathBuf::from).collect(),
    }
}

/// The glyph a window draws for each picture, and the theme name a
/// notification draws for the same glyph. A pair is one concept: the file
/// browser's folder, the screenshot's image. Each name is a PNG the theme
/// carries (`scripts/make-icons.sh`), and a test holds that.
const PAIRS: &[(Glyph, &str)] = &[
    (Glyph::Folder, "folder"),
    (Glyph::File, "text-x-generic"),
    (Glyph::Document, "x-office-document"),
    (Glyph::Pdf, "application-pdf"),
    (Glyph::Html, "text-html"),
    (Glyph::Archive, "package-x-generic"),
    (Glyph::Executable, "application-x-executable"),
    (Glyph::Image, "image-x-generic"),
    (Glyph::Video, "video-x-generic"),
    (Glyph::Music, "audio-x-generic"),
    (Glyph::Terminal, "utilities-terminal"),
    (Glyph::Globe, "globe"),
    (Glyph::Info, "dialog-information"),
    (Glyph::Warning, "dialog-warning"),
    (Glyph::Warning, "dialog-error"),
    (Glyph::Volume, "audio-volume-high"),
    (Glyph::Brightness, "display-brightness"),
    (Glyph::Bell, "preferences-system-notifications"),
    (Glyph::Trash, "user-trash"),
    (Glyph::Clipboard, "clipboard-history"),
    (Glyph::Smile, "emoji-picker"),
    (Glyph::Search, "system-search"),
    (Glyph::Server, "network-server"),
    (Glyph::Wifi, "network-wireless"),
    (Glyph::Bluetooth, "bluetooth"),
    (Glyph::Keyboard, "input-keyboard"),
    (Glyph::Mouse, "input-touchpad"),
    (Glyph::Lock, "system-lock-screen"),
    (Glyph::Download, "folder-download"),
    (Glyph::Camera, "camera-web"),
    (Glyph::Capture, "applets-screenshooter"),
    (Glyph::Mic, "audio-input-microphone"),
    (Glyph::Clock, "preferences-system-time"),
    (Glyph::Power, "system-shutdown"),
    (Glyph::Gear, "preferences-system"),
    (Glyph::Window, "preferences-system-windows"),
    (Glyph::Calculator, "accessories-calculator"),
    (Glyph::Book, "help-browser"),
    (Glyph::Disk, "drive-harddisk"),
];

/// The names [`kind_of`] can produce, which are the ones a card asks for by
/// default.
const KIND_NAMES: &[&str] = &[
    "dialog-information",
    "dialog-error",
    "globe",
    "folder",
    "image-x-generic",
    "video-x-generic",
    "audio-x-generic",
    "application-pdf",
    "package-x-generic",
    "text-html",
    "x-office-document",
    "text-x-generic",
];

/// Every theme name the binary can ask for. `lazyos --icon-names` prints this
/// and `check-system.sh` asks the installed theme for each one.
pub fn names() -> impl Iterator<Item = &'static str> {
    PAIRS
        .iter()
        .map(|(_, name)| *name)
        .chain(KIND_NAMES.iter().copied())
}

/// The theme name a glyph is drawn with in a notification.
pub fn theme_name(glyph: Glyph) -> Option<&'static str> {
    PAIRS
        .iter()
        .find(|(g, _)| *g == glyph)
        .map(|(_, name)| *name)
}

/// The glyph a theme name is drawn with in a window.
pub fn glyph_for(name: &str) -> Option<Glyph> {
    PAIRS
        .iter()
        .find(|(_, n)| *n == name)
        .map(|(glyph, _)| *glyph)
}

/// The glyph a window draws for an icon, or `None` for text.
pub fn window_glyph(icon: &Icon) -> Option<Glyph> {
    match icon {
        Icon::None => None,
        Icon::Glyph(glyph) => Some(*glyph),
        Icon::Name(name) => glyph_for(name),
    }
}

/// The theme name for a kind of result.
pub fn kind_name(kind: Kind<'_>) -> &'static str {
    match kind {
        Kind::Info => "dialog-information",
        Kind::Error => "dialog-error",
        Kind::Url => "globe",
        Kind::Path { dir: true, .. } => "folder",
        Kind::Path { name, .. } => file_name_icon(name),
    }
}

/// What a text argument is, for `auto`: a URL, a path (`is_dir` is `Some` when
/// it exists), or information; a critical card is an error whatever its text.
pub fn kind_of(text: &str, urgency: Urgency, path: Option<bool>) -> Kind<'_> {
    if urgency == Urgency::Critical {
        return Kind::Error;
    }
    if text.starts_with("http://") || text.starts_with("https://") {
        return Kind::Url;
    }
    if let Some(dir) = path {
        let name = Path::new(text)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(text);
        return Kind::Path { dir, name };
    }
    Kind::Info
}

fn file_name_icon(name: &str) -> &'static str {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "avif" | "heic" | "tif"
        | "tiff" => "image-x-generic",
        "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" => "video-x-generic",
        "mp3" | "flac" | "ogg" | "opus" | "wav" | "m4a" => "audio-x-generic",
        "pdf" => "application-pdf",
        "tar" | "gz" | "tgz" | "xz" | "zst" | "zip" | "7z" | "bz2" | "deb" => "package-x-generic",
        "html" | "htm" => "text-html",
        "odt" | "ods" | "odp" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "rtf"
        | "epub" => "x-office-document",
        _ => "text-x-generic",
    }
}

/// The picture a notification draws, or why it is text.
///
/// `Ok(None)` is a card that asked for no picture; `Err` names the missing one.
/// A glyph is looked up through its theme name first, then the theme is asked
/// for a name directly - the same order the window's glyph column follows.
pub fn toast_picture(icon: &Icon, roots: &[PathBuf]) -> Result<Option<String>, String> {
    match icon {
        Icon::None => Ok(None),
        Icon::Glyph(glyph) => {
            let Some(name) = theme_name(*glyph) else {
                return Err(format!(
                    "glyph '{}' has no picture in the LazyOS theme",
                    glyph.name()
                ));
            };
            if drawn(roots, name) {
                Ok(Some(name.to_string()))
            } else {
                Err(format!("the LazyOS theme has no {name}.png"))
            }
        }
        Icon::Name(name) => {
            if drawn(roots, name) {
                Ok(Some(name.clone()))
            } else {
                Err(format!("the LazyOS theme has no {name}.png"))
            }
        }
    }
}

fn drawn(roots: &[PathBuf], name: &str) -> bool {
    roots.iter().any(|root| {
        root.join("48x48/apps")
            .join(format!("{name}.png"))
            .is_file()
    })
}

/// mako reads summary and body as markup, so the three characters that start a
/// tag or an entity are escaped; a title with an `&` is text, not an error.
fn escape_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// The clipboard, the way the shell's `ui_copy` puts text on it: on stdin, so
/// the value is never on a command line.
/// Put `value` on the Wayland clipboard through `wl-copy`, which reads it on
/// stdin - never on the command line, where another local process could read
/// it from `/proc`. The secret helper uses it, and so does any surface with a
/// copy action (spec 007: the Ask answer).
pub fn copy_to_clipboard(value: &str) -> io::Result<()> {
    let mut child = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(value.as_bytes())?;
    }
    if child.wait()?.success() {
        Ok(())
    } else {
        Err(io::Error::other("wl-copy failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::Density;
    use ratatui::layout::Rect;

    fn repo_theme() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../uis/sway/files/usr/share/icons/LazyOS")
    }

    fn repo_roots() -> Vec<PathBuf> {
        vec![repo_theme()]
    }

    /// A theme root with exactly the named pictures, in a scratch directory.
    fn scratch_root(label: &str, names: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("lazyos-card-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let apps = root.join("48x48/apps");
        std::fs::create_dir_all(&apps).unwrap();
        for name in names {
            std::fs::write(apps.join(format!("{name}.png")), b"").unwrap();
        }
        root
    }

    fn render_to_text(card: &Card, width: u16, height: u16) -> String {
        let design = Design::solarized().with_reduced_motion(true);
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        card.render(area, &mut buf, &design);
        crate::screen(&buf)
    }

    #[test]
    fn icon_resolution_prefers_glyph() {
        // The glyph's own picture is what a notification draws, even though the
        // glyph's name ("warning") is not a file the theme carries.
        let roots = repo_roots();
        let icon = Icon::Glyph(Glyph::Warning);
        assert_eq!(
            toast_picture(&icon, &roots),
            Ok(Some("dialog-warning".to_string()))
        );
        assert!(!drawn(&roots, "warning"));
        // And a window draws the glyph itself, not a text stand-in.
        assert_eq!(window_glyph(&icon), Some(Glyph::Warning));
        // A glyph with no theme name answers why it is text.
        let no_picture = Icon::Glyph(Glyph::Shield);
        assert!(toast_picture(&no_picture, &roots).is_err());
        assert_eq!(window_glyph(&no_picture), Some(Glyph::Shield));
        // The shell's own call: the image icon the spec's example asks for.
        assert_eq!(
            toast_picture(&Icon::Glyph(Glyph::Image), &roots),
            Ok(Some("image-x-generic".to_string()))
        );
    }

    #[test]
    fn missing_icon_is_text_and_logs() {
        let root = scratch_root("missing", &["folder"]);
        let roots = vec![root.clone()];
        let card = Card::new("Saved").icon(Icon::Name("no-such-icon".into()));
        let prepared = card.prepare(&roots);
        assert_eq!(prepared.message.app_icon, "");
        let reason = prepared.fallback.expect("a missing picture is reported");
        assert!(reason.contains("no-such-icon"), "{reason}");

        // A card that asked for no picture asked for nothing to report.
        let plain = Card::new("Saved").no_icon().prepare(&roots);
        assert_eq!(plain.message.app_icon, "");
        assert_eq!(plain.fallback, None);

        // A picture that is there is sent by name, with nothing to report.
        let found = Card::new("Folder")
            .icon(Icon::Name("folder".into()))
            .prepare(&roots);
        assert_eq!(found.message.app_icon, "folder");
        assert_eq!(found.fallback, None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn secret_never_in_body() {
        let value = "hunter2-LAZY-7f3a";
        let mut copied = String::new();
        let card = Card::secret_with("password", value, |text| {
            copied = text.to_string();
            Ok(())
        });
        assert_eq!(copied, value, "the value goes to the clipboard");
        assert!(!card.title().contains(value));
        assert!(!card.body_text().contains(value));
        assert!(card.body_text().contains("password"), "it names the label");
        assert!(!format!("{card:?}").contains(value), "nothing keeps it");

        let prepared = card.prepare(&repo_roots());
        assert!(!format!("{:?}", prepared.message).contains(value));
        assert!(!render_to_text(&card, 60, 6).contains(value));

        // A clipboard that refuses must not leak the value into the error card
        // either.
        let failed = Card::secret_with("password", value, |_| Err(io::Error::other("no")));
        assert_eq!(failed.urgency_level(), Urgency::Critical);
        assert!(!format!("{failed:?}").contains(value));
    }

    #[test]
    fn body_truncates_to_width() {
        let long = "word ".repeat(60);
        let card = Card::new("Long").body(long).no_icon();
        let text = render_to_text(&card, 40, 6);
        let body_row = text.lines().nth(1).expect("the first body row");
        assert_eq!(body_row.chars().count(), 40, "the window keeps its width");
        assert!(
            body_row.contains('\u{2026}'),
            "cut with an ellipsis: {body_row:?}"
        );
        assert!(body_row.ends_with("\u{2502}"), "the frame is still there");
        assert_eq!(text.lines().count(), 6, "the card never grows the window");
    }

    #[test]
    fn tag_is_stable() {
        let roots = repo_roots();
        let first = Card::new("Volume").group("osd").prepare(&roots).message;
        let second = Card::new("Volume").group("osd").prepare(&roots).message;
        assert_eq!(first.group.as_deref(), Some("lazyos-osd"));
        assert_eq!(first, second, "the same card carries the same tag");
        assert_eq!(GROUP_HINT, "x-canonical-private-synchronous");
        assert_eq!(Card::new("x").prepare(&roots).message.group, None);
    }

    #[test]
    fn urgency_maps_to_mako_and_to_a_role() {
        assert_eq!(Urgency::Low.level(), 0);
        assert_eq!(Urgency::Normal.level(), 1);
        assert_eq!(Urgency::Critical.level(), 2);
        assert_eq!(Urgency::Low.role(), ColorRole::Dim);
        assert_eq!(Urgency::Normal.role(), ColorRole::Accent);
        assert_eq!(Urgency::Critical.role(), ColorRole::Danger);
        assert_eq!(Urgency::parse("critical"), Some(Urgency::Critical));
        assert_eq!(Urgency::parse("loud"), None);
    }

    #[test]
    fn a_critical_card_is_drawn_in_the_danger_role() {
        let design = Design::solarized();
        let area = Rect::new(0, 0, 30, 4);
        let mut buf = Buffer::empty(area);
        Card::new("Failed")
            .urgency(Urgency::Critical)
            .no_icon()
            .render(area, &mut buf, &design);
        assert_eq!(
            buf.cell((0, 0)).unwrap().fg,
            design.color(ColorRole::Danger),
            "the frame is the urgency's colour"
        );
    }

    #[test]
    fn progress_is_clamped_and_carried_to_the_hint() {
        let roots = repo_roots();
        let over = Card::new("Volume").progress(150).prepare(&roots).message;
        assert_eq!(over.value, Some(100));
        let mid = Card::new("Volume").progress(40).prepare(&roots).message;
        assert_eq!(mid.value, Some(40));
        assert_eq!(Card::new("x").prepare(&roots).message.value, None);
    }

    #[test]
    fn timeouts_are_makos_numbers() {
        let roots = repo_roots();
        let expire = |t: Timeout| Card::new("x").timeout(t).prepare(&roots).message.expire_ms;
        assert_eq!(expire(Timeout::Default), -1);
        assert_eq!(expire(Timeout::Never), 0);
        assert_eq!(expire(Timeout::Millis(1200)), 1200);
    }

    #[test]
    fn mako_reads_the_text_as_markup_so_it_is_escaped() {
        let message = Card::new("a & <b>")
            .body("x > y")
            .prepare(&repo_roots())
            .message;
        assert_eq!(message.summary, "a &amp; &lt;b&gt;");
        assert_eq!(message.body, "x &gt; y");
    }

    #[test]
    fn kind_decides_the_icon_when_a_call_site_asks_for_auto() {
        assert_eq!(
            kind_of("https://example.org", Urgency::Normal, None),
            Kind::Url
        );
        assert_eq!(kind_of("notes", Urgency::Critical, None), Kind::Error);
        assert_eq!(kind_of("notes", Urgency::Normal, None), Kind::Info);
        let file = kind_of("/home/u/shot.PNG", Urgency::Normal, Some(false));
        assert_eq!(kind_name(file), "image-x-generic");
        assert_eq!(
            kind_name(kind_of("/home/u/Pictures", Urgency::Normal, Some(true))),
            "folder"
        );
        assert_eq!(kind_name(Kind::Info), "dialog-information");
        assert_eq!(kind_name(Kind::Url), "globe");
        assert_eq!(
            kind_name(Kind::Path {
                dir: false,
                name: "notes.md"
            }),
            "text-x-generic"
        );
        // A new call site that forgets the icon still gets one.
        assert_eq!(Card::new("x").icon, Icon::Name("dialog-information".into()));
    }

    #[test]
    fn every_name_the_binary_asks_for_is_drawn_by_the_theme() {
        let theme = repo_theme();
        let mut missing = Vec::new();
        for name in names() {
            if !theme
                .join("48x48/apps")
                .join(format!("{name}.png"))
                .is_file()
            {
                missing.push(name);
            }
        }
        assert!(missing.is_empty(), "the theme has no PNG for: {missing:?}");
    }

    #[test]
    fn mako_draws_from_the_theme_root_this_binary_names() {
        let config = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../uis/sway/files/home/.config/mako/config");
        let text = std::fs::read_to_string(config).unwrap();
        assert!(
            text.lines().any(|line| line
                .strip_prefix("icon-path=")
                .is_some_and(|paths| { paths.split(':').any(|root| root == THEME_ROOTS[0]) })),
            "mako's icon-path must name {}",
            THEME_ROOTS[0]
        );
    }

    #[test]
    fn the_message_carries_the_same_text_the_window_draws() {
        let card = Card::new("Saved")
            .body("~/Pictures/shot.png")
            .kind(Kind::Path {
                dir: false,
                name: "shot.png",
            })
            .timeout(Timeout::Never);
        let message = card.prepare(&repo_roots()).message;
        assert_eq!(message.app_icon, "image-x-generic");
        assert_eq!(message.summary, "Saved");
        assert_eq!(message.body, "~/Pictures/shot.png");
        assert_eq!(message.expire_ms, 0);
        assert_eq!(APP_NAME, "LazyOS");
        let text = render_to_text(&card, 40, 5);
        assert!(text.contains("Saved") && text.contains("~/Pictures/shot.png"));
        assert!(text.contains(glyph_for("image-x-generic").unwrap().cell()));
    }

    #[test]
    fn a_card_fits_the_tablet_density_too() {
        let design = Design::solarized().with_density(Density::Tablet);
        let area = Rect::new(0, 0, 50, 6);
        let mut buf = Buffer::empty(area);
        Card::new("Tablet")
            .body("touch")
            .render(area, &mut buf, &design);
        assert!(crate::screen(&buf).contains("touch"));
    }
}
