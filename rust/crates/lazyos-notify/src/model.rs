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

pub const ANIM_DURATION: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimState {
    Opening { start: Instant },
    Active,
    Closing { start: Instant, reason: u32 },
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
    pub anim_state: AnimState,
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

    #[allow(dead_code)]
    pub fn is_animating(&self) -> bool {
        if std::env::var("LAZYOS_REDUCED_MOTION").is_ok() {
            return false;
        }
        match self.anim_state {
            AnimState::Opening { start } => start.elapsed() < ANIM_DURATION,
            AnimState::Active => false,
            AnimState::Closing { start, .. } => start.elapsed() < ANIM_DURATION,
        }
    }

    pub fn is_closing(&self) -> bool {
        matches!(self.anim_state, AnimState::Closing { .. })
    }

    #[allow(dead_code)]
    pub fn is_opening(&self) -> bool {
        matches!(self.anim_state, AnimState::Opening { .. })
    }

    pub fn animation_factors(&self) -> (f32, f32) {
        if std::env::var("LAZYOS_REDUCED_MOTION").is_ok() {
            return (1.0, 1.0);
        }
        match self.anim_state {
            AnimState::Opening { start } => {
                let t = (start.elapsed().as_secs_f32() / 0.150).min(1.0);
                let p = 1.0 - (1.0 - t).powi(3); // ease-out cubic
                (0.85 + 0.15 * p, p)
            }
            AnimState::Active => (1.0, 1.0),
            AnimState::Closing { start, .. } => {
                let t = (start.elapsed().as_secs_f32() / 0.150).min(1.0);
                let p = (1.0 - t).max(0.0).powi(3); // ease-in cubic
                (0.85 + 0.15 * p, p)
            }
        }
    }

    pub fn is_osd(&self) -> bool {
        self.sync_tag.as_deref() == Some("lazyos-osd")
            || self.sync_tag.as_deref() == Some("volume")
            || self.sync_tag.as_deref() == Some("brightness")
            || self.sync_tag.as_deref() == Some("osd")
            || self.category.as_deref() == Some("osd")
            || self.app_name.to_lowercase() == "osd"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_notification(anim_state: AnimState) -> Notification {
        Notification {
            id: 1,
            app_name: "test".to_string(),
            summary: "Test".to_string(),
            body: "Test body".to_string(),
            app_icon: "".to_string(),
            actions: Vec::new(),
            urgency: Urgency::Normal,
            category: None,
            progress: None,
            sync_tag: None,
            created_at: Instant::now(),
            timeout: Duration::from_secs(5),
            anim_state,
            card_y: 0.0,
            card_height: 70.0,
            card_bounds: Rect::new(0.0, 0.0, 300.0, 70.0),
            action_rects: Vec::new(),
            close_rect: None,
            is_hovered: false,
            hovered_action: None,
            dismissing: false,
            progress_color_override: None,
        }
    }

    #[test]
    fn test_animation_active() {
        let n = dummy_notification(AnimState::Active);
        assert!(!n.is_closing());
        assert!(!n.is_opening());
        assert!(!n.is_animating());
        let (scale, alpha) = n.animation_factors();
        assert_eq!(scale, 1.0);
        assert_eq!(alpha, 1.0);
    }

    #[test]
    fn test_animation_opening_factors() {
        // Just started opening
        let n = dummy_notification(AnimState::Opening {
            start: Instant::now(),
        });
        assert!(n.is_opening());
        assert!(!n.is_closing());
        assert!(n.is_animating());
        let (scale, alpha) = n.animation_factors();
        assert!((0.85..=0.87).contains(&scale));
        assert!((0.0..=0.05).contains(&alpha));

        // Finished opening
        let past = Instant::now() - Duration::from_millis(200);
        let n_done = dummy_notification(AnimState::Opening { start: past });
        assert!(!n_done.is_animating());
        let (scale, alpha) = n_done.animation_factors();
        assert_eq!(scale, 1.0);
        assert_eq!(alpha, 1.0);
    }

    #[test]
    fn test_animation_closing_factors() {
        // Just started closing
        let n = dummy_notification(AnimState::Closing {
            start: Instant::now(),
            reason: 1,
        });
        assert!(n.is_closing());
        assert!(!n.is_opening());
        assert!(n.is_animating());
        let (scale, alpha) = n.animation_factors();
        assert!((0.98..=1.0).contains(&scale));
        assert!((0.95..=1.0).contains(&alpha));

        // Finished closing
        let past = Instant::now() - Duration::from_millis(200);
        let n_done = dummy_notification(AnimState::Closing {
            start: past,
            reason: 1,
        });
        assert!(!n_done.is_animating());
        let (scale, alpha) = n_done.animation_factors();
        assert_eq!(scale, 0.85);
        assert_eq!(alpha, 0.0);
    }
}
