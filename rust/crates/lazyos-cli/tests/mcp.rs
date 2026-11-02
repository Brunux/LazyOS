//! `lazyos mcp` as an agent meets it: the real binary over a pipe, a scripted
//! session, and fake programs on `PATH` for everything that would touch a
//! desktop (spec 008).
//!
//! What is proved here and nowhere else:
//!
//! - the served table is the manifest, every tool has an implementation, and a
//!   refused name is an error the log keeps while the server stays up;
//! - a propose tool opens the gated window the menu opens - and nothing else;
//! - the screenshots die with the session: at the end of input and on SIGTERM;
//! - every tool's mapping resolves to a real subcommand or a script the image
//!   ships, and the shipped instructions pin the binary's manifest version.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use lazyos_mcp::{Maps, TOOLS, Tier, manifest};
use serde_json::{Value, json};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_lazyos")
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lazyos-mcp-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn script(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let scratch = path.with_file_name(format!(
        "{}.new",
        path.file_name().unwrap().to_string_lossy()
    ));
    fs::write(&scratch, format!("#!/bin/sh\n{body}")).unwrap();
    fs::set_permissions(&scratch, fs::Permissions::from_mode(0o755)).unwrap();
    fs::rename(&scratch, path).unwrap();
}

/// A home, a runtime directory and fake programs. Every fake that opens
/// something appends its name and argv to `$HOME/opened`, one line each.
struct Desk {
    root: PathBuf,
}

const RECORD: &str = r#"{ printf '%s' "$(basename "$0")"; for a in "$@"; do printf ' [%s]' "$a"; done; printf '\n'; } >>"$HOME/opened"
"#;

