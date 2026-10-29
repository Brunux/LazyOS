//! The preview pane (spec 009): the picture is the preview.
//!
//! [`kind_for`] decides what a path shows - its thumbnail, a text excerpt, a
//! directory's first names, or the facts about it - and [`Previewer`] keeps
//! what it built per path, so moving the cursor back over a row costs nothing.
//!
//! Three decisions are the feature:
//!
//! - **The thumbnail is the XDG cache.** `~/.cache/thumbnails/large/<md5 of
//!   the path>.png` (256 px), made by `lazyos-thumbnail` - the four-slot `flock`
//!   gate, the `.part` rename - so a picture is decoded once, ever, and the UI
//!   never decodes a 24 MP photo. The rofi list's 128 px `normal` size was a
//!   32 px icon's source; a pane is ~600 px wide, so it asks for the large one
//!   and draws it at most twice its size (a 128 px picture stretched across the
//!   pane was a blur, and `Resize::Fit` alone drew it as a stamp in a corner).
//!   A missing one is asked for in a thread and the pane says so until it
//!   arrives.
//! - **The protocol is the terminal's.** foot answers the query with sixel, so
//!   the thumbnail is drawn as a picture; a terminal that does not answer gets
//!   half blocks. The query is made once, on this thread, before the first
//!   frame ([`query_terminal`] says why not in a thread), and
//!   `LAZYOS_PICTURES=halfblocks|off` decides it without asking.
//! - **Text is text**: the first lines of the file, tabs expanded and control
//!   characters dropped, because an escape sequence in a log must not repaint
//!   the window.

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use lazyos_ui::widgets::{put, put_trunc};
use lazyos_ui::{BodyCtx, Design, TextStyle, draw_card};
use ratatui::layout::Rect;
use ratatui::widgets::StatefulWidget;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{Resize, StatefulImage};

use super::open::family;
use super::{date, human, mode_string};

/// The size `lazyos-thumbnail` is asked for: the XDG "large" size.
pub const THUMB_SIZE: u32 = 256;
/// The longest the terminal is waited for when asked how to draw pictures.
const QUERY_TIMEOUT: Duration = Duration::from_millis(300);
/// How far a thumbnail may be enlarged to fill the pane.
const MAX_SCALE: u32 = 2;
/// How many previews are kept: one window's worth of moving back and forth.
const KEEP: usize = 96;
/// How much of a text file the excerpt reads.
const TEXT_BYTES: usize = 64 * 1024;
const TEXT_LINES: usize = 200;
/// The longest a thumbnail may take before the pane gives up on it.
const THUMB_TIMEOUT: Duration = Duration::from_secs(8);
/// Thumbnails asked for at once; `lazyos-thumbnail` gates the decoders to
/// four itself, so more would only queue processes behind its locks.
const IN_FLIGHT: usize = 4;

/// What a path previews as - the decision, apart from the drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Picture,
    Text,
    Directory,
    Facts,
}

/// Decide by extension, then by the NUL test: a picture family that
/// `lazyos-thumbnail` can decode is a picture, a directory lists, text is text,
/// and the rest is its facts.
pub fn kind_for(path: &Path) -> Kind {
    let Ok(meta) = fs::metadata(path) else {
        return Kind::Facts;
    };
    if meta.is_dir() {
        return Kind::Directory;
    }
    use std::os::unix::fs::PermissionsExt;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let family = family(&name, meta.permissions().mode() & 0o111 != 0);
    if family.thumbnail() {
        return Kind::Picture;
    }
    if meta.is_file() && super::open::is_text(path) {
        return Kind::Text;
    }
    Kind::Facts
}

/// The thumbnail cache file for a path, as rofi and `lazyos-find` name it.
pub fn thumbnail_path(cache: &Path, path: &Path) -> PathBuf {
    let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    use std::os::unix::ffi::OsStrExt;
    let name = super::md5::hex(real.as_os_str().as_bytes());
    cache.join(format!("{name}.png"))
}

