//! The three search surfaces and the opener (spec 009), driven the way a
//! person drives them: a fixture tree in a temp dir, a scripted key sequence,
//! and fake programs on `PATH` that write down how they were started.
//!
//! - the golden snapshots (`tests/snapshots/files-*`, `find-*`, `grep-*`, at
//!   80x24 and 152x45, `UPDATE_SNAPSHOTS=1` to regenerate) are the rows, the
//!   preview pane at both widths - a thumbnail drawn in half blocks, a text
//!   excerpt - and the empty and refused states;
//! - `lazyos-view` (a link to the binary) starts the program the table names
//!   for a picture, a text file, a directory and a broken link;
//! - the walk is filtered while it runs and is killed with the window, and a
//!   search that is stopped leaves no `rg` behind.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use lazyos_cli::files::Chosen;
use lazyos_cli::files::browse::{FilesScreen, Mode, Setup};
use lazyos_cli::files::find::FindScreen;
use lazyos_cli::files::grep::GrepScreen;
use lazyos_cli::files::preview::{Pictures, Previewer, thumbnail_path};
use lazyos_ui::{App, Design, Header, Outcome, Screen};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

const SIZES: [(u16, u16); 2] = [(80, 24), (152, 45)];

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lazyos-files-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn script(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// 2026-10-01 12:00 UTC: the same local day in every time zone a build host
/// is likely to have, so a snapshot's dates do not depend on where it ran.
fn noon() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_790_856_000)
}

fn touch(path: &Path, body: &[u8]) {
    fs::write(path, body).unwrap();
    // The mode is the fixture's, not this machine's umask.
    fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(noon())
        .unwrap();
}

/// A home with a folder, a note, a picture whose thumbnail is already in the
/// cache, a video, a dotfile and a broken link.
struct Tree {
    home: PathBuf,
    cache: PathBuf,
}

impl Tree {
    fn new(name: &str) -> Self {
        let root = scratch(name);
        let home = root.join("home");
        let cache = root.join("cache");
        fs::create_dir_all(home.join("Photos")).unwrap();
        fs::create_dir_all(home.join("src")).unwrap();
        fs::create_dir_all(&cache).unwrap();
        touch(
            &home.join("notes.md"),
            b"# Notes\n\n\tindented with a tab\nthe \x1b[2Jescape stays out\nlast line\n",
        );
        touch(&home.join("photo.png"), b"\x89PNG not really");
        touch(&home.join("film.mkv"), b"\x1a\x45\xdf\xa3\0");
        touch(&home.join(".profile"), b"export X=1\n");
        std::os::unix::fs::symlink(home.join("nowhere"), home.join("gone")).unwrap();
        // The thumbnail the cache already holds: a 120x80 gradient, so the
        // half blocks have something to draw that is the same on every machine.
        let picture = image::RgbImage::from_fn(120, 80, |x, y| {
            image::Rgb([(x * 2) as u8, (y * 3) as u8, 0x80])
        });
        picture
            .save(thumbnail_path(&cache, &home.join("photo.png")))
            .unwrap();
        Self { home, cache }
    }

    fn previewer(&self) -> Previewer {
        Previewer::new(
            self.cache.clone(),
            self.home.join("no-thumbnailer"),
            Pictures::from_env(Some("halfblocks")),
        )
    }

    fn setup(&self, dir: &Path, mode: Mode, path_var: &str) -> Setup {
        Setup {
            dir: dir.to_path_buf(),
            mode,
            hidden: false,
            home: self.home.clone(),
            memory: self.home.join(".local/state/lazyos/files-last"),
            path_var: path_var.to_string(),
        }
    }

    fn files(&self) -> FilesScreen {
        FilesScreen::new(
            self.setup(&self.home, Mode::Browse, "/usr/bin:/bin"),
            Some(self.previewer()),
        )
    }
}

fn design() -> Design {
    Design::solarized().with_reduced_motion(true)
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn ctrl(ch: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL))
}

fn typed(text: &str) -> Vec<Event> {
    text.chars().map(|c| press(KeyCode::Char(c))).collect()
}

fn drive(screen: &mut dyn Screen, events: Vec<Event>) -> Outcome {
    let mut app = App::new(Header::new("LazyOS"))
        .design(design())
        .screen(screen);
    // One frame first: a screen learns its rows' height when it is drawn.
    let mut terminal = Terminal::new(TestBackend::new(152, 45)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let mut last = Outcome::Continue;
    for event in events {
        last = app.handle(event);
        terminal.draw(|frame| app.render(frame)).unwrap();
        if matches!(last, Outcome::Done(_)) {
            break;
        }
    }
    last
}

fn render(screen: &mut dyn Screen, width: u16, height: u16) -> String {
    let mut app = App::new(Header::new("LazyOS"))
        .design(design())
        .screen(screen);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    text(terminal.backend().buffer())
}

fn text(buf: &Buffer) -> String {
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        while out.ends_with(' ') {
            out.pop();
        }
        out.push('\n');
    }
    out
}

