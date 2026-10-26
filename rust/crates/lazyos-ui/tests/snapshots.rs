//! Golden snapshots: every widget, at the popup grid and at the panel grid.
//!
//! A change that moves a border, drops a column or loses the key bar fails this
//! suite, which is the point: the design system is a promise about layout, and
//! layout is the one thing a unit test cannot read from the code.
//!
//! Regenerate deliberately with `UPDATE_SNAPSHOTS=1 cargo test -p lazyos-ui`
//! and read the diff - a snapshot updated without reading it is a test that
//! agrees with whatever the code does.
//!
//! The text is compared, not the colours: colours are covered on the other side
//! by `tests/colours.rs` (only `design.rs` may name one) and by the design
//! module's own token tests, while a cell's style is asserted by the widget
//! tests that care (selection fill, title accent, danger summary).

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use lazyos_ui::widgets::put;
use lazyos_ui::{
    App, BodyCtx, Card, ColorRole, Confirm, Design, EmptyScreen, FailureScreen, GLYPHS, Glyph,
    Header, Icon, KeyHint, Kind, ListState, LogPane, Row, Screen, StepState, Task, TaskState,
    TaskView, TextStyle, Urgency, draw_badge, draw_card, draw_empty, draw_failure, draw_gauge,
    draw_header, draw_keybar, draw_progress,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

/// The two grids the session has: a 900x600 popup (80x24 at 11 pt) and the
/// 1366x912 panel (152x45).
const SIZES: [(u16, u16); 2] = [(80, 24), (152, 45)];

fn snapshots_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots")
}

fn screen(buf: &Buffer) -> String {
    let mut out = String::with_capacity((buf.area.width as usize + 1) * buf.area.height as usize);
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        // Trailing spaces are noise in a committed file and invisible in a
        // diff, so a row is stored with its right margin trimmed.
        while out.ends_with(' ') {
            out.pop();
        }
        out.push('\n');
    }
    out
}

/// Compare (or, with `UPDATE_SNAPSHOTS=1`, rewrite) one snapshot file.
fn compare(name: &str, rendered: String) {
    let path = snapshots_dir().join(format!("{name}.txt"));
    if std::env::var("UPDATE_SNAPSHOTS").as_deref() == Ok("1") {
        fs::create_dir_all(snapshots_dir()).unwrap();
        fs::write(&path, &rendered).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing snapshot {}: run UPDATE_SNAPSHOTS=1 cargo test -p lazyos-ui",
            path.display()
        )
    });
    assert_eq!(
        rendered,
        expected,
        "snapshot {} changed:\n{}",
        path.display(),
        first_difference(&expected, &rendered)
    );
}

/// A widget that draws into a buffer, at both grids.
fn check(name: &str, mut draw: impl FnMut(&mut Buffer, Rect, &Design)) {
    let mut rendered = String::new();
    for (width, height) in SIZES {
        let design = Design::solarized();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw(frame.buffer_mut(), area, &design);
            })
            .unwrap();
        let _ = writeln!(rendered, "===== {width}x{height} =====");
        rendered.push_str(&screen(terminal.backend().buffer()));
    }
    compare(name, rendered);
}

/// A whole window (the `App` shell owns the frame), at both grids.
fn check_terminal(name: &str, mut render: impl FnMut(&mut Terminal<TestBackend>)) {
    let mut rendered = String::new();
    for (width, height) in SIZES {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        render(&mut terminal);
        let _ = writeln!(rendered, "===== {width}x{height} =====");
        rendered.push_str(&screen(terminal.backend().buffer()));
    }
    compare(name, rendered);
}

/// The first differing line, with both sides, so a failure reads like a diff
/// instead of a 70-line string comparison.
fn first_difference(expected: &str, got: &str) -> String {
    for (number, (want, have)) in expected.lines().zip(got.lines()).enumerate() {
        if want != have {
            return format!("line {}:\n  want: {want:?}\n  got:  {have:?}", number + 1);
        }
    }
    format!(
        "the file has {} lines and the render {}",
        expected.lines().count(),
        got.lines().count()
    )
}

