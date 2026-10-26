//! The task surface: a slow action that keeps the person in the loop.
//!
//! A task is a declarative checklist plus one running child at a time. It writes
//! its state atomically so `lazyos task attach` can reopen it, and it mirrors the
//! existing shell `busy`/`bar-busy` protocol so the bar's spinner keeps working
//! until the bar itself is ported (spec 014).
//!
//! The rules this module enforces in its tests:
//!
//! - state transitions are the only way the view changes;
//! - a stopped child is never reported as a failure;
//! - output that arrives after the stop key is dropped, not appended;
//! - the state file is written with a temp + rename;
//! - a pid is trusted only after `/proc/<pid>/comm` matches;
//! - a stoppable task writes a `BUSY_STOP`-style line, a non-stoppable one does
//!   not.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::style::Style;
use unicode_width::UnicodeWidthStr;

use crate::app::{BodyCtx, Outcome, Screen};
use crate::design::{ColorRole, Design, TextStyle};
use crate::widgets::{KeyHint, LogPane, draw_card, draw_progress, put, put_trunc};

// ---------------------------------------------------------------------------
// Public state machine
// ---------------------------------------------------------------------------

/// The state of one step in a task checklist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StepState {
    Waiting,
    Running,
    Ok,
    Failed,
    Skipped,
}

/// One line of the checklist.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Step {
    pub label: String,
    pub state: StepState,
    pub detail: Option<String>,
}

impl Step {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            state: StepState::Waiting,
            detail: None,
        }
    }
}

/// The state of the whole task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TaskState {
    Queued,
    Running,
    Done,
    Failed,
    Stopped,
    DidNotFinish,
}

// ---------------------------------------------------------------------------
// On-disk record
// ---------------------------------------------------------------------------

/// The part of a task that survives a closed window.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub title: String,
    pub steps: Vec<Step>,
    pub state: TaskState,
    pub progress: f64,
    pub stoppable: bool,
    pub percent: u8,
    pub result: Option<String>,
    pub error_line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argv: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot_id: Option<String>,
}

impl TaskRecord {
    fn from_task(task: &Task) -> Self {
        Self {
            id: task.id.clone(),
            title: task.title.clone(),
            steps: task.steps.clone(),
            state: task.state,
            progress: task.progress,
            stoppable: task.stoppable,
            percent: task.percent,
            result: task.result.clone(),
            error_line: task.error_line.clone(),
            argv: task.argv.clone(),
            started_at: task.started_at.map(system_to_secs),
            finished_at: task.finished_at.map(system_to_secs),
            pid: std::process::id().into(),
            comm: current_comm(),
            boot_id: Some(current_boot_id()),
        }
    }
}

// ---------------------------------------------------------------------------
// Process control
// ---------------------------------------------------------------------------

enum ChildEvent {
    Line(String),
    Done(io::Result<std::process::ExitStatus>),
}

struct RunningChild {
    pid: u32,
    receiver: Receiver<ChildEvent>,
}

fn pipe_reader<R: std::io::Read + Send + 'static>(
    stream: R,
    log_path: PathBuf,
    tx: mpsc::Sender<ChildEvent>,
) {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();
    let mut writer = file.map(BufWriter::new);
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        match line {
            Ok(text) => {
                if let Some(w) = &mut writer {
                    let _ = writeln!(w, "{text}");
                }
                let _ = tx.send(ChildEvent::Line(text));
            }
            Err(_) => break,
        }
    }
}

fn spawn_child(argv: &[&str], log_path: &Path) -> io::Result<(u32, Receiver<ChildEvent>)> {
    if argv.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot spawn an empty command",
        ));
    }
    let mut cmd = Command::new(argv[0]);
    cmd.args(&argv[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn()?;
    let pid = child.id();

    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let (tx, rx) = mpsc::channel::<ChildEvent>();
    let stdout = child.stdout.take().expect("stdout was piped");
    thread::spawn({
        let tx = tx.clone();
        let log_path = log_path.to_path_buf();
        move || pipe_reader(stdout, log_path, tx)
    });
    let stderr = child.stderr.take().expect("stderr was piped");
    thread::spawn({
        let tx = tx.clone();
        let log_path = log_path.to_path_buf();
        move || pipe_reader(stderr, log_path, tx)
    });
    thread::spawn(move || {
        let status = child.wait();
        let _ = tx.send(ChildEvent::Done(status));
    });

    Ok((pid, rx))
}

static STOP_FLAG: AtomicBool = AtomicBool::new(false);

extern "C" fn stop_handler(_: libc::c_int) {
    STOP_FLAG.store(true, Ordering::Relaxed);
}

fn install_stop_handler() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        libc::signal(
            libc::SIGUSR1,
            stop_handler as *const () as libc::sighandler_t,
        );
    });
}

fn process_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

fn send_signal(pid: u32, signal: libc::c_int) {
    unsafe {
        libc::kill(pid as libc::pid_t, signal);
    }
}

fn current_boot_id() -> String {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn current_comm() -> Option<String> {
    std::fs::read_to_string("/proc/self/comm")
        .map(|s| s.trim().to_string())
        .ok()
}

fn read_proc_comm(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|s| s.trim().to_string())
        .ok()
}

fn system_to_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

