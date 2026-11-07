//! Disk Cleaner (`rofi-clean`, `lazyos menu clean`).
//!
//! Scans reclaimable caches: apt package cache, systemd journal logs,
//! thumbnail cache, and user trash. Provides `--dry-run` output for
//! automation and witness checks.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{out, say, sudo};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reclaimable {
    pub trash_bytes: u64,
    pub thumb_bytes: u64,
    pub apt_bytes: u64,
    pub journal_bytes: u64,
    pub cache_bytes: u64,
    pub free_str: String,
    pub total_str: String,
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "0 MB".to_string()
    } else if bytes < 1024 * 1024 {
        format!("{} KB", bytes / 1024)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.0} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Recursively sum size of files in a directory.
pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(ft) = entry.file_type() {
                if ft.is_dir() {
                    total += dir_size(&entry.path());
                } else if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            }
        }
    }
    total
}

/// Parse journalctl --disk-usage output.
pub fn parse_journal_usage(raw: &str) -> u64 {
    // "Archived and active journals take up 32.0M in the file system."
    let parts: Vec<&str> = raw.split_whitespace().collect();
    for (i, &word) in parts.iter().enumerate() {
        if word == "up" && i + 1 < parts.len() {
            let val_str = parts[i + 1];
            if let Some(ch) = val_str.chars().last() {
                let num_str = &val_str[..val_str.len() - 1];
                if let Ok(num) = num_str.parse::<f64>() {
                    let mult = match ch {
                        'K' | 'k' => 1024.0,
                        'M' | 'm' => 1024.0 * 1024.0,
                        'G' | 'g' => 1024.0 * 1024.0 * 1024.0,
                        _ => 1.0,
                    };
                    return (num * mult) as u64;
                }
            }
        }
    }
    0
}

/// Measure all reclaimable sizes safely.
pub fn calculate_reclaimable() -> Reclaimable {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let trash_path = Path::new(&home).join(".local/share/Trash");
    let thumb_path = Path::new(&home).join(".cache/thumbnails");
    let apt_path = Path::new("/var/cache/apt/archives");
    let cache_path = Path::new(&home).join(".cache");

    let trash_bytes = dir_size(&trash_path);
    let thumb_bytes = dir_size(&thumb_path);
    let apt_bytes = dir_size(apt_path);
    let cache_bytes = dir_size(&cache_path);

    let journal_raw = out(&["journalctl", "--disk-usage"])
        .or_else(|| sudo(&["journalctl", "--disk-usage"]).ok())
        .unwrap_or_default();
    let journal_bytes = parse_journal_usage(&journal_raw);

    // Free and total on root
    let df_out = out(&["df", "-h", "--output=avail,size", "/"]).unwrap_or_default();
    let mut free_str = "?".to_string();
    let mut total_str = "?".to_string();
    if let Some(line) = df_out.lines().nth(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 2 {
            free_str = cols[0].to_string();
            total_str = cols[1].to_string();
        }
    }

    Reclaimable {
        trash_bytes,
        thumb_bytes,
        apt_bytes,
        journal_bytes,
        cache_bytes,
        free_str,
        total_str,
    }
}

/// Headless query mode: `lazyos menu clean --dry-run`.
pub fn dry_run() {
    let rec = calculate_reclaimable();
    println!("trash: {}", format_bytes(rec.trash_bytes));
    println!("thumbnails: {}", format_bytes(rec.thumb_bytes));
    println!("apt: {}", format_bytes(rec.apt_bytes));
    println!("journal: {}", format_bytes(rec.journal_bytes));
    println!("cache: {}", format_bytes(rec.cache_bytes));
    println!("free: {} of {}", rec.free_str, rec.total_str);
}

/// Interactive `lazyos menu clean`.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    let rec = calculate_reclaimable();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());

    let rows = vec![
        row(
            "Free space",
            format!("{} of {}", rec.free_str, rec.total_str),
            Glyph::Disk,
        ),
        row(
            "Empty the trash",
            format_bytes(rec.trash_bytes),
            Glyph::Trash,
        ),
        row(
            "Remove thumbnails",
            format_bytes(rec.thumb_bytes),
            Glyph::Image,
        ),
        row(
            "Clean the package cache",
            format_bytes(rec.apt_bytes),
            Glyph::Box,
        ),
        row(
            "Shrink journal to 100 MB",
            format_bytes(rec.journal_bytes),
            Glyph::Book,
        ),
        row(
            "Your cache (~/.cache)",
            format_bytes(rec.cache_bytes),
            Glyph::Globe,
        ),
    ];

    let keys = vec![
        "none".to_string(),
        "trash".to_string(),
        "thumbnails".to_string(),
        "apt".to_string(),
        "journal".to_string(),
        "cache".to_string(),
    ];

    let mut picker = Picker::new(
        "Free space",
        vec![Question::choose(
            "Free space",
            "Enter cleans that one · Escape closes",
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "Free space", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    let Some(action) = answers.first() else {
        return Ok(1);
    };

    match action.as_str() {
        "trash" => {
            let files = Path::new(&home).join(".local/share/Trash/files");
            let info = Path::new(&home).join(".local/share/Trash/info");
            let _ = fs::remove_dir_all(&files);
            let _ = fs::remove_dir_all(&info);
            say("Free space", "Trash emptied", Urgency::Normal, Glyph::Trash);
            Ok(0)
        }
        "thumbnails" => {
            let thumbs = Path::new(&home).join(".cache/thumbnails");
            let _ = fs::remove_dir_all(&thumbs);
            say(
                "Free space",
                "Thumbnails removed",
                Urgency::Normal,
                Glyph::Image,
            );
            Ok(0)
        }
        "apt" => match sudo(&["apt-get", "clean"]) {
            Ok(_) => {
                say(
                    "Free space",
                    "Package cache cleaned",
                    Urgency::Normal,
                    Glyph::Box,
                );
                Ok(0)
            }
            Err(err) => {
                say(
                    "Free space",
                    format!("Could not clean package cache: {err}"),
                    Urgency::Critical,
                    Glyph::Box,
                );
                Ok(3)
            }
        },
        "journal" => match sudo(&["journalctl", "--vacuum-size=100M"]) {
            Ok(_) => {
                say(
                    "Free space",
                    "Journal shrunk to 100 MB",
                    Urgency::Normal,
                    Glyph::Book,
                );
                Ok(0)
            }
            Err(err) => {
                say(
                    "Free space",
                    format!("Could not vacuum journal: {err}"),
                    Urgency::Critical,
                    Glyph::Book,
                );
                Ok(3)
            }
        },
        "cache" => {
            let cache_dir = Path::new(&home).join(".cache");
            crate::system::spawn_detached(&format!(
                "exec lazyos menu files '{}'",
                cache_dir.display()
            ));
            Ok(0)
        }
        _ => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_scales_appropriately() {
        assert_eq!(format_bytes(0), "0 MB");
        assert_eq!(format_bytes(500 * 1024), "500 KB");
        assert_eq!(format_bytes(50 * 1024 * 1024), "50 MB");
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024), "2.0 GB");
    }

    #[test]
    fn parse_journal_usage_extracts_bytes() {
        let raw = "Archived and active journals take up 64.0M in the file system.";
        assert_eq!(parse_journal_usage(raw), 64 * 1024 * 1024);
        let raw_g = "Journals take up 1.5G in the file system.";
        assert_eq!(
            parse_journal_usage(raw_g),
            (1.5 * 1024.0 * 1024.0 * 1024.0) as u64
        );
    }
}
