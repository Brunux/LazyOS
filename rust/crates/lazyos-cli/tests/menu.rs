//! The root menu, the palette and the Ask strip, driven the way a person drives
//! them: keys into the window, then what the window decided (spec 007).
//!
//! Three things are proved here that a unit test cannot see:
//!
//! - the screens, as golden snapshots at both grids (`tests/snapshots/`,
//!   `UPDATE_SNAPSHOTS=1` to regenerate - and read the diff);
//! - a scripted key sequence ends on the action the row names, as a program and
//!   its words, and a destructive row needs more than a stray Enter;
//! - a question runs the agent the menu names (a fake `opencode` and a fake
//!   `claude` on the agents' `PATH`), the answer lands in the window, and no
//!   process the agent started is alive afterwards - not the one in its group,
//!   and not the one that called `setsid` to leave it.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use lazyos_cli::ask::{Agent, Choice};
use lazyos_cli::menu::screen::{MenuScreen, Picked, Setup};
use lazyos_cli::menu::{Context, SessionAction, Surface};
use lazyos_ui::{App, Design, Header, Outcome, Screen};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

const SIZES: [(u16, u16); 2] = [(80, 24), (152, 45)];

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_lazyos")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lazyos-menu-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A home with an empty state directory, a runtime directory, and the
/// agents' bin directories the session would search.
struct Home {
    root: PathBuf,
}

