//! Capture: screenshots, annotate, OCR, QR, recording (`rofi-capture`).
//!
//! The old script was a rofi list over tools the image already has. Every row is
//! one command that ends before the list closes: grimshot takes the picture
//! (screen, area, window, saved under ~/Pictures/Screenshots/<date> and named
//! in a notification), swappy annotates one, tesseract reads the text out of a
//! region and zbarimg reads a QR or barcode out of one. Both copy the result
//! to the clipboard and say so.
//!
//! Recording is the exception: wf-recorder keeps running after the menu closes,
//! so it is started here, announced on the bar and in a mako card, and stopped
//! by opening this same menu again. Its pid and file name live in
//! $XDG_RUNTIME_DIR/lazyos-capture, so a session that dies takes the state with
//! it and a killed recorder is not remembered; the file it was writing stays.

#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
use std::cell::Cell;
use std::path::Path;
use std::time::Duration;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use crate::system::{missing, out, print, say};

const NEEDED: &[&str] = &[
    "grimshot",
    "swappy",
    "tesseract",
    "zbarimg",
    "wf-recorder",
    "slurp",
    "grim",
];

/// The capture screen.
pub struct CaptureScreen {
    recording: bool,
    rec_file: String,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    keys: Vec<String>,
    open_qr: bool,
}

impl CaptureScreen {
    pub fn new() -> Self {
        let mut screen = Self {
            keys: Vec::new(),
            recording: false,
            rec_file: String::new(),
            list: ListState::new(),
            header: Header::new("LazyOS").subtitle("Capture"),
            keybar: vec![KeyHint::new("Enter", "run"), KeyHint::new("Esc", "close")],
            rows_height: Cell::new(20),
            open_qr: false,
        };
        screen.check_recording();
        screen.build_rows();
        screen
    }

    fn check_recording(&mut self) {
        let state = std::env::var("XDG_RUNTIME_DIR")
            .map(|d| Path::new(&d).join("lazyos-capture"))
            .unwrap_or_else(|_| Path::new("/tmp").join("lazyos-capture"));
        let pid_file = state.join("recording.pid");
        let file_file = state.join("recording.file");
        if pid_file.exists() && file_file.exists() {
            if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    if std::process::Command::new("kill")
                        .args(["-0", &pid.to_string()])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false)
                    {
                        self.recording = true;
                        self.rec_file = std::fs::read_to_string(&file_file)
                            .unwrap_or_default()
                            .trim()
                            .to_string();
                        return;
                    }
                }
            }
            // Stale state
            let _ = std::fs::remove_file(&pid_file);
            let _ = std::fs::remove_file(&file_file);
        }
    }

    fn build_rows(&mut self) {
        let mut rows = Vec::new();
        if self.recording {
            rows.push(
                Row::new("Stop recording")
                    .glyph(Glyph::Stop)
                    .detail(format!(
                        "{}",
                        Path::new(&self.rec_file)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    ))
                    .value("stop"),
            );
        } else {
            let shotdir = dirs::picture_dir()
                .map(|p| p.join("Screenshots"))
                .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures"));
            let day = chrono::Local::now().format("%Y-%m-%d").to_string();
            rows.push(
                Row::new("Screenshot · the whole screen")
                    .glyph(Glyph::Camera)
                    .detail(format!("saved to {}/{}", shotdir.display(), day))
                    .value("shot:save:screen"),
            );
            rows.push(
                Row::new("Screenshot · an area")
                    .glyph(Glyph::Crop)
                    .detail(format!("saved to {}/{}", shotdir.display(), day))
                    .value("shot:save:area"),
            );
            rows.push(
                Row::new("Screenshot · an area, to the clipboard")
                    .glyph(Glyph::Crop)
                    .detail("paste it anywhere")
                    .value("shot:copy:area"),
            );
            rows.push(
                Row::new("Screenshot · a window")
                    .glyph(Glyph::Image)
                    .detail("pick one with the pointer")
                    .value("shot:save:window"),
            );
            rows.push(
                Row::new("Annotate an area")
                    .glyph(Glyph::Camera)
                    .detail("arrows, text, crop (swappy)")
                    .value("annotate"),
            );
            rows.push(
                Row::new("Record · the whole screen")
                    .glyph(Glyph::Video)
                    .detail("until you stop it")
                    .value("rec:none"),
            );
            rows.push(
                Row::new("Record · an area")
                    .glyph(Glyph::Video)
                    .detail("pick it with the pointer")
                    .value("rec:area"),
            );
        }
        rows.push(Row::new("").glyph(Glyph::Camera)); // separator
        rows.push(
            Row::new("Text from an area")
                .glyph(Glyph::File)
                .detail("OCR (tesseract) → clipboard")
                .value("ocr"),
        );
        rows.push(
            Row::new("QR code from an area")
                .glyph(Glyph::Qr)
                .detail("zbarimg → clipboard")
                .value("qr"),
        );
        rows.push(
            Row::new("Make a QR code")
                .glyph(Glyph::Qr)
                .detail("the Wi-Fi password, or any text")
                .value("make-qr"),
        );
        let shotdir = dirs::picture_dir()
            .map(|p| p.join("Screenshots"))
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures"));
        rows.push(
            Row::new("Open the pictures folder")
                .glyph(Glyph::File)
                .detail(shotdir.display().to_string())
                .value("folder"),
        );
        self.keys = rows.iter().filter_map(|r| r.value.clone()).collect();
        self.list.set_rows(rows);
    }
}

