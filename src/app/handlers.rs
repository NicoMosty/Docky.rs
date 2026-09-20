use super::*;

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        self.output_scale = new_factor;
        self.request_redraw(qh);
        self.request_menu_redraw(qh);
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        if self
            .screenshot
            .as_ref()
            .is_some_and(|s| s.is_selector_surface(surface))
        {
            self.draw_region_frame();
            return;
        }
        // ----- el toast tiene superficie propia: su frame es el reloj de su fade -----
        let is_toast = self
            .toast_layer
            .as_ref()
            .is_some_and(|l| l.wl_surface() == surface);
        if is_toast {
            self.tick_notification_frame(qh);
            return;
        }
        if surface == self.layer.wl_surface() {
            self.awaiting_frame = false;
            // ----- el toast ya no pasa por acá: tiene su propia superficie y su propio
            // frame (`is_toast`, arriba). Con la rama vieja, cualquier redibujado del
            // dock mientras el aviso estaba arriba consumía el frame del dock y
            // frenaba la animación del reveal/de la isla. -----
            if self.osd_mode.is_some() {
                self.tick_osd_frame(qh);
            } else if self.ws_flash_mode.is_some() {
                self.tick_ws_flash_frame(qh);
            } else if self.app_search_mode.is_some() {
                self.tick_app_search_frame(qh);
            } else if self.dock_menu_mode.is_some() {
                self.tick_dock_menu_frame(qh);
            } else if self.wallpaper_mode.is_some() {
                self.tick_wallpaper_frame(qh);
            } else if self.clipboard_mode.is_some() {
                self.tick_clipboard_frame(qh);
            } else {
                let icons_animating = self.dock.step_animation();
                let splitting = self.tick_island_split_frame(qh);
                let revealing = self.tick_reveal_frame(qh);
                if self.marquee.workspace_animating() {
                    self.tick_workspace(qh);
                } else if !splitting && !revealing && icons_animating {
                    self.draw(qh);
                }
            }
            return;
        }
        let is_menu = self
            .menu
            .as_ref()
            .map(|m| surface == m.layer.wl_surface())
            .unwrap_or(false);
        if is_menu {
            self.tick_menu_frame(qh);
            return;
        }
        let is_popup = self
            .popup_mode
            .as_ref()
            .map(|p| surface == p.layer.wl_surface())
            .unwrap_or(false);
        if is_popup {
            self.tick_popup_frame(qh);
        }
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

/// Qué hacer con un `configure` de la superficie del dock, según el tamaño que mandó el
/// compositor y el que pedimos (`applied_size`):
///
/// - `Adoptar`: no pedimos nada todavía (primer configure), manda el compositor.
/// - `Reconciliar`: el tamaño no es el nuestro, así que se re-aplica el reparto propio una
///   vez (si no, el buffer queda de otro tamaño que el layout y los clicks caen corridos:
///   el hit test usa coordenadas locales, AUDIT.md B7).
/// - `Dibujar`: coincide (el caso normal) o el compositor ya insistió con otro tamaño y no
///   tiene sentido girar en bucle de `set_size` -> `configure` -> `set_size`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConfigureAccion {
    Adoptar,
    Reconciliar,
    Dibujar,
}