/// How pictures are drawn, decided once, before the first frame.
pub enum Pictures {
    Ready(Box<Picker>),
    /// No pictures (`LAZYOS_PICTURES=off`): the pane shows the facts.
    Off,
}

impl Pictures {
    /// `LAZYOS_PICTURES=off|halfblocks` decides without asking; otherwise
    /// the terminal is asked ([`query_terminal`]).
    pub fn from_env(value: Option<&str>) -> Self {
        match value {
            Some("off") => Pictures::Off,
            Some("halfblocks") => Pictures::Ready(Box::new(Picker::halfblocks())),
            _ => Pictures::Ready(Box::new(query_terminal(QUERY_TIMEOUT))),
        }
    }
}

/// What a terminal said about itself: the cell size in pixels (`CSI 16 t`)
/// and whether its device attributes (`CSI c`) list sixel (attribute 4).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Answer {
    pub cell: Option<(u16, u16)>,
    pub sixel: bool,
    /// The device attributes arrived: the terminal has said all it will.
    pub complete: bool,
}

/// Read the two answers out of what the terminal sent back:
/// `ESC [ 6 ; height ; width t` and `ESC [ ? 62 ; 4 ; ... c`.
pub fn parse_answer(bytes: &[u8]) -> Answer {
    let text = String::from_utf8_lossy(bytes);
    let mut answer = Answer::default();
    for part in text.split('\x1b').filter_map(|p| p.strip_prefix('[')) {
        if let Some(body) = part.strip_prefix("6;")
            && let Some(end) = body.find('t')
        {
            let mut numbers = body[..end].split(';').filter_map(|n| n.parse::<u16>().ok());
            if let (Some(height), Some(width)) = (numbers.next(), numbers.next())
                && height > 0
                && width > 0
            {
                answer.cell = Some((width, height));
            }
        } else if let Some(body) = part.strip_prefix('?')
            && let Some(end) = body.find('c')
        {
            answer.complete = true;
            answer.sixel = body[..end].split(';').any(|attribute| attribute == "4");
        }
    }
    answer
}

/// Ask the terminal how to draw pictures, on this thread, reading stdin with
/// a deadline. `ratatui_image`'s own query reads stdin in a thread of its own,
/// and when the terminal does not answer in time that thread outlives the
/// query and swallows the next key: measured in a pty, the Escape that should
/// have closed the window was read by it, and the window then waited for ever
/// for a key it had been sent. Here nothing outlives the call; a terminal that
/// stays silent costs `timeout` once, before the first frame, and gets half
/// blocks. foot answers both in a few milliseconds and gets sixel.
pub fn query_terminal(timeout: Duration) -> Picker {
    let answer = parse_answer(&ask(b"\x1b[16t\x1b[c", timeout));
    let font = answer
        .cell
        .map(|(width, height)| ratatui_image::FontSize::new(width, height));
    #[allow(deprecated)]
    let mut picker = match font {
        Some(font) => Picker::from_fontsize(font),
        None => Picker::halfblocks(),
    };
    if answer.sixel && font.is_some() {
        picker.set_protocol_type(ratatui_image::picker::ProtocolType::Sixel);
    } else {
        picker.set_protocol_type(ratatui_image::picker::ProtocolType::Halfblocks);
    }
    picker
}

/// Write `question` to the terminal and collect its answer until the device
/// attributes arrive or the deadline passes.
fn ask(question: &[u8], timeout: Duration) -> Vec<u8> {
    use std::io::{IsTerminal, Write};
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Vec::new();
    }
    let mut out = std::io::stdout();
    if out.write_all(question).and_then(|()| out.flush()).is_err() {
        return Vec::new();
    }
    let deadline = Instant::now() + timeout;
    let mut answer = Vec::new();
    while !parse_answer(&answer).complete {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let mut poll = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one pollfd that lives across the call.
        let ready = unsafe { libc::poll(&mut poll, 1, left.as_millis().max(1) as i32) };
        if ready <= 0 {
            break;
        }
        let mut chunk = [0u8; 256];
        // SAFETY: reads into the stack buffer, at most its length.
        let read = unsafe { libc::read(0, chunk.as_mut_ptr().cast(), chunk.len()) };
        if read <= 0 {
            break;
        }
        answer.extend_from_slice(&chunk[..read as usize]);
    }
    answer
}

