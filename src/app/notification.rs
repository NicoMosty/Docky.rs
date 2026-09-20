use super::*;

impl App {
    /// Guarda el aviso en el historial (el más nuevo primero, tope
    /// `NOTIF_HISTORY_CAP`) y, si el panel está abierto, lo re-abre para que el alto
    /// crezca con la fila nueva (el frame sale de la cantidad de avisos).
    ///
    /// La hora es la del reloj del dock **sin AM/PM** (`11:38`): la misma cadena que
    /// muestra la isla, así no hay un segundo formato de hora en el código.
    pub(crate) fn push_notification(
        &mut self,
        title: String,
        body: String,
        qh: &QueueHandle<Self>,
    ) {
        let at = crate::widgets::sin_ampm(&self.widgets.time).to_string();
        self.notifications
            .insert(0, menu::NotifyEntry { title, body, at });
        self.notifications.truncate(menu::NOTIF_HISTORY_CAP);
        if self.notifications_mode.is_some() {
            self.notifications_mode = None;
            self.open_notifications(qh);
        }
    }

    pub(crate) fn show_notification(
        &mut self,
        title: String,
        body: String,
        qh: &QueueHandle<Self>,
    ) {
        // ----- el historial primero: el panel tiene el aviso aunque el toast no llegue
        // a verse. Y no hay corte por modos abiertos: el toast vive en su propia
        // superficie, así que ya no le roba la suya a ningún panel -----
        self.push_notification(title.clone(), body.clone(), qh);
        let length = menu::OSD_NOTIFICATION_BASE_LEN + menu::NOTIFICATION_GROWTH_W;
        // ----- el toast va SIEMPRE apaisado (76x300 era el tubo vertical del dock: es
        // una caja de esquina, no una píldora pegada al borde del dock) -----
        let pad = menu::MENU_PADDING;
        let text_x = pad + 9.0 * 2.0 + 14.0;
        let max_w = (length - text_x - pad).max(1.0);
        // ----- el alto del cuerpo (líneas) define el cross y el timeout -----
        let body_lines =
            menu_render::wrap_to_width(&body, 8.5, max_w, menu::NOTIFICATION_BODY_MAX_LINES)
                .len()
                .max(1);
        let block_h = 9.5 * 1.4 + 3.0 + body_lines as f32 * 8.5 * 1.4;
        let cross =
            (block_h + pad * 2.0).max(menu::NOTIFICATION_MIN_PANEL_H + menu::NOTIFICATION_GROWTH_H);
        let (panel_w, panel_h) = (length, cross);
        let timeout =
            (menu::NOTIFICATION_TIMEOUT_MS + body_lines.saturating_sub(1) as u64 * 900).min(15000);
        let needs_resize = match self.notification_mode.as_ref() {
            None => true,
            Some(m) => m.panel_w != panel_w || m.panel_h != panel_h,
        };
        if self.notification_mode.is_none() {
            self.notification_mode = Some(NotificationMode {
                title,
                body,
                anim: 0.0,
                target_anim: 1.0,
                closing: false,
                panel_w,
                panel_h,
            });
        } else if let Some(m) = self.notification_mode.as_mut() {
            m.title = title;
            m.body = body;
            m.closing = false;
            m.target_anim = 1.0;
            m.panel_w = panel_w;
            m.panel_h = panel_h;
        }
        // ----- la superficie del toast: se crea al primer aviso y se le ajusta el
        // tamaño si el cuerpo cambió de alto (no se re-crea: eso la desmapearía) -----
        if self.toast_layer.is_none() {
            self.create_toast_surface(panel_w, panel_h, qh);
        } else if needs_resize && let Some(l) = self.toast_layer.as_ref() {
            l.set_size(
                panel_w.round().max(1.0) as u32,
                panel_h.round().max(1.0) as u32,
            );
            l.commit();
        }
        let _ = self.notification_reset_tx.send(timeout);
        // ----- una superficie recién creada no tiene tamaño hasta que llega el
        // configure (el attach antes de eso lo rechaza el compositor): ahí es donde se
        // pinta la primera vez, en `handlers::configure` -----
        if self.toast_layer.is_some() && !needs_resize {
            self.draw_notification_mode(qh);
        }
    }