fn body_rows() -> ListState {
    let mut list = ListState::new().with_empty("no file matches");
    list.set_rows(vec![
        Row::new("Files")
            .glyph(Glyph::Folder)
            .detail("rofi-files")
            .value("Alt+Ctrl+F"),
        Row::new("Files (window)")
            .glyph(Glyph::Folder)
            .detail("pcmanfm")
            .value("Alt+Ctrl+Shift+F"),
        Row::new("Capture")
            .glyph(Glyph::Capture)
            .detail("screenshot")
            .value("Alt+Ctrl+C"),
        Row::new("Translate")
            .glyph(Glyph::Globe)
            .detail("selection")
            .value("Alt+Ctrl+T"),
        Row::new("Search inside files")
            .glyph(Glyph::Search)
            .detail("ripgrep")
            .value("Alt+Ctrl+G"),
        Row::new("Download")
            .glyph(Glyph::Download)
            .detail("yt-dlp")
            .value("Alt+Ctrl+U"),
        Row::new("Settings")
            .glyph(Glyph::Gear)
            .detail("radios")
            .value("Alt+Ctrl+O"),
        Row::new("Notifications")
            .glyph(Glyph::Bell)
            .detail("mako history")
            .value("Alt+Ctrl+N"),
        Row::new("Clipboard")
            .glyph(Glyph::Clipboard)
            .detail("cliphist")
            .value("Alt+Ctrl+V"),
        Row::new("Emoji")
            .glyph(Glyph::Smile)
            .detail("1,296 rows")
            .value("Alt+Ctrl+E"),
        Row::new("Camera")
            .glyph(Glyph::Camera)
            .detail("uvcvideo")
            .value("row"),
        Row::new("Power")
            .glyph(Glyph::Power)
            .detail("rofi-power")
            .value("Alt+Escape"),
        // One name the stripped symbol set does not draw: the row shows its
        // token (`bt`) and every other label keeps its column (spec 003).
        Row::new("Bluetooth")
            .glyph(Glyph::Bluetooth)
            .detail("bluez")
            .value("Alt+Ctrl+B"),
    ]);
    list
}

struct ListScreen {
    list: ListState,
}

impl ListScreen {
    fn inner(area: Rect, design: &Design) -> Rect {
        // `draw_card`'s inner area: one border cell and `Space::One` padding
        // (plus one on the tablet) on each side, one border on top and bottom.
        let pad = design.padding(lazyos_ui::Space::One);
        Rect {
            x: area.x + 1 + pad,
            y: area.y + 1,
            width: area.width.saturating_sub(2 * (1 + pad)),
            height: area.height.saturating_sub(2),
        }
    }
}

impl Screen for ListScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        let inner = draw_card(
            ctx.buf,
            ctx.area,
            ctx.design,
            Some("the session menu"),
            true,
        );
        self.list.render(inner, ctx.buf, ctx.design, true);
    }

    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        self.list.hit_row(Self::inner(area, design), x, y, design)
    }
}

#[test]
fn header() {
    check("header", |buf, area, design| {
        let band = Rect {
            height: design.header_height(),
            ..area
        };
        draw_header(
            buf,
            band,
            design,
            &Header::new("LazyOS")
                .subtitle("the session menu")
                .status("13 rows"),
        );
    });
}

#[test]
fn keybar() {
    check("keybar", |buf, area, design| {
        let band = Rect {
            y: area.bottom() - 1,
            height: 1,
            ..area
        };
        draw_keybar(
            buf,
            band,
            design,
            &[
                KeyHint::new("Enter", "open"),
                KeyHint::new("Alt+1", "actions"),
                KeyHint::new("type", "filter"),
                KeyHint::new("Esc", "close"),
            ],
        );
    });
}

#[test]
fn list() {
    check("list", |buf, area, design| {
        let mut list = body_rows();
        list.set_cursor(2);
        list.render(area, buf, design, true);
    });
}

#[test]
fn list_without_focus() {
    check("list-unfocused", |buf, area, design| {
        let mut list = body_rows();
        list.set_cursor(2);
        list.render(area, buf, design, false);
    });
}

