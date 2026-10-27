//! The task runner as a separate process: closing the window does not take the
//! task down, and `lazyos task attach` reopens it by id.
//!
//! The runner owns the child; this test process plays the "window" that closes
//! by simply never owning the child. The runner writes the state file, so a
//! later `Task::load` sees the result whether or not anything was watching.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use lazyos_ui::{Task, TaskState};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_lazyos")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lazyos-task-run-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn wait_for<F>(timeout: Duration, mut predicate: F) -> bool
where
    F: FnMut() -> bool,
{
    let start = Instant::now();
    while start.elapsed() < timeout {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    predicate()
}

/// The state a task lives in when `Paths::resolve` sees `XDG_STATE_HOME=tmp`.
fn state_dir(tmp: &std::path::Path) -> PathBuf {
    tmp.join("lazyos")
}

/// Start the detached runner the way a menu does: write the record, then hand
/// it to `spawn_runner`, which `setsid`s so the window's closing signal never
/// reaches it. A menu passes no environment (it and the runner agree on
/// `XDG_STATE_HOME`); the test names the scratch directory instead.
fn start_detached(id: &str, tmp: &std::path::Path, task: &mut Task) {
    task.write_state().unwrap();
    let tmp = tmp.to_str().expect("the scratch path is UTF-8");
    lazyos_cli::task_cmd::spawn_runner(id, bin(), &[("XDG_STATE_HOME", tmp)])
        .expect("the detached runner starts");
}

#[test]
fn a_detached_task_runs_to_done_without_a_viewer() {
    let tmp = scratch("detached");
    let dir = state_dir(&tmp);

    // The "menu" side writes the record with its command, then hands off.
    let mut task = Task::new("detached", "detached runner", &dir).with_command(&[
        "/bin/sh",
        "-c",
        "echo done",
    ]);
    task.add_step("run the command");
    start_detached("detached", &tmp, &mut task);

    // Nothing here owns the child: this process is the window that closed.
    let finished = wait_for(Duration::from_secs(10), || {
        Task::load("detached", &dir)
            .ok()
            .flatten()
            .is_some_and(|t| t.state == TaskState::Done)
    });
    assert!(finished, "the detached runner should reach Done");

    // Reattach after it finished shows the saved result and log.
    let loaded = Task::load("detached", &dir)
        .unwrap()
        .expect("the state file survives");
    assert_eq!(loaded.state, TaskState::Done);
    assert!(
        loaded.log.lines().iter().any(|l| l == "done"),
        "the log should carry the child's output"
    );
}

#[test]
fn a_failed_detached_task_keeps_its_error_line() {
    let tmp = scratch("detached-fail");
    let dir = state_dir(&tmp);

    let mut task = Task::new("fails", "detached failure", &dir).with_command(&[
        "/bin/sh",
        "-c",
        "echo 'ERROR: it broke' >&2; exit 1",
    ]);
    task.add_step("run the command");
    start_detached("fails", &tmp, &mut task);

    let failed = wait_for(Duration::from_secs(10), || {
        Task::load("fails", &dir)
            .ok()
            .flatten()
            .is_some_and(|t| t.state == TaskState::Failed)
    });
    assert!(failed, "a non-zero child should be recorded as Failed");

    let loaded = Task::load("fails", &dir).unwrap().unwrap();
    assert_eq!(
        loaded.error_line.as_deref(),
        Some("ERROR: it broke"),
        "the last ERROR: line is what the card shows"
    );
}

#[test]
fn a_viewer_stops_a_detached_task_through_the_request_file() {
    let tmp = scratch("detached-stop");
    let dir = state_dir(&tmp);

    let mut task =
        Task::new("stop-me", "detached stop", &dir).with_command(&["/bin/sh", "-c", "sleep 30"]);
    task.set_stoppable(true);
    task.add_step("sleep");
    start_detached("stop-me", &tmp, &mut task);

    // Wait until the runner owns the child and reports Running.
    let running = wait_for(Duration::from_secs(10), || {
        Task::load("stop-me", &dir)
            .ok()
            .flatten()
            .is_some_and(|t| t.state == TaskState::Running)
    });
    assert!(running, "the runner should report Running");

    // A viewer asks the runner to stop by writing the request file; it never
    // signals a process it does not own.
    let viewer = Task::load("stop-me", &dir).unwrap().unwrap();
    assert!(!viewer.owns_child(), "the viewer does not own the child");
    viewer.request_stop();

    let stopped = wait_for(Duration::from_secs(10), || {
        Task::load("stop-me", &dir)
            .ok()
            .flatten()
            .is_some_and(|t| t.state == TaskState::Stopped)
    });
    assert!(
        stopped,
        "the runner should stop the child and record Stopped"
    );

    // A stopped task is never a failure; the card says so in its result.
    let loaded = Task::load("stop-me", &dir).unwrap().unwrap();
    assert_eq!(loaded.state, TaskState::Stopped);
    assert!(!loaded.log.lines().iter().any(|l| l.is_empty()));
}