enum Content {
    /// A thumbnail, ready to draw, with the facts under it.
    Picture {
        protocol: Box<StatefulProtocol>,
        /// The picture's own size, in cells, for the most it may be enlarged.
        cells: (u16, u16),
        facts: Vec<String>,
    },
    /// The thumbnail is on its way (or was refused); the facts meanwhile.
    Waiting {
        facts: Vec<String>,
        note: String,
    },
    Text {
        lines: Vec<String>,
    },
    Lines {
        lines: Vec<String>,
    },
}

/// The previews this window has built, and the thumbnails it is waiting for.
pub struct Previewer {
    cache_dir: PathBuf,
    thumbnailer: PathBuf,
    pictures: Pictures,
    built: HashMap<PathBuf, Content>,
    order: VecDeque<PathBuf>,
    asked: HashMap<PathBuf, Instant>,
    sent: Sender<(PathBuf, bool)>,
    done: Receiver<(PathBuf, bool)>,
}

impl Previewer {
    pub fn new(cache_dir: PathBuf, thumbnailer: PathBuf, pictures: Pictures) -> Self {
        let (sent, done) = channel();
        Self {
            cache_dir,
            thumbnailer,
            pictures,
            built: HashMap::new(),
            order: VecDeque::new(),
            asked: HashMap::new(),
            sent,
            done,
        }
    }

