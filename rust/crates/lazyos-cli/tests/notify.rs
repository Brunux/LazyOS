//! `lazyos notify` as a process (spec 006).
//!
//! Two witnesses, because each can see one thing:
//!
//! - **The argument vector.** A stub `notify-send` is put first on the child's
//!   `PATH` and records what it was asked. That is the card the binary built -
//!   the picture, the group tag, the bar, the urgency, the expiry - seen from
//!   outside the process, with no bus at all. Runs everywhere.
//! - **The route.** The real `notify-send` runs on a private session bus while
//!   `dbus-monitor` watches it. With no daemon on the bus, `notify-send` stops
//!   after asking for the server's information, so what the monitor can show is
//!   the call addressed to `org.freedesktop.Notifications` on the session bus and
//!   the refusal that comes back. Skipped (not failed) when `dbus-daemon`,
//!   `dbus-monitor` or `notify-send` is missing, the same rule the pty tests
//!   follow; the workflow installs all three so this witness runs on the runner.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_lazyos")
}

fn theme() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../uis/sway/files/usr/share/icons/LazyOS")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lazyos-notify-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A `notify-send` that writes its arguments to `log`, one per line, and prints
/// an id the way mako does with `-p`. Returns the directory to put first on PATH.
fn stub_notify_send(name: &str) -> (PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch(name);
    let log = dir.join("argv.txt");
    let program = dir.join("notify-send");
    // Through a scratch name: a writer still open in another thread's `fork`
    // makes the child's `exec` fail with `ETXTBSY` ("Text file busy").
    let scratch = dir.join("notify-send.new");
    std::fs::write(
        &scratch,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\necho 7\n",
            log.display()
        ),
    )
    .unwrap();
    let mut perms = std::fs::metadata(&scratch).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&scratch, perms).unwrap();
    std::fs::rename(&scratch, &program).unwrap();
    (dir, log)
}

