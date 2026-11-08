#![allow(unknown_lints)]
#![allow(clippy::chunks_exact_to_as_chunks)]

use tiny_skia::{Color, FillRule, Paint, PathBuilder, PixmapMut, Stroke, Transform};

use crate::font::FontRenderer;
use crate::model::{Notification, Rect, Urgency};

pub const OSD_WIDTH: f32 = 280.0;
pub const OSD_HEIGHT: f32 = 44.0;
pub const CARD_WIDTH: f32 = 340.0;
pub const CARD_SPACING: f32 = 8.0;
pub const CORNER_RADIUS: f32 = 6.0;

fn with_alpha(c: Color, alpha: f32) -> Color {
    let a = (c.alpha() * alpha).clamp(0.0, 1.0);
    Color::from_rgba(c.red(), c.green(), c.blue(), a).unwrap_or(c)
}

pub struct CardTheme {
    pub bg: Color,
    #[allow(dead_code)]
    pub bg_elevated: Color,
    pub border_subtle: Color,
    pub text_bright: Color,
    pub text_body: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub danger: Color,
    #[allow(dead_code)]
    pub warn: Color,
    #[allow(dead_code)]
    pub ok: Color,
    pub track_bg: Color,
    pub btn_bg: Color,
    pub btn_bg_hover: Color,
}

impl Default for CardTheme {
    fn default() -> Self {
        Self {
            bg: Color::from_rgba8(7, 54, 66, 240), // surface: #073642
            bg_elevated: Color::from_rgba8(9, 64, 76, 248),
            border_subtle: Color::from_rgba8(101, 123, 131, 220), // dim: #657b83
            text_bright: Color::from_rgba8(42, 161, 152, 255),    // accent: #2aa198
            text_body: Color::from_rgba8(253, 246, 227, 235),     // strong: #fdf6e3
            text_dim: Color::from_rgba8(101, 123, 131, 255),
            accent: Color::from_rgba8(42, 161, 152, 255),
            danger: Color::from_rgba8(220, 50, 47, 255),
            warn: Color::from_rgba8(181, 137, 0, 255),
            ok: Color::from_rgba8(133, 153, 0, 255),
            track_bg: Color::from_rgba8(9, 64, 76, 160),
            btn_bg: Color::from_rgba8(9, 64, 76, 204),
            btn_bg_hover: Color::from_rgba8(9, 64, 76, 245),
        }
    }
}

impl CardTheme {
    pub fn from_active() -> Self {
        let paths = lazyos_theme::Paths::from_env();
        if let Some(slug) = lazyos_theme::store::active(&paths) {
            if let Some(text) = lazyos_theme::builtin::get(&slug) {
                if let Ok(theme) = lazyos_theme::Theme::parse(text) {
                    return Self::from_theme(&theme);
                }
            }
        }
        if let Ok(theme) = lazyos_theme::Theme::parse(lazyos_theme::builtin::SOLARIZED) {
            return Self::from_theme(&theme);
        }
        Self::default()
    }

    pub fn from_theme(theme: &lazyos_theme::Theme) -> Self {
        let r = theme.roles();
        let to_color =
            |hex: lazyos_theme::Hex, a: u8| Color::from_rgba8(hex.0.r, hex.0.g, hex.0.b, a);

        Self {
            // Swaybar hover box (tooltip) style:
            // background: {{ surface }}, border: 1px solid {{ dim }}, border-radius: 6px
            bg: to_color(r.surface, 240),
            bg_elevated: to_color(r.hover, 248),
            border_subtle: to_color(r.dim, 220),
            // Font color matches the penguin on the bar: accent
            text_bright: to_color(r.accent, 255),
            // Content matches tooltip label: strong text
            text_body: to_color(r.strong, 235),
            text_dim: to_color(r.dim, 255),
            accent: to_color(r.accent, 255),
            danger: to_color(r.danger, 255),
            warn: to_color(r.warn, 255),
            ok: to_color(r.ok, 255),
            track_bg: to_color(r.hover, 160),
            btn_bg: to_color(r.hover, 204),
            btn_bg_hover: to_color(r.hover, 245),
        }
    }
}

