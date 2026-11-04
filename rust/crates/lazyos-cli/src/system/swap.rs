//! Auxiliary swap: a removable card or stick used as more memory (`rofi-swap`).
//!
//! The candidate list is `disk::candidates` — the same safety as Format a disk —
//! and the guard is asked again after the pick because setup rewrites the whole
//! disk. The steps are: a fresh GPT, one Linux-swap partition with the
//! `lazyos_swap` label, a marked block in `/etc/fstab` (replaced, not appended),
//! `systemctl daemon-reload`, and `swapon -p 10` with a read-back that joins
//! the kernel's list onto one line (zram is always first).
//!
//! The priority is 10, below zram's 100 (`scripts/rootfs-setup.sh`), so the
//! compressed RAM is used first and the card is the spill.
//!
//! It is **not stoppable**, like Format a disk: a kill between the table and
//! `mkswap` leaves a card that is neither a filesystem nor swap.

use std::path::Path;
use std::time::{Duration, Instant};

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::disk::{self, Disk, Facts};
use super::fstab;
use super::pick::{Picker, Question, row};
use super::step::{self, Failed, Step};
use super::{missing, out, say};

/// What the menu needs before it offers anything.
pub const NEEDED: &[&str] = &["parted", "mkswap", "swapon", "swapoff", "blkid", "lsblk"];

/// The state of the auxiliary swap, readable from the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// The partition node carrying `lazyos_swap`, if any.
    pub part: Option<String>,
    /// The size `lsblk` reports for the partition.
    pub size: Option<String>,
    /// The priority the kernel shows for the partition, if active.
    pub priority: Option<u8>,
    /// Whether `/etc/fstab` asks for it (the block or a hand-written line).
    pub configured: bool,
    /// Whether the partition is currently active in the kernel's list.
    pub active: bool,
}

impl Status {
    /// Read the current state from the machine.
    pub fn from_machine() -> Self {
        let part = out(&["blkid", "-L", "lazyos_swap"]).unwrap_or_default();
        let part = if part.is_empty() { None } else { Some(part) };

        let size = part
            .as_ref()
            .and_then(|p| out(&["lsblk", "-dno", "SIZE", p]));
        let (priority, active) = if let Some(ref p) = part {
            let listing = out(&["swapon", "--show=NAME,PRIO", "--noheadings"]).unwrap_or_default();
            let mut prio = None;
            let mut active = false;
            for line in listing.lines() {
                let mut fields = line.split_whitespace();
                if fields.next() == Some(p.as_str()) {
                    active = true;
                    if let Some(pr) = fields.next()
                        && let Ok(pr) = pr.parse::<u8>()
                    {
                        prio = Some(pr);
                    }
                    break;
                }
            }
            (prio, active)
        } else {
            (None, false)
        };

        let configured =
            fstab::configured(&std::fs::read_to_string("/etc/fstab").unwrap_or_default());

        Self {
            part,
            size,
            priority,
            configured,
            active,
        }
    }

    /// A one-line summary for `--status`.
    pub fn line(&self) -> String {
        let mut out = String::new();
        if let Some(ref part) = self.part {
            out.push_str(part);
            out.push(' ');
            if let Some(ref size) = self.size {
                out.push_str(size);
                out.push(' ');
            }
            if let Some(prio) = self.priority {
                out.push_str(&format!("pri={prio} "));
            }
            if self.active {
                out.push_str("active ");
            }
        }
        if self.configured {
            out.push_str("configured ");
        }
        out.trim().to_string()
    }

    /// The detail lines the status row shows.
    pub fn detail(&self) -> String {
        if let Some(ref part) = self.part {
            let mut d = format!("{part} · {}", self.size.as_deref().unwrap_or("?"));
            if self.active {
                if let Some(prio) = self.priority {
                    d.push_str(&format!(" · priority {prio}, after the compressed RAM"));
                }
            } else {
                d.push_str(" · not active now");
            }
            d
        } else {
            "the lazyos_swap card is not inserted · a boot without it waits 15 s at most".into()
        }
    }
}

/// The whole-card size and the smaller fixed sizes (with 64 MiB margin).
fn sizes(total_mib: u64) -> Vec<(String, String)> {
    let mut out = Vec::new();
    out.push(("100%".to_string(), "The whole card".to_string()));
    for gib in [2, 4, 8] {
        let need = gib * 1024 + 64;
        if total_mib >= need {
            out.push((
                format!("{gib}GiB"),
                format!("{} GiB · the rest of the card stays unpartitioned", gib),
            ));
        }
    }
    out
}