/// Run `lazyos notify ARGS` with the stub first on PATH.
fn run_with_stub(name: &str, args: &[&str], roots: Option<&Path>) -> (Output, Vec<String>) {
    let (dir, log) = stub_notify_send(name);
    let path = format!(
        "{}:{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let mut command = Command::new(bin());
    command
        .arg("notify")
        .args(args)
        .env("PATH", path)
        .env_remove("LAZYOS_UI");
    if let Some(roots) = roots {
        command.env("LAZYOS_THEME_ROOTS", roots);
    }
    let output = command.output().expect("lazyos runs");
    let argv = std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    (output, argv)
}

/// The argument immediately after `flag`, if the flag was passed.
fn after(argv: &[String], flag: &str) -> Option<String> {
    let at = argv.iter().position(|arg| arg == flag)?;
    argv.get(at + 1).cloned()
}

fn has(argv: &[String], flag: &str, value: &str) -> bool {
    argv.windows(2)
        .any(|pair| pair[0] == flag && pair[1] == value)
}

#[test]
fn the_osd_card_asks_for_its_group_its_bar_and_its_picture() {
    let (out, argv) = run_with_stub(
        "osd",
        &[
            "progress",
            "osd",
            "40",
            "Volume",
            "40%",
            "--icon",
            "audio-volume-high",
            "--urgency",
            "low",
        ],
        Some(&theme()),
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(
        has(
            &argv,
            "-h",
            "string:x-canonical-private-synchronous:lazyos-osd"
        ),
        "{argv:?}"
    );
    assert!(has(&argv, "-h", "int:value:40"), "{argv:?}");
    assert_eq!(after(&argv, "-u").as_deref(), Some("low"), "{argv:?}");
    assert_eq!(
        after(&argv, "-i").as_deref(),
        Some("audio-volume-high"),
        "{argv:?}"
    );
    assert_eq!(after(&argv, "-a").as_deref(), Some("LazyOS"), "{argv:?}");
    let end = argv.iter().position(|a| a == "--").expect("the terminator");
    assert_eq!(
        &argv[end + 1..],
        &["Volume", "40%"],
        "the text follows the terminator"
    );
}

#[test]
fn a_glyph_name_is_the_theme_picture_and_auto_is_a_folder_for_a_directory() {
    let (_, argv) = run_with_stub(
        "glyph",
        &["send", "Saved", "--icon", "image"],
        Some(&theme()),
    );
    assert_eq!(
        after(&argv, "-i").as_deref(),
        Some("image-x-generic"),
        "`image` is the alphabet's name and draws the theme's image icon: {argv:?}"
    );

    let dir = theme();
    let (_, argv) = run_with_stub(
        "auto",
        &["say", "Opened", &dir.display().to_string()],
        Some(&theme()),
    );
    assert_eq!(
        after(&argv, "-i").as_deref(),
        Some("folder"),
        "auto reads a directory as a folder: {argv:?}"
    );
}

#[test]
fn a_picture_the_theme_lacks_is_sent_as_text_and_reported() {
    let (out, argv) = run_with_stub(
        "missing",
        &["send", "Saved", "--icon", "no-such-picture"],
        Some(Path::new("/nonexistent/lazyos-theme")),
    );
    assert_eq!(out.status.code(), Some(0), "a card still goes out as text");
    assert!(
        text(&out.stderr).contains("no-such-picture.png"),
        "the fallback names what is missing: {}",
        text(&out.stderr)
    );
    assert!(
        argv.iter().all(|a| a != "-i"),
        "no picture is sent: {argv:?}"
    );
    assert!(
        argv.iter().all(|a| !a.contains("no-such-picture")),
        "{argv:?}"
    );
}

#[test]
fn critical_and_never_expire_are_passed_as_asked() {
    let (_, argv) = run_with_stub(
        "urgency",
        &[
            "send",
            "Failed",
            "--urgency",
            "critical",
            "--timeout",
            "never",
        ],
        Some(&theme()),
    );
    assert_eq!(after(&argv, "-u").as_deref(), Some("critical"), "{argv:?}");
    assert!(
        argv.iter().any(|a| a == "--expire-time=0"),
        "never is 0: {argv:?}"
    );
}

#[test]
fn no_icon_sends_no_picture_and_a_bad_percentage_sends_nothing() {
    let (_, argv) = run_with_stub("none", &["send", "Quiet", "--icon", "none"], Some(&theme()));
    assert!(argv.iter().all(|a| a != "-i"), "{argv:?}");

    let (out, argv) = run_with_stub(
        "usage",
        &["progress", "osd", "150", "Volume"],
        Some(&theme()),
    );
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(
        argv.is_empty(),
        "a usage error never reaches the daemon: {argv:?}"
    );
}

/// A session bus of our own. Dropping it stops the daemon.
struct Bus {
    daemon: Child,
    address: String,
    dir: PathBuf,
}

impl Bus {
    /// `None` when the bus tools or the client are not installed.
    ///
    /// `notify-send` is part of the condition, not an assumption: the route this
    /// test watches is the one the *shipped* client makes, so with no
    /// `libnotify-bin` on the machine there is no call to watch and the test
    /// would fail on the runner for a reason that has nothing to do with the
    /// code (`cannot run notify-send: No such file or directory`, which is what
    /// CI said). The workflow installs it so this witness runs there; a
    /// developer's machine that lacks it skips instead of lying.
    fn start(name: &str) -> Option<Self> {
        if !have("dbus-daemon") || !have("dbus-monitor") || !have("notify-send") {
            return None;
        }
        let dir = scratch(&format!("bus-{name}"));
        let config = dir.join("bus.conf");
        std::fs::write(
            &config,
            format!(
                "<!DOCTYPE busconfig PUBLIC \"-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN\" \
                 \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\">\n\
                 <busconfig>\n  <type>session</type>\n  <listen>unix:tmpdir={}</listen>\n  \
                 <auth>EXTERNAL</auth>\n  <policy context=\"default\">\n    \
                 <allow send_destination=\"*\"/>\n    \
                 <allow receive_sender=\"*\"/>\n    \
                 <allow eavesdrop=\"true\"/>\n    <allow own=\"*\"/>\n  </policy>\n</busconfig>\n",
                dir.display()
            ),
        )
        .unwrap();
        let mut daemon = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .arg("--print-address=1")
            .arg("--nofork")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut line = String::new();
        BufReader::new(daemon.stdout.take()?)
            .read_line(&mut line)
            .ok()?;
        let address = line.trim().to_string();
        if address.is_empty() {
            let _ = daemon.kill();
            return None;
        }
        Some(Self {
            daemon,
            address,
            dir,
        })
    }

    /// Run `lazyos notify ARGS` on this bus while a monitor records the traffic.
    fn notify(&self, args: &[&str]) -> (Output, String) {
        let mut monitor = Command::new("dbus-monitor")
            .arg("--address")
            .arg(&self.address)
            .arg("type='method_call',interface='org.freedesktop.Notifications'")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-monitor starts");
        // The monitor's match has to be in place before the call.
        std::thread::sleep(Duration::from_millis(700));

        let output = Command::new(bin())
            .arg("notify")
            .args(args)
            .env("DBUS_SESSION_BUS_ADDRESS", &self.address)
            // The card takes its picture from the installed theme, and this test
            // runs on machines that have no image: the binary's default roots are
            // `/usr/share/icons/LazyOS`, which exists only on the device, so the
            // card came out as text and stderr carried "no picture: the LazyOS
            // theme has no dialog-information.png". Pointing the run at the
            // checkout's own theme tests the picture path for real instead of
            // stepping around it.
            .env("LAZYOS_THEME_ROOTS", theme())
            .env_remove("LAZYOS_UI")
            .output()
            .expect("lazyos runs");

        std::thread::sleep(Duration::from_millis(400));
        // SIGTERM ends dbus-monitor once it has printed what it read. SIGINT is
        // not used: a background job inherits it as ignored. SIGKILL is the
        // fallback, so a monitor that ignores both cannot hang the suite.
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(monitor.id().to_string())
            .status();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match monitor.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                _ => {
                    let _ = monitor.kill();
                    let _ = monitor.wait();
                    break;
                }
            }
        }
        let mut transcript = String::new();
        if let Some(mut out) = monitor.stdout.take() {
            let _ = out.read_to_string(&mut transcript);
        }
        (output, transcript)
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

#[test]
fn the_card_is_addressed_to_the_notification_service_on_the_session_bus() {
    let Some(bus) = Bus::start("route") else {
        eprintln!(
            "skipped: dbus-daemon, dbus-monitor or notify-send (libnotify-bin) is not installed"
        );
        return;
    };
    let (out, transcript) = bus.notify(&["progress", "osd", "40", "Volume", "40%"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "no daemon answers: the card is refused"
    );
    let stderr = text(&out.stderr);
    assert!(
        stderr.contains("no notification daemon"),
        "the refusal is said in words: {stderr}"
    );
    // The picture resolves against the checkout's theme, so a card that came out
    // text here means the icon path broke - and that failure is silent on the
    // device, which is the whole reason this is asserted.
    assert!(
        !stderr.contains("no picture"),
        "the card kept its picture: {stderr}"
    );
    assert!(
        transcript.contains("destination=org.freedesktop.Notifications")
            && transcript.contains("member=GetServerInformation"),
        "the call goes to the notification service on the session bus: {transcript}"
    );
}
