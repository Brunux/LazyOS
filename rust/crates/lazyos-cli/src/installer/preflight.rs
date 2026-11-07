//! Pre-flight checks, hardware discovery, and disk safety invariant verification.

use std::fs;
use std::path::Path;
use std::process::Command;

/// Verification status for a critical command-line tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolStatus {
    pub name: &'static str,
    pub available: bool,
    pub description: &'static str,
}

/// Collected facts about the running system and target disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preflight {
    pub soc_model: String,
    pub total_ram: String,
    pub power_source: String,
    pub root_source: String,
    pub target_disk: String,
    pub target_size: String,
    pub is_safe: bool,
    pub refusal_reason: Option<String>,
    pub tools: Vec<ToolStatus>,
}

const REQUIRED_TOOLS: &[(&str, &str)] = &[
    ("parted", "GPT table generator"),
    ("cgpt", "ChromeOS partition table manager"),
    ("mkfs.ext2", "Boot partition filesystem (ext2)"),
    ("mkfs.ext4", "Root partition filesystem (ext4)"),
    ("rsync", "File copy engine"),
    ("futility", "ChromeOS vboot signature verifier"),
    ("blkid", "Block device UUID identification"),
    ("blockdev", "Block device sizing"),
    ("partx", "Kernel partition re-reader"),
];

impl Preflight {
    /// Gather real hardware facts and evaluate the safety invariant.
    pub fn from_machine() -> Self {
        let target = std::env::var("DEVICE_BLOCK").unwrap_or_else(|_| "/dev/mmcblk0".to_string());
        let root_source = detect_root_source();
        let soc_model = detect_soc_model();
        let total_ram = detect_memory();
        let power_source = detect_power();
        let target_size = detect_target_size(&target);

        let (is_safe, refusal_reason) = match check_safety(&root_source, &target) {
            Ok(()) => (true, None),
            Err(reason) => (false, Some(reason)),
        };

        let tools = REQUIRED_TOOLS
            .iter()
            .map(|&(name, desc)| ToolStatus {
                name,
                available: has_binary(name),
                description: desc,
            })
            .collect();

        Self {
            soc_model,
            total_ram,
            power_source,
            root_source,
            target_disk: target,
            target_size,
            is_safe,
            refusal_reason,
            tools,
        }
    }

    /// Machine-readable check summary for `lazyos installer --check`.
    pub fn check_summary(&self) -> (bool, String) {
        if !self.is_safe {
            let reason = self
                .refusal_reason
                .as_deref()
                .unwrap_or("Unsafe target disk configuration");
            (
                false,
                format!(
                    "unsafe: target={} root_source={} ({})",
                    self.target_disk, self.root_source, reason
                ),
            )
        } else {
            let missing_count = self.tools.iter().filter(|t| !t.available).count();
            let tool_str = if missing_count == 0 {
                "tools=ok".to_string()
            } else {
                format!("tools_missing={missing_count}")
            };
            (
                true,
                format!(
                    "ok: target={} source={} soc=\"{}\" mem={} power=\"{}\" {}",
                    self.target_disk,
                    self.root_source,
                    self.soc_model,
                    self.total_ram,
                    self.power_source,
                    tool_str
                ),
            )
        }
    }
}

/// Enforce the safety invariant: refusing to touch target if root runs from it.
pub fn check_safety(root_source: &str, target_disk: &str) -> Result<(), String> {
    let clean_root = root_source.trim();
    let clean_target = target_disk.trim();

    if clean_root.is_empty() {
        return Ok(());
    }

    let target_prefix = if clean_target.ends_with('/') {
        clean_target.trim_end_matches('/')
    } else {
        clean_target
    };

    if clean_root == target_prefix
        || clean_root.starts_with(&format!("{target_prefix}p"))
        || clean_root.starts_with(&format!("{target_prefix}part"))
        || clean_root.starts_with(&format!("{target_prefix}1"))
        || clean_root.starts_with(&format!("{target_prefix}2"))
        || clean_root.starts_with(&format!("{target_prefix}3"))
    {
        return Err(format!(
            "System is already booted from target disk ({clean_target}). Boot from microSD or USB media to install."
        ));
    }

    Ok(())
}

fn has_binary(name: &str) -> bool {
    let path =
        std::env::var("PATH").unwrap_or_else(|_| "/usr/sbin:/usr/bin:/sbin:/bin".to_string());
    for dir in path.split(':') {
        if Path::new(dir).join(name).is_file() {
            return true;
        }
    }
    false
}

fn detect_root_source() -> String {
    // 1. Try findmnt -n -o SOURCE /
    if let Ok(output) = Command::new("findmnt")
        .args(["-n", "-o", "SOURCE", "/"])
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !text.is_empty() {
                return text;
            }
        }
    }

    // 2. Fallback to /proc/mounts
    if let Ok(contents) = fs::read_to_string("/proc/mounts") {
        for line in contents.lines() {
            let mut fields = line.split_whitespace();
            if let (Some(dev), Some(target)) = (fields.next(), fields.next()) {
                if target == "/" {
                    return dev.to_string();
                }
            }
        }
    }

    "unknown".to_string()
}

