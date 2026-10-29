//! Files (spec 009): one directory per screen, or the newest 500 files of
//! the home directory (`--search`), with the preview beside the list.
//!
//! What the rofi list did, kept: directories first and case folded, the
//! parent row on top and the dotfile switch at the bottom, a glyph on every
//! row from the opener table, the size and the day dim beside the name, a
//! typed path that opens when no row matches it, and the directory it was in
//! last time (`~/.local/state/lazyos/files-last` - never `$PWD`, which is
//! wherever the session happened to be). What is new is the window: Enter on a
//! directory stays in it instead of starting the script again, Backspace on an
//! empty filter goes up, and the preview is a pane rather than a 32 px icon.
//!
//! The actions on a row are `Tab` - `Alt+1` too, where it reaches the window:
//! sway binds it to workspace 1 and foot asks for no shortcut inhibitor, so
//! inside the session it never arrives (rofi could only use it because it does
//! inhibit, `global-kb`).

use std::cell::Cell;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, KeyHint, ListAction, ListState, Outcome, Row, Screen,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;

use super::open::family;
use super::preview::Previewer;
use super::{Chosen, day, draw_split, free_space, human, shown, split};

/// The search's pool: 500 rows is every file a person can name from memory,
/// and the bound keeps the one `fd` run short on a big tree.
pub const SEARCH_LIMIT: usize = 500;

/// The directories the walks never enter: a cache, a repository's objects,
/// a node tree and the container store are 330k of the 350k files a home
/// directory here holds.
pub const EXCLUDES: &[&str] = &[".cache", ".git", "node_modules", ".local/share/containers"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Parent(PathBuf),
    Dir(PathBuf),
    File(PathBuf),
    Broken(PathBuf),
    /// The dotfile switch; `true` when they are shown now.
    Hidden(bool),
}

impl Item {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Item::Parent(p) | Item::Dir(p) | Item::File(p) | Item::Broken(p) => Some(p),
            Item::Hidden(_) => None,
        }
    }
}

/// How the window was opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Search,
}

pub struct Setup {
    pub dir: PathBuf,
    pub mode: Mode,
    pub hidden: bool,
    pub home: PathBuf,
    /// `~/.local/state/lazyos/files-last`.
    pub memory: PathBuf,
    /// `PATH`, where `--search` looks for `fd`.
    pub path_var: String,
}

/// Where to start: the directory given, else the one remembered, else home.
/// A remembered directory that is gone is home again, quietly.
pub fn start_dir(given: Option<&Path>, memory: &Path, home: &Path) -> PathBuf {
    if let Some(dir) = given {
        return dir.to_path_buf();
    }
    if let Ok(text) = fs::read_to_string(memory)
        && let Some(line) = text.lines().next()
        && !line.is_empty()
        && Path::new(line).is_dir()
    {
        return PathBuf::from(line);
    }
    home.to_path_buf()
}

/// Write the directory the next window should reopen. Every failure is
/// ignored: a read-only state directory must not stop a file from opening.
pub fn remember(memory: &Path, dir: &Path) {
    if !dir.is_dir() {
        return;
    }
    if let Some(parent) = memory.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(memory, format!("{}\n", dir.display()));
}

/// One directory, the way the list shows it: directories first, then files,
/// each case folded; a link is followed (a link to a directory is a directory)
/// and only a broken one stays a link.
pub fn list_dir(dir: &Path, hidden: bool) -> io::Result<Vec<Item>> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        if !hidden && name.to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        match fs::metadata(&path) {
            Ok(meta) if meta.is_dir() => dirs.push(Item::Dir(path)),
            Ok(_) => files.push(Item::File(path)),
            Err(_) => files.push(Item::Broken(path)),
        }
    }
    let key = |item: &Item| {
        item.path()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    };
    dirs.sort_by_key(key);
    files.sort_by_key(key);
    let mut items = Vec::with_capacity(dirs.len() + files.len() + 2);
    if let Some(parent) = dir.parent() {
        items.push(Item::Parent(parent.to_path_buf()));
    }
    items.extend(dirs);
    items.extend(files);
    items.push(Item::Hidden(hidden));
    Ok(items)
}

/// `fd` or Debian's `fdfind`, whichever `PATH` has.
pub fn fd_program(path: &str) -> Option<&'static str> {
    ["fd", "fdfind"]
        .into_iter()
        .find(|name| super::open::on_path(path, name))
}