fn secs_to_system(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

fn elapsed_between(start: SystemTime, end: SystemTime) -> Duration {
    end.duration_since(start).unwrap_or(Duration::ZERO)
}

// ---------------------------------------------------------------------------
// Spinner
// ---------------------------------------------------------------------------

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

#[derive(Clone, Debug, Default)]
struct Spinner {
    frame: usize,
}

impl Spinner {
    fn advance(&mut self) {
        self.frame = (self.frame + 1) % SPINNER_FRAMES.len();
    }

    fn glyph(&self) -> char {
        SPINNER_FRAMES[self.frame]
    }
}

// ---------------------------------------------------------------------------
// Task
// ---------------------------------------------------------------------------

/// The model and controller for one long-running action.
pub struct Task {
    pub id: String,
    pub title: String,
    pub steps: Vec<Step>,
    pub state: TaskState,
    pub progress: f64,
    pub log: LogPane,
    pub stoppable: bool,
    pub started_at: Option<SystemTime>,
    pub finished_at: Option<SystemTime>,
    pub result: Option<String>,
    pub error_line: Option<String>,
    pub percent: u8,
    /// The command the task runs, so a detached runner can start it without the
    /// UI passing it in-process. None until a menu (or a test) sets it.
    pub argv: Option<Vec<String>>,

    state_dir: PathBuf,
    runtime_dir: Option<PathBuf>,
    busy_tag: String,
    busy_icon: String,

    child: Option<RunningChild>,
    requested_stop: bool,
    stop_sent: Option<Instant>,
    term_sent: Option<Instant>,
    last_state_write: Option<Instant>,
    state_dirty: bool,
    spinner: Spinner,
    message: Option<(String, Instant)>,
}

impl Task {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        state_dir: impl AsRef<Path>,
    ) -> Self {
        let id = id.into();
        let busy_tag = slugify(&id);
        Self {
            id,
            title: title.into(),
            steps: Vec::new(),
            state: TaskState::Queued,
            progress: 0.0,
            log: LogPane::new(),
            stoppable: false,
            started_at: None,
            finished_at: None,
            result: None,
            error_line: None,
            percent: 0,
            argv: None,
            state_dir: state_dir.as_ref().to_path_buf(),
            runtime_dir: None,
            busy_tag,
            busy_icon: "system-software-install".to_string(),
            child: None,
            requested_stop: false,
            stop_sent: None,
            term_sent: None,
            last_state_write: None,
            state_dirty: true,
            spinner: Spinner::default(),
            message: None,
        }
    }

    pub fn with_runtime_dir(mut self, dir: impl AsRef<Path>) -> Self {
        self.runtime_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    pub fn with_busy_icon(mut self, icon: impl Into<String>) -> Self {
        self.busy_icon = icon.into();
        self
    }

    /// The command a detached runner starts: an argv array, never a shell
    /// string, so a menu pattern cannot become shell injection.
    pub fn with_command(mut self, argv: &[&str]) -> Self {
        self.argv = Some(argv.iter().map(|s| s.to_string()).collect());
        self
    }

    pub fn set_stoppable(&mut self, stoppable: bool) {
        self.stoppable = stoppable;
        self.state_dirty = true;
    }

    pub fn add_step(&mut self, label: impl Into<String>) -> usize {
        let index = self.steps.len();
        self.steps.push(Step::new(label));
        self.state_dirty = true;
        index
    }

    pub fn set_step_state(&mut self, index: usize, state: StepState) {
        if let Some(step) = self.steps.get_mut(index) {
            step.state = state;
            self.state_dirty = true;
        }
    }

    pub fn set_step_detail(&mut self, index: usize, detail: impl Into<String>) {
        if let Some(step) = self.steps.get_mut(index) {
            step.detail = Some(detail.into());
            self.state_dirty = true;
        }
    }

    pub fn set_progress(&mut self, progress: f64) {
        self.progress = progress.clamp(0.0, 1.0);
        self.percent = (self.progress * 100.0).round() as u8;
        self.state_dirty = true;
    }

    pub fn set_result(&mut self, result: impl Into<String>) {
        self.result = Some(result.into());
        self.state_dirty = true;
    }

    fn state_file(&self) -> PathBuf {
        self.state_dir
            .join("tasks")
            .join(format!("{}.json", self.id))
    }

    fn log_file(&self) -> PathBuf {
        self.state_dir
            .join("tasks")
            .join(format!("{}.log", self.id))
    }

    fn busy_dir(&self) -> Option<PathBuf> {
        self.runtime_dir.as_ref().map(|d| d.join("lazyos-busy"))
    }

    fn busy_base(&self) -> Option<PathBuf> {
        self.busy_dir().map(|d| d.join(&self.busy_tag))
    }

    /// Path to the stop-request file the bar's `BUSY_STOP` line can touch.
    /// This is the signal path: `bar-busy` runs the line stored in
    /// `<tag>.stop`, and that line sends `SIGUSR1` to the task's UI process.
    pub fn stop_request_path(&self) -> PathBuf {
        self.state_dir
            .join("tasks")
            .join(format!("{}.stop", self.id))
    }

    /// Whether this task owns the child process (the runner's case) or only
    /// watches a detached runner through the state file (the viewer's case). A
    /// runner exits once this is false; a viewer never owns the child.
    pub fn owns_child(&self) -> bool {
        self.child.is_some()
    }

    /// Start the child process. The command is an argv array; no shell is
    /// involved, so a menu pattern cannot become shell injection.
    pub fn spawn(&mut self, argv: &[&str]) -> io::Result<()> {
        if self.state != TaskState::Queued {
            return Err(io::Error::other("task has already been started"));
        }
        install_stop_handler();
        let log_path = self.log_file();
        let (pid, rx) = spawn_child(argv, &log_path)?;
        self.child = Some(RunningChild { pid, receiver: rx });
        self.started_at = Some(SystemTime::now());
        self.state = TaskState::Running;
        self.percent = 0;
        self.progress = 0.0;
        if !self.steps.is_empty() {
            self.steps[0].state = StepState::Running;
        }
        self.state_dirty = true;
        self.write_state()?;
        self.write_busy()?;
        Ok(())
    }

    /// Stop a running, stoppable task. The stop contract is SIGINT first,
    /// SIGTERM after two seconds; the final state is always `Stopped`, never
    /// `Failed`.
    pub fn stop(&mut self) {
        if !self.stoppable {
            self.set_message("this task cannot be stopped");
            return;
        }
        if self.state != TaskState::Running {
            return;
        }
        self.requested_stop = true;
        self.state = TaskState::Stopped;
        self.result = Some("Stopped".to_string());
        if let Some(child) = &self.child {
            send_signal(child.pid, libc::SIGINT);
        }
        self.stop_sent = Some(Instant::now());
        self.state_dirty = true;
        let _ = self.write_state();
    }

    /// A viewer that does not own the child asks the runner to stop by writing
    /// the request file the runner's [`Self::tick`] checks. The runner then
    /// forwards SIGINT (and SIGTERM after two seconds) to the child and records
    /// `Stopped`. The viewer never signals a process it does not own.
    pub fn request_stop(&self) {
        let path = self.stop_request_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, "stop\n");
    }

    /// Called when the UI loop wakes for a tick. It drains child output,
    /// enforces the stop timeout, advances the spinner, and writes state.
    pub fn tick(&mut self) -> io::Result<()> {
        self.check_external_stop_request();
        if self.child.is_some() {
            let child = self.child.take().expect("child was checked");
            let mut done = None;
            while let Ok(event) = child.receiver.try_recv() {
                match event {
                    ChildEvent::Line(line) => self.push_line(&line),
                    ChildEvent::Done(status) => done = Some(status),
                }
            }
            if done.is_none() {
                self.child = Some(child);
            }
            if let Some(status) = done {
                self.finalize(status)?;
            }
        }
        if self.requested_stop {
            self.enforce_stop_timeout();
        }
        self.spinner.advance();
        self.maybe_write_state();
        self.clear_expired_message();
        Ok(())
    }

    fn push_line(&mut self, line: &str) {
        // Output that arrives after the person asked to stop is dropped: the
        // result is the interruption, not a half line from a dying child.
        if self.requested_stop {
            return;
        }
        self.log.push(line.to_string());
        if line.contains("ERROR:") {
            self.error_line = Some(line.to_string());
        }
    }

    fn finalize(&mut self, status: io::Result<std::process::ExitStatus>) -> io::Result<()> {
        self.child = None;
        self.finished_at = Some(SystemTime::now());
        self.clear_busy();
        self.remove_stop_request_file();

        if self.requested_stop {
            self.state = TaskState::Stopped;
            self.result.get_or_insert_with(|| "Stopped".to_string());
            self.update_steps_after_stop();
        } else {
            match status {
                Ok(status) if status.success() => {
                    self.state = TaskState::Done;
                    self.result.get_or_insert_with(|| "Finished".to_string());
                    for step in &mut self.steps {
                        if step.state == StepState::Waiting || step.state == StepState::Running {
                            step.state = StepState::Ok;
                        }
                    }
                }
                _ => {
                    self.state = TaskState::Failed;
                    self.result.get_or_insert_with(|| "Failed".to_string());
                    for step in &mut self.steps {
                        if step.state == StepState::Running {
                            step.state = StepState::Failed;
                        }
                    }
                }
            }
        }
        self.state_dirty = true;
        self.write_state()
    }

    fn update_steps_after_stop(&mut self) {
        for step in &mut self.steps {
            if step.state == StepState::Running {
                step.state = StepState::Failed;
            }
        }
    }

    fn enforce_stop_timeout(&mut self) {
        if let Some(sent) = self.stop_sent
            && self.term_sent.is_none()
            && sent.elapsed() >= Duration::from_secs(2)
            && let Some(child) = &self.child
        {
            send_signal(child.pid, libc::SIGTERM);
            self.term_sent = Some(Instant::now());
        }
        if let Some(sent) = self.term_sent
            && sent.elapsed() >= Duration::from_secs(2)
            && let Some(child) = &self.child
        {
            send_signal(child.pid, libc::SIGKILL);
        }
    }

    fn check_external_stop_request(&mut self) {
        if STOP_FLAG.swap(false, Ordering::Relaxed) {
            self.stop();
        }
        if self.stop_request_path().is_file() {
            self.remove_stop_request_file();
            self.stop();
        }
    }

    fn remove_stop_request_file(&self) {
        let _ = std::fs::remove_file(self.stop_request_path());
    }

    /// Write the state file atomically (temp + rename).
    pub fn write_state(&mut self) -> io::Result<()> {
        let path = self.state_file();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let record = TaskRecord::from_task(self);
        let tmp = path.with_extension("tmp");
        let file = File::create(&tmp)?;
        serde_json::to_writer_pretty(file, &record)?;
        std::fs::rename(tmp, path)?;
        self.last_state_write = Some(Instant::now());
        self.state_dirty = false;
        Ok(())
    }

    fn maybe_write_state(&mut self) {
        let due = self
            .last_state_write
            .is_none_or(|t| t.elapsed() >= Duration::from_millis(500));
        if self.state_dirty || due {
            let _ = self.write_state();
        }
    }

    /// Load a previously saved task. A running task whose boot id or pid/comm
    /// does not match is reported as `DidNotFinish` instead of pretending.
    pub fn load(id: impl Into<String>, state_dir: impl AsRef<Path>) -> io::Result<Option<Self>> {
        let id = id.into();
        let path = state_dir.as_ref().join("tasks").join(format!("{id}.json"));
        if !path.is_file() {
            return Ok(None);
        }
        let file = File::open(&path)?;
        let record: TaskRecord = serde_json::from_reader(file)?;

        let mut state = record.state;
        if state == TaskState::Running {
            let current_boot = current_boot_id();
            let boot_ok = record
                .boot_id
                .as_deref()
                .map(|b| b == current_boot)
                .unwrap_or(false);
            let pid_ok = match (record.pid, record.comm.as_deref()) {
                (Some(pid), Some(comm)) => {
                    process_alive(pid) && read_proc_comm(pid).as_deref() == Some(comm)
                }
                _ => false,
            };
            if !boot_ok || !pid_ok {
                state = TaskState::DidNotFinish;
            }
        }

        let log_path = state_dir.as_ref().join("tasks").join(format!("{id}.log"));
        let mut log = LogPane::new();
        if let Ok(text) = std::fs::read_to_string(&log_path) {
            log.set_lines(text.lines().map(String::from).collect());
        }

        Ok(Some(Self {
            id: record.id,
            title: record.title,
            steps: record.steps,
            state,
            progress: record.progress,
            log,
            stoppable: record.stoppable,
            started_at: record.started_at.map(secs_to_system),
            finished_at: record.finished_at.map(secs_to_system),
            result: record.result,
            error_line: record.error_line,
            percent: record.percent,
            argv: record.argv,
            state_dir: state_dir.as_ref().to_path_buf(),
            runtime_dir: None,
            busy_tag: slugify(&id),
            busy_icon: "system-software-install".to_string(),
            child: None,
            requested_stop: state == TaskState::Stopped,
            stop_sent: None,
            term_sent: None,
            last_state_write: None,
            state_dirty: false,
            spinner: Spinner::default(),
            message: None,
        }))
    }

    /// Poll the state file and log for changes when this process does not own
    /// the child. This is the reattach path: the UI loads the record and then
    /// watches it until the task ends or is recognised as gone.
    pub fn reload(&mut self) -> io::Result<()> {
        let path = self.state_file();
        if !path.is_file() {
            if self.state == TaskState::Running {
                self.state = TaskState::DidNotFinish;
                self.state_dirty = true;
            }
            return Ok(());
        }
        let file = File::open(&path)?;
        let record: TaskRecord = serde_json::from_reader(file)?;

        // If the runner is still alive, keep the running state and sync the
        // rest; if it is gone, the task did not finish in a way we can report.
        let mut new_state = record.state;
        if new_state == TaskState::Running {
            let boot_ok = record
                .boot_id
                .as_deref()
                .map(|b| b == current_boot_id())
                .unwrap_or(false);
            let pid_ok = match (record.pid, record.comm.as_deref()) {
                (Some(pid), Some(comm)) => {
                    process_alive(pid) && read_proc_comm(pid).as_deref() == Some(comm)
                }
                _ => false,
            };
            if !boot_ok || !pid_ok {
                new_state = TaskState::DidNotFinish;
            }
        }

        self.title = record.title;
        self.steps = record.steps;
        self.state = new_state;
        self.progress = record.progress;
        self.percent = record.percent;
        self.result = record.result;
        self.error_line = record.error_line;
        self.stoppable = record.stoppable;
        self.started_at = record.started_at.map(secs_to_system);
        self.finished_at = record.finished_at.map(secs_to_system);

        let log_path = self.log_file();
        if let Ok(text) = std::fs::read_to_string(&log_path) {
            let lines: Vec<String> = text.lines().map(String::from).collect();
            if lines.len() != self.log.lines().len() {
                self.log.set_lines(lines);
            }
        }

        Ok(())
    }

    /// Elapsed time of the task, or zero if it has not started.
    pub fn elapsed(&self) -> Duration {
        match (self.started_at, self.finished_at) {
            (Some(start), Some(end)) => elapsed_between(start, end),
            (Some(start), None) => SystemTime::now()
                .duration_since(start)
                .unwrap_or(Duration::ZERO),
            _ => Duration::ZERO,
        }
    }

    pub fn elapsed_string(&self) -> String {
        let elapsed = self.elapsed();
        let secs = elapsed.as_secs();
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }

    fn set_message(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), Instant::now()));
    }

    fn clear_expired_message(&mut self) {
        if let Some((_, at)) = self.message
            && at.elapsed() >= Duration::from_secs(3)
        {
            self.message = None;
        }
    }

    // -----------------------------------------------------------------------
    // Busy bridge
    // -----------------------------------------------------------------------

    /// Write the bar-busy state files so the spinner appears while the task
    /// runs. A stoppable task also writes a `.stop` line; a non-stoppable one
    /// does not.
    pub fn write_busy(&self) -> io::Result<()> {
        let Some(base) = self.busy_base() else {
            return Ok(());
        };
        if let Some(dir) = base.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(base.with_extension("label"), &self.title)?;
        std::fs::write(base.with_extension("icon"), &self.busy_icon)?;
        std::fs::write(base.with_extension("percent"), format!("{}", self.percent))?;
        if self.stoppable {
            let stop_line = format!("kill -USR1 {}", std::process::id());
            let path = base.with_extension("stop");
            let tmp = path.with_extension("stop.tmp");
            std::fs::write(&tmp, stop_line)?;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&tmp, perms)?;
            std::fs::rename(tmp, path)?;
        }
        Ok(())
    }

    /// Remove the bar-busy state files so the spinner disappears.
    pub fn clear_busy(&self) {
        let Some(base) = self.busy_base() else {
            return;
        };
        for ext in ["label", "stop", "icon", "percent", "id"] {
            let _ = std::fs::remove_file(base.with_extension(ext));
        }
    }
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_lowercase()
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