    /// The session's: `~/.cache/thumbnails/normal`, the shared generator,
    /// and `LAZYOS_PICTURES`.
    pub fn for_session(home: &Path) -> Self {
        let cache = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".cache"));
        Self::new(
            cache.join("thumbnails").join("large"),
            home.join(".config/bin/lazyos-thumbnail"),
            Pictures::from_env(std::env::var("LAZYOS_PICTURES").ok().as_deref()),
        )
    }

    /// Wake while a thumbnail is on its way; otherwise the window sleeps.
    pub fn needs_tick(&self) -> Option<Duration> {
        if !self.asked.is_empty() {
            Some(Duration::from_millis(100))
        } else {
            None
        }
    }

    /// Collect finished thumbnails.
    /// True when something changed and the pane must be redrawn.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        while let Ok((path, _made)) = self.done.try_recv() {
            self.asked.remove(&path);
            self.built.remove(&path);
            changed = true;
        }
        let stale: Vec<PathBuf> = self
            .asked
            .iter()
            .filter(|(_, since)| since.elapsed() > THUMB_TIMEOUT * 2)
            .map(|(path, _)| path.clone())
            .collect();
        for path in stale {
            self.asked.remove(&path);
        }
        changed
    }

    /// Draw the preview of `path` into `area`.
    pub fn draw(&mut self, ctx: &mut BodyCtx, area: Rect, path: Option<&Path>) {
        let title = path
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "preview".to_string());
        let inner = draw_card(ctx.buf, area, ctx.design, Some(&title), false);
        let Some(path) = path else {
            put(
                ctx.buf,
                inner.x + 1,
                inner.y,
                "nothing selected",
                inner.width.saturating_sub(1),
                ctx.design.style(TextStyle::Dim),
            );
            return;
        };
        if !self.built.contains_key(path) {
            let content = self.build(path);
            self.remember(path.to_path_buf(), content);
        }
        let design = ctx.design;
        let buf = &mut *ctx.buf;
        match self.built.get_mut(path) {
            Some(Content::Picture {
                protocol,
                cells,
                facts,
            }) => {
                let facts_height = (facts.len() as u16 + 1).min(inner.height / 2);
                let room = Rect {
                    height: inner.height.saturating_sub(facts_height),
                    ..inner
                };
                // Centred, and never past twice its own size: `Scale` keeps
                // the aspect and fills this rectangle, so the rectangle is the
                // limit.
                let width = (cells.0 * MAX_SCALE as u16).min(room.width).max(1);
                let height = (cells.1 * MAX_SCALE as u16).min(room.height).max(1);
                let picture = Rect {
                    x: room.x + (room.width - width) / 2,
                    width,
                    height,
                    ..room
                };
                StatefulImage::<StatefulProtocol>::default()
                    .resize(Resize::Scale(Some(image::imageops::FilterType::Triangle)))
                    .render(picture, buf, protocol.as_mut());
                let picture = Rect {
                    height: room.height,
                    ..picture
                };
                draw_lines(
                    buf,
                    design,
                    Rect {
                        y: picture.bottom() + 1,
                        height: facts_height.saturating_sub(1),
                        ..inner
                    },
                    facts,
                    false,
                );
            }
            Some(Content::Waiting { facts, note }) => {
                draw_lines(buf, design, inner, facts, false);
                let y = inner.y + facts.len() as u16 + 1;
                if y < inner.bottom() {
                    put_trunc(
                        buf,
                        inner.x + 1,
                        y,
                        note,
                        inner.width.saturating_sub(2),
                        design.style(TextStyle::Dim),
                    );
                }
            }
            Some(Content::Text { lines }) => draw_lines(buf, design, inner, lines, true),
            Some(Content::Lines { lines }) => draw_lines(buf, design, inner, lines, false),
            None => {}
        }
    }

    fn remember(&mut self, path: PathBuf, content: Content) {
        if self.built.len() >= KEEP
            && let Some(old) = self.order.pop_front()
        {
            self.built.remove(&old);
        }
        self.order.push_back(path.clone());
        self.built.insert(path, content);
    }

    fn build(&mut self, path: &Path) -> Content {
        match kind_for(path) {
            Kind::Picture => self.picture(path),
            Kind::Text => Content::Text {
                lines: excerpt(path),
            },
            Kind::Directory => Content::Lines {
                lines: directory(path),
            },
            Kind::Facts => Content::Lines { lines: facts(path) },
        }
    }

    fn picture(&mut self, path: &Path) -> Content {
        let facts = facts(path);
        if matches!(self.pictures, Pictures::Off) {
            return Content::Lines { lines: facts };
        }
        let thumb = thumbnail_path(&self.cache_dir, path);
        if thumb.is_file() {
            let Pictures::Ready(picker) = &self.pictures else {
                return Content::Lines { lines: facts };
            };
            return match image::ImageReader::open(&thumb)
                .and_then(|r| r.with_guessed_format())
                .map_err(|e| e.to_string())
                .and_then(|r| r.decode().map_err(|e| e.to_string()))
            {
                Ok(image) => {
                    let font = picker.font_size();
                    let cells = (
                        (image.width() / u32::from(font.width.max(1))).max(1) as u16,
                        (image.height() / u32::from(font.height.max(1))).max(1) as u16,
                    );
                    Content::Picture {
                        protocol: Box::new(picker.new_resize_protocol(image)),
                        cells,
                        facts,
                    }
                }
                Err(error) => Content::Waiting {
                    facts,
                    note: format!("the thumbnail cannot be read: {error}"),
                },
            };
        }
        let note = self.ask_thumbnail(path, &thumb);
        Content::Waiting { facts, note }
    }

    /// Start `lazyos-thumbnail` for one file in a thread; the pane is rebuilt
    /// when it ends. Refused (with the reason as the note) when the generator
    /// is missing or four are already running.
    fn ask_thumbnail(&mut self, path: &Path, thumb: &Path) -> String {
        if self.asked.contains_key(path) {
            return "making the thumbnail…".into();
        }
        if !self.thumbnailer.is_file() {
            return "no thumbnail: lazyos-thumbnail is missing".into();
        }
        if self.asked.len() >= IN_FLIGHT {
            return "waiting for the thumbnails before it…".into();
        }
        if let Some(dir) = thumb.parent()
            && fs::create_dir_all(dir).is_err()
        {
            return format!("no thumbnail: {} is not writable", dir.display());
        }
        let child = Command::new(&self.thumbnailer)
            .arg(path)
            .arg(thumb)
            .arg(THUMB_SIZE.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = child else {
            return "no thumbnail: lazyos-thumbnail did not start".into();
        };
        self.asked.insert(path.to_path_buf(), Instant::now());
        let sent = self.sent.clone();
        let path = path.to_path_buf();
        let thumb = thumb.to_path_buf();
        std::thread::spawn(move || {
            let started = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if started.elapsed() > THUMB_TIMEOUT => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(40)),
                    Err(_) => break,
                }
            }
            let _ = sent.send((path, thumb.is_file()));
        });
        "making the thumbnail…".into()
    }
}

