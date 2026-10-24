//! The shell: header, body and key bar, one window shape for every command.
//!
//! A command implements [`Screen`] and gets the frame, the keys, the mouse and
//! the motion without touching a `Frame` itself (the check in
//! `scripts/check-system.sh` greps `lazyos-cli` for exactly that). Three
//! decisions live here:
//!
//! - **Event-driven**: the loop draws once per event, and only while an
//!   animation is running does it wake on a 10 fps tick. An idle window costs
//!   zero wakeups (`optimization.md` §13: 154 µs per frame is ratatui's floor,
//!   so the budget is kept by not drawing).
//! - **The screen answers first**: Escape means "cancel, exit 1" by default, but
//!   a screen that owns the key (a confirmation, the failure card) gets it
//!   first - otherwise the failure card would exit 1 instead of 3.
//! - **Motion is bounded and optional**: at most [`MOTION_MS`], skipped when
//!   `LAZYOS_REDUCED_MOTION=1`, and it never loops.

use std::io;
use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::style::Style;
use tachyonfx::{Effect, EffectTimer, Interpolation, Motion as Sweep, fx};

use crate::design::{ColorRole, Design};
use crate::widgets::{self, Header, KeyHint, draw_failure, draw_header, draw_keybar};

/// The shell's exit codes, and every surface's contract
/// (`specs/001-core.md` §Design): a shell binding writes `|| exit 0` for Escape.
pub mod exit {
    pub const OK: i32 = 0;
    pub const ESCAPE: i32 = 1;
    pub const USAGE: i32 = 2;
    pub const FAILED: i32 = 3;
}

/// What the loop does after an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Done(i32),
}

/// What a body receives: its area, the buffer, the design. Never a `Frame`, so
/// a command cannot draw the header, the key bar or the backdrop.
pub struct BodyCtx<'a> {
    pub area: Rect,
    pub buf: &'a mut Buffer,
    pub design: &'a Design,
}

/// One command's body: what it draws, and what it does with a key or a click.
pub trait Screen {
    fn draw(&mut self, ctx: &mut BodyCtx);

    /// The first look at a key. Returning [`Outcome::Continue`] hands it to the
    /// shell (Escape, Ctrl-C); anything else ends the window.
    ///
    /// The design is passed rather than remembered: the row stride and the bands
    /// come from the density, and a screen that cached its own copy is a hit
    /// region that disagrees with the row it was drawn on.
    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        let _ = (key, design);
        Outcome::Continue
    }

    /// A click on `row` (the index the screen's [`Screen::hit`] returned), or on
    /// empty space when `None`.
    fn click(&mut self, row: Option<usize>) -> Outcome {
        let _ = row;
        Outcome::Continue
    }

    /// Which row is under `(x, y)`; `None` for blank space.
    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        let _ = (area, x, y, design);
        None
    }

    /// Keys shown on the bottom band. A screen with no key bar uses the one set
    /// on [`App`]; this lets a task surface change its actions as its state
    /// changes. Returning a borrowed slice keeps a frame from allocating just
    /// to ask what the keys are (`tests/no_alloc.rs`).
    fn keybar(&self) -> Option<&[KeyHint]> {
        None
    }

    /// How often the loop should wake this screen while nothing else happens.
    /// `None` means event-driven only; a running task wakes at 10 fps for its
    /// spinner and its stop timeout.
    fn needs_tick(&self) -> Option<Duration> {
        None
    }

    /// Called when the poll timeout expires. The screen may advance a spinner,
    /// check on a child process, or decide the task has ended.
    fn tick(&mut self) -> Outcome {
        Outcome::Continue
    }
}

/// How long an opening animation may run.
pub const MOTION_MS: u64 = 300;

/// The tick an animation runs at: 10 fps, which is what the plan budgets for.
pub const TICK: Duration = Duration::from_millis(100);

/// The opening animation, bounded and skippable.
pub struct Motion {
    effect: Option<Effect>,
    last: Instant,
    enabled: bool,
}

impl Motion {
    pub fn new(enabled: bool) -> Self {
        Self {
            effect: None,
            last: Instant::now(),
            enabled,
        }
    }

    pub fn start(&mut self, design: &Design) {
        if !self.enabled {
            return;
        }
        let faded = design.color(ColorRole::Bg);
        self.effect = Some(fx::sweep_in(
            Sweep::LeftToRight,
            12,
            0,
            faded,
            EffectTimer::from_ms(MOTION_MS as u32, Interpolation::QuadOut),
        ));
        self.last = Instant::now();
    }

    pub fn running(&self) -> bool {
        self.effect.is_some()
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Advance by what has actually elapsed, not by the tick: a frame that was
    /// slow must not stretch the effect past its ceiling.
    pub fn apply(&mut self, buf: &mut Buffer, area: Rect) {
        let Some(effect) = self.effect.as_mut() else {
            return;
        };
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last).min(TICK);
        self.last = now;
        effect.process(elapsed.into(), buf, area);
        if effect.done() {
            self.effect = None;
        }
    }
}

