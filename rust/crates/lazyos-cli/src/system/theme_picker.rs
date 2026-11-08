//! Theme & Appearance selector: interactive visual theme browser with live preview (`rofi-theme`).
//!
//! Provides a zero-friction, eye-candy dual-pane TUI for exploring, previewing and applying
//! LazyOS color themes and wallpapers. Adheres strictly to the LazyOS framework:
//! single multi-call binary, no resident daemons, event-driven Ratatui rendering,
//! truecolor palette swatches, mini desktop UI mockups, WCAG contrast verification,
//! wallpaper cycling, and live desktop peeking with safe rollback.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::PathBuf;

use image::RgbImage;
use lazyos_theme::{Theme, builtin, contrast, store};
use lazyos_ui::app::{BodyCtx, Outcome, Screen};
use lazyos_ui::design::{Border, ColorRole, Density, Design, TextStyle};
use lazyos_ui::widgets::{
    KeyHint, ListAction, ListState, Row, draw_card, draw_card_with, put, put_trunc,
};
use lazyos_ui::{Glyph, Header};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::Failure;
use crate::theme::{apply_theme, cycle_wallpaper_for, paths};

fn fill_rect(buf: &mut ratatui::buffer::Buffer, area: Rect, style: Style) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_style(style);
            }
        }
    }
}

/// Discovers and loads a 40x16 PNG wallpaper thumbnail from standard system and user cache locations.
fn load_wallpaper_thumbnail(filename: &str) -> Option<RgbImage> {
    let mut dirs_to_search = Vec::new();

    if let Ok(home) = std::env::var("HOME") {
        let h = PathBuf::from(&home);
        dirs_to_search.push(h.join(".cache/lazyos/wallpapers/thumbnails"));
        dirs_to_search.push(h.join("Pictures/Wallpapers/thumbnails"));
        dirs_to_search.push(h.join("Pictures/Wallpapers"));
    }
    dirs_to_search.push(PathBuf::from("/usr/share/backgrounds/lazyos/thumbnails"));
    dirs_to_search.push(PathBuf::from("/usr/share/lazyos/wallpapers/thumbnails"));
    dirs_to_search.push(PathBuf::from(
        "uis/sway/files/home/Pictures/Wallpapers/thumbnails",
    ));
    dirs_to_search.push(PathBuf::from("uis/sway/files/home/Pictures/Wallpapers"));

    let bare = filename
        .strip_suffix(".jpg")
        .or_else(|| filename.strip_suffix(".png"))
        .unwrap_or(filename);

    let candidates = [
        format!("{filename}.png"),
        filename.to_string(),
        format!("{bare}.png"),
    ];

    for dir in &dirs_to_search {
        for candidate in &candidates {
            let path = dir.join(candidate);
            if path.is_file() {
                if let Ok(bytes) = std::fs::read(&path) {
                    if let Ok(img) = image::load_from_memory(&bytes) {
                        return Some(img.to_rgb8());
                    }
                }
            }
        }
    }
    None
}

/// Three-stop linear interpolation for smooth wallpaper gradient fallbacks.
fn interpolate_rgb(
    c1: lazyos_theme::Rgb,
    c2: lazyos_theme::Rgb,
    c3: lazyos_theme::Rgb,
    t: f32,
) -> lazyos_theme::Rgb {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        let f = t * 2.0;
        let r = (c1.r as f32 * (1.0 - f) + c2.r as f32 * f).round() as u8;
        let g = (c1.g as f32 * (1.0 - f) + c2.g as f32 * f).round() as u8;
        let b = (c1.b as f32 * (1.0 - f) + c2.b as f32 * f).round() as u8;
        lazyos_theme::Rgb::new(r, g, b)
    } else {
        let f = (t - 0.5) * 2.0;
        let r = (c2.r as f32 * (1.0 - f) + c3.r as f32 * f).round() as u8;
        let g = (c2.g as f32 * (1.0 - f) + c3.g as f32 * f).round() as u8;
        let b = (c2.b as f32 * (1.0 - f) + c3.b as f32 * f).round() as u8;
        lazyos_theme::Rgb::new(r, g, b)
    }
}

