use super::*;

/// Plazo para ocultar el dock cuando el puntero sale de la franja, en vez del
/// delay configurable (1200 ms por defecto). Corto, pero con margen para que el
/// re-foco del compositor (que llega como Enter) lo cancele si fue un `Leave`
/// espurio.
pub(crate) const LEAVE_HIDE_MS: u64 = 150;

impl App {
    pub(super) fn draw(&mut self, qh: &QueueHandle<Self>) {
        self.draw_ex(qh, false, false);
    }

    pub(super) fn draw_ex(
        &mut self,
        qh: &QueueHandle<Self>,
        advance_marquee: bool,
        advance_ws: bool,
    ) {
        // ----- sin dibujar antes del primer configure (la escala puede llegar antes) -----
        if self.first_configure {
            return;
        }
        self.enforce_keyboard();
        // ----- se cerró el último modo que usaba la superficie: el estado del
        // puntero quedó viejo (lo capturó el modo, no el dock) y con `pointer_pos`
        // en `Some` el dock no se oculta nunca más. Se refresca como una salida: si
        // el cursor sigue sobre la franja, el compositor manda Enter/Motion y el
        // dock se queda. Cubre los tres cierres que no lo hacían (selector de
        // fondos, portapapeles y búsqueda) sin parchear cada uno. -----
        let mode_open = self.autohide_force_visible();
        if self.mode_was_open && !mode_open {
            self.dock.set_pointer(None);
            self.ptr_left_at = None;
            self.last_ptr_event = Some(std::time::Instant::now());
            self.autohide_armed = false;
            self.arm_autohide();
        }
        self.mode_was_open = mode_open;
        // ----- autohide: revelar si un modo necesita la superficie -----
        if self.dock.config.settings.autohide && !self.dock_visible && self.autohide_force_visible()
        {
            self.dock_visible = true;
            self.sync_autohide_surfaces();
        }
        // ----- oculto: pinta transparente (sin desmapear; attach(NULL)
        // resetea el tamaño de la layer surface en niri y rompe el remapeo) -----
        if self.dock.config.settings.autohide && !self.dock_visible && self.ws_flash_mode.is_none()
        {
            self.draw_hidden();
            return;
        }
        // ----- auto-armar ocultado una sola vez por episodio visible -----
        if self.dock.config.settings.autohide && self.should_hide() && !self.autohide_armed {
            self.arm_autohide();
        }
        if self.notification_mode.is_some() {
            self.draw_notification_mode(qh);
        } else if self.osd_mode.is_some() {
            self.draw_osd_mode(qh);
        } else if self.ws_flash_mode.is_some() {
            self.draw_ws_flash_mode(qh);
        } else if self.app_search_mode.is_some() {
            self.draw_app_search_mode(qh);
        } else if self.dock_menu_mode.is_some() {
            self.draw_dock_menu_mode(qh);
        } else if self.wallpaper_mode.is_some() {
            self.draw_wallpaper_mode(qh);
        } else if self.clipboard_mode.is_some() {
            self.draw_clipboard_mode(qh);
        } else {
            self.draw_icons(qh, advance_marquee, advance_ws);
        }
    }

    pub(crate) fn tick_marquee(&mut self, qh: &QueueHandle<Self>) {
        self.draw_ex(qh, true, false);
    }

    pub(crate) fn tick_workspace(&mut self, qh: &QueueHandle<Self>) {
        self.draw_ex(qh, false, true);
    }

    // ----- idle throttle -----
    pub(super) fn set_marquee_rate(&mut self, rate: u64) {
        if rate != self.marquee_rate {
            self.marquee_rate = rate;
            let _ = self.marquee_tick_tx.send(rate);
        }
    }

    // ----- buffer transparente del mismo tamaño: mantiene la superficie mapeada -----
    pub(super) fn draw_hidden(&mut self) {
        let scale = self.output_scale.max(1) as f32;
        let (base_w, base_h) = self.dock.base_size();
        let width = (base_w as f32 * scale).round() as i32;
        let height = (base_h as f32 * scale).round() as i32;
        if width <= 0 || height <= 0 {
            return;
        }
        let stride = width * 4;
        // ----- sin /dev/shm o sin fds `create_buffer` falla, y esto corre en CADA
        // ocultado: paniquear acá mataba el dock estando invisible y no se
        // recuperaba. Ante el fallo dejamos el buffer viejo puesto (la superficie
        // sigue mapeada) en vez de morir. -----
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
        else {
            log::error!("no pude crear el buffer oculto ({width}x{height})");
            return;
        };
        canvas.fill(0);
        let surface = self.layer.wl_surface();
        surface.set_buffer_scale(self.output_scale.max(1));
        if let Err(err) = buffer.attach_to(surface) {
            log::error!("no pude mapear el buffer oculto: {err}");
            return;
        }
        surface.damage_buffer(0, 0, width, height);
        surface.commit();
        self.awaiting_frame = false;
    }

