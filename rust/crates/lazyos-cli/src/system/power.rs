//! Whether this machine can hibernate, and why not when it cannot.
//!
//! The shell's `sway-hibernate` refuses unless three things hold, and each one
//! has a different fix, so each says its own message instead of a generic
//! failure. What it shipped and then had to fix is the reason this is a value
//! with tests:
//!
//! - **The size is `/proc/swaps` column 3 (`Size`), never column 4 (`Used`).**
//!   An idle swap reads `0` in `Used`, so a fresh 8 GB partition on the microSD
//!   was refused as "0 MiB" while `swapon --show` reported 8G.
//! - **A `resume=` that names nothing on this boot is a refusal of its own**:
//!   the image would be written and the next boot would come up cold, which
//!   reads as hibernation losing the session rather than as a spec that points
//!   at a card that is gone.
//! - **The image goes to the swap `resume=` names**, not the biggest one, when
//!   that swap is active: with two disk swaps the old rule silently measured
//!   the wrong one.
//!
//! Everything is read through [`Hibernate::read`] from text, so a fixture from
//! a real machine is a unit test and a changed `/proc/swaps` field is a failing
//! test rather than a wrong answer.

/// The hibernation image can need about two fifths of RAM (the kernel's own
/// `image_size` default), which is the floor the swap must reach.
const NEED_SHARE: u64 = 2;

/// `/sys/power/state` has to offer `disk`, or the kernel cannot hibernate at
/// all.
pub fn kernel_supports(state: &str) -> bool {
    state.split_whitespace().any(|word| word == "disk")
}

/// One row of `/proc/swaps`: device, and its `Size` in KiB (column 3).
fn swap_rows(swaps: &str) -> Vec<(String, u64)> {
    swaps
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let device = fields.next()?.to_string();
            let _kind = fields.next()?;
            let size = fields.next()?.parse::<u64>().ok()?;
            Some((device, size))
        })
        .collect()
}

/// `MemTotal` from `/proc/meminfo`, in KiB.
fn mem_total(meminfo: &str) -> u64 {
    meminfo
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemTotal:")
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|kb| kb.parse().ok())
        })
        .unwrap_or(0)
}

/// The `resume=` spec on the kernel command line, if there is one.
fn resume_spec(cmdline: &str) -> Option<String> {
    cmdline
        .split_whitespace()
        .find_map(|word| word.strip_prefix("resume=").map(str::to_string))
}

/// A spec `findfs` understands and cannot resolve is a resume that names
/// nothing; a bare `major:minor` is legal for the kernel and is not judged, the
/// same limit `sway-hibernate` states.
fn resolvable(spec: &str) -> bool {
    spec.is_empty()
        || spec.starts_with("LABEL=")
        || spec.starts_with("UUID=")
        || spec.starts_with("PARTUUID=")
        || spec.starts_with("PARTLABEL=")
        || spec.starts_with("/dev/")
}

/// The conditions, read once. Built literally in the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hibernate {
    /// `/sys/power/state` offers `disk`.
    pub kernel: bool,
    /// The disk swap the image goes to, `None` when there is none.
    pub swap_dev: Option<String>,
    /// That swap's `Size` in KiB.
    pub swap_kb: u64,
    /// Two fifths of `MemTotal`, in KiB.
    pub need_kb: u64,
    /// The `resume=` spec, `None` when the command line has none.
    pub resume: Option<String>,
    /// `resume=` names nothing this boot can resolve.
    pub resume_broken: bool,
}

impl Hibernate {
    /// Read the four files and ask `findfs` for the resume spec.
    pub fn read(
        state: &str,
        swaps: &str,
        meminfo: &str,
        cmdline: &str,
        findfs: impl Fn(&str) -> Option<String>,
    ) -> Self {
        let rows = swap_rows(swaps);
        // zram keeps pages compressed but pays 1:1 for the ones that do not
        // compress, so it can never hold the image: only a disk swap can.
        let biggest = rows
            .iter()
            .filter(|(device, _)| !device.contains("zram"))
            .max_by_key(|(_, size)| *size)
            .map(|(device, size)| (device.clone(), *size));
        let (mut swap_dev, mut swap_kb) = match biggest {
            Some((device, size)) => (Some(device), size),
            None => (None, 0),
        };

        let resume = resume_spec(cmdline);
        let mut resume_broken = false;
        if let Some(spec) = &resume {
            if spec.is_empty() {
                resume_broken = true;
            } else if let Some(device) = findfs(spec) {
                // The image goes where resume= points, when that swap is
                // active; otherwise the biggest one is still the best guess.
                if let Some((_, size)) = rows.iter().find(|(name, _)| *name == device) {
                    swap_dev = Some(device);
                    swap_kb = *size;
                }
            } else if resolvable(spec) {
                resume_broken = true;
            }
        }

        Self {
            kernel: kernel_supports(state),
            swap_dev,
            swap_kb,
            need_kb: mem_total(meminfo) * NEED_SHARE / 5,
            resume,
            resume_broken,
        }
    }

