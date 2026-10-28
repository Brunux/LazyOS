//! The widgets the shell is made of.
//!
//! Every one of them draws into a `Buffer` and allocates nothing: the text a row
//! shows is built once, when the command's state changes, and a frame only
//! copies cells (`optimization.md` §13 - rebuilding 40 formatted rows per frame
//! costs 445 µs against 231 µs for one changing cell, and the difference is
//! allocations, not ratatui).
//!
//! The contract each widget keeps with the rest of the system:
//!
//! - colours and text styles come from `Design` (spec 002's tokens);
//! - the density decides the row stride, so a hit region and the row it names
//!   are the same arithmetic - `ListState::hit_row` is the one that answers, and
//!   a click test drives it at both densities;
//! - the key bar carries the way out, because a window with no visible escape
//!   is the failure the session's manifests keep hitting.

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::design::{Border, ColorRole, Design, TextStyle};
use crate::glyphs::Glyph;

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

/// Write `text` at `(x, y)` and return the column after it.
pub fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, max: u16, style: Style) -> u16 {
    if max == 0 || y >= buf.area.bottom() || x >= buf.area.right() {
        return x;
    }
    buf.set_stringn(x, y, text, max as usize, style).0
}

/// Write `text`, replacing its tail with `…` when it does not fit.
///
/// Two writes instead of a `format!`: an ellipsis built with `format!` allocates
/// once per truncated row per frame, which is the cost this crate exists to
/// avoid.
pub fn put_trunc(buf: &mut Buffer, x: u16, y: u16, text: &str, max: u16, style: Style) -> u16 {
    if max == 0 {
        return x;
    }
    let width = UnicodeWidthStr::width(text) as u16;
    if width <= max {
        return put(buf, x, y, text, max, style);
    }
    let budget = max.saturating_sub(1);
    let mut used = 0u16;
    let mut cut = text.len();
    for (index, ch) in text.char_indices() {
        let cell = UnicodeWidthChar::width(ch).unwrap_or(0) as u16;
        if used + cell > budget {
            cut = index;
            break;
        }
        used += cell;
    }
    let x = put(buf, x, y, &text[..cut], budget, style);
    put(buf, x, y, "\u{2026}", 1, style)
}

/// The same truncation rule, but the text is right-aligned inside `area`.
fn put_right(buf: &mut Buffer, area: Rect, y: u16, text: &str, style: Style, floor: u16) {
    let width = UnicodeWidthStr::width(text) as u16;
    let x = area.right().saturating_sub(width + 1);
    if x > floor && x >= area.left() {
        put(buf, x, y, text, area.right() - x, style);
    }
}

/// Fill a rectangle with one style, keeping the cells' characters.
fn fill(buf: &mut Buffer, area: Rect, style: Style) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    buf.set_style(area, style);
}

/// `width` spaces, without formatting a string per row per frame.
const PADDING: &str = "                ";

fn spaces(width: u16) -> &'static str {
    &PADDING[..(width as usize).min(PADDING.len())]
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

/// The band at the top of every window: what this is, and the one status worth
/// showing while it runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub title: String,
    pub subtitle: Option<String>,
    pub status: Option<String>,
}

impl Header {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            status: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn status(mut self, status: impl Into<String>) -> Self {
        self.status = Some(status.into());
        self
    }
}

pub fn draw_header(buf: &mut Buffer, area: Rect, design: &Design, header: &Header) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    fill(buf, area, design.style(TextStyle::Body));
    let mut x = put(
        buf,
        area.x,
        area.y,
        &header.title,
        area.width,
        design.style(TextStyle::Title),
    );
    match (&header.subtitle, area.height) {
        (Some(subtitle), height) if height >= 3 => {
            put(
                buf,
                area.x,
                area.y + 1,
                subtitle,
                area.width,
                design.style(TextStyle::Dim),
            );
        }
        (Some(subtitle), _) => {
            x = put(
                buf,
                x,
                area.y,
                " \u{00b7} ",
                area.right() - x,
                design.style(TextStyle::Dim),
            );
            put(
                buf,
                x,
                area.y,
                subtitle,
                area.right() - x,
                design.style(TextStyle::Dim),
            );
        }
        (None, _) => {}
    }
    if let Some(status) = &header.status {
        put_right(buf, area, area.y, status, design.style(TextStyle::Dim), x);
    }
    // The rule that separates the header from the body is what makes the band
    // read as a band rather than as a title that happens to be at the top.
    if area.height >= 2 {
        let y = area.bottom() - 1;
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_symbol("\u{2500}");
                cell.set_style(Style::default().fg(design.color(ColorRole::SurfaceAlt)));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Key bar
// ---------------------------------------------------------------------------

/// One `[key] what it does` pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyHint {
    pub key: String,
    pub label: String,
}

impl KeyHint {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
        }
    }
}

/// The bottom band: the keys that are not obvious.
///
/// Letters in brackets rather than Alt chords, because sway sees every Alt chord
/// before the focused surface does (spec 002 §Design) - a hint the session would
/// swallow is worse than no hint.
pub fn draw_keybar(buf: &mut Buffer, area: Rect, design: &Design, hints: &[KeyHint]) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    fill(buf, area, design.style(TextStyle::Body));
    let mut x = area.x + 1;
    for hint in hints {
        if x >= area.right() {
            break;
        }
        x = put(
            buf,
            x,
            area.y,
            "[",
            area.right() - x,
            design.style(TextStyle::Key),
        );
        x = put(
            buf,
            x,
            area.y,
            &hint.key,
            area.right() - x,
            design.style(TextStyle::Key),
        );
        x = put(
            buf,
            x,
            area.y,
            "]",
            area.right() - x,
            design.style(TextStyle::Key),
        );
        x = put(
            buf,
            x,
            area.y,
            " ",
            area.right() - x,
            design.style(TextStyle::Dim),
        );
        x = put(
            buf,
            x,
            area.y,
            " ",
            area.right() - x,
            design.style(TextStyle::Dim),
        );
        x = put(
            buf,
            x,
            area.y,
            &hint.label,
            area.right() - x,
            design.style(TextStyle::Dim),
        );
        x = put(
            buf,
            x,
            area.y,
            "   ",
            area.right() - x,
            design.style(TextStyle::Dim),
        );
    }
}

// ---------------------------------------------------------------------------
// Card
// ---------------------------------------------------------------------------