    pub(super) fn draw_icons(
        &mut self,
        qh: &QueueHandle<Self>,
        advance_marquee: bool,
        advance_ws: bool,
    ) {
        log::debug!(
            "autohide:{} paint visible={} awaiting={}",
            crate::app::hdbg_ms(),
            self.dock_visible,
            self.awaiting_frame
        );
        let scale = self.output_scale.max(1) as f32;
        let (base_w, base_h) = self.dock.base_size();
        let width = (base_w as f32 * scale).round() as i32;
        let height = (base_h as f32 * scale).round() as i32;
        if width <= 0 || height <= 0 {
            return;
        }
        let stride = width * 4;

        let (buffer, canvas) = self
            .pool
            .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
            .expect("failed to create shm buffer");

        // ----- reused buffer -----
        let pixmap = match &mut self.frame_pixmap {
            Some(p) if p.width() == width as u32 && p.height() == height as u32 => p,
            slot => {
                *slot = Some(tiny_skia::Pixmap::new(width as u32, height as u32).unwrap());
                slot.as_mut().unwrap()
            }
        };
        let tray_icons = self.tray.lock().unwrap();
        let needs_marquee = render::draw(
            pixmap,
            &self.dock,
            &mut self.icon_cache,
            &mut self.text_cache,
            &self.widgets,
            &tray_icons,
            &mut self.marquee,
            advance_marquee,
            advance_ws,
            scale,
        );
        drop(tray_icons);
        bgra_from_rgba(pixmap.data(), canvas);
        let rate: u64 = if needs_marquee {
            render::MARQUEE_TICK_MS
        } else {
            0
        };
        self.set_marquee_rate(rate);

        let surface = self.layer.wl_surface();
        surface.set_buffer_scale(self.output_scale.max(1));
        buffer.attach_to(surface).expect("failed to attach buffer");
        surface.damage_buffer(0, 0, width, height);
        surface.frame(qh, surface.clone());
        self.awaiting_frame = true;
        surface.commit();
    }

    pub(crate) fn request_redraw(&mut self, qh: &QueueHandle<Self>) {
        if !self.awaiting_frame {
            self.draw(qh);
        }
    }

    // ----- layer borrowed -----
    pub(crate) fn layer_is_borrowed(&self) -> bool {
        self.dock_menu_mode.is_some()
            || self.osd_mode.is_some()
            || self.ws_flash_mode.is_some()
            || self.wallpaper_mode.is_some()
            || self.notification_mode.is_some()
            || self.app_search_mode.is_some()
            || self.clipboard_mode.is_some()
    }

    pub(super) fn relayout_dock(&mut self, qh: &QueueHandle<Self>) {
        self.sync_widget_bar_len();
        self.dock.relayout();
        if !self.layer_is_borrowed() {
            let (w, h) = self.dock.base_size();
            // ----- sólo si cambió: set_size dispara un configure del compositor
            // que puede quitar el foco del puntero (y ese foco es lo que mantiene
            // el dock visible con autohide) -----
            if self.applied_size != Some((w, h)) {
                self.layer.set_size(w, h);
                self.applied_size = Some((w, h));
            }
            self.sync_autohide_surfaces();
        }
        self.request_redraw(qh);
    }
    // ================= autohide dinámico =================
    /// Modos que además exigen el dock VISIBLE. El HUD de workspaces no: usa la
    /// superficie sin que el dock tenga que mostrarse.
    fn forces_dock_visible(&self) -> bool {
        self.dock_menu_mode.is_some()
            || self.osd_mode.is_some()
            || self.wallpaper_mode.is_some()
            || self.notification_mode.is_some()
            || self.app_search_mode.is_some()
            || self.clipboard_mode.is_some()
    }

    fn autohide_force_visible(&self) -> bool {
        self.forces_dock_visible() || self.menu.is_some() || self.popup_mode.is_some()
    }

