//! Bookmarks & Disks Jump (`rofi-places`, `lazyos menu places`).
//!
//! Lists user directories (`~/Downloads`, `~/Documents`, etc.), mounted
//! and mountable block storage devices with filesystem usage and mountpoints.
//! Also provides `--rows` headless output for witnesses.

use std::path::{Path, PathBuf};
use std::process::Command;

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{have, out, say};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceEntry {
    pub key: String,
    pub label: String,
    pub target: String,
    pub glyph: Glyph,
}

/// Parse findmnt output into mounted filesystem mountpoints.
pub fn parse_findmnt(raw: &str) -> Vec<(String, String)> {
    let mut mounts = Vec::new();
    for line in raw.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let target = parts[0];
        let fstype = parts[1];
        match fstype {
            "ext2" | "ext3" | "ext4" | "vfat" | "exfat" | "btrfs" | "xfs" | "f2fs" | "ntfs"
            | "ntfs3" => {}
            _ => continue,
        }
        if target == "/" || target.starts_with("/boot") {
            continue;
        }
        let base = Path::new(target)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| target.to_string());
        mounts.push((base, target.to_string()));
    }
    mounts
}

/// Parse lsblk output into unmounted partitions.
pub fn parse_lsblk(raw: &str) -> Vec<(String, String)> {
    let mut devices = Vec::new();
    for line in raw.lines() {
        // NAME TYPE FSTYPE LABEL MOUNTPOINT
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let name = parts[0];
        let ptype = parts[1];
        let fstype = parts[2];

        if ptype != "part" && ptype != "disk" {
            continue;
        }
        if fstype.is_empty() || fstype == "swap" || fstype == "crypto_LUKS" {
            continue;
        }
        // If there are 5 parts, parts[4] is mountpoint; if 4 parts and fstype is set,
        // it may or may not have a mountpoint. Check if line ends with a mountpoint or if parts len indicates mounted.
        // In raw lsblk: if MOUNTPOINT is non-empty, it's mounted.
        // We can inspect lines where mountpoint column is empty.
        let is_mounted = parts.len() >= 5 || (parts.len() == 4 && parts[3].starts_with('/'));
        if is_mounted {
            continue;
        }
        let label = if parts.len() >= 4 && !parts[3].starts_with('/') {
            parts[3].replace("\\x20", " ")
        } else {
            name.to_string()
        };
        devices.push((label, format!("/dev/{name}")));
    }
    devices
}

/// Build the full places list.
pub fn collect_places() -> Vec<PlaceEntry> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let mut list = Vec::new();

    // Standard user folders
    list.push(PlaceEntry {
        key: format!("open:{home}"),
        label: "Home".to_string(),
        target: "~".to_string(),
        glyph: Glyph::Home,
    });

    let dirs = [
        ("Documents", "Documents", Glyph::File),
        ("Downloads", "Downloads", Glyph::Folder),
        ("Pictures", "Pictures", Glyph::Image),
        ("Music", "Music", Glyph::Music),
        ("Videos", "Videos", Glyph::Video),
        ("Dev", "Dev", Glyph::Code),
    ];

    for (name, folder, glyph) in dirs {
        let p = Path::new(&home).join(folder);
        if p.is_dir() {
            list.push(PlaceEntry {
                key: format!("open:{}", p.display()),
                label: name.to_string(),
                target: format!("~/{folder}"),
                glyph,
            });
        }
    }

    if Path::new("/etc/sway").is_dir() {
        list.push(PlaceEntry {
            key: "open:/etc/sway".to_string(),
            label: "Sway configuration".to_string(),
            target: "/etc/sway".to_string(),
            glyph: Glyph::Gear,
        });
    }

    list.push(PlaceEntry {
        key: "open:/".to_string(),
        label: "Root filesystem".to_string(),
        target: "/".to_string(),
        glyph: Glyph::Globe,
    });

    // Mounted removable devices
    let findmnt_raw = out(&["findmnt", "-rn", "-o", "TARGET,FSTYPE"]).unwrap_or_default();
    for (label, target) in parse_findmnt(&findmnt_raw) {
        list.push(PlaceEntry {
            key: format!("open:{target}"),
            label,
            target,
            glyph: Glyph::Disk,
        });
    }

    // Unmounted devices
    if have("udisksctl") {
        let lsblk_raw =
            out(&["lsblk", "-rno", "NAME,TYPE,FSTYPE,LABEL,MOUNTPOINT"]).unwrap_or_default();
        for (label, dev) in parse_lsblk(&lsblk_raw) {
            list.push(PlaceEntry {
                key: format!("mount:{dev}"),
                label: format!("Mount {label}"),
                target: dev,
                glyph: Glyph::Disk,
            });
        }
    }

    // Shell & Disk actions
    list.push(PlaceEntry {
        key: format!("term:{home}"),
        label: "Open a terminal".to_string(),
        target: "~".to_string(),
        glyph: Glyph::Terminal,
    });

    list.push(PlaceEntry {
        key: "action:swap".to_string(),
        label: "Auxiliary swap".to_string(),
        target: "a card as more memory, on every boot".to_string(),
        glyph: Glyph::Chip,
    });

    list.push(PlaceEntry {
        key: "action:format".to_string(),
        label: "Format a disk".to_string(),
        target: "a stick or a card, wiped clean".to_string(),
        glyph: Glyph::Warning,
    });

    list
}