/// A [`Screen`] that draws a task into the framework window.
pub struct TaskView {
    task: Task,
    log_focused: bool,
    keybar: Vec<KeyHint>,
}

impl TaskView {
    pub fn new(task: Task) -> Self {
        let mut view = Self {
            task,
            log_focused: false,
            keybar: Vec::new(),
        };
        view.update_keybar();
        view
    }

    fn update_keybar(&mut self) {
        let mut hints = vec![KeyHint::new("Esc", "close")];
        if self.task.state == TaskState::Running && self.task.stoppable {
            hints.push(KeyHint::new("s", "stop"));
        }
        if self.task.state == TaskState::Failed {
            hints.push(KeyHint::new("l", "log"));
        }
        if !self.task.log.following() {
            hints.push(KeyHint::new("End", "follow"));
        }
        self.keybar = hints;
    }

    pub fn task(&self) -> &Task {
        &self.task
    }

    pub fn task_mut(&mut self) -> &mut Task {
        &mut self.task
    }

    fn step_glyph(state: StepState) -> &'static str {
        match state {
            StepState::Waiting => "\u{25cb}", // ○
            StepState::Running => "\u{25cb}", // replaced by spinner when running
            StepState::Ok => "\u{2713}",      // ✓
            StepState::Failed => "\u{2717}",  // ✗
            StepState::Skipped => "-",
        }
    }
}

