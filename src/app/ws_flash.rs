use super::*;

/// Relleno horizontal/vertical del panel alrededor del indicador.
const WS_FLASH_PAD: f32 = 26.0;

impl App {
    /// Muestra el HUD de workspaces: mismo panel y misma posición que el dock,
    /// pero con un único contenido (el indicador de workspaces).
    pub(crate) fn show_ws_flash(&mut self, qh: &QueueHandle<Self>) {
        log::debug!(
            "wsflash: show visible={} placed={} borrowed={} menu={} popup={} ws={:?}",
            self.dock_visible,
            self.widget_placed(crate::config::WidgetKind::Workspaces),
            self.layer_is_borrowed(),
            self.menu.is_some(),
            self.popup_mode.is_some(),
            self.widgets
                .workspaces
                .iter()
                .map(|w| (w.id, w.active))
                .collect::<Vec<_>>()
        );
        // ----- el propio HUD no cuenta como "ocupado": si ya está abierto se
        // actualiza con el workspace nuevo (si no, el indicador queda clavado) -----
        if (self.layer_is_borrowed() && self.ws_flash_mode.is_none())
            || self.menu.is_some()
            || self.popup_mode.is_some()
        {
            return;
        }
        // ----- sólo tiene sentido si el indicador está colocado y NO se ve: si el
        // dock ya está en pantalla, ya muestra los workspaces -----
        if self.dock_visible || !self.widget_placed(crate::config::WidgetKind::Workspaces) {
            return;
        }
        let len = render::workspaces_geometry(&self.widgets.workspaces, 1.0) + WS_FLASH_PAD;
        let thick = self.dock.thickness() as f32;
        let (panel_w, panel_h) = if self.dock.is_vertical() {
            (thick, len)
        } else {
            (len, thick)
        };
        let needs_resize = match self.ws_flash_mode.as_ref() {
            None => true,
            Some(m) => m.panel_w != panel_w || m.panel_h != panel_h,
        };
        if self.ws_flash_mode.is_none() {
            self.layer.set_layer(Layer::Overlay);
            self.ws_flash_mode = Some(WsFlashMode {
                anim: 0.0,
                target_anim: 1.0,
                closing: false,
                panel_w,
                panel_h,
            });
        } else if let Some(m) = self.ws_flash_mode.as_mut() {
            m.closing = false;
            m.target_anim = 1.0;
            m.panel_w = panel_w;
            m.panel_h = panel_h;
        }
        if needs_resize {
            let s = &self.dock.config.settings;
            let (anchor, margin) = edge_anchor_margin(s.dock_edge, s.dock_align, s.pos_y, 0);
            self.layer.set_anchor(anchor);
            self.layer
                .set_margin(margin.0, margin.1, margin.2, margin.3);
            self.layer.set_size(panel_w as u32, panel_h as u32);
            self.applied_size = Some((panel_w as u32, panel_h as u32));
            // ----- el HUD también cambia anchor/margin: anotarlo para que la
            // idempotencia de sync_autohide_surfaces no se confunda -----
            self.applied_geom = Some((anchor, margin));
            log::debug!("wsflash: resize panel {panel_w}x{panel_h}");
        }
        let _ = self.ws_reset_tx.send(());
        self.request_redraw(qh);
    }

    pub(crate) fn close_ws_flash_mode(&mut self, qh: &QueueHandle<Self>) {
        if let Some(m) = self.ws_flash_mode.as_mut() {
            m.closing = true;
            m.target_anim = 0.0;
        }
        self.request_redraw(qh);
    }

