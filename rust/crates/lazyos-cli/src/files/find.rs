//! Find (spec 009): `fd` walks the whole tree under a directory and the
//! window filters it while it walks - the framework is the fuzzy finder now,
//! where `lazyos-find` piped the same walk into fzf.
//!
//! The walk is a child process and a thread: the thread reads the child's
//! NUL-separated paths and hands each one to the window over a **bounded**
//! channel, so a tree of a million files costs memory in `fd`'s pipe and not
//! in this window, and the window takes what has arrived on every tick. A path
//! is never held back to fill a batch: a walk that pauses on a slow disk still
//! shows what it found (the first version did hold them, and a fake `fd` that
//! paused after 301 paths showed 256). The list is the ranked [`ListState`]
//! (exact, prefix, word, substring, subsequence - what fzf's matching felt
//! like), extended in place so new rows never move the row under the cursor. **Escape stops the walk**: dropping
//! the window kills `fd` and waits for it, which is a handle on the process
//! this window started - no `pkill` pattern to guess.

use std::cell::Cell;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::time::{Duration, Instant};

use lazyos_ui::{BodyCtx, Design, Header, KeyHint, ListAction, ListState, Outcome, Row, Screen};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;

use super::browse::EXCLUDES;
use super::open::family;
use super::preview::{Previewer, clean};
use super::{Chosen, draw_split, shown, split};

/// Paths in flight: the walk blocks when the window is this far behind, so a
/// tree of a million files waits in `fd`'s pipe, not in this window.
const QUEUE: usize = 8192;
/// The most paths one tick takes, so a fast walk cannot starve the keys.
const PER_TICK: usize = 20_000;

/// What the walk thread sends.
pub enum Walked {
    Path(PathBuf),
    /// The walk ended (or could not start), with fd's own words on failure.
    Done(Option<String>),
}

/// Cut a NUL-separated stream into paths and hand each one over as it is
/// read. Each path is sent on its own, never held back for a batch: a walk
/// that pauses on a slow disk must still show what it has found. A path split
/// across two reads is one path, an empty record is skipped, and a refused
/// send (the window is gone) stops the reading.
pub fn read_paths(reader: impl BufRead, mut send: impl FnMut(PathBuf) -> bool) {
    use std::os::unix::ffi::OsStrExt;
    for record in reader.split(0) {
        let Ok(record) = record else { break };
        if record.is_empty() {
            continue;
        }
        if !send(PathBuf::from(std::ffi::OsStr::from_bytes(&record))) {
            return;
        }
    }
}

/// The walk this window owns.
struct Walk {
    child: Child,
    rx: Receiver<Walked>,
}

impl Walk {
    fn start(fd: &str, root: &Path) -> std::io::Result<Self> {
        let mut command = Command::new(fd);
        command
            .arg("--type")
            .arg("f")
            .arg("--hidden")
            .arg("--strip-cwd-prefix")
            .arg("--print0");
        for exclude in EXCLUDES {
            command.arg("--exclude").arg(exclude);
        }
        // Relative to the root: `--strip-cwd-prefix` drops fd's `./`, so a
        // row is the path a person types, and the root is joined back on open.
        let mut child = command
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let (tx, rx): (SyncSender<Walked>, Receiver<Walked>) = sync_channel(QUEUE);
        std::thread::spawn(move || {
            let paths = tx.clone();
            read_paths(BufReader::new(stdout), |path| {
                paths.send(Walked::Path(path)).is_ok()
            });
            let mut err = String::new();
            let _ = std::io::Read::read_to_string(&mut BufReader::new(stderr), &mut err);
            let first = err.lines().find(|l| !l.trim().is_empty()).map(clean);
            let _ = tx.send(Walked::Done(first));
        });
        Ok(Self { child, rx })
    }
}

impl Drop for Walk {
    fn drop(&mut self) {
        // The window is going: the walk goes with it, and is reaped.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct FindScreen {
    root: PathBuf,
    paths: Vec<PathBuf>,
    list: ListState,
    walk: Option<Walk>,
    previewer: Option<Previewer>,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    started: Instant,
    /// When the first rows arrived, for the header (and the 100 ms budget).
    first: Option<Duration>,
    pub chosen: Option<Chosen>,
}

impl FindScreen {
    /// Start walking `root` with `fd` (or say why there is nothing to walk
    /// with: a missing package is a row of text, never an empty list).
    pub fn new(
        root: PathBuf,
        home: PathBuf,
        fd: Option<&str>,
        previewer: Option<Previewer>,
    ) -> Self {
        let mut list = ListState::new().with_empty("nothing matches yet").ranked();
        let (walk, status) = match fd {
            None => {
                list = ListState::new()
                    .with_empty("fd is missing (package fd-find): there is nothing to walk with")
                    .ranked();
                (None, "fd is missing".to_string())
            }
            Some(fd) => match Walk::start(fd, &root) {
                Ok(walk) => (Some(walk), "walking…".to_string()),
                Err(error) => {
                    list = ListState::new()
                        .with_empty(format!("fd did not start: {error}"))
                        .ranked();
                    (None, "fd did not start".to_string())
                }
            },
        };
        let header = Header::new("Find")
            .subtitle(shown(&root, &home))
            .status(status);
        Self {
            root,
            paths: Vec::new(),
            list,
            walk,
            previewer,
            header,
            keybar: vec![
                KeyHint::new("Enter", "open"),
                KeyHint::new("Ctrl+Y", "copy path"),
                KeyHint::new("Esc", "stop and close"),
            ],
            rows_height: Cell::new(10),
            started: Instant::now(),
            first: None,
            chosen: None,
        }
    }

