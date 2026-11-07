use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;
use std::sync::mpsc::Sender;

use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_pointer, wl_region, wl_registry, wl_seat, wl_shm, wl_shm_pool,
    wl_surface,
};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::model::Notification;
use crate::render::Renderer;

pub enum UiEvent {
    ActionInvoked(u32, String),
    NotificationClosed(u32, u32), // id, reason (1 = expired, 2 = user dismissed)
}

pub struct WaylandState {
    pub compositor: Option<wl_compositor::WlCompositor>,
    pub shm: Option<wl_shm::WlShm>,
    pub layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    pub seat: Option<wl_seat::WlSeat>,
    pub pointer: Option<wl_pointer::WlPointer>,
    pub surface: Option<wl_surface::WlSurface>,
    pub layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,

    pub width: u32,
    pub height: u32,
    pub configured: bool,

    pub pointer_x: f64,
    pub pointer_y: f64,

    pub notifications: Vec<Notification>,
    pub renderer: Renderer,
    pub event_tx: Sender<UiEvent>,
}

impl WaylandState {
    pub fn new(event_tx: Sender<UiEvent>) -> Self {
        Self {
            compositor: None,
            shm: None,
            layer_shell: None,
            seat: None,
            pointer: None,
            surface: None,
            layer_surface: None,
            width: 0,
            height: 0,
            configured: false,
            pointer_x: 0.0,
            pointer_y: 0.0,
            notifications: Vec::new(),
            renderer: Renderer::new(),
            event_tx,
        }
    }

    pub fn setup_layer_surface(&mut self, qh: &QueueHandle<Self>, w: u32, h: u32) {
        if self.surface.is_some() {
            return;
        }

        let Some(compositor) = self.compositor.as_ref() else {
            return;
        };
        let Some(layer_shell) = self.layer_shell.as_ref() else {
            return;
        };

        let surface = compositor.create_surface(qh, ());
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            None,
            zwlr_layer_shell_v1::Layer::Overlay,
            "lazyos-notify".to_string(),
            qh,
            (),
        );