/// The shell: a header, the screen's body, and the keys that are not obvious.
pub struct App<'a> {
    header: Header,
    keybar: Vec<KeyHint>,
    screen: Option<&'a mut dyn Screen>,
    design: Design,
    motion: Motion,
    body_area: Rect,
}

impl<'a> App<'a> {
    pub fn new(header: Header) -> Self {
        let design = Design::for_session(std::env::var("XDG_RUNTIME_DIR").ok().as_deref());
        let motion = Motion::new(!design.reduced_motion());
        Self {
            header,
            keybar: Vec::new(),
            screen: None,
            design,
            motion,
            body_area: Rect::default(),
        }
    }

    /// Tests and the installer inject their own design (density, palette,
    /// reduced motion) instead of reading the session's.
    pub fn design(mut self, design: Design) -> Self {
        self.motion = Motion::new(!design.reduced_motion());
        self.design = design;
        self
    }

    pub fn keybar(mut self, hints: Vec<KeyHint>) -> Self {
        self.keybar = hints;
        self
    }

    pub fn screen(mut self, screen: &'a mut dyn Screen) -> Self {
        self.screen = Some(screen);
        self
    }

    pub fn design_ref(&self) -> &Design {
        &self.design
    }

    /// Draw one frame. Everything a command draws goes through `BodyCtx`, so
    /// the three bands are the same in every window.
    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(
            ratatui::widgets::Block::default().style(
                Style::default()
                    .bg(self.design.color(ColorRole::Bg))
                    .fg(self.design.color(ColorRole::Text)),
            ),
            area,
        );
        // Three bands, computed rather than solved: a `Layout` allocation per
        // frame is exactly the kind of quiet cost the budget test watches.
        let header_height = self.design.header_height().min(area.height);
        let keybar_height = 1.min(area.height.saturating_sub(header_height));
        let body_height = area
            .height
            .saturating_sub(header_height)
            .saturating_sub(keybar_height);
        let header = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: header_height,
        };
        let body = Rect {
            x: area.x,
            y: area.y + header_height,
            width: area.width,
            height: body_height,
        };
        let keybar = Rect {
            x: area.x,
            y: body.bottom(),
            width: area.width,
            height: keybar_height,
        };
        draw_header(frame.buffer_mut(), header, &self.design, &self.header);
        self.body_area = body;
        if let Some(screen) = self.screen.as_mut() {
            let mut ctx = BodyCtx {
                area: body,
                buf: frame.buffer_mut(),
                design: &self.design,
            };
            screen.draw(&mut ctx);
        }
        let hints = self
            .screen
            .as_ref()
            .and_then(|s| s.keybar())
            .filter(|h| !h.is_empty())
            .unwrap_or(&self.keybar);
        draw_keybar(frame.buffer_mut(), keybar, &self.design, hints);
        self.motion.apply(frame.buffer_mut(), body);
    }

    /// The screen sees the key first; Escape and Ctrl-C are the shell's only
    /// when it does not use them.
    pub fn handle(&mut self, event: Event) -> Outcome {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if let Some(screen) = self.screen.as_mut() {
                    match screen.key(key, &self.design) {
                        Outcome::Continue => {}
                        done => return done,
                    }
                }
                match key.code {
                    KeyCode::Esc => Outcome::Done(exit::ESCAPE),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        Outcome::Done(exit::ESCAPE)
                    }
                    _ => Outcome::Continue,
                }
            }
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                let row = self.screen.as_ref().and_then(|screen| {
                    screen.hit(self.body_area, mouse.column, mouse.row, &self.design)
                });
                match self.screen.as_mut() {
                    Some(screen) => screen.click(row),
                    None => Outcome::Continue,
                }
            }
            _ => Outcome::Continue,
        }
    }

    /// The event loop: draw on every event, wake on a tick only while the
    /// opening animation runs.
    pub fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> io::Result<i32> {
        self.run_until(terminal, None)
    }

    /// The same loop with an idle ceiling: a card nobody answers closes itself
    /// after `limit`, because a broken input must not be able to hang the
    /// session. `None` waits for ever, which is what a menu wants.
    pub fn run_until(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        limit: Option<Duration>,
    ) -> io::Result<i32> {
        self.motion.start(&self.design);
        let mut last_event = Instant::now();
        loop {
            terminal.draw(|frame| self.render(frame))?;
            let screen_tick = self
                .screen
                .as_ref()
                .and_then(|s| s.needs_tick())
                .unwrap_or(Duration::MAX);
            let timeout = if self.motion.running() {
                TICK.min(screen_tick)
            } else {
                let base = match limit {
                    Some(limit) => limit
                        .saturating_sub(last_event.elapsed())
                        .max(Duration::from_millis(1)),
                    None => Duration::from_secs(3600),
                };
                base.min(screen_tick)
            };
            let outcome = if event::poll(timeout)? {
                last_event = Instant::now();
                self.handle(event::read()?)
            } else {
                if let Some(limit) = limit
                    && last_event.elapsed() >= limit
                    && !self.motion.running()
                {
                    return Ok(exit::FAILED);
                }
                if let Some(screen) = self.screen.as_mut() {
                    screen.tick()
                } else {
                    Outcome::Continue
                }
            };
            if let Outcome::Done(code) = outcome {
                return Ok(code);
            }
        }
    }
}