/// A framed surface, and the area inside it for its content.
///
/// The caller passes the rect; the frame is the design's (`rounded`, the surface
/// colour, the title in the accent). A focused card is drawn in the accent so
/// two cards on one screen say which one has the keys.
pub fn draw_card(
    buf: &mut Buffer,
    area: Rect,
    design: &Design,
    title: Option<&str>,
    focused: bool,
) -> Rect {
    if focused {
        draw_card_with(
            buf,
            area,
            design,
            title,
            ColorRole::Accent,
            design.style(TextStyle::Title),
        )
    } else {
        draw_card_with(
            buf,
            area,
            design,
            title,
            ColorRole::SurfaceAlt,
            design.style(TextStyle::Dim),
        )
    }
}

/// The card frame with its border and title chosen by the caller: a
/// notification's card is drawn in its urgency's role (spec 006), a focused
/// list card in the accent. The frame, the title placement and the truncation
/// are the same in both, which is why this is the one function that draws them.
pub fn draw_card_with(
    buf: &mut Buffer,
    area: Rect,
    design: &Design,
    title: Option<&str>,
    border: ColorRole,
    title_style: Style,
) -> Rect {
    let block = design
        .block(Border::Rounded)
        .border_style(Style::default().fg(design.color(border)));
    let inner = block.inner(area);
    block.render(area, buf);
    // The title is written over the top border by hand instead of through
    // `Block::title`: a padded title built with `format!` allocates once per
    // card per frame, and a card is drawn on every tick of every screen that
    // has one.
    if let Some(title) = title {
        let x = put(buf, area.x + 1, area.y, " ", 1, title_style);
        let x = put_trunc(
            buf,
            x,
            area.y,
            title,
            area.width.saturating_sub(4),
            title_style,
        );
        put(buf, x, area.y, " ", 1, title_style);
    }
    inner
}

// ---------------------------------------------------------------------------
// Row and list
// ---------------------------------------------------------------------------

/// One line of a list: an icon, a label, a dim detail and a value.
///
/// The four fields are what the session's `ui_row` drew with rofi, and keeping
/// them means a ported menu looks like the menu it replaces.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub glyph: Option<String>,
    pub label: String,
    pub detail: Option<String>,
    pub value: Option<String>,
    /// How strongly a [ranked](ListState::ranked) list prefers this row among
    /// rows that match a needle equally well (spec 007: frecency). A plain list
    /// ignores it, and so does a ranked list with an empty needle: the order a
    /// command gave its rows is the order a person reads them in.
    pub boost: u32,
}

impl Row {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            glyph: None,
            label: label.into(),
            detail: None,
            value: None,
            boost: 0,
        }
    }

    pub fn boost(mut self, boost: u32) -> Self {
        self.boost = boost;
        self
    }

    /// The row's picture, from the TUI alphabet ([`Glyph`]).
    ///
    /// The glyph is resolved once, here, not per frame: a name draws either its
    /// codepoint or its text token (`Glyph::cell`), and `ListState` reserves the
    /// widest cell among the rows so the labels line up whether a row has a
    /// picture, a token or nothing.
    pub fn glyph(mut self, glyph: Glyph) -> Self {
        self.glyph = Some(glyph.cell().to_string());
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// `needle` arrives already lowercased (`ListState` does that once per
    /// keystroke, not once per row).
    fn matches(&self, needle: &str) -> bool {
        if needle.is_empty() {
            return true;
        }
        let ascii = needle.is_ascii();
        contains_fold(&self.label, needle, ascii)
            || self
                .detail
                .as_deref()
                .is_some_and(|d| contains_fold(d, needle, ascii))
            || self
                .value
                .as_deref()
                .is_some_and(|v| contains_fold(v, needle, ascii))
    }
}

/// Does `haystack` contain `needle`, ignoring case?
///
/// An **ASCII needle takes the byte path** and allocates nothing: UTF-8 never
/// encodes an ASCII byte inside a multi-byte character, so folding `a-z`/`A-Z`
/// is the whole of case-insensitivity for it - and it is what makes typing in
/// a 5,000-row list a scan instead of a reallocation of every row
/// (`tests/filter.rs` counts both). Any other needle falls back to
/// `to_lowercase`, the only form that folds `Ü` to `ü`; a row pays one
/// allocation, and only while the needle itself carries an accent.
fn contains_fold(haystack: &str, needle_lower: &str, needle_is_ascii: bool) -> bool {
    if needle_is_ascii {
        contains_ascii_fold(haystack.as_bytes(), needle_lower.as_bytes())
    } else {
        haystack.to_lowercase().contains(needle_lower)
    }
}

fn contains_ascii_fold(haystack: &[u8], needle: &[u8]) -> bool {
    match needle {
        [] => true,
        // One character is what the list looks like after the first key.
        [only] => {
            let only = only.to_ascii_lowercase();
            haystack.iter().any(|b| b.to_ascii_lowercase() == only)
        }
        _ => {
            if needle.len() > haystack.len() {
                return false;
            }
            let first = needle[0].to_ascii_lowercase();
            haystack
                .windows(needle.len())
                .any(|w| w[0].to_ascii_lowercase() == first && w.eq_ignore_ascii_case(needle))
        }
    }
}

/// How well a row matches a needle in a [ranked](ListState::ranked) list,
/// best first. `None` is no match; the variants are ordered so a larger value
/// is a better match (spec 007: "exact prefix > word prefix > subsequence").
///
/// The tiers only ever compare the **label**: a person types the name of the
/// thing, and a row whose description happens to contain the letters ranks
/// below every row whose name does - which is `Detail`, the floor, kept so a
/// ranked list still finds what a plain one finds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// The needle is in the detail or the value, not in the label.
    Detail = 1,
    /// The label holds the needle's characters in order (`tst` → `theme set`).
    Subsequence = 2,
    /// The label contains the needle somewhere else.
    Substring = 3,
    /// A word of the label starts with the needle (`set` → `theme set`).
    WordPrefix = 4,
    /// The label starts with the needle.
    Prefix = 5,
    /// The label is the needle.
    Exact = 6,
}

impl Tier {
    /// The tier `row` reaches for `needle` (already lowercased), or `None`.
    ///
    /// Same two paths as the plain filter: an ASCII needle compares bytes and
    /// allocates nothing, any other needle folds the label once.
    pub fn of(row: &Row, needle_lower: &str) -> Option<Tier> {
        if needle_lower.is_empty() {
            return Some(Tier::Exact);
        }
        let ascii = needle_lower.is_ascii();
        let label = if ascii {
            label_tier_ascii(row.label.as_bytes(), needle_lower.as_bytes())
        } else {
            label_tier_unicode(&row.label.to_lowercase(), needle_lower)
        };
        label.or_else(|| {
            let found = |text: Option<&str>| {
                text.is_some_and(|text| contains_fold(text, needle_lower, ascii))
            };
            (found(row.detail.as_deref()) || found(row.value.as_deref())).then_some(Tier::Detail)
        })
    }
}

