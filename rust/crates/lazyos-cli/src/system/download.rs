//! Media Downloader (`rofi-download`, `lazyos menu download`).
//!
//! Prompts for video/audio URL (auto-populating from clipboard if valid),
//! asks for download profile (video vs audio-only), and spawns yt-dlp into
//! `~/Downloads/`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{have, out, say};

/// Get the first URL in the clipboard if valid.
pub fn clipboard_url() -> Option<String> {
    if !have("wl-paste") {
        return None;
    }
    let text = out(&["wl-paste", "-n"]).unwrap_or_default();
    let first = text.lines().next()?.trim();
    if first.starts_with("http://") || first.starts_with("https://") {
        Some(first.to_string())
    } else {
        None
    }
}

/// Check if yt-dlp is currently running.
pub fn is_downloading() -> bool {
    let procs = out(&["pgrep", "-f", "yt-dlp"]).unwrap_or_default();
    !procs.trim().is_empty()
}

/// Count files in ~/Downloads.
pub fn downloads_count() -> usize {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let dl = Path::new(&home).join("Downloads");
    if let Ok(entries) = fs::read_dir(&dl) {
        entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|ft| ft.is_file()).unwrap_or(false))
            .count()
    } else {
        0
    }
}

/// Spawn yt-dlp download in background.
pub fn spawn_download(url: &str, audio_only: bool) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let dl = Path::new(&home).join("Downloads");
    let _ = fs::create_dir_all(&dl);

    let mut args = vec![
        "-P".to_string(),
        dl.to_string_lossy().to_string(),
        "--no-playlist".to_string(),
        "--no-mtime".to_string(),
    ];

    if audio_only {
        args.push("-x".to_string());
        args.push("--audio-format".to_string());
        args.push("m4a".to_string());
    }

    args.push("--".to_string());
    args.push(url.to_string());

    say(
        "Download",
        format!("Fetching {}…", url.chars().take(48).collect::<String>()),
        Urgency::Normal,
        Glyph::Download,
    );

    let _ = Command::new("yt-dlp").args(&args).spawn();
}

/// Interactive `lazyos menu download`.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if !have("yt-dlp") {
        say(
            "Download",
            "yt-dlp is not installed (package yt-dlp)",
            Urgency::Critical,
            Glyph::Download,
        );
        return Ok(3);
    }

    let clip = clipboard_url();
    let count = downloads_count();

    let mut rows = Vec::new();
    let mut keys = Vec::new();

    rows.push(row(
        "Paste a link",
        "YouTube, Vimeo, SoundCloud, a direct file",
        Glyph::Cloud,
    ));
    keys.push("link".to_string());

    if let Some(ref url) = clip {
        rows.push(row("The link in the clipboard", url, Glyph::Clipboard));
        keys.push(format!("clip:{}", url));
    }

    let s = if count == 1 { "" } else { "s" };
    rows.push(row(
        "The Downloads folder",
        format!("{count} file{s} · ~/Downloads"),
        Glyph::Folder,
    ));
    keys.push("folder".to_string());

    if is_downloading() {
        rows.push(row(
            "Stop the download",
            "kills yt-dlp; the .part file stays",
            Glyph::Warning,
        ));
        keys.push("stop".to_string());
    }

    let mut picker = Picker::new(
        "Download",
        vec![Question::choose(
            "Download",
            "Enter downloads · Escape closes",
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "Download", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    let Some(action) = answers.first() else {
        return Ok(1);
    };

    match action.as_str() {
        "folder" => {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            let dl = Path::new(&home).join("Downloads");
            let _ = Command::new("lazyos")
                .args(["menu", "files", dl.to_str().unwrap_or(".")])
                .status();
            Ok(0)
        }
        "stop" => {
            let _ = Command::new("pkill").args(["-f", "yt-dlp"]).status();
            say(
                "Download",
                "Stopped · the .part file is still in ~/Downloads",
                Urgency::Normal,
                Glyph::Warning,
            );
            Ok(0)
        }
        "link" => {
            let mut url_picker = Picker::new(
                "Link",
                vec![Question::input(
                    "Download URL",
                    "Enter URL starting with http:// or https://",
                    "https://...",
                    false,
                )],
            );
            let uc = crate::run_screen(terminal, design, "Link", &mut url_picker)?;
            if uc != 0 {
                return Ok(uc);
            }
            let Some(raw_url) = url_picker.answers().first() else {
                return Ok(1);
            };
            let url = raw_url.trim();
            if !url.starts_with("http://") && !url.starts_with("https://") {
                say(
                    "Download",
                    "That is not a valid link (must start with http:// or https://)",
                    Urgency::Critical,
                    Glyph::Warning,
                );
                return Ok(3);
            }
            ask_kind_and_download(terminal, design, url)
        }
        act if act.starts_with("clip:") => {
            let url = &act[5..];
            ask_kind_and_download(terminal, design, url)
        }
        _ => Ok(0),
    }
}

fn ask_kind_and_download(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
    url: &str,
) -> Result<i32, crate::Failure> {
    let rows = vec![
        row(
            "Video",
            "the best picture and sound it can merge",
            Glyph::Video,
        ),
        row(
            "Audio only",
            "m4a, from the best audio stream",
            Glyph::Music,
        ),
    ];
    let keys = vec!["video".to_string(), "audio".to_string()];

    let mut picker = Picker::new(
        "Download profile",
        vec![Question::choose("Download profile", url, rows, keys)],
    );

    let code = crate::run_screen(terminal, design, "Download profile", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let audio_only = picker.answers().first().map(String::as_str) == Some("audio");
    spawn_download(url, audio_only);
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_url_prefixes() {
        assert!("https://example.com/video".starts_with("https://"));
        assert!("http://example.com/audio".starts_with("http://"));
        assert!(!"ftp://example.com".starts_with("http://"));
    }
}
