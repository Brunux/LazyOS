//! The binary as a process: the exit codes the shell contract depends on, the
//! host's one-window rule, and the terminal after a panic.
//!
//! The pty tests use util-linux's `script(1)` to give the child a real terminal;
//! without one every UI mode takes the "no terminal" path and would pass while
//! the thing being tested never ran. They skip when `script` is not installed
//! rather than fail, because a minimal container is a missing tool, not a bug.

use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_lazyos")
}

fn version_on_disk() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../VERSION");
    std::fs::read_to_string(path).unwrap().trim().to_string()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lazyos-test-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A `foot` that records its argv and then holds the "window" open for a couple
/// of seconds, so a host that waited on it would be visibly slow.
fn fake_foot(dir: &Path) -> PathBuf {
    let log = dir.join("foot.log");
    let path = dir.join("foot");
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf 'LAZYOS_UI=%s\\n' \"$LAZYOS_UI\" >> '{}'\nsleep 2\n",
        log.display(),
        log.display()
    );
    // Through a scratch name: another test thread can fork while this file is
    // open for writing, the child inherits the descriptor, and the next exec
    // then fails with `ETXTBSY` ("Text file busy"). A rename publishes a file
    // no child holds.
    let scratch = path.with_file_name("foot.new");
    std::fs::write(&scratch, script).unwrap();
    set_executable(&scratch);
    std::fs::rename(&scratch, &path).unwrap();
    log
}

fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

fn wait_for(path: &Path, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

fn have_script() -> bool {
    Command::new("script")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run the binary inside a pty, with `input` typed at it, and return what the
/// terminal received plus the exit status. `configure` sets the environment on
/// `script`, which is what the pty child inherits.
fn in_pty_with(args: &[&str], input: &str, configure: impl FnOnce(&mut Command)) -> (String, i32) {
    // A pty created by `script` whose own stdout is a pipe starts at 0x0, so
    // ratatui would faithfully render nothing. Setting the size first is what
    // makes this the panel's grid instead of a zero-area screen.
    let mut command = format!("stty rows 45 cols 152 2>/dev/null; {}", bin());
    for arg in args {
        command.push(' ');
        command.push_str(arg);
    }
    let mut spawn = Command::new("script");
    spawn
        .args(["-qec", &command, "/dev/null"])
        .env_remove("LAZYOS_FORCE_PANIC")
        .env("TERM", "xterm-256color")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure(&mut spawn);
    let mut child = spawn.spawn().expect("script should start");
    // Type only once the program has taken the screen: a key that reaches the
    // pty before raw mode is in the line discipline's cooked buffer (echoed as
    // `^[`), and the program then waits for ever for a key it never gets. On a
    // loaded machine the binary starts late enough for that to happen.
    let mut stdout = child.stdout.take().unwrap();
    let (seen, taken) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut all = Vec::new();
        let mut chunk = [0u8; 4096];
        let mut told = false;
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => all.extend_from_slice(&chunk[..n]),
            }
            if !told && all.windows(8).any(|w| w == b"\x1b[?1049h") {
                told = true;
                let _ = seen.send(());
            }
        }
        all
    });
    // A program that never draws (a usage error, a panic before the screen)
    // still gets its input after the ceiling, so the test ends either way.
    let _ = taken.recv_timeout(Duration::from_secs(10));
    let mut stdin = child.stdin.take().unwrap();
    let _ = stdin.write_all(input.as_bytes());
    let _ = stdin.flush();
    // The pipe stays open until the program is gone: `script(1)` writes a
    // literal EOT (`\x04`) into the pty when its own stdin reaches EOF, and
    // crossterm parses `ESC` followed by another byte as an `Alt` chord - so a
    // closed stdin turned Escape into `Alt+Ctrl+D` (the Developer menu) and
    // the window waited for ever. Browsers do not hang up the tty either.
    let output = child.wait_with_output().unwrap();
    drop(stdin);
    let stdout = reader.join().unwrap();
    let mut text = String::from_utf8_lossy(&stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (text, output.status.code().unwrap_or(-1))
}

fn in_pty(args: &[&str], input: &str, envs: &[(&str, &str)]) -> (String, i32) {
    in_pty_with(args, input, |command| {
        command.env("LAZYOS_UI", "1");
        for (key, value) in envs {
            command.env(key, value);
        }
    })
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .args(args)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("LAZYOS_UI")
        .env_remove("LAZYOS_FORCE_PANIC")
        .output()
        .unwrap()
}

