//! The Ask strip v1 (spec 007): one question, answered by the default agent,
//! the answer inline in the window.
//!
//! Three rules shape this file, and each has a test:
//!
//! - **The default agent answers.** The Agents menu marks one agent with `●`
//!   (`~/.config/lazyos/agents/default`); Claude Code and OpenCode have a
//!   one-shot mode wired here, and OpenCode answers when the default is unset,
//!   not installed, or has none (`resolve`). The prompt names the agent before
//!   anything is sent.
//! - **Only the question leaves the machine.** The agent runs in an empty
//!   directory of its own under `$XDG_RUNTIME_DIR`, Claude Code with no tools
//!   and no saved session, so there is no working directory to read and nothing
//!   to attach. The argument vector is built here and handed to `exec`; no shell
//!   ever sees the question (`the_question_is_one_argument_whatever_it_contains`).
//! - **Nothing outlives the answer.** `lazyos --ask-run` (the task's child)
//!   starts the agent in a process group of its own and kills the group when the
//!   answer is in, when it is stopped, and when the window closes, so a helper
//!   the agent started (`opencode serve`) cannot stay behind. Every process of
//!   that group carries `LAZYOS_ASK=<task id>` in its environment, so one that
//!   left the group (`setsid`) is found by that mark and ended too - and the
//!   mark is how `check-system.sh` finds one that survived anyway. "The window
//!   closes" is watched, not reported: the child holds a pidfd on the window's
//!   process, so Alt+Q on the window or a killed foot ends the question as
//!   surely as Escape does.

use std::collections::BTreeMap;
use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use lazyos_ui::app::exit::{ESCAPE, FAILED, OK, USAGE};

/// The two agents with a one-shot mode wired here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Claude,
    OpenCode,
}

impl Agent {
    pub fn bin(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::OpenCode => "opencode",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::OpenCode => "OpenCode",
        }
    }

    pub fn from_bin(bin: &str) -> Option<Self> {
        match bin {
            "claude" => Some(Agent::Claude),
            "opencode" => Some(Agent::OpenCode),
            _ => None,
        }
    }
}

/// The names `rofi-agents` records (its `AGENTS` table), for the sentence that
/// says why another agent answers.
fn display_name(bin: &str) -> &str {
    match bin {
        "claude" => "Claude Code",
        "codex" => "Codex",
        "opencode" => "OpenCode",
        "gemini" => "Gemini CLI",
        "copilot" => "GitHub Copilot",
        other => other,
    }
}

/// Who answers, and why when it is not the default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub agent: Agent,
    /// Set when the answer does not come from the default the person chose.
    pub note: Option<String>,
}

/// The agent a question goes to: the default when it is one of the two wired
/// here and installed; otherwise OpenCode, which the image ships; otherwise
/// Claude Code. `None` when neither is installed.
pub fn resolve(default: Option<&str>, installed: &dyn Fn(&str) -> bool) -> Option<Choice> {
    let default = default.map(str::trim).filter(|d| !d.is_empty());
    if let Some(bin) = default
        && let Some(agent) = Agent::from_bin(bin)
        && installed(bin)
    {
        return Some(Choice { agent, note: None });
    }
    let why = default.map(|bin| {
        if Agent::from_bin(bin).is_some() {
            format!("{} is not installed", display_name(bin))
        } else {
            format!("{} has no one-question mode here", display_name(bin))
        }
    });
    for agent in [Agent::OpenCode, Agent::Claude] {
        if installed(agent.bin()) {
            let note = why.map(|why| format!("{why}; {} answers", agent.name()));
            return Some(Choice { agent, note });
        }
    }
    None
}