/// Headless `--rows` output for witness check.
pub fn rows() -> Vec<(String, String, String)> {
    collect_places()
        .into_iter()
        .map(|p| (p.key, p.label, p.target))
        .collect()
}

/// Interactive `lazyos menu places`.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    let places = collect_places();
    let mut rows = Vec::new();
    let mut keys = Vec::new();

    for p in &places {
        rows.push(row(&p.label, &p.target, p.glyph));
        keys.push(p.key.clone());
    }

    let mut picker = Picker::new(
        "Places",
        vec![Question::choose(
            "Places",
            "Enter opens · Escape closes",
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "Places", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    let Some(key) = answers.first() else {
        return Ok(1);
    };

    match key.as_str() {
        "action:swap" => {
            crate::system::spawn_detached("exec lazyos menu swap");
            Ok(0)
        }
        "action:format" => {
            crate::system::spawn_detached("exec lazyos menu format");
            Ok(0)
        }
        k if k.starts_with("term:") => {
            let path = &k[5..];
            let escaped = path.replace('\'', "'\\''");
            crate::system::spawn_detached(&format!("exec foot --working-directory '{escaped}'"));
            Ok(0)
        }
        k if k.starts_with("open:") => {
            let path = &k[5..];
            let escaped = path.replace('\'', "'\\''");
            crate::system::spawn_detached(&format!("exec lazyos menu files '{escaped}'"));
            Ok(0)
        }
        k if k.starts_with("mount:") => {
            let dev = &k[6..];
            let out = Command::new("udisksctl")
                .args(["mount", "-b", dev])
                .output();
            match out {
                Ok(res) if res.status.success() => {
                    let text = String::from_utf8_lossy(&res.stdout);
                    // "Mounted /dev/sda1 at /run/media/user/LABEL."
                    if let Some(pos) = text.rfind(" at ") {
                        let mp = text[pos + 4..].trim().trim_end_matches('.');
                        let escaped = mp.replace('\'', "'\\''");
                        crate::system::spawn_detached(&format!(
                            "exec lazyos menu files '{escaped}'"
                        ));
                    } else {
                        say("Places", "Device mounted", Urgency::Normal, Glyph::Disk);
                    }
                    Ok(0)
                }
                Ok(res) => {
                    let err = String::from_utf8_lossy(&res.stderr);
                    say(
                        "Places",
                        format!("Could not mount {dev}: {}", err.trim()),
                        Urgency::Critical,
                        Glyph::Disk,
                    );
                    Ok(3)
                }
                Err(e) => {
                    say(
                        "Places",
                        format!("Could not mount {dev}: {e}"),
                        Urgency::Critical,
                        Glyph::Disk,
                    );
                    Ok(3)
                }
            }
        }
        _ => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_findmnt_filters_root_and_boot() {
        let raw = "\
/ ext4
/boot ext4
/run/media/brunux/STICK vfat
/home/user/other btrfs
";
        let res = parse_findmnt(raw);
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].0, "STICK");
        assert_eq!(res[0].1, "/run/media/brunux/STICK");
        assert_eq!(res[1].0, "other");
    }

    #[test]
    fn parse_lsblk_extracts_unmounted_partitions() {
        let raw = "\
sda disk
sda1 part vfat STICK_LABEL
mmcblk0p1 part ext4 ROOT /
";
        let res = parse_lsblk(raw);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0, "STICK_LABEL");
        assert_eq!(res[0].1, "/dev/sda1");
    }

    #[test]
    fn rows_includes_swap_and_format_actions() {
        let r = rows();
        assert!(
            r.iter()
                .any(|(k, l, _)| k == "action:swap" && l == "Auxiliary swap")
        );
        assert!(
            r.iter()
                .any(|(k, l, _)| k == "action:format" && l == "Format a disk")
        );
    }
}