#[test]
fn list_filtered() {
    check("list-filtered", |buf, area, design| {
        let mut list = body_rows();
        list.set_filter("window");
        let shown = list.visible_len();
        list.render(area, buf, design, true);
        let note = format!("{shown} of 12");
        let x = area.right().saturating_sub(note.len() as u16 + 1);
        draw_badge(
            buf,
            x,
            area.bottom() - 1,
            design,
            &note,
            ColorRole::SurfaceAlt,
        );
    });
}

#[test]
fn empty_list_says_so() {
    check("list-empty", |buf, area, design| {
        let list = ListState::new().with_empty("nothing here yet");
        list.render(area, buf, design, true);
    });
}

#[test]
fn card() {
    check("card", |buf, area, design| {
        let inner = draw_card(buf, area, design, Some("Free space"), true);
        let list = body_rows();
        list.render(inner, buf, design, true);
    });
}

/// A notification's card drawn inside a window (spec 006). A snapshot holds the
/// text, so this pins the layout: the glyph column, the title on the border, the
/// body and the bar. The frame colours of the three urgencies are asserted cell
/// by cell in `card::tests` (`a_critical_card_is_drawn_in_the_danger_role`).
#[test]
fn notice_cards_at_three_urgencies() {
    check("card-urgency", |buf, area, design| {
        for (row, urgency) in [Urgency::Low, Urgency::Normal, Urgency::Critical]
            .into_iter()
            .enumerate()
        {
            let rect = Rect {
                y: area.y + row as u16 * 5,
                height: 5,
                ..area
            };
            Card::new("Saved")
                .urgency(urgency)
                .kind(Kind::Path {
                    dir: false,
                    name: "shot.png",
                })
                .body("~/Pictures/shot.png")
                .render(rect, buf, design);
        }
    });
}

#[test]
fn notice_card_with_and_without_a_picture() {
    check("card-icon", |buf, area, design| {
        Card::new("Saved")
            .body("with a picture")
            .render(Rect { height: 4, ..area }, buf, design);
        Card::new("Saved").body("text only").no_icon().render(
            Rect {
                y: area.y + 4,
                height: 4,
                ..area
            },
            buf,
            design,
        );
    });
}

#[test]
fn notice_card_with_progress() {
    check("card-progress", |buf, area, design| {
        Card::new("Volume")
            .body("40%")
            .icon(Icon::Glyph(Glyph::Volume))
            .progress(40)
            .render(Rect { height: 4, ..area }, buf, design);
        Card::new("Brightness")
            .urgency(Urgency::Critical)
            .body("full")
            .progress(100)
            .render(
                Rect {
                    y: area.y + 4,
                    height: 4,
                    ..area
                },
                buf,
                design,
            );
    });
}

#[test]
fn notice_card_truncates_a_long_body() {
    check("card-long", |buf, area, design| {
        let body = "a long line of text that is not going to fit in a card of this width \
            and must end in an ellipsis rather than grow the window";
        Card::new("Long").body(body).no_icon().render(
            Rect {
                width: area.width.min(48),
                height: 4,
                ..area
            },
            buf,
            design,
        );
    });
}

#[test]
fn badge_gauge_progress() {
    check("badges", |buf, area, design| {
        let mut y = area.y;
        let mut x = draw_badge(buf, area.x, y, design, "connected", ColorRole::Ok);
        x = draw_badge(buf, x, y, design, "warning", ColorRole::Warn);
        draw_badge(buf, x, y, design, "battery", ColorRole::Danger);
        y += 1;
        draw_gauge(
            buf,
            Rect {
                y,
                height: 1,
                ..area
            },
            design,
            0.42,
            "battery",
        );
        y += 1;
        draw_gauge(
            buf,
            Rect {
                y,
                height: 1,
                ..area
            },
            design,
            0.91,
            "disk",
        );
        y += 1;
        draw_progress(
            buf,
            Rect {
                y,
                height: 1,
                ..area
            },
            design,
            0.63,
            "downloading",
            ColorRole::Accent,
        );
        put(
            buf,
            area.x,
            y + 1,
            "the label carries the meaning, the bar the motion",
            area.width,
            design.style(TextStyle::Dim),
        );
    });
}

