//! Contextual File Actions (`rofi-file-actions`, `lazyos menu file-actions`).
//!
//! Secondary actions for the file under the cursor: disk usage, compress,
//! extract, mount disk image, convert media, copy path, terminal.

use std::path::{Path, PathBuf};
use std::process::Command;

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{have, out, say};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileKind {
    Directory,
    ArchiveZip,
    ArchiveTar,
    DiskImage,
    Video,
    Audio,
    Other,
}

pub fn detect_kind(path: &Path) -> FileKind {
    if path.is_dir() {
        return FileKind::Directory;
    }
    let lower = path
        .file_name()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if lower.ends_with(".zip") {
        FileKind::ArchiveZip
    } else if lower.ends_with(".tar")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".tar.zst")
        || lower.ends_with(".tgz")
        || lower.ends_with(".txz")
        || lower.ends_with(".tbz")
        || lower.ends_with(".tbz2")
        || lower.ends_with(".tzst")
    {
        FileKind::ArchiveTar
    } else if lower.ends_with(".iso") || lower.ends_with(".img") {
        FileKind::DiskImage
    } else if lower.ends_with(".mp4")
        || lower.ends_with(".mkv")
        || lower.ends_with(".webm")
        || lower.ends_with(".mov")
        || lower.ends_with(".avi")
        || lower.ends_with(".m4v")
    {
        FileKind::Video
    } else if lower.ends_with(".mp3")
        || lower.ends_with(".flac")
        || lower.ends_with(".ogg")
        || lower.ends_with(".wav")
        || lower.ends_with(".m4a")
        || lower.ends_with(".aac")
    {
        FileKind::Audio
    } else {
        FileKind::Other
    }
}