impl Screen for TaskView {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        let area = ctx.area;
        if area.width < 8 || area.height < 6 {
            put(
                ctx.buf,
                area.x,
                area.y,
                "task: terminal too small",
                area.width,
                ctx.design.style(TextStyle::Danger),
            );
            return;
        }

        let card_inner = draw_card(ctx.buf, area, ctx.design, None, true);
        let stride = ctx.design.row_stride();
        let mut y = card_inner.y;

        // Title and elapsed time.
        let elapsed = self.task.elapsed_string();
        let title_style = ctx.design.style(TextStyle::Title);
        put(
            ctx.buf,
            card_inner.x,
            y,
            &self.task.title,
            card_inner.width.saturating_sub(elapsed.len() as u16 + 2),
            title_style,
        );
        let dim = ctx.design.style(TextStyle::Dim);
        put_right(
            ctx.buf,
            Rect {
                x: card_inner.x,
                y,
                width: card_inner.width,
                height: 1,
            },
            y,
            &elapsed,
            dim,
            card_inner.x,
        );
        y += stride;

        // Checklist.
        for step in &self.task.steps {
            let glyph = if step.state == StepState::Running {
                self.task.spinner.glyph().to_string()
            } else {
                Self::step_glyph(step.state).to_string()
            };
            let glyph_style = match step.state {
                StepState::Ok => ctx
                    .design
                    .style(TextStyle::Body)
                    .fg(ctx.design.color(ColorRole::Ok)),
                StepState::Failed => ctx.design.style(TextStyle::Danger),
                StepState::Running => ctx.design.style(TextStyle::Title),
                _ => ctx.design.style(TextStyle::Dim),
            };
            let mut x = put(ctx.buf, card_inner.x, y, &glyph, 2, glyph_style);
            x = put(ctx.buf, x, y, " ", 1, ctx.design.style(TextStyle::Body));
            let label_style = if step.state == StepState::Running {
                ctx.design.style(TextStyle::Title)
            } else {
                ctx.design.style(TextStyle::Body)
            };
            let detail_room = step
                .detail
                .as_deref()
                .map(|d| UnicodeWidthStr::width(d) as u16 + 2)
                .unwrap_or(0);
            let label_room = card_inner.right().saturating_sub(x + detail_room + 1);
            x = put_trunc(ctx.buf, x, y, &step.label, label_room, label_style);
            if let Some(detail) = &step.detail {
                x = put(
                    ctx.buf,
                    x,
                    y,
                    "  ",
                    card_inner.right().saturating_sub(x),
                    label_style,
                );
                put_trunc(
                    ctx.buf,
                    x,
                    y,
                    detail,
                    card_inner.right().saturating_sub(x),
                    ctx.design.style(TextStyle::Dim),
                );
            }
            y += stride;
        }