/// The newest files under `home`: one bounded `fd` run and one `stat` per
/// match in this process, never a subprocess per row.
pub fn search(home: &Path, hidden: bool, fd: &str) -> io::Result<Vec<Item>> {
    let mut command = Command::new(fd);
    command
        .arg("--type")
        .arg("f")
        .arg("--max-results")
        .arg(SEARCH_LIMIT.to_string())
        .arg("--print0")
        .arg("--absolute-path");
    for exclude in EXCLUDES {
        command.arg("--exclude").arg(exclude);
    }
    if hidden {
        command.arg("--hidden");
    }
    command
        .arg("--search-path")
        .arg(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command.spawn()?;
    let mut out = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout.read_to_end(&mut out)?;
    }
    let _ = child.wait();
    use std::os::unix::ffi::OsStrExt;
    let mut found: Vec<(i64, PathBuf)> = out
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| PathBuf::from(std::ffi::OsStr::from_bytes(p)))
        .filter_map(|path| {
            use std::os::unix::fs::MetadataExt;
            let meta = fs::metadata(&path).ok()?;
            Some((meta.mtime(), path))
        })
        .collect();
    // Newest first: a search is usually for what was just written.
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let mut items: Vec<Item> = found.into_iter().map(|(_, p)| Item::File(p)).collect();
    items.push(Item::Hidden(hidden));
    Ok(items)
}

/// The row a person sees for an item.
pub fn row(item: &Item, mode: &Mode, home: &Path) -> Row {
    match item {
        Item::Parent(_) => Row::new("..")
            .glyph(Glyph::ArrowUp)
            .detail("parent directory"),
        Item::Dir(path) => {
            let name = name_of(path);
            Row::new(format!("{name}/")).glyph(Glyph::Folder)
        }
        Item::Broken(path) => Row::new(format!("{}@", name_of(path)))
            .glyph(Glyph::BrokenLink)
            .detail("broken link"),
        Item::File(path) => {
            let meta = fs::metadata(path).ok();
            use std::os::unix::fs::PermissionsExt;
            let executable = meta
                .as_ref()
                .is_some_and(|m| m.permissions().mode() & 0o111 != 0);
            let label = match mode {
                Mode::Browse => name_of(path),
                Mode::Search => shown(path, home),
            };
            let row = Row::new(super::preview::clean(&label))
                .glyph(family(&name_of(path), executable).glyph());
            match meta {
                Some(meta) => row.value(format!("{} · {}", human(meta.len()), day(&meta))),
                None => row,
            }
        }
        Item::Hidden(shown) => Row::new("hidden files")
            .glyph(Glyph::Hidden)
            .detail(if *shown {
                "shown - Enter hides them"
            } else {
                "hidden - Enter shows them"
            }),
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| super::preview::clean(&n.to_string_lossy()))
        .unwrap_or_else(|| path.display().to_string())
}

pub struct FilesScreen {
    setup: Setup,
    items: Vec<Item>,
    list: ListState,
    previewer: Option<Previewer>,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    pub chosen: Option<Chosen>,
}

impl FilesScreen {
    pub fn new(setup: Setup, previewer: Option<Previewer>) -> Self {
        let keybar = vec![
            KeyHint::new("Enter", "open"),
            KeyHint::new("Tab", "actions"),
            KeyHint::new("Bksp", "up"),
            KeyHint::new("Ctrl+H", "dotfiles"),
            KeyHint::new("Esc", "close"),
        ];
        let mut screen = Self {
            setup,
            items: Vec::new(),
            list: ListState::new().with_empty("nothing matches - Enter opens it as a path"),
            previewer,
            header: Header::new("Files"),
            keybar,
            rows_height: Cell::new(10),
            chosen: None,
        };
        screen.load();
        screen
    }

