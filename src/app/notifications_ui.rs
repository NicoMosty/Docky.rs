//! Panel de notificaciones: historial, apertura/cierre y eventos.
//!
//! Es el panel más simple del overlay (ver `menu/notifications.rs`): filas de alto
//! fijo, sin buscador, sin selección y sin acciones — la rueda y las flechas scrollean,
//! ESC y el click afuera cierran. Lo llena `App::show_notification`, el MISMO camino
//! que dibuja el pill: lo que se ve 4 s como aviso queda guardado acá.

use super::*;

impl App {
    /// Abre o cierra el panel. Lo llaman el IPC (`--toggle-notifications`) y el ciclo
    /// de pestañas del overlay.
    pub(crate) fn toggle_notifications(&mut self, qh: &QueueHandle<Self>) {
        if self.notifications_mode.is_some() {
            self.close_notifications_mode(qh);
        } else {
            self.open_notifications(qh);
        }
    }

    pub(super) fn open_notifications(&mut self, qh: &QueueHandle<Self>) {
        // ----- un solo panel a la vez, como el portapapeles -----
        self.wallpaper_mode = None;
        self.dock_menu_mode = None;
        self.app_search_mode = None;
        self.osd_mode = None;
        self.clipboard_mode = None;
        self.popup_mode = None;
        self.menu = None;
        self.held_key = None;

        self.layer.set_layer(Layer::Top);
        let is_vertical = self.dock.is_vertical();
        // ----- los avisos son frases cortas: en el vertical alcanza el cross angosto
        // (el mismo del selector de fondos) y en el ancho, el ancho del overlay -----
        // ----- mismo tamaño que el portapapeles (pedido: "parecido al UI del
        // clipboard en tamaño"): el cross ancho en vertical y su alto de contenido, no
        // un panel propio que crecía con la cantidad de avisos -----
        let content_w = if is_vertical {
            menu::OVERLAY_PANEL_VERTICAL_WIDE.max(self.dock.thickness() as f32)
        } else {
            menu::OVERLAY_PANEL_W
        };
        let frame = menu::frame_for(
            content_w,
            menu_render::clip_content_h(),
            is_vertical,
            self.dock.band_left(),
        );
        let (panel_w, panel_h) = menu::panel_size(frame, is_vertical);
        // ----- el teclado lo tiene el panel: es lo que hace que ESC y las flechas
        // lleguen (igual que el portapapeles) -----
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        self.apply_panel_size(panel_w, panel_h);
        log::debug!("notifs: abre con {} aviso(s)", self.notifications.len());
        self.notifications_mode = Some(NotificationsMode {
            scroll: 0.0,
            hovered: None,
            frame,
            is_vertical,
        });
        self.request_redraw(qh);
    }

    pub(super) fn close_notifications_mode(&mut self, qh: &QueueHandle<Self>) {
        // ----- cierre inmediato, como el portapapeles y el panel de ajustes: con
        // animación el cambio de pestaña se cruzaba con el que entra -----
        if self.notifications_mode.is_none() {
            return;
        }
        self.notifications_mode = None;
        self.held_key = None;
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::None);
        self.restore_dock_size();
        self.draw(qh);
        trim_heap();
    }

    /// Mueve el scroll y lo clampea a lo que sobra. Es el único estado del panel.
    fn scroll_notifications(&mut self, delta: f32, qh: &QueueHandle<Self>) {
        let count = self.notifications.len();
        let Some(nm) = self.notifications_mode.as_mut() else {
            return;
        };
        let max = menu::notif_max_scroll(nm.frame, count);
        let next = (nm.scroll + delta).clamp(0.0, max);
        if next == nm.scroll {
            return;
        }
        nm.scroll = next;
        self.request_redraw(qh);
    }

    pub(super) fn handle_notifications_key(&mut self, event: KeyEvent, qh: &QueueHandle<Self>) {
        let row = menu::NOTIF_ROW_H;
        let page = row * menu::NOTIF_VISIBLE_ROWS as f32;
        match event.keysym {
            Keysym::Escape => self.close_notifications_mode(qh),
            // ----- sin selección: no hay nada que activar, así que las flechas sólo
            // scrollean (el resaltado es el hover del mouse) -----
            Keysym::Down => self.scroll_notifications(row, qh),
            Keysym::Up => self.scroll_notifications(-row, qh),
            Keysym::Next => self.scroll_notifications(page, qh),
            Keysym::Prior => self.scroll_notifications(-page, qh),
            Keysym::Home => self.scroll_notifications(-1e6, qh),
            Keysym::End => self.scroll_notifications(1e6, qh),
            _ => {}
        }
    }

    pub(super) fn handle_notifications_pointer_event(
        &mut self,
        event: &PointerEvent,
        qh: &QueueHandle<Self>,
    ) {
        // ----- coordenadas de la superficie -> del panel (va debajo del dock cuando el
        // dock está arriba: trampa 4) -----
        let (px, py) = self.panel_local(event.position.0, event.position.1);
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                let hit = self.notifications_row_at(px as f32, py as f32);
                if let Some(nm) = self.notifications_mode.as_mut()
                    && nm.hovered != hit
                {
                    nm.hovered = hit;
                    self.request_redraw(qh);
                }
            }
            PointerEventKind::Leave { .. } => {
                if let Some(nm) = self.notifications_mode.as_mut() {
                    nm.hovered = None;
                }
                self.request_redraw(qh);
            }
            PointerEventKind::Axis {
                horizontal,
                vertical,
                ..
            } => {
                let delta = if vertical.absolute != 0.0 {
                    vertical.absolute
                } else {
                    horizontal.absolute
                };
                self.scroll_notifications(delta as f32 * 0.6, qh);
            }
            _ => {}
        }
    }

    fn notifications_row_at(&self, x: f32, y: f32) -> Option<usize> {
        let nm = self.notifications_mode.as_ref()?;
        menu::notif_row_at(nm.frame, nm.scroll, x, y, self.notifications.len())
    }

    pub(super) fn draw_notifications_mode(&mut self, qh: &QueueHandle<Self>) {
        let scale = self.output_scale.max(1) as f32;
        let tabs = self.current_overlay().map(OverlayMode::tab_index);
        // ----- el reparto (dock arriba, panel abajo) se calcula ANTES de tomar
        // prestado el modo: `overlay_layout` necesita `&self` entero -----
        let Some(layout) = self.overlay_layout() else {
            return;
        };
        let Some(nm) = self.notifications_mode.as_ref() else {
            return;
        };
        let (panel_w, panel_h) = menu::panel_size(nm.frame, nm.is_vertical);
        let (width, height) = (
            (panel_w * scale).round() as i32,
            (panel_h * scale).round() as i32,
        );
        if width <= 0 || height <= 0 {
            return;
        }
        let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32).unwrap();
        let args = menu_render::NotifArgs {
            settings: &self.dock.config.settings,
            entries: &self.notifications,
            scroll: nm.scroll,
            hovered: nm.hovered,
            render_scale: scale,
            frame: nm.frame,
            is_vertical: nm.is_vertical,
            overlay_tabs: tabs,
        };
        menu_render::draw_notifications(&mut pixmap, &mut self.text_cache, args);
        // ----- el panel ya trae su propio fondo; la superficie va opaca con el dock
        // nítido arriba (igual que el portapapeles) -----
        self.show_panel_surface(qh, &pixmap, layout, 1.0);
    }
}
