//! `lazyos mcp`: the desktop's tools for an agent (spec 008).
//!
//! The protocol, the tool table and the argument checks are `lazyos-mcp`; this
//! module is the [`Handler`] behind them, and it does every action the way the
//! menus do it - the same program, the same words - so a tool is never a second
//! implementation of something a person can do:
//!
//! - **read** tools read procfs/sysfs, the task records, or run a reader
//!   (`grim`, `swaymsg`, `dpkg-query`, `wl-paste`, `check-system.sh`) with a
//!   timeout and their output captured;
//! - **act** tools run this binary's subcommands (`theme set`, `notify send`,
//!   `task stop`) or open a window detached from the server (`lazyos menu`,
//!   `lazyos task attach`, `lazyos-view`);
//! - **propose** tools open the gated window the menu opens (`rofi-install`,
//!   `rofi-format`, `rofi-swap`, `lazyos menu --power ACTION`) and return; what
//!   the person answers there is never seen here.
//!
//! Nothing runs through a shell: every program is a fixed name with its words.
//!
//! **Screenshots are the one thing the session leaves on disk**, so their
//! lifetime is this process's: a `0700` directory of its own under
//! `$XDG_RUNTIME_DIR`, at most four files in fixed slots, removed at the end
//! of input, on SIGTERM/SIGINT/SIGHUP (a handler that only unlinks the fixed
//! paths and exits - all async-signal-safe), and by the next server when this
//! one was killed outright.

use std::collections::BTreeMap;
use std::ffi::CString;
use std::io::{self, IsTerminal, Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use lazyos_mcp::server::Resource;
use lazyos_mcp::{CallLog, Handler, Reply, Server, ToolSpec, manifest, serve};
use lazyos_ui::Task;
use serde_json::{Map, Value, json};

use crate::command::Resolved;
use crate::version;

const EXIT_OK: i32 = 0;
const EXIT_USAGE: i32 = 2;
const EXIT_FAILED: i32 = 3;

/// The screenshot slots: a fifth screenshot overwrites the first. Fixed names
/// are what lets a signal handler remove them without allocating.
const SHOT_SLOTS: usize = 4;

pub fn run(resolved: &Resolved) -> i32 {
    match resolved.args.as_slice() {
        [] => serve_stdio(),
        [flag] if flag == "--tools" => {
            let _ = io::stdout().write_all(manifest::text().as_bytes());
            EXIT_OK
        }
        [flag] if flag == "--version" => {
            println!("{}", manifest::version());
            EXIT_OK
        }
        _ => {
            eprintln!(
                "usage: lazyos mcp            serve the desktop's tools on stdin/stdout (an MCP client starts this)"
            );
            eprintln!(
                "       lazyos mcp --tools    the manifest: tier, name, schema hash, limit, what it runs"
            );
            eprintln!(
                "       lazyos mcp --version  the manifest version the instructions file pins"
            );
            EXIT_USAGE
        }
    }
}

fn serve_stdio() -> i32 {
    let vars: BTreeMap<String, String> = std::env::vars().collect();
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "lazyos".to_string());
    let desk = match Desk::new(vars, exe) {
        Ok(desk) => desk,
        Err(error) => {
            eprintln!("lazyos mcp: {error}");
            return EXIT_FAILED;
        }
    };
    desk.sweep_stale();
    desk.install_cleanup();
    if io::stdin().is_terminal() {
        eprintln!(
            "lazyos mcp: speaking MCP (JSON-RPC lines) on stdin and stdout; an agent starts this. Ctrl-D ends it."
        );
    }
    let log = CallLog::new(Some(desk.state.join("mcp.log")));
    let mut server = Server::new(desk, log, version::version());
    let stdin = io::stdin();
    let stdout = io::stdout();
    let result = serve(&mut server, &mut stdin.lock(), &mut stdout.lock());
    // serve() cleans up at the end of input; a write error (the client went
    // away mid-answer) returns before that, so finish again - it is idempotent.
    server.finish();
    match result {
        Ok(()) => EXIT_OK,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => EXIT_OK,
        Err(error) => {
            eprintln!("lazyos mcp: {error}");
            EXIT_FAILED
        }
    }
}