fn label_tier_ascii(label: &[u8], needle: &[u8]) -> Option<Tier> {
    if label.len() == needle.len() && label.eq_ignore_ascii_case(needle) {
        return Some(Tier::Exact);
    }
    if label.len() >= needle.len() && label[..needle.len()].eq_ignore_ascii_case(needle) {
        return Some(Tier::Prefix);
    }
    if label.len() > needle.len() {
        let first = needle[0].to_ascii_lowercase();
        let mut substring = false;
        for start in 1..=label.len() - needle.len() {
            if label[start].to_ascii_lowercase() != first
                || !label[start..start + needle.len()].eq_ignore_ascii_case(needle)
            {
                continue;
            }
            if !label[start - 1].is_ascii_alphanumeric() {
                return Some(Tier::WordPrefix);
            }
            substring = true;
        }
        if substring {
            return Some(Tier::Substring);
        }
    }
    let mut want = needle.iter().map(u8::to_ascii_lowercase).peekable();
    for byte in label {
        if want.peek() == Some(&byte.to_ascii_lowercase()) {
            want.next();
        }
    }
    want.peek().is_none().then_some(Tier::Subsequence)
}

fn label_tier_unicode(label: &str, needle: &str) -> Option<Tier> {
    if label == needle {
        return Some(Tier::Exact);
    }
    if label.starts_with(needle) {
        return Some(Tier::Prefix);
    }
    let mut substring = false;
    for (start, _) in label.match_indices(needle) {
        let after_boundary = label[..start]
            .chars()
            .next_back()
            .is_none_or(|before| !before.is_alphanumeric());
        if after_boundary {
            return Some(Tier::WordPrefix);
        }
        substring = true;
    }
    if substring {
        return Some(Tier::Substring);
    }
    let mut want = needle.chars().peekable();
    for ch in label.chars() {
        if want.peek() == Some(&ch) {
            want.next();
        }
    }
    want.peek().is_none().then_some(Tier::Subsequence)
}

/// What the list did with a key, so the command decides what a choice means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListAction {
    /// The list did not use this key.
    Ignored,
    Moved,
    Activated(usize),
}

/// The state a list keeps between frames: its rows, the cursor and the window
/// onto them.
///
/// Rows are owned and filtered once per state change, so drawing 40 rows costs
/// copies, not formatting (`tests/no_alloc.rs` watches it).
pub struct ListState {
    rows: Vec<Row>,
    visible: Vec<usize>,
    cursor: usize,
    offset: usize,
    filter: String,
    /// `filter`, lowercased once per change; `Row::matches` takes the folded
    /// form so no row allocates to compare.
    needle: String,
    /// The needle `visible` was computed for. When the next one extends it,
    /// the visible set can only shrink, so the pass is a `retain`.
    applied: String,
    empty: String,
    /// The widest glyph cell among the rows, reserved for every row so a text
    /// token (`bt`) does not shift the labels under it.  Computed when the rows
    /// change, from *all* of them: filtering a row out must not move the column.
    glyph_width: u16,
    /// Order the matches by how well they match (spec 007) instead of keeping
    /// the rows' own order. Off by default: a file list is in the order the
    /// directory gave it, a menu or a palette is in the order of the best match.
    ranked: bool,
    /// The tier each row reached for the current needle, indexed like `rows`
    /// (0 = no match). One buffer for the list's life, so ranking a keystroke
    /// costs no allocation per row - the rule `tests/filter.rs` enforces for
    /// the plain filter holds for the ranked one too.
    tiers: Vec<u8>,
}