    /// El puntero se fue del dock y no está en el menú: el menú se cierra. Mientras
    /// `popup_hovered` sea true el salto icono -> menú está en curso y no se cierra.
    fn popup_should_dismiss(&self) -> bool {
        self.popup_mode.is_some()
            && Self::popup_dismiss_due(
                self.popup_mode.as_ref().is_some_and(|p| p.popup_hovered),
                self.dock.pointer_pos.is_some(),
                self.ptr_left_at.map(|t| t.elapsed().as_millis()),
            )
    }

    // ----- con un popup abierto `draw_ex` no corre: el único tick es el del
    // autohide. Se agenda a mano para que el cierre del menú no dependa de que el
    // ocultado automático esté encendido. -----
    pub(crate) fn arm_popup_tick(&mut self) {
        let _ = self.autohide_hide_tx.send(LEAVE_HIDE_MS);
    }

    /// ¿Ya toca cerrar el menú? El puntero tiene que haberse ido del dock y no estar
    /// dentro del menú, y tiene que haber pasado el plazo corto: si no, el salto
    /// icono -> menú (Leave + Enter en el mismo lote de eventos) lo cerraría al pasar.
    fn popup_dismiss_due(popup_hovered: bool, ptr_on_dock: bool, left_ms: Option<u128>) -> bool {
        !popup_hovered && !ptr_on_dock && left_ms.is_some_and(|ms| ms >= LEAVE_HIDE_MS as u128)
    }

    fn should_hide(&self) -> bool {
        // El foco del puntero se pierde (Leave) cuando el compositor reconfigura
        // la superficie, y en el borde exacto el hit-test oscila. Por eso no basta
        // con pointer_pos: se exige además que no haya habido actividad reciente.
        let quieto = self.last_ptr_event.is_none_or(|t| {
            t.elapsed()
                >= std::time::Duration::from_millis(self.dock.config.settings.autohide_delay_ms)
        });
        // ----- salir de la franja oculta enseguida. `ptr_left_at` lo limpia
        // cualquier Enter/Motion, así que un `Leave` espurio (reconfiguración de
        // la superficie, oscilación en el borde) se cancela antes de vencer y la
        // salida real oculta sin esperar el delay configurable. -----
        let salio = self
            .ptr_left_at
            .is_some_and(|t| t.elapsed() >= std::time::Duration::from_millis(LEAVE_HIDE_MS));
        self.dock.config.settings.autohide
            && self.dock_visible
            && (quieto || salio)
            && self.dock.pointer_pos.is_none()
            && !self.pointer_down
            && !self.autohide_force_visible()
    }

    pub(crate) fn arm_autohide(&mut self) {
        let ms = self.dock.config.settings.autohide_delay_ms;
        self.arm_autohide_after(ms);
    }

    /// Arma el ocultado en `ms` milisegundos. Salir de la franja usa un plazo
    /// corto (LEAVE_HIDE_MS); el resto de los casos, el delay configurable.
    pub(crate) fn arm_autohide_after(&mut self, ms: u64) {
        if !self.dock.config.settings.autohide || !self.dock_visible {
            return;
        }
        self.autohide_armed = true;
        let _ = self.autohide_hide_tx.send(ms);
    }

    pub(crate) fn reveal_dock(&mut self, qh: &QueueHandle<Self>) {
        if !self.dock.config.settings.autohide || self.dock_visible {
            return;
        }
        // ----- el HUD de workspaces comparte la superficie, y el despacho de
        // dibujo lo prefiere a él: si no se cierra acá, el dock queda revelado
        // por dentro pero se sigue viendo sólo el indicador en lugar del dock
        // completo. -----
        if self.ws_flash_mode.is_some() {
            self.ws_flash_mode = None;
            self.layer.set_layer(Layer::Top);
            // ----- el HUD dejó la superficie con SU tamaño (y lo anotó en
            // applied_size), y sync_autohide_surfaces no toca el tamaño: sin este
            // relayout el dock completo se dibuja dentro de la pastilla chica y
            // aparece fuera de lugar. -----
            self.relayout_dock(qh);
        }
        self.dock_visible = true;
        self.autohide_armed = false;
        log::debug!("autohide:{} reveal", crate::app::hdbg_ms());
        self.sync_autohide_surfaces();
        // ----- ventana garantizada para llegar al dock -----
        self.arm_autohide();
        self.request_redraw(qh);
    }