    /// Superficie del toast: `Layer::Overlay` **anclada arriba a la derecha**, sin zona
    /// exclusiva, sin teclado y con la input region **vacía** (los clicks la atraviesan:
    /// el aviso se va solo con su timeout). Es descartable, como el popup.
    fn create_toast_surface(&mut self, panel_w: f32, panel_h: f32, qh: &QueueHandle<Self>) {
        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some("dockyrs-notify"),
            self.pinned_output.as_ref(),
        );
        const MARGIN: i32 = 8;
        layer.set_anchor(Anchor::TOP | Anchor::RIGHT);
        layer.set_margin(MARGIN, MARGIN, 0, 0);
        layer.set_size(
            panel_w.round().max(1.0) as u32,
            panel_h.round().max(1.0) as u32,
        );
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        if let Ok(region) = Region::new(&self.compositor) {
            // ----- el aviso se come el click en SU rectángulo: es lo que lo descarta.
            // La superficie mide justo eso, así que no intercepta nada de más -----
            region.add(
                0,
                0,
                panel_w.round().max(1.0) as i32,
                panel_h.round().max(1.0) as i32,
            );
            layer
                .wl_surface()
                .set_input_region(Some(region.wl_region()));
        }
        layer.commit();
        log::debug!("notify: toast {panel_w}x{panel_h} arriba a la derecha");
        self.toast_layer = Some(layer);
    }

    /// Cierra el toast. Es **inmediato a propósito**: antes se iba con un fade que
    /// dependía de los frame callbacks de la superficie, y si esos no llegaban el aviso
    /// se quedaba pegado en pantalla para siempre. El aviso dura su timeout y se suelta.
    pub(crate) fn close_notification_mode(&mut self, _qh: &QueueHandle<Self>) {
        if self.notification_mode.is_none() {
            return;
        }
        self.notification_mode = None;
        // ----- soltar la superficie la desmapea (es descartable, como el popup) -----
        self.toast_layer = None;
        trim_heap();
    }

    pub(super) fn draw_notification_mode(&mut self, qh: &QueueHandle<Self>) {
        let scale = self.output_scale.max(1) as f32;
        let Some(m) = self.notification_mode.as_mut() else {
            return;
        };
        let linear = m.anim.clamp(0.0, 1.0);
        let eased = (0.5 - 0.5 * (std::f32::consts::PI * linear).cos()).max(if m.closing {
            0.0
        } else {
            0.04
        });

        let width = (m.panel_w * scale).round() as i32;
        let height = (m.panel_h * scale).round() as i32;
        if width <= 0 || height <= 0 {
            return;
        }

        let args = menu_render::NotificationArgs {
            title: &m.title,
            body: &m.body,
            panel_w: m.panel_w,
            panel_h: m.panel_h,
            dock: &self.dock,
            // ----- el toast va en la esquina: siempre apaisado -----
            is_vertical: false,
            render_scale: scale,
        };
        let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32).unwrap();
        if !m.closing && eased >= 0.999 {
            menu_render::draw_notification(&mut pixmap, &mut self.text_cache, &args);
        } else {
            let mut content = tiny_skia::Pixmap::new(width as u32, height as u32).unwrap();
            menu_render::draw_notification(&mut content, &mut self.text_cache, &args);
            let paint = tiny_skia::PixmapPaint {
                opacity: eased,
                ..Default::default()
            };
            if m.closing {
                let (full_w, full_h) = (width as f32, height as f32);
                let (rw, rh) = (full_w * linear, full_h * linear);
                let rect = tiny_skia::Rect::from_xywh(
                    (full_w - rw) / 2.0,
                    (full_h - rh) / 2.0,
                    rw.max(0.0),
                    rh.max(0.0),
                );
                if let Some(rect) = rect {
                    let mut mask = tiny_skia::Mask::new(width as u32, height as u32).unwrap();
                    let path = tiny_skia::PathBuilder::from_rect(rect);
                    mask.fill_path(
                        &path,
                        tiny_skia::FillRule::Winding,
                        true,
                        tiny_skia::Transform::identity(),
                    );
                    pixmap.draw_pixmap(
                        0,
                        0,
                        content.as_ref(),
                        &paint,
                        tiny_skia::Transform::identity(),
                        Some(&mask),
                    );
                }
            } else {
                pixmap.draw_pixmap(
                    0,
                    0,
                    content.as_ref(),
                    &paint,
                    tiny_skia::Transform::identity(),
                    None,
                );
            }
        }

        let stride = width * 4;
        let (buffer, canvas) = self
            .pool
            .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
            .expect("failed to create shm buffer");
        bgra_from_rgba(pixmap.data(), canvas);

        // ----- el buffer va a la superficie DEL TOAST (no a la del dock) -----
        let Some(layer) = self.toast_layer.as_ref() else {
            return;
        };
        let surface = layer.wl_surface();
        surface.set_buffer_scale(self.output_scale.max(1));
        buffer.attach_to(surface).expect("failed to attach buffer");
        surface.damage_buffer(0, 0, width, height);
        surface.frame(qh, surface.clone());
        surface.commit();
    }

    pub(super) fn tick_notification_frame(&mut self, qh: &QueueHandle<Self>) {
        let Some(m) = self.notification_mode.as_mut() else {
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
        let closing = m.closing;
        let anim = m.anim;

        if closing && anim <= 0.0 {
            self.notification_mode = None;
            // ----- soltar la superficie del toast la desmapea: es descartable, no la
            // compartida del dock (esa sí es trampa 2) -----
            self.toast_layer = None;
            trim_heap();
            return;
        }
        if !animating {
            return;
        }
        self.draw_notification_mode(qh);
    }
}
