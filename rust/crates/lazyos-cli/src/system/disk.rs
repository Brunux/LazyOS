//! Which disks a menu may write, and which it may never.
//!
//! This is the port of `ui_disk_system`/`ui_disks` from
//! `~/.config/bin/lib-rofi.sh`, and the two functions exist because the shell
//! got this wrong on hardware: the first version offered the eMMC's hardware
//! boot partitions and the SPI flash, and a later one offered the microSD that
//! had installed the eMMC. So the rules are stated here once, over the text the
//! system tools print, and every one of them has a test with a fixture from
//! this machine:
//!
//! - **A fixed disk is not a candidate.** `/sys/block/<disk>/removable` is `0`
//!   for *both* MMC devices on this Chromebook - the eMMC and the microSD - so
//!   the device-level `/sys/block/<disk>/device/removable` (`fixed` vs
//!   `removable`) and `TRAN=usb` decide what the first one cannot. A USB
//!   enclosure may report `0` for both, which is why the transport counts.
//! - **The disk this system runs from is not a candidate.** Root, `/boot`, the
//!   kernel partition named by `kern_guid=` and every active swap resolve to
//!   their disk with `lsblk -s`. The kernel-partition rule alone reopens the
//!   hole the moment a table is wiped or `/boot` is unmounted.
//! - **A disk with anything mounted is not a candidate.** A busy disk fails
//!   halfway anyway, and an active swap partition shows as mounted here.
//! - **A disk carrying the ChromeOS kernel partition is not a candidate.** It
//!   is a disk this firmware can load a kernel from - p1 of the layout in
//!   `scripts/lib-cgpt.sh` - which is how the card that installed the eMMC is
//!   refused while every other test says it is free.
//! - **Pseudo devices are not disks.** `zram`, `loop`, `ram`, `sr`, `mtd`
//!   (the SPI flash this port must never write), the eMMC's 4 MB hardware boot
//!   partitions (`mmcblk*boot*`, `mmcblk*rpmb`), `dm-*`, `md*`, `nbd*`, `rbd*`.

use std::collections::BTreeMap;

use super::pick::row;
use lazyos_ui::Glyph;

/// The ChromeOS kernel partition type: a disk carrying one is a disk this
/// firmware can load a kernel from, and never one a menu erases.
pub const KERNEL_TYPE: &str = "fe3a2a5d-4f32-41a7-b725-accc3285a309";

/// The sentence the empty list shows. It names the fourth reason because that
/// is the one a person actually hits: the card that installed the eMMC is
/// removable, unmounted and free on every other count, and only its kernel
/// partition says it is bootable.
pub const NOTHING: &str = "Nothing to offer: every disk here is mounted, built into the machine, \
     carries a bootable kernel partition (the card that installed this system), or is the one this \
     system runs from";

/// One row of `lsblk -dnro NAME,SIZE,MODEL,TRAN`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disk {
    pub name: String,
    pub size: String,
    pub model: String,
    pub tran: Option<String>,
}

/// The transports `lsblk` prints. The last column is only read as one when it
/// is a transport, because an empty TRAN (`zram`, a card reader) collapses
/// under whitespace and the column before it is the model.
const TRANSPORTS: [&str; 13] = [
    "usb", "mmc", "sata", "ata", "nvme", "scsi", "virtio", "sas", "fc", "pci", "ieee1394", "spi",
    "sdio",
];

impl Disk {
    pub fn dev(&self) -> String {
        format!("/dev/{}", self.name)
    }

    /// `/dev/sda · USB DISK 3.0 · 29.5G · usb`, the line every confirm uses.
    pub fn describe(&self) -> String {
        let mut parts = vec![self.dev(), self.model.clone(), self.size.clone()];
        if let Some(tran) = &self.tran {
            parts.push(tran.clone());
        }
        parts.join(" · ")
    }
}

/// `lsblk -r` escapes a space in a model as `\x20`, and a row shows a model,
/// not an escape sequence.
fn unescape(field: &str) -> String {
    field.replace("\\x20", " ")
}