/// The desktop, as the tools see it.
pub struct Desk {
    vars: BTreeMap<String, String>,
    exe: String,
    home: PathBuf,
    /// `~/.local/state/lazyos`: the task records and `mcp.log`.
    state: PathBuf,
    doc_dir: PathBuf,
    shots: Option<PathBuf>,
    next_shot: usize,
}

impl Desk {
    pub fn new(vars: BTreeMap<String, String>, exe: String) -> Result<Self, String> {
        let paths = crate::paths::Paths::resolve(&vars).map_err(|e| e.to_string())?;
        let home = PathBuf::from(vars.get("HOME").cloned().unwrap_or_default());
        let shots = vars
            .get("XDG_RUNTIME_DIR")
            .filter(|v| !v.is_empty())
            .map(|dir| PathBuf::from(dir).join(format!("lazyos-mcp-{}", std::process::id())));
        // The manual a person reads on the device; a checkout's copy for tests.
        let doc_dir = vars
            .get("LAZYOS_DOC_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/usr/share/doc/lazyos"));
        Ok(Self {
            vars,
            exe,
            home,
            state: paths.state,
            doc_dir,
            shots,
            next_shot: 0,
        })
    }

    fn var(&self, key: &str) -> Option<&str> {
        self.vars
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    fn session_bin(&self, name: &str) -> PathBuf {
        crate::menu::session_bin(&self.vars).join(name)
    }

    /// A screenshot directory whose server died without cleaning up (SIGKILL,
    /// a crash): remove it when its pid is gone.
    fn sweep_stale(&self) {
        let Some(runtime) = self.var("XDG_RUNTIME_DIR") else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(runtime) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = name
                .to_str()
                .and_then(|n| n.strip_prefix("lazyos-mcp-"))
                .and_then(|p| p.parse::<i32>().ok())
            else {
                continue;
            };
            // SAFETY: signal 0 only asks whether the process exists.
            let gone = unsafe { libc::kill(pid, 0) } != 0
                && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
            if gone {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }

    /// Register the screenshot paths with the signal handler.
    fn install_cleanup(&self) {
        let Some(dir) = &self.shots else {
            return;
        };
        let mut paths: Vec<CString> = (0..SHOT_SLOTS)
            .filter_map(|slot| {
                CString::new(shot_path(dir, slot).into_os_string().into_encoded_bytes()).ok()
            })
            .collect();
        if let Ok(dir) = CString::new(dir.clone().into_os_string().into_encoded_bytes()) {
            paths.push(dir);
        }
        if CLEANUP.set(paths).is_ok() {
            for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
                // SAFETY: the handler only calls unlink, rmdir and _exit.
                unsafe {
                    libc::signal(signal, on_signal as *const () as libc::sighandler_t);
                }
            }
        }
    }

    fn remove_shots(&self) {
        if let Some(dir) = &self.shots {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    // -- running things ------------------------------------------------------

    /// Open a window for the person: detached (its own session, no stdio), so
    /// it outlives nothing it should not and the server's pipes stay clean.
    /// `LAZYOS_UI` is removed, or a `lazyos` child would think it is already
    /// the window and look for a terminal on a pipe.
    fn open_window(&self, argv: &[String], what: &str) -> Result<(), String> {
        if self.var("WAYLAND_DISPLAY").is_none() {
            return Err(format!(
                "cannot open {what}: this session has no desktop (WAYLAND_DISPLAY is unset - an ssh or TTY session?)"
            ));
        }
        use std::os::unix::process::CommandExt;
        let (program, args) = argv.split_first().ok_or("an empty command")?;
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(&self.home)
            .env_remove("LAZYOS_UI")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: setsid is async-signal-safe, the only call between fork and exec.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("cannot start {what}: {e}"))?;
        // A window that is going to fail usually fails at once (no foot, a
        // script that is not executable); a working one is still running, or
        // has handed itself to foot and exited 0.
        std::thread::sleep(Duration::from_millis(150));
        match child.try_wait() {
            Ok(Some(status)) if !status.success() => {
                Err(format!("{what} closed at once ({status})"))
            }
            _ => Ok(()),
        }
    }

    fn lazyos(&self, words: &[&str], timeout: Duration) -> Result<Captured, String> {
        capture(&self.exe, words, timeout)
    }

    // -- the tools -------------------------------------------------------------

    fn desktop_status(&self) -> Reply {
        let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let mem = |key: &str| -> u64 {
            meminfo
                .lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix(':'))
                .and_then(|v| v.split_whitespace().next()?.parse().ok())
                .unwrap_or(0)
                / 1024
        };
        let uptime = std::fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|t| t.split_whitespace().next()?.parse::<f64>().ok())
            .unwrap_or(0.0);
        let load: Vec<f64> = std::fs::read_to_string("/proc/loadavg")
            .unwrap_or_default()
            .split_whitespace()
            .take(3)
            .filter_map(|v| v.parse().ok())
            .collect();
        let mut disks = vec![disk("/")];
        if self.home.is_dir() && device_of(&self.home) != device_of(Path::new("/")) {
            disks.push(disk(&self.home.display().to_string()));
        }
        let theme = lazyos_theme::store::active(&crate::theme::paths());
        Reply::data(json!({
            "host": read_trim("/proc/sys/kernel/hostname"),
            "lazyos": version::version(),
            "kernel": read_trim("/proc/sys/kernel/osrelease"),
            "uptime_minutes": (uptime / 60.0).round() as u64,
            "load": load,
            "memory_mib": {
                "total": mem("MemTotal"),
                "available": mem("MemAvailable"),
                "swap_total": mem("SwapTotal"),
                "swap_free": mem("SwapFree"),
            },
            "battery": batteries(),
            "disk": disks.into_iter().flatten().collect::<Vec<_>>(),
            "theme": theme,
            "desktop": self.var("WAYLAND_DISPLAY").is_some(),
        }))
    }

    fn screenshot(&mut self, args: &Map<String, Value>) -> Result<Reply, String> {
        if self.var("WAYLAND_DISPLAY").is_none() {
            return Err("no desktop to capture: WAYLAND_DISPLAY is unset".to_string());
        }
        let Some(dir) = self.shots.clone() else {
            return Err(
                "no $XDG_RUNTIME_DIR: a screenshot would have to go somewhere less private, so none is taken"
                    .to_string(),
            );
        };
        if !dir.is_dir() {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        let slot = self.next_shot % SHOT_SLOTS;
        self.next_shot += 1;
        let path = shot_path(&dir, slot);
        let path_text = path.display().to_string();
        let mut words: Vec<&str> = Vec::new();
        let output = args.get("output").and_then(Value::as_str);
        if let Some(output) = output {
            if !output
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
            {
                return Err(format!("`{output}` is not an output name (eDP-1, DP-1)"));
            }
            words.extend(["-o", output]);
        }
        words.extend(["-t", "png", path_text.as_str()]);
        let shot = capture("grim", &words, Duration::from_secs(10))?;
        if !shot.ok() {
            return Err(format!(
                "grim did not capture the screen: {}",
                shot.reason()
            ));
        }
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        let (width, height) = png_size(&path).unwrap_or((0, 0));
        Ok(Reply {
            text: format!(
                "{path_text} ({width}x{height} PNG, private to this session; it is deleted when the session ends)"
            ),
            structured: Some(json!({"path": path_text, "width": width, "height": height})),
        })
    }

    fn windows(&self) -> Result<Reply, String> {
        let tree = capture("swaymsg", &["-t", "get_tree", "-r"], Duration::from_secs(5))?;
        if !tree.ok() {
            return Err(format!(
                "swaymsg could not read the window tree: {}",
                tree.reason()
            ));
        }
        let root: Value = serde_json::from_str(&tree.stdout)
            .map_err(|e| format!("swaymsg answered something that is not JSON: {e}"))?;
        let mut windows = Vec::new();
        collect_windows(&root, None, false, &mut windows);
        Ok(Reply::data(json!({"windows": windows})))
    }

    fn theme_current(&self) -> Result<Reply, String> {
        let current = self.lazyos(&["theme", "current"], Duration::from_secs(10))?;
        let line = current.stdout.lines().next().unwrap_or("").to_string();
        let (slug, name) = line.split_once('\t').unwrap_or((line.as_str(), ""));
        if slug.is_empty() {
            return Err("no active theme (`lazyos theme set solarized` sets one)".to_string());
        }
        Ok(Reply::data(json!({"slug": slug, "name": name})))
    }

    fn packages(&self, args: &Map<String, Value>) -> Result<Reply, String> {
        let filter = args
            .get("filter")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_lowercase();
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(200) as usize;
        let list = capture(
            "dpkg-query",
            &["-W", "-f", "${db:Status-Abbrev}\t${Package}\t${Version}\n"],
            Duration::from_secs(20),
        )?;
        if !list.ok() {
            return Err(format!("dpkg-query failed: {}", list.reason()));
        }
        let installed: Vec<(&str, &str)> = list
            .stdout
            .lines()
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let status = fields.next()?;
                let (name, version) = (fields.next()?, fields.next()?);
                status.starts_with("ii").then_some((name, version))
            })
            .collect();
        let matched: Vec<Value> = installed
            .iter()
            .filter(|(name, _)| filter.is_empty() || name.to_lowercase().contains(&filter))
            .map(|(name, version)| json!({"name": name, "version": version}))
            .collect();
        Ok(Reply::data(json!({
            "installed": installed.len(),
            "matched": matched.len(),
            "packages": matched.into_iter().take(limit).collect::<Vec<_>>(),
        })))
    }

    fn system_report(&self) -> Result<Reply, String> {
        let script = which(self.var("PATH").unwrap_or(""), "check-system.sh")
            .or_else(|| {
                Some(PathBuf::from("/usr/local/sbin/check-system.sh")).filter(|p| p.is_file())
            })
            .ok_or("check-system.sh is not installed (/usr/local/sbin/check-system.sh)")?;
        let report = capture(&script.display().to_string(), &[], Duration::from_secs(120))?;
        let text = strip_ansi(&report.stdout);
        let mut counts = BTreeMap::new();
        let (mut fails, mut warns) = (Vec::new(), Vec::new());
        for line in text.lines() {
            let trimmed = line.trim_start();
            for (mark, list) in [
                ("FAIL", Some(&mut fails)),
                ("WARN", Some(&mut warns)),
                ("OK", None),
                ("SKIP", None),
            ] {
                if let Some(rest) = trimmed.strip_prefix(mark)
                    && rest.starts_with(' ')
                {
                    *counts.entry(mark.to_lowercase()).or_insert(0u32) += 1;
                    if let Some(list) = list
                        && list.len() < 60
                    {
                        list.push(rest.trim().to_string());
                    }
                }
            }
        }
        Ok(Reply::data(json!({
            "exit": report.status,
            "counts": counts,
            "fail": fails,
            "warn": warns,
        })))
    }

    fn clipboard(&self) -> Result<Reply, String> {
        if self.var("WAYLAND_DISPLAY").is_none() {
            return Err("no desktop clipboard: WAYLAND_DISPLAY is unset".to_string());
        }
        let paste = capture(
            "wl-paste",
            &["--no-newline", "--type", "text"],
            Duration::from_secs(3),
        )?;
        if !paste.ok() {
            return Ok(Reply::data(
                json!({"text": null, "note": "the clipboard holds no text"}),
            ));
        }
        const CAP: usize = 64 * 1024;
        let mut text = paste.stdout;
        let truncated = text.len() > CAP;
        if truncated {
            let mut cut = CAP;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
        }
        Ok(Reply::data(json!({"text": text, "truncated": truncated})))
    }

    fn task_list(&self) -> Reply {
        let dir = self.state.join("tasks");
        let mut tasks: Vec<(u64, Value)> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry
                    .file_name()
                    .to_str()?
                    .strip_suffix(".json")?
                    .to_string();
                let task = Task::load(&name, &self.state).ok()??;
                let started = task
                    .started_at
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                Some((
                    started,
                    json!({
                        "id": task.id,
                        "title": task.title,
                        "state": format!("{:?}", task.state).to_lowercase(),
                        "stoppable": task.stoppable,
                        "result": task.error_line.clone().or(task.result.clone()),
                    }),
                ))
            })
            .collect();
        tasks.sort_by_key(|task| std::cmp::Reverse(task.0));
        let tasks: Vec<Value> = tasks.into_iter().take(20).map(|(_, t)| t).collect();
        Reply::data(json!({"tasks": tasks}))
    }

    fn menu_open(&self, args: &Map<String, Value>) -> Result<Reply, String> {
        let mut argv = vec![self.exe.clone(), "menu".to_string()];
        let palette = args.get("surface").and_then(Value::as_str) == Some("palette");
        if palette {
            argv.push("palette".to_string());
        }
        let filter = args
            .get("filter")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if !filter.is_empty() {
            argv.extend(["--filter".to_string(), filter.to_string()]);
        }
        let what = if palette {
            "the command palette"
        } else {
            "the root menu"
        };
        self.open_window(&argv, what)?;
        Ok(Reply::text(if filter.is_empty() {
            format!("{what} is open in front of the person")
        } else {
            format!(
                "{what} is open with `{filter}` typed; the person presses Enter on the row they want"
            )
        }))
    }

    fn task_id(args: &Map<String, Value>) -> Result<String, String> {
        let id = args.get("id").and_then(Value::as_str).unwrap_or("");
        if !crate::task_cmd::valid_id(id) {
            return Err(format!("`{id}` is not a task id (task_list names them)"));
        }
        Ok(id.to_string())
    }

    fn task_open(&self, args: &Map<String, Value>) -> Result<Reply, String> {
        let id = Self::task_id(args)?;
        if !self
            .state
            .join("tasks")
            .join(format!("{id}.json"))
            .is_file()
        {
            return Err(format!("no task `{id}` (task_list names them)"));
        }
        self.open_window(
            &[self.exe.clone(), "task".into(), "attach".into(), id.clone()],
            "the task window",
        )?;
        Ok(Reply::text(format!("the window of task `{id}` is open")))
    }

    fn task_stop(&self, args: &Map<String, Value>) -> Result<Reply, String> {
        let id = Self::task_id(args)?;
        let stop = self.lazyos(&["task", "stop", &id], Duration::from_secs(10))?;
        match stop.status {
            Some(0) | Some(1) => Ok(Reply::text(stop.stdout.trim().to_string())),
            _ => Err(stop.reason()),
        }
    }

    fn theme_set(&self, args: &Map<String, Value>) -> Result<Reply, String> {
        let name = args.get("name").and_then(Value::as_str).unwrap_or("");
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            || name.starts_with('-')
        {
            return Err(format!(
                "`{name}` is not a theme name (lowercase letters, digits and -)"
            ));
        }
        let set = self.lazyos(&["theme", "set", name], Duration::from_secs(60))?;
        let said = strip_ansi(&format!("{}{}", set.stdout, set.stderr))
            .trim()
            .to_string();
        match set.status {
            Some(0) => Ok(Reply::text(format!(
                "the desktop is re-themed with {name}\n{said}"
            ))),
            Some(1) => Ok(Reply::text(format!("nothing was changed\n{said}"))),
            _ => Err(last_lines(&said, 4)),
        }
    }

    fn notify(&self, args: &Map<String, Value>) -> Result<Reply, String> {
        let title = args.get("title").and_then(Value::as_str).unwrap_or("");
        // A title is a positional word, so one that starts with `--` would be
        // read as a flag: the same character from the Unicode hyphen block
        // reads the same and parses as text.
        let title = match title.strip_prefix('-') {
            Some(rest) => format!("\u{2010}{rest}"),
            None => title.to_string(),
        };
        let mut words: Vec<String> = vec!["notify".into(), "send".into(), title.clone()];
        if let Some(body) = args.get("body").and_then(Value::as_str) {
            words.extend(["--body".to_string(), body.to_string()]);
        }
        if let Some(icon) = args.get("icon").and_then(Value::as_str) {
            if !icon.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err(format!("`{icon}` is not an icon name"));
            }
            words.extend(["--icon".to_string(), icon.to_string()]);
        }
        if let Some(urgency) = args.get("urgency").and_then(Value::as_str) {
            words.extend(["--urgency".to_string(), urgency.to_string()]);
        }
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let sent = self.lazyos(&words, Duration::from_secs(10))?;
        if sent.ok() {
            Ok(Reply::text(format!(
                "the card \"{title}\" is on the screen"
            )))
        } else {
            Err(last_lines(&sent.reason(), 3))
        }
    }

    fn view_open(&self, args: &Map<String, Value>) -> Result<Reply, String> {
        let asked = args.get("path").and_then(Value::as_str).unwrap_or("");
        let expanded = match asked.strip_prefix("~/") {
            Some(rest) => self.home.join(rest),
            None if asked == "~" => self.home.clone(),
            None => PathBuf::from(asked),
        };
        if !expanded.is_absolute() {
            return Err(format!("`{asked}` is not an absolute path (or ~/...)"));
        }
        // Canonical, so the opener is handed an absolute path that cannot start
        // with `-` and that names what is really there.
        let path = expanded
            .canonicalize()
            .map_err(|e| format!("`{asked}` cannot be opened: {e}"))?;
        let meta = std::fs::metadata(&path).map_err(|e| format!("`{asked}`: {e}"))?;
        let shown = path.display().to_string();
        if meta.is_dir() {
            self.open_window(
                &[
                    self.session_bin("rofi-files").display().to_string(),
                    "--".into(),
                    shown.clone(),
                ],
                "the file list",
            )?;
            return Ok(Reply::text(format!("the file list is open at {shown}")));
        }
        if !meta.is_file() {
            return Err(format!("`{shown}` is not a file or a directory"));
        }
        self.open_window(
            &[
                self.session_bin("lazyos-view").display().to_string(),
                shown.clone(),
            ],
            "the viewer",
        )?;
        Ok(Reply::text(format!("{shown} is open in its viewer")))
    }

    fn propose(&self, script: &str, what: &str, gate: &str) -> Result<Reply, String> {
        let path = self.session_bin(script);
        let runnable = std::fs::metadata(&path)
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
        if !runnable {
            return Err(format!(
                "{what} is not installed here ({} is missing or not executable)",
                path.display()
            ));
        }
        self.open_window(&[path.display().to_string()], what)?;
        Ok(Reply::text(format!(
            "proposed: {what} is open in front of the person. {gate} Nothing happens unless they do, and you will not see their answer."
        )))
    }

    fn power(&self, args: &Map<String, Value>) -> Result<Reply, String> {
        let action = args.get("action").and_then(Value::as_str).unwrap_or("");
        let verb = match action {
            "logout" => "log out",
            "reboot" => "reboot",
            "poweroff" => "shut down",
            other => return Err(format!("`{other}` is not logout, reboot or poweroff")),
        };
        self.open_window(
            &[
                self.exe.clone(),
                "menu".into(),
                "--power".into(),
                action.to_string(),
            ],
            "the Power confirm",
        )?;
        Ok(Reply::text(format!(
            "proposed: the Power confirm asks the person whether to {verb}, with Cancel selected. Only their second answer ends the session; you will not see it."
        )))
    }

    // -- resources -------------------------------------------------------------

    fn manual_pages(&self) -> Vec<String> {
        let mut pages: Vec<String> = std::fs::read_dir(&self.doc_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(String::from))
            .filter(|n| n.ends_with(".md") && !n.starts_with('.'))
            .collect();
        pages.sort();
        pages
    }
}