impl ListState {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            visible: Vec::new(),
            cursor: 0,
            offset: 0,
            filter: String::new(),
            needle: String::new(),
            applied: String::new(),
            empty: "nothing here yet".to_string(),
            glyph_width: 0,
            ranked: false,
            tiers: Vec::new(),
        }
    }

    pub fn with_empty(mut self, text: impl Into<String>) -> Self {
        self.empty = text.into();
        self
    }

    /// A list that orders its matches: the best [`Tier`] first, then the
    /// larger [`Row::boost`], then the shorter label, then the rows' own order.
    ///
    /// With an empty needle nothing is reordered, so a menu still reads in the
    /// order its groups were written. This is the one ranking the session has:
    /// the root menu and the palette both use it (spec 007), and a surface that
    /// wants another order sets boosts instead of writing a second matcher.
    pub fn ranked(mut self) -> Self {
        self.ranked = true;
        self.tiers.clear();
        self.tiers.resize(self.rows.len(), 0);
        self.applied.clear();
        self.refilter();
        self
    }

    pub fn is_ranked(&self) -> bool {
        self.ranked
    }

    pub fn set_rows(&mut self, rows: Vec<Row>) {
        self.glyph_width = rows
            .iter()
            .filter_map(|row| row.glyph.as_deref())
            .map(|glyph| UnicodeWidthStr::width(glyph) as u16)
            .max()
            .unwrap_or(0);
        self.rows = rows;
        if self.ranked {
            self.tiers.clear();
            self.tiers.resize(self.rows.len(), 0);
        }
        // The visible set belonged to the old rows. Every index is a match
        // again until `refilter` says otherwise, which is what lets its
        // narrowing pass (a `retain`) walk the new list exactly once.
        self.visible.clear();
        self.visible.extend(0..self.rows.len());
        self.applied.clear();
        self.refilter();
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn visible_len(&self) -> usize {
        self.visible.len()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: usize) {
        if !self.visible.is_empty() {
            self.cursor = cursor.min(self.visible.len() - 1);
        }
    }

    pub fn selected(&self) -> Option<&Row> {
        self.visible.get(self.cursor).map(|i| &self.rows[*i])
    }

    /// The selected row's index in the unfiltered set (what a command acts on:
    /// a filter must not change which file it opens).
    pub fn selected_index(&self) -> Option<usize> {
        self.visible.get(self.cursor).copied()
    }

    pub fn filter(&self) -> &str {
        &self.filter
    }

    pub fn set_filter(&mut self, filter: impl Into<String>) {
        self.filter = filter.into();
        self.needle = self.filter.to_lowercase();
        self.refilter();
    }

    /// Recompute the visible set for the current needle.
    ///
    /// Typing appends to the needle, and a longer needle can only match a
    /// subset of a shorter one's matches (`"repo"` matches nothing `"rep"`
    /// did not), so the common case - one more character - tests the rows that
    /// are already visible and keeps the vector in place: no scan of the full
    /// list and no allocation. Anything else (a backspace, a pasted word, a
    /// cleared filter, new rows) rebuilds from scratch.
    fn refilter(&mut self) {
        let narrowing =
            self.needle.len() > self.applied.len() && self.needle.starts_with(&self.applied);
        let rows = &self.rows;
        let needle = &self.needle;
        if self.ranked && !needle.is_empty() {
            // Every tier is monotonic in the needle (a row that holds "repo"
            // as a subsequence holds "rep" too), so narrowing may keep the
            // visible set here as well; the order is rebuilt either way.
            let tiers = &mut self.tiers;
            let mut score = |i: usize| {
                let tier = Tier::of(&rows[i], needle).map_or(0, |t| t as u8);
                tiers[i] = tier;
                tier > 0
            };
            if narrowing {
                self.visible.retain(|&i| score(i));
            } else {
                self.visible.clear();
                self.visible.extend((0..rows.len()).filter(|&i| score(i)));
            }
            let tiers = &self.tiers;
            // Unstable, because the last key is the index: a total order sorts
            // the same way every time, and `sort_unstable` allocates nothing.
            self.visible.sort_unstable_by(|&a, &b| {
                tiers[b]
                    .cmp(&tiers[a])
                    .then(rows[b].boost.cmp(&rows[a].boost))
                    .then(
                        UnicodeWidthStr::width(rows[a].label.as_str())
                            .cmp(&UnicodeWidthStr::width(rows[b].label.as_str())),
                    )
                    .then(a.cmp(&b))
            });
        } else if narrowing {
            self.visible.retain(|&i| rows[i].matches(needle));
        } else {
            self.visible.clear();
            self.visible
                .extend((0..rows.len()).filter(|&i| rows[i].matches(needle)));
        }
        self.applied.clear();
        self.applied.push_str(needle);
        if self.ranked {
            // The best match is the first row, and Enter must take it: a cursor
            // left on the third row by the previous needle would open the
            // third-best answer to this one.
            self.cursor = 0;
        } else if self.cursor >= self.visible.len() {
            self.cursor = self.visible.len().saturating_sub(1);
        }
        self.offset = 0;
    }

    /// How many rows fit in `height` lines at this density.
    pub fn capacity(&self, height: u16, design: &Design) -> u16 {
        height / design.row_stride().max(1)
    }

    pub fn handle(&mut self, key: KeyEvent, height: u16, design: &Design) -> ListAction {
        if key.kind != ratatui::crossterm::event::KeyEventKind::Press {
            return ListAction::Ignored;
        }
        let capacity = self.capacity(height, design).max(1);
        let len = self.visible.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if len == 0 {
                    return ListAction::Ignored;
                }
                self.cursor = self.cursor.saturating_sub(1);
                self.scroll_into_view(capacity);
                ListAction::Moved
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if len == 0 {
                    return ListAction::Ignored;
                }
                self.cursor = (self.cursor + 1).min(len - 1);
                self.scroll_into_view(capacity);
                ListAction::Moved
            }
            KeyCode::PageUp => {
                self.cursor = self.cursor.saturating_sub(capacity as usize);
                self.scroll_into_view(capacity);
                ListAction::Moved
            }
            KeyCode::PageDown => {
                self.cursor = (self.cursor + capacity as usize).min(len.saturating_sub(1));
                self.scroll_into_view(capacity);
                ListAction::Moved
            }
            KeyCode::Home => {
                self.cursor = 0;
                self.scroll_into_view(capacity);
                ListAction::Moved
            }
            KeyCode::End => {
                self.cursor = len.saturating_sub(1);
                self.scroll_into_view(capacity);
                ListAction::Moved
            }
            KeyCode::Enter => match self.selected_index() {
                Some(index) => ListAction::Activated(index),
                None => ListAction::Ignored,
            },
            _ => ListAction::Ignored,
        }
    }

    /// Backspace removes one character of the filter; a printable key adds it.
    /// Returns true when the filter changed.
    pub fn type_filter(&mut self, key: KeyEvent) -> bool {
        if key.kind != ratatui::crossterm::event::KeyEventKind::Press {
            return false;
        }
        match key.code {
            KeyCode::Backspace => {
                let mut filter = self.filter.clone();
                filter.pop();
                self.set_filter(filter);
                true
            }
            KeyCode::Char(ch)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                let mut filter = self.filter.clone();
                filter.push(ch);
                self.set_filter(filter);
                true
            }
            _ => false,
        }
    }

    fn scroll_into_view(&mut self, capacity: u16) {
        let capacity = capacity as usize;
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if capacity > 0 && self.cursor >= self.offset + capacity {
            self.offset = self.cursor + 1 - capacity;
        }
    }

    /// The row under `(x, y)`, or `None` for a blank line (tablet density leaves
    /// one between rows, and a click there must not choose the row above it).
    pub fn hit_row(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        if x < area.left() || x >= area.right() || y < area.y || y >= area.bottom() {
            return None;
        }
        let stride = design.row_stride().max(1);
        let line = (y - area.y) / stride;
        if !(y - area.y).is_multiple_of(stride) {
            return None;
        }
        let index = self.offset + line as usize;
        if index < self.visible.len() {
            Some(index)
        } else {
            None
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, design: &Design, focused: bool) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        fill(
            buf,
            area,
            Style::default()
                .bg(design.color(ColorRole::Bg))
                .fg(design.color(ColorRole::Text)),
        );
        if self.visible.is_empty() {
            put(
                buf,
                area.x + 1,
                area.y,
                &self.empty,
                area.width.saturating_sub(1),
                design.style(TextStyle::Dim),
            );
            return;
        }
        let stride = design.row_stride().max(1);
        let capacity = self.capacity(area.height, design) as usize;
        for line in 0..capacity {
            let index = self.offset + line;
            let Some(row_index) = self.visible.get(index) else {
                break;
            };
            let row = &self.rows[*row_index];
            let y = area.y + line as u16 * stride;
            let selected = focused && index == self.cursor;
            let mut x = area.x;
            if selected {
                let rect = Rect {
                    x: area.x,
                    y,
                    width: area.width,
                    height: 1,
                };
                fill(
                    buf,
                    rect,
                    Style::default().bg(design.color(ColorRole::Selection)),
                );
                x = put(
                    buf,
                    x,
                    y,
                    "\u{258d} ",
                    area.width,
                    design.style(TextStyle::Title),
                );
            } else {
                x = put(buf, x, y, "  ", area.width, design.style(TextStyle::Dim));
            }
            if self.glyph_width > 0 {
                let mut room = self.glyph_width;
                if let Some(glyph) = &row.glyph {
                    let width = UnicodeWidthStr::width(glyph.as_str()) as u16;
                    x = put(
                        buf,
                        x,
                        y,
                        glyph,
                        area.right() - x,
                        design.style(TextStyle::Body),
                    );
                    room = room.saturating_sub(width);
                }
                // A row with no picture of its own still spends the column, or
                // its label starts further left than every other row's.
                x = put(
                    buf,
                    x,
                    y,
                    spaces(room),
                    area.right() - x,
                    design.style(TextStyle::Body),
                );
                x = put(
                    buf,
                    x,
                    y,
                    " ",
                    area.right() - x,
                    design.style(TextStyle::Body),
                );
            }
            let value_width = row
                .value
                .as_deref()
                .map(|v| UnicodeWidthStr::width(v) as u16 + 2)
                .unwrap_or(0);
            let label_room = area.right().saturating_sub(x + value_width + 1);
            let style = if selected {
                design.style(TextStyle::Title)
            } else {
                design.style(TextStyle::Body)
            };
            let mut label_end = put_trunc(buf, x, y, &row.label, label_room, style);
            if let Some(detail) = &row.detail {
                label_end = put(buf, label_end, y, "  ", area.right() - label_end, style);
                label_end = put_trunc(
                    buf,
                    label_end,
                    y,
                    detail,
                    area.right().saturating_sub(label_end + value_width),
                    design.style(TextStyle::Dim),
                );
            }
            if let Some(value) = &row.value {
                let value_style = if selected {
                    design.style(TextStyle::Title)
                } else {
                    design.style(TextStyle::Dim)
                };
                put_right(
                    buf,
                    Rect {
                        x: label_end,
                        y,
                        width: area.right().saturating_sub(label_end + 1),
                        height: 1,
                    },
                    y,
                    value,
                    value_style,
                    label_end,
                );
            }
        }
    }
}