/// Interactive `lazyos menu file-actions <path>`.
pub fn menu_for_path(
    path_str: &str,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    let target = Path::new(path_str);
    if !target.exists() && !target.is_symlink() {
        say(
            "File actions",
            format!("{} is not there any more", target.display()),
            Urgency::Critical,
            Glyph::Warning,
        );
        return Ok(3);
    }

    let base = target
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| target.display().to_string());
    let parent = target
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let kind = detect_kind(target);
    let mut rows = Vec::new();
    let mut keys = Vec::new();

    if kind == FileKind::Directory {
        rows.push(row("Disk usage", "du -sh: size calculation", Glyph::Disk));
        keys.push("du".to_string());
    }

    rows.push(row("Compress", format!("→ {base}.tar.xz"), Glyph::Box));
    keys.push("pack".to_string());

    if matches!(kind, FileKind::ArchiveZip | FileKind::ArchiveTar) {
        rows.push(row(
            "Extract here",
            "→ folder named after archive",
            Glyph::Box,
        ));
        keys.push("unpack".to_string());
    }

    if kind == FileKind::DiskImage {
        rows.push(row(
            "Mount the image",
            "udisksctl loop-setup then open",
            Glyph::Disk,
        ));
        keys.push("mount".to_string());
    }

    if matches!(kind, FileKind::Video | FileKind::Audio) {
        rows.push(row(
            "Convert",
            "ffmpeg: MP4 video or MP3 audio",
            Glyph::Video,
        ));
        keys.push("convert".to_string());
    }

    rows.push(row(
        "Copy the path",
        target.display().to_string(),
        Glyph::Clipboard,
    ));
    keys.push("copy".to_string());

    rows.push(row(
        "A terminal here",
        parent.display().to_string(),
        Glyph::Terminal,
    ));
    keys.push("term".to_string());

    let mut picker = Picker::new(
        &format!("File actions · {base}"),
        vec![Question::choose(
            format!("File actions · {base}"),
            target.display().to_string(),
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(
        terminal,
        design,
        &format!("File actions · {base}"),
        &mut picker,
    )?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    let Some(action) = answers.first() else {
        return Ok(1);
    };

    match action.as_str() {
        "copy" => {
            let _ = Command::new("wl-copy")
                .arg(target.to_str().unwrap_or(""))
                .status();
            say(
                "File actions",
                "The path is in the clipboard",
                Urgency::Normal,
                Glyph::Clipboard,
            );
            Ok(0)
        }
        "term" => {
            let dir = if kind == FileKind::Directory {
                target
            } else {
                &parent
            };
            crate::system::spawn_detached(&format!(
                "exec foot --working-directory '{}'",
                dir.display()
            ));
            Ok(0)
        }
        "du" => {
            let out_str = out(&["du", "-sh", target.to_str().unwrap_or("")]).unwrap_or_default();
            let size = out_str.split_whitespace().next().unwrap_or("?");
            say(
                "Disk usage",
                format!("{base} · {size}"),
                Urgency::Normal,
                Glyph::Disk,
            );
            Ok(0)
        }
        "pack" => {
            let out_archive = parent.join(format!("{base}.tar.xz"));
            let res = Command::new("tar")
                .args([
                    "-caf",
                    out_archive.to_str().unwrap_or(""),
                    "-C",
                    parent.to_str().unwrap_or("."),
                    "--",
                    &base,
                ])
                .status();
            if res.map(|s| s.success()).unwrap_or(false) {
                say(
                    "Compress",
                    format!("Packed {base}.tar.xz"),
                    Urgency::Normal,
                    Glyph::Box,
                );
                Ok(0)
            } else {
                say(
                    "Compress",
                    format!("Could not pack {base}"),
                    Urgency::Critical,
                    Glyph::Warning,
                );
                Ok(3)
            }
        }
        "unpack" => {
            let stem = base
                .strip_suffix(".tar.gz")
                .or_else(|| base.strip_suffix(".tar.xz"))
                .or_else(|| base.strip_suffix(".tar.bz2"))
                .or_else(|| base.strip_suffix(".tar.zst"))
                .or_else(|| base.strip_suffix(".zip"))
                .or_else(|| base.strip_suffix(".tar"))
                .unwrap_or(&base);
            let dest = parent.join(stem);
            let _ = std::fs::create_dir_all(&dest);
            let status = if kind == FileKind::ArchiveZip {
                Command::new("unzip")
                    .args([
                        "-q",
                        "-o",
                        target.to_str().unwrap_or(""),
                        "-d",
                        dest.to_str().unwrap_or("."),
                    ])
                    .status()
            } else {
                Command::new("tar")
                    .args([
                        "-xf",
                        target.to_str().unwrap_or(""),
                        "-C",
                        dest.to_str().unwrap_or("."),
                    ])
                    .status()
            };
            if status.map(|s| s.success()).unwrap_or(false) {
                say(
                    "Extract",
                    format!("Unpacked into {stem}/"),
                    Urgency::Normal,
                    Glyph::Box,
                );
                Ok(0)
            } else {
                say(
                    "Extract",
                    format!("Could not unpack {base}"),
                    Urgency::Critical,
                    Glyph::Warning,
                );
                Ok(3)
            }
        }
        "mount" => {
            let res = Command::new("udisksctl")
                .args(["loop-setup", "-f", target.to_str().unwrap_or("")])
                .output();
            if let Ok(o) = res {
                let text = String::from_utf8_lossy(&o.stdout);
                if let Some(pos) = text.find("/dev/loop") {
                    let loop_dev = text[pos..].split_whitespace().next().unwrap_or("");
                    let mount_res = Command::new("udisksctl")
                        .args(["mount", "-b", loop_dev])
                        .output();
                    if let Ok(mo) = mount_res {
                        let mtext = String::from_utf8_lossy(&mo.stdout);
                        if let Some(at) = mtext.rfind(" at ") {
                            let mp = mtext[at + 4..].trim().trim_end_matches('.');
                            let _ = Command::new("lazyos").args(["menu", "files", mp]).status();
                        }
                    }
                }
            }
            Ok(0)
        }
        "convert" => {
            let out_file = parent.join(format!("{}-converted.mp4", base));
            let escaped_target = target.display().to_string().replace('\'', "'\\''");
            let escaped_out = out_file.display().to_string().replace('\'', "'\\''");
            let escaped_base = base.replace('\'', "'\\''");
            let cmd = format!(
                "ffmpeg -hide_banner -loglevel error -y -i '{escaped_target}' -c:v libx264 -crf 23 -preset veryfast -c:a aac -b:a 160k '{escaped_out}' 2>&1; \
                if [ $? -eq 0 ]; then \
                    lazyos notify send --app Convert --title 'Conversion complete' --body '{escaped_base}' --glyph video; \
                else \
                    lazyos notify send --app Convert --title 'Conversion failed' --body '{escaped_base}' --glyph video --urgency critical; \
                fi"
            );
            crate::system::spawn_detached(&cmd);
            say(
                "Convert",
                format!("Converting {base} in background…"),
                Urgency::Normal,
                Glyph::Video,
            );
            Ok(0)
        }
        _ => Ok(0),
    }
}

pub fn menu(
    args: &[String],
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    let path = args.first().cloned().unwrap_or_else(|| ".".to_string());
    menu_for_path(&path, terminal, design)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_kind_extension_matching() {
        assert_eq!(detect_kind(Path::new("archive.zip")), FileKind::ArchiveZip);
        assert_eq!(
            detect_kind(Path::new("bundle.tar.gz")),
            FileKind::ArchiveTar
        );
        assert_eq!(detect_kind(Path::new("os.iso")), FileKind::DiskImage);
        assert_eq!(detect_kind(Path::new("song.mp3")), FileKind::Audio);
        assert_eq!(detect_kind(Path::new("clip.mp4")), FileKind::Video);
        assert_eq!(detect_kind(Path::new("doc.pdf")), FileKind::Other);
    }
}