    /// Read this machine.
    pub fn from_machine() -> Self {
        let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        let state = read("/sys/power/state");
        let swaps = read("/proc/swaps");
        let meminfo = read("/proc/meminfo");
        let cmdline = read("/proc/cmdline");
        Self::read(&state, &swaps, &meminfo, &cmdline, |spec| {
            for tool in ["/usr/sbin/findfs", "/sbin/findfs"] {
                if let Some(device) = super::out(&[tool, spec]) {
                    return Some(device);
                }
            }
            None
        })
    }

    /// Whether every condition holds, i.e. whether the menu may offer it.
    pub fn can(&self) -> bool {
        self.kernel
            && self.swap_dev.is_some()
            && self.swap_kb >= self.need_kb
            && self.resume.is_some()
            && !self.resume_broken
    }

    /// The first reason it cannot, in `sway-hibernate`'s order and words: the
    /// fix differs per reason, so the answer cannot be generic.
    pub fn refusal(&self) -> Option<String> {
        if !self.kernel {
            return Some("This kernel cannot hibernate (no 'disk' in /sys/power/state).".into());
        }
        let Some(device) = &self.swap_dev else {
            return Some(
                "No swap on disk. Hibernation needs a swap partition (2 GB or more) \
                 labelled lazyos_swap, normally on the microSD."
                    .into(),
            );
        };
        if self.swap_kb < self.need_kb {
            return Some(format!(
                "The swap on {device} is {} MiB; the hibernation image can need about {} MiB.",
                self.swap_kb / 1024,
                self.need_kb / 1024
            ));
        }
        match self.resume.as_deref() {
            None => Some(
                "The kernel command line has no resume=, so a hibernated session could not be \
                 restored. Add it once with CONFIRM=yes RESUME=yes \
                 ~/Dev/LazyOS/scripts/install-kpart.sh (it rebuilds the kernel partition and keeps \
                 resume= across later kernel updates)."
                    .into(),
            ),
            Some("") => Some(
                "The kernel command line has an empty resume=, so a hibernated session could not \
                 be restored. Rebuild the kernel partition without it: CONFIRM=yes RESUME=no \
                 ~/Dev/LazyOS/scripts/install-kpart.sh."
                    .into(),
            ),
            Some(spec) if self.resume_broken => {
                let fix = match spec.strip_prefix("LABEL=") {
                    Some(label) => format!(
                        "Relabel that swap (sudo swaplabel -L {label} <device>), or rebuild"
                    ),
                    None => "Rebuild".to_string(),
                };
                Some(format!(
                    "The kernel command line says resume={spec}, but no device carries that name \
                     now, so the next boot could not find the hibernation image. {fix} the kernel \
                     partition without it: CONFIRM=yes RESUME=no \
                     ~/Dev/LazyOS/scripts/install-kpart.sh."
                ))
            }
            Some(_) => None,
        }
    }

    /// The `--status` lines, the single copy of the answer `check-system.sh`
    /// reads: the row the menu draws and the line a report compares cannot
    /// disagree.
    pub fn status(&self) -> Vec<String> {
        vec![
            format!("kernel={}", yes(self.kernel)),
            format!("swap={}", self.swap_dev.clone().unwrap_or_default()),
            format!("swap_kb={}", self.swap_kb),
            format!("need_kb={}", self.need_kb),
            format!("resume={}", yes(self.resume.is_some())),
            format!("resume_broken={}", yes(self.resume_broken)),
            format!("hibernate={}", yes(self.can())),
        ]
    }
}