    fn set_dock_visible(&mut self, visible: bool) {
        if self.dock_visible == visible {
            return;
        }
        log::debug!(
            "autohide:{} {}",
            crate::app::hdbg_ms(),
            if visible { "show" } else { "hide" }
        );
        self.dock_visible = visible;
        // ----- al soltar el buffer no llegará callback de frame -----
        // (si queda en true, request_redraw se vuelve un no-op y el dock no reaparece)
        self.awaiting_frame = false;
        if !visible {
            self.autohide_armed = false;
            self.dock.set_pointer(None);
            self.set_marquee_rate(0);
        }
        self.sync_autohide_surfaces();
    }

    pub(crate) fn autohide_timeout(&mut self, qh: &QueueHandle<Self>) {
        self.autohide_armed = false;
        log::debug!(
            "autohide:{} timeout visible={} ptr={:?} down={} borrowed={} menu={} popup={}",
            crate::app::hdbg_ms(),
            self.dock_visible,
            self.dock.pointer_pos,
            self.pointer_down,
            self.layer_is_borrowed(),
            self.menu.is_some(),
            self.popup_mode.is_some()
        );
        // ----- el menú se cierra cuando el puntero no está ni en el dock ni en él:
        // antes se cerraba sólo con un click, y ese click lo consumía, así que el
        // menú "no aparecía" a la primera y quedaba abierto para siempre. -----
        if self.popup_should_dismiss() {
            self.close_popup_mode(qh);
        }
        if self.should_hide() {
            self.set_dock_visible(false);
            // ----- el contenido visible sigue enganchado: sustituirlo por transparente -----
            self.draw_hidden();
        } else if self.dock_visible {
            // ----- re-verificar más tarde (el puntero puede seguir encima) -----
            self.arm_autohide();
        }
    }

    pub(crate) fn sync_autohide_surfaces(&mut self) {
        // ----- copiar ajustes primero para no pelear préstamos -----
        let edge = self.dock.config.settings.dock_edge;
        let align = self.dock.config.settings.dock_align;
        let pos_y = self.dock.config.settings.pos_y;
        let autohide = self.dock.config.settings.autohide;
        // ----- con autohide desactivado el dock queda fijo -----
        if !autohide {
            self.dock_visible = true;
        }
        // ----- IDEMPOTENTE: cualquier set_* de layer-shell dispara un configure
        // que puede hacer perder el foco del puntero, y ese foco es justo lo que
        // mantiene el dock visible -----
        let (anchor, margin) = edge_anchor_margin(edge, align, pos_y, 0);
        if self.applied_geom != Some((anchor, margin)) {
            self.layer.set_anchor(anchor);
            self.layer
                .set_margin(margin.0, margin.1, margin.2, margin.3);
            self.applied_geom = Some((anchor, margin));
        }
        // ----- input region SIEMPRE activa: es el disparador del autohide y
        // toglearla no surte efecto hasta que el puntero se mueve -----
        self.layer.wl_surface().set_input_region(None);
        self.layer.commit();
    }

    pub(crate) fn widget_placed(&self, kind: crate::config::WidgetKind) -> bool {
        self.dock
            .config
            .settings
            .widgets
            .iter()
            .any(|w| w.kind == kind)
    }

    pub(crate) fn refresh_clock(&mut self, qh: &QueueHandle<Self>) {
        if !self.dock.icons.is_empty() || !self.widget_placed(crate::config::WidgetKind::Clock) {
            return;
        }
        if !self.widgets.refresh_clock() {
            return;
        }
        self.sync_widget_bar_len();
        self.relayout_dock(qh);
    }

    pub(crate) fn refresh_media(&mut self, qh: &QueueHandle<Self>) {
        if !self.dock.icons.is_empty() || !self.widget_placed(crate::config::WidgetKind::Media) {
            return;
        }
        self.widgets.refresh_media();
        // ----- fixed text budget -----
        self.request_redraw(qh);
    }

    pub(crate) fn refresh_battery(&mut self, qh: &QueueHandle<Self>) {
        if !self.dock.icons.is_empty() || !self.widget_placed(crate::config::WidgetKind::Battery) {
            return;
        }
        if !self.widgets.refresh_battery() {
            return;
        }
        self.sync_widget_bar_len();
        self.relayout_dock(qh);
    }

    pub(crate) fn refresh_bluetooth(&mut self, qh: &QueueHandle<Self>) {
        if !self.dock.icons.is_empty() || !self.widget_placed(crate::config::WidgetKind::Bluetooth)
        {
            return;
        }
        self.widgets.refresh_bluetooth();
        self.sync_widget_bar_len();
        self.relayout_dock(qh);
    }