    /// The state the rows are built from, once per change and never per
    /// frame.
    fn load(&mut self) {
        let (items, status) = match self.setup.mode {
            Mode::Browse => match list_dir(&self.setup.dir, self.setup.hidden) {
                Ok(items) => {
                    let dirs = items.iter().filter(|i| matches!(i, Item::Dir(_))).count();
                    let files = items
                        .iter()
                        .filter(|i| matches!(i, Item::File(_) | Item::Broken(_)))
                        .count();
                    let free = free_space(&self.setup.dir)
                        .map(|f| format!(" · {f} free"))
                        .unwrap_or_default();
                    (
                        items,
                        format!(
                            "{dirs} dir{} · {files} file{}{free}",
                            plural(dirs),
                            plural(files)
                        ),
                    )
                }
                Err(error) => (
                    self.setup
                        .dir
                        .parent()
                        .map(|p| vec![Item::Parent(p.to_path_buf())])
                        .unwrap_or_default(),
                    format!("cannot be read: {error}"),
                ),
            },
            Mode::Search => match fd_program(&self.setup.path_var) {
                None => (
                    Vec::new(),
                    "fd is missing (package fd-find): there is nothing to search with".to_string(),
                ),
                Some(fd) => match search(&self.setup.home, self.setup.hidden, fd) {
                    Ok(items) => {
                        let found = items.len().saturating_sub(1);
                        let status = if found >= SEARCH_LIMIT {
                            format!("the newest {SEARCH_LIMIT} - type to narrow it")
                        } else {
                            format!("{found} file{}", plural(found))
                        };
                        (items, status)
                    }
                    Err(error) => (Vec::new(), format!("fd did not run: {error}")),
                },
            },
        };
        let title = match self.setup.mode {
            Mode::Browse => shown(&self.setup.dir, &self.setup.home),
            Mode::Search => "Search".to_string(),
        };
        self.header = Header::new("Files").subtitle(title).status(status.clone());
        if items.is_empty() {
            self.list = ListState::new().with_empty(status);
        }
        let rows = items
            .iter()
            .map(|item| row(item, &self.setup.mode, &self.setup.home))
            .collect();
        self.items = items;
        self.list.set_filter("");
        self.list.set_rows(rows);
        // The parent row is first, so the cursor starts on the first entry.
        if matches!(self.items.first(), Some(Item::Parent(_))) && self.items.len() > 2 {
            self.list.set_cursor(1);
        }
    }

    fn enter_dir(&mut self, dir: PathBuf) {
        self.setup.dir = dir;
        self.setup.mode = Mode::Browse;
        remember(&self.setup.memory, &self.setup.dir);
        self.load();
    }

    fn selected(&self) -> Option<&Item> {
        self.list.selected_index().and_then(|i| self.items.get(i))
    }

    fn activate(&mut self, index: usize) -> Outcome {
        let Some(item) = self.items.get(index).cloned() else {
            return Outcome::Continue;
        };
        match item {
            Item::Parent(dir) | Item::Dir(dir) => {
                self.enter_dir(dir);
                Outcome::Consumed
            }
            Item::Hidden(shown) => {
                self.setup.hidden = !shown;
                self.load();
                Outcome::Consumed
            }
            Item::File(path) | Item::Broken(path) => self.open(path),
        }
    }

    fn open(&mut self, path: PathBuf) -> Outcome {
        // The next window starts in the folder this file lives in.
        if let Some(parent) = path.parent() {
            remember(&self.setup.memory, parent);
        }
        self.chosen = Some(Chosen::Open(path));
        Outcome::Done(lazyos_ui::app::exit::OK)
    }

    /// A typed path, when no row matches it: `~/x`, an absolute path, or a
    /// name relative to the directory on screen.
    fn typed_path(&mut self) -> Outcome {
        let text = self.list.filter().trim().to_string();
        if text.is_empty() {
            return Outcome::Continue;
        }
        let path = if text == "~" {
            self.setup.home.clone()
        } else if let Some(rest) = text.strip_prefix("~/") {
            self.setup.home.join(rest)
        } else if text.starts_with('/') {
            PathBuf::from(&text)
        } else {
            self.setup.dir.join(&text)
        };
        if path.is_dir() {
            self.enter_dir(path);
            Outcome::Consumed
        } else if path.is_file() {
            self.open(path)
        } else {
            self.header = self.header.clone().status(format!("no such path: {text}"));
            Outcome::Consumed
        }
    }