        // Progress / spinner line while running.
        let progress_height = if self.task.state == TaskState::Running {
            1u16
        } else {
            0u16
        };
        if progress_height > 0 && y + progress_height <= card_inner.bottom() {
            draw_progress(
                ctx.buf,
                Rect {
                    x: card_inner.x,
                    y,
                    width: card_inner.width,
                    height: 1,
                },
                ctx.design,
                self.task.progress,
                &format!("{} running", self.task.spinner.glyph()),
                ColorRole::Accent,
            );
            y += progress_height * stride;
        }

        // Result block for terminal states.
        let is_terminal = matches!(
            self.task.state,
            TaskState::Done | TaskState::Failed | TaskState::Stopped | TaskState::DidNotFinish
        );
        let result_height = if is_terminal { 3u16 } else { 0u16 };
        let remaining = card_inner.bottom().saturating_sub(y);
        let log_height = remaining.saturating_sub(result_height * stride).max(3);

        // Log pane.
        let log_area = Rect {
            x: card_inner.x,
            y,
            width: card_inner.width,
            height: log_height,
        };
        self.task.log.render(log_area, ctx.buf, ctx.design);
        y += log_height;

        // Result block.
        if is_terminal && y + result_height * stride <= card_inner.bottom() {
            let (text, role) = match self.task.state {
                TaskState::Done => (
                    self.task.result.as_deref().unwrap_or("Finished"),
                    ColorRole::Ok,
                ),
                TaskState::Failed => (
                    self.task
                        .error_line
                        .as_deref()
                        .unwrap_or(self.task.result.as_deref().unwrap_or("Failed")),
                    ColorRole::Danger,
                ),
                TaskState::Stopped => (
                    self.task.result.as_deref().unwrap_or("Stopped"),
                    ColorRole::Warn,
                ),
                TaskState::DidNotFinish => (
                    "Did not finish (reboot or process replaced)",
                    ColorRole::Warn,
                ),
                _ => ("", ColorRole::Text),
            };
            let style = ctx.design.style(TextStyle::Body).fg(ctx.design.color(role));
            put(
                ctx.buf,
                card_inner.x,
                y,
                "Result: ",
                card_inner.width,
                ctx.design.style(TextStyle::Dim),
            );
            let prefix = "Result: ";
            let prefix_width = UnicodeWidthStr::width(prefix) as u16;
            put_trunc(
                ctx.buf,
                card_inner.x + prefix_width,
                y,
                text,
                card_inner.width.saturating_sub(prefix_width),
                style,
            );
        }

