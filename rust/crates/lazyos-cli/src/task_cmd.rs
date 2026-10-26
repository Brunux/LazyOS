//! The task surface: reattach to a slow action and watch it finish.
//!
//! A task is created by the menus that own the action (Wi-Fi scan, apt,
//! dictation, the installer). This command only reopens one by id, so a person
//! who closed the window can come back to the result and the log.

use std::io;
use std::process::{Command, Stdio};
use std::time::Duration;

use lazyos_ui::app::exit::{FAILED, OK};
use lazyos_ui::{App, Design, Header, Task, TaskState, TaskView};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::Failure;
use crate::command::Resolved;
use crate::paths::Paths;

pub fn attach(
    resolved: &Resolved,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    design: &Design,
) -> Result<i32, Failure> {
    let id = match resolved.args.first() {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            return Err(Failure::new("no task id", "usage: lazyos task attach <id>"));
        }
    };

    let paths = Paths::from_process().map_err(|error| {
        Failure::new("cannot find the session state directory", error.to_string())
    })?;

    let task = match Task::load(&id, &paths.state) {
        Ok(Some(task)) => task,
        Ok(None) => {
            return Err(Failure::new(
                format!("task '{id}' was not found"),
                "the task may have been cleaned up, or the id is wrong",
            ));
        }
        Err(error) => {
            return Err(Failure::new(
                format!("task '{id}' could not be read"),
                error.to_string(),
            ));
        }
    };

    let title = task.title.clone();
    let mut view = TaskView::new(task);
    let mut app = App::new(Header::new("LazyOS").subtitle(&title))
        .design(design.clone())
        .screen(&mut view);
    app.run(terminal)
        .map_err(|error| Failure::new("the terminal stopped answering", error.to_string()))
}

/// The headless runner: the process that actually owns the task's child.
///
/// A menu starts a slow action by writing its spec (with the command) and then
/// spawning `lazyos --task-run <id>` detached. This process has no terminal and
/// no window: it loads the record, runs the child in its own process, writes the
/// state atomically, and exits with the result code. Because it is a separate
/// process from the viewer, closing the viewer's window does not take the task
/// down - the viewer reopens it with `lazyos task attach`.
pub fn run(id: &str) -> i32 {
    let paths = match Paths::from_process() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("lazyos task run: cannot find the session state directory: {error}");
            return FAILED;
        }
    };

    let mut task = match Task::load(id, &paths.state) {
        Ok(Some(task)) => task,
        Ok(None) => {
            eprintln!("lazyos task run: task '{id}' was not found");
            return FAILED;
        }
        Err(error) => {
            eprintln!("lazyos task run: task '{id}' could not be read: {error}");
            return FAILED;
        }
    };

    let argv = match task.argv.clone() {
        Some(argv) if !argv.is_empty() => argv,
        _ => {
            eprintln!("lazyos task run: task '{id}' has no command to run");
            return FAILED;
        }
    };
    let argv_refs: Vec<&str> = argv.iter().map(String::as_str).collect();
    if let Err(error) = task.spawn(&argv_refs) {
        eprintln!("lazyos task run: cannot start '{id}': {error}");
        return FAILED;
    }

    // The loop: drain the child's output, enforce the stop timeout, write the
    // state. The stop arrives either as SIGUSR1 (from the bar's `.stop` line) or
    // as the `.stop` request file a viewer writes; both are handled in `tick`.
    // The exit signal is the child being reaped (`finalize` clears it), not the
    // state: `stop()` sets `Stopped` before the child has died, so leaving on the
    // state would return while the child was still running.
    loop {
        if let Err(error) = task.tick() {
            eprintln!("lazyos task run: task '{id}' failed: {error}");
            return FAILED;
        }
        if !task.owns_child() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    match task.state {
        TaskState::Done | TaskState::Stopped => OK, // stopped is not a failure
        TaskState::Failed | TaskState::DidNotFinish => FAILED,
        _ => FAILED,
    }
}

/// Start a task detached from the caller: the runner is a new session
/// (`setsid`), so it survives the terminal and the window that started it. The
/// caller has already written the record (with its command) to the state file.
///
/// `env` is applied on top of the inherited environment. A menu passes an empty
/// slice, because it and the runner read the same `XDG_STATE_HOME`; a test
/// points the runner at a scratch directory, which is the only honest way to
/// give a detached child its own state without mutating the test's own
/// environment (an `unsafe` and racy operation in edition 2024).
pub fn spawn_runner(id: &str, exe: &str, env: &[(&str, &str)]) -> io::Result<()> {
    use std::os::unix::process::CommandExt;
    // `pre_exec` is unsafe: the closure runs between fork and exec, so it may
    // only make async-signal-safe calls (setsid is one). Wrapped here because
    // the 2024 edition requires it to be explicit.
    let mut child = unsafe {
        Command::new(exe)
            .args(["--task-run", id])
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .pre_exec(|| {
                // A new session with no controlling terminal: a SIGHUP from the
                // closing window does not reach the runner, and it keeps writing
                // state after the UI is gone.
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            })
            .spawn()?
    };
    // Non-blocking reap, like `host::spawn`: the caller exits immediately and
    // the runner is reparented; this only clears an instant failure in tests.
    let _ = child.try_wait();
    Ok(())
}