/// The lines of a text file worth a glance: lossy UTF-8, tabs as four
/// spaces, control characters dropped.
pub fn excerpt(path: &Path) -> Vec<String> {
    let mut head = Vec::with_capacity(TEXT_BYTES);
    if let Ok(file) = fs::File::open(path) {
        let _ = file.take(TEXT_BYTES as u64).read_to_end(&mut head);
    }
    if head.is_empty() {
        return vec!["empty file".into()];
    }
    String::from_utf8_lossy(&head)
        .lines()
        .take(TEXT_LINES)
        .map(clean)
        .collect()
}

/// One line, safe to put in a cell: no escape, no carriage return, no tab.
pub fn clean(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        match ch {
            '\t' => out.push_str("    "),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn directory(path: &Path) -> Vec<String> {
    let mut dirs = 0;
    let mut files = 0;
    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let is_dir = entry.path().is_dir();
            if is_dir {
                dirs += 1;
            } else {
                files += 1;
            }
            names.push(if is_dir { format!("{name}/") } else { name });
        }
    } else {
        return vec!["cannot be read by this user".into()];
    }
    names.sort_by_key(|n| (!n.ends_with('/'), n.to_lowercase()));
    let mut lines = vec![
        format!(
            "{dirs} folder{} · {files} file{}",
            if dirs == 1 { "" } else { "s" },
            if files == 1 { "" } else { "s" }
        ),
        String::new(),
    ];
    lines.extend(names.into_iter().take(TEXT_LINES).map(|n| clean(&n)));
    lines
}

/// What can be said about a file without opening it.
pub fn facts(path: &Path) -> Vec<String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let link = fs::symlink_metadata(path).ok();
    let Ok(meta) = fs::metadata(path) else {
        let mut lines = vec!["broken link".to_string()];
        if let Ok(target) = fs::read_link(path) {
            lines.push(format!("→ {}", clean(&target.display().to_string())));
        }
        return lines;
    };
    use std::os::unix::fs::PermissionsExt;
    let mode = meta.permissions().mode();
    let family = if meta.is_dir() {
        "folder"
    } else {
        family(&name, mode & 0o111 != 0).label()
    };
    let mut lines = vec![
        format!("{family} · {}", human(meta.len())),
        format!("modified {}", date(&meta)),
        mode_string(mode),
    ];
    if link.is_some_and(|l| l.file_type().is_symlink())
        && let Ok(target) = fs::read_link(path)
    {
        lines.push(format!("→ {}", clean(&target.display().to_string())));
    }
    lines
}