impl Screen for CaptureScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Capture"), true);
        let (filter, rows) = super::pick::list_layout(ctx.area);
        ctx.buf.set_string(
            filter.x,
            filter.y,
            "Enter runs · Escape closes",
            ctx.design.style(TextStyle::Dim),
        );
        self.rows_height.set(rows.height);
        self.list.render(rows, ctx.buf, ctx.design, true);
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }
        let rows_height = self.rows_height.get();
        match key.code {
            KeyCode::Esc => return Outcome::Done(lazyos_ui::app::exit::ESCAPE),
            KeyCode::Enter => {
                if let Some(index) = self.list.selected_index() {
                    return self.activate(index);
                }
            }
            _ => {}
        }
        match self.list.handle(key, rows_height, design) {
            ListAction::Activated(index) => self.activate(index),
            _ => Outcome::Continue,
        }
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        if let Some(row) = row {
            self.list.set_cursor(row);
            if let Some(index) = self.list.selected_index() {
                return self.activate(index);
            }
        }
        Outcome::Continue
    }

    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        let (_, rows) = super::pick::list_layout(area);
        self.list.hit_row(rows, x, y, design)
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        Some(&self.keybar)
    }

    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }
}

impl CaptureScreen {
    fn activate(&mut self, index: usize) -> Outcome {
        let rows = self.list.rows();
        if index >= rows.len() {
            return Outcome::Continue;
        }
        let key = self.keys.get(index).cloned().unwrap_or_default();
        if key.is_empty() || key == "-" {
            return Outcome::Continue;
        }
        if key.starts_with("shot:") {
            let action = &key[5..];
            let target = action.split(':').nth(1).unwrap_or("");
            let act = action.split(':').next().unwrap_or("");
            Self::screenshot(act, target);
            Outcome::Done(lazyos_ui::app::exit::OK)
        } else {
            match key.as_str() {
                "annotate" => {
                    Self::annotate();
                    return Outcome::Done(lazyos_ui::app::exit::OK);
                }
                "ocr" => {
                    Self::ocr();
                    return Outcome::Done(lazyos_ui::app::exit::OK);
                }
                "qr" => {
                    Self::scan_qr();
                    return Outcome::Done(lazyos_ui::app::exit::OK);
                }
                "rec:none" => {
                    Self::start_recording(None);
                    self.check_recording();
                    self.build_rows();
                }
                "rec:area" => {
                    if let Some(geom) = Self::slurp_area() {
                        Self::start_recording(Some(geom));
                        self.check_recording();
                        self.build_rows();
                    }
                }
                "stop" => {
                    Self::stop_recording();
                    self.check_recording();
                    self.build_rows();
                }
                "make-qr" => {
                    self.open_qr = true;
                    return Outcome::Done(lazyos_ui::app::exit::OK);
                }
                "folder" => {
                    let shotdir = dirs::picture_dir()
                        .map(|p| p.join("Screenshots"))
                        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures"));
                    let _ = std::process::Command::new("xdg-open")
                        .arg(&shotdir)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();
                    return Outcome::Done(lazyos_ui::app::exit::OK);
                }
                _ => {}
            }
            Outcome::Continue
        }
    }