#[test]
fn empty_state() {
    check("empty", |buf, area, design| {
        draw_empty(
            buf,
            area,
            design,
            "the session menu",
            "arrives in spec 007 - Esc closes",
        );
    });
}

#[test]
fn failure_card() {
    check("failure", |buf, area, design| {
        draw_failure(
            buf,
            area,
            design,
            "the menu does not know that name",
            "try: lazyos menu",
        );
    });
}

#[test]
fn confirmation_starts_on_cancel() {
    check("confirm", |buf, area, design| {
        let confirm = Confirm::new("Format the card?", "Every byte on it is erased.");
        confirm.render(area, buf, design);
    });
}

#[test]
fn confirmation_on_continue() {
    check("confirm-continue", |buf, area, design| {
        let mut confirm = Confirm::new("Format the card?", "Every byte on it is erased.");
        confirm.handle(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        confirm.render(area, buf, design);
    });
}

#[test]
fn log_pane() {
    check("logpane", |buf, area, design| {
        let mut log = LogPane::new();
        for i in 0..40 {
            log.push(format!(
                "==> step {i:02}: {}",
                ["copying", "syncing", "installing the kernel", "done"][i % 4]
            ));
        }
        log.render(area, buf, design);
    });
}

#[test]
fn log_pane_paused() {
    check("logpane-paused", |buf, area, design| {
        let mut log = LogPane::new();
        for i in 0..40 {
            log.push(format!("==> step {i:02}"));
        }
        let _ = log.handle(
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE),
            area.height,
        );
        log.render(area, buf, design);
    });
}

#[test]
fn the_app_shell_with_the_menu() {
    let mut body = ListScreen { list: body_rows() };
    let mut app = App::new(
        Header::new("LazyOS")
            .subtitle("the session menu")
            .status("13 rows"),
    )
    .design(Design::solarized().with_reduced_motion(true))
    .keybar(vec![
        KeyHint::new("Enter", "open"),
        KeyHint::new("Alt+1", "actions"),
        KeyHint::new("Esc", "close"),
    ])
    .screen(&mut body);
    check_terminal("app-menu", |terminal| {
        terminal.draw(|frame| app.render(frame)).unwrap();
    });
}

#[test]
fn the_app_shell_with_the_failure_card() {
    let mut body = FailureScreen::new("the menu does not know that name", "try: lazyos menu");
    let mut app = App::new(Header::new("LazyOS").subtitle("failed"))
        .design(Design::solarized().with_reduced_motion(true))
        .keybar(vec![KeyHint::new("any key", "close")])
        .screen(&mut body);
    // The failure card is the one window whose key bar says "any key".
    check_terminal("app-failure", |terminal| {
        terminal.draw(|frame| app.render(frame)).unwrap();
    });
}

#[test]
fn the_app_shell_with_the_empty_state() {
    let mut body = EmptyScreen::new("the session menu", "arrives in spec 007");
    let mut app = App::new(Header::new("LazyOS").subtitle("the session menu"))
        .design(Design::solarized().with_reduced_motion(true))
        .keybar(vec![
            KeyHint::new("Esc", "close"),
            KeyHint::new("q", "close"),
        ])
        .screen(&mut body);
    check_terminal("app-empty", |terminal| {
        terminal.draw(|frame| app.render(frame)).unwrap();
    });
}

struct TaskScreen {
    view: TaskView,
}

impl TaskScreen {
    fn new(task: Task) -> Self {
        Self {
            view: TaskView::new(task),
        }
    }
}

impl Screen for TaskScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        self.view.draw(ctx);
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        self.view.keybar()
    }
}