/// The program and arguments that ask `question` once. The question is one
/// argument, so nothing in it can become a flag or a second command.
pub fn argv(agent: Agent, question: &str) -> Vec<String> {
    let mut argv: Vec<String> = match agent {
        // No tools at all (no file reads, no shell, no MCP), and no session on
        // disk: this is a question, not a working session.
        Agent::Claude => [
            "claude",
            "-p",
            "--output-format",
            "json",
            "--tools",
            "",
            "--disallowedTools",
            "mcp__*",
            "--no-session-persistence",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        // `run` refuses permission prompts itself; the empty directory is what
        // keeps its read tools from having anything to read.
        Agent::OpenCode => ["opencode", "run", "--format", "json"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };
    match agent {
        // `--` ends the options (Claude Code's parser documents it), so a
        // question that starts with `-` is a question.
        Agent::Claude => {
            argv.push("--".to_string());
            argv.push(question.to_string());
        }
        // OpenCode's parser is not documented to honour `--`, so a question
        // that would read as an option gets a leading space instead: the model
        // reads the same words, and no parser reads a flag.
        Agent::OpenCode => argv.push(if question.starts_with('-') {
            format!(" {question}")
        } else {
            question.to_string()
        }),
    }
    argv
}

/// The answer from Claude Code's `--output-format json`: one object, the text
/// in `result`, `is_error` when the run failed (and then `result` says why).
pub fn parse_claude(stdout: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|_| {
        first_line(stdout).unwrap_or_else(|| "Claude Code printed no answer".to_string())
    })?;
    let result = value
        .get("result")
        .and_then(|r| r.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    let failed = value
        .get("is_error")
        .and_then(|e| e.as_bool())
        .unwrap_or(false);
    if failed {
        return Err(first_line(&result).unwrap_or_else(|| "Claude Code reported an error".into()));
    }
    if result.is_empty() {
        return Err("Claude Code answered with nothing".to_string());
    }
    Ok(result)
}

/// The answer from `opencode run --format json`: one JSON event per line, the
/// text in `part.text` of the `text` events (joined like paragraphs), and an
/// `error` event when the run failed. A line that is not JSON is skipped - the
/// events are the contract, not whatever else reaches stdout.
pub fn parse_opencode(stdout: &str) -> Result<String, String> {
    let mut texts: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
            continue;
        };
        match event.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(text) = event.pointer("/part/text").and_then(|t| t.as_str())
                    && !text.trim().is_empty()
                {
                    texts.push(text.trim().to_string());
                }
            }
            Some("error") => {
                let message = event
                    .get("message")
                    .and_then(|m| m.as_str())
                    .or_else(|| {
                        event
                            .pointer("/error/data/message")
                            .and_then(|m| m.as_str())
                    })
                    .or_else(|| event.pointer("/error/name").and_then(|m| m.as_str()))
                    .unwrap_or("OpenCode reported an error");
                return Err(first_line(message).unwrap_or_else(|| message.to_string()));
            }
            _ => {}
        }
    }
    if texts.is_empty() {
        return Err("OpenCode answered with nothing".to_string());
    }
    Ok(texts.join("\n\n"))
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(String::from)
}

/// Where the agent CLIs live: the image installs OpenCode in `~/.opencode/bin`
/// and npm puts the others in `~/.local/bin`, and sway's own environment has
/// neither (the reason `rofi-agents` extends `PATH` the same way).
pub fn agent_path(vars: &BTreeMap<String, String>) -> String {
    let home = vars.get("HOME").map(String::as_str).unwrap_or("");
    let path = vars
        .get("PATH")
        .map(String::as_str)
        .unwrap_or("/usr/bin:/bin");
    if home.is_empty() {
        return path.to_string();
    }
    format!("{home}/.opencode/bin:{home}/.local/bin:{path}")
}

pub fn on_path(path: &str, bin: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.split(':').filter(|d| !d.is_empty()).any(|dir| {
        std::fs::metadata(Path::new(dir).join(bin))
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    })
}

/// The default the Agents menu recorded, if any.
pub fn default_agent(vars: &BTreeMap<String, String>) -> Option<String> {
    let config = match vars.get("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(vars.get("HOME")?).join(".config"),
    };
    let text = std::fs::read_to_string(config.join("lazyos/agents/default")).ok()?;
    first_line(&text)
}

/// Claude Code's account, the way `rofi-agents` starts a session with it: the
/// account marked default for `claude` is a `CLAUDE_CONFIG_DIR`. Without one,
/// Claude Code uses its own default directory and sign-in.
pub fn claude_config_dir(vars: &BTreeMap<String, String>) -> Option<PathBuf> {
    let data = match vars.get("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(vars.get("HOME")?).join(".local/share"),
    };
    let agents = data.join("lazyos/agents/claude");
    let account = first_line(&std::fs::read_to_string(agents.join("default")).ok()?)?;
    let dir = agents.join(account);
    dir.is_dir().then_some(dir)
}