    /// Build the argv to invoke `~/.config/bin/screenshot` so saved screenshots
    /// land in `~/Pictures/Screenshots/<date>/` instead of `XDG_PICTURES_DIR`.
    pub fn screenshot_argv(home: &str, action: &str, target: &str) -> Vec<String> {
        let mut argv = vec![
            format!("{home}/.config/bin/screenshot"),
            "-n".to_string(),
            action.to_string(),
        ];
        if !target.is_empty() {
            argv.push(target.to_string());
        }
        argv
    }

    fn screenshot(action: &str, target: &str) {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let argv = Self::screenshot_argv(&home, action, target);
        let _ = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    fn annotate() {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let script = format!("{home}/.config/bin/screenshot");
        let shotdir = dirs::picture_dir()
            .map(|p| p.join("Screenshots"))
            .unwrap_or_else(|| Path::new(&home).join("Pictures/Screenshots"));
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        let dir = shotdir.join(day);
        let _ = std::fs::create_dir_all(&dir);
        let out_file = dir.join(format!(
            "annotated-{}.png",
            chrono::Local::now().format("%H%M%S")
        ));
        let tmp = std::env::temp_dir().join(format!("annotate-{}.png", std::process::id()));
        if Self::region_image(&tmp) {
            let _ = std::process::Command::new("swappy")
                .args([
                    "-f",
                    &tmp.to_string_lossy(),
                    "-o",
                    &out_file.to_string_lossy(),
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            let _ = std::fs::remove_file(&tmp);
            if out_file.exists() && out_file.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
                let name = out_file
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                say(
                    "Capture",
                    format!("Saved {name}"),
                    lazyos_ui::Urgency::Normal,
                    Glyph::Camera,
                );
            } else {
                say(
                    "Capture",
                    "The annotation was not saved",
                    lazyos_ui::Urgency::Normal,
                    Glyph::Camera,
                );
            }
        } else {
            let _ = script;
        }
    }

    fn ocr() {
        let tmp = std::env::temp_dir().join(format!("capture-{}.png", std::process::id()));
        if Self::region_image(&tmp) {
            let text = out(&["tesseract", &tmp.to_string_lossy(), "stdout", "-l", "eng"])
                .unwrap_or_default()
                .trim()
                .to_string();
            let _ = std::fs::remove_file(&tmp);
            if !text.is_empty() {
                let preview = text
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(60)
                    .collect::<String>();
                print(&format!("{}", text));
                say(
                    "Text from the screen",
                    format!("Text copied · {}", preview),
                    lazyos_ui::Urgency::Normal,
                    Glyph::File,
                );
                let _ = std::process::Command::new("wl-copy")
                    .arg(&text)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            } else {
                say(
                    "Text from the screen",
                    "No text was found in that region",
                    lazyos_ui::Urgency::Normal,
                    Glyph::File,
                );
            }
        }
    }

    fn scan_qr() {
        let tmp = std::env::temp_dir().join(format!("capture-{}.png", std::process::id()));
        if Self::region_image(&tmp) {
            let code = out(&["zbarimg", "-q", "--raw", &tmp.to_string_lossy()])
                .unwrap_or_default()
                .trim()
                .to_string();
            let _ = std::fs::remove_file(&tmp);
            if !code.is_empty() {
                let preview = code.chars().take(60).collect::<String>();
                say(
                    "QR code from the screen",
                    format!("Copied · {}", preview),
                    lazyos_ui::Urgency::Normal,
                    Glyph::Qr,
                );
                let _ = std::process::Command::new("wl-copy")
                    .arg(&code)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            } else {
                say(
                    "QR code from the screen",
                    "No code was found in that region",
                    lazyos_ui::Urgency::Normal,
                    Glyph::Qr,
                );
            }
        }
    }

    fn region_image(path: &Path) -> bool {
        if let Some(geom) = Self::slurp_area() {
            let status = std::process::Command::new("grim")
                .args(["-g", &geom, &path.to_string_lossy()])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            if status.map(|s| s.success()).unwrap_or(false) {
                return true;
            }
        }
        false
    }

    fn slurp_area() -> Option<String> {
        out(&["slurp", "-d"])
    }

    fn start_recording(geom: Option<String>) {
        let state = std::env::var("XDG_RUNTIME_DIR")
            .map(|d| Path::new(&d).join("lazyos-capture"))
            .unwrap_or_else(|_| Path::new("/tmp").join("lazyos-capture"));
        let _ = std::fs::create_dir_all(&state);
        let pid_file = state.join("recording.pid");
        let file_file = state.join("recording.file");
        let videos = dirs::video_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Videos"));
        let file = videos.join(format!(
            "lazyos-{}.mp4",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        ));
        let mut cmd = std::process::Command::new("wf-recorder");
        cmd.args([
            "-c",
            "libx264",
            "-p",
            "preset=veryfast",
            "-p",
            "crf=28",
            "-r",
            "30",
            "-f",
            &file.to_string_lossy(),
        ]);
        if let Some(g) = geom {
            cmd.args(["-g", &g]);
        }
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::from(
                std::fs::File::create(state.join("recorder.log"))
                    .unwrap_or_else(|_| std::fs::File::create("/dev/null").unwrap()),
            ));
        if let Ok(mut child) = cmd.spawn() {
            let pid = child.id();
            let _ = std::fs::write(&pid_file, pid.to_string());
            let _ = std::fs::write(&file_file, file.to_string_lossy().to_string());
            // Detach
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            // Brief wait to check it started
            std::thread::sleep(Duration::from_millis(500));
            if std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                print(&format!("Recording started: {}", file.display()));
            } else {
                let _ = std::fs::remove_file(&pid_file);
                let _ = std::fs::remove_file(&file_file);
            }
        }
    }