#[test]
fn version_prints_the_committed_file() {
    let out = run(&["--version"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        version_on_disk()
    );
}

#[test]
fn help_lists_every_command_the_table_carries() {
    let out = run(&["--help"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("menu"), "the list names the menu");
    assert!(text.contains("menu files"), "the sub-surface is listed");
}

#[test]
fn symlinks_are_read_from_the_same_table() {
    let out = run(&["--symlinks"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.lines().any(|line| line == "lazyos-menu\tmenu"),
        "expected the installed link, got: {text}"
    );
}

#[test]
fn unknown_command_exits_two_and_lists_the_known_ones() {
    let out = run(&["nope"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown command 'nope'"), "{stderr}");
    assert!(stderr.contains("menu"), "the list is the fix: {stderr}");
}

#[test]
fn no_command_is_usage_not_a_menu() {
    let out = run(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage:"));
}

#[test]
fn a_symlink_resolves_to_its_command() {
    let dir = scratch("symlink");
    let link = dir.join("lazyos-menu");
    symlink(bin(), &link).unwrap();

    // No tty: if the name had not resolved, this would be "unknown command"
    // (exit 2). The no-terminal path names the command it reached (exit 3).
    let out = Command::new(&link)
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("LAZYOS_UI")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("lazyos menu:"), "{stderr}");
    assert!(!stderr.contains("unknown command"), "{stderr}");
}

#[test]
fn from_sway_one_window_opens_and_the_parent_returns() {
    let dir = scratch("spawn");
    let log = fake_foot(&dir);
    let started = Instant::now();
    // No pipes here: this measures the parent's own exit, and a pipe would
    // keep it alive until the window it spawned closed.
    let out = Command::new(bin())
        .args(["menu", "files"])
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env_remove("LAZYOS_UI")
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    let elapsed = started.elapsed();

    assert!(out.success());
    assert!(
        elapsed < Duration::from_millis(1500),
        "the host waited for a 2 s window ({elapsed:?}): a key binding cannot block"
    );
    assert!(wait_for(&log, Duration::from_secs(3)), "foot never ran");
    let recorded = std::fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = recorded.lines().collect();
    assert_eq!(lines[0], "--app-id=lazyos-pick", "{recorded}");
    assert_eq!(lines[1], "--window-size-pixels=900x600", "{recorded}");
    assert_eq!(lines[2], "-e");
    assert_eq!(lines[3], bin());
    // The resolved words, not the name it was called by.
    assert_eq!(&lines[4..6], &["menu", "files"], "{recorded}");
    assert!(
        recorded.contains("LAZYOS_UI=1"),
        "the child is marked as UI"
    );
}

#[test]
fn lazyos_ui_never_opens_a_second_window() {
    let dir = scratch("nest");
    let log = fake_foot(&dir);
    let out = Command::new(bin())
        .arg("menu")
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("LAZYOS_UI", "1")
        .env("PATH", format!("{}:/usr/bin:/bin", dir.display()))
        .output()
        .unwrap();
    // No tty in this test: the guard keeps it from spawning and reports why.
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no terminal"));
    std::thread::sleep(Duration::from_millis(150));
    assert!(!log.exists(), "the UI process spawned another window");
}

#[test]
fn without_a_terminal_the_reason_goes_to_stderr() {
    let out = run(&["menu"]);
    assert_eq!(out.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("no terminal"), "{stderr}");
}

#[test]
fn escape_returns_one_and_draws_the_root_menu() {
    if !have_script() {
        eprintln!("skipping: script(1) is not installed");
        return;
    }
    // A lone Escape: the root menu filters as you type, so `q` is a letter
    // there, not a way out (spec 007).
    // Without motion the menu is one frame: the opening sweep repaints cells
    // over 300 ms and ratatui sends only the changed ones, so on a slow runner a
    // word the test looks for arrived split across two frames.
    let (text, code) = in_pty(
        &["menu"],
        "\x1b",
        &[("LAZYOS_UI", "1"), ("LAZYOS_REDUCED_MOTION", "1")],
    );
    assert_eq!(code, 1, "Escape is exit 1; output was {text:?}");
    assert!(
        text.contains("Terminal"),
        "the first row is drawn: {text:?}"
    );
    assert!(text.contains("type to filter"), "{text:?}");
    assert!(
        text.contains("\x1b[?1049h"),
        "the alternate screen was entered"
    );
    assert!(
        text.contains(lazyos_cli::term::RESTORE),
        "the terminal was restored: {text:?}"
    );
}

#[test]
fn a_forced_panic_restores_the_terminal() {
    if !have_script() {
        eprintln!("skipping: script(1) is not installed");
        return;
    }
    let (text, code) = in_pty(
        &["menu", "files"],
        "",
        &[("LAZYOS_UI", "1"), ("LAZYOS_FORCE_PANIC", "1")],
    );
    assert_eq!(code, 101, "a panic is 101, got {code}: {text:?}");
    let entered = text
        .find("\x1b[?1049h")
        .expect("entered the alternate screen");
    let restored = text
        .find(lazyos_cli::term::RESTORE)
        .expect("the hook restored the terminal");
    assert!(
        restored > entered,
        "the restore must come after the screen was taken: {text:?}"
    );
    assert!(text.contains("LAZYOS_FORCE_PANIC"), "the reason survives");
}

#[test]
fn a_failed_command_shows_the_card_and_exits_three() {
    if !have_script() {
        eprintln!("skipping: script(1) is not installed");
        return;
    }
    let (text, code) = in_pty(&["menu", "bogus"], "x", &[("LAZYOS_UI", "1")]);
    assert_eq!(code, 3, "a failed command is exit 3: {text:?}");
    assert!(
        text.contains("is not something this menu knows"),
        "{text:?}"
    );
    assert!(text.contains("try: lazyos menu"), "{text:?}");
}

#[test]
fn a_menu_writes_nothing_to_disk() {
    if !have_script() {
        eprintln!("skipping: script(1) is not installed");
        return;
    }
    let home = scratch("home");
    let (text, code) = in_pty_with(&["menu"], "\x1b", |command| {
        command
            .env("LAZYOS_UI", "1")
            .env("HOME", &home)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_STATE_HOME")
            .env_remove("XDG_CACHE_HOME");
    });
    assert_eq!(code, 1, "{text:?}");

    let leftovers: Vec<_> = std::fs::read_dir(&home)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        leftovers.is_empty(),
        "a menu that was cancelled left {leftovers:?} behind"
    );
}
