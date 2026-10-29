//! The framework's design system: tokens, widgets, and the window shell.
//!
//! Spec 001 shipped the frame (a terminal, a palette, two states). Spec 002
//! makes it a system every later surface is built from:
//!
//! - [`design`] is the vocabulary - `ColorRole`, `Space`, `TextStyle`, `Border`
//!   and `Density` - so a widget asks for a role, never for a hex value;
//! - [`widgets`] is the set: `Row`/`ListState`, `Header`, the key bar, `Card`,
//!   badge, gauge, progress, `LogPane`, `Confirm`, the empty state and the
//!   failure card;
//! - [`app`] is the shell: a header band, the screen's body and the key bar, one
//!   window shape for every command, with the keys, the mouse and a bounded
//!   opening animation handled once.
//!
//! [`glyphs`] is the alphabet a row's picture comes from: a Nerd Font codepoint
//! per name, filtered against the font that has to draw it (spec 003), so a
//! name the stripped symbol package does not carry draws text instead of an
//! empty box.
//!
//! A command implements [`app::Screen`] and draws into [`app::BodyCtx`] - it
//! never sees a `ratatui::Frame`, which is what keeps the three bands identical
//! everywhere. `scripts/check-system.sh` greps `lazyos-cli` for a `Frame` to
//! prove it.
//!
//! The measured rules behind the shape are in `docs/optimization.md` §13: a
//! frame costs 154 µs empty and 445 µs when it rebuilds 40 formatted rows, so
//! the loop redraws on events only (a 10 fps tick only while an animation runs)
//! and widgets copy cached cells instead of formatting text per frame
//! (`tests/no_alloc.rs` watches that).

pub mod app;
pub mod card;
pub mod design;
pub mod glyphs;
pub mod task;
pub mod widgets;

pub use app::{App, BodyCtx, EmptyScreen, FailureScreen, MOTION_MS, Outcome, Screen};
pub use card::{Card, Icon, Kind, Timeout, Urgency};
pub use design::{Border, ColorRole, Density, Design, GLYPHS, Space, TextStyle};
pub use glyphs::Glyph;
pub use task::{Step, StepState, Task, TaskState, TaskView};
pub use widgets::{
    Confirm, ConfirmAction, Header, Input, InputAction, KeyHint, ListAction, ListState, LogPane,
    Row, Tier, centered, draw_badge, draw_card, draw_card_with, draw_empty, draw_failure,
    draw_gauge, draw_header, draw_keybar, draw_progress,
};

/// A buffer as text, one line per row, for tests that want to read the screen
/// instead of indexing cells.
#[doc(hidden)]
pub fn screen(buf: &ratatui::buffer::Buffer) -> String {
    let mut out = String::with_capacity((buf.area.width as usize + 1) * buf.area.height as usize);
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    fn draw(app: &mut App<'_>, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        screen(terminal.backend().buffer())
    }

    #[test]
    fn the_empty_state_names_the_title_the_hint_and_the_way_out() {
        let mut body = EmptyScreen::new("the session menu", "arrives in spec 007");
        let mut app = App::new(Header::new("LazyOS").subtitle("the session menu"))
            .design(Design::solarized().with_reduced_motion(true))
            .keybar(vec![KeyHint::new("Esc", "close")])
            .screen(&mut body);
        let text = draw(&mut app, 80, 24);
        assert!(text.contains("LazyOS"), "the header carries the brand");
        assert!(text.contains("the session menu"));
        assert!(text.contains("arrives in spec 007"));
        assert!(
            text.contains('╭'),
            "rounded corners are the design language"
        );
        assert!(text.contains("[Esc]"), "the way out is on the key bar");
    }

    #[test]
    fn the_failure_card_shows_the_summary_and_the_detail() {
        let mut body = FailureScreen::new("the menu does not know that name", "try: lazyos menu");
        let mut app = App::new(Header::new("LazyOS"))
            .design(Design::solarized().with_reduced_motion(true))
            .keybar(vec![KeyHint::new("any key", "close")])
            .screen(&mut body);
        let text = draw(&mut app, 80, 24);
        assert!(text.contains("the menu does not know"));
        assert!(text.contains("try: lazyos menu"));
        assert!(text.contains("press any key"));
    }

    #[test]
    fn the_window_survives_the_panels_grid_and_a_tiny_terminal() {
        for (w, h) in [(152, 45), (80, 24), (24, 8), (12, 4)] {
            let mut body = FailureScreen::new("failed", "because");
            let mut app = App::new(Header::new("LazyOS"))
                .design(Design::solarized().with_reduced_motion(true))
                .screen(&mut body);
            let text = draw(&mut app, w, h);
            assert_eq!(text.lines().count(), h as usize, "{w}x{h}");
        }
    }

    #[test]
    fn centered_never_leaves_the_area() {
        let area = Rect::new(0, 0, 152, 45);
        let rect = widgets::centered(area, 76, 11);
        assert!(rect.x + rect.width <= area.width);
        assert!(rect.y + rect.height <= area.height);
        assert_eq!(rect.width, 115, "76 % of 152");
        let tiny = widgets::centered(Rect::new(0, 0, 10, 3), 76, 11);
        assert_eq!(tiny.width, 10);
        assert_eq!(tiny.height, 3);
    }

    #[test]
    fn the_backdrop_paints_every_cell() {
        let design = Design::solarized();
        let mut terminal = Terminal::new(TestBackend::new(40, 6)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                frame.render_widget(
                    ratatui::widgets::Block::default().style(
                        ratatui::style::Style::default()
                            .bg(design.color(ColorRole::Bg))
                            .fg(design.color(ColorRole::Text)),
                    ),
                    area,
                );
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        for y in 0..6 {
            for x in 0..40 {
                assert_eq!(buf.cell((x, y)).unwrap().bg, design.color(ColorRole::Bg));
            }
        }
    }
}
