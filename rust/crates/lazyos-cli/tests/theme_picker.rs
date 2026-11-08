//! Tests for the interactive theme picker (`lazyos menu theme`, `rofi-theme`).

use std::process::Command;

use lazyos_cli::command;
use lazyos_cli::system::theme_picker::{ThemePickerScreen, rows};
use lazyos_ui::app::Outcome;
use lazyos_ui::{App, Design, Header};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn theme_picker_screen_initializes_with_all_builtins() {
    let screen = ThemePickerScreen::new();
    assert_eq!(
        screen.entries.len(),
        16,
        "all 16 built-in themes are loaded"
    );
    assert!(!screen.active_slug.is_empty(), "active slug is present");
    assert!(!screen.original_slug.is_empty(), "original slug is present");

    for entry in &screen.entries {
        assert!(!entry.slug.is_empty());
        assert!(!entry.theme.name.is_empty());
        assert!(
            entry.contrast_text >= 4.5,
            "{} text contrast {:.1} meets WCAG floor 4.5:1",
            entry.slug,
            entry.contrast_text
        );
        assert!(
            entry.contrast_accent >= 3.0,
            "{} accent contrast {:.1} meets WCAG floor 3.0:1",
            entry.slug,
            entry.contrast_accent
        );
    }
}

#[test]
fn theme_picker_headless_rows_has_all_entries() {
    let r = rows();
    assert_eq!(r.len(), 16);
    let slugs: Vec<&str> = r.iter().map(|(s, _, _)| *s).collect();
    assert!(slugs.contains(&"solarized"));
    assert!(slugs.contains(&"tokyo-night"));
    assert!(slugs.contains(&"catppuccin"));
    assert!(slugs.contains(&"nord"));
    assert!(slugs.contains(&"gruvbox"));
    assert!(slugs.contains(&"dracula"));
    assert!(slugs.contains(&"one-dark"));
}

#[test]
fn theme_picker_resolves_from_command_and_symlink() {
    let direct = command::resolve("lazyos", &["menu".into(), "theme".into()]).unwrap();
    assert_eq!(direct.spec.path, "menu theme");
    assert_eq!(direct.spec.shape, command::Shape::Task);

    let via_symlink = command::resolve("rofi-theme", &[]).unwrap();
    assert_eq!(via_symlink.spec.path, "menu theme");
}

#[test]
fn theme_picker_rendering_and_key_handling() {
    use ratatui::crossterm::event::Event;

    let mut screen = ThemePickerScreen::new();
    let design = Design::solarized().with_reduced_motion(true);

    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let mut app = App::new(Header::new("LazyOS"))
        .design(design)
        .screen(&mut screen);

    // Initial draw
    terminal.draw(|frame| app.render(frame)).unwrap();

    // Move cursor down with j
    let key_j = press(KeyCode::Char('j'));
    let out = app.handle(Event::Key(key_j));
    assert_eq!(out, Outcome::Continue);

    // Filter with typing
    let key_n = press(KeyCode::Char('n'));
    let _ = app.handle(Event::Key(key_n));
    let key_o = press(KeyCode::Char('o'));
    let _ = app.handle(Event::Key(key_o));

    // Redraw with filter
    terminal.draw(|frame| app.render(frame)).unwrap();

    // Escape exits with ESCAPE
    let key_esc = press(KeyCode::Esc);
    let out_esc = app.handle(Event::Key(key_esc));
    assert_eq!(out_esc, Outcome::Done(lazyos_ui::app::exit::ESCAPE));
}

#[test]
fn theme_picker_cli_query_modes() {
    let exe = env!("CARGO_BIN_EXE_lazyos");

    // --list
    let output = Command::new(exe)
        .args(["menu", "theme", "--list"])
        .output()
        .expect("lazyos menu theme --list");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("solarized"));
    assert!(stdout.contains("tokyo-night"));

    // --current
    let output = Command::new(exe)
        .args(["menu", "theme", "--current"])
        .output()
        .expect("lazyos menu theme --current");
    assert!(output.status.success());

    // --rows
    let output = Command::new(exe)
        .args(["menu", "theme", "--rows"])
        .output()
        .expect("lazyos menu theme --rows");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("solarized\tSolarized Dark"));
}

#[test]
fn theme_picker_wallpaper_preview_and_cycle() {
    use ratatui::crossterm::event::Event;

    let mut screen = ThemePickerScreen::new();
    let design = Design::solarized().with_reduced_motion(true);

    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    let mut app = App::new(Header::new("LazyOS"))
        .design(design)
        .screen(&mut screen);

    // Initial draw renders wallpaper preview section
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    let content = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(content.contains("Desktop & Wallpaper Simulation"));
    assert!(content.contains("Wallpaper:"));

    // Cycle wallpaper with 'w'
    let key_w = press(KeyCode::Char('w'));
    let out = app.handle(Event::Key(key_w));
    assert_eq!(out, Outcome::Continue);

    // Redraw after cycle
    terminal.draw(|frame| app.render(frame)).unwrap();
}
