use super::*;

/// La rueda sobre el widget de volumen: `Some(true)` = subir, `None` = no hacer
/// nada (fin de scroll o sin movimiento).
///
/// OJO con el signo: en `wl_pointer` el eje vertical es **positivo hacia abajo**,
/// así que `discrete` negativo es rueda hacia ARRIBA. Se verificó contra la app
/// (`pointer.py --scroll up` manda REL_WHEEL +1 y llega como `discrete = -1`): con
/// el signo al revés la rueda hacía lo contrario. Y ojo con `f64::signum` para
/// descartar el cero: `signum(+0.0)` es `1.0`, no `0.0`.
fn wheel_raise(scroll: &smithay_client_toolkit::seat::pointer::AxisScroll) -> Option<bool> {
    if scroll.stop {
        return None;
    }
    let pasos = if scroll.discrete != 0 {
        scroll.discrete
    } else if scroll.absolute == 0.0 {
        0
    } else if scroll.absolute > 0.0 {
        // ponytail: touchpad sin pasos discretos -> un paso por evento, sin
        // acumular. Si molesta, acumular `absolute` acá.
        1
    } else {
        -1
    };
    (pasos != 0).then_some(pasos < 0)
}

impl App {
    pub(super) fn handle_dock_pointer_event(
        &mut self,
        event: &PointerEvent,
        qh: &QueueHandle<Self>,
    ) {
        // ----- HUD de workspaces: un click sobre un punto cambia de workspace.
        // Va ANTES del revelado a propósito: el HUD vive justamente con el dock
        // oculto, así que la rama de abajo se tragaba el click y el hit-test de
        // los puntos nunca llegaba a ejecutarse. -----
        if self.ws_flash_mode.is_some() {
            self.handle_ws_flash_pointer_event(event, qh);
            return;
        }
        // ----- oculto: la franja del dock es el disparador del reveal. La isla NO
        // recibe puntero, y no es un olvido: al entrar a la franja el `Enter` revela
        // el dock en este mismo handler y `should_hide()` exige `pointer_pos` en
        // `None`, así que la isla sólo se ve con el puntero lejos de la superficie.
        // Para darle interacción habría que **achicar el disparador al blob** (perder
        // el gesto de tirar el mouse al borde) o aceptar que scrollear revele: ver
        // "Isla dinámica, lo que sigue" en AGENTS.md. -----
        if !self.dock_visible {
            match event.kind {
                PointerEventKind::Enter { .. }
                | PointerEventKind::Motion { .. }
                | PointerEventKind::Press { .. } => {
                    // registrar el puntero ya en el evento que revela: si no, el
                    // timer no ve interacción y vuelve a ocultarlo enseguida
                    self.dock.set_pointer(Some(event.position));
                    self.reveal_dock(qh);
                }
                _ => {}
            }
            return;
        }
        if self.osd_mode.is_some() {
            if let PointerEventKind::Press { .. } = event.kind {
                self.close_osd_mode(qh);
            }
            return;
        }
        // ----- click en la banda de pestañas del overlay: va derecho a la pestaña
        // clickeada en vez de ciclar, y va ANTES de las ramas de cada panel para que
        // el hit test del panel no se lo coma (la banda está fuera del frame del
        // contenido, pero el orden lo deja explícito). Si no hay modo abierto o el
        // punto no cae en la banda, sigue el camino de siempre. -----
        if let PointerEventKind::Press { button, .. } = event.kind
            && button == BTN_LEFT
        {
            let (px, py) = self.panel_local(event.position.0, event.position.1);
            if let Some(index) = self.overlay_tab_hit(px, py)
                && let Some(current) = self.current_overlay()
            {
                let next = OVERLAY_ORDER[index];
                let dir = (index as i32 - current.tab_index() as i32).signum();
                log::debug!("overlay: click en la banda ({px:.0},{py:.0}) -> {next:?}");
                self.switch_overlay(current, next, dir, qh);
                return;
            }
        }
        if self.app_search_mode.is_some() {
            self.handle_app_search_pointer_event(event, qh);
            return;
        }
        if self.dock_menu_mode.is_some() {
            self.handle_dock_menu_pointer_event(event, qh);
            return;
        }
        if self.wallpaper_mode.is_some() {
            self.handle_wallpaper_pointer_event(event, qh);
            return;
        }
        if self.notifications_mode.is_some() {
            self.handle_notifications_pointer_event(event, qh);
            return;
        }
        if self.clipboard_mode.is_some() {
            self.handle_clipboard_pointer_event(event, qh);
            return;
        }
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                let (x, y) = event.position;
                self.dock.set_pointer(Some((x, y)));
                // ----- actividad del puntero: reinicia el reloj de ocultado -----
                self.last_ptr_event = Some(std::time::Instant::now());
                // ----- el puntero volvió: cancelar el ocultado por salida -----
                self.ptr_left_at = None;
                // ----- puntero encima: no ocultar -----
                self.autohide_armed = false;
                // ----- hover para pills (solo redibuja, el layout es fijo) -----
                let tray_count = self.tray.lock().unwrap().len();
                let hovered = render::widget_hit_test(&self.dock, &self.widgets, tray_count, x, y);
                self.dock.hovered_widget = hovered;
                // ----- el reloj abre su calendario después de un plazo corto de
                // hover (ver `note_calendar_hover`) -----
                self.note_calendar_hover(hovered == Some(crate::config::WidgetKind::Clock));

                if self.pointer_down {
                    if self.dock.dragging_index.is_some() {
                        self.dock.drag_to(x as f32, y as f32);
                    } else if let (Some(press_idx), Some((px, py))) =
                        (self.press_icon_index, self.press_pos)
                    {
                        let dist = ((x - px).powi(2) + (y - py).powi(2)).sqrt() as f32;
                        if dist > dock::DRAG_THRESHOLD {
                            self.dock.start_drag(press_idx);
                            self.dock.drag_to(x as f32, y as f32);
                        }
                    }
                }
                self.request_redraw(qh);
            }
            PointerEventKind::Leave { .. } => {
                self.dock.set_pointer(None);
                self.dock.hovered_widget = None;
                self.calendar_hover_at = None;
                // ----- salir de la franja oculta enseguida en vez de esperar el
                // delay configurable. No se oculta acá porque este Leave también
                // llega cuando el compositor reconfigura la superficie o cuando el
                // hit-test oscila en el borde: el ocultado lo decide el plazo
                // corto, y cualquier Enter/Motion lo cancela antes. -----
                self.ptr_left_at = Some(std::time::Instant::now());
                log::debug!("autohide:{} leave (franja)", crate::app::hdbg_ms());
                self.arm_autohide_after(super::draw::LEAVE_HIDE_MS);
                // ----- si hay un menú abierto, necesita su propio tick para cerrarse
                // (el autohide no lo agenda cuando está desactivado) -----
                if self.popup_mode.is_some() {
                    self.arm_popup_tick();
                }
                self.request_redraw(qh);
            }
            PointerEventKind::Press { button, .. } => {
                if self.menu.is_some() {
                    self.close_menu(qh);
                    return;
                }
                if self
                    .popup_mode
                    .as_ref()
                    .map(|p| !(p.closing && p.anim <= 0.0))
                    .unwrap_or(false)
                {
                    self.close_popup_mode(qh);
                    return;
                }
                let (x, y) = event.position;
                if button == BTN_LEFT {
                    self.pointer_down = true;
                    self.press_pos = Some((x, y));
                    self.press_icon_index = self.dock.icon_at(x, y);

                    if self.press_icon_index.is_none() {
                        // ----- hit test -----
                        let tray_count = self.tray.lock().unwrap().len();
                        if let Some(kind) =
                            render::widget_hit_test(&self.dock, &self.widgets, tray_count, x, y)
                        {
                            // ----- mismo log que el click derecho: es lo que deja
                            // ubicar un widget por su log en vez de adivinar -----
                            log::debug!("dock: click ({x:.0},{y:.0}) -> {kind:?}");
                            let accion = widget_action(
                                &self.dock,
                                &self.widgets,
                                tray_count,
                                x,
                                y,
                                crate::widget::WidgetClick::Left,
                                kind,
                            );
                            if let Some(action) = accion {
                                self.run_widget_action(action, qh);
                                return;
                            }
                        }
                        // ----- el doble click abre el selector de fondos SÓLO en la
                        // banda central: en los extremos se dispara sin querer. No se
                        // quita del todo porque es la única vía (no hay keybind). -----
                        const WALLPAPER_BAND: f64 = 220.0;
                        let (bw, bh) = self.dock.base_size();
                        let (along, span) = if self.dock.is_vertical() {
                            (y, bh as f64)
                        } else {
                            (x, bw as f64)
                        };
                        let en_centro = (along - span / 2.0).abs() <= WALLPAPER_BAND / 2.0;
                        let now = std::time::Instant::now();
                        let is_double = en_centro
                            && self
                                .last_empty_click
                                .map(|(t, px, py)| {
                                    now.duration_since(t).as_millis() < 400
                                        && (x - px).abs() < 12.0
                                        && (y - py).abs() < 12.0
                                })
                                .unwrap_or(false);
                        if is_double {
                            self.last_empty_click = None;
                            self.open_wallpaper_picker(qh);
                        } else {
                            self.last_empty_click = Some((now, x, y));
                        }
                    }
                } else if button == BTN_RIGHT {
                    match self.dock.icon_at(x, y) {
                        Some(idx) => self.open_menu(menu::MenuScreen::IconMenu(idx), qh),
                        None => {
                            let tray_count = self.tray.lock().unwrap().len();
                            let hit = render::widget_hit_test(
                                &self.dock,
                                &self.widgets,
                                tray_count,
                                x,
                                y,
                            );
                            log::debug!("dock: click derecho ({x:.0},{y:.0}) -> {hit:?}");
                            let accion = hit.and_then(|kind| {
                                widget_action(
                                    &self.dock,
                                    &self.widgets,
                                    tray_count,
                                    x,
                                    y,
                                    crate::widget::WidgetClick::Right,
                                    kind,
                                )
                            });
                            if let Some(action) = accion {
                                self.run_widget_action(action, qh);
                            }
                        }
                    }
                }
            }
            PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                self.pointer_down = false;
                if self.dock.dragging_index.is_some() {
                    self.dock.end_drag();
                    if let Err(err) = self.dock.config.save() {
                        log::warn!("failed to save dock config: {err}");
                    }
                } else if let Some(idx) = self.press_icon_index
                    && let Some(icon) = self.dock.icons.get(idx)
                {
                    // ----- `get`, no `[idx]`: el índice es del press y entre
                    // press y release un drag puede haber sacado el icono -----
                    launch_app(&icon.app.exec);
                }
                self.press_pos = None;
                self.press_icon_index = None;
                self.request_redraw(qh);
            }
            // ----- rueda: volumen en pasos del 5% si el puntero está sobre el
            // widget de volumen. Es la única interacción por rueda de la barra. -----
            PointerEventKind::Axis { vertical, .. } => {
                let Some(subir) = wheel_raise(&vertical) else {
                    return;
                };
                let (x, y) = event.position;
                let tray_count = self.tray.lock().unwrap().len();
                let hit = render::widget_hit_test(&self.dock, &self.widgets, tray_count, x, y);
                log::debug!("dock: rueda ({x:.0},{y:.0}) subir={subir} -> {hit:?}");
                let accion = hit.and_then(|kind| {
                    widget_action(
                        &self.dock,
                        &self.widgets,
                        tray_count,
                        x,
                        y,
                        crate::widget::WidgetClick::Wheel { up: subir },
                        kind,
                    )
                });
                if let Some(action) = accion {
                    self.run_widget_action(action, qh);
                }
            }
            _ => {}
        }
    }

    /// Ejecuta lo que la tabla de widgets decidió.
    ///
    /// Es el ÚNICO lugar donde un click de widget se convierte en algo que hace
    /// la app, y está acá y no en la tabla porque varias acciones necesitan
    /// `&mut App` (abrir el menú de apagado, el panel de volumen, el menú del
    /// tray) o el estado del tray, que no vive en el widget.
    fn run_widget_action(&mut self, action: crate::widget::WidgetAction, qh: &QueueHandle<App>) {
        use crate::widget::WidgetAction as A;
        match action {
            A::MediaToggle => widgets::media_toggle(),
            A::ToggleMic => widgets::mic_toggle(),
            A::ToggleRecording => {
                // ----- el script del repo: arranca o para wf-recorder y deja la ruta
                // en ~/.cache/dockyrs-recording-path, que es lo que lee la isla -----
                if let Some(script) = repo_dir()
                    .join("record-toggle.sh")
                    .to_str()
                    .map(String::from)
                {
                    let _ = std::process::Command::new(script).spawn();
                }
            }
            A::OpenPowerMenu => self.open_power_menu(qh),
            A::OpenBluetoothManager => {
                if let Some(bt) = &self.widgets.bluetooth {
                    widgets::open_bluetooth_manager(true, bt.powered);
                }
            }
            A::ActivateTray { index } => {
                let icons = self.tray.lock().unwrap();
                if let Some(item) = icons.get(index) {
                    crate::tray::activate(item.service.clone(), item.path.clone());
                }
            }
            A::OpenTrayMenu { index } => self.open_tray_menu(index, qh),
            A::SwitchWorkspace { id } => {
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
            A::OpenNetworkSettings => widgets::open_network_settings(),
            A::OpenSystemMonitor => widgets::open_system_monitor(),
            A::OpenVolumePanel => self.open_volume_panel(qh),
            A::OpenVolumeControl => widgets::open_volume_control(),
            A::VolumeStep { up } => {
                widgets::volume_step(up);
                // ----- releer ya, y SÓLO el volumen: con el tick de sistema el
                // número quedaba hasta 2 s viejo, y `refresh_sys` de paso relee
                // red y layout de teclado (~20 ms por muesca de rueda). -----
                self.refresh_volume(qh);
            }
            A::NextKbdLayout => widgets::kblayout_next(),
            A::OpenDockMenu => self.open_dock_menu(qh),
            A::OpenWidgetTrayMenu(which) => {
                // ----- wifi y bluetooth salen del tray visible, así que su menú
                // (nm-applet/blueman) se abre desde su propio widget de la
                // izquierda -----
                let abierto = match which {
                    crate::config::WidgetKind::Network => {
                        self.open_widget_tray_menu(which, |s| s.contains("nm_applet"), qh)
                    }
                    _ => self.open_widget_tray_menu(
                        which,
                        |s| {
                            let s = s.to_lowercase();
                            s.contains("blueman") || s.contains("bluetooth")
                        },
                        qh,
                    ),
                };
                if !abierto {
                    // ----- sin el item registrado: que haga lo del click
                    // izquierdo en vez de nada -----
                    match which {
                        crate::config::WidgetKind::Network => widgets::open_network_settings(),
                        _ => {
                            let powered = self
                                .widgets
                                .bluetooth
                                .as_ref()
                                .map(|b| b.powered)
                                .unwrap_or(false);
                            widgets::open_bluetooth_manager(true, powered);
                        }
                    }
                }
            }
        }
    }
}