        layer_surface
            .set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Right);
        layer_surface.set_margin(46, 12, 0, 0);
        layer_surface
            .set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
        layer_surface.set_size(w, h);

        surface.commit();

        self.surface = Some(surface);
        self.layer_surface = Some(layer_surface);
        self.width = w;
        self.height = h;
        self.configured = false;
    }

    pub fn destroy_surface(&mut self) {
        if let Some(ls) = self.layer_surface.take() {
            ls.destroy();
        }
        if let Some(s) = self.surface.take() {
            s.destroy();
        }
        self.width = 0;
        self.height = 0;
        self.configured = false;
    }

    pub fn paint_surface(&mut self, qh: &QueueHandle<Self>) {
        if !self.configured || self.notifications.is_empty() {
            return;
        }

        let Some(ref surface) = self.surface else {
            return;
        };
        let Some(ref shm) = self.shm else {
            return;
        };
        let Some(ref compositor) = self.compositor else {
            return;
        };

        let target_w = self.width;
        let target_h = self.height;
        if target_w == 0 || target_h == 0 {
            return;
        }

        // Allocate buffer and paint
        let stride = (target_w * 4) as usize;
        let size = stride * target_h as usize;

        let file = match tempfile::tempfile() {
            Ok(f) => f,
            Err(_) => return,
        };
        let _ = file.set_len(size as u64);

        let pool = shm.create_pool(file.as_fd(), size as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            target_w as i32,
            target_h as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
            qh,
            (),
        );

        let mut data = vec![0u8; size];
        if let Some(mut pixmap) = tiny_skia::PixmapMut::from_bytes(&mut data, target_w, target_h) {
            self.renderer.render(&mut pixmap, &mut self.notifications);
        }

        let _ = file.write_all_at(&data, 0);

        // Update input region around cards only
        let region = compositor.create_region(qh, ());
        for notif in &self.notifications {
            region.add(
                notif.card_bounds.x.round() as i32,
                notif.card_bounds.y.round() as i32,
                notif.card_bounds.w.round() as i32,
                notif.card_bounds.h.round() as i32,
            );
        }
        surface.set_input_region(Some(&region));

        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, target_w as i32, target_h as i32);
        surface.commit();
    }

    pub fn handle_pointer_button(
        &mut self,
        button: u32,
        state: wl_pointer::ButtonState,
        qh: &QueueHandle<Self>,
    ) {
        if state != wl_pointer::ButtonState::Pressed {
            return;
        }

        let px = self.pointer_x as f32;
        let py = self.pointer_y as f32;

        // Find which card was clicked
        let mut clicked_index = None;
        let mut clicked_action = None;
        let mut clicked_close = false;

        for (i, notif) in self.notifications.iter().enumerate() {
            if notif.card_bounds.contains(px, py) {
                clicked_index = Some(i);

                // Check close button
                if let Some(ref close) = notif.close_rect {
                    if close.contains(px, py) {
                        clicked_close = true;
                        break;
                    }
                }

                // Check action buttons
                for (key, rect) in &notif.action_rects {
                    if rect.contains(px, py) {
                        clicked_action = Some(key.clone());
                        break;
                    }
                }
                break;
            }
        }

        let Some(idx) = clicked_index else {
            return;
        };

        match button {
            0x110 => {
                // Left click
                if clicked_close {
                    let id = self.notifications[idx].id;
                    self.notifications.remove(idx);
                    let _ = self.event_tx.send(UiEvent::NotificationClosed(id, 2));
                    if self.notifications.is_empty() {
                        self.destroy_surface();
                    } else {
                        let metrics = self.renderer.compute_layout(&mut self.notifications);
                        self.width = metrics.total_width.ceil() as u32;
                        self.height = metrics.total_height.ceil() as u32;
                        self.paint_surface(qh);
                    }
                } else if let Some(action_key) = clicked_action {
                    let id = self.notifications[idx].id;
                    self.notifications.remove(idx);
                    let _ = self.event_tx.send(UiEvent::ActionInvoked(id, action_key));
                    let _ = self.event_tx.send(UiEvent::NotificationClosed(id, 2));
                    if self.notifications.is_empty() {
                        self.destroy_surface();
                    } else {
                        let metrics = self.renderer.compute_layout(&mut self.notifications);
                        self.width = metrics.total_width.ceil() as u32;
                        self.height = metrics.total_height.ceil() as u32;
                        self.paint_surface(qh);
                    }
                } else {
                    // Click on card body: invoke "default" action if present, or dismiss
                    let id = self.notifications[idx].id;
                    let has_default = self.notifications[idx]
                        .actions
                        .iter()
                        .any(|a| a.key == "default");
                    if has_default {
                        let _ = self
                            .event_tx
                            .send(UiEvent::ActionInvoked(id, "default".to_string()));
                    }
                    self.notifications.remove(idx);
                    let _ = self.event_tx.send(UiEvent::NotificationClosed(id, 2));
                    if self.notifications.is_empty() {
                        self.destroy_surface();
                    } else {
                        let metrics = self.renderer.compute_layout(&mut self.notifications);
                        self.width = metrics.total_width.ceil() as u32;
                        self.height = metrics.total_height.ceil() as u32;
                        self.paint_surface(qh);
                    }
                }
            }
            0x111 => {
                // Right click: dismiss card
                let id = self.notifications[idx].id;
                self.notifications.remove(idx);
                let _ = self.event_tx.send(UiEvent::NotificationClosed(id, 2));
                if self.notifications.is_empty() {
                    self.destroy_surface();
                } else {
                    let metrics = self.renderer.compute_layout(&mut self.notifications);
                    self.width = metrics.total_width.ceil() as u32;
                    self.height = metrics.total_height.ceil() as u32;
                    self.paint_surface(qh);
                }
            }
            0x112 => {
                // Middle click: dismiss all from this app
                let app = self.notifications[idx].app_name.clone();
                let mut remaining = Vec::new();
                for notif in self.notifications.drain(..) {
                    if notif.app_name == app {
                        let _ = self.event_tx.send(UiEvent::NotificationClosed(notif.id, 2));
                    } else {
                        remaining.push(notif);
                    }
                }
                self.notifications = remaining;
                if self.notifications.is_empty() {
                    self.destroy_surface();
                } else {
                    let metrics = self.renderer.compute_layout(&mut self.notifications);
                    self.width = metrics.total_width.ceil() as u32;
                    self.height = metrics.total_height.ceil() as u32;
                    self.paint_surface(qh);
                }
            }
            _ => {}
        }
    }

    pub fn handle_pointer_motion(&mut self, x: f64, y: f64, qh: &QueueHandle<Self>) {
        self.pointer_x = x;
        self.pointer_y = y;

        let px = x as f32;
        let py = y as f32;
        let mut needs_repaint = false;

        for notif in &mut self.notifications {
            let was_hovered = notif.is_hovered;
            let is_hovered = notif.card_bounds.contains(px, py);
            if was_hovered != is_hovered {
                notif.is_hovered = is_hovered;
                needs_repaint = true;
            }

            let old_action = notif.hovered_action.clone();
            let mut new_action = None;
            for (key, rect) in &notif.action_rects {
                if rect.contains(px, py) {
                    new_action = Some(key.clone());
                    break;
                }
            }
            if old_action != new_action {
                notif.hovered_action = new_action;
                needs_repaint = true;
            }
        }

        if needs_repaint {
            self.paint_surface(qh);
        }
    }
}

// --- Wayland Dispatch Implementations ---

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, 4.min(version), qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, 1, qh, ()));
                }
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, 4.min(version), qh, ()));
                }
                "wl_seat" => {
                    let seat: wl_seat::WlSeat = registry.bind(name, 7.min(version), qh, ());
                    state.seat = Some(seat);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm::WlShm, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: wl_shm_pool::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for WaylandState {
    fn event(
        _: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        event: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            buffer.destroy();
        }
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_region::WlRegion, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_region::WlRegion,
        _: wl_region::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for WaylandState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            let has_pointer = match capabilities {
                wayland_client::WEnum::Value(caps) => caps.contains(wl_seat::Capability::Pointer),
                _ => false,
            };
            if has_pointer && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.handle_pointer_motion(surface_x, surface_y, qh);
            }
            wl_pointer::Event::Button {
                button,
                state: wayland_client::WEnum::Value(val),
                ..
            } => {
                state.handle_pointer_button(button, val, qh);
            }
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                layer_surface.ack_configure(serial);
                state.configured = true;
                state.paint_surface(qh);
            }
            zwlr_layer_surface_v1::Event::Closed => {
                state.configured = false;
            }
            _ => {}
        }
    }
}