fn task(title: &str, state: TaskState) -> Task {
    let mut task = Task::new(title.to_string().replace(' ', "-"), title, "/tmp");
    task.add_step("prepare");
    task.add_step("copy files");
    task.add_step("finish");
    match state {
        TaskState::Running => {
            task.state = TaskState::Running;
            task.steps[0].state = StepState::Ok;
            task.steps[1].state = StepState::Running;
            task.progress = 0.42;
        }
        TaskState::Done => {
            task.state = TaskState::Done;
            task.steps[0].state = StepState::Ok;
            task.steps[1].state = StepState::Ok;
            task.steps[2].state = StepState::Ok;
            task.progress = 1.0;
            task.result = Some("Copied 12 files".to_string());
        }
        TaskState::Failed => {
            task.state = TaskState::Failed;
            task.steps[0].state = StepState::Ok;
            task.steps[1].state = StepState::Failed;
            task.error_line = Some("ERROR: disk full".to_string());
        }
        TaskState::Stopped => {
            task.state = TaskState::Stopped;
            task.steps[0].state = StepState::Ok;
            task.steps[1].state = StepState::Failed;
            task.result = Some("Stopped".to_string());
        }
        _ => {}
    }
    for i in 0..20 {
        task.log.push(format!("==> line {i:02}"));
    }
    task
}

#[test]
fn task_queued() {
    check_terminal("task-queued", |terminal| {
        let mut screen = TaskScreen::new(task("Copy files", TaskState::Queued));
        let mut app = App::new(Header::new("LazyOS").subtitle("Copy files"))
            .design(Design::solarized().with_reduced_motion(true))
            .screen(&mut screen);
        terminal.draw(|frame| app.render(frame)).unwrap();
    });
}

#[test]
fn task_running() {
    check_terminal("task-running", |terminal| {
        let mut screen = TaskScreen::new(task("Copy files", TaskState::Running));
        let mut app = App::new(Header::new("LazyOS").subtitle("Copy files"))
            .design(Design::solarized().with_reduced_motion(true))
            .screen(&mut screen);
        terminal.draw(|frame| app.render(frame)).unwrap();
    });
}

#[test]
fn task_running_stoppable() {
    check_terminal("task-running-stoppable", |terminal| {
        let mut t = task("Copy files", TaskState::Running);
        t.set_stoppable(true);
        let mut screen = TaskScreen::new(t);
        let mut app = App::new(Header::new("LazyOS").subtitle("Copy files"))
            .design(Design::solarized().with_reduced_motion(true))
            .screen(&mut screen);
        terminal.draw(|frame| app.render(frame)).unwrap();
    });
}

#[test]
fn task_done() {
    check_terminal("task-done", |terminal| {
        let mut screen = TaskScreen::new(task("Copy files", TaskState::Done));
        let mut app = App::new(Header::new("LazyOS").subtitle("Copy files"))
            .design(Design::solarized().with_reduced_motion(true))
            .screen(&mut screen);
        terminal.draw(|frame| app.render(frame)).unwrap();
    });
}

#[test]
fn task_failed() {
    check_terminal("task-failed", |terminal| {
        let mut screen = TaskScreen::new(task("Copy files", TaskState::Failed));
        let mut app = App::new(Header::new("LazyOS").subtitle("Copy files"))
            .design(Design::solarized().with_reduced_motion(true))
            .screen(&mut screen);
        terminal.draw(|frame| app.render(frame)).unwrap();
    });
}

#[test]
fn task_stopped() {
    check_terminal("task-stopped", |terminal| {
        let mut screen = TaskScreen::new(task("Copy files", TaskState::Stopped));
        let mut app = App::new(Header::new("LazyOS").subtitle("Copy files"))
            .design(Design::solarized().with_reduced_motion(true))
            .screen(&mut screen);
        terminal.draw(|frame| app.render(frame)).unwrap();
    });
}

#[test]
fn every_glyph_in_the_table_is_drawn_or_comes_from_a_border() {
    // A glyph in the table that nothing draws is a font check for a character
    // that never appears. The four corners and the vertical rule come from
    // ratatui's rounded border, so they are not in widgets.rs as literals.
    let from_ratatui = ['\u{256d}', '\u{256e}', '\u{256f}', '\u{2570}', '\u{2502}'];
    let source =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/widgets.rs"))
            .unwrap();
    for glyph in GLYPHS {
        if from_ratatui.contains(glyph) {
            continue;
        }
        let escape = format!("\\u{{{:04x}}}", *glyph as u32);
        assert!(
            source.contains(&escape),
            "widgets.rs does not draw {escape}"
        );
    }
}
