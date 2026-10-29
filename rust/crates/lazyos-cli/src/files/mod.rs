//! The three search questions (spec 009), and the one opener behind them.
//!
//! ```text
//! lazyos menu files [DIR]     which file is it, in this folder   (rofi-files)
//! lazyos menu files --search  which file was it, newest first    (Alt+Search)
//! lazyos menu find [DIR]      I will know it when I see it       (lazyos-find)
//! lazyos menu grep [TEXT]     where did I write this             (rofi-grep)
//! lazyos open PATH            open it the way the list would     (lazyos-view)
//! ```
//!
//! Each surface is a list with the preview beside it, and each ends the same
//! way: the window closes and the file opens through [`open::plan`], the one
//! table - so the list, the window manager's double click and an `xdg-open`
//! from a script agree about where a file goes.

pub mod browse;
pub mod find;
pub mod grep;
mod md5;
pub mod open;
pub mod preview;

use std::fs::Metadata;
use std::path::{Path, PathBuf};

use lazyos_ui::widgets::{put, put_trunc};
use lazyos_ui::{BodyCtx, Card, Glyph, Icon, ListState, TextStyle, Urgency, draw_card};
use ratatui::layout::Rect;

use preview::Previewer;

/// What a surface decided, for the caller once the window is gone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chosen {
    /// Open this path with its program.
    Open(PathBuf),
    /// The actions menu for this path (`rofi-file-actions`).
    Actions(PathBuf),
}

/// Bytes as the list printed them: `512 B`, `4.2K`, `18M`.
pub fn human(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    const UNITS: [&str; 4] = ["K", "M", "G", "T"];
    while value >= 1024.0 && unit < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1}{}", UNITS[unit - 1])
    } else {
        format!("{value:.0}{}", UNITS[unit - 1])
    }
}

/// The modification day, local time, as `find -printf %TY-%Tm-%Td` wrote it.
pub fn day(meta: &Metadata) -> String {
    local(meta, false)
}

/// The modification time with the minutes, for the preview's facts.
pub fn date(meta: &Metadata) -> String {
    local(meta, true)
}

fn local(meta: &Metadata, minutes: bool) -> String {
    use std::os::unix::fs::MetadataExt;
    let seconds = meta.mtime() as libc::time_t;
    // SAFETY: localtime_r writes only into `tm`, which outlives the call.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&seconds, &mut tm).is_null() };
    if !ok {
        return "?".into();
    }
    let date = format!(
        "{:04}-{:02}-{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday
    );
    if minutes {
        format!("{date} {:02}:{:02}", tm.tm_hour, tm.tm_min)
    } else {
        date
    }
}

/// `rwxr-xr-x`, the way `ls` says it.
pub fn mode_string(mode: u32) -> String {
    let mut out = String::with_capacity(9);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 7;
        out.push(if bits & 4 != 0 { 'r' } else { '-' });
        out.push(if bits & 2 != 0 { 'w' } else { '-' });
        out.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    out
}