impl Default for ListState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Empty state, failure card
// ---------------------------------------------------------------------------

/// A rectangle centred in `area`, `percent_x` wide and `height` lines tall.
pub fn centered(area: Rect, percent_x: u16, height: u16) -> Rect {
    let width = area
        .width
        .saturating_mul(percent_x)
        .saturating_div(100)
        .max(24)
        .min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn centred_line(buf: &mut Buffer, area: Rect, y: u16, text: &str, style: Style) {
    let width = UnicodeWidthStr::width(text) as u16;
    let x = area.x + area.width.saturating_sub(width) / 2;
    put(buf, x, y, text, area.width, style);
}

/// The card a command shows when it has nothing to show yet.
pub fn draw_empty(buf: &mut Buffer, area: Rect, design: &Design, title: &str, hint: &str) {
    let card = centered(area, 62, 9);
    let inner = draw_card(buf, card, design, Some("LazyOS"), true);
    centred_line(
        buf,
        inner,
        inner.y + 1,
        title,
        design.style(TextStyle::Title),
    );
    centred_line(buf, inner, inner.y + 3, hint, design.style(TextStyle::Dim));
}

/// The card a command shows when it failed: what failed, and the way out.
pub fn draw_failure(buf: &mut Buffer, area: Rect, design: &Design, summary: &str, detail: &str) {
    let card = centered(area, 76, 11);
    let inner = draw_card(
        buf,
        card,
        design,
        Some("LazyOS: something went wrong"),
        true,
    );
    centred_line(
        buf,
        inner,
        inner.y + 1,
        summary,
        design.style(TextStyle::Danger),
    );
    centred_line(
        buf,
        inner,
        inner.y + 3,
        detail,
        design.style(TextStyle::Body),
    );
    centred_line(
        buf,
        inner,
        inner.bottom().saturating_sub(1),
        "press any key to close",
        design.style(TextStyle::Dim),
    );
}

// ---------------------------------------------------------------------------
// Badge, gauge, progress
// ---------------------------------------------------------------------------

/// A one-line pill: ` text ` on the role's colour. Returns the column after it.
pub fn draw_badge(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    design: &Design,
    text: &str,
    role: ColorRole,
) -> u16 {
    let style = Style::default()
        .fg(design.color(ColorRole::Bg))
        .bg(design.color(role));
    let mut x = put(buf, x, y, " ", 1, style);
    x = put(buf, x, y, text, u16::MAX, style);
    put(buf, x, y, " ", 1, style)
}

/// The width `put_percent` writes: three digits and the sign, never more.
pub const PERCENT_WIDTH: u16 = 4;

const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// Write ` 42%` (four cells, right-aligned) without formatting a string.
///
/// `format!` here would be one allocation per gauge per frame, which is the cost
/// `optimization.md` §13 names; a percentage is an integer from 0 to 100 and
/// needs no formatter.
pub fn put_percent(buf: &mut Buffer, x: u16, y: u16, value: f64, style: Style) -> u16 {
    let percent = (value.clamp(0.0, 1.0) * 100.0).round() as u16;
    let (hundreds, tens, units) = if percent >= 100 {
        ("1", "0", "0")
    } else if percent >= 10 {
        (
            " ",
            DIGITS[(percent / 10) as usize],
            DIGITS[(percent % 10) as usize],
        )
    } else {
        (" ", " ", DIGITS[percent as usize])
    };
    let mut x = put(buf, x, y, hundreds, 1, style);
    x = put(buf, x, y, tens, 1, style);
    x = put(buf, x, y, units, 1, style);
    put(buf, x, y, "%", 1, style)
}

/// A bar with end caps: `▕████░░░░▕  42% battery`.
///
/// The label is reserved before the bar is measured, or a wide gauge eats the
/// word that says what the number means (80 columns of bar and `b…`).
pub fn draw_gauge(buf: &mut Buffer, area: Rect, design: &Design, value: f64, label: &str) {
    if area.width < 12 || area.height == 0 {
        return;
    }
    let value = value.clamp(0.0, 1.0);
    let role = if value >= 0.9 {
        ColorRole::Danger
    } else if value >= 0.7 {
        ColorRole::Warn
    } else {
        ColorRole::Ok
    };
    let style = Style::default().fg(design.color(role));
    let empty = Style::default().fg(design.color(ColorRole::SurfaceAlt));
    let label_room = (UnicodeWidthStr::width(label) as u16).min(area.width / 3);
    // "100%" / " 42%" / "  7%": four cells, built from digits rather than
    // `format!` so a gauge allocates nothing per frame.
    let fixed = 4 + 1 + PERCENT_WIDTH + if label_room > 0 { 1 + label_room } else { 0 };
    let bars = area.width.saturating_sub(fixed);
    let filled = (bars as f64 * value).round() as u16;
    let mut x = put(buf, area.x, area.y, "\u{2595}", 1, empty);
    for i in 0..bars {
        let (symbol, style) = if i < filled {
            ("\u{2588}", style)
        } else {
            ("\u{2591}", empty)
        };
        x = put(buf, x, area.y, symbol, 1, style);
    }
    x = put(buf, x, area.y, "\u{2595}", 1, empty);
    x = put(buf, x, area.y, " ", 1, empty);
    x = put_percent(buf, x, area.y, value, style);
    if label_room > 0 {
        x = put(buf, x, area.y, " ", 1, empty);
        put_trunc(
            buf,
            x,
            area.y,
            label,
            area.right() - x,
            design.style(TextStyle::Dim),
        );
    }
}

/// A thin progress bar for a task whose total is unknown: the bar carries the
/// fraction, the label says what is running.
pub fn draw_progress(
    buf: &mut Buffer,
    area: Rect,
    design: &Design,
    value: f64,
    label: &str,
    role: ColorRole,
) {
    if area.width < 8 || area.height == 0 {
        return;
    }
    let value = value.clamp(0.0, 1.0);
    let style = Style::default().fg(design.color(role));
    let empty = Style::default().fg(design.color(ColorRole::SurfaceAlt));
    let label_width = UnicodeWidthStr::width(label) as u16;
    let bar_width = if label_width == 0 {
        area.width
    } else {
        area.width.saturating_sub(label_width + 1)
    };
    let filled = (bar_width as f64 * value).round() as u16;
    for x in area.left()..area.left() + bar_width {
        let (symbol, style) = if x - area.left() < filled {
            ("\u{2588}", style)
        } else {
            ("\u{2591}", empty)
        };
        put(buf, x, area.y, symbol, 1, style);
    }
    if label_width > 0 {
        let x = area.left() + bar_width + 1;
        put_trunc(
            buf,
            x,
            area.y,
            label,
            area.right().saturating_sub(x),
            design.style(TextStyle::Dim),
        );
    }
}

// ---------------------------------------------------------------------------
// Log pane
// ---------------------------------------------------------------------------

/// The tail of a log, following it unless the person scrolled up.
///
/// Following is a state, not a timer: the pane jumps to the end when a line is
/// pushed while following, and stops following the moment a scroll key arrives -
/// which is how "the log stays where I put it" works without a flag the command
/// has to remember.
pub struct LogPane {
    lines: Vec<String>,
    follow: bool,
    offset: usize,
}

impl LogPane {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            follow: true,
            offset: 0,
        }
    }

    pub fn push(&mut self, line: impl Into<String>) {
        self.lines.push(line.into());
        if self.lines.len() > 5_000 {
            self.lines.drain(..1_000);
        }
        if self.follow {
            self.offset = 0;
        }
    }

    pub fn set_lines(&mut self, lines: Vec<String>) {
        self.lines = lines;
        if self.follow {
            self.offset = 0;
        }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn following(&self) -> bool {
        self.follow
    }

    pub fn follow(&mut self, follow: bool) {
        self.follow = follow;
        if follow {
            self.offset = 0;
        }
    }

    /// Scroll keys; returns true when the pane used the key.
    pub fn handle(&mut self, key: KeyEvent, height: u16) -> bool {
        if key.kind != ratatui::crossterm::event::KeyEventKind::Press {
            return false;
        }
        let page = height.saturating_sub(1).max(1) as usize;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.follow = false;
                self.offset = (self.offset + 1).min(self.lines.len().saturating_sub(1));
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.offset = self.offset.saturating_sub(1);
                if self.offset == 0 {
                    self.follow = true;
                }
                true
            }
            KeyCode::PageUp => {
                self.follow = false;
                self.offset = (self.offset + page).min(self.lines.len().saturating_sub(1));
                true
            }
            KeyCode::PageDown => {
                self.offset = self.offset.saturating_sub(page);
                if self.offset == 0 {
                    self.follow = true;
                }
                true
            }
            KeyCode::End => {
                self.follow = true;
                self.offset = 0;
                true
            }
            _ => false,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, design: &Design) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        fill(buf, area, design.style(TextStyle::Body));
        let height = area.height as usize;
        let end = self.lines.len().saturating_sub(self.offset);
        let start = end.saturating_sub(height);
        for (line_no, line) in self.lines[start..end].iter().enumerate() {
            let y = area.y + line_no as u16;
            put_trunc(
                buf,
                area.x,
                y,
                line,
                area.width,
                design.style(TextStyle::Dim),
            );
        }
        if !self.follow && !self.lines.is_empty() {
            put_right(
                buf,
                area,
                area.bottom().saturating_sub(1),
                "[paused]",
                design.style(TextStyle::Danger),
                area.left(),
            );
        }
    }
}