/// The `parted` end argument for the chosen size.
fn extent(answer: &str, total_mib: u64) -> String {
    match answer {
        "100%" => "100%".to_string(),
        "2GiB" => "2049MiB".to_string(),
        "4GiB" => "4097MiB".to_string(),
        "8GiB" => "8193MiB".to_string(),
        _ => format!("{}MiB", total_mib.min(100)),
    }
}

/// Whether another disk already carries the `lazyos_swap` label.
fn label_taken(existing: &str, dev: &str, parent: &str, name: &str) -> bool {
    if existing.is_empty() {
        return false;
    }
    existing != dev && parent != name
}

/// Read `/proc/swaps`-style listing and return it joined onto one line.
/// The newline→space fix is what makes `case " $swaps "` find the card.
/// Returns a string with leading and trailing spaces so patterns like
/// ` {part} ` match even the first and last entries.
fn joined(listing: &str) -> String {
    let mut s = String::new();
    s.push(' ');
    s.push_str(&listing.lines().collect::<Vec<_>>().join(" "));
    s.push(' ');
    s
}

/// Whether the partition is in the joined kernel swap list.
fn active_in(joined_listing: &str, part: &str) -> bool {
    let needle = format!(" {part} ");
    joined_listing.contains(&needle)
}

/// The priority from the joined listing, if present.
/// Finds the part name, then returns the next token that parses as u8.
#[allow(dead_code)]
fn priority_of(joined_listing: &str, part: &str) -> Option<u8> {
    let mut found_part = false;
    for token in joined_listing.split_whitespace() {
        if found_part && let Ok(prio) = token.parse::<u8>() {
            return Some(prio);
        }
        if token == part {
            found_part = true;
        }
    }
    None
}

/// The partition-table and partition steps.
fn partition_steps(dev: &str, end: &str) -> Vec<Step> {
    vec![
        Step::root("partition-table", &["parted", "-s", dev, "mktable", "gpt"]),
        Step::root(
            "partition",
            &[
                "parted",
                "-s",
                dev,
                "mkpart",
                "primary",
                "linux-swap",
                "1MiB",
                end,
            ],
        ),
    ]
}

/// The `mkswap` step (with `-f` because the new partition may carry an old signature).
fn swap_area_step(part: &str) -> Step {
    Step::root("swap-area", &["mkswap", "-f", "-L", fstab::LABEL, part])
}

/// The activation step with the priority.
fn activation_step(part: &str) -> Step {
    Step::root("activation", &["swapon", "-p", "10", part])
}

/// The systemd reload step.
fn reload_step() -> Step {
    Step::root("systemd", &["systemctl", "daemon-reload"])
}

/// Wait for a path to exist with a custom timeout (used by tests).
#[allow(dead_code)]
fn wait_for_in(node: &str, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if Path::new(node).exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Path::new(node).exists()
}

/// Set up the auxiliary swap on `dev` with the chosen `end` and allocation `alloc`.
/// `fstab_text` is the current `/etc/fstab` so we can write the new one atomically.
fn apply(dev: &str, end: &str, alloc: &str, fstab_text: &str) -> Result<String, Failed> {
    let part = fstab::partition(dev);

    let steps = partition_steps(dev, end);
    step::run(&steps)?;

    if !super::wait_for(&part) {
        return Err(Failed {
            step: "partition",
            reason: format!("{part} did not appear: the kernel still has the old table"),
        });
    }

    step::run(&[swap_area_step(&part)])?;

    let new_fstab = fstab::with_swap(fstab_text);
    fstab::replace(&new_fstab)?;

    step::run(&[reload_step()])?;

    step::run(&[activation_step(&part)])?;

    let listing = out(&["swapon", "--show=NAME", "--noheadings"]).unwrap_or_default();
    let joined = joined(&listing);
    if !active_in(&joined, &part) {
        // Activation did not stick: turn it off and roll back fstab.
        let _ = out(&["swapoff", &part]);
        let stripped = fstab::strip(fstab_text);
        let _ = fstab::replace(&stripped);
        return Err(Failed {
            step: "activation",
            reason: format!("swapon did not report {part} as active"),
        });
    }

    Ok(format!(
        "{dev} is {alloc} of swap (label {}) · active now and on every boot with the card in",
        fstab::LABEL
    ))
}