    /// ----- click sobre el HUD de workspaces: cambia al workspace del punto -----
    /// El HUD se traga TODOS los eventos de puntero mientras está visible, no
    /// sólo los clicks: si el movimiento revelara el dock completo, el HUD se
    /// cerraba justo cuando el puntero se le acercaba y el punto se volvía
    /// inclickeable.
    pub(super) fn handle_ws_flash_pointer_event(
        &mut self,
        event: &PointerEvent,
        qh: &QueueHandle<Self>,
    ) {
        // ----- acercarse al HUD revela el dock completo: el HUD es un indicador
        // transitorio, así que acá NO se traga el movimiento (tragarlo dejaba el
        // dock inalcanzable durante los 3s que dura el HUD). reveal_dock cierra
        // el HUD y el dibujo pasa al dock. -----
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                self.reveal_dock(qh);
                return;
            }
            PointerEventKind::Press { button, .. } if button == BTN_LEFT => {}
            _ => return,
        }
        let (x, y) = event.position;
        let Some((panel_w, panel_h)) = self.ws_flash_mode.as_ref().map(|m| (m.panel_w, m.panel_h))
        else {
            return;
        };
        let Some(id) = render::ws_flash_dot_hit(
            &self.widgets.workspaces,
            self.dock.is_vertical(),
            panel_w,
            panel_h,
            x,
            y,
        ) else {
            log::debug!("wsflash: click ({x:.0},{y:.0}) fuera de los puntos");
            return;
        };
        log::debug!("wsflash: click ({x:.0},{y:.0}) -> workspace {id}");
        let output = self
            .widgets
            .workspaces
            .iter()
            .find(|w| w.id == id)
            .map(|w| w.output.clone())
            .unwrap_or_default();
        let current = self
            .widgets
            .workspaces
            .iter()
            .find(|w| w.active)
            .map(|w| w.output.clone())
            .unwrap_or_default();
        widgets::workspace_switch(id, &output, &current);
    }

    pub(super) fn draw_ws_flash_mode(&mut self, qh: &QueueHandle<Self>) {
        let scale = self.output_scale.max(1) as f32;
        let transparency = self.dock.config.settings.transparency;
        let Some(m) = self.ws_flash_mode.as_ref() else {
            return;
        };
        let linear = m.anim.clamp(0.0, 1.0);
        let closing = m.closing;
        let eased = 0.5 - 0.5 * (std::f32::consts::PI * linear).cos();
        let (panel_w, panel_h) = (m.panel_w, m.panel_h);
        let width = (panel_w * scale).round() as i32;
        let height = (panel_h * scale).round() as i32;
        if width <= 0 || height <= 0 {
            return;
        }
        let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32).unwrap();
        // ----- advance = true: el HUD también anima el deslizamiento del
        // indicador hacia el workspace nuevo (si no, queda clavado en el viejo) -----
        render::draw_ws_flash(
            &mut pixmap,
            &self.dock,
            &self.widgets,
            &mut self.marquee,
            true,
            scale,
            panel_w,
            panel_h,
        );
        if closing || eased < 0.999 {
            // ----- desvanecido de entrada/salida -----
            let opacity = anim_opacity(transparency, eased.max(if closing { 0.0 } else { 0.05 }));
            let mut faded = tiny_skia::Pixmap::new(width as u32, height as u32).unwrap();
            faded.draw_pixmap(
                0,
                0,
                pixmap.as_ref(),
                &tiny_skia::PixmapPaint {
                    opacity,
                    ..Default::default()
                },
                tiny_skia::Transform::identity(),
                None,
            );
            pixmap = faded;
        }

        let stride = width * 4;
        let (buffer, canvas) = self
            .pool
            .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
            .expect("failed to create shm buffer");
        bgra_from_rgba(pixmap.data(), canvas);
        let surface = self.layer.wl_surface();
        surface.set_buffer_scale(self.output_scale.max(1));
        buffer.attach_to(surface).expect("failed to attach buffer");
        surface.damage_buffer(0, 0, width, height);
        surface.frame(qh, surface.clone());
        self.awaiting_frame = true;
        surface.commit();
    }

    pub(super) fn tick_ws_flash_frame(&mut self, qh: &QueueHandle<Self>) {
        let (panel_animating, closing, anim) = {
            let Some(m) = self.ws_flash_mode.as_mut() else {
                return;
            };
            let animating = if m.anim < m.target_anim {
                m.anim = (m.anim + menu::ANIM_STEP_OPEN).min(m.target_anim);
                true
            } else if m.anim > m.target_anim {
                m.anim = (m.anim - menu::ANIM_STEP_CLOSE).max(m.target_anim);
                true
            } else {
                false
            };
            (animating, m.closing, m.anim)
        };

        if closing && anim <= 0.0 {
            // ----- devolver la superficie al dock, con su tamaño -----
            self.ws_flash_mode = None;
            self.layer.set_layer(Layer::Top);
            let (w, h) = self.dock.base_size();
            self.layer.set_size(w, h);
            self.applied_size = Some((w, h));
            self.draw(qh);
            crate::app::trim_heap();
            return;
        }
        // ----- seguir dibujando también mientras el indicador se desliza -----
        if panel_animating || self.marquee.workspace_animating() {
            self.draw_ws_flash_mode(qh);
        }
    }
}