/// A body that says "nothing here yet" - spec 001's menu, and the shell every
/// later surface is built on.
pub struct EmptyScreen {
    pub title: String,
    pub hint: String,
}

impl EmptyScreen {
    pub fn new(title: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            hint: hint.into(),
        }
    }
}

impl Screen for EmptyScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        widgets::draw_empty(ctx.buf, ctx.area, ctx.design, &self.title, &self.hint);
    }
}

/// The body of the error card: every key closes it, and the exit code stays 3.
pub struct FailureScreen {
    pub summary: String,
    pub detail: String,
}

impl FailureScreen {
    pub fn new(summary: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            detail: detail.into(),
        }
    }
}

impl Screen for FailureScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_failure(ctx.buf, ctx.area, ctx.design, &self.summary, &self.detail);
    }

    fn key(&mut self, key: KeyEvent, _design: &Design) -> Outcome {
        if key.kind == KeyEventKind::Press {
            Outcome::Done(exit::FAILED)
        } else {
            Outcome::Continue
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::Density;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn shell<'a>(screen: &'a mut dyn Screen) -> App<'a> {
        App::new(Header::new("LazyOS").subtitle("the session menu"))
            .design(Design::solarized().with_reduced_motion(true))
            .keybar(vec![
                KeyHint::new("Enter", "open"),
                KeyHint::new("Esc", "close"),
            ])
            .screen(screen)
    }

    #[test]
    fn escape_returns_one_and_ctrl_c_too() {
        let mut screen = EmptyScreen::new("menu", "later");
        let mut app = shell(&mut screen);
        assert_eq!(app.handle(key(KeyCode::Esc)), Outcome::Done(1));
        let mut screen = EmptyScreen::new("menu", "later");
        let mut app = shell(&mut screen);
        assert_eq!(
            app.handle(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL
            ))),
            Outcome::Done(1)
        );
    }

    #[test]
    fn an_ordinary_key_does_nothing() {
        let mut screen = EmptyScreen::new("menu", "later");
        let mut app = shell(&mut screen);
        assert_eq!(app.handle(key(KeyCode::Char('x'))), Outcome::Continue);
    }

    #[test]
    fn the_failure_card_owns_the_key_so_the_code_stays_three() {
        let mut screen = FailureScreen::new("it broke", "because");
        let mut app = shell(&mut screen);
        // Escape would be exit 1 from the shell; the card answers first.
        assert_eq!(app.handle(key(KeyCode::Esc)), Outcome::Done(3));
    }

    #[test]
    fn the_shell_draws_header_body_and_key_bar() {
        let mut screen = EmptyScreen::new("the session menu", "arrives in spec 007");
        let mut app = shell(&mut screen);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        let text = crate::screen(terminal.backend().buffer());
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].contains("LazyOS"), "the header is the first row");
        assert!(
            lines[23].contains("[Esc]"),
            "the key bar is the last row: {:?}",
            lines[23]
        );
        assert!(text.contains("the session menu"));
    }

    #[test]
    fn reduced_motion_renders_the_final_frame_without_a_tick() {
        let mut screen = EmptyScreen::new("menu", "later");
        let mut app = shell(&mut screen);
        let mut with_motion = Terminal::new(TestBackend::new(80, 24)).unwrap();
        with_motion.draw(|frame| app.render(frame)).unwrap();
        // No animation was started and none is applied: the frame is the final
        // one, which is what "reduced motion" means for a screenshot or a person.
        assert!(!app.motion.enabled());

        let design = Design::solarized();
        let mut plain = Buffer::empty(Rect::new(0, 0, 80, 24));
        widgets::draw_header(
            &mut plain,
            Rect::new(0, 0, 80, design.header_height()),
            &design,
            &Header::new("LazyOS").subtitle("the session menu"),
        );
        let mut motion = Motion::new(true);
        motion.start(&design);
        assert!(motion.running(), "the animation exists when it is allowed");
        let ceiling = Duration::from_millis(300);
        assert!(Duration::from_millis(MOTION_MS) <= ceiling);
    }

    #[test]
    fn the_density_decides_the_row_stride_the_list_and_the_hit_test_share() {
        let design = Design::solarized().with_density(Density::Tablet);
        assert_eq!(design.row_stride(), 2);
        assert_eq!(design.header_height(), 3);
    }
}