    fn stop_recording() {
        let state = std::env::var("XDG_RUNTIME_DIR")
            .map(|d| Path::new(&d).join("lazyos-capture"))
            .unwrap_or_else(|_| Path::new("/tmp").join("lazyos-capture"));
        let pid_file = state.join("recording.pid");
        let file_file = state.join("recording.file");
        if pid_file.exists() && file_file.exists() {
            if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    let file = std::fs::read_to_string(&file_file).unwrap_or_default();
                    let _ = std::process::Command::new("kill")
                        .args(["-INT", &pid.to_string()])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                    // Wait for it to finish
                    let mut n = 0;
                    while n < 50 {
                        std::thread::sleep(Duration::from_millis(100));
                        if std::process::Command::new("kill")
                            .args(["-0", &pid.to_string()])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status()
                            .map(|s| !s.success())
                            .unwrap_or(true)
                        {
                            break;
                        }
                        n += 1;
                    }
                    let _ = std::fs::remove_file(&pid_file);
                    let _ = std::fs::remove_file(&file_file);
                    let path = Path::new(&file);
                    if path.exists() && path.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
                        print(&format!("Recording saved: {}", path.display()));
                        say(
                            "Capture",
                            format!(
                                "Recording saved · {}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            ),
                            lazyos_ui::Urgency::Normal,
                            Glyph::Video,
                        );
                    } else {
                        say(
                            "Capture",
                            format!(
                                "Recording failed · {} is empty",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            ),
                            lazyos_ui::Urgency::Critical,
                            Glyph::Video,
                        );
                    }
                }
            }
        }
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        if let Some(row) = row {
            self.list.set_cursor(row);
            if let Some(index) = self.list.selected_index() {
                return self.activate(index);
            }
        }
        Outcome::Continue
    }

    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        let (_, rows) = super::pick::list_layout(area);
        self.list.hit_row(rows, x, y, design)
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        Some(&self.keybar)
    }

    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }
}

impl CaptureScreen {}

/// `lazyos menu capture` (`rofi-capture`): the window.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Capture",
            format!("{said} (grimshot, swappy, tesseract, zbarimg, wf-recorder, slurp, grim)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Capture,
        );
        return Ok(3);
    }
    let mut screen = CaptureScreen::new();
    let code = crate::run_screen(terminal, design, "Capture", &mut screen)?;
    if screen.open_qr {
        return super::qr::menu(terminal, design);
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_needed_tools() {
        assert!(NEEDED.contains(&"grimshot"));
        assert!(NEEDED.contains(&"wf-recorder"));
    }

    #[test]
    fn screenshot_uses_wrapper_in_config_bin() {
        let argv = CaptureScreen::screenshot_argv("/home/brunux", "save", "area");
        assert_eq!(
            argv,
            vec!["/home/brunux/.config/bin/screenshot", "-n", "save", "area"]
        );
    }
}