/// The disks `lsblk -dnro NAME,SIZE,MODEL,TRAN` listed.
pub fn disks(listing: &str) -> Vec<Disk> {
    let mut out = Vec::new();
    for line in listing.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else { continue };
        if name.is_empty() {
            continue;
        }
        let rest: Vec<&str> = fields.collect();
        // NAME, SIZE, then a MODEL that may be several words, then TRAN.
        let tran = rest
            .last()
            .filter(|last| TRANSPORTS.contains(last))
            .map(|last| unescape(last));
        let body = &rest[..rest.len() - usize::from(tran.is_some())];
        let size = body.first().map(|v| unescape(v)).unwrap_or_default();
        let model = body
            .get(1..)
            .map(|m| unescape(&m.join(" ")))
            .unwrap_or_default();
        out.push(Disk {
            name: name.to_string(),
            size,
            model: if model.is_empty() {
                "unknown model".to_string()
            } else {
                model
            },
            tran,
        });
    }
    out
}

/// Why a disk is not offered. The order is the shell's: what the machine *is*
/// first, then what is on it, then what it carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Built into the machine, or the disk this system runs from.
    System,
    /// Something on it (or a partition of it) is mounted or active as swap.
    Mounted,
    /// It carries a ChromeOS kernel partition.
    Kernel,
}

impl Refusal {
    pub fn because(self) -> &'static str {
        match self {
            Refusal::System => "built into this machine or the disk this system runs from",
            Refusal::Mounted => "something on it is mounted",
            Refusal::Kernel => "it carries a bootable kernel partition",
        }
    }
}

/// What this machine says about its disks, with every system tool's answer
/// already read once. Built literally in the tests, from the machine in
/// [`Facts::from_machine`].
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// `/sys/block/<disk>/removable` (the disk-level flag, `0` for both MMCs).
    pub removable: BTreeMap<String, String>,
    /// `/sys/block/<disk>/device/removable` (`fixed` or `removable`).
    pub device_removable: BTreeMap<String, String>,
    /// `lsblk -dnro TRAN`.
    pub tran: BTreeMap<String, String>,
    /// Every source the system runs from: `/`, `/boot`, the kernel partition
    /// named by `kern_guid=`, and each active swap.
    pub sources: Vec<String>,
    /// `lsblk -snro NAME <source>` resolved to the disk, one entry per source.
    pub parents: Vec<(String, String)>,
    /// `lsblk -nro MOUNTPOINT /dev/<disk>`: one line per mounted partition.
    pub mounts: BTreeMap<String, Vec<String>>,
    /// `lsblk -nro PARTTYPE /dev/<disk>`.
    pub parttypes: BTreeMap<String, Vec<String>>,
}

impl Facts {
    /// A disk a person can pull out: the transport says USB, or the
    /// device-level flag says so (the disk-level one is `0` for the microSD
    /// on this machine, which is why it is only the first of three).
    fn hot_pluggable(&self, name: &str) -> bool {
        let get = |map: &BTreeMap<String, String>| map.get(name).cloned().unwrap_or_default();
        get(&self.removable) == "1"
            || get(&self.device_removable) == "removable"
            || get(&self.tran) == "usb"
    }

    fn runs_from(&self, name: &str) -> bool {
        self.sources.iter().any(|source| {
            self.parents
                .iter()
                .any(|(from, disk)| from == source && disk == name)
        })
    }

    /// Read the machine. Every tool is optional: a missing `findfs` or
    /// `findmnt` leaves that source out, which can only make the list stricter
    /// in one direction and looser in the other - so the caller still confirms,
    /// and the last gate asks [`is_system_disk`] again after the pick.
    pub fn from_machine() -> Self {
        let mut facts = Facts::default();
        let listing = super::out(&["lsblk", "-dnro", "NAME,SIZE,MODEL,TRAN"]).unwrap_or_default();
        for disk in disks(&listing) {
            let name = &disk.name;
            if let Ok(value) = std::fs::read_to_string(format!("/sys/block/{name}/removable")) {
                facts
                    .removable
                    .insert(name.clone(), value.trim().to_string());
            }
            if let Ok(value) =
                std::fs::read_to_string(format!("/sys/block/{name}/device/removable"))
            {
                facts
                    .device_removable
                    .insert(name.clone(), value.trim().to_string());
            }
            if let Some(tran) = disk.tran.clone() {
                facts.tran.insert(name.clone(), tran);
            }
            let mounts = mountpoints(name);
            if !mounts.is_empty() {
                facts.mounts.insert(name.clone(), mounts);
            }
            let types = parttypes(name);
            if !types.is_empty() {
                facts.parttypes.insert(name.clone(), types);
            }
        }

        facts.sources = sources();
        for source in &facts.sources {
            // `lsblk -s` walks from a partition up to its disk, so a
            // partition, a swap partition and a whole-disk source all answer
            // with the disk's name. The list is short and the last line is the
            // disk.
            if let Some(chain) = super::out(&["lsblk", "-snro", "NAME", source])
                && let Some(disk) = chain.lines().last()
            {
                facts
                    .parents
                    .push((source.clone(), disk.trim().to_string()));
            }
        }
        facts
    }
}

