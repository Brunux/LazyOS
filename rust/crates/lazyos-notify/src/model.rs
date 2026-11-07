use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => Self::Low,
            1 => Self::Normal,
            2 => Self::Critical,
            _ => Self::Normal,
        }
    }

    pub fn to_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::Critical => "critical",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionItem {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

#[derive(Clone, Debug)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub app_icon: String,
    pub actions: Vec<ActionItem>,
    pub urgency: Urgency,
    pub category: Option<String>,
    pub progress: Option<u8>,
    pub sync_tag: Option<String>,
    pub created_at: Instant,
    pub timeout: Duration,

    // Layout and animation state
    pub card_y: f32,
    pub card_height: f32,
    pub card_bounds: Rect,
    pub action_rects: Vec<(String, Rect)>,
    pub close_rect: Option<Rect>,
    pub is_hovered: bool,
    pub hovered_action: Option<String>,
    #[allow(dead_code)]
    pub dismissing: bool,
    pub progress_color_override: Option<[u8; 4]>,
}

impl Notification {
    pub fn is_expired(&self) -> bool {
        if self.urgency == Urgency::Critical || self.timeout.is_zero() {
            return false;
        }
        self.created_at.elapsed() >= self.timeout
    }

    pub fn is_osd(&self) -> bool {
        self.sync_tag.as_deref() == Some("lazyos-osd")
            || self.category.as_deref() == Some("osd")
            || self.app_name.to_lowercase() == "osd"
    }
}
