//! The shell as a command uses it: keys, clicks and a filter, through `App`.
//!
//! This is the integration test spec 002 asks for (Down, Down, Enter selects the
//! third row and returns its key), plus the mouse half of the same question at
//! both densities - a hit region that disagrees with the drawn row is a menu
//! that opens the row above the one that was clicked, and nobody notices until
//! the wrong file opens.

use lazyos_ui::{
    App, BodyCtx, Density, Design, Header, KeyHint, ListAction, ListState, Outcome, Row, Screen,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn click(column: u16, row: u16) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

/// A menu of keys, shaped like the root menu will be: a list, a filter and a
/// choice that ends the window with the chosen key as its result.
struct Menu {
    list: ListState,
    keys: Vec<&'static str>,
    /// The body's height as last drawn: `Screen::key` has no area, and a list
    /// needs one to page. Remembering it is the pattern every screen uses.
    height: u16,
}

impl Menu {
    fn new() -> Self {
        let keys = vec!["files", "capture", "settings", "power"];
        let mut list = ListState::new();
        list.set_rows(
            keys.iter()
                .map(|key| Row::new(*key).detail("a menu").value("Enter"))
                .collect(),
        );
        Self {
            list,
            keys,
            height: 10,
        }
    }

    fn chosen(&self) -> Option<&'static str> {
        self.list.selected_index().map(|index| self.keys[index])
    }
}

impl Screen for Menu {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        self.height = ctx.area.height;
        self.list.render(ctx.area, ctx.buf, ctx.design, true);
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        match self.list.handle(key, self.height, design) {
            ListAction::Activated(_) => Outcome::Done(0),
            _ => {
                // Anything printable is a filter, which is how a person narrows
                // a list instead of scrolling it.
                self.list.type_filter(key);
                Outcome::Continue
            }
        }
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        match row {
            Some(index) => {
                self.list.set_cursor(index);
                Outcome::Done(0)
            }
            None => Outcome::Continue,
        }
    }

    fn hit(&self, area: ratatui::layout::Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        self.list.hit_row(area, x, y, design)
    }
}

fn shell<'a>(menu: &'a mut Menu, design: Design) -> App<'a> {
    App::new(Header::new("LazyOS").subtitle("the session menu"))
        .design(design.with_reduced_motion(true))
        .keybar(vec![
            KeyHint::new("Enter", "open"),
            KeyHint::new("Esc", "close"),
        ])
        .screen(menu)
}

#[test]
fn down_down_enter_selects_the_third_row() {
    let mut menu = Menu::new();
    let mut app = shell(&mut menu, Design::solarized());
    assert_eq!(app.handle(key(KeyCode::Down)), Outcome::Continue);
    assert_eq!(app.handle(key(KeyCode::Down)), Outcome::Continue);
    assert_eq!(app.handle(key(KeyCode::Enter)), Outcome::Done(0));
    assert_eq!(menu.chosen(), Some("settings"));
}

#[test]
fn a_click_chooses_the_row_it_was_drawn_on() {
    for density in [Density::Normal, Density::Tablet] {
        let mut menu = Menu::new();
        let mut app = shell(&mut menu, Design::solarized().with_density(density));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        // The shell records the body area while drawing, which is what a click
        // is measured against.
        terminal.draw(|frame| app.render(frame)).unwrap();
        let header = app.design_ref().header_height();
        let stride = app.design_ref().row_stride();
        // The third row (index 2), clicked on its own line.
        let y = header + 2 * stride;
        assert_eq!(app.handle(click(10, y)), Outcome::Done(0), "{density:?}");
        assert_eq!(menu.chosen(), Some("settings"), "{density:?}");
    }
}

#[test]
fn a_click_on_the_blank_line_between_touch_rows_chooses_nothing() {
    // In tablet mode a row is two lines tall, and the second line is blank. A
    // hit test that divides without checking the remainder would open the row
    // above the finger - which is the wrong file, silently.
    let mut menu = Menu::new();
    let mut app = shell(&mut menu, Design::solarized().with_density(Density::Tablet));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let blank = app.design_ref().header_height() + 2 * 2 + 1;
    assert_eq!(app.handle(click(10, blank)), Outcome::Continue);
    assert_ne!(menu.chosen(), Some("settings"));
}

#[test]
fn a_click_outside_the_rows_does_nothing() {
    let mut menu = Menu::new();
    let mut app = shell(&mut menu, Design::solarized());
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    // The last line of the window is the key bar, which is not a row.
    assert_eq!(app.handle(click(10, 23)), Outcome::Continue);
    assert_eq!(menu.chosen(), Some("files"), "the selection did not move");
}

#[test]
fn typing_filters_and_enter_opens_the_match() {
    let mut menu = Menu::new();
    let mut app = shell(&mut menu, Design::solarized());
    for ch in "pow".chars() {
        assert_eq!(app.handle(key(KeyCode::Char(ch))), Outcome::Continue);
    }
    assert_eq!(app.handle(key(KeyCode::Enter)), Outcome::Done(0));
    // The borrow of the menu ends with the last use of the shell, which is how
    // the assertions below can read it.
    assert_eq!(menu.list.visible_len(), 1);
    assert_eq!(menu.chosen(), Some("power"));
}

#[test]
fn escape_is_the_shells_and_says_nothing_happened() {
    let mut menu = Menu::new();
    let mut app = shell(&mut menu, Design::solarized());
    assert_eq!(app.handle(key(KeyCode::Esc)), Outcome::Done(1));
}

/// A body with a sub-mode, the way the root menu has its power list: Escape
/// steps back once, and only the next Escape is the shell's.
struct Nested {
    inner: bool,
}

impl Screen for Nested {
    fn draw(&mut self, _ctx: &mut BodyCtx) {}

    fn key(&mut self, key: KeyEvent, _design: &Design) -> Outcome {
        if key.code == KeyCode::Esc && self.inner {
            self.inner = false;
            return Outcome::Consumed;
        }
        Outcome::Continue
    }
}

#[test]
fn a_consumed_escape_steps_back_instead_of_closing() {
    let mut nested = Nested { inner: true };
    let mut app = App::new(Header::new("LazyOS"))
        .design(Design::solarized().with_reduced_motion(true))
        .screen(&mut nested);
    assert_eq!(app.handle(key(KeyCode::Esc)), Outcome::Continue);
    assert_eq!(app.handle(key(KeyCode::Esc)), Outcome::Done(1));
}

#[test]
fn a_release_event_is_not_a_press() {
    // Terminals that report both kinds would otherwise act twice per key.
    let mut menu = Menu::new();
    let mut app = shell(&mut menu, Design::solarized());
    let release = Event::Key(KeyEvent {
        code: KeyCode::Down,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Release,
        state: ratatui::crossterm::event::KeyEventState::NONE,
    });
    assert_eq!(app.handle(release), Outcome::Continue);
    assert_eq!(menu.list.cursor(), 0);
}
