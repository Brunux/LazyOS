#![allow(unknown_lints)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::chunks_exact_to_as_chunks)]

mod dbus;
mod font;
mod history;
mod model;
mod render;
mod wayland;

use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::channel;
use std::thread;
use std::time::{Duration, Instant};

use wayland_client::Connection;
use zbus::blocking::connection::Builder;

use crate::dbus::{DbusCommand, NotificationServer};
use crate::history::{append_record, record_from_notification};
use crate::render::CardTheme;
use crate::wayland::{UiEvent, WaylandState};

static THEME_DIRTY: AtomicBool = AtomicBool::new(false);
static GLOBAL_WAKE_FD: AtomicI32 = AtomicI32::new(-1);

extern "C" fn sigusr1_handler(_: libc::c_int) {
    THEME_DIRTY.store(true, Ordering::SeqCst);
    let fd = GLOBAL_WAKE_FD.load(Ordering::SeqCst);
    if fd >= 0 {
        let val: u64 = 1;
        unsafe {
            libc::write(
                fd,
                &val as *const _ as *const libc::c_void,
                std::mem::size_of::<u64>(),
            );
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (dbus_tx, dbus_rx) = channel::<DbusCommand>();
    let (ui_tx, ui_rx) = channel::<UiEvent>();

    // 0. Setup eventfd for instantaneous wakeups on D-Bus notifications and signals
    let wake_fd = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
    if wake_fd >= 0 {
        GLOBAL_WAKE_FD.store(wake_fd, Ordering::SeqCst);
        unsafe {
            libc::signal(libc::SIGUSR1, sigusr1_handler as *const () as usize);
        }
    }

    // 1. Start D-Bus thread
    let server = NotificationServer::new(dbus_tx, wake_fd);

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

    let mut last_check = Instant::now();

    // 3. Main event loop
    loop {
        // Dispatch Wayland events
        event_queue.dispatch_pending(&mut state)?;
        let _ = conn.flush();

        let mut dirty = false;

        // Check theme reload signal
        if THEME_DIRTY.swap(false, Ordering::SeqCst) {
            state.renderer.theme = CardTheme::from_active();
            if !state.notifications.is_empty() {
                dirty = true;
            }
        }

        // Process incoming D-Bus commands
        while let Ok(cmd) = dbus_rx.try_recv() {
            match cmd {
                DbusCommand::New(notif) => {
                    let notif = *notif;
                    append_record(&record_from_notification(&notif));
                    state.renderer.theme = CardTheme::from_active();

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

        // Timeout check (every 50 ms)
        if last_check.elapsed() >= Duration::from_millis(50) {
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
            if state.notifications.is_empty() {
                state.destroy_surface();
                let _ = conn.flush();
            } else {
                let metrics = state.renderer.compute_layout(&mut state.notifications);
                let target_w = metrics.total_width.ceil() as u32;
                let target_h = metrics.total_height.ceil() as u32;

                if state.surface.is_none() {
                    state.setup_layer_surface(&qh, target_w, target_h);
                    event_queue.roundtrip(&mut state)?;
                    let _ = conn.flush();
                } else if target_w != state.width || target_h != state.height {
                    state.width = target_w;
                    state.height = target_h;
                    if let Some(ref ls) = state.layer_surface {
                        ls.set_size(target_w, target_h);
                    }
                    if let Some(ref s) = state.surface {
                        s.commit();
                    }
                    event_queue.roundtrip(&mut state)?;
                    let _ = conn.flush();
                } else {
                    state.paint_surface(&qh);
                    let _ = conn.flush();
                }
            }
        }

        // Prepare poll for socket & wake_fd
        if let Some(guard) = conn.prepare_read() {
            let wayland_fd = guard.connection_fd().as_raw_fd();
            let mut pfds = [
                libc::pollfd {
                    fd: wayland_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: wake_fd,
                    events: libc::POLLIN,
                    revents: 0,
                },
            ];

            let timeout = if state.notifications.is_empty() {
                -1 // Idle poll: wait indefinitely for wake_fd or Wayland event
            } else {
                50 // Active animation/timer poll
            };

            let _ = conn.flush();
            let ret = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, timeout) };
            if ret > 0 {
                if pfds[0].revents & libc::POLLIN != 0 {
                    let _ = guard.read();
                }
                if pfds[1].revents & libc::POLLIN != 0 {
                    let mut buf = 0u64;
                    let _ = unsafe {
                        libc::read(
                            wake_fd,
                            &mut buf as *mut _ as *mut libc::c_void,
                            std::mem::size_of::<u64>(),
                        )
                    };
                }
            }
        }

        event_queue.dispatch_pending(&mut state)?;
        let _ = conn.flush();
    }
}
