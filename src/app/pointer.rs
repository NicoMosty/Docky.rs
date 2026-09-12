use super::*;

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
        // ----- oculto: la franja superior del dock es el disparador -----
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
        if self.notification_mode.is_some() {
            if let PointerEventKind::Press { .. } = event.kind {
                self.close_notification_mode(qh);
            }
            return;
        }
        if self.osd_mode.is_some() {
            if let PointerEventKind::Press { .. } = event.kind {
                self.close_osd_mode(qh);
            }
            return;
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
                // ----- salir de la franja oculta enseguida en vez de esperar el
                // delay configurable. No se oculta acá porque este Leave también
                // llega cuando el compositor reconfigura la superficie o cuando el
                // hit-test oscila en el borde: el ocultado lo decide el plazo
                // corto, y cualquier Enter/Motion lo cancela antes. -----
                self.ptr_left_at = Some(std::time::Instant::now());
                log::debug!("autohide:{} leave (franja)", crate::app::hdbg_ms());
                self.arm_autohide_after(super::draw::LEAVE_HIDE_MS);
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
                            match kind {
                                crate::config::WidgetKind::Media => {
                                    if render::media_toggle_hit(
                                        &self.dock,
                                        &self.widgets,
                                        tray_count,
                                        x,
                                        y,
                                    ) {
                                        widgets::media_toggle();
                                    }
                                    return;
                                }
                                crate::config::WidgetKind::PowerMenu => {
                                    if !widgets::open_power_external() {
                                        self.open_power_menu(qh);
                                    }
                                    return;
                                }
                                crate::config::WidgetKind::Bluetooth => {
                                    if let Some(bt) = &self.widgets.bluetooth {
                                        widgets::open_bluetooth_manager(true, bt.powered);
                                    }
                                    return;
                                }
                                crate::config::WidgetKind::Tray => {
                                    if let Some(idx) = render::tray_icon_hit(
                                        &self.dock,
                                        &self.widgets,
                                        tray_count,
                                        x,
                                        y,
                                    ) {
                                        let icons = self.tray.lock().unwrap();
                                        if let Some(item) = icons.get(idx) {
                                            crate::tray::activate(
                                                item.service.clone(),
                                                item.path.clone(),
                                            );
                                        }
                                    }
                                    return;
                                }
                                crate::config::WidgetKind::Workspaces => {
                                    if let Some(id) = render::workspace_dot_hit(
                                        &self.dock,
                                        &self.widgets,
                                        tray_count,
                                        x,
                                        y,
                                    ) {
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
                                    return;
                                }
                                crate::config::WidgetKind::Network => {
                                    widgets::open_network_settings();
                                    return;
                                }
                                crate::config::WidgetKind::Ram => {
                                    widgets::open_system_monitor();
                                    return;
                                }
                                crate::config::WidgetKind::Volume => {
                                    widgets::open_volume_control();
                                    return;
                                }
                                crate::config::WidgetKind::KbdLayout => {
                                    widgets::kblayout_next();
                                    return;
                                }
                                _ => {}
                            }
                        }
                        let now = std::time::Instant::now();
                        let is_double = self
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
                            if matches!(hit, Some(crate::config::WidgetKind::Tray)) {
                                if let Some(idx) = render::tray_icon_hit(
                                    &self.dock,
                                    &self.widgets,
                                    tray_count,
                                    x,
                                    y,
                                ) {
                                    self.open_tray_menu(idx, qh);
                                }
                            } else if matches!(hit, Some(crate::config::WidgetKind::Workspaces)) {
                                // ----- ajustes: SÓLO con click derecho sobre el
                                // indicador de workspaces. Antes se abría en cualquier
                                // punto sin icono ni tray: reloj, huecos entre zonas,
                                // o encima de cualquier widget. -----
                                self.open_dock_menu(qh);
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
                } else if let Some(idx) = self.press_icon_index {
                    launch_app(&self.dock.icons[idx].app.exec);
                }
                self.press_pos = None;
                self.press_icon_index = None;
                self.request_redraw(qh);
            }
            _ => {}
        }
    }
}