/// Renders a truecolor half-block (`▀`) graphical matrix directly into the Ratatui cell buffer.
fn render_wallpaper_preview(
    buf: &mut ratatui::buffer::Buffer,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    thumb: Option<&RgbImage>,
    theme: &Theme,
) {
    if width == 0 || height == 0 {
        return;
    }

    let roles = theme.roles();
    let bg_rgb = theme.colors.background.0;
    let surface_rgb = roles.surface.0;
    let accent_rgb = roles.accent.0;

    for row in 0..height {
        let py = y + row;
        for col in 0..width {
            let px = x + col;

            let (top_rgb, bot_rgb) = match thumb {
                Some(img) => {
                    let img_w = img.width();
                    let img_h = img.height();
                    if img_w > 0 && img_h > 0 {
                        let ix = (col as u32 * img_w) / width as u32;
                        let iy_top = (row as u32 * 2 * img_h) / (height as u32 * 2);
                        let iy_bot = ((row as u32 * 2 + 1) * img_h) / (height as u32 * 2);

                        let p_top = img.get_pixel(ix.min(img_w - 1), iy_top.min(img_h - 1));
                        let p_bot = img.get_pixel(ix.min(img_w - 1), iy_bot.min(img_h - 1));

                        (
                            lazyos_theme::Rgb::new(p_top[0], p_top[1], p_top[2]),
                            lazyos_theme::Rgb::new(p_bot[0], p_bot[1], p_bot[2]),
                        )
                    } else {
                        (bg_rgb, surface_rgb)
                    }
                }
                None => {
                    let t_top = (col + row * 2) as f32 / (width + height * 2) as f32;
                    let t_bot = (col + row * 2 + 1) as f32 / (width + height * 2) as f32;

                    let c_top = interpolate_rgb(bg_rgb, surface_rgb, accent_rgb, t_top);
                    let c_bot = interpolate_rgb(bg_rgb, surface_rgb, accent_rgb, t_bot);
                    (c_top, c_bot)
                }
            };

            let fg = lazyos_ui::design::color(top_rgb);
            let bg = lazyos_ui::design::color(bot_rgb);
            let style = Style::default().fg(fg).bg(bg);

            if let Some(cell) = buf.cell_mut((px, py)) {
                cell.set_char('\u{2580}').set_style(style);
            }
        }
    }
}

/// An entry in the theme browser.
#[derive(Clone, Debug)]
pub struct ThemeEntry {
    pub slug: String,
    pub theme: Theme,
    pub contrast_text: f32,
    pub contrast_accent: f32,
    pub contrast_danger: f32,
    pub contrast_warn: f32,
    pub contrast_ok: f32,
}

/// The interactive theme picker screen.
pub struct ThemePickerScreen {
    pub entries: Vec<ThemeEntry>,
    pub list: ListState,
    pub active_slug: String,
    pub original_slug: String,
    pub original_theme: Theme,
    pub live_design: Design,
    pub committed: bool,
    pub peeking: bool,
    pub header: Header,
    pub keybar: Vec<KeyHint>,
    pub rows_height: Cell<u16>,
    pub status_msg: Option<String>,
    wallpaper_cache: RefCell<HashMap<String, Option<RgbImage>>>,
}

impl ThemePickerScreen {
    pub fn new() -> Self {
        Self::with_design(&Design::for_session(None))
    }

