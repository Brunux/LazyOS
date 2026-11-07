//! Translation surface (`translate`, `lazyos menu translate`).
//!
//! Translates selected text or clipboard via Google / MyMemory fallback.
//! Supports `--languages`, `--target-name`, and `config` language selector.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{have, out, say};

pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("pt", "Portuguese"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("ru", "Russian"),
    ("uk", "Ukrainian"),
    ("pl", "Polish"),
    ("tr", "Turkish"),
    ("ar", "Arabic"),
    ("he", "Hebrew"),
    ("hi", "Hindi"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("zh-CN", "Chinese (simplified)"),
    ("zh-TW", "Chinese (traditional)"),
    ("vi", "Vietnamese"),
    ("th", "Thai"),
    ("id", "Indonesian"),
    ("sv", "Swedish"),
    ("cs", "Czech"),
    ("el", "Greek"),
    ("ro", "Romanian"),
    ("fa", "Persian"),
];

pub fn name_of(code: &str) -> &'static str {
    LANGUAGES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, n)| *n)
        .unwrap_or("English")
}

pub fn config_file() -> PathBuf {
    let base_config = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            PathBuf::from(home).join(".config")
        });
    base_config.join("lazyos/translate.conf")
}

pub fn target_code() -> String {
    if let Ok(content) = fs::read_to_string(config_file()) {
        for line in content.lines() {
            if let Some(val) = line.trim().strip_prefix("target=") {
                let code = val.trim();
                if LANGUAGES.iter().any(|(c, _)| *c == code) {
                    return code.to_string();
                }
            }
        }
    }
    "en".to_string()
}

pub fn set_target_code(code: &str) -> std::io::Result<()> {
    let p = config_file();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(p, format!("target={code}\n"))
}

/// Headless query: print language table.
pub fn print_languages() {
    for (code, name) in LANGUAGES {
        println!("{code}\t{name}");
    }
}

/// Headless query: print target language name.
pub fn print_target_name() {
    println!("{}", name_of(&target_code()));
}

/// Query Google translation API.
pub fn query_google(text: &str, target: &str) -> Option<(String, String)> {
    let output = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "8",
            "-G",
            "https://clients5.google.com/translate_a/t",
            "--data-urlencode",
            "client=dict-chrome-ex",
            "--data-urlencode",
            "sl=auto",
            "--data-urlencode",
            &format!("tl={target}"),
            "--data-urlencode",
            &format!("q={text}"),
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }
    let body = String::from_utf8_lossy(&output.stdout);
    // Parse JSON array `[[translated_text, detected_code]]`
    let val: serde_json::Value = serde_json::from_str(&body).ok()?;
    let arr = val.as_array()?;
    let mut translated = String::new();
    let mut detected = String::new();
    for item in arr {
        if let Some(pair) = item.as_array() {
            if let Some(t) = pair.first().and_then(|v| v.as_str()) {
                translated.push_str(t);
            }
            if detected.is_empty() {
                if let Some(d) = pair.get(1).and_then(|v| v.as_str()) {
                    detected = d.to_string();
                }
            }
        }
    }
    if translated.trim().is_empty() {
        None
    } else {
        Some((translated, detected))
    }
}

/// Perform translation and notification.
pub fn run_translate() -> i32 {
    let target = target_code();
    let target_name = name_of(&target);

    // Primary selection, then clipboard
    let mut text = out(&["wl-paste", "-p", "-n"]).unwrap_or_default();
    if text.trim().is_empty() {
        text = out(&["wl-paste", "-n"]).unwrap_or_default();
    }
    let text = text.trim();
    if text.is_empty() {
        say(
            "Translate",
            format!("No text selected to translate into {target_name}"),
            Urgency::Normal,
            Glyph::Globe,
        );
        return 0;
    }

    let capped_text: String = text.chars().take(3000).collect();

    say("Translate", "Translating…", Urgency::Normal, Glyph::Globe);

    if let Some((translated, detected)) = query_google(&capped_text, &target) {
        let _ = Command::new("wl-copy").arg(&translated).status();
        let src_name = name_of(&detected);
        let title = format!("{src_name} → {target_name}");
        say(&title, &translated, Urgency::Normal, Glyph::Globe);

        if translated.len() > 400 || translated.lines().count() > 3 {
            // Open floated viewer
            let runtime_dir =
                std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
            let doc = std::path::Path::new(&runtime_dir)
                .join(format!("lazyos-translate.{}.txt", std::process::id()));
            let content = format!("{title}\n\n{translated}\n\n── what was selected ──\n{text}\n");
            if std::fs::write(&doc, content).is_ok() {
                let escaped_title = title.replace('\'', "'\\''");
                let escaped_doc = doc.display().to_string().replace('\'', "'\\''");
                let script = format!(
                    "exec foot --app-id=popup --title='{escaped_title}' -e sh -c 'env BAT_THEME=ansi bat --paging=always --style=plain --file-name=\"$1\" -- \"$2\"; rm -f \"$2\"' _ '{escaped_title}' '{escaped_doc}'"
                );
                crate::system::spawn_detached(&script);
            }
        }
        0
    } else {
        say(
            "Translate",
            "No answer from translator (check connection)",
            Urgency::Critical,
            Glyph::Warning,
        );
        1
    }
}

/// Interactive language configuration picker: `lazyos menu translate config`.
pub fn menu(
    args: &[String],
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if args.first().map(String::as_str) == Some("config") {
        return menu_config(terminal, design);
    }
    Ok(run_translate())
}

pub fn menu_config(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    let cur = target_code();
    let cur_name = name_of(&cur);

    let mut rows = Vec::new();
    let mut keys = Vec::new();

    for (code, name) in LANGUAGES {
        let is_cur = *code == cur;
        let mark = if is_cur { "● " } else { "  " };
        let detail = if is_cur {
            format!("{code} · current")
        } else {
            code.to_string()
        };
        rows.push(row(format!("{}{}", mark, name), detail, Glyph::Globe));
        keys.push(code.to_string());
    }

    let mut picker = Picker::new(
        "Translate",
        vec![Question::choose(
            "Translate Target Language",
            format!("Everything goes into {cur_name} · Enter sets language"),
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "Translate", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    if let Some(chosen) = picker.answers().first() {
        let _ = set_target_code(chosen);
        say(
            "Translate",
            format!("Target language: {}", name_of(chosen)),
            Urgency::Normal,
            Glyph::Globe,
        );
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_contains_core_set() {
        assert_eq!(name_of("en"), "English");
        assert_eq!(name_of("es"), "Spanish");
        assert_eq!(name_of("fr"), "French");
    }

    #[test]
    fn unknown_language_code_falls_back_to_english() {
        assert_eq!(name_of("xx"), "English");
    }
}