impl Default for LogPane {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Confirm
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmAction {
    None,
    Cancelled,
    Chosen(usize),
}

/// A destructive question whose first answer is Cancel and whose first row is
/// selected: a stray Enter does nothing.
pub struct Confirm {
    pub title: String,
    pub message: String,
    pub detail: Option<String>,
    options: Vec<String>,
    selected: usize,
}

impl Confirm {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            detail: None,
            options: vec!["Cancel".to_string(), "Continue".to_string()],
            selected: 0,
        }
    }

    pub fn with_options(mut self, options: &[&str]) -> Self {
        self.options = options.iter().map(|o| o.to_string()).collect();
        self.selected = 0;
        self
    }

    pub fn selected_label(&self) -> &str {
        self.options
            .get(self.selected)
            .map(String::as_str)
            .unwrap_or("Cancel")
    }

    pub fn handle(&mut self, key: KeyEvent) -> ConfirmAction {
        if key.kind != ratatui::crossterm::event::KeyEventKind::Press {
            return ConfirmAction::None;
        }
        match key.code {
            KeyCode::Esc => ConfirmAction::Cancelled,
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                ConfirmAction::None
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.options.len().saturating_sub(1));
                ConfirmAction::None
            }
            KeyCode::Enter => ConfirmAction::Chosen(self.selected),
            // Typing the first letter of an answer is how a person says "no"
            // quickly; it never activates the destructive row by accident,
            // because Cancel starts with C and is the first answer.
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                let wanted = ch.to_lowercase().to_string();
                match self
                    .options
                    .iter()
                    .position(|o| o.to_lowercase().starts_with(&wanted))
                {
                    Some(index) => ConfirmAction::Chosen(index),
                    None => ConfirmAction::None,
                }
            }
            _ => ConfirmAction::None,
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, design: &Design) {
        let card = centered(area, 70, 11);
        let inner = draw_card(buf, card, design, Some(&self.title), true);
        centred_line(
            buf,
            inner,
            inner.y,
            &self.message,
            design.style(TextStyle::Body),
        );
        if let Some(detail) = &self.detail {
            centred_line(
                buf,
                inner,
                inner.y + 1,
                detail,
                design.style(TextStyle::Dim),
            );
        }
        let row = inner.bottom().saturating_sub(2);
        // The options sit where the key bar's hints would: the selected answer
        // in the accent, the rest dim, Cancel always the first.
        let width: u16 = self
            .options
            .iter()
            .map(|o| UnicodeWidthStr::width(o.as_str()) as u16 + 4)
            .sum();
        let mut x = inner.x + inner.width.saturating_sub(width) / 2;
        for (index, option) in self.options.iter().enumerate() {
            let selected = index == self.selected;
            let style = if selected {
                design.style(TextStyle::Key)
            } else {
                design.style(TextStyle::Dim)
            };
            x = put(buf, x, row, " ", 1, style);
            x = put(buf, x, row, option, u16::MAX, style);
            x = put(buf, x, row, " ", 1, style);
            x = put(buf, x, row, "  ", 2, design.style(TextStyle::Dim));
        }
        let _ = x;
    }
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// What a key did to an [`Input`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputAction {
    /// Not a key the input uses (the screen may give it to the shell).
    Ignored,
    Changed,
    /// Enter, with text in the field.
    Submitted,
    /// Escape: the person did not want to ask after all.
    Cancelled,
}