/// `~/rest` for a path under the home directory, the path otherwise.
pub fn shown(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// The free space under a path, as `df -h` would print it.
pub fn free_space(path: &Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: statvfs fills `stat` and reads the NUL-terminated path.
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    Some(human(stat.f_bavail as u64 * stat.f_frsize as u64))
}

/// The list and its preview, side by side once the window is wide enough to
/// give the preview room; a narrow window keeps the list alone.
pub struct Split {
    pub filter: Rect,
    pub rows: Rect,
    pub list_card: Rect,
    pub preview: Option<Rect>,
}

/// Below this many columns the preview is dropped: a 40-column list beside a
/// 30-column picture is two things nobody can read.
const PREVIEW_MIN_WIDTH: u16 = 84;

pub fn split(area: Rect) -> Split {
    let (list_card, preview) = if area.width >= PREVIEW_MIN_WIDTH {
        let list_width = area.width * 55 / 100;
        (
            Rect {
                width: list_width,
                ..area
            },
            Some(Rect {
                x: area.x + list_width,
                width: area.width - list_width,
                ..area
            }),
        )
    } else {
        (area, None)
    };
    // Inside the card's border: the filter line, a rule's worth of space,
    // then the rows.
    let inner = Rect {
        x: list_card.x + 1,
        y: list_card.y + 1,
        width: list_card.width.saturating_sub(2),
        height: list_card.height.saturating_sub(2),
    };
    let filter = Rect { height: 1, ..inner };
    let rows = Rect {
        y: inner.y + 2,
        height: inner.height.saturating_sub(2),
        ..inner
    };
    Split {
        filter,
        rows,
        list_card,
        preview,
    }
}

/// The list card with its filter line, the way the menu draws it, and the
/// preview of `selected` beside it.
#[allow(clippy::too_many_arguments)]
pub fn draw_split(
    ctx: &mut BodyCtx,
    title: &str,
    placeholder: &str,
    list: &ListState,
    rows_height: &std::cell::Cell<u16>,
    previewer: Option<&mut Previewer>,
    selected: Option<&Path>,
) {
    let layout = split(ctx.area);
    draw_card(ctx.buf, layout.list_card, ctx.design, Some(title), true);
    let filter = layout.filter;
    let x = put(
        ctx.buf,
        filter.x,
        filter.y,
        Glyph::Search.cell(),
        filter.width,
        ctx.design.style(TextStyle::Title),
    );
    let x = put(
        ctx.buf,
        x,
        filter.y,
        " ",
        1,
        ctx.design.style(TextStyle::Body),
    );
    let room = filter.right().saturating_sub(x);
    if list.filter().is_empty() {
        put_trunc(
            ctx.buf,
            x,
            filter.y,
            placeholder,
            room,
            ctx.design.style(TextStyle::Dim),
        );
    } else {
        let end = put_trunc(
            ctx.buf,
            x,
            filter.y,
            list.filter(),
            room.saturating_sub(1),
            ctx.design.style(TextStyle::Body),
        );
        put(
            ctx.buf,
            end,
            filter.y,
            " ",
            1,
            ctx.design.style(TextStyle::Key),
        );
    }
    rows_height.set(layout.rows.height);
    list.render(layout.rows, ctx.buf, ctx.design, true);
    if let (Some(area), Some(previewer)) = (layout.preview, previewer) {
        previewer.draw(ctx, area, selected);
    }
}

/// Open a path once the window is gone: the plan from the table, started
/// detached so it outlives this process. A refusal is a card, because there
/// is no window left to say it in.
pub fn launch(path: &Path, home: &str) -> i32 {
    let tools = open::System::from_env();
    let text = open::is_text(path);
    match open::plan(path, text, &tools) {
        Ok(plan) => {
            if plan.image_hint {
                image_hint(home);
            }
            match crate::menu::spawn_detached(&plan.argv, home) {
                Ok(()) => 0,
                Err(error) => {
                    refuse(path, &format!("Could not start {}: {error}", plan.argv[0]));
                    3
                }
            }
        }
        Err(refusal) => {
            refuse(path, refusal.message());
            3
        }
    }
}

/// `lazyos-image-hint`: imv's keys, once per session. It never fails an open.
pub fn image_hint(home: &str) {
    let hint = Path::new(home).join(".config/bin/lazyos-image-hint");
    let _ = std::process::Command::new(hint)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

pub fn refuse(path: &Path, message: &str) {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let _ = Card::new(name)
        .body(message)
        .icon(Icon::Glyph(Glyph::Warning))
        .urgency(Urgency::Normal)
        .send();
}

/// The actions menu for a path, after the window is gone. It is still the
/// rofi script: its long steps are `busy` cards with their own stop patterns,
/// and porting them is spec 010's dialog work, not a search's.
pub fn actions(path: &Path, home: &str) -> i32 {
    let script = Path::new(home).join(".config/bin/rofi-file-actions");
    let argv = vec![script.display().to_string(), path.display().to_string()];
    match crate::menu::spawn_detached(&argv, home) {
        Ok(()) => 0,
        Err(error) => {
            refuse(path, &format!("The actions menu did not start: {error}"));
            3
        }
    }
}

/// What a surface asked for, done.
pub fn finish(chosen: Option<Chosen>, code: i32, home: &str) -> i32 {
    match chosen {
        Some(Chosen::Open(path)) => launch(&path, home),
        Some(Chosen::Actions(path)) => actions(&path, home),
        None => code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_the_old_list() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(1023), "1023 B");
        assert_eq!(human(1024), "1.0K");
        assert_eq!(human(4300), "4.2K");
        assert_eq!(human(18 * 1024 * 1024), "18M");
        assert_eq!(human(3 * 1024 * 1024 * 1024), "3.0G");
    }

    #[test]
    fn modes_read_like_ls() {
        assert_eq!(mode_string(0o755), "rwxr-xr-x");
        assert_eq!(mode_string(0o600), "rw-------");
    }

    #[test]
    fn a_home_path_is_shown_with_a_tilde() {
        let home = Path::new("/home/me");
        assert_eq!(shown(Path::new("/home/me"), home), "~");
        assert_eq!(shown(Path::new("/home/me/a/b"), home), "~/a/b");
        assert_eq!(shown(Path::new("/etc"), home), "/etc");
    }

    #[test]
    fn the_preview_needs_room() {
        let wide = split(Rect::new(0, 0, 100, 30));
        assert!(wide.preview.is_some());
        assert_eq!(wide.list_card.width + wide.preview.unwrap().width, 100);
        assert!(split(Rect::new(0, 0, 80, 24)).preview.is_none());
    }
}