fn yes(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE: &str = "freeze mem disk\n";
    const SWAPS: &str = "\
Filename                                Type            Size            Used            Priority
/dev/zram0                              partition       1948160         0               100
/dev/mmcblk1p4                          partition       8388604         0               10
";
    const MEMINFO: &str = "MemTotal:        1948000 kB\nMemFree:          900000 kB\n";

    fn machine(resume: Option<&str>, findfs: impl Fn(&str) -> Option<String>) -> Hibernate {
        let cmdline = match resume {
            Some(spec) => format!("quiet loglevel=2 resume={spec} root=UUID=1234"),
            None => "quiet loglevel=2 root=UUID=1234".to_string(),
        };
        Hibernate::read(STATE, SWAPS, MEMINFO, &cmdline, findfs)
    }

    #[test]
    fn a_machine_with_a_big_disk_swap_and_resume_can_hibernate() {
        let h = machine(Some("LABEL=lazyos_swap"), |spec| {
            assert_eq!(spec, "LABEL=lazyos_swap");
            Some("/dev/mmcblk1p4".into())
        });
        assert_eq!(h.need_kb, 1948000 * 2 / 5);
        assert_eq!(h.swap_dev.as_deref(), Some("/dev/mmcblk1p4"));
        assert!(h.can(), "{:?}", h.refusal());
        assert!(h.status().contains(&"hibernate=yes".to_string()));
    }

    #[test]
    fn zram_is_never_the_hibernation_swap() {
        let swaps = "\
Filename                                Type            Size            Used            Priority
/dev/zram0                              partition       1948160         0               100
";
        let h = Hibernate::read(STATE, swaps, MEMINFO, "resume=LABEL=x", |_| None);
        assert_eq!(h.swap_dev, None);
        assert!(h.refusal().unwrap().starts_with("No swap on disk"));
    }

    #[test]
    fn the_size_is_column_three_and_an_idle_swap_is_not_zero() {
        // `Used` is 0 here and a big swap is still big: the bug the port hit.
        let h = machine(Some("LABEL=lazyos_swap"), |_| Some("/dev/mmcblk1p4".into()));
        assert_eq!(h.swap_kb, 8388604);
        assert!(h.can());
    }

    #[test]
    fn a_kernel_without_disk_says_so_first() {
        let h = Hibernate::read("freeze mem\n", SWAPS, MEMINFO, "resume=LABEL=x", |_| {
            Some("/dev/mmcblk1p4".into())
        });
        assert!(!h.can());
        assert_eq!(
            h.refusal().unwrap(),
            "This kernel cannot hibernate (no 'disk' in /sys/power/state)."
        );
    }

    #[test]
    fn a_swap_that_is_too_small_names_both_numbers() {
        let swaps = "\
Filename                                Type            Size            Used            Priority
/dev/mmcblk1p4                          partition       512000          0               10
";
        let h = Hibernate::read(STATE, swaps, MEMINFO, "resume=LABEL=x", |_| {
            Some("/dev/mmcblk1p4".into())
        });
        assert_eq!(
            h.refusal().unwrap(),
            "The swap on /dev/mmcblk1p4 is 500 MiB; the hibernation image can need about 760 MiB."
        );
    }

    #[test]
    fn no_resume_has_the_install_kpart_instruction() {
        let h = machine(None, |_| None);
        let said = h.refusal().unwrap();
        assert!(said.contains("has no resume="), "{said}");
        assert!(said.contains("CONFIRM=yes RESUME=yes"), "{said}");
    }

    #[test]
    fn an_empty_resume_is_its_own_message() {
        let h = machine(Some(""), |_| None);
        assert!(h.resume_broken);
        let said = h.refusal().unwrap();
        assert!(said.contains("has an empty resume="), "{said}");
        assert!(said.contains("RESUME=no"), "{said}");
    }

    #[test]
    fn a_resume_label_that_names_nothing_says_how_to_relabel() {
        let h = machine(Some("LABEL=lazyos_swap"), |_| None);
        assert!(h.resume_broken);
        let said = h.refusal().unwrap();
        assert!(
            said.contains("sudo swaplabel -L lazyos_swap <device>"),
            "{said}"
        );
    }

    #[test]
    fn a_resume_uuid_that_names_nothing_says_rebuild() {
        let h = machine(Some("UUID=dead-beef"), |_| None);
        let said = h.refusal().unwrap();
        assert!(said.contains("Rebuild the kernel partition"), "{said}");
        assert!(!said.contains("swaplabel"), "{said}");
    }

    #[test]
    fn a_resume_major_minor_is_not_judged() {
        // The kernel accepts `resume=8:1`; findfs does not, and that is not a
        // broken spec - the same limit sway-hibernate states.
        let h = machine(Some("8:1"), |_| None);
        assert!(!h.resume_broken);
        assert!(h.can(), "{:?}", h.refusal());
    }

    #[test]
    fn the_image_goes_to_the_swap_resume_names_not_the_biggest() {
        let swaps = "\
Filename                                Type            Size            Used            Priority
/dev/sda2                               partition       4194304         0               10
/dev/sdb2                               partition       8388608         0               10
";
        let h = Hibernate::read(STATE, swaps, MEMINFO, "resume=/dev/sda2", |_| {
            Some("/dev/sda2".into())
        });
        assert_eq!(h.swap_dev.as_deref(), Some("/dev/sda2"));
        assert_eq!(h.swap_kb, 4194304);
    }

    #[test]
    fn a_resume_device_that_is_not_swapped_falls_back_to_the_biggest() {
        let h = machine(Some("/dev/sdb2"), |_| Some("/dev/sdb2".into()));
        assert!(!h.resume_broken);
        assert_eq!(h.swap_dev.as_deref(), Some("/dev/mmcblk1p4"));
    }

    #[test]
    fn status_prints_the_shells_lines_in_its_order() {
        let h = machine(Some("LABEL=lazyos_swap"), |_| Some("/dev/mmcblk1p4".into()));
        assert_eq!(
            h.status(),
            vec![
                "kernel=yes".to_string(),
                "swap=/dev/mmcblk1p4".to_string(),
                "swap_kb=8388604".to_string(),
                "need_kb=779200".to_string(),
                "resume=yes".to_string(),
                "resume_broken=no".to_string(),
                "hibernate=yes".to_string(),
            ]
        );
    }
}