    pub fn with_design(design: &Design) -> Self {
        let p = paths();
        let active = store::active(&p).unwrap_or_else(|| "solarized".to_string());
        let original_slug = active.clone();

        let state_wp = std::fs::read_to_string(p.state.join("wallpaper"))
            .ok()
            .map(|s| s.trim().to_string());

        let mut entries = Vec::new();
        let mut rows = Vec::new();
        let mut original_theme_opt = None;

        for (slug, toml) in builtin::ALL {
            if let Ok(mut theme) = Theme::parse(toml) {
                if slug == active {
                    original_theme_opt = Some(theme.clone());
                }
                // If there is an active wallpaper matching this theme, rotate it to the front
                if let Some(current_wp) = &state_wp {
                    if let Some(pos) = theme.wallpapers.iter().position(|w| w == current_wp) {
                        theme.wallpapers.rotate_left(pos);
                    }
                }

                let roles = theme.roles();
                let bg = theme.colors.background.0;
                let c_text = contrast::ratio(roles.text.0, bg);
                let c_accent = contrast::ratio(roles.accent.0, bg);
                let c_danger = contrast::ratio(roles.danger.0, bg);
                let c_warn = contrast::ratio(roles.warn.0, bg);
                let c_ok = contrast::ratio(roles.ok.0, bg);

                let is_active = slug == active;
                let mark = if is_active { "\u{25cf}" } else { "\u{25cb}" };
                let label = format!("{mark} {}", theme.name);

                let row = Row::new(label)
                    .glyph(Glyph::Theme)
                    .detail(slug)
                    .value(roles.accent.hashed());

                rows.push(row);
                entries.push(ThemeEntry {
                    slug: slug.to_string(),
                    theme,
                    contrast_text: c_text,
                    contrast_accent: c_accent,
                    contrast_danger: c_danger,
                    contrast_warn: c_warn,
                    contrast_ok: c_ok,
                });
            }
        }

        let original_theme = original_theme_opt
            .or_else(|| entries.first().map(|e| e.theme.clone()))
            .unwrap_or_else(|| Theme::parse(builtin::SOLARIZED).expect("solarized builtin"));

        let mut list = ListState::new().with_empty("no matching themes found");
        list.set_rows(rows);

        // Pre-select the currently active theme
        if let Some(pos) = entries.iter().position(|e| e.slug == active) {
            list.set_cursor(pos);
        }

        let live_design = if let Some(selected) = entries.iter().find(|e| e.slug == active) {
            design.clone().with_palette(selected.theme.palette())
        } else {
            design.clone()
        };

        let mut picker = Self {
            entries,
            list,
            active_slug: active,
            original_slug,
            original_theme,
            live_design,
            committed: false,
            peeking: false,
            header: Header::new("LazyOS").subtitle("Theme & Appearance"),
            keybar: vec![
                KeyHint::new("Enter", "apply"),
                KeyHint::new("\u{2191}\u{2193}", "preview"),
                KeyHint::new("w", "wallpaper"),
                KeyHint::new("p/Space", "peek desktop"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
            status_msg: None,
            wallpaper_cache: RefCell::new(HashMap::new()),
        };

        picker.sync_live_preview();
        picker
    }

    fn sync_live_preview(&mut self) {
        let update = self
            .selected_entry()
            .map(|e| (e.theme.palette(), e.theme.osc_sequences()));
        if let Some((palette, osc)) = update {
            self.live_design = self.live_design.clone().with_palette(palette);
            use std::io::Write;
            let mut out = std::io::stdout();
            let _ = out.write_all(osc.as_bytes());
            let _ = out.flush();
        }
    }

    fn restore_original_terminal(&mut self) {
        use std::io::Write;
        let mut out = std::io::stdout();
        let _ = out.write_all(self.original_theme.osc_sequences().as_bytes());
        let _ = out.write_all(b"\x1b]110\x1b\\\x1b]111\x1b\\\x1b]112\x1b\\\x1b]104\x1b\\");
        let _ = out.flush();
    }

    fn selected_entry(&self) -> Option<&ThemeEntry> {
        let idx = self.list.selected_index().unwrap_or(0);
        self.entries.get(idx)
    }

    fn selected_entry_mut(&mut self) -> Option<&mut ThemeEntry> {
        let idx = self.list.selected_index().unwrap_or(0);
        self.entries.get_mut(idx)
    }

    fn get_wallpaper_thumbnail(&self, filename: &str) -> Option<RgbImage> {
        if let Some(cached) = self.wallpaper_cache.borrow().get(filename) {
            return cached.clone();
        }
        let loaded = load_wallpaper_thumbnail(filename);
        self.wallpaper_cache
            .borrow_mut()
            .insert(filename.to_string(), loaded.clone());
        loaded
    }
}

impl Screen for ThemePickerScreen {
    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        Some(&self.keybar)
    }

    fn design(&self) -> Option<Design> {
        Some(self.live_design.clone())
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        if let Some(visible) = row {
            let old_idx = self.list.selected_index();
            self.list.set_cursor(visible);
            if self.list.selected_index() != old_idx {
                self.sync_live_preview();
            }
        }
        Outcome::Continue
    }

    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        let left_w = ((area.width as u32 * 34) / 100).clamp(28, 38) as u16;
        let left_rect = Rect::new(area.x, area.y, left_w, area.height);
        self.list.hit_row(left_rect, x, y, design)
    }