    pub fn walking(&self) -> bool {
        self.walk.is_some()
    }

    pub fn found(&self) -> usize {
        self.paths.len()
    }

    /// Take what the walk has sent since the last tick.
    pub fn drain(&mut self) -> bool {
        let Some(walk) = self.walk.as_ref() else {
            return false;
        };
        let mut ended = None;
        let mut batch = Vec::new();
        while batch.len() < PER_TICK {
            match walk.rx.try_recv() {
                Ok(Walked::Path(path)) => batch.push(path),
                Ok(Walked::Done(error)) => {
                    ended = Some(error);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    ended = Some(None);
                    break;
                }
            }
        }
        let mut changed = !batch.is_empty();
        if changed {
            let rows: Vec<Row> = batch.iter().map(|p| row(p)).collect();
            self.paths.extend(batch);
            self.list.extend_rows(rows);
            if self.first.is_none() {
                self.first = Some(self.started.elapsed());
            }
        }
        let count = self.paths.len();
        let status = match &ended {
            None => format!("walking… {count} files"),
            Some(None) => format!("{count} file{}", if count == 1 { "" } else { "s" }),
            Some(Some(error)) => format!("{count} files - fd: {error}"),
        };
        if ended.is_some() {
            self.walk = None;
            if count == 0 {
                self.list = ListState::new().with_empty("no files here").ranked();
            }
            changed = true;
        }
        if changed {
            self.header = self.header.clone().status(status);
        }
        changed
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.list
            .selected_index()
            .and_then(|i| self.paths.get(i))
            .map(|p| self.root.join(p))
    }
}

fn row(path: &Path) -> Row {
    let text = path.to_string_lossy();
    // No `stat` per row: the walk is the cost, and the preview says the rest.
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Row::new(clean(&text)).glyph(family(&name, false).glyph())
}

impl Screen for FindScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        let selected = self.selected_path();
        draw_split(
            ctx,
            "Find",
            "type to narrow it while it walks",
            &self.list,
            &self.rows_height,
            self.previewer.as_mut(),
            selected.as_deref(),
        );
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if control && key.code == KeyCode::Char('y') {
            if let Some(path) = self.selected_path() {
                let copied = lazyos_ui::card::copy_to_clipboard(&path.display().to_string());
                let status = match copied {
                    Ok(()) => "copied the path".to_string(),
                    Err(error) => format!("not copied: {error}"),
                };
                self.header = self.header.clone().status(status);
            }
            return Outcome::Consumed;
        }
        let typed = matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace)
            && !control
            && !key.modifiers.contains(KeyModifiers::ALT);
        if typed {
            self.list.type_filter(key);
            return Outcome::Continue;
        }
        match self.list.handle(key, self.rows_height.get(), design) {
            ListAction::Activated(index) => match self.paths.get(index) {
                Some(path) => {
                    self.chosen = Some(Chosen::Open(self.root.join(path)));
                    Outcome::Done(lazyos_ui::app::exit::OK)
                }
                None => Outcome::Continue,
            },
            _ => Outcome::Continue,
        }
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        if let Some(visible) = row {
            self.list.set_cursor(visible);
        }
        Outcome::Continue
    }

    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        self.list.hit_row(split(area).rows, x, y, design)
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        Some(&self.keybar)
    }

    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }

    fn needs_tick(&self) -> Option<Duration> {
        let walk = self.walk.is_some().then_some(Duration::from_millis(30));
        let preview = self.previewer.as_ref().and_then(Previewer::needs_tick);
        match (walk, preview) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn tick(&mut self) -> Outcome {
        self.drain();
        if let Some(previewer) = self.previewer.as_mut() {
            previewer.tick();
        }
        Outcome::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_chunk_handling() {
        let stream: &[u8] = b"a.txt\0dir/b.rs\0\0c d.md\0last";
        let mut paths = Vec::new();
        read_paths(stream, |path| {
            paths.push(path);
            true
        });
        assert_eq!(
            paths,
            ["a.txt", "dir/b.rs", "c d.md", "last"].map(PathBuf::from)
        );
        // A record split across two reads of the pipe is still one path.
        let split = std::io::BufReader::with_capacity(3, &b"abcdef\0gh\0"[..]);
        let mut paths = Vec::new();
        read_paths(split, |path| {
            paths.push(path);
            true
        });
        assert_eq!(paths, ["abcdef", "gh"].map(PathBuf::from));
    }

    #[test]
    fn a_refused_send_stops_the_reader() {
        let stream: &[u8] = b"1\x002\x003\x004\0";
        let mut calls = 0;
        read_paths(stream, |_| {
            calls += 1;
            false
        });
        assert_eq!(calls, 1, "a window that is gone stops the walk being read");
    }

    #[test]
    fn find_survives_a_directory_that_disappears() {
        // A root that is gone before fd starts: the window says so instead of
        // hanging or panicking, and ends the walk.
        let root = std::env::temp_dir().join(format!("lazyos-find-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut screen = FindScreen::new(root.clone(), root.clone(), Some("/bin/false"), None);
        // Whatever fd did with the root, the walk ends and nothing is listed.
        let deadline = Instant::now() + Duration::from_secs(5);
        while screen.walking() && Instant::now() < deadline {
            screen.drain();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!screen.walking());
        assert_eq!(screen.found(), 0);
    }
}