/// The free space is this machine's, not the frame's: masked before the
/// snapshot compares.
fn mask(rendered: &str) -> String {
    rendered
        .lines()
        .map(|line| match line.find("modified 2026-10-01 ") {
            // The day is the same in every likely zone; the hour is not.
            Some(at) => {
                let start = at + "modified 2026-10-01 ".len();
                format!("{}HH:MM{}", &line[..start], &line[start + 5..])
            }
            None => line.to_string(),
        })
        .map(|line| match (line.find(" · "), line.find(" free")) {
            (Some(_), Some(end)) => {
                let start = line[..end].rfind(" · ").unwrap();
                let masked = format!("{} · ### free{}", &line[..start], &line[end + 5..]);
                // Keep the width, so the rest of the line stays where it was.
                let pad = line.chars().count().saturating_sub(masked.chars().count());
                format!("{masked}{}", " ".repeat(pad))
                    .trim_end()
                    .to_string()
            }
            _ => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn snapshot(name: &str, rendered: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    let path = dir.join(format!("{name}.txt"));
    if std::env::var("UPDATE_SNAPSHOTS").as_deref() == Ok("1") {
        fs::create_dir_all(&dir).unwrap();
        fs::write(&path, rendered).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing snapshot {}: run UPDATE_SNAPSHOTS=1 cargo test -p lazyos-cli",
            path.display()
        )
    });
    assert_eq!(rendered, expected, "snapshot {} changed", path.display());
}

fn snapshots(name: &str, mut make: impl FnMut() -> Box<dyn Screen>) {
    for (width, height) in SIZES {
        let mut screen = make();
        let rendered = mask(&render(screen.as_mut(), width, height));
        snapshot(&format!("{name}-{width}x{height}"), &rendered);
    }
}

#[test]
fn the_list_draws_a_glyph_on_every_row_and_previews_the_text() {
    let tree = Tree::new("snap-text");
    snapshots("files-text", || {
        let mut screen = tree.files();
        // `..`, Photos, src, film.mkv, gone, notes.md: the note is the sixth.
        drive(
            &mut screen,
            vec![
                press(KeyCode::Down),
                press(KeyCode::Down),
                press(KeyCode::Down),
                press(KeyCode::Down),
            ],
        );
        Box::new(screen)
    });
}

#[test]
fn a_picture_previews_its_thumbnail() {
    let tree = Tree::new("snap-picture");
    snapshots("files-picture", || {
        let mut screen = tree.files();
        let mut keys = typed("photo.p");
        keys.insert(0, press(KeyCode::Home));
        drive(&mut screen, keys);
        Box::new(screen)
    });
}

#[test]
fn an_empty_folder_and_a_search_with_no_fd_say_so() {
    let tree = Tree::new("snap-empty");
    let empty = tree.home.join("src");
    snapshots("files-empty", || {
        Box::new(FilesScreen::new(
            tree.setup(&empty, Mode::Browse, "/usr/bin:/bin"),
            Some(tree.previewer()),
        ))
    });
    let nowhere = tree.home.join("no-bin");
    snapshots("files-search-no-fd", || {
        Box::new(FilesScreen::new(
            tree.setup(&tree.home, Mode::Search, &nowhere.display().to_string()),
            Some(tree.previewer()),
        ))
    });
}

#[test]
fn navigating_opening_and_remembering_where_it_was() {
    let tree = Tree::new("keys");
    let mut screen = tree.files();
    // Into Photos (the first row after `..`), back up with Backspace.
    drive(&mut screen, vec![press(KeyCode::Enter)]);
    let memory = fs::read_to_string(tree.home.join(".local/state/lazyos/files-last")).unwrap();
    assert_eq!(
        memory.trim(),
        tree.home.join("Photos").display().to_string()
    );
    drive(&mut screen, vec![press(KeyCode::Backspace)]);
    // Type a name and open it: the window ends on the file, and the next one
    // starts in its folder.
    let mut keys = typed("notes");
    keys.push(press(KeyCode::Enter));
    let outcome = drive(&mut screen, keys);
    assert_eq!(outcome, Outcome::Done(0));
    assert_eq!(
        screen.chosen,
        Some(Chosen::Open(tree.home.join("notes.md")))
    );
    let memory = fs::read_to_string(tree.home.join(".local/state/lazyos/files-last")).unwrap();
    assert_eq!(memory.trim(), tree.home.display().to_string());
}

#[test]
fn a_typed_path_opens_when_no_row_matches_and_tab_asks_for_the_actions() {
    let tree = Tree::new("typed");
    let mut screen = tree.files();
    let mut keys = typed("~/Photos");
    keys.push(press(KeyCode::Enter));
    drive(&mut screen, keys);
    let memory = fs::read_to_string(tree.home.join(".local/state/lazyos/files-last")).unwrap();
    assert!(memory.trim().ends_with("/Photos"), "{memory}");

    let mut screen = tree.files();
    let mut keys = typed("film");
    keys.push(press(KeyCode::Tab));
    assert_eq!(drive(&mut screen, keys), Outcome::Done(0));
    assert_eq!(
        screen.chosen,
        Some(Chosen::Actions(tree.home.join("film.mkv")))
    );
    // Alt+1, where it reaches the window, is the same key.
    let mut screen = tree.files();
    let mut keys = typed("film");
    keys.push(Event::Key(KeyEvent::new(
        KeyCode::Char('1'),
        KeyModifiers::ALT,
    )));
    drive(&mut screen, keys);
    assert_eq!(
        screen.chosen,
        Some(Chosen::Actions(tree.home.join("film.mkv")))
    );
}

#[test]
fn ctrl_h_shows_the_dotfiles() {
    let tree = Tree::new("hidden");
    let mut screen = tree.files();
    let mut keys = vec![ctrl('h')];
    keys.extend(typed("profile"));
    keys.push(press(KeyCode::Enter));
    drive(&mut screen, keys);
    assert_eq!(
        screen.chosen,
        Some(Chosen::Open(tree.home.join(".profile")))
    );
}

/// A fake `fd` that prints `count` paths, then waits, writing its pid first:
/// the walk that is still running when the window closes.
fn fake_fd(dir: &Path, count: usize, then_sleep: u32) -> PathBuf {
    let fd = dir.join("fd");
    script(
        &fd,
        &format!(
            "echo $$ > \"{pid}\"\ni=1\nwhile [ $i -le {count} ]; do printf 'dir/file-%s.txt\\0' $i; i=$((i+1)); done\nprintf 'notes.md\\0'\nexec sleep {then_sleep}\n",
            pid = dir.join("fd.pid").display(),
        ),
    );
    fd
}

/// The pid a fake wrote, once it has finished writing it.
fn read_pid(path: &Path) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(pid) = fs::read_to_string(path).unwrap_or_default().trim().parse() {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "{} was never written",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn alive(pid: i32) -> bool {
    // A zombie is not alive: read the state, not only the pid.
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .map(|stat| !stat.split_whitespace().nth(2).is_some_and(|s| s == "Z"))
        .unwrap_or(false)
}

#[test]
fn typing_narrows_while_the_walk_runs_and_escape_ends_it() {
    let tree = Tree::new("find");
    let bin = tree.home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let fd = fake_fd(&bin, 300, 60);
    let mut screen = FindScreen::new(
        tree.home.clone(),
        tree.home.clone(),
        Some(fd.to_str().unwrap()),
        None,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while screen.found() < 301 && Instant::now() < deadline {
        screen.tick();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(screen.found(), 301);
    assert!(screen.walking(), "fd is still running");
    let mut keys = typed("notes");
    keys.push(press(KeyCode::Enter));
    drive(&mut screen, keys);
    assert_eq!(
        screen.chosen,
        Some(Chosen::Open(tree.home.join("notes.md")))
    );
    let pid = read_pid(&bin.join("fd.pid"));
    assert!(
        alive(pid),
        "the walk was running while the list was filtered"
    );
    drop(screen);
    assert!(!alive(pid), "the window took fd with it");
}

#[test]
fn the_walk_is_a_snapshot_too() {
    let tree = Tree::new("snap-find");
    let bin = tree.home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let fd = fake_fd(&bin, 5, 0);
    snapshots("find-walked", || {
        let mut screen = FindScreen::new(
            tree.home.clone(),
            tree.home.clone(),
            Some(fd.to_str().unwrap()),
            None,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while screen.walking() && Instant::now() < deadline {
            screen.tick();
            std::thread::sleep(Duration::from_millis(10));
        }
        Box::new(screen)
    });
    snapshots("find-no-fd", || {
        Box::new(FindScreen::new(
            tree.home.clone(),
            tree.home.clone(),
            None,
            None,
        ))
    });
}

/// A fake `rg` that answers from a fixture: `-- QUERY ROOT` are its last
/// two words, and a query of `(` is ripgrep's own parse error.
fn fake_rg(dir: &Path, home: &Path) -> PathBuf {
    let rg = dir.join("rg");
    let notes = home.join("notes.md");
    script(
        &rg,
        &format!(
            r#"echo $$ > "{pid}"
for last; do :; done
eval "q=\${{$(($#-1))}}"
case $q in
"(") printf 'regex parse error:\n    (\n    ^\nerror: unclosed group\n' >&2; exit 2 ;;
slow) exec sleep 60 ;;
none) exit 1 ;;
esac
printf '%s\0%s:%s\n' "{notes}" 1 '# Notes' "{notes}" 5 'last line'
"#,
            pid = dir.join("rg.pid").display(),
            notes = notes.display(),
        ),
    );
    rg
}

#[test]
fn a_match_opens_at_its_line_and_a_bad_pattern_is_ripgreps_own_words() {
    let tree = Tree::new("grep");
    let bin = tree.home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let rg = fake_rg(&bin, &tree.home);
    let rg = Some(rg.display().to_string());

    let finish = |screen: &mut GrepScreen| {
        let deadline = Instant::now() + Duration::from_secs(5);
        while screen.running() && Instant::now() < deadline {
            screen.poll();
            std::thread::sleep(Duration::from_millis(10));
        }
    };

    snapshots("grep-matches", || {
        let mut screen = GrepScreen::new(
            tree.home.clone(),
            tree.home.clone(),
            rg.clone(),
            Some("line".into()),
        );
        finish(&mut screen);
        Box::new(screen)
    });
    snapshots("grep-at-the-line", || {
        let mut screen = GrepScreen::new(
            tree.home.clone(),
            tree.home.clone(),
            rg.clone(),
            Some("line".into()),
        );
        finish(&mut screen);
        drive(
            &mut screen,
            vec![press(KeyCode::Down), press(KeyCode::Enter)],
        );
        let view = screen.viewing().expect("the match is open");
        assert_eq!(view.target, 4, "line 5, zero-based");
        Box::new(screen)
    });
    snapshots("grep-bad-pattern", || {
        let mut screen = GrepScreen::new(
            tree.home.clone(),
            tree.home.clone(),
            rg.clone(),
            Some("(".into()),
        );
        finish(&mut screen);
        assert!(screen.failure().unwrap()[0].contains("regex parse error"));
        Box::new(screen)
    });
    snapshots("grep-no-ripgrep", || {
        Box::new(GrepScreen::new(
            tree.home.clone(),
            tree.home.clone(),
            None,
            Some("x".into()),
        ))
    });

    // `o` in the viewer hands the file to the opener; Escape goes back first.
    let mut screen = GrepScreen::new(
        tree.home.clone(),
        tree.home.clone(),
        rg.clone(),
        Some("line".into()),
    );
    finish(&mut screen);
    drive(
        &mut screen,
        vec![press(KeyCode::Enter), press(KeyCode::Esc)],
    );
    assert!(
        screen.viewing().is_none(),
        "Escape goes back to the matches"
    );
    drive(
        &mut screen,
        vec![press(KeyCode::Enter), press(KeyCode::Char('o'))],
    );
    assert_eq!(
        screen.chosen,
        Some(Chosen::Open(tree.home.join("notes.md")))
    );
}

#[test]
fn escape_stops_a_search_that_is_still_running() {
    let tree = Tree::new("grep-stop");
    let bin = tree.home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let rg = fake_rg(&bin, &tree.home);
    let mut screen = GrepScreen::new(
        tree.home.clone(),
        tree.home.clone(),
        Some(rg.display().to_string()),
        Some("slow".into()),
    );
    let pid = read_pid(&bin.join("rg.pid"));
    assert!(screen.running() && alive(pid));
    let outcome = drive(&mut screen, vec![press(KeyCode::Esc)]);
    assert_eq!(
        outcome,
        Outcome::Continue,
        "Escape stops the search, not the window"
    );
    assert!(!screen.running());
    assert!(!alive(pid), "the rg this window started is gone");
}

/// `lazyos-view` (the link) with fake viewers that write their argv down.
#[test]
fn lazyos_view_starts_what_the_table_names() {
    let tree = Tree::new("open");
    let bin = tree.home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let log = tree.home.join("opened");
    for program in ["imv", "foot", "bat", "mpv", "papers", "firefox-esr"] {
        script(
            &bin.join(program),
            &format!("printf '%s\\n' \"$0 $*\" >> \"{}\"\n", log.display()),
        );
    }
    let view = bin.join("lazyos-view");
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_lazyos"), &view).unwrap();
    let run = |arg: &Path| {
        let _ = fs::remove_file(&log);
        // A session: a Wayland display and a `foot`, which is how a folder
        // reaches the file list from a double click.
        let status = Command::new(&view)
            .arg(arg)
            .env("PATH", &bin)
            .env("HOME", &tree.home)
            .env("WAYLAND_DISPLAY", "wayland-test")
            .env_remove("LAZYOS_UI")
            .status()
            .unwrap();
        (status.code(), fs::read_to_string(&log).unwrap_or_default())
    };
    let (code, opened) = run(&tree.home.join("photo.png"));
    assert_eq!(code, Some(0));
    assert!(opened.contains("/imv -- "), "{opened}");
    let (_, opened) = run(&tree.home.join("notes.md"));
    assert!(opened.contains("/foot --app-id=popup"), "{opened}");
    assert!(opened.contains("bat --paging=always"), "{opened}");
    let (_, opened) = run(&tree.home.join("film.mkv"));
    assert!(opened.contains("/mpv --no-config"), "{opened}");
    // A directory opens the file list, which is this binary again: in a
    // `foot` window, on that folder.
    let (code, _) = run(&tree.home.join("Photos"));
    assert_eq!(code, Some(0));
    // The window is started detached, so its record arrives a moment later.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut opened = String::new();
    while Instant::now() < deadline {
        opened = fs::read_to_string(&log).unwrap_or_default();
        if !opened.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(opened.contains("/foot --app-id=lazyos-pick"), "{opened}");
    let folder = format!("menu files -- {}", tree.home.join("Photos").display());
    assert!(opened.contains(&folder), "{opened}");
    // A broken link is refused: a card (no daemon here, so it fails quietly)
    // and exit 3, and nothing is started.
    let (code, opened) = run(&tree.home.join("gone"));
    assert_eq!(code, Some(3));
    assert!(opened.is_empty(), "{opened}");
    // A file:// URI from GIO, with its space encoded.
    touch(&tree.home.join("My Notes.txt"), b"x\n");
    let uri = format!("file://{}/My%20Notes.txt", tree.home.display());
    let (_, opened) = run(Path::new(&uri));
    assert!(opened.contains("My Notes.txt"), "{opened}");
}

/// The spec's budget: a 100k-file tree paints its first result under 100 ms.
/// It needs the real `fd` and makes 100,000 files, so it is run on purpose
/// (`cargo test -p lazyos-cli --test files -- --ignored`), and the number is
/// recorded in the spec.
#[test]
#[ignore]
fn a_hundred_thousand_files_paint_the_first_row_under_100_ms() {
    let path = std::env::var("PATH").unwrap_or_default();
    let Some(fd) = lazyos_cli::files::browse::fd_program(&path) else {
        eprintln!("skipping: no fd on PATH");
        return;
    };
    let root = scratch("hundred-k");
    for d in 0..100 {
        let dir = root.join(format!("d{d:03}"));
        fs::create_dir_all(&dir).unwrap();
        for f in 0..1000 {
            fs::write(dir.join(format!("f{f:04}.txt")), b"").unwrap();
        }
    }
    let started = Instant::now();
    let mut screen = FindScreen::new(root.clone(), root.clone(), Some(fd), None);
    let mut first = None;
    let mut terminal = Terminal::new(TestBackend::new(152, 45)).unwrap();
    while screen.walking() {
        screen.tick();
        if first.is_none() && screen.found() > 0 {
            let mut app = App::new(Header::new("LazyOS"))
                .design(design())
                .screen(&mut screen);
            terminal.draw(|frame| app.render(frame)).unwrap();
            first = Some(started.elapsed());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let total = started.elapsed();
    eprintln!(
        "first row painted after {:?}, {} files in {:?}",
        first.unwrap(),
        screen.found(),
        total
    );
    assert_eq!(screen.found(), 100_000);
    assert!(first.unwrap() < Duration::from_millis(100));
    // A keystroke over the whole tree, ranked: what typing costs once it walked.
    let mut app = App::new(Header::new("LazyOS"))
        .design(design())
        .screen(&mut screen);
    for (i, ch) in "f0999".chars().enumerate() {
        let started = Instant::now();
        app.handle(press(KeyCode::Char(ch)));
        terminal.draw(|frame| app.render(frame)).unwrap();
        eprintln!(
            "keystroke {} ({ch}) filtered and drew in {:?}",
            i + 1,
            started.elapsed()
        );
    }
    let _ = fs::remove_dir_all(&root);
}