impl Handler for Desk {
    fn call(
        &mut self,
        tool: &'static ToolSpec,
        args: &Map<String, Value>,
    ) -> Result<Reply, String> {
        match tool.name {
            "desktop_status" => Ok(self.desktop_status()),
            "desktop_screenshot" => self.screenshot(args),
            "desktop_windows" => self.windows(),
            "theme_current" => self.theme_current(),
            "packages_installed" => self.packages(args),
            "system_report" => self.system_report(),
            "clipboard_read" => self.clipboard(),
            "task_list" => Ok(self.task_list()),
            "menu_open" => self.menu_open(args),
            "task_open" => self.task_open(args),
            "task_stop" => self.task_stop(args),
            "theme_set" => self.theme_set(args),
            "notify_send" => self.notify(args),
            "view_open" => self.view_open(args),
            "install_propose" => self.propose(
                "rofi-install",
                "the Install LazyOS window",
                "It checks the machine and asks the person to type the disk's name.",
            ),
            "format_propose" => self.propose(
                "rofi-format",
                "the Format a disk menu",
                "It lists only removable disks this system does not use, and asks once more with Cancel first.",
            ),
            "swap_propose" => self.propose(
                "rofi-swap",
                "the Auxiliary swap menu",
                "The person picks the disk and the size, and confirms with Cancel first.",
            ),
            "power_propose" => self.power(args),
            // The table and this match are one list; a tool without an arm is a
            // bug the integration test calls every tool to find.
            other => Err(format!("{other} has no implementation in this binary")),
        }
    }