/// The choice for this session: the recorded default, resolved against what is
/// installed on the agents' `PATH`.
pub fn choose(vars: &BTreeMap<String, String>) -> Option<Choice> {
    let path = agent_path(vars);
    let default = default_agent(vars);
    resolve(default.as_deref(), &|bin| on_path(&path, bin))
}

// ---------------------------------------------------------------------------
// `lazyos --ask-run <id> <agent> <question>`: the task's child
// ---------------------------------------------------------------------------

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

/// Run the agent once and print the answer, one line per line, on stdout -
/// which is the task's log, and so what the window shows. A failure prints one
/// `ERROR:` line (the task's error line) and exits 3.
///
/// `viewer` is the pid of the window that asked; `0` watches nothing (a
/// question asked from a script, or a test that owns the lifetime itself).
pub fn run(args: &[String]) -> i32 {
    let [id, agent, viewer, question] = args else {
        eprintln!("usage: lazyos --ask-run <task id> <claude|opencode> <viewer pid> <question>");
        return USAGE;
    };
    let Ok(viewer) = viewer.parse::<libc::pid_t>() else {
        eprintln!("lazyos --ask-run: '{viewer}' is not a pid");
        return USAGE;
    };
    let Some(agent) = Agent::from_bin(agent) else {
        println!("ERROR: {agent} has no one-question mode here");
        return FAILED;
    };
    let vars: BTreeMap<String, String> = std::env::vars().collect();

    // SIGINT is the task's stop (spec 005: SIGINT, then SIGTERM), SIGTERM the
    // runner's escalation; both end in the same place: the group is killed.
    unsafe {
        libc::signal(libc::SIGINT, on_stop as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_stop as *const () as libc::sighandler_t);
    }

    let dir = match private_dir(&vars, id) {
        Ok(dir) => dir,
        Err(error) => {
            println!("ERROR: no private directory for the agent: {error}");
            return FAILED;
        }
    };
    let watch = Watch::open(viewer);
    let outcome = ask(agent, id, question, &vars, &dir, &watch);
    let _ = std::fs::remove_dir_all(&dir);

    match outcome {
        Asked::Answer(text) => {
            for line in text.lines() {
                println!("{line}");
            }
            OK
        }
        Asked::Failed(reason) => {
            println!("ERROR: {reason}");
            FAILED
        }
        Asked::Stopped => ESCAPE,
    }
}

enum Asked {
    Answer(String),
    Failed(String),
    Stopped,
}

fn ask(
    agent: Agent,
    id: &str,
    question: &str,
    vars: &BTreeMap<String, String>,
    dir: &Path,
    watch: &Watch,
) -> Asked {
    let argv = argv(agent, question);
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(dir)
        .env("PATH", agent_path(vars))
        .env("LAZYOS_ASK", id)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // A group of its own: the agent and everything it starts can be ended
        // with one signal, and a helper it forks cannot outlive the question.
        .process_group(0);
    if agent == Agent::Claude
        && let Some(config) = claude_config_dir(vars)
    {
        command.env("CLAUDE_CONFIG_DIR", config);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return Asked::Failed(format!("cannot start {}: {error}", agent.bin())),
    };
    let group = child.id() as libc::pid_t;

    let stdout = read_in_background(child.stdout.take());
    let stderr = read_in_background(child.stderr.take());

    let status = loop {
        if STOP.load(Ordering::Relaxed) || watch.gone() {
            end_group(group);
            sweep(id);
            let _ = child.wait();
            return Asked::Stopped;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                end_group(group);
                sweep(id);
                return Asked::Failed(format!("lost the agent: {error}"));
            }
        }
    };
    // The answer is in. Whatever the agent left running in its group goes now,
    // before the pipes are read to the end (a helper holding them open would
    // otherwise keep this process - and the task - waiting).
    end_group(group);
    sweep(id);
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();

    let parsed = match agent {
        Agent::Claude => parse_claude(&stdout),
        Agent::OpenCode => parse_opencode(&stdout),
    };
    match parsed {
        Ok(answer) => Asked::Answer(answer),
        // The agent's own words first, then its stderr, then the exit status.
        Err(_) if !status.success() && stdout.trim().is_empty() => Asked::Failed(
            first_line(&stderr).unwrap_or_else(|| format!("{} exited with {status}", agent.bin())),
        ),
        Err(reason) => Asked::Failed(reason),
    }
}