/// One line of text the person types: the Ask strip's question (spec 007), and
/// later the run prompt (spec 012).
///
/// It edits at the end only - the plan's keymap has no Alt chords inside a
/// window, and a one-line question is typed, not edited in the middle - with
/// Backspace, `Ctrl+U` (clear) and `Ctrl+W` (the last word). Enter on an empty
/// field does nothing: a stray Enter must not send an empty question.
#[derive(Clone, Debug, Default)]
pub struct Input {
    text: String,
}

impl Input {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn handle(&mut self, key: KeyEvent) -> InputAction {
        if key.kind != ratatui::crossterm::event::KeyEventKind::Press {
            return InputAction::Ignored;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => InputAction::Cancelled,
            KeyCode::Enter if self.text.trim().is_empty() => InputAction::Ignored,
            KeyCode::Enter => InputAction::Submitted,
            KeyCode::Backspace => {
                if self.text.pop().is_some() {
                    InputAction::Changed
                } else {
                    InputAction::Ignored
                }
            }
            KeyCode::Char('u') if control => {
                self.text.clear();
                InputAction::Changed
            }
            KeyCode::Char('w') if control => {
                let kept = self.text.trim_end().len();
                let start = self.text[..kept]
                    .rfind(char::is_whitespace)
                    .map_or(0, |space| space + 1);
                self.text.truncate(start);
                InputAction::Changed
            }
            KeyCode::Char(ch) if !control && !key.modifiers.contains(KeyModifiers::ALT) => {
                self.text.push(ch);
                InputAction::Changed
            }
            _ => InputAction::Ignored,
        }
    }

    /// One line: the text, or the dim placeholder, and a block cursor. A text
    /// wider than the field shows its tail, because the end is where the
    /// person is typing.
    pub fn render(&self, area: Rect, buf: &mut Buffer, design: &Design, placeholder: &str) {
        if area.width < 2 || area.height == 0 {
            return;
        }
        fill(
            buf,
            Rect { height: 1, ..area },
            Style::default()
                .bg(design.color(ColorRole::Surface))
                .fg(design.color(ColorRole::Text)),
        );
        let room = area.width.saturating_sub(2);
        let x = area.x + 1;
        if self.text.is_empty() {
            put_trunc(
                buf,
                x + 1,
                area.y,
                placeholder,
                room.saturating_sub(1),
                design
                    .style(TextStyle::Dim)
                    .bg(design.color(ColorRole::Surface)),
            );
            put(buf, x, area.y, " ", 1, design.style(TextStyle::Key));
            return;
        }
        // The tail that fits, measured in cells from the end.
        let mut start = self.text.len();
        let mut width = 0u16;
        for (index, ch) in self.text.char_indices().rev() {
            let w = UnicodeWidthStr::width(ch.encode_utf8(&mut [0; 4]) as &str) as u16;
            if width + w + 1 > room {
                break;
            }
            width += w;
            start = index;
        }
        let end = put(
            buf,
            x,
            area.y,
            &self.text[start..],
            room,
            design
                .style(TextStyle::Body)
                .bg(design.color(ColorRole::Surface)),
        );
        put(buf, end, area.y, " ", 1, design.style(TextStyle::Key));
    }
}

