//! No command draws its own frame.
//!
//! The shell owns the three bands (`lazyos_ui::app`), and a command that reaches
//! for `ratatui::Frame` is the first step back to thirty windows with thirty
//! shapes. `scripts/check-system.sh` runs this same grep on the device, so a
//! hand edit that skips the test suite is still caught; this copy is what the CI
//! runs on every push.

use std::fs;
use std::path::Path;

fn sources(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(sources(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    found
}

#[test]
fn the_binary_does_not_mention_the_frame_type() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = sources(&src);
    assert!(!files.is_empty(), "the walk found no sources");
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for pattern in ["ratatui::Frame", "&mut Frame", "Frame<"] {
            assert!(
                !text.contains(pattern),
                "{} contains `{pattern}`: draw into a BodyCtx from lazyos_ui::app instead",
                file.display()
            );
        }
    }
}

#[test]
fn the_binary_asks_the_shell_for_the_window() {
    // The positive side of the same rule: the menu is an `App` with a `Screen`,
    // so the next command has a shape to copy.
    // The screens live beside the commands that open them (`menu/`, `files/`),
    // so the whole source tree is the place to look.
    let lib = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs")).unwrap();
    assert!(lib.contains("App::new"));
    assert!(lib.contains(".run(terminal)"));
    let screens = sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
        .into_iter()
        .filter(|file| fs::read_to_string(file).is_ok_and(|text| text.contains("impl Screen for")))
        .count();
    assert!(
        screens >= 3,
        "the commands implement Screen ({screens} files do)"
    );
}