pub(super) fn accion_del_configure(
    applied: Option<(u32, u32)>,
    nuevo: (u32, u32),
    ya_reconciliado: bool,
) -> ConfigureAccion {
    match applied {
        None => ConfigureAccion::Adoptar,
        Some(ap) if ap != nuevo => {
            if ya_reconciliado {
                ConfigureAccion::Dibujar
            } else {
                ConfigureAccion::Reconciliar
            }
        }
        Some(_) => ConfigureAccion::Dibujar,
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if layer.wl_surface() == self.layer.wl_surface() {
            self.exit = true;
        } else if self
            .toast_layer
            .as_ref()
            .is_some_and(|l| l.wl_surface() == layer.wl_surface())
        {
            self.toast_layer = None;
        } else if self
            .menu
            .as_ref()
            .map(|m| m.layer.wl_surface() == layer.wl_surface())
            .unwrap_or(false)
        {
            self.menu = None;
        } else if self
            .popup_mode
            .as_ref()
            .map(|p| p.layer.wl_surface() == layer.wl_surface())
            .unwrap_or(false)
        {
            self.popup_mode = None;
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        if self
            .screenshot
            .as_ref()
            .is_some_and(|s| s.is_selector_layer(layer))
        {
            let (w, h) = configure.new_size;
            self.configure_region(w, h);
            return;
        }
        if layer.wl_surface() == self.layer.wl_surface() {
            let (cw, ch) = configure.new_size;
            // ----- el compositor manda el tamaño REAL de la superficie; si no es el que
            // pedimos, el buffer queda de otro tamaño que el layout y los clicks caen
            // corridos respecto a lo dibujado, porque el hit test usa coordenadas locales
            // del reparto (AUDIT.md B7) -----
            match accion_del_configure(self.applied_size, (cw, ch), self.configure_reconciliado) {
                ConfigureAccion::Adoptar => {
                    // ----- primer configure: es el que manda (todavía no pedimos nada) y
                    // queda anotado para poder detectar desajustes después -----
                    self.applied_size = Some((cw, ch));
                    self.configure_reconciliado = false;
                }
                ConfigureAccion::Reconciliar => {
                    self.configure_reconciliado = true;
                    log::warn!(
                        "dock: configure new_size=({cw},{ch}) != applied={:?}: re-aplico el reparto",
                        self.applied_size
                    );
                    let panel = self
                        .overlay_panel_size()
                        .or_else(|| self.dock_menu_mode.as_ref().map(|m| (m.panel_w, m.panel_h)));
                    match panel {
                        Some((pw, ph)) => self.apply_panel_size(pw, ph),
                        None => self.restore_dock_size(),
                    }
                    self.draw(qh);
                    return;
                }
                ConfigureAccion::Dibujar => {
                    if self.applied_size == Some((cw, ch)) {
                        // ----- volvió a coincidir: se re-arma la reconciliación -----
                        self.configure_reconciliado = false;
                    } else {
                        log::warn!(
                            "dock: el compositor insiste con ({cw},{ch}) contra applied={:?}: dibujo como vino",
                            self.applied_size
                        );
                    }
                }
            }
            log::debug!(
                "dock: configure new_size={:?} base={:?} applied={:?}",
                configure.new_size,
                self.dock.base_size(),
                self.applied_size
            );
            if self.first_configure {
                self.first_configure = false;
            }
            self.draw(qh);
        } else if self
            .menu
            .as_ref()
            .is_some_and(|m| m.layer.wl_surface() == layer.wl_surface() && !m.closing)
        {
            self.draw_menu(qh);
        } else if self
            .popup_mode
            .as_ref()
            .is_some_and(|p| p.layer.wl_surface() == layer.wl_surface() && !p.closing)
        {
            self.draw_popup_mode(qh);
        } else if self
            .toast_layer
            .as_ref()
            .is_some_and(|l| l.wl_surface() == layer.wl_surface())
        {
            // ----- el configure del toast: el tamaño ya lo fijó `create_toast_surface`,
            // así que acá sólo se pinta -----
            self.draw_notification_mode(qh);
        } else if self
            .click_catcher
            .as_ref()
            .is_some_and(|c| c.surface() == layer.wl_surface())
        {
            self.catcher_configure(configure.new_size);
        }
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = Some(self.seat_state.get_pointer(qh, &seat).expect("get pointer"));
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(
                self.seat_state
                    .get_keyboard(qh, &seat, None)
                    .expect("get keyboard"),
            );
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for App {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            log::debug!(
                "autohide:{} ptr {:?} dock={}",
                crate::app::hdbg_ms(),
                event.kind,
                event.surface == *self.layer.wl_surface()
            );
            if self
                .screenshot
                .as_ref()
                .is_some_and(|s| s.is_selector_surface(&event.surface))
            {
                self.handle_region_pointer_event(event, qh);
                continue;
            }
            let is_menu = self
                .menu
                .as_ref()
                .map(|m| event.surface == *m.layer.wl_surface())
                .unwrap_or(false);
            let is_popup = self
                .popup_mode
                .as_ref()
                .map(|p| event.surface == *p.layer.wl_surface())
                .unwrap_or(false);
            let is_catcher = self
                .click_catcher
                .as_ref()
                .map(|c| event.surface == *c.surface())
                .unwrap_or(false);
            // ----- el toast de notificaciones: un click lo descarta -----
            let is_toast = self
                .toast_layer
                .as_ref()
                .map(|l| event.surface == *l.wl_surface())
                .unwrap_or(false);
            if is_toast {
                if let PointerEventKind::Press { .. } = event.kind {
                    self.close_notification_mode(qh);
                }
            } else if is_menu {
                self.handle_menu_pointer_event(event, qh);
            } else if is_popup {
                self.handle_popup_pointer_event(event, qh);
            } else if is_catcher {
                self.handle_catcher_pointer_event(event, qh);
            } else if event.surface == *self.layer.wl_surface() {
                self.handle_dock_pointer_event(event, qh);
            }
        }
    }
}

impl App {
    // ----- stepper acceleration -----
    pub(super) fn poll_held_key(&mut self) -> Option<(Keysym, u32)> {
        let (keysym, frames, mut accum) = self.held_key?;
        accum += crate::menu::stepper_speed(frames) / 60.0;
        let steps = accum.floor();
        accum -= steps;
        self.held_key = Some((keysym, frames + 1, accum));
        (steps > 0.0).then_some((keysym, steps as u32))
    }
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if self.screenshot.as_ref().is_some_and(|s| s.selector.mapped) {
            if event.keysym == Keysym::Escape {
                self.close_region(None, qh);
            }
            return;
        }
        // ----- Shift+←/→ cambia de modo del overlay (el equivalente a los modos de
        // rofi): launcher -> portapapeles -> fondos -> ventanas. En el panel VERTICAL la
        // banda de pestañas es una columna, así que la flecha que corre por ella es ↑/↓
        // y también cicla (las cuatro siguen andando: el gesto de ←/→ no se toca). Va
        // ANTES de armar `held_key`; si no, el auto-repeat de la flecha ciclaría un modo
        // por frame (y el release es el que limpia `overlay_cycle_key`). -----
        if self.modifiers.shift && flecha_de_la_banda(event.keysym, self.dock.is_vertical()) {
            let accion = accion_de_la_banda(
                self.overlay_cycle_key,
                event.keysym,
                self.current_overlay().is_some(),
            );
            match accion {
                AccionBanda::Ciclar => {
                    self.overlay_cycle_key = Some(event.keysym);
                    let dir = overlay_cycle_dir(event.keysym);
                    if self.cycle_overlay(dir, qh) {
                        return;
                    }
                }
                AccionBanda::Tragar => return,
                AccionBanda::Seguir => {}
            }
        }
        self.held_key = matches!(
            event.keysym,
            Keysym::Left | Keysym::Right | Keysym::Up | Keysym::Down
        )
        .then_some((event.keysym, 0, 0.0));
        if self.notifications_mode.is_some() {
            self.handle_notifications_key(event, qh);
        } else if self.clipboard_mode.is_some() {
            self.handle_clipboard_key(event, qh);
        } else if self.app_search_mode.is_some() {
            self.handle_app_search_key(event, qh);
        } else if self.wallpaper_mode.is_some() {
            self.handle_wallpaper_key(event.keysym, qh);
        } else if self.dock_menu_mode.is_some() {
            self.handle_dock_menu_key(event, qh);
        } else if self.popup_mode.is_some() {
            // menús del tray, menú de energía y panel de volumen
            self.handle_popup_key(event, qh);
        } else {
            self.handle_search_key(event, qh);
        }
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if self.held_key.is_some_and(|(k, ..)| k == event.keysym) {
            self.held_key = None;
        }
        // ----- soltar la flecha re-arma el ciclo de la banda (B1): la próxima
        // pulsación de esa misma flecha tiene que ciclar de nuevo -----
        if self.overlay_cycle_key == Some(event.keysym) {
            self.overlay_cycle_key = None;
        }
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
    }
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

impl Dispatch<wl_callback::WlCallback, ()> for App {
    fn event(
        _: &mut Self,
        _: &wl_callback::WlCallback,
        _: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_compositor!(App);
delegate_output!(App);
delegate_shm!(App);
delegate_seat!(App);
smithay_client_toolkit::delegate_pointer!(App);
delegate_keyboard!(App);
delegate_layer!(App);
delegate_registry!(App);

#[cfg(test)]
mod configure_accion_tests {
    use super::{ConfigureAccion, accion_del_configure};

    /// B7: el caso normal es `Dibujar` (el compositor devuelve el tamaño que pedimos) y
    /// NUNCA `Reconciliar`: si esto se rompe, cada configure re-aplicaría el tamaño y el
    /// dock entraría en churn de `set_size` (con la pérdida de foco del puntero que eso
    /// trae).
    #[test]
    fn el_caso_normal_no_reconcilia_nada() {
        assert_eq!(
            accion_del_configure(Some((640, 236)), (640, 236), false),
            ConfigureAccion::Dibujar
        );
        // ----- el primero manda el compositor -----
        assert_eq!(
            accion_del_configure(None, (832, 26), false),
            ConfigureAccion::Adoptar
        );
        // ----- desajuste: se reconcilia UNA vez -----
        assert_eq!(
            accion_del_configure(Some((640, 236)), (700, 236), false),
            ConfigureAccion::Reconciliar
        );
        // ----- y si el compositor insiste, se dibuja como vino (sin bucle) -----
        assert_eq!(
            accion_del_configure(Some((640, 236)), (700, 236), true),
            ConfigureAccion::Dibujar
        );
    }
}