// ---------------------------------------------------------------------------
// Tests that do not need a snapshot
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn rows(n: usize) -> ListState {
        let mut list = ListState::new();
        list.set_rows(
            (0..n)
                .map(|i| Row::new(format!("row {i:02}")).detail("detail"))
                .collect(),
        );
        list
    }

    #[test]
    fn the_cursor_scrolls_the_window_and_stops_at_the_ends() {
        let design = Design::solarized();
        let mut list = rows(40);
        for _ in 0..39 {
            assert_eq!(
                list.handle(key(KeyCode::Down), 10, &design),
                ListAction::Moved
            );
        }
        assert_eq!(list.cursor(), 39);
        assert_eq!(
            list.handle(key(KeyCode::Down), 10, &design),
            ListAction::Moved
        );
        assert_eq!(list.cursor(), 39, "the last row is the end");
        assert_eq!(
            list.handle(key(KeyCode::Home), 10, &design),
            ListAction::Moved
        );
        assert_eq!(list.cursor(), 0);
    }

    #[test]
    fn the_window_follows_the_cursor() {
        let design = Design::solarized();
        let mut list = rows(40);
        for _ in 0..25 {
            list.handle(key(KeyCode::Down), 10, &design);
        }
        // The cursor must be inside the drawn window.
        assert!(list.offset <= list.cursor);
        assert!(list.cursor < list.offset + 10);
    }

    #[test]
    fn a_click_maps_to_the_row_it_was_drawn_on() {
        let normal = Design::solarized();
        let tablet = Design::solarized().with_density(crate::design::Density::Tablet);
        let list = rows(40);
        let area = Rect::new(0, 3, 80, 20);
        for (design, step) in [(normal, 1u16), (tablet, 2u16)] {
            for row in 0..5u16 {
                let y = area.y + row * step;
                assert_eq!(
                    list.hit_row(area, 10, y, &design),
                    Some(row as usize),
                    "{design:?} row {row}"
                );
                if step == 2 {
                    // The blank line between two touch rows belongs to nobody.
                    assert_eq!(list.hit_row(area, 10, y + 1, &design), None);
                }
            }
            assert_eq!(list.hit_row(area, 10, area.bottom(), &design), None);
            assert_eq!(list.hit_row(area, 10, area.y + 200, &design), None);
        }
    }

    #[test]
    fn the_filter_narrows_the_rows_and_keeps_the_index() {
        let mut list = ListState::new();
        list.set_rows(vec![
            Row::new("alpha").detail("first"),
            Row::new("beta").detail("second"),
            Row::new("gamma").detail("third"),
        ]);
        list.set_filter("amm");
        assert_eq!(list.visible_len(), 1);
        assert_eq!(list.selected().unwrap().label, "gamma");
        // The index is the unfiltered one: a command acts on the row, not on
        // its position in the filtered view.
        assert_eq!(list.selected_index(), Some(2));
    }

    #[test]
    fn typing_builds_the_filter_and_backspace_removes_it() {
        let mut list = rows(5);
        assert!(list.type_filter(key(KeyCode::Char('r'))));
        assert_eq!(list.filter(), "r");
        assert!(!list.type_filter(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL)));
        assert!(list.type_filter(key(KeyCode::Backspace)));
        assert_eq!(list.filter(), "");
    }

    #[test]
    fn a_row_without_a_glyph_lines_up_with_one_that_has_it() {
        let design = Design::solarized();
        let mut list = ListState::new();
        list.set_rows(vec![
            Row::new("pictured").glyph(Glyph::Folder),
            Row::new("bare"),
            // A text token is wider than a codepoint; the column takes the
            // widest row, so every label still starts at the same place.
            Row::new("tokenised").glyph(Glyph::Bluetooth),
        ]);
        let area = Rect::new(0, 0, 60, 3);
        let mut buf = Buffer::empty(area);
        list.render(area, &mut buf, &design, false);
        let line = |y: u16| -> String {
            (0..60)
                .map(|x| buf.cell((x, y)).unwrap().symbol().to_string())
                .collect()
        };
        let columns: Vec<usize> = ["pictured", "bare", "tokenised"]
            .iter()
            .enumerate()
            .map(|(row, label)| {
                let text = line(row as u16);
                let byte = text
                    .find(label)
                    .unwrap_or_else(|| panic!("{label} is not on row {row}"));
                // A glyph is three bytes and one cell, so the column is the
                // number of characters before the label, not of bytes.
                text[..byte].chars().count()
            })
            .collect();
        assert_eq!(columns[0], columns[1], "a row with no glyph drifts left");
        assert_eq!(columns[0], columns[2], "a text token widens the column");
        assert_eq!(columns[0], 5, "two of margin, the token, one of gap");
    }

    #[test]
    fn a_list_without_glyphs_does_not_reserve_the_column() {
        let design = Design::solarized();
        let mut list = ListState::new();
        list.set_rows(vec![Row::new("plain")]);
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);
        list.render(area, &mut buf, &design, false);
        let text: String = (0..40)
            .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
            .collect();
        assert!(text.starts_with("  plain"), "{text:?}");
    }

    #[test]
    fn the_fallback_glyph_is_text() {
        assert_eq!(Glyph::Bluetooth.codepoint(), None);
        assert_eq!(Glyph::Bluetooth.cell(), "bt");
        assert_eq!(Glyph::Folder.cell(), "\u{ea83}");
        let row = Row::new("x").glyph(Glyph::Bluetooth);
        assert_eq!(row.glyph.as_deref(), Some("bt"));
    }

    #[test]
    fn confirm_starts_on_cancel_and_needs_a_second_key_for_continue() {
        let mut confirm = Confirm::new("Format the card?", "This erases it");
        assert_eq!(confirm.selected_label(), "Cancel");
        assert_eq!(confirm.handle(key(KeyCode::Right)), ConfirmAction::None);
        assert_eq!(confirm.selected_label(), "Continue");
        assert_eq!(confirm.handle(key(KeyCode::Right)), ConfirmAction::None);
        assert_eq!(
            confirm.selected_label(),
            "Continue",
            "the last answer is the end"
        );
        assert_eq!(
            confirm.handle(key(KeyCode::Enter)),
            ConfirmAction::Chosen(1)
        );
        let mut confirm = Confirm::new("Format the card?", "This erases it");
        assert_eq!(confirm.handle(key(KeyCode::Esc)), ConfirmAction::Cancelled);
    }

    #[test]
    fn a_destructive_answer_is_not_the_default() {
        let mut confirm = Confirm::new("Shut down?", "");
        // Enter without moving chooses the first row, which is Cancel.
        assert_eq!(
            confirm.handle(key(KeyCode::Enter)),
            ConfirmAction::Chosen(0)
        );
        assert_eq!(confirm.selected_label(), "Cancel");
    }

    #[test]
    fn the_log_follows_until_scrolled_and_comes_back() {
        let mut log = LogPane::new();
        for i in 0..100 {
            log.push(format!("line {i}"));
        }
        assert!(log.following());
        assert!(log.handle(key(KeyCode::PageUp), 10));
        assert!(!log.following());
        assert!(log.handle(key(KeyCode::End), 10));
        assert!(log.following());
    }

    #[test]
    fn the_selected_row_is_filled_and_bars_the_margin() {
        let design = Design::solarized();
        let mut list = rows(3);
        list.set_cursor(1);
        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);
        list.render(area, &mut buf, &design, true);
        let bar = buf.cell((0, 1)).unwrap();
        assert_eq!(bar.symbol(), "\u{258d}", "the selected row has the bar");
        assert_eq!(bar.fg, design.color(ColorRole::Accent));
        assert_eq!(
            buf.cell((8, 1)).unwrap().bg,
            design.color(ColorRole::Selection),
            "and its whole width is filled"
        );
        assert_eq!(
            buf.cell((8, 0)).unwrap().bg,
            design.color(ColorRole::Bg),
            "the other rows are not"
        );
        // Without focus nothing is highlighted: two lists on one screen must
        // not both claim the keys.
        let mut buf = Buffer::empty(area);
        list.render(area, &mut buf, &design, false);
        assert_ne!(buf.cell((0, 1)).unwrap().symbol(), "\u{258d}");
    }

    #[test]
    fn truncation_keeps_the_ellipsis_inside_the_width() {
        let area = Rect::new(0, 0, 20, 3);
        let mut buf = Buffer::empty(area);
        let design = Design::solarized();
        let used = put_trunc(
            &mut buf,
            0,
            0,
            "a very long label indeed",
            10,
            design.style(TextStyle::Body),
        );
        assert!(used <= 10, "the truncation respected the width: {used}");
        let line: String = (0..10)
            .map(|x| buf.cell((x, 0)).unwrap().symbol().to_string())
            .collect();
        assert!(line.ends_with('\u{2026}'), "{line:?}");
    }
}