impl Desk {
    fn new(name: &str) -> Self {
        let root = scratch(name);
        for dir in ["home/.config/bin", "run", "fake", "state"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::set_permissions(root.join("run"), fs::Permissions::from_mode(0o700)).unwrap();
        let fake = root.join("fake");
        // grim writes a PNG header for a 1366x912 picture to its last argument.
        script(
            &fake.join("grim"),
            r#"for last; do :; done
printf '\211PNG\r\n\032\n\000\000\000\015IHDR\000\000\005\126\000\000\003\220' >"$last"
"#,
        );
        script(
            &fake.join("swaymsg"),
            r#"[ "$1 $2" = "-t get_tree" ] || exit 0
printf '%s\n' '{"type":"root","nodes":[{"type":"output","name":"eDP-1","nodes":[{"type":"workspace","name":"2","nodes":[{"type":"con","pid":7,"app_id":"foot","name":"~/Work","focused":true,"nodes":[]}],"floating_nodes":[]}]}]}'
"#,
        );
        script(&fake.join("wl-paste"), "printf 'copied text'\n");
        script(
            &fake.join("dpkg-query"),
            "printf 'ii \\tsway\\t1.12.0\\nii \\tfoot\\t1.28\\nrc \\told-thing\\t0.1\\n'\n",
        );
        script(
            &fake.join("check-system.sh"),
            "printf '  \\033[32mOK\\033[0m    sway runs\\n  \\033[33mWARN\\033[0m  a warning\\n  \\033[31mFAIL\\033[0m  a failure\\n'\nexit 1\n",
        );
        // foot records the window it was asked to open and exits, as if the
        // window had closed: `lazyos menu` hands itself to foot this way.
        script(&fake.join("foot"), RECORD);
        script(&fake.join("notify-send"), &format!("{RECORD}echo 1\n"));
        let bin = root.join("home/.config/bin");
        for name in [
            "rofi-install",
            "rofi-format",
            "rofi-swap",
            "lazyos-view",
            "rofi-files",
        ] {
            script(&bin.join(name), RECORD);
        }
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn env(&self) -> BTreeMap<String, String> {
        let root = self.root.display().to_string();
        [
            ("HOME", format!("{root}/home")),
            ("PATH", format!("{root}/fake:/usr/bin:/bin")),
            ("XDG_RUNTIME_DIR", format!("{root}/run")),
            ("XDG_STATE_HOME", format!("{root}/state")),
            ("XDG_CONFIG_HOME", format!("{root}/home/.config")),
            ("WAYLAND_DISPLAY", "wayland-test".to_string()),
            (
                "LAZYOS_DOC_DIR",
                repo()
                    .join("uis/sway/files/usr/share/doc/lazyos")
                    .display()
                    .to_string(),
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }

    fn opened(&self) -> Vec<String> {
        fs::read_to_string(self.home().join("opened"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    fn log(&self) -> String {
        fs::read_to_string(self.root.join("state/lazyos/mcp.log")).unwrap_or_default()
    }

    fn shots(&self, pid: u32) -> PathBuf {
        self.root.join(format!("run/lazyos-mcp-{pid}"))
    }

    fn start(&self) -> Session {
        let mut child = Command::new(bin())
            .arg("mcp")
            .env_clear()
            .envs(self.env())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut session = Session {
            child,
            input: Some(input),
            output,
            id: 0,
        };
        let init = session.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "lazyos", "{init}");
        session.notify("notifications/initialized");
        session
    }
}

struct Session {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    id: u64,
}

impl Session {
    fn send(&mut self, message: Value) {
        let input = self.input.as_mut().expect("the session is open");
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        self.send(json!({"jsonrpc": "2.0", "id": self.id, "method": method, "params": params}));
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        let answer: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line:?}"));
        assert_eq!(
            answer["id"], self.id,
            "answers come back in order: {answer}"
        );
        answer
    }

    fn notify(&mut self, method: &str) {
        self.send(json!({"jsonrpc": "2.0", "method": method}));
    }

    fn call(&mut self, name: &str, args: Value) -> Value {
        self.request("tools/call", json!({"name": name, "arguments": args}))
    }

    /// A successful call's text.
    fn ok(&mut self, name: &str, args: Value) -> Value {
        let answer = self.call(name, args.clone());
        assert_eq!(
            answer["result"]["isError"], false,
            "{name} {args}: {answer}"
        );
        answer["result"].clone()
    }

    fn close(mut self) -> std::process::ExitStatus {
        drop(self.input.take());
        wait(&mut self.child, Duration::from_secs(5)).expect("the server exits when its input ends")
    }
}

fn wait(child: &mut Child, timeout: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

fn wait_for(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    done()
}

fn text(result: &Value) -> String {
    result["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

// ---------------------------------------------------------------------------

#[test]
fn a_scripted_session_lists_the_table_reads_and_refuses() {
    let desk = Desk::new("session");
    let mut s = desk.start();

    let listed = s.request("tools/list", json!({}));
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    let table: Vec<&str> = TOOLS.iter().map(|t| t.name).collect();
    assert_eq!(names, table, "tools/list is the manifest");

    let status = s.ok("desktop_status", json!({}));
    assert_eq!(status["structuredContent"]["desktop"], true);
    assert!(
        status["structuredContent"]["memory_mib"]["total"]
            .as_u64()
            .unwrap()
            > 0
    );

    let windows = s.ok("desktop_windows", json!({}));
    assert_eq!(windows["structuredContent"]["windows"][0]["app"], "foot");
    assert_eq!(windows["structuredContent"]["windows"][0]["workspace"], "2");

    let packages = s.ok("packages_installed", json!({"filter": "SWAY"}));
    assert_eq!(
        packages["structuredContent"]["installed"], 2,
        "rc is not installed"
    );
    assert_eq!(packages["structuredContent"]["packages"][0]["name"], "sway");

    let report = s.ok("system_report", json!({}));
    assert_eq!(report["structuredContent"]["counts"]["fail"], 1);
    assert_eq!(
        report["structuredContent"]["fail"][0], "a failure",
        "ANSI is gone"
    );
    assert_eq!(report["structuredContent"]["exit"], 1);

    assert_eq!(
        s.ok("clipboard_read", json!({}))["structuredContent"]["text"],
        "copied text"
    );
    assert_eq!(
        s.ok("task_list", json!({}))["structuredContent"]["tasks"],
        json!([])
    );

    // Refused: an error naming the way that exists, logged, and the server lives.
    let refused = s.call("format_apply", json!({"device": "/dev/mmcblk0"}));
    assert_eq!(refused["error"]["code"], -32602);
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("format_propose"),
        "{refused}"
    );
    let unknown = s.call("rm_everything", json!({}));
    assert_eq!(unknown["error"]["code"], -32602);
    assert_eq!(s.request("ping", json!({}))["result"], json!({}));

    // Resources: the manual a person reads, by name only.
    let resources = s.request("resources/list", json!({}));
    let uris: Vec<&str> = resources["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    assert!(uris.contains(&"lazyos://manual/07-agents.md"), "{uris:?}");
    let page = s.request(
        "resources/read",
        json!({"uri": "lazyos://manual/07-agents.md"}),
    );
    assert!(
        page["result"]["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Agents")
    );
    let escape = s.request(
        "resources/read",
        json!({"uri": "lazyos://manual/../../../../etc/passwd"}),
    );
    assert_eq!(
        escape["error"]["code"], -32002,
        "no path is built from a URI"
    );

    assert!(s.close().success());
    let log = desk.log();
    for (tool, outcome) in [
        ("desktop_status", "\tread\tok\t"),
        ("format_apply", "\trefuse\trefused\t"),
        ("rm_everything", "\t-\tunknown\t"),
    ] {
        assert!(
            log.lines()
                .any(|l| l.contains(&format!("\t{tool}{outcome}"))),
            "{tool}: {log}"
        );
    }
    assert!(
        !log.contains("mmcblk0"),
        "arguments never reach the log: {log}"
    );
    let mode = fs::metadata(desk.root.join("state/lazyos/mcp.log"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "the log is the person's");
}

#[test]
fn every_tool_in_the_table_has_an_implementation() {
    let desk = Desk::new("every");
    let mut s = desk.start();
    for tool in TOOLS {
        let args = match tool.name {
            "task_open" | "task_stop" => json!({"id": "no-such-task"}),
            "theme_set" => json!({"name": "no-such-theme"}),
            "notify_send" => json!({"title": "t"}),
            "view_open" => json!({"path": "/"}),
            "power_propose" => json!({"action": "logout"}),
            _ => json!({}),
        };
        let answer = s.call(tool.name, args);
        assert!(answer.get("result").is_some(), "{}: {answer}", tool.name);
        assert!(
            !text(&answer["result"]).contains("no implementation"),
            "{} has no arm in Desk::call",
            tool.name
        );
    }
    s.close();
}

#[test]
fn a_propose_tool_opens_the_gated_window_and_nothing_else() {
    let desk = Desk::new("propose");
    let mut s = desk.start();
    for (tool, window) in [
        ("install_propose", "rofi-install"),
        ("format_propose", "rofi-format"),
        ("swap_propose", "rofi-swap"),
    ] {
        let result = s.ok(tool, json!({}));
        assert!(text(&result).starts_with("proposed:"), "{tool}: {result}");
        assert!(
            wait_for(Duration::from_secs(3), || desk
                .opened()
                .iter()
                .any(|l| l == window)),
            "{tool} opened {:?}",
            desk.opened()
        );
    }
    let result = s.ok("power_propose", json!({"action": "reboot"}));
    assert!(text(&result).contains("Cancel selected"), "{result}");
    assert!(
        wait_for(Duration::from_secs(3), || desk
            .opened()
            .iter()
            .any(|l| l.starts_with("foot ")
                && l.contains("[--app-id=lazyos-pick]")
                && l.ends_with("[menu] [--power] [reboot]"))),
        "power_propose opens the framework's Power confirm: {:?}",
        desk.opened()
    );
    // Opened, never applied: no window was asked for anything but the gate, and
    // a second proposal waits for the person instead of stacking.
    assert_eq!(desk.opened().len(), 4, "{:?}", desk.opened());
    let again = s.call("power_propose", json!({"action": "reboot"}));
    assert_eq!(again["result"]["isError"], true);
    assert!(text(&again["result"]).contains("still be open"));
    s.close();
}

#[test]
fn act_tools_open_the_window_a_person_would() {
    let desk = Desk::new("act");
    let mut s = desk.start();
    s.ok("menu_open", json!({"filter": "wifi"}));
    assert!(
        wait_for(Duration::from_secs(3), || desk
            .opened()
            .iter()
            .any(|l| l.ends_with("[menu] [--filter] [wifi]"))),
        "{:?}",
        desk.opened()
    );
    let file = desk.home().join("notes.txt");
    fs::write(&file, "hi").unwrap();
    s.ok("view_open", json!({"path": "~/notes.txt"}));
    s.ok(
        "view_open",
        json!({"path": desk.home().display().to_string()}),
    );
    let canonical = file.canonicalize().unwrap().display().to_string();
    let home = desk.home().canonicalize().unwrap().display().to_string();
    assert!(
        wait_for(Duration::from_secs(3), || {
            let opened = desk.opened();
            opened.contains(&format!("lazyos-view [{canonical}]"))
                && opened.contains(&format!("rofi-files [--] [{home}]"))
        }),
        "{:?}",
        desk.opened()
    );
    let missing = s.call("view_open", json!({"path": "/no/such/file"}));
    assert_eq!(missing["result"]["isError"], true);
    let relative = s.call("view_open", json!({"path": "notes.txt"}));
    assert!(text(&relative["result"]).contains("absolute"));

    // notify: a title that looks like a flag is sent as text.
    s.ok(
        "notify_send",
        json!({"title": "--icon", "body": "--urgency critical"}),
    );
    let sent = desk.opened();
    let card = sent
        .iter()
        .find(|l| l.starts_with("notify-send"))
        .expect("a card was sent");
    assert!(card.contains("[\u{2010}-icon]"), "{card}");
    assert!(
        card.contains("[--urgency critical]"),
        "the body arrives whole: {card}"
    );

    let bad = s.call("theme_set", json!({"name": "../../etc"}));
    assert_eq!(bad["result"]["isError"], true);
    s.close();
}

#[test]
fn task_stop_only_stops_what_says_it_can_be_stopped() {
    let desk = Desk::new("tasks");
    let state = desk.root.join("state/lazyos");
    // Records written by this process: its pid is alive, so a Running record
    // stays Running when the server loads it.
    for (id, stoppable) in [("scan-1", true), ("install-1", false)] {
        let mut task = lazyos_ui::Task::new(id, id, &state).with_command(&["true"]);
        task.set_stoppable(stoppable);
        task.write_state().unwrap();
        let path = state.join(format!("tasks/{id}.json"));
        let mut record: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        record["state"] = json!("Running");
        record["started_at"] = json!(1_700_000_000u64);
        fs::write(&path, record.to_string()).unwrap();
    }
    // A finished task says the same capability in its record, and the list must
    // not offer it as something to stop (measured on the device: every Done
    // task came back with `stoppable: true`).
    {
        let mut task = lazyos_ui::Task::new("ask-1", "ask-1", &state).with_command(&["true"]);
        task.set_stoppable(true);
        task.write_state().unwrap();
        let path = state.join("tasks/ask-1.json");
        let mut record: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        record["state"] = json!("Done");
        fs::write(&path, record.to_string()).unwrap();
    }
    let mut s = desk.start();
    let tasks = s.ok("task_list", json!({}));
    assert_eq!(
        tasks["structuredContent"]["tasks"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let listed = |id: &str| {
        tasks["structuredContent"]["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == json!(id))
            .cloned()
            .unwrap()
    };
    assert_eq!(listed("scan-1")["stoppable"], json!(true));
    assert_eq!(listed("install-1")["stoppable"], json!(false));
    assert_eq!(
        listed("ask-1")["stoppable"],
        json!(false),
        "a task that is done is not a task to stop"
    );
    let stopped = s.ok("task_stop", json!({"id": "scan-1"}));
    assert!(text(&stopped).contains("stop requested"), "{stopped}");
    assert!(
        state.join("tasks/scan-1.stop").is_file(),
        "the runner's stop request is written"
    );
    let refused = s.call("task_stop", json!({"id": "install-1"}));
    assert_eq!(refused["result"]["isError"], true);
    assert!(text(&refused["result"]).contains("cannot be stopped"));
    assert!(!state.join("tasks/install-1.stop").exists());
    let traversal = s.call("task_stop", json!({"id": "../../x"}));
    assert_eq!(traversal["result"]["isError"], true);
    s.close();
}

#[test]
fn a_screenshot_leaves_nothing_after_the_client_disconnects() {
    let desk = Desk::new("shot-eof");
    let mut s = desk.start();
    let pid = s.child.id();
    let shot = s.ok("desktop_screenshot", json!({}));
    let path = PathBuf::from(shot["structuredContent"]["path"].as_str().unwrap());
    assert_eq!(shot["structuredContent"]["width"], 1366);
    assert_eq!(shot["structuredContent"]["height"], 912);
    assert!(
        path.starts_with(desk.shots(pid)),
        "under $XDG_RUNTIME_DIR, in the server's own directory"
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(desk.shots(pid)).unwrap().permissions().mode() & 0o777,
        0o700
    );
    // Five shots, four slots: the oldest is overwritten, never a fifth file.
    for _ in 0..4 {
        s.ok("desktop_screenshot", json!({}));
    }
    assert_eq!(fs::read_dir(desk.shots(pid)).unwrap().count(), 4);
    assert!(
        !desk.home().join("Pictures").exists(),
        "nothing is written into $HOME"
    );
    assert!(s.close().success());
    assert!(
        !desk.shots(pid).exists(),
        "the end of input removes every screenshot"
    );
}

#[test]
fn a_screenshot_leaves_nothing_after_sigterm_and_a_killed_server_is_swept() {
    let desk = Desk::new("shot-term");
    let mut s = desk.start();
    let pid = s.child.id();
    s.ok("desktop_screenshot", json!({}));
    assert!(desk.shots(pid).is_dir());
    // SAFETY: a signal to the child this test started.
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let status = wait(&mut s.child, Duration::from_secs(5)).expect("SIGTERM ends the server");
    assert_eq!(
        std::os::unix::process::ExitStatusExt::into_raw(status) >> 8,
        128 + libc::SIGTERM
    );
    assert!(
        !desk.shots(pid).exists(),
        "the signal handler removed the screenshots"
    );

    // SIGKILL leaves the directory; the next server sweeps it.
    let mut s = desk.start();
    let pid = s.child.id();
    s.ok("desktop_screenshot", json!({}));
    s.child.kill().unwrap();
    s.child.wait().unwrap();
    assert!(
        desk.shots(pid).is_dir(),
        "nothing can clean up after SIGKILL"
    );
    let next = desk.start();
    assert!(
        !desk.shots(pid).exists(),
        "a dead server's screenshots are swept at start"
    );
    next.close();
}

#[test]
fn without_a_desktop_the_act_tools_say_so() {
    let desk = Desk::new("no-wayland");
    let mut child = Command::new(bin())
        .arg("mcp")
        .env_clear()
        .envs(
            desk.env()
                .into_iter()
                .filter(|(k, _)| k != "WAYLAND_DISPLAY"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for (id, name) in [(1, "menu_open"), (2, "desktop_screenshot")] {
        writeln!(input, "{}", json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":{}}})).unwrap();
    }
    drop(input);
    let out = child.wait_with_output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
    for line in text.lines() {
        let answer: Value = serde_json::from_str(line).unwrap();
        assert_eq!(answer["result"]["isError"], true, "{answer}");
        assert!(line.contains("WAYLAND_DISPLAY"), "{line}");
    }
    assert!(desk.opened().is_empty());
}

// ---------------------------------------------------------------------------
// The table against the binary and the image
// ---------------------------------------------------------------------------

#[test]
fn every_act_has_a_subcommand() {
    let bin_dir = repo().join("uis/sway/files/home/.config/bin");
    for tool in TOOLS {
        match tool.maps {
            Maps::Lazyos(words) => {
                let args: Vec<String> = words.iter().map(|w| w.to_string()).collect();
                let resolved = lazyos_cli::command::resolve("lazyos", &args).unwrap_or_else(|e| {
                    panic!(
                        "{}: `lazyos {}` resolves to nothing: {e:?}",
                        tool.name,
                        words.join(" ")
                    )
                });
                // What follows the command is a verb of it (`theme current`) or
                // a flag (`menu --power`); a verb must be one the command's own
                // usage names, so a typo in the table is a failure here.
                let path_words = resolved.spec.path.split(' ').count();
                let verbs: Vec<&str> = words[path_words..]
                    .iter()
                    .copied()
                    .filter(|w| !w.starts_with("--"))
                    .collect();
                if !verbs.is_empty() {
                    let mut help: Vec<&str> = resolved.spec.path.split(' ').collect();
                    help.push("--help");
                    let out = Command::new(bin())
                        .args(&help)
                        .env_remove("WAYLAND_DISPLAY")
                        .output()
                        .unwrap();
                    let usage = format!(
                        "{}{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    for verb in verbs {
                        assert!(
                            usage.contains(verb),
                            "{}: `lazyos {}` has no verb `{verb}`:\n{usage}",
                            tool.name,
                            resolved.spec.path
                        );
                    }
                }
            }
            Maps::Session(name) => {
                let path = bin_dir.join(name);
                // A ported script is a link to the binary (spec 007, 009): it
                // is real when it points there and the table resolves its name.
                if let Ok(target) = fs::read_link(&path) {
                    assert_eq!(
                        target,
                        Path::new("/usr/bin/lazyos"),
                        "{}: {name} links somewhere else",
                        tool.name
                    );
                    assert!(
                        lazyos_cli::command::COMMANDS
                            .iter()
                            .any(|spec| spec.installed.contains(&name)),
                        "{}: {name} is a link the command table does not install",
                        tool.name
                    );
                    continue;
                }
                let mode = fs::metadata(&path)
                    .unwrap_or_else(|_| {
                        panic!("{}: {} is not in the image", tool.name, path.display())
                    })
                    .permissions()
                    .mode();
                assert!(mode & 0o111 != 0, "{}: {name} is not executable", tool.name);
            }
            Maps::Program(_) | Maps::Internal => assert_eq!(
                tool.tier,
                Tier::Read,
                "{}: only a read tool may answer from a reader or in-process; an act or a propose runs what the menu runs",
                tool.name
            ),
        }
    }
}

#[test]
fn the_shipped_registration_and_instructions_match_the_binary() {
    let config = repo().join("uis/sway/files/home/.config/opencode");
    let json: Value =
        serde_json::from_str(&fs::read_to_string(config.join("opencode.json")).unwrap()).unwrap();
    assert_eq!(json["mcp"]["lazyos"]["type"], "local");
    assert_eq!(json["mcp"]["lazyos"]["command"], json!(["lazyos", "mcp"]));
    assert_eq!(json["mcp"]["lazyos"]["enabled"], true);
    let instructions = fs::read_to_string(config.join("AGENTS.md")).unwrap();
    let pinned = instructions
        .lines()
        .find_map(|l| l.strip_prefix("lazyos-mcp manifest: "))
        .expect("the instructions pin a manifest version");
    assert_eq!(
        pinned.trim(),
        manifest::version(),
        "the instructions describe another manifest: update ~/.config/opencode/AGENTS.md with the table"
    );
    for tool in TOOLS {
        assert!(
            instructions.contains(tool.name),
            "the instructions do not mention {}",
            tool.name
        );
    }
    // `--tools` and `--version` from the real binary say the same.
    let tools = Command::new(bin())
        .args(["mcp", "--tools"])
        .output()
        .unwrap();
    assert!(tools.status.success());
    assert_eq!(String::from_utf8(tools.stdout).unwrap(), manifest::text());
    let version = Command::new(bin())
        .args(["mcp", "--version"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        pinned.trim()
    );
}