    fn resources(&mut self) -> Vec<Resource> {
        let mut out: Vec<Resource> = self
            .manual_pages()
            .into_iter()
            .map(|page| Resource {
                uri: format!("lazyos://manual/{page}"),
                description: format!("the LazyOS manual, {page} (what the person reads in Learn)"),
                name: page,
                mime: "text/markdown".to_string(),
            })
            .collect();
        out.push(Resource {
            uri: "lazyos://theme/current".to_string(),
            name: "the active theme".to_string(),
            description: "theme.toml of the palette every surface is generated from".to_string(),
            mime: "application/toml".to_string(),
        });
        out
    }

    fn read(&mut self, uri: &str) -> Option<(String, String)> {
        if let Some(page) = uri.strip_prefix("lazyos://manual/") {
            // Only a name the listing has: no path is ever built from the URI.
            if !self.manual_pages().iter().any(|p| p == page) {
                return None;
            }
            let text = std::fs::read_to_string(self.doc_dir.join(page)).ok()?;
            return Some(("text/markdown".to_string(), text));
        }
        if uri == "lazyos://theme/current" {
            let slug = lazyos_theme::store::active(&crate::theme::paths())
                .unwrap_or_else(|| "solarized".to_string());
            let text = lazyos_theme::builtin::get(&slug)?;
            return Some((
                "application/toml".to_string(),
                format!("# active theme: {slug}\n{text}"),
            ));
        }
        None
    }