    fn draw(&mut self, ctx: &mut BodyCtx) {
        let area = ctx.area;
        if area.width < 50 || area.height < 15 {
            return;
        }

        // Split horizontally: ~34% left (theme list), ~66% right (preview canvas)
        let left_w = ((area.width as u32 * 34) / 100).clamp(28, 38) as u16;
        let right_w = area.width.saturating_sub(left_w);

        let left_rect = Rect::new(area.x, area.y, left_w, area.height);
        let right_rect = Rect::new(area.x + left_w, area.y, right_w, area.height);

        // -------------------------------------------------------------------
        // Left Column: Theme List & Filter
        // -------------------------------------------------------------------
        let list_card = draw_card_with(
            ctx.buf,
            left_rect,
            ctx.design,
            Some("Themes"),
            ColorRole::SurfaceAlt,
            ctx.design.style(TextStyle::Title),
        );

        let mut list_y = list_card.y;
        if !self.list.filter().is_empty() {
            let filter_text = format!("Filter: {}_", self.list.filter());
            put(
                ctx.buf,
                list_card.x,
                list_y,
                &filter_text,
                list_card.width,
                ctx.design.style(TextStyle::Code),
            );
            list_y += 1;
        }

        let rows_area = Rect::new(
            list_card.x,
            list_y,
            list_card.width,
            list_card.bottom().saturating_sub(list_y),
        );
        self.rows_height.set(rows_area.height);
        self.list.render(rows_area, ctx.buf, ctx.design, true);

        // -------------------------------------------------------------------
        // Right Column: Live Eye-Candy Preview
        // -------------------------------------------------------------------
        let Some(selected) = self.selected_entry() else {
            return;
        };

        let preview_title = format!("Preview: {}", selected.theme.name);
        let right_card = draw_card_with(
            ctx.buf,
            right_rect,
            ctx.design,
            Some(&preview_title),
            ColorRole::Accent,
            ctx.design.style(TextStyle::Title),
        );

        let mut py = right_card.y;
        let px = right_card.x + 1;
        let pw = right_card.width.saturating_sub(2);

        // 1. Theme Identity Banner & Origin
        let theme_roles = selected.theme.roles();
        let accent_color = lazyos_ui::design::color(theme_roles.accent.0);
        let bg_color = lazyos_ui::design::color(selected.theme.colors.background.0);
        let surface_color = lazyos_ui::design::color(theme_roles.surface.0);
        let text_color = lazyos_ui::design::color(theme_roles.text.0);
        let dim_color = lazyos_ui::design::color(theme_roles.dim.0);

        let name_style = Style::default()
            .fg(accent_color)
            .add_modifier(Modifier::BOLD);
        put(
            ctx.buf,
            px,
            py,
            &format!("{} ({})", selected.theme.name, selected.slug),
            pw,
            name_style,
        );

        // Status Badge
        let status_badge = if selected.slug == self.active_slug {
            "[ ACTIVE SESSION THEME ]"
        } else if self.peeking && selected.slug == self.original_slug {
            "[ ORIGINAL THEME ]"
        } else {
            "[ AVAILABLE ]"
        };
        let badge_style = if selected.slug == self.active_slug {
            Style::default()
                .fg(lazyos_ui::design::color(theme_roles.ok.0))
                .add_modifier(Modifier::BOLD)
        } else {
            ctx.design.style(TextStyle::Dim)
        };
        let badge_x = (px + pw).saturating_sub(status_badge.len() as u16);
        put(ctx.buf, badge_x, py, status_badge, pw, badge_style);
        py += 1;

        // Author & source
        let meta = if !selected.theme.author.is_empty() {
            format!("by {}", selected.theme.author)
        } else {
            "LazyOS Builtin Theme".to_string()
        };
        put(ctx.buf, px, py, &meta, pw, ctx.design.style(TextStyle::Dim));
        py += 2;

        // 2. Desktop & Wallpaper Simulation (The Eye Candy!)
        put(
            ctx.buf,
            px,
            py,
            "Desktop & Wallpaper Simulation",
            pw,
            ctx.design.style(TextStyle::Title),
        );
        py += 1;

        let sim_w = pw.min(48);

        // Mini Waybar Strip on top of the desktop
        let bar_area = Rect::new(px, py, sim_w, 1);
        fill_rect(ctx.buf, bar_area, Style::default().bg(surface_color));
        let mark_style = Style::default()
            .fg(accent_color)
            .bg(surface_color)
            .add_modifier(Modifier::BOLD);
        put(ctx.buf, px + 1, py, "\u{f013} LazyOS", 10, mark_style);

        let chip_style = Style::default().fg(text_color).bg(surface_color);
        put(
            ctx.buf,
            px + 14,
            py,
            "\u{f126} main  \u{2502}  \u{f1eb} LazyNet  \u{f028} 80%  \u{f240} 95%  \u{f017} 10:14",
            sim_w.saturating_sub(15),
            chip_style,
        );
        py += 1;

        // Truecolor Halfblock Wallpaper Preview Matrix
        let current_wp = selected
            .theme
            .wallpapers
            .first()
            .cloned()
            .unwrap_or_else(|| "lazyos.png".to_string());
        let thumb = self.get_wallpaper_thumbnail(&current_wp);
        let wp_h = 5;
        render_wallpaper_preview(
            ctx.buf,
            px,
            py,
            sim_w,
            wp_h,
            thumb.as_ref(),
            &selected.theme,
        );
        py += wp_h;

        // Wallpaper metadata and cycle hint
        let wp_count = selected.theme.wallpapers.len();
        let wp_info = if wp_count > 1 {
            format!("Wallpaper: {current_wp} ({wp_count} bundled) \u{2014} Press [w] to cycle")
        } else {
            format!("Wallpaper: {current_wp} (standard theme background)")
        };
        put(
            ctx.buf,
            px,
            py,
            &wp_info,
            pw,
            ctx.design.style(TextStyle::Dim),
        );
        py += 2;

        // 3. ANSI 16-Color Terminal Matrix
        put(
            ctx.buf,
            px,
            py,
            "Terminal Palette (16 ANSI Swatches)",
            pw,
            ctx.design.style(TextStyle::Title),
        );
        py += 1;

        let colors = selected.theme.colors;
        let regular = [
            ("0", colors.regular0),
            ("1", colors.regular1),
            ("2", colors.regular2),
            ("3", colors.regular3),
            ("4", colors.regular4),
            ("5", colors.regular5),
            ("6", colors.regular6),
            ("7", colors.regular7),
        ];
        let bright = [
            ("8", colors.bright0),
            ("9", colors.bright1),
            ("10", colors.bright2),
            ("11", colors.bright3),
            ("12", colors.bright4),
            ("13", colors.bright5),
            ("14", colors.bright6),
            ("15", colors.bright7),
        ];

        let mut cx = px;
        for (label, hex) in regular {
            let col = lazyos_ui::design::color(hex.0);
            let style = Style::default().bg(col).fg(bg_color);
            put(ctx.buf, cx, py, &format!(" {label} "), 5, style);
            cx += 5;
        }
        py += 1;

        cx = px;
        for (label, hex) in bright {
            let col = lazyos_ui::design::color(hex.0);
            let style = Style::default().bg(col).fg(bg_color);
            put(ctx.buf, cx, py, &format!(" {label} "), 5, style);
            cx += 5;
        }
        py += 2;

        // 4. Key Semantic UI Roles
        put(
            ctx.buf,
            px,
            py,
            "Semantic Roles",
            pw,
            ctx.design.style(TextStyle::Title),
        );
        py += 1;

        let role_pills = [
            ("Accent", theme_roles.accent),
            ("Surface", theme_roles.surface),
            ("Border", theme_roles.border),
            ("Ok", theme_roles.ok),
            ("Warn", theme_roles.warn),
            ("Danger", theme_roles.danger),
        ];

        cx = px;
        for (name, hex) in role_pills {
            let col = lazyos_ui::design::color(hex.0);
            let block_style = Style::default().fg(col);
            put(ctx.buf, cx, py, "\u{2588}\u{2588} ", 3, block_style);
            cx += 3;
            let text_style = ctx.design.style(TextStyle::Code);
            let label = format!("{name}: {}  ", hex.hashed());
            let len = label.len() as u16;
            put(ctx.buf, cx, py, &label, len, text_style);
            cx += len;
            if cx + 18 > px + pw {
                cx = px;
                py += 1;
            }
        }
        py += 2;

        // 5. Contrast & Accessibility Scorecard
        put(
            ctx.buf,
            px,
            py,
            "Contrast & Accessibility (WCAG 2.1)",
            pw,
            ctx.design.style(TextStyle::Title),
        );
        py += 1;

        let text_verdict = if selected.contrast_text >= 4.5 {
            "AAA PASS"
        } else {
            "FAIL"
        };
        let accent_verdict = if selected.contrast_accent >= 3.0 {
            "PASS"
        } else {
            "LOW"
        };
        let danger_verdict = if selected.contrast_danger >= 3.0 {
            "PASS"
        } else {
            "LOW"
        };

        let card_text = format!(
            "Text: {:>4.1}:1 ({})   Accent: {:>4.1}:1 ({})   Danger: {:>4.1}:1 ({})",
            selected.contrast_text,
            text_verdict,
            selected.contrast_accent,
            accent_verdict,
            selected.contrast_danger,
            danger_verdict
        );
        let score_style = if selected.contrast_text >= 4.5 && selected.contrast_accent >= 3.0 {
            Style::default().fg(lazyos_ui::design::color(theme_roles.ok.0))
        } else {
            Style::default().fg(lazyos_ui::design::color(theme_roles.warn.0))
        };
        put(ctx.buf, px, py, &card_text, pw, score_style);
        py += 2;

        // 6. Mini Notification Card (if space allows)
        if py + 4 <= right_card.bottom() {
            let notif_w = pw.min(48);
            let notif_rect = Rect::new(px, py, notif_w, 3);
            fill_rect(ctx.buf, notif_rect, Style::default().bg(surface_color));

            let border_style = Style::default().fg(accent_color);
            let top_border = format!(
                "\u{256d}\u{2500} \u{f0f3} System Notice {}\u{256e}",
                "\u{2500}".repeat(notif_w.saturating_sub(20) as usize)
            );
            let mid_border = format!(
                "\u{2502} Palette updated to {}.{}\u{2502}",
                selected.theme.name,
                " ".repeat(notif_w.saturating_sub(selected.theme.name.len() as u16 + 25) as usize)
            );
            let bot_border = format!(
                "\u{2570}{}\u{256f}",
                "\u{2500}".repeat(notif_w.saturating_sub(2) as usize)
            );

            put(
                ctx.buf,
                px,
                py,
                &top_border,
                notif_w,
                border_style.add_modifier(Modifier::BOLD),
            );
            put(
                ctx.buf,
                px,
                py + 1,
                &mid_border,
                notif_w,
                Style::default().fg(text_color).bg(surface_color),
            );
            put(ctx.buf, px, py + 2, &bot_border, notif_w, border_style);
        }

        // 7. Interactive Toast / Notification Message Banner
        if let Some(msg) = &self.status_msg {
            let toast_y = right_card.bottom().saturating_sub(2);
            let toast_area = Rect::new(px, toast_y, pw, 1);
            let toast_style = Style::default()
                .fg(lazyos_ui::design::color(theme_roles.accent.0))
                .bg(surface_color)
                .add_modifier(Modifier::BOLD);
            fill_rect(ctx.buf, toast_area, toast_style);
            put(
                ctx.buf,
                px + 2,
                toast_y,
                &format!("\u{2139} {msg}"),
                pw.saturating_sub(4),
                toast_style,
            );
        }
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }

        let p = paths();

        match key.code {
            // Cancel / Exit
            KeyCode::Esc | KeyCode::Char('q') => {
                // Restore original terminal palette
                self.restore_original_terminal();
                // If we were peeking, restore the original theme before exiting!
                if self.peeking {
                    let _ = apply_theme(&p, &self.original_slug, true, false);
                }
                Outcome::Done(lazyos_ui::app::exit::ESCAPE)
            }

            // Apply selected theme
            KeyCode::Enter => {
                if let Some(selected) = self.selected_entry() {
                    let slug = selected.slug.clone();
                    match apply_theme(&p, &slug, true, false) {
                        Ok(_) => {
                            self.active_slug = slug.clone();
                            self.committed = true;
                            self.peeking = false;
                            // Clear dynamic OSC overrides so terminal follows new foot.ini
                            use std::io::Write;
                            let mut out = std::io::stdout();
                            let _ = out.write_all(
                                b"\x1b]110\x1b\\\x1b]111\x1b\\\x1b]112\x1b\\\x1b]104\x1b\\",
                            );
                            let _ = out.flush();
                            Outcome::Done(lazyos_ui::app::exit::OK)
                        }
                        Err(e) => {
                            self.status_msg = Some(format!("Error applying theme: {e}"));
                            Outcome::Consumed
                        }
                    }
                } else {
                    Outcome::Continue
                }
            }

            // Wallpaper cycle for selected theme
            KeyCode::Char('w') => {
                if let Some(selected) = self.selected_entry() {
                    let slug = selected.slug.clone();
                    match cycle_wallpaper_for(&p, Some(&slug)) {
                        Ok((wp, _)) => {
                            if let Some(entry) = self.selected_entry_mut() {
                                if let Some(pos) =
                                    entry.theme.wallpapers.iter().position(|w| *w == wp)
                                {
                                    entry.theme.wallpapers.rotate_left(pos);
                                }
                            }
                            self.status_msg = Some(format!("Cycled wallpaper for {slug}: {wp}"));
                        }
                        Err(err) => {
                            self.status_msg = Some(err);
                        }
                    }
                }
                Outcome::Consumed
            }

            // Peek live desktop mode
            KeyCode::Char('p') | KeyCode::Char(' ') => {
                let Some((target_slug, theme_name)) = self
                    .selected_entry()
                    .map(|s| (s.slug.clone(), s.theme.name.clone()))
                else {
                    return Outcome::Continue;
                };

                if self.peeking && target_slug == self.original_slug {
                    // Already back to original
                    self.peeking = false;
                    self.status_msg =
                        Some("Desktop peeking stopped. Active theme intact.".to_string());
                } else if self.peeking {
                    // Switch peek to this new theme
                    let _ = apply_theme(&p, &target_slug, true, false);
                    self.status_msg = Some(format!(
                        "Peeking desktop '{theme_name}'! Press Enter to keep, Esc to revert."
                    ));
                } else {
                    // Start peeking
                    let _ = apply_theme(&p, &target_slug, true, false);
                    self.peeking = true;
                    self.status_msg = Some(format!(
                        "Peeking desktop '{theme_name}'! Press Enter to keep, Esc to revert."
                    ));
                }
                Outcome::Consumed
            }

            // Navigation
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::Char('j')
            | KeyCode::Char('k') => {
                let old_idx = self.list.selected_index();
                let action = self.list.handle(key, self.rows_height.get(), design);
                if action == ListAction::Moved || self.list.selected_index() != old_idx {
                    self.sync_live_preview();
                    if self.peeking {
                        // Update live desktop peek to the newly highlighted theme!
                        if let Some((target_slug, theme_name)) = self
                            .selected_entry()
                            .map(|s| (s.slug.clone(), s.theme.name.clone()))
                        {
                            let _ = apply_theme(&p, &target_slug, true, false);
                            self.status_msg = Some(format!(
                                "Peeking desktop '{theme_name}'! Press Enter to keep, Esc to revert."
                            ));
                        }
                    }
                }
                Outcome::Consumed
            }

            // Real-time case-folded filtering as you type
            KeyCode::Backspace | KeyCode::Char(_) => {
                let old_idx = self.list.selected_index();
                if self.list.type_filter(key) {
                    if self.list.selected_index() != old_idx {
                        self.sync_live_preview();
                    }
                    Outcome::Consumed
                } else {
                    Outcome::Continue
                }
            }

            _ => Outcome::Continue,
        }
    }
}