        // Transient message (e.g. "this task cannot be stopped").
        if let Some((msg, _)) = &self.task.message {
            let msg_y = card_inner.bottom().saturating_sub(1);
            put(
                ctx.buf,
                card_inner.x,
                msg_y,
                msg,
                card_inner.width,
                ctx.design
                    .style(TextStyle::Body)
                    .fg(ctx.design.color(ColorRole::Warn)),
            );
        }
    }

    fn key(&mut self, key: KeyEvent, _design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }

        if self.log_focused && self.task.log.handle(key, 10) {
            self.update_keybar();
            return Outcome::Continue;
        }

        match key.code {
            KeyCode::Char('s') | KeyCode::Char('S') => {
                if self.task.child.is_some() {
                    // This view owns the child (a test or a runner): stop it.
                    self.task.stop();
                } else {
                    // A viewer of a detached runner: ask the runner to stop.
                    self.task.request_stop();
                }
                self.update_keybar();
                Outcome::Continue
            }
            KeyCode::Char('l') | KeyCode::Char('L') => {
                self.log_focused = !self.log_focused;
                self.update_keybar();
                Outcome::Continue
            }
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                Outcome::Done(crate::app::exit::ESCAPE)
            }
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End => {
                let _ = self.task.log.handle(key, 10);
                self.update_keybar();
                Outcome::Continue
            }
            _ => Outcome::Continue,
        }
    }

    fn tick(&mut self) -> Outcome {
        if self.task.child.is_some() {
            if let Err(error) = self.task.tick() {
                self.task.state = TaskState::Failed;
                self.task.result = Some(format!("task controller failed: {error}"));
                self.task.clear_busy();
            }
        } else if self.task.state == TaskState::Running {
            // Reattach path: this UI does not own the child, so it polls the
            // state file and log instead of draining a channel.
            let _ = self.task.reload();
        }
        self.update_keybar();
        Outcome::Continue
    }

    fn needs_tick(&self) -> Option<Duration> {
        // 10 fps while the task is alive; idle tasks are event-driven.
        match self.task.state {
            TaskState::Running | TaskState::Stopped => Some(Duration::from_millis(100)),
            _ => None,
        }
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        Some(&self.keybar)
    }
}

