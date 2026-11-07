#![allow(clippy::collapsible_if)]

mod dbus;
mod font;
mod history;
mod model;
mod render;
mod wayland;

use std::os::fd::AsRawFd;
use std::sync::mpsc::channel;
use std::thread;
use std::time::{Duration, Instant};

use wayland_client::Connection;
use zbus::blocking::connection::Builder;

use crate::dbus::{DbusCommand, NotificationServer};
use crate::history::{append_record, record_from_notification};
use crate::wayland::{UiEvent, WaylandState};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (dbus_tx, dbus_rx) = channel::<DbusCommand>();
    let (ui_tx, ui_rx) = channel::<UiEvent>();

    // 1. Start D-Bus thread
    let server = NotificationServer::new(dbus_tx);

    let _dbus_thread = thread::spawn(
        move || -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            // Fallback to /run/user/$UID/bus if DBUS_SESSION_BUS_ADDRESS is unset
            if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
                let uid = unsafe { libc::getuid() };
                let bus_path = format!("/run/user/{uid}/bus");
                if std::path::Path::new(&bus_path).exists() {
                    unsafe {
                        std::env::set_var(
                            "DBUS_SESSION_BUS_ADDRESS",
                            format!("unix:path={bus_path}"),
                        );
                    }
                }
            }

            let connection = Builder::session()?
                .name("org.freedesktop.Notifications")?
                .serve_at("/org/freedesktop/Notifications", server)?
                .build()?;

            let iface_ref = connection
                .object_server()
                .interface::<_, NotificationServer>("/org/freedesktop/Notifications")?;

            while let Ok(event) = ui_rx.recv() {
                match event {
                    UiEvent::ActionInvoked(id, key) => {
                        let _ = zbus::block_on(NotificationServer::action_invoked(
                            iface_ref.signal_emitter(),
                            id,
                            &key,
                        ));
                    }
                    UiEvent::NotificationClosed(id, reason) => {
                        let _ = zbus::block_on(NotificationServer::notification_closed(
                            iface_ref.signal_emitter(),
                            id,
                            reason,
                        ));
                    }
                }
            }

            Ok(())
        },
    );

    // 2. Connect to Wayland
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lazyos-notify: could not connect to Wayland display: {e}");
            return Ok(());
        }
    };

    let mut event_queue = conn.new_event_queue();
    let qh = event_queue.handle();

    let mut state = WaylandState::new(ui_tx.clone());

    let display = conn.display();
    let _ = display.get_registry(&qh, ());

    // Initial roundtrip to bind globals
    event_queue.roundtrip(&mut state)?;
    event_queue.roundtrip(&mut state)?;

    state.setup_layer_surface(&qh);
    event_queue.roundtrip(&mut state)?;

    let mut last_check = Instant::now();

    // 3. Main event loop
    loop {
        // Dispatch Wayland events
        event_queue.dispatch_pending(&mut state)?;
        conn.flush()?;

        // Process incoming D-Bus commands
        let mut dirty = false;
        while let Ok(cmd) = dbus_rx.try_recv() {
            match cmd {
                DbusCommand::New(notif) => {
                    let notif = *notif;
                    append_record(&record_from_notification(&notif));

                    // Check for in-place replacement (replaces_id or matching sync_tag)
                    let mut replaced = false;
                    for existing in &mut state.notifications {
                        let matches_tag = notif.sync_tag.is_some()
                            && existing.sync_tag.as_ref() == notif.sync_tag.as_ref();
                        let matches_id = notif.id == existing.id;

                        if matches_id || matches_tag {
                            existing.summary = notif.summary.clone();
                            existing.body = notif.body.clone();
                            existing.app_icon = notif.app_icon.clone();
                            existing.progress = notif.progress;
                            existing.actions = notif.actions.clone();
                            existing.urgency = notif.urgency;
                            existing.created_at = Instant::now();
                            existing.timeout = notif.timeout;
                            replaced = true;
                            dirty = true;
                            break;
                        }
                    }

                    if !replaced {
                        state.notifications.push(notif);
                        dirty = true;
                    }
                }
                DbusCommand::Close(id) => {
                    let before = state.notifications.len();
                    state.notifications.retain(|n| n.id != id);
                    if state.notifications.len() != before {
                        dirty = true;
                        let _ = ui_tx.send(UiEvent::NotificationClosed(id, 2));
                    }
                }
            }
        }

        // Timeout check (every 100 ms)
        if last_check.elapsed() >= Duration::from_millis(100) {
            last_check = Instant::now();
            let mut expired = Vec::new();
            state.notifications.retain(|n| {
                if n.is_expired() {
                    expired.push(n.id);
                    false
                } else {
                    true
                }
            });
            for id in expired {
                let _ = ui_tx.send(UiEvent::NotificationClosed(id, 1));
                dirty = true;
            }
        }

        if dirty {
            state.update_surface(&qh);
        }

        // Prepare poll for socket
        if let Some(guard) = conn.prepare_read() {
            let fd = guard.connection_fd().as_raw_fd();
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };

            let timeout = if state.notifications.is_empty() {
                250 // Idle poll
            } else {
                50 // Active animation/timer poll
            };

            let ret = unsafe { libc::poll(&mut pfd, 1, timeout) };
            if ret > 0 {
                let _ = guard.read();
            }
        }

        event_queue.dispatch_pending(&mut state)?;
    }
}
