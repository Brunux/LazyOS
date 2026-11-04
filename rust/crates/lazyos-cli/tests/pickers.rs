//! The system pickers (spec 010) drawn the way a person sees them: one card,
//! one note line, rows with a glyph, and a confirm whose first answer is
//! Cancel.
//!
//! The golden snapshots (`tests/snapshots/picker-*`, at 80x24 and 152x45,
//! `UPDATE_SNAPSHOTS=1` to regenerate) are what makes a moved border or a
//! dropped glyph visible in review; the driven flows next to them are what
//! keeps the destructive answer one step away from a stray Enter.

use std::fs;
use std::path::Path;

use lazyos_cli::system::disk::Disk;
use lazyos_cli::system::format;
use lazyos_cli::system::pick::Picker;
use lazyos_ui::{App, Design, Header, Outcome, Screen};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

const SIZES: [(u16, u16); 2] = [(80, 24), (152, 45)];

/// Two sticks: one whose model is a real one, and a card, because the two
/// device-name shapes (`sda` and `mmcblk1`) both have to draw.
fn disks() -> Vec<Disk> {
    vec![
        Disk {
            name: "sda".into(),
            size: "29.5G".into(),
            model: "USB DISK 3.0".into(),
            tran: Some("usb".into()),
        },
        Disk {
            name: "mmcblk1".into(),
            size: "58.2G".into(),
            model: "SD64G".into(),
            tran: Some("mmc".into()),
        },
    ]
}

fn design() -> Design {
    Design::solarized().with_reduced_motion(true)
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Draw, then send the key, the order the app's loop uses.
fn drive(screen: &mut Picker, keys: &[KeyEvent], width: u16, height: u16) -> Outcome {
    let mut app = App::new(Header::new("LazyOS"))
        .design(design())
        .screen(screen);
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut outcome = Outcome::Continue;
    for key in keys {
        terminal.draw(|frame| app.render(frame)).unwrap();
        outcome = app.handle(Event::Key(*key));
        if let Outcome::Done(_) = outcome {
            break;
        }
    }
    outcome
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

fn snapshots(name: &str, keys: &[KeyEvent], mut make: impl FnMut() -> Picker) {
    for (width, height) in SIZES {
        let mut picker = make();
        drive(&mut picker, keys, width, height);
        snapshot(
            &format!("{name}-{width}x{height}"),
            &render(&mut picker, width, height),
        );
    }
}

#[test]
fn the_format_list_names_the_device_and_what_it_is() {
    // The first question is the list that is the safety: only a disk with
    // nothing mounted reaches it, and each row says what the device is.
    snapshots("picker-format-list", &[], || {
        Picker::new("Format a disk", format::questions(&disks()))
    });
}

#[test]
fn the_format_confirm_carries_the_two_answers_and_cancel_is_selected() {
    // Enter picks the stick, Enter picks exFAT, and the third question is the
    // confirm - drawn with Cancel where the cursor already is.
    snapshots(
        "picker-format-confirm",
        &[press(KeyCode::Enter), press(KeyCode::Enter)],
        || Picker::new("Format a disk", format::questions(&disks())),
    );
}

#[test]
fn a_stray_enter_on_the_confirm_steps_back_and_erases_nothing() {
    let mut picker = Picker::new("Format a disk", format::questions(&disks()));
    // The two choices, then Enter on the confirm - which is Cancel, and takes
    // the person back to the filesystem question with the disk still known.
    let keys = [
        press(KeyCode::Enter),
        press(KeyCode::Enter),
        press(KeyCode::Enter),
    ];
    assert_eq!(drive(&mut picker, &keys, 80, 24), Outcome::Continue);
    assert_eq!(picker.answers(), &["/dev/sda".to_string()]);
    let rendered = render(&mut picker, 80, 24);
    assert!(
        rendered.contains("/dev/sda · everything on it is erased"),
        "cancel did not step back to the filesystem question:\n{rendered}"
    );
}

#[test]
fn the_whole_format_flow_answers_disk_filesystem_and_yes() {
    let mut picker = Picker::new("Format a disk", format::questions(&disks()));
    let keys = [
        // The card, not the stick.
        press(KeyCode::Down),
        press(KeyCode::Enter),
        // ext4, the second filesystem row.
        press(KeyCode::Down),
        press(KeyCode::Enter),
        // Right moves off Cancel, then Enter says yes.
        press(KeyCode::Right),
        press(KeyCode::Enter),
    ];
    assert_eq!(drive(&mut picker, &keys, 80, 24), Outcome::Done(0));
    assert_eq!(
        picker.answers(),
        &[
            "/dev/mmcblk1".to_string(),
            "ext4".to_string(),
            "go".to_string()
        ]
    );
}

#[test]
fn escape_at_the_first_question_closes_with_nothing() {
    let mut picker = Picker::new("Format a disk", format::questions(&disks()));
    assert_eq!(
        drive(&mut picker, &[press(KeyCode::Esc)], 80, 24),
        Outcome::Done(1)
    );
    assert!(picker.answers().is_empty());
}

#[test]
fn the_filesystem_question_is_asked_about_the_disk_just_chosen() {
    // The note of the second question carries the first answer, so the person
    // reads `… /dev/sda · everything on it is erased` before choosing.
    let mut picker = Picker::new("Format a disk", format::questions(&disks()));
    drive(
        &mut picker,
        &[press(KeyCode::Down), press(KeyCode::Enter)],
        80,
        24,
    );
    let rendered = render(&mut picker, 80, 24);
    assert!(
        rendered.contains("/dev/mmcblk1 · everything on it is erased"),
        "the second question does not name the disk:\n{rendered}"
    );
}