    fn finish(&mut self) {
        self.remove_shots();
    }
}

// ---------------------------------------------------------------------------
// The signal half of the screenshot lifetime
// ---------------------------------------------------------------------------

static CLEANUP: OnceLock<Vec<CString>> = OnceLock::new();

extern "C" fn on_signal(signal: libc::c_int) {
    if let Some(paths) = CLEANUP.get()
        && let Some((dir, files)) = paths.split_last()
    {
        for file in files {
            // SAFETY: unlink and rmdir are async-signal-safe; the strings were
            // built before the handler was installed and are never freed.
            unsafe {
                libc::unlink(file.as_ptr());
            }
        }
        unsafe {
            libc::rmdir(dir.as_ptr());
        }
    }
    // SAFETY: _exit is async-signal-safe and runs no destructor.
    unsafe { libc::_exit(128 + signal) }
}

fn shot_path(dir: &Path, slot: usize) -> PathBuf {
    dir.join(format!("shot-{slot}.png"))
}

// ---------------------------------------------------------------------------
// Small readers
// ---------------------------------------------------------------------------

/// A finished child: its exit code (`None` when a signal ended it) and what it
/// said.
pub struct Captured {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Captured {
    fn ok(&self) -> bool {
        self.status == Some(0)
    }

    /// The child's own words for why it failed.
    fn reason(&self) -> String {
        let said = strip_ansi(self.stderr.trim());
        let said = if said.is_empty() {
            strip_ansi(self.stdout.trim())
        } else {
            said
        };
        if said.is_empty() {
            match self.status {
                Some(code) => format!("exit status {code}"),
                None => "ended by a signal".to_string(),
            }
        } else {
            last_lines(&said, 3)
        }
    }
}

/// Run `program` with `args`, no stdin, both outputs captured, killed when it
/// outlives `timeout`. Never a shell.
pub fn capture(program: &str, args: &[&str], timeout: Duration) -> Result<Captured, String> {
    let mut child = Command::new(program)
        .args(args)
        .env_remove("LAZYOS_UI")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => format!("{program} is not installed"),
            _ => format!("cannot run {program}: {e}"),
        })?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{program} took longer than {} s and was stopped",
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("lost {program}: {e}")),
        }
    };
    Ok(Captured {
        status: status.code(),
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
    })
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn read_trim(path: &str) -> String {
    std::fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn which(path: &str, name: &str) -> Option<PathBuf> {
    path.split(':')
        .filter(|d| !d.is_empty())
        .map(|d| Path::new(d).join(name))
        .find(|p| p.is_file())
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Remove CSI/OSC escape sequences: a report's colours are for a terminal.
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' || (c == '\u{1b}' && chars.peek() == Some(&'\\')) {
                        chars.next_if_eq(&'\\');
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn png_size(path: &Path) -> Option<(u32, u32)> {
    let mut header = [0u8; 24];
    std::fs::File::open(path)
        .ok()?
        .read_exact(&mut header)
        .ok()?;
    if &header[..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(header[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(header[20..24].try_into().ok()?);
    Some((width, height))
}

fn batteries() -> Vec<Value> {
    std::fs::read_dir("/sys/class/power_supply")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| read_trim(&e.path().join("type").display().to_string()) == "Battery")
        .map(|e| {
            let at = |f: &str| read_trim(&e.path().join(f).display().to_string());
            json!({
                "name": e.file_name().to_string_lossy(),
                "capacity": at("capacity").parse::<u32>().ok(),
                "status": at("status"),
            })
        })
        .collect()
}

fn device_of(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| m.dev())
}

fn disk(mount: &str) -> Option<Value> {
    let c = CString::new(mount).ok()?;
    // SAFETY: statvfs fills a plain struct we own; the path is NUL-terminated.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut st) } != 0 {
        return None;
    }
    let gib = |blocks: u64| {
        (blocks as f64 * st.f_frsize as f64 / (1u64 << 30) as f64 * 10.0).round() / 10.0
    };
    Some(
        json!({"mount": mount, "size_gib": gib(st.f_blocks as u64), "free_gib": gib(st.f_bavail as u64)}),
    )
}

/// Walk sway's tree: a node with a `pid` is a window (a view); the workspace
/// above it names where it is.
fn collect_windows(node: &Value, workspace: Option<&str>, floating: bool, out: &mut Vec<Value>) {
    let kind = node.get("type").and_then(Value::as_str).unwrap_or("");
    let workspace = if kind == "workspace" {
        node.get("name").and_then(Value::as_str)
    } else {
        workspace
    };
    if node.get("pid").is_some() && kind != "workspace" && kind != "output" {
        let app = node
            .get("app_id")
            .and_then(Value::as_str)
            .or_else(|| {
                node.pointer("/window_properties/class")
                    .and_then(Value::as_str)
            })
            .unwrap_or("");
        out.push(json!({
            "workspace": workspace,
            "app": app,
            "title": node.get("name").and_then(Value::as_str).unwrap_or(""),
            "focused": node.get("focused").and_then(Value::as_bool).unwrap_or(false),
            "floating": floating || kind == "floating_con",
        }));
    }
    for child in node
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect_windows(child, workspace, floating, out);
    }
    for child in node
        .get("floating_nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect_windows(child, workspace, true, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_is_stripped_from_a_report() {
        assert_eq!(strip_ansi("  \u{1b}[1;31mFAIL\u{1b}[0m  x"), "  FAIL  x");
        assert_eq!(
            strip_ansi("\u{1b}]8;;http://a\u{7}link\u{1b}]8;;\u{7}"),
            "link"
        );
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn the_window_tree_becomes_a_list() {
        let tree = json!({"type":"root","nodes":[{"type":"output","name":"eDP-1","nodes":[
            {"type":"workspace","name":"1","nodes":[
                {"type":"con","pid":10,"app_id":"foot","name":"~","focused":true,"nodes":[]}
            ],"floating_nodes":[
                {"type":"floating_con","pid":11,"app_id":null,"window_properties":{"class":"Steam"},"name":"Steam","nodes":[]}
            ]}
        ]}]});
        let mut out = Vec::new();
        collect_windows(&tree, None, false, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["workspace"], "1");
        assert_eq!(out[0]["app"], "foot");
        assert_eq!(out[0]["focused"], true);
        assert_eq!(out[1]["app"], "Steam");
        assert_eq!(out[1]["floating"], true);
    }

    #[test]
    fn a_task_id_is_one_plain_path_component() {
        use crate::task_cmd::valid_id;
        assert!(valid_id("ask-1793234096-4242"));
        for bad in ["", "../x", "a/b", ".hidden", "a b", "x;rm", "--help"] {
            assert!(!valid_id(bad), "{bad:?}");
        }
    }
}