// Helper used inside the view: right-aligned text within a rect.
fn put_right(buf: &mut Buffer, area: Rect, y: u16, text: &str, style: Style, floor: u16) {
    let width = UnicodeWidthStr::width(text) as u16;
    let x = area.right().saturating_sub(width + 1);
    if x > floor && x >= area.left() {
        put(buf, x, y, text, area.right().saturating_sub(x), style);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dirs() -> (PathBuf, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let base =
            std::env::temp_dir().join(format!("lazyos-task-test-{}-{}", std::process::id(), n));
        let state = base.join("state");
        let runtime = base.join("runtime");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&state).unwrap();
        std::fs::create_dir_all(&runtime).unwrap();
        (state, runtime)
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

    fn run_task_to_end(task: &mut Task, argv: &[&str]) {
        task.spawn(argv).unwrap();
        while matches!(task.state, TaskState::Running | TaskState::Stopped) {
            task.tick().unwrap();
            if task.state == TaskState::Running {
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }

    #[test]
    fn stop_is_not_failure() {
        let (state, runtime) = temp_dirs();
        let mut task = Task::new("stop-test", "stop test", &state).with_runtime_dir(&runtime);
        task.set_stoppable(true);
        task.add_step("sleep");
        task.spawn(&["/bin/sh", "-c", "sleep 10"]).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        task.stop();
        // Wait for SIGTERM/SIGKILL to finish the child.
        wait_for(Duration::from_secs(3), || {
            task.tick().unwrap();
            !matches!(task.state, TaskState::Running)
        });
        assert_eq!(task.state, TaskState::Stopped);
        assert!(task.result.as_deref().unwrap_or("").contains("Stopped"));
    }

    #[test]
    fn late_output_after_stop_is_dropped() {
        let (state, runtime) = temp_dirs();
        let mut task =
            Task::new("late-test", "late output test", &state).with_runtime_dir(&runtime);
        task.set_stoppable(true);
        // The script prints one line, then ignores SIGINT and tries to print more.
        task.spawn(&[
            "/bin/sh",
            "-c",
            "echo before; trap '' INT; sleep 0.5; echo after; sleep 10",
        ])
        .unwrap();
        wait_for(Duration::from_secs(1), || {
            task.tick().unwrap();
            task.log.lines().contains(&"before".to_string())
        });
        task.stop();
        // The "after" line may be produced by the child, but it must not be
        // kept because the task was already stopped.
        wait_for(Duration::from_secs(3), || {
            task.tick().unwrap();
            !matches!(task.state, TaskState::Running)
        });
        assert!(!task.log.lines().contains(&"after".to_string()));
    }

    #[test]
    fn state_file_is_atomic() {
        let (state, _) = temp_dirs();
        let mut task = Task::new("atomic", "atomic test", &state);
        task.add_step("one");
        task.set_progress(0.42);
        task.write_state().unwrap();

        let path = task.state_file();
        // There must be no stray temp file left behind.
        let tmp = path.with_extension("tmp");
        assert!(!tmp.exists(), "temp state file must be renamed away");
        assert!(path.exists());

        let loaded = Task::load("atomic", &state).unwrap().expect("task loads");
        assert_eq!(loaded.state, TaskState::Queued);
        assert_eq!(loaded.progress, 0.42);
        assert_eq!(loaded.steps.len(), 1);
    }

    #[test]
    fn pid_identity_rejects_reuse() {
        let (state, _) = temp_dirs();
        let record = TaskRecord {
            id: "pid-test".to_string(),
            title: "pid test".to_string(),
            steps: vec![Step::new("one")],
            state: TaskState::Running,
            progress: 0.0,
            stoppable: true,
            percent: 0,
            result: None,
            error_line: None,
            argv: None,
            started_at: None,
            finished_at: None,
            // Use this process's pid but a comm that cannot match.
            pid: Some(std::process::id()),
            comm: Some("definitely-not-lazyos".to_string()),
            boot_id: Some(current_boot_id()),
        };
        let path = state.join("tasks").join("pid-test.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = File::create(&path).unwrap();
        serde_json::to_writer_pretty(file, &record).unwrap();

        let loaded = Task::load("pid-test", &state)
            .unwrap()
            .expect("record exists");
        assert_eq!(loaded.state, TaskState::DidNotFinish);
    }

    #[test]
    fn busy_tag_matches_stop() {
        let (state, runtime) = temp_dirs();
        let mut task = Task::new("busy-stop", "busy stop test", &state).with_runtime_dir(&runtime);
        task.set_stoppable(true);
        task.spawn(&["/bin/sh", "-c", "sleep 5"]).unwrap();

        let base = task.busy_base().unwrap();
        assert!(base.with_extension("label").exists());
        assert!(base.with_extension("stop").exists());
        let stop_line = std::fs::read_to_string(base.with_extension("stop")).unwrap();
        assert!(stop_line.starts_with("kill -USR1 "));

        task.stop();
        wait_for(Duration::from_secs(2), || {
            task.tick().unwrap();
            task.state == TaskState::Stopped
        });
        task.clear_busy();
        assert!(!base.with_extension("stop").exists());
    }

    #[test]
    fn non_stoppable_task_writes_no_stop_file() {
        let (state, runtime) = temp_dirs();
        let mut task = Task::new("no-stop", "no stop test", &state).with_runtime_dir(&runtime);
        task.set_stoppable(false);
        task.spawn(&["/bin/sh", "-c", "sleep 5"]).unwrap();
        let base = task.busy_base().unwrap();
        assert!(base.with_extension("label").exists());
        assert!(!base.with_extension("stop").exists());
        task.stop();
        assert_eq!(task.state, TaskState::Running);
    }

    #[test]
    fn success_finishes_done() {
        let (state, runtime) = temp_dirs();
        let mut task = Task::new("ok", "ok test", &state).with_runtime_dir(&runtime);
        task.add_step("one");
        run_task_to_end(&mut task, &["/bin/sh", "-c", "echo success"]);
        assert_eq!(task.state, TaskState::Done);
        assert!(task.log.lines().contains(&"success".to_string()));
    }

    #[test]
    fn failure_keeps_the_error_line() {
        let (state, runtime) = temp_dirs();
        let mut task = Task::new("fail", "fail test", &state).with_runtime_dir(&runtime);
        task.add_step("one");
        run_task_to_end(
            &mut task,
            &["/bin/sh", "-c", "echo ERROR: disk full; exit 1"],
        );
        assert_eq!(task.state, TaskState::Failed);
        assert_eq!(task.error_line.as_deref(), Some("ERROR: disk full"));
    }

    #[test]
    fn stop_key_on_non_stoppable_says_why() {
        let (state, runtime) = temp_dirs();
        let mut task =
            Task::new("no-stop-key", "no stop key test", &state).with_runtime_dir(&runtime);
        task.set_stoppable(false);
        task.spawn(&["/bin/sh", "-c", "sleep 5"]).unwrap();
        let mut view = TaskView::new(task);
        let design = crate::design::Design::solarized();
        let key = ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char('s'),
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        view.key(key, &design);
        assert_eq!(view.task().state, TaskState::Running);
        assert!(
            view.task().message.is_some(),
            "a non-stoppable task tells the person why it cannot stop"
        );
    }

    #[test]
    fn attach_after_finish_shows_the_saved_result() {
        let (state, _) = temp_dirs();
        let mut task = Task::new("reattach", "reattach test", &state);
        task.add_step("one");
        task.add_step("two");
        run_task_to_end(&mut task, &["/bin/sh", "-c", "echo done; exit 0"]);
        assert_eq!(task.state, TaskState::Done);

        let loaded = Task::load("reattach", &state)
            .unwrap()
            .expect("state file exists after the task finishes");
        assert_eq!(loaded.state, TaskState::Done);
        assert_eq!(loaded.result.as_deref(), Some("Finished"));
        assert!(loaded.log.lines().contains(&"done".to_string()));
    }

    #[test]
    fn ignore_sigint_is_stopped() {
        let (state, runtime) = temp_dirs();
        let mut task = Task::new("ignore", "ignore INT test", &state).with_runtime_dir(&runtime);
        task.set_stoppable(true);
        task.add_step("one");
        task.spawn(&["/bin/sh", "-c", "trap '' INT; sleep 30"])
            .unwrap();
        std::thread::sleep(Duration::from_millis(50));
        task.stop();
        wait_for(Duration::from_secs(4), || {
            task.tick().unwrap();
            task.state == TaskState::Stopped
        });
        assert_eq!(task.state, TaskState::Stopped);
    }

    #[test]
    fn the_record_round_trips_the_command() {
        let (state, _) = temp_dirs();
        let mut task =
            Task::new("argv", "argv test", &state).with_command(&["/bin/sh", "-c", "echo hi"]);
        task.write_state().unwrap();
        let loaded = Task::load("argv", &state).unwrap().expect("task loads");
        assert_eq!(
            loaded.argv,
            Some(vec!["/bin/sh".into(), "-c".into(), "echo hi".into()])
        );
    }

    #[test]
    fn a_viewer_stop_writes_the_request_file_not_a_signal() {
        let (state, _) = temp_dirs();
        // A viewer's Task has no child: it asks the runner through a file.
        let task = Task::new("viewer", "viewer test", &state);
        assert!(task.child.is_none());
        task.request_stop();
        assert!(task.stop_request_path().is_file());
    }

    #[test]
    fn stopping_a_finished_task_does_nothing() {
        let (state, _) = temp_dirs();
        let mut task = Task::new("done-stop", "stop after done", &state);
        task.set_stoppable(true);
        task.add_step("one");
        run_task_to_end(&mut task, &["/bin/sh", "-c", "exit 0"]);
        assert_eq!(task.state, TaskState::Done);
        let result = task.result.clone();

        // The key is still bound; it must not turn a finished task into a
        // stopped one, and it must not rewrite the result.
        task.stop();
        assert_eq!(task.state, TaskState::Done);
        assert_eq!(task.result, result);
    }

    #[test]
    fn the_log_is_a_ring_that_drops_the_oldest_lines() {
        let (state, _) = temp_dirs();
        let mut task = Task::new("ring", "ring test", &state);
        for n in 0..5_001 {
            task.log.push(format!("line {n}"));
        }
        // 5,001 lines trip the cap once and drop the oldest 1,000.
        assert_eq!(task.log.lines().len(), 4_001);
        assert_eq!(task.log.lines().first().unwrap(), "line 1000");
    }
}