impl Drop for ThemePickerScreen {
    fn drop(&mut self) {
        use std::io::Write;
        let mut out = std::io::stdout();
        if !self.committed {
            let _ = out.write_all(self.original_theme.osc_sequences().as_bytes());
        }
        let _ = out.write_all(b"\x1b]110\x1b\\\x1b]111\x1b\\\x1b]112\x1b\\\x1b]104\x1b\\");
        let _ = out.flush();
    }
}

/// Headless rows output for scripts and tests (`lazyos menu theme --rows`).
pub fn rows() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("solarized", "Solarized Dark", "built-in Ethan Schoonover"),
        ("catppuccin", "Catppuccin Mocha", "built-in Catppuccin Org"),
        ("tokyo-night", "Tokyo Night", "built-in enkia / Folke"),
        ("rose-pine", "Rose Pine", "built-in Rosé Pine"),
        ("nord", "Nord", "built-in Arctic Ice Studio"),
        ("gruvbox", "Gruvbox Dark", "built-in morhetz"),
        ("everforest", "Everforest Dark", "built-in sainnhe"),
        ("kanagawa", "Kanagawa", "built-in rebelot"),
        ("dracula", "Dracula", "built-in Zeno Rocha"),
        (
            "catppuccin-latte",
            "Catppuccin Latte",
            "built-in Catppuccin Org",
        ),
        ("hackerman", "Hackerman", "built-in Omarchy"),
        ("lumon", "Lumon", "built-in Omarchy"),
        ("matte-black", "Matte Black", "built-in Omarchy"),
        ("omacon", "Omacon", "built-in Omarchy"),
        ("flexoki-light", "Flexoki Light", "built-in Steph Ango"),
        ("one-dark", "One Dark", "built-in Atom / GitHub"),
    ]
}

