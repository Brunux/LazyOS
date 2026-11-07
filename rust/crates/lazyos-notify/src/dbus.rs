use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use zbus::object_server::SignalEmitter;
use zbus::zvariant::Value;

use crate::model::{ActionItem, Notification, Rect, Urgency};

pub enum DbusCommand {
    New(Box<Notification>),
    Close(u32),
}

pub struct NotificationServer {
    next_id: AtomicU32,
    tx: Sender<DbusCommand>,
}

impl NotificationServer {
    pub fn new(tx: Sender<DbusCommand>) -> Self {
        Self {
            next_id: AtomicU32::new(1),
            tx,
        }
    }
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, Value<'_>>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            self.next_id.fetch_add(1, Ordering::SeqCst)
        };

        // Parse urgency (0 = Low, 1 = Normal, 2 = Critical)
        let urgency = if let Some(val) = hints.get("urgency") {
            if let Ok(u) = u8::try_from(val) {
                Urgency::from_u8(u)
            } else {
                Urgency::Normal
            }
        } else {
            Urgency::Normal
        };

        // Parse category
        let category = hints.get("category").and_then(|v| match v {
            Value::Str(s) => Some(s.to_string()),
            _ => None,
        });

        // Parse progress value (0..=100)
        let progress = hints.get("value").and_then(|v| {
            if let Ok(n) = i32::try_from(v) {
                Some(n.clamp(0, 100) as u8)
            } else if let Ok(n) = u8::try_from(v) {
                Some(n.min(100))
            } else {
                None
            }
        });

        // Parse sync tag
        let sync_tag = hints
            .get("x-canonical-private-synchronous")
            .and_then(|v| match v {
                Value::Str(s) => Some(s.to_string()),
                _ => None,
            });

        // Parse action items: pair of [id, label, id, label, ...]
        let mut action_items = Vec::new();
        let mut iter = actions.into_iter();
        while let (Some(key), Some(label)) = (iter.next(), iter.next()) {
            action_items.push(ActionItem { key, label });
        }

        let timeout_ms = match expire_timeout {
            t if t < 0 => match urgency {
                Urgency::Low => 3000,
                Urgency::Normal => 5000,
                Urgency::Critical => 0, // Critical doesn't auto-expire
            },
            0 => 0,
            t => t as u64,
        };

        let notification = Notification {
            id,
            app_name,
            summary,
            body,
            app_icon,
            actions: action_items,
            urgency,
            category,
            progress,
            sync_tag,
            created_at: std::time::Instant::now(),
            timeout: Duration::from_millis(timeout_ms),
            card_y: 0.0,
            card_height: 70.0,
            card_bounds: Rect::new(0.0, 0.0, 0.0, 0.0),
            action_rects: Vec::new(),
            close_rect: None,
            is_hovered: false,
            hovered_action: None,
            dismissing: false,
            progress_color_override: None,
        };

        let _ = self.tx.send(DbusCommand::New(Box::new(notification)));
        id
    }

    fn close_notification(&self, id: u32) {
        let _ = self.tx.send(DbusCommand::Close(id));
    }

    fn get_capabilities(&self) -> Vec<&'static str> {
        vec![
            "actions",
            "body",
            "body-hyperlinks",
            "body-markup",
            "icon-static",
            "persistence",
        ]
    }

    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("lazyos-notify", "LazyOS", "0.4.2", "1.2")
    }

    #[zbus(signal)]
    pub async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;
}