/// Resuelve un click con la tabla de widgets. `None` si el widget no reacciona.
///
/// El `ClickCtx` se arma y se tira acá adentro a propósito: presta `&self.dock` y
/// `&self.widgets`, así que no puede sobrevivir hasta el momento de ejecutar la
/// acción, que necesita `&mut self`.
fn widget_action(
    dock: &crate::dock::Dock,
    widgets: &crate::widgets::WidgetSnapshot,
    tray_count: usize,
    x: f64,
    y: f64,
    click: crate::widget::WidgetClick,
    kind: crate::config::WidgetKind,
) -> Option<crate::widget::WidgetAction> {
    let click_fn = crate::widget::spec_for(kind)?.click?;
    // ----- las tres sub-zonas se resuelven aca: asi las decisiones de la tabla
    // son puras (y testeables) y no dependen del `Dock` -----
    let cx = crate::widget::ClickCtx {
        click,
        tray_index: render::tray_icon_hit(dock, widgets, tray_count, x, y),
        workspace_id: render::workspace_dot_hit(dock, widgets, tray_count, x, y),
        media_toggle: render::media_toggle_hit(dock, widgets, tray_count, x, y),
    };
    click_fn(&cx)
}

#[cfg(test)]
mod wheel_tests {
    // ----- el signo del eje vertical es lo que se equivocó: sin este test la
    // rueda hacía lo contrario (arriba bajaba el volumen). -----
    use super::wheel_raise;
    use smithay_client_toolkit::seat::pointer::AxisScroll;

    fn axis(absolute: f64, discrete: i32, stop: bool) -> AxisScroll {
        AxisScroll {
            absolute,
            discrete,
            stop,
        }
    }

    #[test]
    fn rueda_arriba_sube_y_abajo_baja() {
        // REL_WHEEL +1 (arriba) llega como discrete = -1, medido en la app
        assert_eq!(wheel_raise(&axis(-1.0, -1, false)), Some(true));
        assert_eq!(wheel_raise(&axis(1.0, 1, false)), Some(false));
    }

    #[test]
    fn touchpad_sin_pasos_discretos_usa_el_signo_del_absoluto() {
        assert_eq!(wheel_raise(&axis(-2.0, 0, false)), Some(true));
        assert_eq!(wheel_raise(&axis(2.0, 0, false)), Some(false));
    }

    #[test]
    fn el_fin_de_scroll_y_el_cero_no_hacen_nada() {
        assert_eq!(wheel_raise(&axis(0.0, 0, true)), None);
        assert_eq!(wheel_raise(&axis(0.0, 0, false)), None);
    }
}
