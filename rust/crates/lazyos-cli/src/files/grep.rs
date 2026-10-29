//! Grep (spec 009): where did I write this.
//!
//! One `rg` run over the home directory with the old limits - three matches
//! per file, 200 columns, the walks' four excludes, the first 400 rows - and
//! the matching **text** on the row, because the text is what tells two
//! matches apart. Enter opens the match **at its line**, in this window: the
//! file's lines with the match highlighted and in view, scrolled with the
//! usual keys. That is what `bat | less +N` could not do - less cannot seek a
//! pipe, so the old window opened at the top with the match below the screen.
//!
//! A pattern ripgrep cannot parse is answered in ripgrep's own words
//! (`regex parse error: …`), never as "nothing matches"; and Escape while it
//! runs kills the `rg` this window started.

use std::cell::Cell;
use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use lazyos_ui::widgets::{put, put_trunc};
use lazyos_ui::{
    BodyCtx, ColorRole, Design, Glyph, Header, Input, InputAction, KeyHint, ListAction, ListState,
    Outcome, Row, Screen, TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;

use super::browse::EXCLUDES;
use super::open::family;
use super::preview::clean;
use super::{Chosen, shown};

/// Rows kept: a home directory is not a repository, and past this a query is
/// too wide to read anyway.
pub const MAX_ROWS: usize = 400;
/// The most of a file the viewer reads.
const VIEW_BYTES: u64 = 8 * 1024 * 1024;

/// One match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub path: PathBuf,
    /// 1-based, as ripgrep prints it.
    pub line: usize,
    pub text: String,
}

/// `path\0line:text`, what `rg --null --line-number --no-heading` prints: the
/// NUL is what makes a path with a colon one path.
pub fn parse_hit(record: &[u8]) -> Option<Hit> {
    use std::os::unix::ffi::OsStrExt;
    let nul = record.iter().position(|b| *b == 0)?;
    let (path, rest) = (&record[..nul], &record[nul + 1..]);
    let colon = rest.iter().position(|b| *b == b':')?;
    let line: usize = std::str::from_utf8(&rest[..colon]).ok()?.parse().ok()?;
    if path.is_empty() || line == 0 {
        return None;
    }
    let text = String::from_utf8_lossy(&rest[colon + 1..]);
    Some(Hit {
        path: PathBuf::from(std::ffi::OsStr::from_bytes(path)),
        line,
        text: clean(text.trim_end_matches(['\n', '\r']).trim()),
    })
}