fn mountpoints(name: &str) -> Vec<String> {
    super::out(&["lsblk", "-nro", "MOUNTPOINT", &format!("/dev/{name}")])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn parttypes(name: &str) -> Vec<String> {
    super::out(&["lsblk", "-nro", "PARTTYPE", &format!("/dev/{name}")])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Everything the running system came from, as block devices: root, `/boot`,
/// the kernel partition `kern_guid=` names, and every active swap.
fn sources() -> Vec<String> {
    let mut sources = Vec::new();
    for dir in ["/", "/boot"] {
        if let Some(source) = super::out(&["findmnt", "-no", "SOURCE", dir]) {
            // A btrfs subvolume reads `/dev/sda2[/@root]`, and only a block
            // device can be a disk.
            let device = source.split('[').next().unwrap_or("").trim().to_string();
            if std::path::Path::new(&device).exists() {
                sources.push(device);
            }
        }
    }
    if let Ok(cmdline) = std::fs::read_to_string("/proc/cmdline")
        && let Some(guid) = cmdline
            .split_whitespace()
            .find_map(|word| word.strip_prefix("kern_guid="))
    {
        for findfs in ["/sbin/findfs", "/usr/sbin/findfs"] {
            if let Some(device) = super::out(&[findfs, &format!("PARTUUID={guid}")]) {
                sources.push(device);
                break;
            }
        }
    }
    if let Ok(swaps) = std::fs::read_to_string("/proc/swaps") {
        for line in swaps.lines().skip(1) {
            if let Some(name) = line.split_whitespace().next() {
                sources.push(name.to_string());
            }
        }
    }
    sources
}

/// Whether `name` is a disk this system must never write: not hot-pluggable,
/// or carrying a source it runs from.
pub fn is_system_disk(name: &str, facts: &Facts) -> bool {
    !facts.hot_pluggable(name) || facts.runs_from(name)
}

/// The first reason `name` is not a candidate, in the order the shell asked.
pub fn refusal(name: &str, facts: &Facts) -> Option<Refusal> {
    if is_system_disk(name, facts) {
        return Some(Refusal::System);
    }
    if facts.mounts.get(name).is_some_and(|m| !m.is_empty()) {
        return Some(Refusal::Mounted);
    }
    if facts
        .parttypes
        .get(name)
        .is_some_and(|types| types.iter().any(|t| t == KERNEL_TYPE))
    {
        return Some(Refusal::Kernel);
    }
    None
}

/// One name lsblk lists that is not a disk a person writes to.
pub fn is_pseudo(name: &str) -> bool {
    const PREFIXES: [&str; 6] = ["zram", "loop", "ram", "sr", "dm-", "md"];
    const FOREIGN: [&str; 3] = ["nbd", "rbd", "mmcblk0rpmb"];
    name.starts_with("mtd")
        || PREFIXES.iter().any(|p| name.starts_with(p))
        || name.contains("boot") && name.starts_with("mmcblk")
        || FOREIGN.iter().any(|p| name.starts_with(p))
        || name.contains("rpmb")
}

/// The disks a menu may offer, in `lsblk`'s order.
pub fn candidates(listing: &str, facts: &Facts) -> Vec<Disk> {
    disks(listing)
        .into_iter()
        .filter(|disk| !is_pseudo(&disk.name))
        .filter(|disk| refusal(&disk.name, facts).is_none())
        .collect()
}

/// The first partition of a disk: `sda1` but `mmcblk1p1`, because the kernel
/// inserts a `p` when the device name already ends in a digit. A menu that
/// wrote `/dev/mmcblk11` would create a device node nobody asked for.
pub fn partition(dev: &str) -> String {
    if dev.ends_with(|c: char| c.is_ascii_digit()) {
        format!("{dev}p1")
    } else {
        format!("{dev}1")
    }
}

/// The rows of the disk question: the device, then what it is.
pub fn rows(found: &[Disk]) -> (Vec<lazyos_ui::Row>, Vec<String>) {
    (
        found
            .iter()
            .map(|d| row(d.dev(), format!("{} · {}", d.model, d.size), Glyph::Disk))
            .collect(),
        found.iter().map(Disk::dev).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(pairs: &[(&str, &str, &str)], sources: &[(&str, &str)]) -> Facts {
        let mut facts = Facts::default();
        for (name, disk_removable, tran) in pairs {
            facts.removable.insert(name.to_string(), "0".into());
            facts
                .device_removable
                .insert(name.to_string(), disk_removable.to_string());
            facts.tran.insert(name.to_string(), tran.to_string());
        }
        for (source, disk) in sources {
            facts.sources.push(source.to_string());
            facts.parents.push((source.to_string(), disk.to_string()));
        }
        facts
    }

    /// The card's own `lsblk -d`: the eMMC, the microSD and the SPI flash.
    const CARD: &str = "\
mmcblk0   58.2G  BJTD4R  mmc
mmcblk1   29.5G  SC64G   mmc
mmcblk0boot0  4M  mmc
mmcblk0boot1  4M  mmc
mtdblock0  8M  mtd
zram0     1.9G
";

    #[test]
    fn the_emmc_is_built_in() {
        let facts = facts(&[("mmcblk0", "fixed", "mmc")], &[]);
        assert_eq!(refusal("mmcblk0", &facts), Some(Refusal::System));
        assert!(is_system_disk("mmcblk0", &facts));
    }

    #[test]
    fn a_stick_the_system_runs_from_is_refused_even_though_it_is_removable() {
        let facts = facts(
            &[("sda", "removable", "usb")],
            &[("/dev/sda2", "sda"), ("/dev/sda1", "sda")],
        );
        assert_eq!(refusal("sda", &facts), Some(Refusal::System));
    }

    #[test]
    fn the_card_that_installed_the_emmc_is_refused_by_its_kernel_partition() {
        let mut facts = facts(&[("mmcblk1", "removable", "mmc")], &[]);
        facts.parttypes.insert(
            "mmcblk1".into(),
            vec![
                KERNEL_TYPE.to_string(),
                "0fc63daf-8483-4772-8e79-3d69d8477de4".into(),
            ],
        );
        assert_eq!(refusal("mmcblk1", &facts), Some(Refusal::Kernel));
    }

    #[test]
    fn a_mounted_stick_is_refused() {
        let mut facts = facts(&[("sda", "removable", "usb")], &[]);
        facts
            .mounts
            .insert("sda".into(), vec!["/run/media/me/LAZYOS".into()]);
        assert_eq!(refusal("sda", &facts), Some(Refusal::Mounted));
    }

    #[test]
    fn a_spare_stick_is_the_only_candidate_on_this_machine() {
        let facts = facts(
            &[("mmcblk0", "fixed", "mmc"), ("mmcblk1", "removable", "mmc")],
            &[],
        );
        let found = candidates(CARD, &facts);
        let names: Vec<&str> = found.iter().map(|d| d.name.as_str()).collect();
        // mmcblk1 has no kernel partition and is not mounted in this fixture,
        // and the hardware partitions, the SPI flash and zram are never rows.
        assert_eq!(names, vec!["mmcblk1"]);
        assert!(!found[0].model.is_empty());
    }

    #[test]
    fn the_pseudo_devices_are_named_and_not_guessed() {
        for name in [
            "zram0",
            "loop0",
            "ram0",
            "sr0",
            "mtdblock0",
            "mmcblk0boot0",
            "mmcblk0rpmb",
            "dm-0",
            "md0",
            "nbd0",
            "rbd0",
        ] {
            assert!(is_pseudo(name), "{name} should never be a row");
        }
        for name in ["mmcblk0", "mmcblk1", "sda", "nvme0n1"] {
            assert!(!is_pseudo(name), "{name} is a disk");
        }
    }

    #[test]
    fn a_model_with_spaces_reads_as_a_model() {
        let found = disks("sda  29.5G  USB\\x20DISK\\x203.0  usb");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].model, "USB DISK 3.0");
        assert_eq!(found[0].tran.as_deref(), Some("usb"));
        assert_eq!(found[0].describe(), "/dev/sda · USB DISK 3.0 · 29.5G · usb");
    }

    #[test]
    fn a_partition_of_a_disk_that_ends_in_a_digit_gets_the_p() {
        assert_eq!(partition("/dev/sda"), "/dev/sda1");
        assert_eq!(partition("/dev/mmcblk1"), "/dev/mmcblk1p1");
        assert_eq!(partition("/dev/nvme0n1"), "/dev/nvme0n1p1");
    }

    #[test]
    fn a_disk_with_no_transport_still_has_a_row() {
        let found = disks("zram0  1.9G");
        assert_eq!(found[0].name, "zram0");
        assert_eq!(found[0].model, "unknown model");
        assert_eq!(found[0].tran, None);
    }
}