pub struct LayoutMetrics {
    pub total_width: f32,
    pub total_height: f32,
}

pub struct Renderer {
    pub font: FontRenderer,
    pub theme: CardTheme,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            font: FontRenderer::new(),
            theme: CardTheme::from_active(),
        }
    }

    pub fn add_rounded_rect(pb: &mut PathBuilder, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let r = r.min(w * 0.5).min(h * 0.5);
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.line_to(x + w, y + h - r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + h - r);
        pb.line_to(x, y + r);
        pb.quad_to(x, y, x + r, y);
        pb.close();
    }

    #[allow(dead_code)]
    pub fn add_circle(pb: &mut PathBuilder, cx: f32, cy: f32, r: f32) {
        Self::add_rounded_rect(pb, cx - r, cy - r, r * 2.0, r * 2.0, r);
    }

    pub fn compute_layout(&self, notifications: &mut [Notification]) -> LayoutMetrics {
        let mut max_w = 0.0f32;
        for notif in notifications.iter() {
            let w = if notif.is_osd() {
                OSD_WIDTH
            } else {
                CARD_WIDTH
            };
            if w > max_w {
                max_w = w;
            }
        }
        if max_w == 0.0 {
            max_w = CARD_WIDTH;
        }

        let mut cur_y = 0.0f32;
        for notif in notifications.iter_mut() {
            let card_w = if notif.is_osd() {
                OSD_WIDTH
            } else {
                CARD_WIDTH
            };
            let card_x = max_w - card_w;

            if notif.is_osd() {
                let height = OSD_HEIGHT;
                notif.card_y = cur_y;
                notif.card_height = height;
                notif.card_bounds = Rect::new(card_x, cur_y, card_w, height);
                cur_y += height + CARD_SPACING;
                continue;
            }

            let text_max_width = card_w - 36.0 - 24.0;
            let body_lines = if notif.body.is_empty() {
                Vec::new()
            } else {
                self.font
                    .wrap_text(&notif.body, 12.0, text_max_width, 4, false)
            };

            let mut height = if body_lines.is_empty() {
                OSD_HEIGHT
            } else {
                20.0 + 6.0 + (body_lines.len() as f32 * 16.0) + 12.0
            };
            if notif.progress.is_some() {
                height += 16.0;
            }
            if !notif.actions.is_empty() {
                height += 30.0;
            }
            height = height.max(OSD_HEIGHT);

            notif.card_y = cur_y;
            notif.card_height = height;
            notif.card_bounds = Rect::new(card_x, cur_y, card_w, height);

            cur_y += height + CARD_SPACING;
        }

        LayoutMetrics {
            total_width: max_w,
            total_height: cur_y.max(1.0),
        }
    }

    pub fn render(&self, pixmap: &mut PixmapMut, notifications: &mut [Notification]) {
        pixmap.fill(Color::TRANSPARENT);

        for notif in notifications.iter_mut() {
            self.render_card(pixmap, notif);
        }
    }

    pub fn resolve_glyph(icon: &str, app: &str, summary: &str, urgency: Urgency) -> &'static str {
        let icon_lower = icon.to_lowercase();
        let app_lower = app.to_lowercase();
        let summary_lower = summary.to_lowercase();

        if icon_lower.contains("muted") || summary_lower.contains("muted") {
            "\u{ea67}"
        } else if icon_lower.contains("volume")
            || app_lower.contains("volume")
            || summary_lower.contains("volume")
        {
            "\u{eb75}"
        } else if icon_lower.contains("brightness")
            || app_lower.contains("brightness")
            || summary_lower.contains("brightness")
        {
            "\u{f522}"
        } else if icon_lower.contains("wifi")
            || icon_lower.contains("network")
            || app_lower.contains("wifi")
            || summary_lower.contains("wifi")
        {
            "\u{eb76}"
        } else if icon_lower.contains("bluetooth")
            || app_lower.contains("bluetooth")
            || summary_lower.contains("bluetooth")
        {
            "\u{eb77}"
        } else if icon_lower.contains("battery")
            || app_lower.contains("battery")
            || summary_lower.contains("battery")
            || summary_lower.contains("power")
        {
            "\u{eb5c}"
        } else if icon_lower.contains("camera")
            || app_lower.contains("camera")
            || summary_lower.contains("capture")
            || summary_lower.contains("screenshot")
        {
            "\u{f4e5}"
        } else if icon_lower.contains("download") || app_lower.contains("download") {
            "\u{eb79}"
        } else if icon_lower.contains("terminal")
            || app_lower.contains("terminal")
            || app_lower.contains("foot")
        {
            "\u{ea85}"
        } else if icon_lower.contains("music")
            || icon_lower.contains("media")
            || app_lower.contains("music")
            || app_lower.contains("player")
        {
            "\u{ec1b}"
        } else if icon_lower.contains("settings") || app_lower.contains("settings") {
            "\u{ea82}"
        } else if urgency == Urgency::Critical
            || icon_lower.contains("error")
            || summary_lower.contains("error")
        {
            "\u{ea6c}"
        } else {
            "\u{ea74}" // info
        }
    }

    fn render_osd_card(&self, pixmap: &mut PixmapMut, notif: &mut Notification) {
        let card_x = notif.card_bounds.x;
        let card_y = notif.card_bounds.y;
        let card_w = notif.card_bounds.w;
        let card_h = notif.card_bounds.h;

        notif.close_rect = None;

        let (scale, alpha) = notif.animation_factors();
        let cx = card_x + card_w * 0.5;
        let cy = card_y + card_h * 0.5;
        let transform = Transform::from_translate(-cx, -cy)
            .post_scale(scale, scale)
            .post_translate(cx, cy);

        let map_x = |x: f32| cx + (x - cx) * scale;
        let map_y = |y: f32| cy + (y - cy) * scale;

        // 1. Background (matching Waybar #custom-menu button: rgba(surface, 0.80))
        let mut bg_builder = PathBuilder::new();
        Self::add_rounded_rect(
            &mut bg_builder,
            card_x,
            card_y,
            card_w,
            card_h,
            CORNER_RADIUS,
        );
        if let Some(bg_path) = bg_builder.finish() {
            let mut paint = Paint::default();
            paint.set_color(with_alpha(self.theme.bg, alpha));
            paint.anti_alias = true;
            pixmap.fill_path(&bg_path, &paint, FillRule::Winding, transform, None);
        }

        // 2. Button border matching Waybar #custom-menu (1px solid rgba(hover, 0.90))
        let mut border_builder = PathBuilder::new();
        Self::add_rounded_rect(
            &mut border_builder,
            card_x + 0.5,
            card_y + 0.5,
            card_w - 1.0,
            card_h - 1.0,
            CORNER_RADIUS,
        );
        if let Some(border_path) = border_builder.finish() {
            let stroke = Stroke {
                width: 1.0,
                ..Default::default()
            };
            let mut paint = Paint::default();
            paint.set_color(with_alpha(self.theme.border_subtle, alpha));
            paint.anti_alias = true;
            pixmap.stroke_path(&border_path, &paint, &stroke, transform, None);
        }

        // 3. Status checks
        let is_muted = notif.body.to_lowercase().contains("mute")
            || notif.app_icon.contains("muted")
            || notif.summary.to_lowercase().contains("mute");

        let is_brightness = notif.summary.to_lowercase().contains("brightness")
            || notif.app_icon.contains("brightness");

        // 4. Vector glyph in penguin accent color
        let icon_glyph = if is_brightness {
            "\u{f522}" // brightness
        } else if is_muted {
            "\u{ea67}" // mute
        } else {
            "\u{eb75}" // volume
        };

        let icon_color = if is_muted {
            self.theme.danger
        } else {
            self.theme.accent
        };

        self.font.draw_text(
            pixmap,
            icon_glyph,
            map_x(card_x + 12.0),
            map_y(card_y + 26.0),
            16.0 * scale,
            with_alpha(icon_color, alpha),
            false,
        );

        // 5. Title & Value in console font (JetBrains Mono) and penguin accent color
        let content_x = card_x + 36.0;
        let right_margin = card_x + card_w - 14.0;

        self.font.draw_text(
            pixmap,
            &notif.summary,
            map_x(content_x),
            map_y(card_y + 17.0),
            12.0 * scale,
            with_alpha(self.theme.text_bright, alpha),
            false,
        );

        let effective_pct = if let Some(pct) = notif.progress {
            pct.min(100)
        } else if let Some(n) = notif
            .body
            .trim()
            .strip_suffix('%')
            .and_then(|s| s.trim().parse::<u16>().ok())
        {
            (n.min(100)) as u8
        } else {
            0
        };

        let val_str = if is_muted {
            "MUTED".to_string()
        } else if notif.progress.is_some() || notif.body.trim().ends_with('%') {
            format!("{effective_pct}%")
        } else if !notif.body.is_empty() {
            notif.body.clone()
        } else {
            "0%".to_string()
        };

        let val_w = self.font.measure_width(&val_str, 12.0 * scale, false);
        let val_color = if is_muted {
            self.theme.danger
        } else {
            self.theme.accent
        };

        self.font.draw_text(
            pixmap,
            &val_str,
            map_x(right_margin) - val_w,
            map_y(card_y + 17.0),
            12.0 * scale,
            with_alpha(val_color, alpha),
            false,
        );

        // 6. Sleek level pill track
        let track_x = content_x;
        let track_y = card_y + 27.0;
        let track_w = right_margin - content_x;
        let track_h = 4.0f32;
        let track_r = 2.0f32;

        let mut track_builder = PathBuilder::new();
        Self::add_rounded_rect(
            &mut track_builder,
            track_x,
            track_y,
            track_w,
            track_h,
            track_r,
        );
        if let Some(track_path) = track_builder.finish() {
            let mut paint = Paint::default();
            paint.set_color(with_alpha(self.theme.track_bg, alpha));
            paint.anti_alias = true;
            pixmap.fill_path(&track_path, &paint, FillRule::Winding, transform, None);
        }

        let fill_pct = if is_muted {
            0.0
        } else {
            (effective_pct as f32 / 100.0).clamp(0.0, 1.0)
        };
        let fill_w = if is_muted {
            0.0
        } else {
            (track_w * fill_pct).max(3.0)
        };

        if fill_w > 0.0 {
            let mut fill_builder = PathBuilder::new();
            Self::add_rounded_rect(
                &mut fill_builder,
                track_x,
                track_y,
                fill_w,
                track_h,
                track_r,
            );
            if let Some(fill_path) = fill_builder.finish() {
                let mut paint = Paint::default();
                paint.set_color(with_alpha(icon_color, alpha));
                paint.anti_alias = true;
                pixmap.fill_path(&fill_path, &paint, FillRule::Winding, transform, None);
            }
        }
    }

    fn render_card(&self, pixmap: &mut PixmapMut, notif: &mut Notification) {
        if notif.is_osd() {
            self.render_osd_card(pixmap, notif);
            return;
        }

        let card_x = notif.card_bounds.x;
        let card_y = notif.card_bounds.y;
        let card_w = notif.card_bounds.w;
        let card_h = notif.card_bounds.h;

        let (scale, alpha) = notif.animation_factors();
        let cx = card_x + card_w * 0.5;
        let cy = card_y + card_h * 0.5;
        let transform = Transform::from_translate(-cx, -cy)
            .post_scale(scale, scale)
            .post_translate(cx, cy);

        let map_x = |x: f32| cx + (x - cx) * scale;
        let map_y = |y: f32| cy + (y - cy) * scale;

        // 1. Background (matching Waybar #custom-menu button: rgba(surface, 0.80))
        let mut bg_builder = PathBuilder::new();
        Self::add_rounded_rect(
            &mut bg_builder,
            card_x,
            card_y,
            card_w,
            card_h,
            CORNER_RADIUS,
        );
        if let Some(bg_path) = bg_builder.finish() {
            let mut paint = Paint::default();
            let bg_color = if notif.is_hovered {
                self.theme.bg_elevated
            } else {
                self.theme.bg
            };
            paint.set_color(with_alpha(bg_color, alpha));
            paint.anti_alias = true;
            pixmap.fill_path(&bg_path, &paint, FillRule::Winding, transform, None);
        }

        // 2. Button border matching Waybar #custom-menu (1px solid rgba(hover, 0.90))
        let mut border_builder = PathBuilder::new();
        Self::add_rounded_rect(
            &mut border_builder,
            card_x + 0.5,
            card_y + 0.5,
            card_w - 1.0,
            card_h - 1.0,
            CORNER_RADIUS,
        );
        if let Some(border_path) = border_builder.finish() {
            let stroke = Stroke {
                width: 1.0,
                ..Default::default()
            };
            let mut paint = Paint::default();
            paint.set_color(with_alpha(self.theme.border_subtle, alpha));
            paint.anti_alias = true;
            pixmap.stroke_path(&border_path, &paint, &stroke, transform, None);
        }

        // 3. Vector glyph directly (like the penguin/icons on Waybar)
        let glyph = Self::resolve_glyph(
            &notif.app_icon,
            &notif.app_name,
            &notif.summary,
            notif.urgency,
        );
        let glyph_color = match notif.urgency {
            Urgency::Low => self.theme.text_dim,
            Urgency::Normal => self.theme.accent,
            Urgency::Critical => self.theme.danger,
        };

        let is_single_line = notif.body.is_empty();
        let icon_y = if is_single_line {
            card_y + 26.0
        } else {
            card_y + 23.0
        };

        self.font.draw_text(
            pixmap,
            glyph,
            map_x(card_x + 12.0),
            map_y(icon_y),
            16.0 * scale,
            with_alpha(glyph_color, alpha),
            false,
        );

        // 4. Close '×' button
        let close_x = card_x + card_w - 20.0;
        let close_y = if is_single_line {
            card_y + 26.0
        } else {
            card_y + 19.0
        };
        let close_rect = Rect::new(
            map_x(close_x - 4.0),
            map_y(close_y - 12.0),
            18.0 * scale,
            18.0 * scale,
        );
        notif.close_rect = Some(close_rect);

        let close_color = if notif.is_hovered {
            self.theme.text_bright
        } else {
            self.theme.text_dim
        };
        self.font.draw_text(
            pixmap,
            "\u{ea76}", // Codicons close
            map_x(close_x),
            map_y(close_y),
            12.0 * scale,
            with_alpha(close_color, alpha),
            false,
        );

        // 5. Content (Dropped LazyOS / app header line as requested)
        let content_x = card_x + 36.0;
        let right_limit = card_x + card_w - 24.0;
        let text_max_width = right_limit - content_x;

        let mut cur_y = if is_single_line {
            self.font.draw_text(
                pixmap,
                &notif.summary,
                map_x(content_x),
                map_y(card_y + 26.0),
                12.0 * scale,
                with_alpha(self.theme.text_bright, alpha),
                false,
            );
            card_y + 26.0
        } else {
            let mut y = card_y + 20.0;
            self.font.draw_text(
                pixmap,
                &notif.summary,
                map_x(content_x),
                map_y(y),
                12.0 * scale,
                with_alpha(self.theme.text_bright, alpha),
                false,
            );

            y += 4.0;
            let body_lines = self
                .font
                .wrap_text(&notif.body, 12.0, text_max_width, 4, false);
            for line in &body_lines {
                y += 16.0;
                self.font.draw_text(
                    pixmap,
                    line,
                    map_x(content_x),
                    map_y(y),
                    12.0 * scale,
                    with_alpha(self.theme.text_body, alpha),
                    false,
                );
            }
            y
        };

        // 6. Progress Bar (if present)
        if let Some(pct) = notif.progress {
            let bar_y = if is_single_line {
                card_y + 32.0
            } else {
                cur_y + 8.0
            };
            let bar_x = content_x;
            let bar_w = text_max_width;
            let bar_h = 4.0f32;

            let mut track_builder = PathBuilder::new();
            Self::add_rounded_rect(&mut track_builder, bar_x, bar_y, bar_w, bar_h, 2.0);
            if let Some(track_path) = track_builder.finish() {
                let mut paint = Paint::default();
                paint.set_color(with_alpha(self.theme.track_bg, alpha));
                paint.anti_alias = true;
                pixmap.fill_path(&track_path, &paint, FillRule::Winding, transform, None);
            }

            let fill_pct = (pct as f32 / 100.0).clamp(0.0, 1.0);
            let fill_w = (bar_w * fill_pct).max(3.0);
            let fill_color = if let Some([r, g, b, a]) = notif.progress_color_override {
                Color::from_rgba8(r, g, b, a)
            } else {
                glyph_color
            };

            let mut fill_builder = PathBuilder::new();
            Self::add_rounded_rect(&mut fill_builder, bar_x, bar_y, fill_w, bar_h, 2.0);
            if let Some(fill_path) = fill_builder.finish() {
                let mut paint = Paint::default();
                paint.set_color(with_alpha(fill_color, alpha));
                paint.anti_alias = true;
                pixmap.fill_path(&fill_path, &paint, FillRule::Winding, transform, None);
            }
        }

        // 7. Action buttons
        notif.action_rects.clear();
        if !notif.actions.is_empty() {
            cur_y += 6.0;
            let btn_h = 22.0f32;
            let mut btn_x = card_x + card_w - 14.0;

            for action in notif.actions.iter().rev() {
                let unscaled_label_w = self.font.measure_width(&action.label, 10.5, true);
                let btn_w = unscaled_label_w + 16.0;
                btn_x -= btn_w;

                let rect = Rect::new(map_x(btn_x), map_y(cur_y), btn_w * scale, btn_h * scale);
                notif.action_rects.push((action.key.clone(), rect));

                let is_btn_hovered = notif.hovered_action.as_deref() == Some(&action.key);

                let mut btn_builder = PathBuilder::new();
                Self::add_rounded_rect(&mut btn_builder, btn_x, cur_y, btn_w, btn_h, 4.0);
                if let Some(btn_path) = btn_builder.finish() {
                    let mut paint = Paint::default();
                    let btn_bg = if is_btn_hovered {
                        self.theme.btn_bg_hover
                    } else {
                        self.theme.btn_bg
                    };
                    paint.set_color(with_alpha(btn_bg, alpha));
                    paint.anti_alias = true;
                    pixmap.fill_path(&btn_path, &paint, FillRule::Winding, transform, None);

                    let stroke = Stroke {
                        width: 1.0,
                        ..Default::default()
                    };
                    let mut bpaint = Paint::default();
                    let btn_border = if is_btn_hovered {
                        self.theme.accent
                    } else {
                        self.theme.border_subtle
                    };
                    bpaint.set_color(with_alpha(btn_border, alpha));
                    bpaint.anti_alias = true;
                    pixmap.stroke_path(&btn_path, &bpaint, &stroke, transform, None);
                }

                let label_color = if is_btn_hovered {
                    self.theme.text_bright
                } else {
                    self.theme.text_body
                };

                self.font.draw_text(
                    pixmap,
                    &action.label,
                    map_x(btn_x + 8.0),
                    map_y(cur_y + 15.0),
                    10.5 * scale,
                    with_alpha(label_color, alpha),
                    true,
                );

                btn_x -= 6.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActionItem, AnimState};
    use std::time::{Duration, Instant};

    #[test]
    fn test_compute_layout_and_render() {
        let renderer = Renderer::new();
        let mut notifications = vec![Notification {
            id: 1,
            app_name: "lazyos".to_string(),
            summary: "WiFi Connected".to_string(),
            body: "Successfully connected to LazyOS-5G\nIP: 192.168.1.50".to_string(),
            app_icon: "network-wireless".to_string(),
            actions: vec![
                ActionItem {
                    key: "disconnect".to_string(),
                    label: "Disconnect".to_string(),
                },
                ActionItem {
                    key: "settings".to_string(),
                    label: "Settings".to_string(),
                },
            ],
            urgency: Urgency::Normal,
            category: Some("network".to_string()),
            progress: Some(85),
            sync_tag: None,
            created_at: Instant::now(),
            timeout: Duration::from_secs(5),
            anim_state: AnimState::Active,
            card_y: 0.0,
            card_height: 0.0,
            card_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            action_rects: Vec::new(),
            close_rect: None,
            is_hovered: false,
            hovered_action: None,
            dismissing: false,
            progress_color_override: None,
        }];

        let metrics = renderer.compute_layout(&mut notifications);
        assert!(metrics.total_width >= 300.0);
        assert!(metrics.total_height > 50.0);
        assert!(notifications[0].card_height > 50.0);

        let mut data =
            vec![
                0u8;
                (metrics.total_width.ceil() as usize) * (metrics.total_height.ceil() as usize) * 4
            ];
        let mut pixmap = PixmapMut::from_bytes(
            &mut data,
            metrics.total_width.ceil() as u32,
            metrics.total_height.ceil() as u32,
        )
        .expect("pixmap allocation");

        renderer.render(&mut pixmap, &mut notifications);
    }

    #[test]
    fn test_urgency_helpers() {
        assert_eq!(Urgency::from_u8(0), Urgency::Low);
        assert_eq!(Urgency::from_u8(1), Urgency::Normal);
        assert_eq!(Urgency::from_u8(2), Urgency::Critical);
        assert_eq!(Urgency::from_u8(99), Urgency::Normal);
        assert_eq!(Urgency::Low.to_str(), "low");
        assert_eq!(Urgency::Normal.to_str(), "normal");
        assert_eq!(Urgency::Critical.to_str(), "critical");
    }

    #[test]
    fn test_compute_layout_and_render_osd() {
        let renderer = Renderer::new();
        let mut notifications = vec![Notification {
            id: 2,
            app_name: "LazyOS".to_string(),
            summary: "Volume".to_string(),
            body: "50%".to_string(),
            app_icon: "audio-volume-high".to_string(),
            actions: Vec::new(),
            urgency: Urgency::Low,
            category: None,
            progress: Some(50),
            sync_tag: Some("lazyos-osd".to_string()),
            created_at: Instant::now(),
            timeout: Duration::from_millis(1200),
            anim_state: AnimState::Active,
            card_y: 0.0,
            card_height: 0.0,
            card_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            action_rects: Vec::new(),
            close_rect: None,
            is_hovered: false,
            hovered_action: None,
            dismissing: false,
            progress_color_override: None,
        }];

        let metrics = renderer.compute_layout(&mut notifications);
        assert_eq!(notifications[0].card_height, 44.0);
        assert!(notifications[0].is_osd());

        let mut data =
            vec![
                0u8;
                (metrics.total_width.ceil() as usize) * (metrics.total_height.ceil() as usize) * 4
            ];
        let mut pixmap = PixmapMut::from_bytes(
            &mut data,
            metrics.total_width.ceil() as u32,
            metrics.total_height.ceil() as u32,
        )
        .expect("pixmap allocation");

        renderer.render(&mut pixmap, &mut notifications);
    }
}
