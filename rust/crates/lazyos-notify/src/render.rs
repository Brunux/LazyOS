#![allow(unknown_lints)]
#![allow(clippy::chunks_exact_to_as_chunks)]

use std::path::{Path, PathBuf};

use tiny_skia::{
    Color, FillRule, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Stroke, Transform,
};

use crate::font::FontRenderer;
use crate::model::{Notification, Rect, Urgency};

pub const CARD_WIDTH: f32 = 380.0;
pub const CORNER_RADIUS: f32 = 12.0;
pub const CARD_SPACING: f32 = 10.0;
pub const PADDING_X: f32 = 16.0;
pub const PADDING_TOP: f32 = 14.0;
pub const PADDING_BOTTOM: f32 = 14.0;

pub struct CardTheme {
    pub bg: Color,
    pub bg_elevated: Color,
    pub border_low: Color,
    pub border_normal: Color,
    pub border_critical: Color,
    pub text_bright: Color,
    pub text_body: Color,
    pub text_dim: Color,
    pub accent: Color,
}

impl Default for CardTheme {
    fn default() -> Self {
        Self {
            bg: Color::from_rgba8(0, 43, 54, 230),
            bg_elevated: Color::from_rgba8(7, 54, 66, 240),
            border_low: Color::from_rgba8(88, 110, 117, 160),
            border_normal: Color::from_rgba8(42, 161, 152, 220),
            border_critical: Color::from_rgba8(220, 50, 47, 240),
            text_bright: Color::from_rgba8(253, 246, 227, 255),
            text_body: Color::from_rgba8(147, 161, 161, 255),
            text_dim: Color::from_rgba8(101, 123, 131, 255),
            accent: Color::from_rgba8(42, 161, 152, 255),
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
            theme: CardTheme::default(),
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

    pub fn add_circle(pb: &mut PathBuilder, cx: f32, cy: f32, r: f32) {
        Self::add_rounded_rect(pb, cx - r, cy - r, r * 2.0, r * 2.0, r);
    }

    pub fn compute_layout(&self, notifications: &mut [Notification]) -> LayoutMetrics {
        let mut cur_y = 12.0f32;
        let text_max_width = CARD_WIDTH - PADDING_X * 2.0 - 44.0; // 44px for icon

        for notif in notifications.iter_mut() {
            let body_lines = if notif.body.is_empty() {
                Vec::new()
            } else {
                self.font
                    .wrap_text(&notif.body, 12.0, text_max_width, 4, false)
            };

            // Estimate card height:
            // Top padding: 14px
            // App name + dot: 14px
            // Summary: 18px
            // Body lines: lines.len() * 16px
            // Progress bar (if any): 22px
            // Actions (if any): 34px
            // Bottom padding: 14px
            let mut height = PADDING_TOP + 14.0 + 18.0;
            if !body_lines.is_empty() {
                height += body_lines.len() as f32 * 16.0 + 4.0;
            }
            if notif.progress.is_some() {
                height += 24.0;
            }
            if !notif.actions.is_empty() {
                height += 36.0;
            }
            height += PADDING_BOTTOM;

            // Ensure min height for icon
            height = height.max(76.0);

            notif.card_y = cur_y;
            notif.card_height = height;

            cur_y += height + CARD_SPACING;
        }

        LayoutMetrics {
            total_width: CARD_WIDTH + 24.0, // extra margin for shadows
            total_height: cur_y + 12.0,
        }
    }

    pub fn render(&self, pixmap: &mut PixmapMut, notifications: &mut [Notification]) {
        pixmap.fill(Color::TRANSPARENT);

        for notif in notifications.iter_mut() {
            self.render_card(pixmap, notif);
        }
    }

    fn render_card(&self, pixmap: &mut PixmapMut, notif: &mut Notification) {
        let card_x = 12.0f32;
        let card_y = notif.card_y;
        let card_w = CARD_WIDTH;
        let card_h = notif.card_height;

        notif.card_bounds = Rect::new(card_x, card_y, card_w, card_h);

        // 1. Draw outer soft drop shadow
        self.draw_shadow(pixmap, card_x, card_y, card_w, card_h, CORNER_RADIUS);

        // 2. Draw card background
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
            paint.set_color(if notif.is_hovered {
                self.theme.bg_elevated
            } else {
                self.theme.bg
            });
            paint.anti_alias = true;
            pixmap.fill_path(
                &bg_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        // 3. Draw border stroke based on urgency
        let border_color = match notif.urgency {
            Urgency::Low => self.theme.border_low,
            Urgency::Normal => self.theme.border_normal,
            Urgency::Critical => self.theme.border_critical,
        };

        let mut border_builder = PathBuilder::new();
        Self::add_rounded_rect(
            &mut border_builder,
            card_x,
            card_y,
            card_w,
            card_h,
            CORNER_RADIUS,
        );
        if let Some(border_path) = border_builder.finish() {
            let stroke = Stroke {
                width: 1.5,
                ..Default::default()
            };
            let mut paint = Paint::default();
            paint.set_color(border_color);
            paint.anti_alias = true;
            pixmap.stroke_path(&border_path, &paint, &stroke, Transform::identity(), None);
        }

        // 4. Draw Icon area
        let icon_box_x = card_x + PADDING_X;
        let icon_box_y = card_y + PADDING_TOP;
        self.draw_icon(pixmap, notif, icon_box_x, icon_box_y);

        // 5. Draw Close '×' button at top-right
        let close_x = card_x + card_w - 28.0;
        let close_y = card_y + 12.0;
        let close_rect = Rect::new(close_x - 4.0, close_y - 4.0, 20.0, 20.0);
        notif.close_rect = Some(close_rect);

        let close_color = if notif.is_hovered {
            Color::from_rgba8(255, 255, 255, 220)
        } else {
            Color::from_rgba8(255, 255, 255, 120)
        };
        self.font.draw_text(
            pixmap,
            "✕",
            close_x,
            close_y + 12.0,
            12.0,
            close_color,
            false,
        );

        // 6. Content area (to right of icon)
        let content_x = card_x + PADDING_X + 44.0;
        let mut cur_y = card_y + PADDING_TOP;

        // App Name + Category + Dot
        let app_label = if let Some(cat) = &notif.category {
            if notif.app_name.is_empty() {
                cat.to_uppercase()
            } else {
                format!("{} • {}", notif.app_name.to_uppercase(), cat.to_uppercase())
            }
        } else if notif.app_name.is_empty() {
            "LAZYOS".to_string()
        } else {
            notif.app_name.to_uppercase()
        };
        // Dot
        let mut dot_builder = PathBuilder::new();
        Self::add_circle(&mut dot_builder, content_x + 3.0, cur_y + 4.0, 3.0);
        if let Some(dot_path) = dot_builder.finish() {
            let mut paint = Paint::default();
            paint.set_color(border_color);
            paint.anti_alias = true;
            pixmap.fill_path(
                &dot_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        self.font.draw_text(
            pixmap,
            &app_label,
            content_x + 12.0,
            cur_y + 8.0,
            10.0,
            self.theme.text_dim,
            true,
        );

        cur_y += 18.0;

        // Summary (Title)
        self.font.draw_text(
            pixmap,
            &notif.summary,
            content_x,
            cur_y + 10.0,
            13.0,
            self.theme.text_bright,
            true,
        );

        cur_y += 18.0;

        // Body Text
        let text_max_width = card_w - PADDING_X * 2.0 - 44.0;
        let body_lines = self
            .font
            .wrap_text(&notif.body, 12.0, text_max_width, 4, false);
        for line in &body_lines {
            self.font.draw_text(
                pixmap,
                line,
                content_x,
                cur_y + 9.0,
                12.0,
                self.theme.text_body,
                false,
            );
            cur_y += 16.0;
        }

        // 7. Progress Bar (if present)
        if let Some(pct) = notif.progress {
            cur_y += 6.0;
            let bar_x = card_x + PADDING_X;
            let bar_w = card_w - PADDING_X * 2.0;
            let bar_h = 6.0f32;

            // Background track
            let mut track_builder = PathBuilder::new();
            Self::add_rounded_rect(&mut track_builder, bar_x, cur_y, bar_w, bar_h, 3.0);
            if let Some(track_path) = track_builder.finish() {
                let mut paint = Paint::default();
                paint.set_color(Color::from_rgba8(255, 255, 255, 35));
                paint.anti_alias = true;
                pixmap.fill_path(
                    &track_path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }

            // Foreground fill
            let fill_pct = (pct as f32 / 100.0).clamp(0.0, 1.0);
            let fill_w = (bar_w * fill_pct).max(3.0);
            let fill_color = if let Some([r, g, b, a]) = notif.progress_color_override {
                Color::from_rgba8(r, g, b, a)
            } else {
                border_color
            };
            let mut fill_builder = PathBuilder::new();
            Self::add_rounded_rect(&mut fill_builder, bar_x, cur_y, fill_w, bar_h, 3.0);
            if let Some(fill_path) = fill_builder.finish() {
                let mut paint = Paint::default();
                paint.set_color(fill_color);
                paint.anti_alias = true;
                pixmap.fill_path(
                    &fill_path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }

            // Percentage label on the right
            let pct_str = format!("{pct}%");
            let pct_w = self.font.measure_width(&pct_str, 10.0, true);
            self.font.draw_text(
                pixmap,
                &pct_str,
                bar_x + bar_w - pct_w,
                cur_y - 4.0,
                10.0,
                border_color,
                true,
            );

            cur_y += 14.0;
        }

        // 8. Action Buttons
        notif.action_rects.clear();
        if !notif.actions.is_empty() {
            cur_y += 8.0;
            let btn_h = 24.0f32;
            let mut btn_x = card_x + card_w - PADDING_X;

            for action in notif.actions.iter().rev() {
                let label_w = self.font.measure_width(&action.label, 11.0, true);
                let btn_w = label_w + 20.0;
                btn_x -= btn_w;

                let rect = Rect::new(btn_x, cur_y, btn_w, btn_h);
                notif.action_rects.push((action.key.clone(), rect));

                let is_btn_hovered = notif.hovered_action.as_deref() == Some(&action.key);

                // Button pill background
                let mut btn_builder = PathBuilder::new();
                Self::add_rounded_rect(&mut btn_builder, btn_x, cur_y, btn_w, btn_h, btn_h * 0.5);
                if let Some(btn_path) = btn_builder.finish() {
                    let mut paint = Paint::default();
                    paint.set_color(if is_btn_hovered {
                        Color::from_rgba8(255, 255, 255, 50)
                    } else {
                        Color::from_rgba8(255, 255, 255, 22)
                    });
                    paint.anti_alias = true;
                    pixmap.fill_path(
                        &btn_path,
                        &paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );

                    // Pill border
                    let stroke = Stroke {
                        width: 1.0,
                        ..Default::default()
                    };
                    let mut bpaint = Paint::default();
                    bpaint.set_color(if is_btn_hovered {
                        border_color
                    } else {
                        Color::from_rgba8(255, 255, 255, 40)
                    });
                    bpaint.anti_alias = true;
                    pixmap.stroke_path(&btn_path, &bpaint, &stroke, Transform::identity(), None);
                }

                // Button label
                let text_x = btn_x + 10.0;
                let text_y = cur_y + 16.0;
                self.font.draw_text(
                    pixmap,
                    &action.label,
                    text_x,
                    text_y,
                    11.0,
                    if is_btn_hovered {
                        self.theme.text_bright
                    } else {
                        self.theme.text_body
                    },
                    true,
                );

                btn_x -= 8.0; // gap between buttons
            }
        }
    }

    fn draw_shadow(&self, pixmap: &mut PixmapMut, x: f32, y: f32, w: f32, h: f32, radius: f32) {
        let shadow_layers = [
            (4.0f32, 2.0f32, Color::from_rgba8(0, 0, 0, 35)),
            (8.0f32, 4.0f32, Color::from_rgba8(0, 0, 0, 20)),
            (14.0f32, 6.0f32, Color::from_rgba8(0, 0, 0, 10)),
        ];

        for (spread, offset_y, color) in shadow_layers {
            let mut builder = PathBuilder::new();
            Self::add_rounded_rect(
                &mut builder,
                x - spread * 0.5,
                y + offset_y - spread * 0.5,
                w + spread,
                h + spread,
                radius + spread * 0.5,
            );
            if let Some(path) = builder.finish() {
                let mut paint = Paint::default();
                paint.set_color(color);
                paint.anti_alias = true;
                pixmap.fill_path(
                    &path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }

    fn draw_icon(&self, pixmap: &mut PixmapMut, notif: &Notification, x: f32, y: f32) {
        let box_size = 36.0f32;

        // Container circle/rounded rect
        let mut box_builder = PathBuilder::new();
        Self::add_rounded_rect(&mut box_builder, x, y, box_size, box_size, 8.0);
        if let Some(box_path) = box_builder.finish() {
            let mut paint = Paint::default();
            paint.set_color(Color::from_rgba8(255, 255, 255, 16));
            paint.anti_alias = true;
            pixmap.fill_path(
                &box_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        // Try loading PNG icon
        if let Some(icon_pixmap) = Self::load_icon(&notif.app_icon) {
            let scale_x = box_size / icon_pixmap.width() as f32;
            let scale_y = box_size / icon_pixmap.height() as f32;
            let transform = Transform::from_scale(scale_x, scale_y).post_translate(x, y);

            pixmap.draw_pixmap(
                0,
                0,
                icon_pixmap.as_ref(),
                &PixmapPaint::default(),
                transform,
                None,
            );
        } else {
            // Draw fallback glyph
            let glyph_str = match notif.app_name.to_lowercase().as_str() {
                "download" => "󰇚",
                "media" | "music" => "󰎈",
                "wifi" | "network" => "󰖩",
                "bluetooth" => "󰂯",
                "battery" => "󰁹",
                "volume" => "󰕾",
                "brightness" => "󰃠",
                "clipboard" => "󰅌",
                _ => match notif.urgency {
                    Urgency::Critical => "󰀪",
                    _ => "󰋼",
                },
            };

            let glyph_color = match notif.urgency {
                Urgency::Low => self.theme.text_dim,
                Urgency::Normal => self.theme.accent,
                Urgency::Critical => self.theme.border_critical,
            };

            self.font.draw_text(
                pixmap,
                glyph_str,
                x + 9.0,
                y + 25.0,
                18.0,
                glyph_color,
                false,
            );
        }
    }

    fn load_icon(name: &str) -> Option<Pixmap> {
        if name.is_empty() {
            return None;
        }

        // Direct path check
        let path = if Path::new(name).is_absolute() {
            PathBuf::from(name)
        } else {
            // Search LazyOS theme roots
            let candidates = [
                format!("/usr/share/icons/LazyOS/48x48/apps/{name}.png"),
                format!("/usr/share/icons/LazyOS/32x32/apps/{name}.png"),
                format!("/usr/share/icons/hicolor/48x48/apps/{name}.png"),
                format!("/usr/share/icons/Adwaita/48x48/status/{name}.png"),
                format!("/usr/share/pixmaps/{name}.png"),
            ];

            candidates
                .iter()
                .map(PathBuf::from)
                .find(|p| p.exists())
                .unwrap_or_default()
        };

        if !path.exists() {
            return None;
        }

        let img_bytes = std::fs::read(&path).ok()?;
        let img = image::load_from_memory(&img_bytes).ok()?.to_rgba8();
        let (w, h) = img.dimensions();

        let mut pixmap = Pixmap::new(w, h)?;
        // Convert to premultiplied alpha for tiny-skia
        let chunk_size = 4;
        for (src_pixel, dst_pixel) in img
            .pixels()
            .zip(pixmap.data_mut().chunks_exact_mut(chunk_size))
        {
            let [r, g, b, a] = src_pixel.0;
            let alpha = a as f32 / 255.0;
            dst_pixel[0] = (r as f32 * alpha).round() as u8;
            dst_pixel[1] = (g as f32 * alpha).round() as u8;
            dst_pixel[2] = (b as f32 * alpha).round() as u8;
            dst_pixel[3] = a;
        }

        Some(pixmap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ActionItem;
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
        assert!(metrics.total_width > 300.0);
        assert!(metrics.total_height > 100.0);
        assert!(notifications[0].card_height > 80.0);

        let mut pixmap = Pixmap::new(
            metrics.total_width.ceil() as u32,
            metrics.total_height.ceil() as u32,
        )
        .expect("pixmap allocation");

        renderer.render(&mut pixmap.as_mut(), &mut notifications);

        // Verify that some non-zero pixels were drawn
        let chunk_size = 4;
        let non_zero_pixels = pixmap
            .data()
            .chunks_exact(chunk_size)
            .filter(|p| p[3] > 0)
            .count();
        assert!(non_zero_pixels > 100, "rendered card has painted pixels");
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
}