/// Turn the auxiliary swap off, given its current `status` and the current fstab.
fn turn_off(status: &Status, fstab_text: &str) -> Result<String, Failed> {
    let part = match status.part {
        Some(ref p) => p.clone(),
        None => {
            // No partition, just the fstab block to remove.
            let stripped = fstab::strip(fstab_text);
            fstab::replace(&stripped)?;
            step::run(&[reload_step()])?;
            return Ok("the /etc/fstab entry is gone; the card is no longer needed at boot".into());
        }
    };

    let listing = out(&["swapon", "--show=NAME", "--noheadings"]).unwrap_or_default();
    let joined = joined(&listing);
    let active = active_in(&joined, &part);

    let steps = if active {
        vec![Step::root("swapoff", &["swapoff", part.as_str()])]
    } else {
        Vec::new()
    };

    step::run(&steps)?;

    let stripped = fstab::strip(fstab_text);
    fstab::replace(&stripped)?;

    step::run(&[reload_step()])?;

    if active {
        Ok(format!(
            "{part} is no longer used; the partition is still there (Format a disk erases it)"
        ))
    } else {
        Ok("the /etc/fstab entry is gone; the card is no longer needed at boot".into())
    }
}

/// The questions a swap setup asks: the disk, the size, and the confirm.
fn questions(found: &[Disk]) -> Vec<Question> {
    let (rows, keys) = {
        let mut rows = Vec::new();
        let mut keys = Vec::new();
        for disk in found {
            rows.push(row(
                disk.dev(),
                format!("{} · {}", disk.model, disk.size),
                Glyph::Disk,
            ));
            keys.push(disk.dev());
        }
        (rows, keys)
    };

    vec![
        Question::choose(
            "Which disk?",
            "Enter picks how much swap · only removable disks with nothing mounted are listed",
            rows,
            keys,
        ),
        Question::choose(
            "How much swap?",
            "{0} · more swap is slower than RAM",
            Vec::new(),
            Vec::new(),
        ),
        Question::ask(
            "Use this card for swap?",
            "{0} · every file on it is lost",
            "{0} · {1} of swap, label lazyos_swap",
            "Erase it",
            "go",
        ),
    ]
}

