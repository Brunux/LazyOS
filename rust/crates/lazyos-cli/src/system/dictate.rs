//! Voice Dictation (`dictate`, `lazyos menu dictate`).
//!
//! Press shortcut / toggle to record, speak, toggle again to transcribe
//! and type result into the focused window using whisper-cli and wtype.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use lazyos_ui::{Glyph, Urgency};

use super::{have, say};

fn runtime_dir() -> PathBuf {
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) => PathBuf::from(dir).join("dictate"),
        Err(_) => PathBuf::from("/tmp/dictate"),
    }
}

pub fn model_path() -> PathBuf {
    if let Ok(m) = std::env::var("DICTATE_MODEL") {
        return PathBuf::from(m);
    }
    let data_dir = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            PathBuf::from(home).join(".local/share")
        });
    data_dir.join("whisper/ggml-tiny.en.bin")
}

/// Toggle dictation recording / transcription.
pub fn toggle() -> i32 {
    let dir = runtime_dir();
    let _ = fs::create_dir_all(&dir);
    let state_file = dir.join("pid");
    let wav_file = dir.join("rec.wav");
    let norm_file = dir.join("norm.wav");

    if state_file.exists() {
        // --- Stop and transcribe ---
        let pid_str = fs::read_to_string(&state_file).unwrap_or_default();
        let _ = fs::remove_file(&state_file);

        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            let _ = Command::new("kill")
                .args(["-INT", &pid.to_string()])
                .status();
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        if !wav_file.exists() || fs::metadata(&wav_file).map(|m| m.len()).unwrap_or(0) == 0 {
            say(
                "Dictation",
                "Nothing was recorded",
                Urgency::Critical,
                Glyph::Warning,
            );
            return 1;
        }

        say("Dictation", "Transcribing…", Urgency::Normal, Glyph::Mic);

        // Optional normalize with ffmpeg
        let mut audio_target = wav_file.clone();
        if have("ffmpeg") {
            let norm_res = Command::new("ffmpeg")
                .args([
                    "-nostdin",
                    "-loglevel",
                    "error",
                    "-y",
                    "-i",
                    wav_file.to_str().unwrap_or(""),
                    "-af",
                    "dynaudnorm=f=150:g=15",
                    "-ar",
                    "16000",
                    "-ac",
                    "1",
                    norm_file.to_str().unwrap_or(""),
                ])
                .status();
            if norm_res.map(|s| s.success()).unwrap_or(false) && norm_file.exists() {
                audio_target = norm_file.clone();
            }
        }

        let model = model_path();
        let out = Command::new("whisper-cli")
            .args([
                "-m",
                model.to_str().unwrap_or(""),
                "-f",
                audio_target.to_str().unwrap_or(""),
                "-l",
                "en",
                "-nt",
                "-np",
            ])
            .output();

        let _ = fs::remove_file(&wav_file);
        let _ = fs::remove_file(&norm_file);

        let text = match out {
            Ok(res) if res.status.success() => {
                let raw = String::from_utf8_lossy(&res.stdout);
                raw.lines()
                    .map(|l| l.trim())
                    .filter(|l| !l.is_empty() && !l.starts_with('['))
                    .collect::<Vec<&str>>()
                    .join(" ")
            }
            _ => String::new(),
        };

        if text.trim().is_empty() {
            say(
                "Dictation",
                "Nothing was recognised",
                Urgency::Critical,
                Glyph::Warning,
            );
            return 1;
        }

        // Copy to clipboard
        let _ = Command::new("wl-copy").arg(&text).status();

        // Type into focused window
        if have("wtype") {
            let typed = Command::new("wtype")
                .arg(&text)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if typed {
                say(
                    "Dictation",
                    format!("Typed · {}", text.chars().take(60).collect::<String>()),
                    Urgency::Normal,
                    Glyph::Mic,
                );
            } else {
                say(
                    "Dictation",
                    "Copied to clipboard · typing failed",
                    Urgency::Normal,
                    Glyph::Mic,
                );
            }
        } else {
            say(
                "Dictation",
                "Copied to clipboard · wtype is missing",
                Urgency::Normal,
                Glyph::Mic,
            );
        }
        0
    } else {
        // --- Start recording ---
        let model = model_path();
        if !model.exists() {
            say(
                "Dictation",
                "Fetching speech model (78 MB, once)…",
                Urgency::Normal,
                Glyph::Mic,
            );
            let _ = Command::new("foot")
                .args(["-T", "Dictation · fetching model", "-e", "dictate-model"])
                .spawn();
            return 0;
        }

        if !have("parecord") {
            say(
                "Dictation",
                "parecord is missing (package pulseaudio-utils)",
                Urgency::Critical,
                Glyph::Warning,
            );
            return 1;
        }

        let _ = Command::new("playerctl").args(["-a", "pause"]).status();
        let _ = fs::remove_file(&wav_file);

        let child = Command::new("parecord")
            .args([
                "--rate=16000",
                "--channels=1",
                "--format=s16le",
                "--file-format=wav",
                wav_file.to_str().unwrap_or(""),
            ])
            .spawn();

        match child {
            Ok(c) => {
                let _ = fs::write(&state_file, c.id().to_string());
                say(
                    "Dictation",
                    "Recording · press Alt+Ctrl+X or toggle again to stop",
                    Urgency::Normal,
                    Glyph::Mic,
                );
                0
            }
            Err(e) => {
                say(
                    "Dictation",
                    format!("Could not start recorder: {e}"),
                    Urgency::Critical,
                    Glyph::Warning,
                );
                1
            }
        }
    }
}

/// Entrypoint for `lazyos menu dictate`.
pub fn menu() -> Result<i32, crate::Failure> {
    Ok(toggle())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_path_defaults_to_whisper_tiny() {
        let p = model_path();
        assert!(p.ends_with("whisper/ggml-tiny.en.bin"));
    }
}