/// Print list for CLI queries (`lazyos menu theme --list`).
pub fn print_list() {
    let p = paths();
    let active = store::active(&p).unwrap_or_else(|| "solarized".to_string());
    for (slug, toml) in builtin::ALL {
        let name = Theme::parse(toml)
            .map(|t| t.name)
            .unwrap_or_else(|_| slug.to_string());
        let mark = if slug == active { "*" } else { " " };
        println!("{mark} {slug:<12} {name}");
    }
}

/// Print current theme (`lazyos menu theme --current`).
pub fn print_current() {
    let p = paths();
    let active = store::active(&p).unwrap_or_else(|| "solarized".to_string());
    let name = Theme::parse(builtin::get(&active).unwrap_or(builtin::SOLARIZED))
        .map(|t| t.name)
        .unwrap_or_else(|_| active.clone());
    println!("{active}\t{name}");
}

/// Launch the interactive TUI theme picker.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    design: &Design,
) -> Result<i32, Failure> {
    let mut screen = ThemePickerScreen::with_design(design);
    let mut app = lazyos_ui::App::new(Header::new("LazyOS").subtitle("Theme & Appearance"))
        .design(design.clone())
        .screen(&mut screen);
    app.run(terminal)
        .map_err(|e| Failure::new("theme picker encountered an error", e.to_string()))
}