/// `lazyos menu swap` (`rofi-swap`): the window, then the steps or the undo.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Auxiliary swap",
            format!("{said} (parted, util-linux)"),
            Urgency::Critical,
            Glyph::Disk,
        );
        return Ok(3);
    }

    // Both setup and undo rewrite /etc/fstab.
    let fstab_text = std::fs::read_to_string("/etc/fstab").map_err(|_| {
        crate::Failure::new(
            "/etc/fstab is missing or unreadable: nothing can be configured".to_string(),
            "the menu cannot run without /etc/fstab".to_string(),
        )
    })?;

    let listing = out(&["lsblk", "-dnro", "NAME,SIZE,MODEL,TRAN"]).unwrap_or_default();
    let facts = Facts::from_machine();
    let found = disk::candidates(&listing, &facts);

    let status = Status::from_machine();

    // Already set up: status and undo.
    if status.configured || status.active {
        let mut picker = Picker::new(
            "Auxiliary swap",
            vec![
                Question::choose(
                    "Auxiliary swap",
                    "Enter acts · Escape closes · the card is not touched",
                    vec![
                        row(
                            "Status",
                            status.detail(),
                            if status.active {
                                Glyph::Chip
                            } else {
                                Glyph::Warning
                            },
                        ),
                        row(
                            "Turn it off",
                            "swapoff, and no swap from that card at the next boot",
                            Glyph::Undo,
                        ),
                    ],
                    vec!["status".to_string(), "undo".to_string()],
                ),
                Question::ask(
                    "Turn off the auxiliary swap?",
                    "a card that is in use can take a while · Escape leaves it as it is",
                    "{0} · files on it are not erased",
                    "Turn it off",
                    "go",
                ),
            ],
        );

        let code = crate::run_screen(terminal, design, "Auxiliary swap", &mut picker)?;
        if code != 0 {
            return Ok(code);
        }

        let answers = picker.answers();
        if answers.get(1) != Some(&"go".to_string()) {
            return Ok(0);
        }

        match turn_off(&status, &fstab_text) {
            Ok(done) => {
                say("Auxiliary swap", done, Urgency::Normal, Glyph::Disk);
                Ok(0)
            }
            Err(failed) => {
                say(
                    "Auxiliary swap",
                    failed.said(),
                    Urgency::Critical,
                    Glyph::Disk,
                );
                Ok(3)
            }
        }
    } else {
        // Nothing set up yet: pick a disk.
        if found.is_empty() {
            say(
                "Auxiliary swap",
                disk::NOTHING,
                Urgency::Normal,
                Glyph::Disk,
            );
            return Ok(0);
        }

        let mut picker = Picker::new("Auxiliary swap", questions(&found));

        // We need to drive the first question (disk), then build the size question,
        // then drive the rest. Picker's queue is fixed at construction, so we handle
        // the first question here, then reconstruct for the remaining two.
        let code = crate::run_screen(terminal, design, "Auxiliary swap", &mut picker)?;
        if code != 0 {
            return Ok(code);
        }

        let answers = picker.answers();
        let Some(dev) = answers.first() else {
            return Ok(1);
        };
        let name = dev.trim_start_matches("/dev/");

        // The same last gate as Format a disk: this rewrites the whole disk with
        // parted, so a disk that `disk::is_system_disk` refuses is refused again
        // after the pick, whatever the list said.
        let now = Facts::from_machine();
        if disk::is_system_disk(name, &now) {
            say(
                "Auxiliary swap",
                format!(
                    "{dev} is built into this machine or is the disk this system runs from: never repartitioned here"
                ),
                Urgency::Critical,
                Glyph::Disk,
            );
            return Ok(3);
        }

        // One label means one card: refuse a different disk that already carries it.
        let existing = out(&["blkid", "-L", "lazyos_swap"]).unwrap_or_default();
        let parent = out(&["lsblk", "-no", "PKNAME", dev]).unwrap_or_default();
        let parent = parent.trim();
        if label_taken(&existing, dev, parent, name) {
            say(
                "Auxiliary swap",
                format!(
                    "{existing} already carries the lazyos_swap label: turn that swap off first"
                ),
                Urgency::Critical,
                Glyph::Disk,
            );
            return Ok(3);
        }

        // Get the disk size in MiB for the size options.
        let bytes = out(&["lsblk", "-bdno", "SIZE", dev]).unwrap_or_default();
        let total_mib: u64 = bytes.trim().parse().unwrap_or(0);

        let size_opts = sizes(total_mib);
        let size_rows: Vec<lazyos_ui::Row> = size_opts
            .iter()
            .map(|(_key, label)| row(label.clone(), String::new(), Glyph::Disk))
            .collect();
        let size_keys: Vec<String> = size_opts.iter().map(|(k, _)| k.clone()).collect();

        // Second question: the size.
        let mut picker = Picker::new(
            "Auxiliary swap",
            vec![
                Question::choose(
                    "How much swap?",
                    "{0} · more swap is slower than RAM",
                    size_rows,
                    size_keys,
                ),
                Question::ask(
                    "Use this card for swap?",
                    "{0} · every file on it is lost",
                    "{0} · {1} of swap, label lazyos_swap",
                    "Erase it",
                    "go",
                ),
            ],
        );

        // Pre-fill the first answer (the device) so the second question's placeholders work.
        picker.set_answers(vec![dev.clone()]);

        let code = crate::run_screen(terminal, design, "Auxiliary swap", &mut picker)?;
        if code != 0 {
            return Ok(code);
        }

        let answers = picker.answers();
        if answers.get(2) != Some(&"go".to_string()) {
            return Ok(0);
        }

        let Some(size_choice) = answers.get(1) else {
            return Ok(1);
        };
        let end = extent(size_choice, total_mib);
        let alloc = match size_choice.as_str() {
            "100%" => format!(
                "{} · one swap partition, nothing beside it",
                out(&["lsblk", "-dno", "SIZE", dev]).unwrap_or_default()
            ),
            "2GiB" => "2 GiB".to_string(),
            "4GiB" => "4 GiB".to_string(),
            "8GiB" => "8 GiB".to_string(),
            _ => size_choice.clone(),
        };

        match apply(dev, &end, &alloc, &fstab_text) {
            Ok(done) => {
                say("Auxiliary swap", done, Urgency::Normal, Glyph::Disk);
                Ok(0)
            }
            Err(failed) => {
                say(
                    "Auxiliary swap",
                    failed.said(),
                    Urgency::Critical,
                    Glyph::Disk,
                );
                Ok(3)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reads_the_machine_and_formats_a_line() {
        let status = Status {
            part: Some("/dev/sda1".to_string()),
            size: Some("8G".to_string()),
            priority: Some(10),
            configured: true,
            active: true,
        };
        let line = status.line();
        assert!(line.contains("/dev/sda1"));
        assert!(line.contains("8G"));
        assert!(line.contains("pri=10"));
        assert!(line.contains("active"));
        assert!(line.contains("configured"));
    }

    #[test]
    fn status_without_partition_is_still_configured() {
        let status = Status {
            part: None,
            size: None,
            priority: None,
            configured: true,
            active: false,
        };
        assert!(status.detail().contains("not inserted"));
        assert!(status.line().contains("configured"));
    }

    #[test]
    fn the_sizes_are_the_whole_card_and_2_4_8_gib_with_margin() {
        let opts = sizes(30 * 1024); // 30 GiB
        assert_eq!(opts[0].0, "100%");
        assert_eq!(opts[1].0, "2GiB");
        assert_eq!(opts[2].0, "4GiB");
        assert_eq!(opts[3].0, "8GiB");

        let small = sizes(1024); // 1 GiB - nothing fits
        assert_eq!(small.len(), 1);
        assert_eq!(small[0].0, "100%");

        let medium = sizes(3 * 1024); // 3 GiB - only 2 GiB fits
        assert_eq!(medium.len(), 2);
        assert_eq!(medium[1].0, "2GiB");
    }

    #[test]
    fn the_extent_is_the_parted_end_for_the_chosen_size() {
        assert_eq!(extent("100%", 30 * 1024), "100%");
        assert_eq!(extent("2GiB", 30 * 1024), "2049MiB");
        assert_eq!(extent("4GiB", 30 * 1024), "4097MiB");
        assert_eq!(extent("8GiB", 30 * 1024), "8193MiB");
    }

    #[test]
    fn label_taken_refuses_a_different_disk_with_the_label() {
        assert!(label_taken("/dev/sdb1", "/dev/sda", "sda", "sdb"));
        assert!(!label_taken("/dev/sda1", "/dev/sda", "sda", "sda"));
        assert!(!label_taken("", "/dev/sda", "sda", "sdb"));
    }

    #[test]
    fn joined_merges_newlines_into_spaces_for_the_case_pattern() {
        let listing = "zram0\n/dev/sda1\n";
        let joined = joined(listing);
        assert_eq!(joined, " zram0 /dev/sda1 ");
        assert!(active_in(&joined, "/dev/sda1"));
        assert!(active_in(&joined, "zram0"));
    }

    #[test]
    fn active_in_finds_the_partition_in_the_joined_list() {
        let joined = " zram0 /dev/sda1 /dev/mmcblk1p1 ";
        assert!(active_in(joined, "/dev/sda1"));
        assert!(active_in(joined, "/dev/mmcblk1p1"));
        assert!(!active_in(joined, "/dev/sdb1"));
    }

    #[test]
    fn priority_of_reads_the_next_token_after_the_name() {
        let joined = "zram0 100 /dev/sda1 10 /dev/mmcblk1p1 10";
        assert_eq!(priority_of(joined, "/dev/sda1"), Some(10));
        assert_eq!(priority_of(joined, "/dev/mmcblk1p1"), Some(10));
        assert_eq!(priority_of(joined, "/dev/sdb1"), None);
    }

    #[test]
    fn the_partition_steps_are_argv_arrays_and_root() {
        let steps = partition_steps("/dev/sda", "100%");
        assert_eq!(steps.len(), 2);
        assert_eq!(
            steps[0].argv,
            ["parted", "-s", "/dev/sda", "mktable", "gpt"]
        );
        assert_eq!(
            steps[1].argv,
            [
                "parted",
                "-s",
                "/dev/sda",
                "mkpart",
                "primary",
                "linux-swap",
                "1MiB",
                "100%"
            ]
        );
        assert!(steps.iter().all(|s| s.root));
    }

    #[test]
    fn the_swap_area_step_uses_mkswap_with_f_and_the_label() {
        let step = swap_area_step("/dev/sda1");
        assert_eq!(
            step.argv,
            ["mkswap", "-f", "-L", "lazyos_swap", "/dev/sda1"]
        );
        assert!(step.root);
    }

    #[test]
    fn the_activation_step_uses_swapon_with_priority_10() {
        let step = activation_step("/dev/sda1");
        assert_eq!(step.argv, ["swapon", "-p", "10", "/dev/sda1"]);
        assert!(step.root);
    }

    #[test]
    fn the_reload_step_is_systemctl_daemon_reload() {
        let step = reload_step();
        assert_eq!(step.argv, ["systemctl", "daemon-reload"]);
        assert!(step.root);
    }

    #[test]
    fn a_wait_for_a_node_that_is_not_there_gives_up() {
        let start = Instant::now();
        assert!(!wait_for_in(
            "/dev/lazyos-not-a-device",
            Duration::from_millis(150)
        ));
        assert!(start.elapsed() >= Duration::from_millis(100));
    }

    #[test]
    fn a_node_that_exists_is_not_waited_for() {
        assert!(wait_for_in("/dev/null", Duration::from_millis(0)));
    }
}