    fn actions(&mut self) -> Outcome {
        match self.selected().cloned() {
            Some(Item::Dir(path) | Item::File(path) | Item::Broken(path)) => {
                self.chosen = Some(Chosen::Actions(path));
                Outcome::Done(lazyos_ui::app::exit::OK)
            }
            _ => {
                self.header = self
                    .header
                    .clone()
                    .status("this row has nothing to act on".to_string());
                Outcome::Consumed
            }
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

impl Screen for FilesScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        let selected = self.selected().and_then(Item::path).map(Path::to_path_buf);
        let title = match self.setup.mode {
            Mode::Browse => "Files",
            Mode::Search => "Search",
        };
        draw_split(
            ctx,
            title,
            "type to filter, or a path - Enter opens it",
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
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Tab => return self.actions(),
            KeyCode::Char('1') if alt => return self.actions(),
            KeyCode::Char('h') if control => {
                self.setup.hidden = !self.setup.hidden;
                self.load();
                return Outcome::Consumed;
            }
            KeyCode::Backspace if self.list.filter().is_empty() => {
                if self.setup.mode == Mode::Browse
                    && let Some(parent) = self.setup.dir.parent().map(Path::to_path_buf)
                {
                    self.enter_dir(parent);
                }
                return Outcome::Consumed;
            }
            KeyCode::Enter if self.list.visible_len() == 0 => return self.typed_path(),
            KeyCode::Enter
                if self.list.filter().contains('/') || self.list.filter().starts_with('~') =>
            {
                // A typed path wins over a row it happens to match by name.
                let text = self.list.filter().to_string();
                if Path::new(&text).is_absolute() || text.starts_with('~') {
                    return self.typed_path();
                }
            }
            _ => {}
        }
        let typed = matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace) && !control && !alt;
        if typed {
            self.list.type_filter(key);
            return Outcome::Continue;
        }
        match self.list.handle(key, self.rows_height.get(), design) {
            ListAction::Activated(index) => self.activate(index),
            _ => Outcome::Continue,
        }
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        match row {
            Some(visible) => {
                self.list.set_cursor(visible);
                match self.list.selected_index() {
                    Some(index) => self.activate(index),
                    None => Outcome::Continue,
                }
            }
            None => Outcome::Continue,
        }
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

    fn needs_tick(&self) -> Option<std::time::Duration> {
        self.previewer.as_ref().and_then(Previewer::needs_tick)
    }

    fn tick(&mut self) -> Outcome {
        if let Some(previewer) = self.previewer.as_mut() {
            previewer.tick();
        }
        Outcome::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lazyos-browse-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn directories_first_case_folded_and_the_switch_last() {
        let dir = tree("order");
        fs::create_dir(dir.join("zeta")).unwrap();
        fs::create_dir(dir.join("Alpha")).unwrap();
        fs::write(dir.join("b.txt"), "b").unwrap();
        fs::write(dir.join("A.txt"), "a").unwrap();
        fs::write(dir.join(".secret"), "s").unwrap();
        std::os::unix::fs::symlink(dir.join("nowhere"), dir.join("gone")).unwrap();
        let items = list_dir(&dir, false).unwrap();
        let names: Vec<String> = items
            .iter()
            .map(|i| match i {
                Item::Parent(_) => "..".into(),
                Item::Hidden(_) => "hidden".into(),
                other => other
                    .path()
                    .unwrap()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into(),
            })
            .collect();
        assert_eq!(
            names,
            ["..", "Alpha", "zeta", "A.txt", "b.txt", "gone", "hidden"]
        );
        assert!(matches!(items[5], Item::Broken(_)));
        let shown = list_dir(&dir, true).unwrap();
        assert!(
            shown
                .iter()
                .any(|i| i.path().is_some_and(|p| p.ends_with(".secret")))
        );
    }

    #[test]
    fn remembered_directory_fallback() {
        let dir = tree("memory");
        let home = dir.join("home");
        fs::create_dir(&home).unwrap();
        let memory = dir.join("state/lazyos/files-last");
        // Nothing remembered: home, never the current directory.
        assert_eq!(start_dir(None, &memory, &home), home);
        let docs = home.join("Docs");
        fs::create_dir(&docs).unwrap();
        remember(&memory, &docs);
        assert_eq!(start_dir(None, &memory, &home), docs);
        // A remembered directory that is gone is home again.
        fs::remove_dir(&docs).unwrap();
        assert_eq!(start_dir(None, &memory, &home), home);
        // A given directory wins.
        assert_eq!(start_dir(Some(&dir), &memory, &home), dir);
    }

    #[test]
    fn a_row_says_what_the_table_says() {
        let dir = tree("rows");
        fs::write(dir.join("photo.JPG"), b"x").unwrap();
        fs::create_dir(dir.join("src")).unwrap();
        let home = dir.clone();
        let photo = row(&Item::File(dir.join("photo.JPG")), &Mode::Browse, &home);
        assert_eq!(photo.glyph.as_deref(), Some(Glyph::Image.cell()));
        assert!(photo.value.as_deref().unwrap().starts_with("1 B · "));
        let folder = row(&Item::Dir(dir.join("src")), &Mode::Browse, &home);
        assert_eq!(folder.label, "src/");
        let searched = row(&Item::File(dir.join("photo.JPG")), &Mode::Search, &home);
        assert_eq!(searched.label, "~/photo.JPG");
    }
}