impl Home {
    fn new(name: &str) -> Self {
        let root = scratch(name);
        for dir in ["state", "run", ".opencode/bin", ".local/bin"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        Self { root }
    }

    fn vars(&self) -> BTreeMap<String, String> {
        let root = self.root.display().to_string();
        [
            ("HOME", root.clone()),
            ("PATH", "/usr/bin:/bin".to_string()),
            ("XDG_RUNTIME_DIR", format!("{root}/run")),
            ("XDG_CONFIG_HOME", format!("{root}/.config")),
            ("XDG_DATA_HOME", format!("{root}/.local/share")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }

    fn state(&self) -> PathBuf {
        self.root.join("state/lazyos")
    }

    /// A fake agent CLI, where the image (OpenCode) or npm (the others) puts it.
    fn agent(&self, name: &str, body: &str) {
        let dir = if name == "opencode" {
            ".opencode/bin"
        } else {
            ".local/bin"
        };
        let path = self.root.join(dir).join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// The pids a fake agent wrote down for the helpers it left behind.
    fn helpers(&self) -> Vec<i32> {
        fs::read_to_string(self.root.join("helpers"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect()
    }
}

fn setup(home: &Home, surface: Surface, agent: Option<Agent>) -> Setup {
    Setup {
        surface,
        context: Context {
            agent: Some("opencode".to_string()),
            ask: None,
        },
        vars: home.vars(),
        exe: bin().to_string(),
        state: home.state(),
        agent: agent.map(|agent| Choice { agent, note: None }),
        ask: None,
    }
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn ctrl(ch: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL))
}

fn design() -> Design {
    Design::solarized().with_reduced_motion(true)
}

/// Feed events through the shell, as the loop would, and return the last
/// outcome. Each event draws first, so a hit region or a page size is the one
/// the person saw.
fn drive(screen: &mut MenuScreen, events: &[Event]) -> Outcome {
    let mut app = App::new(Header::new("LazyOS"))
        .design(design())
        .screen(screen);
    let mut terminal = Terminal::new(TestBackend::new(152, 45)).unwrap();
    let mut outcome = Outcome::Continue;
    for event in events {
        terminal.draw(|frame| app.render(frame)).unwrap();
        outcome = app.handle(event.clone());
        if let Outcome::Done(_) = outcome {
            break;
        }
    }
    outcome
}

fn typed(text: &str) -> Vec<Event> {
    text.chars().map(|ch| press(KeyCode::Char(ch))).collect()
}

fn render(screen: &mut MenuScreen, width: u16, height: u16) -> String {
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

fn compare(name: &str, rendered: &str) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
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

/// Both grids of one state, named `<name>-<w>x<h>`.
fn snapshot(name: &str, make: impl Fn() -> MenuScreen) {
    for (w, h) in SIZES {
        let mut screen = make();
        compare(&format!("{name}-{w}x{h}"), &render(&mut screen, w, h));
    }
}

fn wait_for(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    done()
}

/// Alive and not a zombie: an orphan that was killed but not yet reaped still
/// answers `kill(pid, 0)`.
fn alive(pid: i32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit(')')
            .next()
            .and_then(|rest| rest.split_whitespace().next())
            .is_some_and(|state| state != "Z" && state != "X"),
        Err(_) => false,
    }
}

/// Tick the screen until the question is answered (or not), as the loop's
/// 100 ms wake would.
fn settle(screen: &mut MenuScreen) {
    let start = Instant::now();
    while screen.needs_tick().is_some() && start.elapsed() < Duration::from_secs(15) {
        let _ = screen.tick();
        std::thread::sleep(Duration::from_millis(50));
    }
}

// ---------------------------------------------------------------------------
// Snapshots
// ---------------------------------------------------------------------------

#[test]
fn snapshots_of_the_menu() {
    let home = Home::new("snap-menu");
    snapshot("menu", || {
        MenuScreen::new(setup(&home, Surface::Root, None))
    });
    snapshot("menu-filtered", || {
        let mut screen = MenuScreen::new(setup(&home, Surface::Root, None));
        drive(&mut screen, &typed("wifi"));
        screen
    });
    snapshot("menu-empty", || {
        let mut screen = MenuScreen::new(setup(&home, Surface::Root, None));
        drive(&mut screen, &typed("zzqx"));
        screen
    });
    snapshot("menu-power", || {
        let mut screen = MenuScreen::new(setup(&home, Surface::Root, None));
        let mut keys = typed("power");
        keys.push(press(KeyCode::Enter));
        drive(&mut screen, &keys);
        screen
    });
    snapshot("menu-confirm", || {
        let mut screen = MenuScreen::new(setup(&home, Surface::Root, None));
        let mut keys = typed("power");
        keys.extend([
            press(KeyCode::Enter),
            press(KeyCode::Down),
            press(KeyCode::Enter),
        ]);
        drive(&mut screen, &keys);
        screen
    });
    snapshot("palette-filtered", || {
        let mut screen = MenuScreen::new(setup(&home, Surface::Palette, None));
        drive(&mut screen, &typed("theme"));
        screen
    });
}

#[test]
fn snapshots_of_the_ask_strip() {
    let home = Home::new("snap-ask");
    snapshot("ask-prompt", || {
        let mut screen = MenuScreen::new(setup(&home, Surface::Root, Some(Agent::OpenCode)));
        let mut keys = typed("ask");
        keys.push(press(KeyCode::Enter));
        keys.extend(typed("What is the capital of France?"));
        drive(&mut screen, &keys);
        screen
    });
    snapshot("ask-no-agent", || {
        let mut s = setup(&home, Surface::Root, None);
        s.ask = Some(String::new());
        MenuScreen::new(s)
    });
}

/// The running state, drawn right after Enter and before the runner's first
/// write: the question has been handed off and the window says who answers.
#[test]
fn snapshot_of_a_question_being_answered() {
    let home = Home::new("snap-running");
    // An agent that takes its time, so the first frame is the running one.
    home.agent("opencode", "sleep 5\n");
    for (w, h) in SIZES {
        let mut s = setup(&home, Surface::Root, Some(Agent::OpenCode));
        s.ask = Some("Why is the sky blue?".to_string());
        let mut screen = MenuScreen::new(s);
        drive(&mut screen, &[press(KeyCode::Enter)]);
        compare(&format!("ask-running-{w}x{h}"), &render(&mut screen, w, h));
        // Closing the window stops the question; nothing is left to clean up.
        assert_eq!(drive(&mut screen, &[press(KeyCode::Esc)]), Outcome::Done(1));
    }
}

// ---------------------------------------------------------------------------
// Scripted keys
// ---------------------------------------------------------------------------

#[test]
fn typing_a_name_and_enter_runs_that_row_as_a_program_and_its_words() {
    let home = Home::new("dispatch");
    let mut screen = MenuScreen::new(setup(&home, Surface::Root, None));
    let mut keys = typed("wifi");
    keys.push(press(KeyCode::Enter));
    assert_eq!(drive(&mut screen, &keys), Outcome::Done(0));
    let bar_net = home.root.join(".config/bin/bar-net").display().to_string();
    assert_eq!(
        screen.chosen,
        Some(Picked::Run {
            key: "wifi".to_string(),
            argv: vec![bar_net],
        })
    );

    // A word of the group narrows the menu to it: `session` puts its rows first.
    let mut screen = MenuScreen::new(setup(&home, Surface::Root, None));
    let mut keys = typed("lock");
    keys.push(press(KeyCode::Enter));
    drive(&mut screen, &keys);
    assert!(
        matches!(&screen.chosen, Some(Picked::Run { key, .. }) if key == "lock"),
        "{:?}",
        screen.chosen
    );
}

#[test]
fn escape_closes_with_exit_one_and_writes_nothing() {
    let home = Home::new("escape");
    let mut screen = MenuScreen::new(setup(&home, Surface::Root, None));
    let mut keys = typed("wif");
    keys.push(press(KeyCode::Esc));
    assert_eq!(drive(&mut screen, &keys), Outcome::Done(1));
    assert_eq!(screen.chosen, None);
    assert!(
        !home.state().exists(),
        "a menu that was closed wrote its state directory"
    );
}

#[test]
fn a_stray_enter_on_power_runs_nothing() {
    let home = Home::new("power");
    for row in 0..3 {
        // power → Enter → the row → Enter → the confirm, Cancel selected → Enter.
        let mut screen = MenuScreen::new(setup(&home, Surface::Root, None));
        let mut keys = typed("power");
        keys.push(press(KeyCode::Enter));
        keys.extend((0..row).map(|_| press(KeyCode::Down)));
        keys.extend([press(KeyCode::Enter), press(KeyCode::Enter)]);
        assert_eq!(drive(&mut screen, &keys), Outcome::Continue, "row {row}");
        assert_eq!(screen.chosen, None, "row {row}: Cancel is the first answer");
    }

    // Escape backs out one level at a time: confirm → power list → menu, and
    // only then closes the window.
    let mut screen = MenuScreen::new(setup(&home, Surface::Root, None));
    let mut keys = typed("power");
    keys.extend([press(KeyCode::Enter), press(KeyCode::Enter)]);
    keys.extend([press(KeyCode::Esc), press(KeyCode::Esc)]);
    assert_eq!(drive(&mut screen, &keys), Outcome::Continue);
    assert_eq!(drive(&mut screen, &[press(KeyCode::Esc)]), Outcome::Done(1));
    assert_eq!(screen.chosen, None);

    // The answer that does it is a deliberate second choice.
    let mut screen = MenuScreen::new(setup(&home, Surface::Root, None));
    let mut keys = typed("power");
    keys.extend([
        press(KeyCode::Enter),
        press(KeyCode::Down),
        press(KeyCode::Enter),
        press(KeyCode::Right),
        press(KeyCode::Enter),
    ]);
    assert_eq!(drive(&mut screen, &keys), Outcome::Done(0));
    assert_eq!(screen.chosen, Some(Picked::Session(SessionAction::Reboot)));
}

#[test]
fn the_password_row_is_handled_by_the_menu_not_a_command_line() {
    let home = Home::new("password");
    let mut screen = MenuScreen::new(setup(&home, Surface::Root, None));
    let mut keys = typed("password");
    keys.push(press(KeyCode::Enter));
    assert_eq!(drive(&mut screen, &keys), Outcome::Done(0));
    assert_eq!(
        screen.chosen,
        Some(Picked::Password {
            key: "password".to_string()
        })
    );
}

#[test]
fn the_palette_sets_a_theme_through_this_binary() {
    let home = Home::new("palette");
    let mut screen = MenuScreen::new(setup(&home, Surface::Palette, None));
    let mut keys = typed("theme: nord");
    keys.push(press(KeyCode::Enter));
    assert_eq!(drive(&mut screen, &keys), Outcome::Done(0));
    assert_eq!(
        screen.chosen,
        Some(Picked::Run {
            key: "theme-nord".to_string(),
            argv: vec![bin().into(), "theme".into(), "set".into(), "nord".into()],
        })
    );
}

#[test]
fn prompt_cancellation_goes_back_and_sends_nothing() {
    let home = Home::new("cancel");
    home.agent("opencode", "touch \"$HOME/asked\"\n");
    let mut screen = MenuScreen::new(setup(&home, Surface::Root, Some(Agent::OpenCode)));
    let mut keys = typed("ask");
    keys.push(press(KeyCode::Enter));
    keys.extend(typed("never mind"));
    keys.push(press(KeyCode::Esc));
    assert_eq!(
        drive(&mut screen, &keys),
        Outcome::Continue,
        "back to the list"
    );
    // An empty prompt does not send on Enter either.
    let mut keys = typed("ask");
    keys.extend([press(KeyCode::Enter), press(KeyCode::Enter)]);
    let mut screen = MenuScreen::new(setup(&home, Surface::Root, Some(Agent::OpenCode)));
    drive(&mut screen, &keys);
    assert_eq!(screen.needs_tick(), None, "nothing is running");

    // Opened as `lazyos ask`, Escape closes the window.
    let mut s = setup(&home, Surface::Root, Some(Agent::OpenCode));
    s.ask = Some(String::new());
    let mut screen = MenuScreen::new(s);
    assert_eq!(drive(&mut screen, &[press(KeyCode::Esc)]), Outcome::Done(1));
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !home.root.join("asked").exists(),
        "the agent was never started"
    );
    assert!(
        !home.state().join("tasks").exists(),
        "no task was written for a question that was not asked"
    );
}

// ---------------------------------------------------------------------------
// The agent, once
// ---------------------------------------------------------------------------

/// A fake `opencode run --format json`: the events on stdout, plus two
/// helpers - one in its process group, one that left it with `setsid` - and
/// their pids where the test can find them.
const OPENCODE_ANSWERS: &str = r#"
[ "$1" = run ] && [ "$2" = --format ] && [ "$3" = json ] || { echo "unexpected: $*" >&2; exit 9; }
[ -z "$(ls -A .)" ] || { echo "not an empty directory: $(pwd)" >&2; exit 9; }
sleep 60 &
echo $! >>"$HOME/helpers"
setsid sleep 60 &
echo $! >>"$HOME/helpers"
printf '%s\n' '{"type":"step_start","part":{"type":"step-start"}}'
printf '%s\n' '{"type":"text","part":{"type":"text","text":"Paris."}}'
printf '%s\n' "{\"type\":\"text\",\"part\":{\"type\":\"text\",\"text\":\"You asked: $4\"}}"
"#;

#[test]
fn opencode_answers_inline_and_leaves_nothing_behind() {
    let home = Home::new("opencode");
    home.agent("opencode", OPENCODE_ANSWERS);
    let mut screen = MenuScreen::new(setup(&home, Surface::Root, Some(Agent::OpenCode)));
    let mut keys = typed("ask");
    keys.push(press(KeyCode::Enter));
    keys.extend(typed("what is $HOME; rm -rf ~"));
    keys.push(press(KeyCode::Enter));
    drive(&mut screen, &keys);
    settle(&mut screen);

    let shown = render(&mut screen, 152, 45);
    assert!(shown.contains("Paris."), "{shown}");
    assert!(
        shown.contains("You asked: what is $HOME; rm -rf ~"),
        "the question arrived as one argument, unexpanded: {shown}"
    );
    assert!(shown.contains("[c]"), "a copy action is offered: {shown}");

    let helpers = home.helpers();
    assert_eq!(helpers.len(), 2, "the fake started its two helpers");
    assert!(
        wait_for(Duration::from_secs(3), || !helpers
            .iter()
            .any(|&p| alive(p))),
        "a helper the agent started outlived the answer: {helpers:?}"
    );
    assert_eq!(drive(&mut screen, &[press(KeyCode::Esc)]), Outcome::Done(0));
}

#[test]
fn claude_code_answers_with_no_tools_and_no_session() {
    let home = Home::new("claude");
    home.agent(
        "claude",
        r#"
echo "$*" >"$HOME/argv"
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"result":"Forty-two."}'
"#,
    );
    let mut s = setup(&home, Surface::Root, Some(Agent::Claude));
    s.ask = Some("-p the answer?".to_string());
    let mut screen = MenuScreen::new(s);
    drive(&mut screen, &[press(KeyCode::Enter)]);
    settle(&mut screen);
    assert!(render(&mut screen, 152, 45).contains("Forty-two."));
    let argv = fs::read_to_string(home.root.join("argv")).unwrap();
    assert_eq!(
        argv.trim(),
        "-p --output-format json --tools  --disallowedTools mcp__* --no-session-persistence -- -p the answer?"
    );
}

#[test]
fn a_failure_shows_the_agents_own_error_line() {
    let home = Home::new("failure");
    home.agent(
        "opencode",
        r#"
printf '%s\n' 'INFO  starting' >&2
printf '%s\n' '{"type":"error","error":{"name":"ProviderInitError","data":{"message":"No provider is signed in: run opencode auth login"}}}'
exit 1
"#,
    );
    let mut s = setup(&home, Surface::Root, Some(Agent::OpenCode));
    s.ask = Some("hello".to_string());
    let mut screen = MenuScreen::new(s);
    drive(&mut screen, &[press(KeyCode::Enter)]);
    settle(&mut screen);
    let shown = render(&mut screen, 152, 45);
    assert!(shown.contains("No answer"), "{shown}");
    assert!(
        shown.contains("No provider is signed in: run opencode auth login"),
        "{shown}"
    );
    assert!(
        !shown.contains("ERROR:"),
        "the protocol mark is not shown: {shown}"
    );

    // An agent that cannot even start says so in its own words (stderr).
    let home = Home::new("crash");
    home.agent(
        "opencode",
        "echo 'error: unknown option --format' >&2\nexit 2\n",
    );
    let mut s = setup(&home, Surface::Root, Some(Agent::OpenCode));
    s.ask = Some("hello".to_string());
    let mut screen = MenuScreen::new(s);
    drive(&mut screen, &[press(KeyCode::Enter)]);
    settle(&mut screen);
    assert!(render(&mut screen, 152, 45).contains("error: unknown option --format"));
}

#[test]
fn stopping_a_question_ends_the_agents_group() {
    let home = Home::new("stop");
    home.agent(
        "opencode",
        "sleep 60 &\necho $! >>\"$HOME/helpers\"\nsetsid sleep 60 &\necho $! >>\"$HOME/helpers\"\nsleep 60\n",
    );
    let mut s = setup(&home, Surface::Root, Some(Agent::OpenCode));
    s.ask = Some("slow".to_string());
    let mut screen = MenuScreen::new(s);
    drive(&mut screen, &[press(KeyCode::Enter)]);
    assert!(
        wait_for(Duration::from_secs(5), || home.helpers().len() == 2),
        "the agent started"
    );
    // Ctrl+C in the window, like Escape: the question is stopped, not orphaned.
    assert_eq!(drive(&mut screen, &[ctrl('c')]), Outcome::Done(1));
    let helpers = home.helpers();
    assert!(
        wait_for(Duration::from_secs(5), || !helpers
            .iter()
            .any(|&p| alive(p))),
        "the agent's helpers outlived a stopped question: {helpers:?}"
    );
}

/// The window can disappear without a word (Alt+Q on it, a killed foot): the
/// child watches the window's process, and its exit ends the question.
#[test]
fn a_window_that_dies_takes_the_question_with_it() {
    let home = Home::new("viewer");
    home.agent(
        "opencode",
        "setsid sleep 60 &\necho $! >>\"$HOME/helpers\"\nsleep 60\n",
    );
    let mut viewer = Command::new("sleep").arg("60").spawn().unwrap();
    let mut env = home.vars();
    env.insert(
        "PATH".into(),
        format!("{}/.opencode/bin:/usr/bin:/bin", home.root.display()),
    );
    let mut ask = Command::new(bin())
        .args([
            "--ask-run",
            "ask-viewer-test",
            "opencode",
            &viewer.id().to_string(),
            "hi",
        ])
        .env_clear()
        .envs(&env)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(wait_for(Duration::from_secs(5), || home.helpers().len() == 1));
    viewer.kill().unwrap();
    viewer.wait().unwrap();
    assert!(
        wait_for(Duration::from_secs(5), || ask.try_wait().unwrap().is_some()),
        "--ask-run kept going after the window was gone"
    );
    let helpers = home.helpers();
    assert!(
        wait_for(Duration::from_secs(3), || !helpers
            .iter()
            .any(|&p| alive(p))),
        "{helpers:?}"
    );
    assert!(
        lazyos_cli::ask::marked("LAZYOS_ASK=ask-viewer-test").is_empty(),
        "a process still carries the question's mark"
    );
}

#[test]
fn rows_lists_the_table_without_a_window() {
    // `--rows` is what check-system.sh compares with bar-menu's rows; from the
    // session it must print, not open a foot.
    let out = Command::new(bin())
        .args(["menu", "--rows"])
        .env("WAYLAND_DISPLAY", "wayland-test")
        .env_remove("LAZYOS_UI")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let keys: Vec<&str> = text
        .lines()
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert_eq!(keys.first(), Some(&"terminal"));
    assert!(keys.contains(&"ask"));
    assert_eq!(keys.last(), Some(&"power"));
    assert!(text.lines().any(|l| l == "ask\tai\tAsk"), "{text}");
}
