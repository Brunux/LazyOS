//! Detached installer worker, execution engine, and state IPC protocol.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::config::InstallConfig;

/// The six sequential phases of installation.
pub const PHASES: &[&str] = &[
    "Partitioning storage (ChromeOS GPT)",
    "Formatting filesystems (ext2, ext4)",
    "Copying root filesystem (rsync)",
    "Configuring system identity & user",
    "Generating signed depthcharge kernel FIT",
    "Verifying vboot signature & finalizing",
];

/// Progress state written by worker to IPC state file.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerState {
    pub phase_index: usize,
    pub phase_name: String,
    pub percent: u8,
    pub status: String, // "running", "success", "failed"
    pub error: Option<String>,
    pub log_lines: Vec<String>,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    pub pid: u32,
    pub dry_run: bool,
}

impl Default for WorkerState {
    fn default() -> Self {
        Self {
            phase_index: 0,
            phase_name: PHASES[0].to_string(),
            percent: 0,
            status: "running".to_string(),
            error: None,
            log_lines: Vec::new(),
            started_at: current_epoch_secs(),
            finished_at: None,
            pid: std::process::id(),
            dry_run: false,
        }
    }
}

pub fn state_dir() -> PathBuf {
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        let p = PathBuf::from(runtime).join("lazyos-install");
        let _ = fs::create_dir_all(&p);
        p
    } else {
        let p = PathBuf::from("/tmp/lazyos-install");
        let _ = fs::create_dir_all(&p);
        p
    }
}

pub fn state_file_path() -> PathBuf {
    state_dir().join("state.json")
}

pub fn log_file_path() -> PathBuf {
    let state_dir = state_dir();
    let now = current_epoch_secs();
    state_dir.join(format!("install-{now}.log"))
}

pub fn load_state() -> Option<WorkerState> {
    let path = state_file_path();
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn save_state(state: &WorkerState) -> io::Result<()> {
    let path = state_file_path();
    let json = serde_json::to_string_pretty(state)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, json)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn current_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Run the installation worker (either real or simulated dry-run).
pub fn run_worker(config: InstallConfig, cancel_token: Option<Arc<AtomicBool>>) -> io::Result<()> {
    let log_path = log_file_path();
    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let mut state = WorkerState {
        phase_index: 0,
        phase_name: PHASES[0].to_string(),
        percent: 0,
        status: "running".to_string(),
        error: None,
        log_lines: Vec::new(),
        started_at: current_epoch_secs(),
        finished_at: None,
        pid: std::process::id(),
        dry_run: config.dry_run,
    };

    let log_msg = |state: &mut WorkerState, log_file: &mut fs::File, msg: &str| {
        let line = format!("[{}] {}", state.phase_name, msg);
        let _ = writeln!(log_file, "{}", line);
        let _ = log_file.flush();
        state.log_lines.push(line);
        if state.log_lines.len() > 1000 {
            state.log_lines.remove(0);
        }
        let _ = save_state(state);
    };

    log_msg(
        &mut state,
        &mut log_file,
        &format!("Starting LazyOS installation to {}", config.target_disk),
    );

    if config.dry_run {
        return run_dry_run_simulation(&config, state, log_file, cancel_token);
    }

    // Phase 1: Partitioning storage
    state.phase_index = 0;
    state.phase_name = PHASES[0].to_string();
    state.percent = 5;
    log_msg(
        &mut state,
        &mut log_file,
        "Creating ChromeOS GPT table layout",
    );
    thread::sleep(Duration::from_millis(500));

    // Stop and mask udisks2 to avoid auto-mounting mid-partitioning
    let _ = Command::new("systemctl")
        .args(["mask", "--runtime", "udisks2"])
        .status();
    let _ = Command::new("systemctl").args(["stop", "udisks2"]).status();

    // Execute install-emmc.sh
    let repo_dir = find_repo_dir().unwrap_or_else(|| "/home/brunux/Dev/LazyOS".to_string());
    let script_path = Path::new(&repo_dir).join("scripts/install-emmc.sh");

    if !script_path.exists() {
        let err = format!("Installer script not found: {}", script_path.display());
        state.status = "failed".to_string();
        state.error = Some(err.clone());
        log_msg(&mut state, &mut log_file, &format!("ERROR: {err}"));
        return Ok(());
    }

    state.phase_index = 1;
    state.phase_name = PHASES[1].to_string();
    state.percent = 20;
    log_msg(
        &mut state,
        &mut log_file,
        "Invoking installer backend engine",
    );

    let mut child = match Command::new(&script_path)
        .env("DEVICE_BLOCK", &config.target_disk)
        .env("CONFIRM", "yes")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let err = format!("Failed to spawn {}: {}", script_path.display(), e);
            state.status = "failed".to_string();
            state.error = Some(err.clone());
            log_msg(&mut state, &mut log_file, &format!("ERROR: {err}"));
            return Ok(());
        }
    };

    use std::io::BufRead;
    if let Some(stdout) = child.stdout.take() {
        let reader = io::BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            log_msg(&mut state, &mut log_file, &line);
            if line.contains("Copying the running system") {
                state.phase_index = 2;
                state.phase_name = PHASES[2].to_string();
                state.percent = 35;
            } else if line.contains("Building kpart") {
                state.phase_index = 4;
                state.phase_name = PHASES[4].to_string();
                state.percent = 80;
            } else if line.contains("Writing kpart") {
                state.phase_index = 5;
                state.phase_name = PHASES[5].to_string();
                state.percent = 92;
            }
        }
    }

    let status = child.wait()?;
    let _ = Command::new("systemctl")
        .args(["unmask", "--runtime", "udisks2"])
        .status();
    let _ = Command::new("systemctl")
        .args(["start", "udisks2"])
        .status();

    if status.success() {
        state.phase_index = 5;
        state.phase_name = PHASES[5].to_string();
        state.percent = 100;
        state.status = "success".to_string();
        state.finished_at = Some(current_epoch_secs());
        log_msg(
            &mut state,
            &mut log_file,
            "Installation completed successfully!",
        );
    } else {
        state.status = "failed".to_string();
        state.error = Some(format!(
            "Installer script failed with exit code {:?}",
            status.code()
        ));
        log_msg(
            &mut state,
            &mut log_file,
            "ERROR: Installation script exited with error",
        );
    }

    let _ = save_state(&state);
    Ok(())
}