/// The arguments, as one place states them.
pub fn rg_args(query: &str, root: &Path) -> Vec<String> {
    let mut args: Vec<String> = [
        "--null",
        "--line-number",
        "--no-heading",
        "--color",
        "never",
        "--max-count",
        "3",
        "--max-columns",
        "200",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for exclude in EXCLUDES {
        args.push("--glob".into());
        args.push(format!("!{exclude}"));
    }
    args.push("--".into());
    args.push(query.into());
    args.push(root.display().to_string());
    args
}

/// What a run ended with.
pub struct Finished {
    pub hits: Vec<Hit>,
    pub total: usize,
    /// ripgrep's exit status: 0 found, 1 nothing, 2 an error.
    pub code: Option<i32>,
    pub error: Vec<String>,
}

struct Running {
    child: Child,
    rx: Receiver<Finished>,
    query: String,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The match's file, open at its line.
pub struct View {
    pub path: PathBuf,
    pub lines: Vec<String>,
    /// 0-based index of the match.
    pub target: usize,
    pub top: Cell<usize>,
    note: Option<String>,
}

impl View {
    pub fn open(path: &Path, line: usize) -> Self {
        let mut bytes = Vec::new();
        let note = match fs::File::open(path) {
            Ok(file) => {
                let _ = file.take(VIEW_BYTES).read_to_end(&mut bytes);
                None
            }
            Err(error) => Some(format!("cannot read it: {error}")),
        };
        let lines: Vec<String> = String::from_utf8_lossy(&bytes).lines().map(clean).collect();
        let target = line.saturating_sub(1).min(lines.len().saturating_sub(1));
        Self {
            path: path.to_path_buf(),
            lines,
            target,
            top: Cell::new(usize::MAX),
            note,
        }
    }

    /// Put the match a third of the way down, the first time the window
    /// knows its height.
    fn settle(&self, height: usize) {
        if self.top.get() == usize::MAX {
            self.top.set(self.target.saturating_sub(height / 3));
        }
    }

    fn scroll(&self, by: isize, height: usize) {
        let max = self.lines.len().saturating_sub(height.max(1));
        let top = self.top.get().min(max) as isize + by;
        self.top.set(top.clamp(0, max as isize) as usize);
    }

    pub fn draw(
        &self,
        buf: &mut ratatui::buffer::Buffer,
        area: Rect,
        design: &Design,
        home: &Path,
    ) {
        let title = format!("{}:{}", shown(&self.path, home), self.target + 1);
        let inner = draw_card(buf, area, design, Some(&title), true);
        if let Some(note) = &self.note {
            put(
                buf,
                inner.x + 1,
                inner.y,
                note,
                inner.width,
                design.style(TextStyle::Danger),
            );
            return;
        }
        let height = inner.height as usize;
        self.settle(height);
        let gutter = (self.lines.len().max(1).ilog10() + 2) as u16;
        for row in 0..height {
            let index = self.top.get() + row;
            let Some(line) = self.lines.get(index) else {
                break;
            };
            let y = inner.y + row as u16;
            let here = index == self.target;
            if here {
                buf.set_style(
                    Rect {
                        x: inner.x,
                        y,
                        width: inner.width,
                        height: 1,
                    },
                    Style::default().bg(design.color(ColorRole::Selection)),
                );
            }
            let mark = if here { "\u{258d}" } else { " " };
            let x = put(buf, inner.x, y, mark, 1, design.style(TextStyle::Title));
            let number = format!("{:>width$} ", index + 1, width = gutter as usize - 1);
            let style = if here {
                design.style(TextStyle::Key)
            } else {
                design.style(TextStyle::Dim)
            };
            let x = put(buf, x, y, &number, gutter, style);
            let text_style = if here {
                design.style(TextStyle::Title)
            } else {
                design.style(TextStyle::Body)
            };
            put_trunc(buf, x, y, line, inner.right().saturating_sub(x), text_style);
        }
    }
}

enum Mode {
    Prompt(Input),
    Running(Running),
    Results,
    Viewing(View),
    Failed(Vec<String>),
}

pub struct GrepScreen {
    root: PathBuf,
    home: PathBuf,
    rg: Option<String>,
    mode: Mode,
    hits: Vec<Hit>,
    list: ListState,
    query: String,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    view_height: Cell<u16>,
    pub chosen: Option<Chosen>,
}

impl GrepScreen {
    /// `rg` is the program to run, or `None` when ripgrep is not installed:
    /// the window then says which package provides it.
    pub fn new(root: PathBuf, home: PathBuf, rg: Option<String>, query: Option<String>) -> Self {
        let mut screen = Self {
            root,
            home,
            rg,
            mode: Mode::Prompt(Input::new()),
            hits: Vec::new(),
            list: ListState::new().with_empty("nothing matches - Backspace widens it"),
            query: String::new(),
            header: Header::new("Search inside files"),
            keybar: Vec::new(),
            rows_height: Cell::new(10),
            view_height: Cell::new(20),
            chosen: None,
        };
        if screen.rg.is_none() {
            screen.mode = Mode::Failed(vec![
                "ripgrep is not installed (package ripgrep)".into(),
                "it is what searches inside the files".into(),
            ]);
        } else if let Some(query) = query.filter(|q| !q.trim().is_empty()) {
            screen.start(query);
        }
        screen.keys();
        screen
    }

    fn keys(&mut self) {
        self.keybar = match self.mode {
            Mode::Prompt(_) => vec![
                KeyHint::new("Enter", "search"),
                KeyHint::new("Esc", "close"),
            ],
            Mode::Running(_) => vec![KeyHint::new("Esc", "stop")],
            Mode::Results => vec![
                KeyHint::new("Enter", "open at the line"),
                KeyHint::new("Ctrl+N", "new search"),
                KeyHint::new("Esc", "close"),
            ],
            Mode::Viewing(_) => vec![
                KeyHint::new("PgUp/PgDn", "scroll"),
                KeyHint::new("o", "open the file"),
                KeyHint::new("Esc", "back to the matches"),
            ],
            Mode::Failed(_) => vec![
                KeyHint::new("Enter", "try again"),
                KeyHint::new("Esc", "close"),
            ],
        };
    }

    pub fn start(&mut self, query: String) {
        let Some(rg) = self.rg.clone() else { return };
        let query = query.trim().to_string();
        let child = Command::new(&rg)
            .args(rg_args(&query, &self.root))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                self.mode = Mode::Failed(vec![format!("rg did not start: {error}")]);
                return;
            }
        };
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut hits = Vec::new();
            let mut total = 0;
            for record in BufReader::new(stdout).split(b'\n') {
                let Ok(record) = record else { break };
                if let Some(hit) = parse_hit(&record) {
                    total += 1;
                    if hits.len() < MAX_ROWS {
                        hits.push(hit);
                    }
                }
            }
            let mut err = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut err);
            let _ = tx.send(Finished {
                hits,
                total,
                code: None,
                error: err
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .take(6)
                    .map(clean)
                    .collect(),
            });
        });
        self.header = Header::new("Search inside files")
            .subtitle(query.clone())
            .status("searching…");
        self.query = query.clone();
        self.mode = Mode::Running(Running { child, rx, query });
        self.keys();
    }

    /// The run ended: its rows, its "nothing", or ripgrep's own error.
    pub fn finish(&mut self, mut finished: Finished, code: Option<i32>) {
        finished.code = code;
        match finished.code {
            Some(2) | None if finished.hits.is_empty() && !finished.error.is_empty() => {
                self.header = self.header.clone().status("ripgrep refused the search");
                self.mode = Mode::Failed(finished.error);
            }
            _ if finished.hits.is_empty() => {
                self.header = self.header.clone().status("no match");
                self.mode = Mode::Failed(vec![format!("Nothing matches \"{}\"", self.query)]);
            }
            _ => {
                let status = if finished.total > finished.hits.len() {
                    format!(
                        "{} matches - the first {} are listed",
                        finished.total,
                        finished.hits.len()
                    )
                } else {
                    format!(
                        "{} match{}",
                        finished.total,
                        if finished.total == 1 { "" } else { "es" }
                    )
                };
                self.header = self.header.clone().status(status);
                let rows = finished.hits.iter().map(|hit| self.row(hit)).collect();
                self.hits = finished.hits;
                self.list = ListState::new().with_empty("nothing matches - Backspace widens it");
                self.list.set_rows(rows);
                self.mode = Mode::Results;
            }
        }
        self.keys();
    }

    fn row(&self, hit: &Hit) -> Row {
        let name = hit
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Row::new(if hit.text.is_empty() {
            "(blank line)".to_string()
        } else {
            hit.text.clone()
        })
        .glyph(family(&name, false).glyph())
        .detail(format!("{}:{}", shown(&hit.path, &self.home), hit.line))
    }

    /// Poll the running search. True when it ended.
    pub fn poll(&mut self) -> bool {
        let Mode::Running(running) = &mut self.mode else {
            return false;
        };
        match running.rx.try_recv() {
            Ok(finished) => {
                let code = running.child.wait().ok().and_then(|s| s.code());
                self.finish(finished, code);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.mode = Mode::Failed(vec!["the search stopped without an answer".into()]);
                self.keys();
                true
            }
        }
    }

    pub fn viewing(&self) -> Option<&View> {
        match &self.mode {
            Mode::Viewing(view) => Some(view),
            _ => None,
        }
    }

    pub fn failure(&self) -> Option<&[String]> {
        match &self.mode {
            Mode::Failed(lines) => Some(lines),
            _ => None,
        }
    }

    pub fn running(&self) -> bool {
        matches!(self.mode, Mode::Running(_))
    }

    fn list_area(area: Rect) -> (Rect, Rect) {
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        (
            Rect { height: 1, ..inner },
            Rect {
                y: inner.y + 2,
                height: inner.height.saturating_sub(2),
                ..inner
            },
        )
    }
}