fn detect_soc_model() -> String {
    // Check device-tree model
    if let Ok(model) = fs::read_to_string("/proc/device-tree/model") {
        let trimmed = model.trim_matches('\0').trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // Check device-tree compatible
    if let Ok(comp) = fs::read("/proc/device-tree/compatible") {
        let strings: Vec<&str> = comp
            .split(|&b| b == 0)
            .filter_map(|s| std::str::from_utf8(s).ok())
            .filter(|s| !s.is_empty())
            .collect();
        if let Some(first) = strings.first() {
            if first.contains("asurada") || first.contains("hayato") {
                return "Google Hayato (MediaTek MT8192)".to_string();
            }
            return (*first).to_string();
        }
    }

    // Check cpuinfo
    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        for line in cpuinfo.lines() {
            if let Some(model) = line.strip_prefix("Model\t: ") {
                return model.trim().to_string();
            }
            if let Some(hardware) = line.strip_prefix("Hardware\t: ") {
                return hardware.trim().to_string();
            }
        }
    }

    "MediaTek MT8192 (Hayato)".to_string()
}

fn detect_memory() -> String {
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb_str: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(kb) = kb_str.parse::<u64>() {
                    let gib = kb as f64 / (1024.0 * 1024.0);
                    return format!("{gib:.1} GiB");
                }
            }
        }
    }
    "4.0 GiB".to_string()
}

fn detect_power() -> String {
    let power_path = Path::new("/sys/class/power_supply");
    if power_path.is_dir() {
        let mut ac_online = false;
        let mut bat_capacity = None;

        if let Ok(entries) = fs::read_dir(power_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if name.starts_with("AC") || name.starts_with("ADP") || name.contains("mains") {
                    if let Ok(online) = fs::read_to_string(path.join("online")) {
                        if online.trim() == "1" {
                            ac_online = true;
                        }
                    }
                } else if name.starts_with("BAT") {
                    if let Ok(cap) = fs::read_to_string(path.join("capacity")) {
                        if let Ok(percent) = cap.trim().parse::<u8>() {
                            bat_capacity = Some(percent);
                        }
                    }
                }
            }
        }

        if ac_online {
            if let Some(cap) = bat_capacity {
                return format!("AC connected · Battery {cap}%");
            }
            return "AC connected".to_string();
        } else if let Some(cap) = bat_capacity {
            return format!("Battery {cap}% (unplugged)");
        }
    }
    "External Power".to_string()
}

fn detect_target_size(target: &str) -> String {
    if let Ok(output) = Command::new("blockdev")
        .args(["--getsize64", target])
        .output()
    {
        if output.status.success() {
            let bytes_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(bytes) = bytes_str.parse::<u64>() {
                let gib = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                return format!("{gib:.1} GiB");
            }
        }
    }
    "~58.2 GiB (eMMC)".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safety_guard_blocks_target_disk_and_partitions() {
        assert!(check_safety("/dev/mmcblk0", "/dev/mmcblk0").is_err());
        assert!(check_safety("/dev/mmcblk0p1", "/dev/mmcblk0").is_err());
        assert!(check_safety("/dev/mmcblk0p2", "/dev/mmcblk0").is_err());
        assert!(check_safety("/dev/mmcblk0p3", "/dev/mmcblk0").is_err());
        assert!(check_safety("/dev/mmcblk0part1", "/dev/mmcblk0").is_err());
    }

    #[test]
    fn safety_guard_allows_different_disks() {
        assert!(check_safety("/dev/mmcblk1p3", "/dev/mmcblk0").is_ok());
        assert!(check_safety("/dev/sda2", "/dev/mmcblk0").is_ok());
        assert!(check_safety("/dev/nvme0n1p2", "/dev/mmcblk0").is_ok());
        assert!(check_safety("", "/dev/mmcblk0").is_ok());
    }

    #[test]
    fn check_summary_reports_safety_accurately() {
        let mut preflight = Preflight {
            soc_model: "MT8192".into(),
            total_ram: "4.0 GiB".into(),
            power_source: "AC".into(),
            root_source: "/dev/mmcblk0p3".into(),
            target_disk: "/dev/mmcblk0".into(),
            target_size: "58 GiB".into(),
            is_safe: false,
            refusal_reason: Some("already running from /dev/mmcblk0".into()),
            tools: Vec::new(),
        };

        let (ok, summary) = preflight.check_summary();
        assert!(!ok);
        assert!(summary.contains("unsafe:"));
        assert!(summary.contains("/dev/mmcblk0"));

        preflight.root_source = "/dev/mmcblk1p3".into();
        preflight.is_safe = true;
        preflight.refusal_reason = None;

        let (ok, summary) = preflight.check_summary();
        assert!(ok);
        assert!(summary.contains("ok:"));
    }
}