fn read_in_background<R: Read + Send + 'static>(
    pipe: Option<R>,
) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_string(&mut text);
        }
        text
    })
}

/// SIGTERM to the whole group, SIGKILL to whatever is left after a second.
fn end_group(group: libc::pid_t) {
    if group <= 1 {
        return;
    }
    unsafe {
        libc::kill(-group, libc::SIGTERM);
    }
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        // ESRCH: no process is left in the group.
        if unsafe { libc::kill(-group, 0) } != 0 {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    unsafe {
        libc::kill(-group, libc::SIGKILL);
    }
}

/// The process that asked, held by a pidfd so a reused pid can never stand in
/// for it: the descriptor refers to that one process and becomes readable when
/// it exits. A kernel without `pidfd_open` (before 5.3) falls back to asking
/// `kill(pid, 0)`, which is right in every case but a pid reused within the
/// 50 ms poll.
struct Watch {
    pid: libc::pid_t,
    fd: Option<libc::c_int>,
    /// The window was already gone when the watch began.
    gone_at_start: bool,
}

impl Watch {
    fn open(pid: libc::pid_t) -> Self {
        if pid <= 0 {
            return Self {
                pid: 0,
                fd: None,
                gone_at_start: false,
            };
        }
        // SAFETY: a plain syscall with an integer argument and no pointers.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as libc::c_int;
        if fd >= 0 {
            return Self {
                pid,
                fd: Some(fd),
                gone_at_start: false,
            };
        }
        let gone = io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        Self {
            pid,
            fd: None,
            gone_at_start: gone,
        }
    }

    fn gone(&self) -> bool {
        if self.gone_at_start {
            return true;
        }
        match self.fd {
            Some(fd) => {
                let mut poll = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: one valid pollfd, a zero timeout.
                let ready = unsafe { libc::poll(&mut poll, 1, 0) };
                ready > 0 && poll.revents & libc::POLLIN != 0
            }
            // SAFETY: signal 0 only asks whether the process exists.
            None => self.pid > 0 && unsafe { libc::kill(self.pid, 0) } != 0,
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if let Some(fd) = self.fd {
            // SAFETY: the descriptor was opened by `Watch::open` and is closed once.
            unsafe {
                libc::close(fd);
            }
        }
    }
}

/// End every process that still carries this question's mark: a helper that
/// called `setsid` left the group `end_group` signalled, but it cannot leave
/// the environment it was started with.
fn sweep(id: &str) {
    let mark = format!("LAZYOS_ASK={id}");
    for pid in marked(&mark) {
        // SAFETY: a signal to a pid read from /proc a moment ago.
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
}

/// The pids whose environment holds exactly `mark` (`LAZYOS_ASK=<id>`), read
/// from `/proc/<pid>/environ`; a process of another user is unreadable and
/// skipped, which is right - this question started none of them.
pub fn marked(mark: &str) -> Vec<libc::pid_t> {
    let me = std::process::id() as libc::pid_t;
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    dir.filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse::<libc::pid_t>().ok())
        .filter(|&pid| pid != me)
        .filter(|pid| {
            std::fs::read(format!("/proc/{pid}/environ"))
                .map(|env| env.split(|&b| b == 0).any(|var| var == mark.as_bytes()))
                .unwrap_or(false)
        })
        .collect()
}

/// An empty directory only this user can read, for the agent to run in.
fn private_dir(vars: &BTreeMap<String, String>, id: &str) -> io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let base = vars
        .get("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("lazyos-ask-{id}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(list: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |bin| list.contains(&bin)
    }

    #[test]
    fn the_default_agent_answers_when_it_can() {
        let both = installed(&["claude", "opencode"]);
        assert_eq!(
            resolve(Some("claude"), &both),
            Some(Choice {
                agent: Agent::Claude,
                note: None
            })
        );
        assert_eq!(
            resolve(Some("opencode"), &both).map(|c| c.agent),
            Some(Agent::OpenCode)
        );
    }

    #[test]
    fn opencode_answers_for_a_default_it_cannot_ask_and_says_why() {
        let both = installed(&["claude", "opencode", "codex"]);
        let codex = resolve(Some("codex"), &both).unwrap();
        assert_eq!(codex.agent, Agent::OpenCode);
        assert_eq!(
            codex.note.as_deref(),
            Some("Codex has no one-question mode here; OpenCode answers")
        );

        let missing = resolve(Some("claude"), &installed(&["opencode"])).unwrap();
        assert_eq!(missing.agent, Agent::OpenCode);
        assert_eq!(
            missing.note.as_deref(),
            Some("Claude Code is not installed; OpenCode answers")
        );

        // No default recorded: OpenCode, the agent the image ships, silently.
        assert_eq!(
            resolve(None, &both),
            Some(Choice {
                agent: Agent::OpenCode,
                note: None
            })
        );
        // Without OpenCode, Claude Code is the last resort.
        assert_eq!(
            resolve(Some("gemini"), &installed(&["claude"])).map(|c| c.agent),
            Some(Agent::Claude)
        );
        assert_eq!(resolve(Some("claude"), &installed(&[])), None);
    }

    #[test]
    fn the_question_is_one_argument_whatever_it_contains() {
        let question = "--tools \"Bash\"; rm -rf ~ && echo $(whoami) | tee `x`";
        for agent in [Agent::Claude, Agent::OpenCode] {
            let argv = argv(agent, question);
            let last = argv.last().unwrap();
            assert_eq!(last.trim_start(), question, "the question is one argument");
            match agent {
                Agent::Claude => {
                    assert_eq!(
                        argv[argv.len() - 2],
                        "--",
                        "options end before the question"
                    )
                }
                Agent::OpenCode => assert!(
                    !last.starts_with('-'),
                    "nothing a parser could read as an option: {last:?}"
                ),
            }
            assert_eq!(argv[0], agent.bin());
            assert!(
                !argv.iter().any(|a| a == "sh" || a == "-c" || a == "bash"),
                "no shell: {argv:?}"
            );
        }
        let claude = argv(Agent::Claude, "hi");
        let tools = claude.iter().position(|a| a == "--tools").unwrap();
        assert_eq!(claude[tools + 1], "", "Claude Code runs with no tools");
        assert!(claude.contains(&"--no-session-persistence".to_string()));
    }

    #[test]
    fn claude_json_gives_the_result_or_its_error_line() {
        let ok = r#"{"type":"result","subtype":"success","is_error":false,"result":"Paris.\n\nIt is the capital.","session_id":"x"}"#;
        assert_eq!(parse_claude(ok).unwrap(), "Paris.\n\nIt is the capital.");
        let failed = r#"{"type":"result","subtype":"success","is_error":true,"result":"Invalid API key · Please run /login"}"#;
        assert_eq!(
            parse_claude(failed).unwrap_err(),
            "Invalid API key · Please run /login"
        );
        assert_eq!(
            parse_claude("error: unknown option '--tools'\n").unwrap_err(),
            "error: unknown option '--tools'",
            "a run that never produced JSON reports its own first line"
        );
    }

    #[test]
    fn opencode_events_give_the_text_parts_or_the_error() {
        let ok = concat!(
            r#"{"type":"step_start","part":{"type":"step-start"}}"#,
            "\n",
            r#"{"type":"text","part":{"type":"text","text":"Paris."}}"#,
            "\n",
            "not json, ignored\n",
            r#"{"type":"text","part":{"type":"text","text":"It is the capital."}}"#,
            "\n",
            r#"{"type":"step_finish","part":{"reason":"stop"}}"#,
            "\n",
        );
        assert_eq!(parse_opencode(ok).unwrap(), "Paris.\n\nIt is the capital.");
        let failed = r#"{"type":"error","error":{"name":"APIError","data":{"message":"Rate limit exceeded"}}}"#;
        assert_eq!(parse_opencode(failed).unwrap_err(), "Rate limit exceeded");
        assert_eq!(
            parse_opencode("").unwrap_err(),
            "OpenCode answered with nothing"
        );
    }

    #[test]
    fn the_agents_path_finds_what_npm_and_the_image_installed() {
        let vars: BTreeMap<String, String> = [("HOME", "/home/b"), ("PATH", "/usr/bin:/bin")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert_eq!(
            agent_path(&vars),
            "/home/b/.opencode/bin:/home/b/.local/bin:/usr/bin:/bin"
        );
    }
}
