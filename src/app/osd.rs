use super::*;

impl App {
    /// Muestra el OSD (volumen/brillo) como un **estado de la isla**: una píldora del
    /// grosor del dock, sin prestar la superficie ni cambiar su tamaño. Por eso no tiene
    /// animación propia —el reloj es el timer del OSD— y entra y sale seco, como el
    /// toast de notificaciones.
    pub(crate) fn show_osd(&mut self, kind: menu::OsdKind, qh: &QueueHandle<Self>) {
        // ----- sí sigue cediendo ante los paneles que SÍ prestan la superficie (el OSD
        // tiene que ganarle al dock y a la isla, no a un panel que el usuario abrió) -----
        if self.wallpaper_mode.is_some()
            || self.dock_menu_mode.is_some()
            || self.app_search_mode.is_some()
            || self.menu.is_some()
            || self.clipboard_mode.is_some()
            || self.keybinds_mode.is_some()
        {
            log::debug!("osd: descartado (panel abierto)");
            return;
        }
        // ----- el OSD reemplaza el HUD de workspaces: los dos son estados de la misma
        // cápsula y el OSD es el más nuevo -----
        self.ws_flash_mode = None;
        let (level, muted) = match kind {
            menu::OsdKind::Volume => match crate::widgets::read_volume() {
                Some((v, m)) => (v, m),
                None => {
                    log::debug!("osd: sin lectura de volumen");
                    return;
                }
            },
            menu::OsdKind::Brightness => match crate::widgets::read_brightness() {
                Some(b) => (b, false),
                None => {
                    log::debug!("osd: sin lectura de brillo");
                    return;
                }
            },
        };
        log::debug!("osd: {kind:?} {level}% muted={muted}");
        self.osd_mode = Some(OsdMode { kind, level, muted });
        self.needs_repaint = true;
        let _ = self.osd_reset_tx.send(());
        self.request_redraw(qh);
    }

    /// Cierra el OSD. Es inmediato a propósito: el estado no tiene animación, así que no
    /// depende de que llegue un frame callback (misma lección que el toast).
    pub(crate) fn close_osd_mode(&mut self, qh: &QueueHandle<Self>) {
        if self.osd_mode.take().is_none() {
            return;
        }
        self.needs_repaint = true;
        self.request_redraw(qh);
    }

    /// Dibuja la píldora del OSD: la MISMA cápsula del dock recortada al largo del OSD,
    /// con el contenido (icono + barra + nivel) adentro. No toca el tamaño de la
    /// superficie ni su layer, así que vale igual con el dock oculto o visible.
    pub(super) fn draw_osd_mode(&mut self, qh: &QueueHandle<Self>) {
        let scale = self.output_scale.max(1) as f32;
        let Some(m) = self.osd_mode.as_ref() else {
            return;
        };
        let (kind, level, muted) = (m.kind, m.level, m.muted);
        let (base_w, base_h) = self.dock.base_size();
        let width = (base_w as f32 * scale).round() as i32;
        let height = (base_h as f32 * scale).round() as i32;
        if width <= 0 || height <= 0 {
            return;
        }
        let stride = width * 4;
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
        else {
            // ----- mismo criterio que la isla: si falla, queda el buffer viejo puesto
            // (la superficie sigue mapeada) en vez de morir -----
            log::error!("no pude crear el buffer del OSD ({width}x{height})");
            return;
        };
        if self.frame_pixmap.as_ref().map(|p| (p.width(), p.height()))
            != Some((width as u32, height as u32))
        {
            match tiny_skia::Pixmap::new(width as u32, height as u32) {
                Some(p) => self.frame_pixmap = Some(p),
                None => {
                    log::error!("no pude crear el pixmap del OSD ({width}x{height})");
                    return;
                }
            }
        }
        // ----- la píldora es la cápsula de la isla: MISMA cuenta (`reveal_capsule`) con
        // el largo del OSD, sin corrimiento -----
        let pill = menu::OSD_PILL_LEN * scale;
        let blob = render::reveal_capsule(
            width as f32,
            height as f32,
            self.dock.is_vertical(),
            pill,
            0.0,
            0.0,
        );
        let (bx, by) = (blob.0.round() as i32, blob.1.round() as i32);
        let (bw, bh) = (
            blob.2.round().max(1.0) as u32,
            blob.3.round().max(1.0) as u32,
        );
        let args = menu_render::OsdArgs {
            kind,
            level,
            muted,
            // ----- el contenido se dibuja en su propia píxel-map y se pega en el blob:
            // el largo/corto sale del blob, así dibujo y medida no se pueden despegar -----
            panel_w: bw as f32 / scale,
            panel_h: bh as f32 / scale,
            dock: &self.dock,
            render_scale: scale,
        };
        let dock = &self.dock;
        let Some(pixmap) = self.frame_pixmap.as_mut() else {
            return;
        };
        pixmap.fill(tiny_skia::Color::TRANSPARENT);
        render::draw_capsule(pixmap, dock, scale, blob);
        let Some(mut content) = tiny_skia::Pixmap::new(bw, bh) else {
            return;
        };
        menu_render::draw_osd(&mut content, &mut self.text_cache, &args);
        if let Some(mask) =
            render::reveal_mask(dock, scale, pill, 0.0, width as f32, height as f32, 0.0)
        {
            pixmap.draw_pixmap(
                bx,
                by,
                content.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                tiny_skia::Transform::identity(),
                Some(&mask),
            );
        }
        bgra_from_rgba(pixmap.data(), canvas);
        log::debug!("osd: píldora {bw}x{bh} en ({bx},{by})");

        let surface = self.layer.wl_surface();
        surface.set_buffer_scale(self.output_scale.max(1));
        if let Err(err) = buffer.attach_to(surface) {
            log::error!("no pude mapear el buffer del OSD: {err}");
            return;
        }
        surface.damage_buffer(0, 0, width, height);
        surface.frame(qh, surface.clone());
        self.awaiting_frame = true;
        surface.commit();
    }
}