fn run_dry_run_simulation(
    config: &InstallConfig,
    mut state: WorkerState,
    mut log_file: fs::File,
    cancel_token: Option<Arc<AtomicBool>>,
) -> io::Result<()> {
    let simulation_steps: &[(&str, u8, u64)] = &[
        (
            "Wiping partition headers and writing ChromeOS GPT table",
            10,
            400,
        ),
        (
            "Adding pmOS_kernel (128 MiB), pmOS_boot (512 MiB), and pmOS_root",
            18,
            400,
        ),
        (
            "Formatting pmOS_boot partition (/dev/mmcblk0p2) with ext2",
            25,
            400,
        ),
        (
            "Formatting pmOS_root partition (/dev/mmcblk0p3) with ext4",
            32,
            500,
        ),
        ("Mounting filesystems into /tmp/emmc-mnt-hayato", 38, 300),
        (
            "rsync: synchronizing system binaries and shared libraries...",
            50,
            700,
        ),
        (
            "rsync: synchronizing desktop configuration and assets...",
            65,
            700,
        ),
        ("Writing /etc/fstab with persistent UUID entries", 72, 300),
        (
            &format!(
                "Configuring primary user '{}' and credentials",
                config.username
            ),
            78,
            300,
        ),
        (
            &format!(
                "Configuring timezone '{}' and locale '{}'",
                config.timezone, config.locale
            ),
            82,
            300,
        ),
        (
            &format!("Setting system hostname to '{}'", config.hostname),
            85,
            200,
        ),
        (
            "Building signed depthcharge kernel FIT image (make-kpart.sh)",
            90,
            600,
        ),
        ("Writing signed kpart image to /dev/mmcblk0p1", 95, 500),
        (
            "Verifying vboot FIT signature (futility show: Body verification succeeded)",
            98,
            400,
        ),
        (
            "Unmounting target filesystems and syncing storage buffers",
            100,
            300,
        ),
    ];

    let log_msg = |state: &mut WorkerState, log_file: &mut fs::File, msg: &str| {
        let line = format!("[{}] {}", state.phase_name, msg);
        let _ = writeln!(log_file, "{}", line);
        let _ = log_file.flush();
        state.log_lines.push(line);
        if state.log_lines.len() > 1000 {
            state.log_lines.remove(0);
        }
        let _ = save_state(state);
    };

    for &(desc, pct, delay_ms) in simulation_steps {
        if let Some(ref cancel) = cancel_token {
            if cancel.load(Ordering::SeqCst) {
                state.status = "failed".to_string();
                state.error = Some("Installation cancelled by user".to_string());
                log_msg(&mut state, &mut log_file, "Installation cancelled by user");
                return Ok(());
            }
        }

        if pct < 20 {
            state.phase_index = 0;
        } else if pct < 35 {
            state.phase_index = 1;
        } else if pct < 70 {
            state.phase_index = 2;
        } else if pct < 88 {
            state.phase_index = 3;
        } else if pct < 96 {
            state.phase_index = 4;
        } else {
            state.phase_index = 5;
        }
        state.phase_name = PHASES[state.phase_index].to_string();
        state.percent = pct;

        log_msg(&mut state, &mut log_file, desc);
        thread::sleep(Duration::from_millis(delay_ms));
    }

    state.status = "success".to_string();
    state.finished_at = Some(current_epoch_secs());
    state.percent = 100;
    log_msg(
        &mut state,
        &mut log_file,
        "Dry-run installation verified successfully!",
    );
    let _ = save_state(&state);
    Ok(())
}

fn find_repo_dir() -> Option<String> {
    if let Ok(dir) = std::env::var("LAZYOS_REPO") {
        if Path::new(&dir).join("scripts/install-emmc.sh").is_file() {
            return Some(dir);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let path = PathBuf::from(home).join("Dev/LazyOS");
        if path.join("scripts/install-emmc.sh").is_file() {
            return Some(path.display().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_json_serialization_round_trip() {
        let state = WorkerState {
            phase_index: 2,
            phase_name: PHASES[2].to_string(),
            percent: 50,
            status: "running".to_string(),
            error: None,
            log_lines: vec!["line 1".into(), "line 2".into()],
            started_at: 1000,
            finished_at: None,
            pid: 1234,
            dry_run: true,
        };

        let json = serde_json::to_string(&state).unwrap();
        let parsed: WorkerState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, parsed);
    }

    #[test]
    fn all_phases_are_declared() {
        assert_eq!(PHASES.len(), 6);
        assert!(PHASES[0].contains("Partitioning"));
        assert!(PHASES[1].contains("Formatting"));
        assert!(PHASES[2].contains("Copying"));
        assert!(PHASES[3].contains("Configuring"));
        assert!(PHASES[4].contains("Generating"));
        assert!(PHASES[5].contains("Verifying"));
    }
}