    pub(crate) fn refresh_workspaces(&mut self, qh: &QueueHandle<Self>) {
        if self.dock.icons.is_empty() {
            let before = render::ws_target_for(&self.widgets.workspaces);
            self.widgets.refresh_workspaces();
            let after = render::ws_target_for(&self.widgets.workspaces);
            self.marquee.track_ws_target(after);
            self.sync_widget_bar_len();
            self.relayout_dock(qh);
            // ----- sólo si el espacio activo cambió de verdad -----
            log::debug!("wsflash: refresh before={before} after={after}");
            if after != before {
                self.show_ws_flash(qh);
            }
        }
    }

    pub(crate) fn refresh_cpu_ram(&mut self, qh: &QueueHandle<Self>) {
        let active = self.dock.config.settings.widgets.iter().any(|w| {
            matches!(
                w.kind,
                crate::config::WidgetKind::Cpu | crate::config::WidgetKind::Ram
            )
        });
        if !active {
            return;
        }
        self.widgets.refresh_cpu_ram();
        self.request_redraw(qh);
    }

    pub(crate) fn refresh_custom(&mut self, qh: &QueueHandle<Self>) {
        // ----- el tick de sistema corre siempre, incluso oculto: un script
        // externo no se lanza sin un `Custom` colocado ni con el dock oculto -----
        let placed = self
            .dock
            .config
            .settings
            .widgets
            .iter()
            .any(|w| matches!(w.kind, crate::config::WidgetKind::Custom(_)));
        if !placed || !self.dock_visible {
            return;
        }
        let changed = self.widgets.refresh_custom(
            &self.dock.config.settings.custom_widgets,
            std::time::Instant::now(),
        );
        if changed {
            self.sync_widget_bar_len();
            self.relayout_dock(qh);
        }
    }

    pub(crate) fn refresh_sys(&mut self, qh: &QueueHandle<Self>) {
        let active = self.dock.config.settings.widgets.iter().any(|w| {
            matches!(
                w.kind,
                crate::config::WidgetKind::Volume
                    | crate::config::WidgetKind::Network
                    | crate::config::WidgetKind::KbdLayout
            )
        });
        if !active {
            return;
        }
        // ----- el panel de volumen relee sus streams aunque el widget no haya
        // cambiado: una app puede empezar o dejar de sonar sin que wpctl cambie -----
        self.refresh_volume_panel(qh);
        if self.widgets.refresh_sys() {
            self.sync_widget_bar_len();
            self.relayout_dock(qh);
        }
    }

    pub(crate) fn sync_tray_layout(&mut self, qh: &QueueHandle<Self>) {
        let count = self.tray.lock().unwrap().len();
        if count != self.last_tray_count {
            self.last_tray_count = count;
            self.sync_widget_bar_len();
            self.relayout_dock(qh);
        } else {
            self.request_redraw(qh);
        }
    }

    pub(crate) fn sync_widget_bar_len(&mut self) {
        let tray_count = self.tray.lock().unwrap().len();
        let is_vertical = self.dock.is_vertical();
        let cross_len = self.dock.cross_len();
        let widget_scale = self.dock.config.settings.widget_scale;
        self.dock.widget_bar_content_len = render::widget_bar_natural_len(
            &self.dock.config.settings,
            &self.widgets,
            tray_count,
            is_vertical,
            cross_len,
            widget_scale,
        );
    }
}

#[cfg(test)]
mod popup_dismiss_tests {
    // ----- el cierre del menú costó dos rondas de depuración: este test fija el
    // contrato del caso que lo rompía (el salto icono -> menú). -----
    use super::App;

    const PLAZO: u128 = 150;

    #[test]
    fn el_salto_icono_menu_no_lo_cierra() {
        // Leave del dock + Enter en el menú en el mismo lote de eventos: ya no está
        // en el dock pero sí en el menú, y así tiene que quedarse.
        assert!(!App::popup_dismiss_due(true, false, Some(30_000)));
        assert!(!App::popup_dismiss_due(true, false, None));
    }

    #[test]
    fn irse_lejos_lo_cierra() {
        assert!(App::popup_dismiss_due(false, false, Some(PLAZO)));
        assert!(App::popup_dismiss_due(false, false, Some(2_000)));
    }

    #[test]
    fn volver_al_dock_o_antes_del_plazo_no_lo_cierra() {
        assert!(!App::popup_dismiss_due(false, true, Some(2_000)));
        assert!(!App::popup_dismiss_due(false, false, Some(PLAZO - 1)));
        // sin Leave registrado tampoco (el puntero nunca dejó el dock)
        assert!(!App::popup_dismiss_due(false, false, None));
    }
}
