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
        // ----- el catcher de clicks afuera sigue al panel abierto: se crea al
        // abrir, recalcula el agujero cuando cambia el alto (cambio de pestaña) y
        // se desmapea al cerrar. Ver `click_catcher.rs`. -----
        self.sync_click_catcher(qh);
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
            self.set_dock_visible(true);
        }
        // ----- o si el workspace activo está vacío o el Overview está abierto (no
        // hay ventana que justifique ocultarlo): misma regla que aplica el refresco
        // de workspaces -----
        self.reveal_dock_if_stays(qh);
        // ----- oculto: pinta transparente (sin desmapear; attach(NULL)
        // resetea el tamaño de la layer surface en niri y rompe el remapeo) -----
        // Con el colapso animado a medias el transparente todavía no va: la cápsula
        // que se encoge se sigue pintando hasta llegar a 0 y ahí pasa a la isla.
        if self.dock.config.settings.autohide
            && !self.dock_visible
            && self.ws_flash_mode.is_none()
            && self.reveal_anim <= 0.0
        {
            self.draw_island(qh);
            return;
        }
        // ----- auto-armar ocultado una sola vez por episodio visible -----
        if self.dock.config.settings.autohide && self.should_hide() && !self.autohide_armed {
            self.arm_autohide();
        }
        // ----- el toast de notificaciones NO se dibuja acá: tiene superficie propia
        // (arriba a la derecha), así que el dock sigue mostrando lo suyo -----
        if self.osd_mode.is_some() {
            self.draw_osd_mode(qh);
        } else if self.ws_flash_mode.is_some() {
            self.draw_ws_flash_mode(qh);
        } else if self.app_search_mode.is_some() {
            self.draw_app_search_mode(qh);
        } else if self.dock_menu_mode.is_some() {
            self.draw_dock_menu_mode(qh);
        } else if self.wallpaper_mode.is_some() {
            self.draw_wallpaper_mode(qh);
        } else if self.notifications_mode.is_some() {
            self.draw_notifications_mode(qh);
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

    /// Estado de la animación de aparición. Con el autohide apagado el dock queda
    /// fijo, así que la isla va entera siempre: ningún ajuste puede dejarlo
    /// colapsado a medias (apagar el autohide desde el panel pone `dock_visible` en
    /// true sin pasar por la animación).
    fn dock_reveal(&self) -> f32 {
        if self.dock.config.settings.autohide {
            self.reveal_anim
        } else {
            1.0
        }
    }

    /// Paso del split de la isla: el indicador de workspaces abriéndose en el medio.
    /// Mismo criterio que el reveal (el frame que se pide al dibujar es el reloj) y
    /// mismo gate de `smooth_transitions`. Devuelve true si dibujó.
    pub(super) fn tick_island_split_frame(&mut self, qh: &QueueHandle<Self>) -> bool {
        if self.island_ws_split == self.island_ws_target {
            return false;
        }
        self.island_ws_split = if !self.dock.config.settings.smooth_transitions {
            self.island_ws_target
        } else if self.island_ws_split < self.island_ws_target {
            (self.island_ws_split + menu::WS_SPLIT_STEP_OPEN).min(self.island_ws_target)
        } else {
            (self.island_ws_split - menu::WS_SPLIT_STEP_CLOSE).max(self.island_ws_target)
        };
        if self.dock_visible {
            self.draw(qh);
        } else {
            self.draw_island(qh);
        }
        true
    }

    /// Paso de la animación de aparición (estilo isla). La llama el callback de
    /// frame de la superficie del dock: el frame que se pide al dibujar es el reloj
    /// de la animación, igual que en los otros modos. Devuelve true si dibujó.
    pub(super) fn tick_reveal_frame(&mut self, qh: &QueueHandle<Self>) -> bool {
        if self.reveal_anim == self.reveal_target {
            return false;
        }
        let smooth = self.dock.config.settings.smooth_transitions;
        let antes = self.reveal_anim;
        self.reveal_anim = if !smooth {
            self.reveal_target
        } else if self.reveal_anim < self.reveal_target {
            (self.reveal_anim + menu::ANIM_STEP_OPEN).min(self.reveal_target)
        } else {
            (self.reveal_anim - menu::ANIM_STEP_CLOSE).max(self.reveal_target)
        };
        log::debug!(
            "reveal:{} {:.2} -> {:.2} visible={}",
            crate::app::hdbg_ms(),
            antes,
            self.reveal_anim,
            self.dock_visible
        );
        // ----- el final del colapso es la isla compacta, no el buffer transparente:
        // el dock no desaparece, se encoge al blob -----
        if self.reveal_anim <= 0.0 {
            self.draw_island(qh);
        } else {
            self.draw(qh);
        }
        true
    }

    // ----- idle throttle -----
    pub(super) fn set_marquee_rate(&mut self, rate: u64) {
        if rate != self.marquee_rate {
            self.marquee_rate = rate;
            let _ = self.marquee_tick_tx.send(rate);
        }
    }

    /// La isla compacta: el estado que se ve MIENTRAS el dock está oculto (ver
    /// `render::draw_island`). Reusa la superficie y el buffer del dock, así que no
    /// agrega ni una superficie ni un wakeup: es el mismo `attach` que hacía
    /// `draw_hidden` con el blob de una actividad adentro.
    ///
    /// Sin actividad (ni media sonando, ni volumen, ni batería) NO se inventa un
    /// blob vacío: queda el buffer transparente de siempre.
    pub(super) fn draw_island(&mut self, qh: &QueueHandle<Self>) {
        if render::island_activities(&self.widgets, self.island_ws_split).is_empty() {
            self.draw_hidden();
            return;
        }
        let scale = self.output_scale.max(1) as f32;
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
            // ----- mismo criterio que `draw_hidden`: si falla, queda el buffer viejo
            // puesto (la superficie sigue mapeada) en vez de morir -----
            log::error!("no pude crear el buffer de la isla ({width}x{height})");
            return;
        };
        // ----- mismo criterio que `draw_hidden`: nada de `unwrap` en un camino que
        // corre en cada redibujado con el dock oculto (un panic mata el dock) -----
        if self.frame_pixmap.as_ref().map(|p| (p.width(), p.height()))
            != Some((width as u32, height as u32))
        {
            match tiny_skia::Pixmap::new(width as u32, height as u32) {
                Some(p) => self.frame_pixmap = Some(p),
                None => {
                    log::error!("no pude crear el pixmap de la isla ({width}x{height})");
                    return;
                }
            }
        }
        let Some(pixmap) = self.frame_pixmap.as_mut() else {
            return;
        };
        let animando = {
            let tray_icons = self.tray.lock().unwrap();
            render::draw_island(
                pixmap,
                &self.dock,
                &mut self.icon_cache,
                &mut self.text_cache,
                &self.widgets,
                &tray_icons,
                &mut self.marquee,
                scale,
                self.island_ws_split,
            )
        };
        bgra_from_rgba(pixmap.data(), canvas);
        // ----- si el título de Media está scrolleando, el tick del marquee es el reloj
        // de la isla; si no, 0 y el reposo queda en el piso (mismo criterio que
        // `draw_icons`). Va después de `bgra_from_rgba` porque ese consume el préstamo
        // del pool. -----
        self.set_marquee_rate(if animando { render::MARQUEE_TICK_MS } else { 0 });
        let surface = self.layer.wl_surface();
        surface.set_buffer_scale(self.output_scale.max(1));
        if let Err(err) = buffer.attach_to(surface) {
            log::error!("no pude mapear el buffer de la isla: {err}");
            return;
        }
        surface.damage_buffer(0, 0, width, height);
        surface.frame(qh, surface.clone());
        self.awaiting_frame = true;
        surface.commit();
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
        // ----- la animación de aparición se lee ANTES de pedir prestado `self.pool`
        // (el préstamo mutable vive hasta el `bgra_from_rgba` del final y no deja
        // llamar a un método &self en el medio) -----
        let reveal = self.dock_reveal();
        // ----- el split de la isla (indicador de workspaces) viaja con el dibujo -----
        let ws_split = self.island_ws_split;
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
            reveal,
            ws_split,
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
            || self.app_search_mode.is_some()
            || self.clipboard_mode.is_some()
            || self.notifications_mode.is_some()
    }

    pub(super) fn relayout_dock(&mut self, qh: &QueueHandle<Self>) {
        self.sync_widget_bar_len();
        self.dock.relayout();
        // ----- el panel de ajustes comparte la superficie y su tamaño sale del
        // ancho del dock: si cambia un ajuste de layout (Dock Width, Dock Size,
        // …) hay que RE-APLICAR el reparto. Antes se salteaba por estar la
        // superficie prestada: el buffer quedaba más ancho que la superficie, el
        // compositor mostraba su borde izquierdo y el dock se veía descentrado
        // (y el panel, corrido respecto de sus hit tests). -----
        if let Some((panel_w, panel_h)) = self
            .dock_menu_mode
            .as_ref()
            .map(|dm| (dm.panel_w, dm.panel_h))
        {
            self.apply_panel_size(panel_w, panel_h);
        } else if !self.layer_is_borrowed() {
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
            || self.app_search_mode.is_some()
            || self.clipboard_mode.is_some()
            || self.notifications_mode.is_some()
    }

    fn autohide_force_visible(&self) -> bool {
        self.forces_dock_visible() || self.menu.is_some() || self.popup_mode.is_some()
    }

    /// El workspace activo no tiene ventanas: el dock se queda en pantalla.
    fn empty_workspace(&self) -> bool {
        render::active_is_empty(&self.widgets.workspaces)
    }

    /// El dock se queda en pantalla aunque no haya puntero ni modo abierto: el
    /// workspace activo está vacío o el Overview de niri está abierto. Una sola
    /// definición para el reveal y para `should_hide`: si se despegan, el dock se
    /// revela y se oculta en el mismo frame.
    fn dock_stays_visible(&self) -> bool {
        self.empty_workspace() || self.overview_open
    }

    /// Reveal del dock por la regla de `dock_stays_visible`. Lo aplica también el
    /// refresco de workspaces, no sólo el dibujo: `request_redraw` se saltea si hay un
    /// frame pendiente, y `show_ws_flash` decide con `dock_visible` en el mismo bloque,
    /// así que si no el HUD alcanzaba a abrirse y el dock recién se revelaba con el frame
    /// siguiente (o el HUD quedaba clavado hasta su timeout).
    fn reveal_dock_if_stays(&mut self, qh: &QueueHandle<Self>) {
        // ----- no pisar un modo abierto: él ya tiene la superficie, y puede estar
        // en `Overlay`, que `close_ws_flash_for_dock` bajaría a `Top` (los modos
        // fijan su layer sólo al abrirse, no en cada frame). -----
        if !self.dock.config.settings.autohide
            || self.dock_visible
            || !self.dock_stays_visible()
            || self.autohide_force_visible()
        {
            return;
        }
        // ----- el HUD comparte la superficie: devolvérsela al dock, o el dock
        // entero se pinta dentro de la pastilla chica del HUD. -----
        self.close_ws_flash_for_dock(qh);
        self.set_dock_visible(true);
        self.sync_autohide_surfaces();
    }

    /// El Overview de niri se abrió o se cerró (llega por el event-stream). Abierto
    /// el dock queda fijo, y al cerrarse vuelve la regla normal (ocultado inmediato
    /// incluido si corresponde). Mismo patrón que el workspace vacío, que se resuelve
    /// en `refresh_workspaces` y no en el dibujo: `request_redraw` se saltea si hay un
    /// frame pendiente.
    pub(crate) fn set_overview_open(&mut self, open: bool, qh: &QueueHandle<Self>) {
        if self.overview_open == open {
            return;
        }
        self.overview_open = open;
        log::debug!("overview: abierto={open}");
        if open {
            self.reveal_dock_if_stays(qh);
        } else if self.should_hide() {
            self.set_dock_visible(false);
        }
        self.request_redraw(qh);
    }

    /// El HUD de workspaces comparte la superficie del dock: al revelar el dock hay
    /// que devolverle la layer y el tamaño (el HUD los dejó con los suyos, y
    /// `sync_autohide_surfaces` no toca el tamaño). Sin esto el dock se pinta dentro
    /// de la pastilla chica del HUD. No hace nada si el HUD no estaba abierto.
    fn close_ws_flash_for_dock(&mut self, qh: &QueueHandle<Self>) {
        // ----- el split de la isla también se cierra al revelar el dock: ahí el
        // indicador ya está en la barra -----
        self.island_ws_target = 0.0;
        if self.ws_flash_mode.is_none() {
            return;
        }
        self.ws_flash_mode = None;
        self.layer.set_layer(Layer::Top);
        self.relayout_dock(qh);
    }

    /// El puntero se fue del dock y no está en el menú: el menú se cierra. Mientras
    /// `popup_hovered` sea true el salto icono -> menú está en curso y no se cierra.
    ///
    /// El calendario es el caso especial: su objetivo es el RELOJ, no el dock, así
    /// que quedarse sobre otro widget de la barra no lo mantiene abierto (lo cierra
    /// el mismo plazo corto, que `note_calendar_hover` arma al dejar el widget).
    fn popup_should_dismiss(&self) -> bool {
        let Some(p) = self.popup_mode.as_ref() else {
            return false;
        };
        let ptr_on_target = if p.screen == menu::MenuScreen::Calendar {
            self.pointer_over_clock()
        } else {
            self.dock.pointer_pos.is_some()
        };
        Self::popup_dismiss_due(
            p.popup_hovered,
            ptr_on_target,
            self.ptr_left_at.map(|t| t.elapsed().as_millis()),
        )
    }

    // ----- con un popup abierto `draw_ex` no corre: el único tick es el del
    // autohide. Se agenda a mano para que el cierre del menú no dependa de que el
    // ocultado automático esté encendido. -----
    pub(crate) fn arm_popup_tick(&mut self) {
        let _ = self.autohide_hide_tx.send(LEAVE_HIDE_MS);
    }

    /// Despierta el tick para revisar el hover del reloj: el canal es el mismo del
    /// autohide, así que sirve con el autohide apagado (el envío es directo).
    pub(crate) fn arm_calendar_tick(&mut self) {
        let _ = self
            .autohide_hide_tx
            .send(super::calendar::CALENDAR_HOVER_MS);
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
            && !self.dock_stays_visible()
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
        // ----- el HUD de workspaces comparte la superficie del dock, y el despacho
        // de dibujo lo prefiere a él: si no se cierra acá, el dock queda revelado
        // por dentro pero se sigue viendo sólo el indicador en lugar del dock
        // completo. -----
        self.close_ws_flash_for_dock(qh);
        self.set_dock_visible(true);
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
        // ----- el destino de la animación de aparición. Con las transiciones
        // apagadas no hay animación: el estado final ya es el destino. -----
        self.reveal_target = if visible { 1.0 } else { 0.0 };
        if !self.dock.config.settings.smooth_transitions {
            self.reveal_anim = self.reveal_target;
        }
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
        // ----- hover sobre el reloj: el panel del calendario se abre cuando el
        // puntero ya se quedó el plazo corto (el tick es el mismo del autohide, y
        // `arm_calendar_tick` lo agenda aunque el autohide esté apagado). -----
        if self.calendar_hover_due() {
            self.calendar_hover_at = None;
            self.open_calendar(qh);
        }
        if self.should_hide() {
            self.set_dock_visible(false);
            // ----- el contenido visible sigue enganchado: sustituirlo por
            // transparente. Con el colapso animado corriendo, el frame siguiente ya
            // lo dibuja y el transparente llega cuando la cápsula termina de
            // encogerse (`tick_reveal_frame`). -----
            if self.reveal_anim > 0.0 {
                self.request_redraw(qh);
            } else {
                self.draw_island(qh);
            }
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
        // ----- el disparador del autohide: VISIBLE la superficie entera; OCULTO
        // sólo el blob de la isla, que es lo único que se ve (pedido: "que aparezca
        // solamente al hacer hover en la isla"). Fuera del blob el puntero le llega a
        // la ventana de abajo, así que el dock ya no se come una franja de 26x610 en
        // toda la altura de la pantalla. Sin isla (sin datos) queda la superficie
        // entera, si no el dock no se podría revelar nunca. -----
        let tray_count = self.tray.lock().unwrap().len();
        let isla =
            render::island_blob_region(&self.dock, &self.widgets, tray_count, self.island_ws_split);
        let want: Option<(i32, i32, i32, i32)> = if self.dock_visible { None } else { isla };
        // ----- idempotente: re-setear la región en cada frame es un `commit` de más -----
        if self.applied_input != Some(want) {
            match want {
                None => self.layer.wl_surface().set_input_region(None),
                Some((x, y, w, h)) => {
                    if let Ok(region) = Region::new(&self.compositor) {
                        region.add(x, y, w, h);
                        self.layer
                            .wl_surface()
                            .set_input_region(Some(region.wl_region()));
                    }
                }
            }
            self.applied_input = Some(want);
        }
        self.layer.commit();
    }

    pub(crate) fn widget_placed(&self, kind: crate::config::WidgetKind) -> bool {
        self.dock.config.settings.has_widget(kind)
    }

    /// Republica lo que los watchers que lanzan un proceso residente necesitan
    /// saber: hoy, si hay un widget Media (el `playerctl` de ~7 MB). Se llama donde
    /// cambian los widgets (hoy el único sitio es `drop_widget_chip`).
    pub(crate) fn publish_watcher_wants(&self) {
        self.media_wanted.store(
            self.dock
                .config
                .settings
                .has_widget(crate::config::WidgetKind::Media),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    /// Llega del hilo que lee en background al arrancar (batería, media,
    /// bluetooth, volumen). NO pasa por los `refresh_*`: esos releen el sistema, y
    /// el punto de este camino es que el dato ya viene leído. Tampoco toca
    /// workspaces, red ni layout de teclado, así que no hay HUD ni regla de
    /// workspace vacío de por medio.
    pub(crate) fn apply_deferred_widgets(
        &mut self,
        deferred: crate::widgets::DeferredWidgets,
        qh: &QueueHandle<Self>,
    ) {
        self.widgets.apply_deferred(deferred);
        self.relayout_dock(qh);
    }

    pub(crate) fn refresh_clock(&mut self, qh: &QueueHandle<Self>) {
        // ----- el reintento del throttle de los workspaces: si una ráfaga de eventos
        // se salteó la relectura, acá se cobra dentro del segundo siguiente -----
        if self.ws_read_pending {
            self.refresh_workspaces(qh);
        }
        // ----- la grabación se refresca en el tick de 1 s: es un `stat` y el tiempo
        // dibujado tiene que avanzar de a un segundo, no de a dos -----
        if self.widgets.refresh_recording() {
            self.sync_widget_bar_len();
            self.relayout_dock(qh);
        }
        if !self.dock.icons.is_empty() || !self.widget_placed(crate::config::WidgetKind::Clock) {
            return;
        }
        if !self.widgets.refresh_clock(&self.dock.config.settings) {
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

    /// Relee los workspaces lanzando `niri msg` (o `hyprctl`) y aplica el cambio. Va
    /// con **throttle**: una sola acción del usuario (abrir una ventana) emite 3-4
    /// eventos que piden relectura, y sin el tope cada uno costaba un `niri msg --json
    /// workspaces` (medido: 27 spawns por 3 ventanas abriéndose y cerrándose). El
    /// primero ya lee el estado FINAL —niri emite después de aplicar el cambio— así que
    /// los de la ráfaga sobran; si alguno se saltea, queda `ws_read_pending` y el tick de
    /// 1 s (`refresh_clock`) lo reintenta. Con el dock sin iconos fijados no hay nada que
    /// medir: se sale antes, como en el resto de los `refresh_*`.
    pub(crate) fn refresh_workspaces(&mut self, qh: &QueueHandle<Self>) {
        if self.dock.icons.is_empty() {
            if !toca_leer_workspaces(self.last_ws_read.map(|t| t.elapsed())) {
                self.ws_read_pending = true;
                return;
            }
            self.last_ws_read = Some(std::time::Instant::now());
            self.ws_read_pending = false;
            // ----- la foto ANTES de tocar la lista: `post_workspaces_changed` la
            // compara con el estado nuevo para saber si el activo cambió de verdad -----
            let (before, was_empty) = self.ws_snapshot();
            self.widgets.refresh_workspaces();
            self.post_workspaces_changed(before, was_empty, qh);
        }
    }

    /// Aplica una lista que YA vino en el evento (`WorkspacesChanged` de niri trae los
    /// workspaces enteros): cero procesos, y pasa por el mismo camino que la relectura
    /// para que el HUD, la regla del workspace vacío y el relayout no se puedan
    /// desincronizar entre los dos caminos.
    pub(crate) fn apply_workspaces(
        &mut self,
        list: Box<[crate::widgets::WorkspaceInfo]>,
        qh: &QueueHandle<Self>,
    ) {
        if self.dock.icons.is_empty() {
            // ----- misma foto previa que la relectura: los dos caminos tienen que
            // comparar contra el estado VIEJO -----
            let (before, was_empty) = self.ws_snapshot();
            self.widgets.set_workspaces(list.into_vec());
            self.post_workspaces_changed(before, was_empty, qh);
        }
    }

    /// Foto de "qué slot está activo" y "el activo está vacío", para comparar el
    /// estado VIEJO contra el nuevo. **Capturala antes de mutar
    /// `widgets.workspaces`**: si se calculara después del update, `before` ya
    /// sería el estado nuevo y `after != before` nunca daría verdadero (el HUD y
    /// el split de la isla dejaban de aparecer, la regresión de B4).
    fn ws_snapshot(&self) -> (f32, bool) {
        (
            render::ws_target_for(&self.widgets.workspaces),
            self.empty_workspace(),
        )
    }

    /// Lo que sigue a tener la lista nueva, venga de donde venga. `before` y
    /// `was_empty` son la foto PREVIA (las captura el llamador con `ws_snapshot`).
    fn post_workspaces_changed(&mut self, before: f32, was_empty: bool, qh: &QueueHandle<Self>) {
        let after = render::ws_target_for(&self.widgets.workspaces);
        self.marquee.track_ws_target(after);
        self.sync_widget_bar_len();
        // ----- la regla del workspace vacío se resuelve ACÁ, antes de decidir el
        // HUD: `show_ws_flash` mira `dock_visible` en este mismo bloque, así que
        // dejarlo al dibujo (que puede saltearse por un frame pendiente) hacía que
        // el HUD apareciera y el dock se revelara tarde. -----
        self.reveal_dock_if_stays(qh);
        // ----- dejó de estar vacío (cambio a uno ocupado, o se abrió una
        // ventana): la regla de "dock fijo" termina YA, no dentro del delay. Si el
        // puntero está encima o hay un modo abierto, `should_hide` es false y el
        // dock se queda (correcto). -----
        if was_empty && !self.empty_workspace() && self.should_hide() {
            self.set_dock_visible(false);
        }
        self.relayout_dock(qh);
        // ----- sólo si el espacio activo cambió de verdad -----
        log::debug!("wsflash: refresh before={before} after={after}");
        if after != before {
            self.show_ws_flash(qh);
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

    /// Relee SÓLO el volumen y lo dibuja. Son los dos caminos que corren muy
    /// seguido: el evento de PipeWire (las teclas de volumen del sistema corren
    /// `wpctl` por fuera del dock) y la rueda sobre el widget, que viene en ráfaga.
    /// `refresh_sys` de paso relee red y layout de teclado (`iw` + `niri msg`, ~20 ms
    /// medidos por lectura) y no hay por qué pagarlos en cada muesca.
    pub(crate) fn refresh_volume(&mut self, qh: &QueueHandle<Self>) {
        if !self.widget_placed(crate::config::WidgetKind::Volume) {
            return;
        }
        if self.widgets.refresh_volume() {
            self.sync_widget_bar_len();
            self.relayout_dock(qh);
        }
    }

    pub(crate) fn refresh_sys(&mut self, qh: &QueueHandle<Self>) {
        // ----- el layout de teclado NO se relee acá: llega por el event-stream
        // (`refresh_kblayout`), así que este tick se ahorra su spawn -----
        let active = self.dock.config.settings.widgets.iter().any(|w| {
            matches!(
                w.kind,
                crate::config::WidgetKind::Volume
                    | crate::config::WidgetKind::Network
                    | crate::config::WidgetKind::Mic
            )
        });
        if !active {
            return;
        }
        // ----- el panel de volumen relee sus streams aunque el widget no haya
        // cambiado: una app puede empezar o dejar de sonar sin que wpctl cambie -----
        self.refresh_volume_panel(qh);
        let mut changed = self.widgets.refresh_sys();
        // ----- el micrófono es otro `wpctl` (~19 ms): sólo si su widget está colocado -----
        if self.widget_placed(crate::config::WidgetKind::Mic) {
            changed |= self.widgets.refresh_mic();
        }
        if changed {
            self.sync_widget_bar_len();
            self.relayout_dock(qh);
        }
    }

    /// El layout de teclado cambió: niri lo avisa por el event-stream, así que el
    /// widget se actualiza al instante y el tick de 2 s no tiene que sondearlo.
    pub(crate) fn refresh_kblayout(&mut self, qh: &QueueHandle<Self>) {
        if !self.widget_placed(crate::config::WidgetKind::KbdLayout) {
            return;
        }
        if self.widgets.refresh_kblayout() {
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