impl Screen for GrepScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        match &self.mode {
            Mode::Prompt(input) => {
                let card = lazyos_ui::centered(ctx.area, 88, 7);
                let inner = draw_card(ctx.buf, card, ctx.design, Some("Search inside files"), true);
                put_trunc(
                    ctx.buf,
                    inner.x,
                    inner.y,
                    "Every file under your home folder, three matches per file.",
                    inner.width,
                    ctx.design.style(TextStyle::Dim),
                );
                input.render(
                    Rect {
                        y: inner.y + 2,
                        height: 1,
                        ..inner
                    },
                    ctx.buf,
                    ctx.design,
                    "a word, or a regular expression",
                );
            }
            Mode::Running(running) => {
                let card = lazyos_ui::centered(ctx.area, 88, 5);
                let inner = draw_card(ctx.buf, card, ctx.design, Some("Searching"), true);
                put_trunc(
                    ctx.buf,
                    inner.x,
                    inner.y,
                    &format!("Looking for \"{}\"… Esc stops it", running.query),
                    inner.width,
                    ctx.design.style(TextStyle::Body),
                );
            }
            Mode::Results => {
                let (filter, rows) = Self::list_area(ctx.area);
                draw_card(ctx.buf, ctx.area, ctx.design, Some("Matches"), true);
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
                let (text, style) = if self.list.filter().is_empty() {
                    (
                        "type to narrow the matches",
                        ctx.design.style(TextStyle::Dim),
                    )
                } else {
                    (self.list.filter(), ctx.design.style(TextStyle::Body))
                };
                put_trunc(
                    ctx.buf,
                    x,
                    filter.y,
                    text,
                    filter.right().saturating_sub(x),
                    style,
                );
                self.rows_height.set(rows.height);
                self.list.render(rows, ctx.buf, ctx.design, true);
            }
            Mode::Viewing(view) => {
                self.view_height.set(ctx.area.height.saturating_sub(2));
                view.draw(ctx.buf, ctx.area, ctx.design, &self.home);
            }
            Mode::Failed(lines) => {
                let card = lazyos_ui::centered(ctx.area, 88, lines.len() as u16 + 4);
                let inner = draw_card(ctx.buf, card, ctx.design, Some("Search inside files"), true);
                for (i, line) in lines.iter().enumerate() {
                    let style = if i == 0 {
                        ctx.design.style(TextStyle::Danger)
                    } else {
                        ctx.design.style(TextStyle::Body)
                    };
                    put_trunc(
                        ctx.buf,
                        inner.x,
                        inner.y + i as u16,
                        line,
                        inner.width,
                        style,
                    );
                }
            }
        }
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        let outcome = match &mut self.mode {
            Mode::Prompt(input) => match input.handle(key) {
                InputAction::Submitted => {
                    let query = input.text().to_string();
                    self.start(query);
                    Outcome::Consumed
                }
                InputAction::Cancelled => Outcome::Done(lazyos_ui::app::exit::ESCAPE),
                _ => Outcome::Consumed,
            },
            // Escape stops the search: the child is killed when the run is
            // dropped, and the prompt comes back with the words still in it.
            Mode::Running(running) if key.code == KeyCode::Esc => {
                let query = running.query.clone();
                self.mode = Mode::Prompt(Input::with_text(query));
                self.header = Header::new("Search inside files").status("stopped");
                Outcome::Consumed
            }
            Mode::Running(_) => Outcome::Consumed,
            Mode::Results => {
                if control && key.code == KeyCode::Char('n') {
                    self.mode = Mode::Prompt(Input::with_text(self.query.clone()));
                    Outcome::Consumed
                } else if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace) && !control {
                    self.list.type_filter(key);
                    Outcome::Consumed
                } else {
                    match self.list.handle(key, self.rows_height.get(), design) {
                        ListAction::Activated(index) => match self.hits.get(index) {
                            Some(hit) => {
                                self.mode = Mode::Viewing(View::open(&hit.path, hit.line));
                                Outcome::Consumed
                            }
                            None => Outcome::Continue,
                        },
                        _ => Outcome::Continue,
                    }
                }
            }
            Mode::Viewing(view) => {
                let height = self.view_height.get() as usize;
                match key.code {
                    KeyCode::Esc => {
                        self.mode = Mode::Results;
                        Outcome::Consumed
                    }
                    KeyCode::Char('o') => {
                        self.chosen = Some(Chosen::Open(view.path.clone()));
                        Outcome::Done(lazyos_ui::app::exit::OK)
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        view.scroll(-1, height);
                        Outcome::Consumed
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        view.scroll(1, height);
                        Outcome::Consumed
                    }
                    KeyCode::PageUp => {
                        view.scroll(-(height as isize), height);
                        Outcome::Consumed
                    }
                    KeyCode::PageDown | KeyCode::Char(' ') => {
                        view.scroll(height as isize, height);
                        Outcome::Consumed
                    }
                    KeyCode::Home | KeyCode::Char('g') => {
                        view.top.set(0);
                        Outcome::Consumed
                    }
                    KeyCode::End | KeyCode::Char('G') => {
                        view.scroll(isize::MAX / 2, height);
                        Outcome::Consumed
                    }
                    _ => Outcome::Consumed,
                }
            }
            Mode::Failed(_) => match key.code {
                KeyCode::Enter if self.rg.is_some() => {
                    self.mode = Mode::Prompt(Input::with_text(self.query.clone()));
                    Outcome::Consumed
                }
                _ => Outcome::Continue,
            },
        };
        self.keys();
        outcome
    }

    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        match self.mode {
            Mode::Results => self.list.hit_row(Self::list_area(area).1, x, y, design),
            _ => None,
        }
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        if let (Mode::Results, Some(visible)) = (&self.mode, row) {
            self.list.set_cursor(visible);
        }
        Outcome::Continue
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        Some(&self.keybar)
    }

    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }

    fn needs_tick(&self) -> Option<Duration> {
        self.running().then_some(Duration::from_millis(50))
    }

    fn tick(&mut self) -> Outcome {
        self.poll();
        Outcome::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grep_parses_path_line_text() {
        assert_eq!(
            parse_hit(b"/home/me/a:b.txt\x0012:  the needle\there"),
            Some(Hit {
                path: PathBuf::from("/home/me/a:b.txt"),
                line: 12,
                text: "the needle    here".into(),
            })
        );
        assert_eq!(parse_hit(b"/x\x00zero:text"), None);
        assert_eq!(parse_hit(b"/x\x000:text"), None);
        assert_eq!(parse_hit(b"no nul:3:text"), None);
        // An escape in a matched line never reaches the terminal.
        assert_eq!(parse_hit(b"/x\x001:\x1b[2Jboom").unwrap().text, "[2Jboom");
    }

    #[test]
    fn the_query_is_one_argument_after_the_flags() {
        let args = rg_args("--files; rm -rf ~", Path::new("/home/me"));
        let dashdash = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args[dashdash + 1], "--files; rm -rf ~");
        assert_eq!(args.last().unwrap(), "/home/me");
        assert!(args.contains(&"!.cache".to_string()));
        assert!(args.contains(&"--null".to_string()));
    }

    #[test]
    fn the_viewer_puts_the_match_in_view() {
        let dir = std::env::temp_dir().join(format!("lazyos-grep-view-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("long.txt");
        let body: String = (1..=500).map(|n| format!("line {n}\n")).collect();
        fs::write(&file, body).unwrap();
        let view = View::open(&file, 300);
        assert_eq!(view.target, 299);
        view.settle(30);
        let top = view.top.get();
        assert!(
            top <= 299 && 299 < top + 30,
            "the match is on screen (top {top})"
        );
        // A line past the end is the last line, not a panic.
        assert_eq!(View::open(&file, 9999).target, 499);
    }
}