fn draw_lines(
    buf: &mut ratatui::buffer::Buffer,
    design: &Design,
    area: Rect,
    lines: &[String],
    numbered: bool,
) {
    let gutter = if numbered {
        (lines.len().max(1).ilog10() + 2) as u16
    } else {
        0
    };
    for (index, line) in lines.iter().enumerate().take(area.height as usize) {
        let y = area.y + index as u16;
        let mut x = area.x + 1;
        if numbered {
            let number = format!("{:>width$} ", index + 1, width = gutter as usize - 1);
            x = put(buf, x, y, &number, gutter, design.style(TextStyle::Dim));
        }
        put_trunc(
            buf,
            x,
            y,
            line,
            area.right().saturating_sub(x + 1),
            design.style(TextStyle::Body),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lazyos-preview-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn preview_picks_text_for_text() {
        let dir = tree("kinds");
        fs::write(dir.join("notes.txt"), "hello\n").unwrap();
        fs::write(dir.join("README"), "read me\n").unwrap();
        fs::write(dir.join("photo.jpg"), b"\xff\xd8\xff\0").unwrap();
        fs::write(dir.join("clip.mp4"), b"\0\0\0\x18ftyp").unwrap();
        fs::write(dir.join("paper.pdf"), "%PDF-1.4").unwrap();
        fs::write(dir.join("blob.bin"), b"\0\x01\x02").unwrap();
        fs::write(dir.join("mark.svg"), "<svg/>").unwrap();
        assert_eq!(kind_for(&dir.join("notes.txt")), Kind::Text);
        assert_eq!(kind_for(&dir.join("README")), Kind::Text);
        assert_eq!(kind_for(&dir.join("photo.jpg")), Kind::Picture);
        assert_eq!(kind_for(&dir.join("clip.mp4")), Kind::Picture);
        assert_eq!(kind_for(&dir.join("paper.pdf")), Kind::Picture);
        assert_eq!(kind_for(&dir.join("blob.bin")), Kind::Facts);
        // An SVG has no thumbnail decoder, and it is text: its source shows.
        assert_eq!(kind_for(&dir.join("mark.svg")), Kind::Text);
        assert_eq!(kind_for(&dir), Kind::Directory);
        assert_eq!(kind_for(&dir.join("gone")), Kind::Facts);
    }

    #[test]
    fn the_terminal_answer_names_sixel_and_the_cell() {
        // What foot 1.28 answers here: 9x20 cells and sixel in its attributes.
        let foot = parse_answer(b"\x1b[6;20;9t\x1b[?62;4;22c");
        assert_eq!(
            foot,
            Answer {
                cell: Some((9, 20)),
                sixel: true,
                complete: true
            }
        );
        let plain = parse_answer(b"\x1b[?1;2c");
        assert!(plain.complete && !plain.sixel && plain.cell.is_none());
        // Nothing yet, or a key typed in the middle: not complete, no guess.
        assert_eq!(parse_answer(b""), Answer::default());
        assert!(!parse_answer(b"\x1b[6;20;9t").complete);
        assert!(!parse_answer(b"\x1b[?62;14c").sixel, "14 is not 4");
    }

    #[test]
    fn an_escape_in_a_file_never_reaches_the_terminal() {
        assert_eq!(clean("a\tb\x1b[2Jc\r"), "a    b[2Jc");
    }

    #[test]
    fn the_cache_name_is_the_md5_of_the_real_path() {
        let dir = tree("cache");
        let file = dir.join("p.png");
        fs::write(&file, b"x").unwrap();
        let real = fs::canonicalize(&file).unwrap();
        use std::os::unix::ffi::OsStrExt;
        let want = format!(
            "{}.png",
            super::super::md5::hex(real.as_os_str().as_bytes())
        );
        assert_eq!(
            thumbnail_path(Path::new("/c"), &file),
            Path::new("/c").join(want)
        );
    }

    #[test]
    fn a_missing_generator_is_said_not_hidden() {
        let dir = tree("nogen");
        let photo = dir.join("photo.png");
        fs::write(&photo, b"\x89PNG").unwrap();
        let mut previewer = Previewer::new(
            dir.join("cache"),
            dir.join("no-such-thumbnailer"),
            Pictures::from_env(Some("halfblocks")),
        );
        match previewer.build(&photo) {
            Content::Waiting { note, .. } => assert!(note.contains("lazyos-thumbnail")),
            _ => panic!("a picture with no thumbnail waits and says why"),
        }
    }
}
