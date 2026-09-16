use super::*;

// ----- la superficie del popup ocupa todo el ancho del dock (hace falta para
// poder centrar el recuadro sobre el icono sin conocer el offset real de la
// superficie en pantalla), pero la input region se limita al recuadro del menú:
// si no, una banda invisible del ancho del dock se tragaba los clicks y el
// puntero nunca llegaba a la ventana de abajo. -----
pub(super) fn popup_input_region(
    compositor: &CompositorState,
    box_x: f32,
    box_y: f32,
    content_height: f32,
) -> Option<Region> {
    let region = Region::new(compositor).ok()?;
    region.add(
        box_x.round() as i32,
        box_y.round() as i32,
        menu::MENU_WIDTH.round() as i32,
        content_height.round() as i32,
    );
    Some(region)
}

impl App {
    pub(super) fn open_power_menu(&mut self, qh: &QueueHandle<Self>) {
        let (controls, content_height) = menu::build_controls(menu::MenuScreen::PowerMenu, 0, 0);
        let tray_count = self.tray.lock().unwrap().len();
        let center = render::widget_center(
            &self.dock,
            &self.widgets,
            tray_count,
            crate::config::WidgetKind::PowerMenu,
        );
        self.create_popup_surface(
            menu::MenuScreen::PowerMenu,
            controls,
            content_height,
            Vec::new(),
            String::new(),
            String::new(),
            center,
            qh,
        );
    }

    pub(super) fn open_tray_menu(&mut self, idx: usize, qh: &QueueHandle<Self>) {
        let icons = self.tray.lock().unwrap();
        let Some(item) = icons.get(idx).cloned() else {
            log::debug!("tray: menu idx={idx} fuera de rango (len={})", icons.len());
            return;
        };
        drop(icons);
        let tray_count = self.tray.lock().unwrap().len();
        let center = render::tray_icon_center(&self.dock, &self.widgets, tray_count, idx);
        self.open_tray_menu_for(item.service, item.path, item.menu_path, center, qh);
    }

    // ----- menú de un item por service/path (no por índice): sirve también para
    // los items filtrados del tray visible, como wifi y bluetooth. -----
    pub(super) fn open_tray_menu_for(
        &mut self,
        service: String,
        path: String,
        menu_path: Option<String>,
        center: Option<(f32, f32)>,
        qh: &QueueHandle<Self>,
    ) {
        let Some(menu_path) = menu_path else {
            // ----- sin menú D-Bus: que haga lo del click izquierdo, antes no
            // pasaba nada y el click derecho parecía roto -----
            log::debug!("tray: {service} sin Menu, activate como izquierdo");
            crate::tray::activate(service, path);
            return;
        };
        let items = crate::tray::fetch_menu(&service, &menu_path, 0);
        if items.is_empty() {
            log::debug!("tray: {service} menú vacío, activate como izquierdo");
            crate::tray::activate(service, path);
            return;
        }
        let (controls, content_height) = menu::build_tray_menu_controls(&items, false);
        self.create_popup_surface(
            menu::MenuScreen::TrayMenu,
            controls,
            content_height,
            items,
            service,
            menu_path,
            center,
            qh,
        );
    }

    // ----- click derecho sobre un widget de la izquierda (Network/Bluetooth):
    // abre el menú del item correspondiente del StatusNotifier aunque esté
    // ignorado en el tray visible. Devuelve false si no hay item con menú, para
    // que el que llama pueda caer en la acción del click izquierdo. -----
    pub(super) fn open_widget_tray_menu(
        &mut self,
        kind: crate::config::WidgetKind,
        matches: impl Fn(&str) -> bool,
        qh: &QueueHandle<Self>,
    ) -> bool {
        let Some((service, path, menu_path)) = crate::tray::find_menu(matches) else {
            log::debug!("tray: {kind:?} sin item StatusNotifier, acción por defecto");
            return false;
        };
        let tray_count = self.tray.lock().unwrap().len();
        let center = render::widget_center(&self.dock, &self.widgets, tray_count, kind);
        self.open_tray_menu_for(service, path, Some(menu_path), center, qh);
        true
    }

    pub(super) fn popup_geometry(
        &self,
        content_height: f32,
        center: Option<(f32, f32)>,
    ) -> (f32, f32, f32, f32) {
        let is_vertical = self.dock.is_vertical();
        let (base_w, base_h) = self.dock.base_size();
        if is_vertical {
            let cy = center.map(|(_, y)| y).unwrap_or(base_h as f32 / 2.0);
            let box_y =
                (cy - content_height / 2.0).clamp(0.0, (base_h as f32 - content_height).max(0.0));
            (menu::MENU_WIDTH, base_h as f32, 0.0, box_y)
        } else {
            let cx = center.map(|(x, _)| x).unwrap_or(base_w as f32 / 2.0);
            let box_x = (cx - menu::MENU_WIDTH / 2.0)
                .clamp(0.0, (base_w as f32 - menu::MENU_WIDTH).max(0.0));
            (base_w as f32, content_height, box_x, 0.0)
        }
    }

    // ----- full redeclare -----
    fn popup_anchor_margin(&self) -> (Anchor, (i32, i32, i32, i32)) {
        let s = &self.dock.config.settings;
        edge_anchor_margin(
            s.dock_edge,
            s.dock_align,
            s.pos_y,
            self.dock.thickness() as i32 + MENU_GAP,
        )
    }

    // ----- submenu navigation -----
    fn set_tray_items(
        &mut self,
        items: Vec<crate::tray::TrayMenuItem>,
        has_back: bool,
        qh: &QueueHandle<Self>,
    ) {
        let (controls, content_height) = menu::build_tray_menu_controls(&items, has_back);
        let center = self.popup_mode.as_ref().and_then(|p| p.center);
        let (surface_w, surface_h, box_x, box_y) = self.popup_geometry(content_height, center);
        let (anchor, margin) = self.popup_anchor_margin();
        let region = popup_input_region(&self.compositor, box_x, box_y, content_height);
        let Some(p) = self.popup_mode.as_mut() else {
            return;
        };
        p.layer.set_anchor(anchor);
        p.layer.set_margin(margin.0, margin.1, margin.2, margin.3);
        p.layer.set_size(surface_w as u32, surface_h as u32);
        if let Some(region) = &region {
            p.layer
                .wl_surface()
                .set_input_region(Some(region.wl_region()));
        }
        p.tray_items = items;
        p.controls = controls;
        p.content_height = content_height;
        p.surface_w = surface_w;
        p.surface_h = surface_h;
        p.box_x = box_x;
        p.box_y = box_y;
        p.hovered = None;
        // ----- los items cambiaron (submenú / volver): re-renderizar el contenido -----
        p.content_dirty = true;
        p.layer.wl_surface().commit();
        self.request_popup_redraw(qh);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn create_popup_surface(
        &mut self,
        screen: menu::MenuScreen,
        controls: Vec<menu::Control>,
        content_height: f32,
        tray_items: Vec<crate::tray::TrayMenuItem>,
        tray_service: String,
        tray_menu_path: String,
        center: Option<(f32, f32)>,
        qh: &QueueHandle<Self>,
    ) {
        self.menu = None;
        self.popup_mode = None;

        let (surface_w, surface_h, box_x, box_y) = self.popup_geometry(content_height, center);

        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Overlay,
            Some("dockyrs-menu"),
            self.pinned_output.as_ref(),
        );
        let (anchor, margin) = self.popup_anchor_margin();
        layer.set_anchor(anchor);
        layer.set_margin(margin.0, margin.1, margin.2, margin.3);
        layer.set_size(surface_w as u32, surface_h as u32);
        // ----- mismo criterio que el menú de íconos: el teclado lo tiene la
        // superficie del popup mientras esté mapeada (con OnDemand no llegaba
        // ninguna tecla y Escape no cerraba nada) -----
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.set_exclusive_zone(-1);
        if let Some(region) = popup_input_region(&self.compositor, box_x, box_y, content_height) {
            layer
                .wl_surface()
                .set_input_region(Some(region.wl_region()));
        }
        layer.commit();

        let pool_size = (surface_w as usize * surface_h as usize * 16).max(65536);
        let pool = SlotPool::new(pool_size, &self.shm).expect("failed to create popup shm pool");

        self.popup_mode = Some(DockPopupMode {
            layer,
            pool,
            awaiting_frame: false,
            screen,
            controls,
            content_height,
            hovered: None,
            anim: 1.0,
            target_anim: 1.0,
            closing: false,
            tray_items,
            tray_service,
            tray_menu_path,
            tray_stack: Vec::new(),
            content: None,
            content_dirty: true,
            center,
            surface_w,
            surface_h,
            box_x,
            box_y,
            popup_hovered: false,
            volume_rows: Vec::new(),
            volume_devices: Vec::new(),
            volume_drag: None,
            volume_apply_at: None,
        });
    }

    pub(super) fn close_popup_mode(&mut self, qh: &QueueHandle<Self>) {
        // ----- cierre directo: sin animación no hay frames que recompongan el panel -----
        if let Some(p) = self.popup_mode.take() {
            p.layer.wl_surface().attach(None, 0, 0);
            p.layer.wl_surface().commit();
            trim_heap();
        }
        let _ = qh;
    }

    pub(super) fn request_popup_redraw(&mut self, qh: &QueueHandle<Self>) {
        if self.popup_mode.as_ref().is_some_and(|p| !p.awaiting_frame) {
            self.draw_popup_mode(qh);
        }
    }

    pub(super) fn draw_popup_mode(&mut self, qh: &QueueHandle<Self>) {
        let t0 = std::time::Instant::now();
        let scale = self.output_scale.max(1) as f32;
        let transparency = self.dock.config.settings.transparency;
        let Some(p) = self.popup_mode.as_ref() else {
            return;
        };
        let linear = p.anim.clamp(0.0, 1.0);
        let eased = 0.5 - 0.5 * (std::f32::consts::PI * linear).cos();

        let sw = (p.surface_w * scale).round().max(1.0) as i32;
        let sh = (p.surface_h * scale).round().max(1.0) as i32;
        let box_w = menu::MENU_WIDTH * scale;
        let box_h = p.content_height * scale;

        let args = menu_render::DrawArgs {
            screen: p.screen,
            controls: &p.controls,
            content_height: p.content_height,
            panel_width: menu::MENU_WIDTH,
            dock: &self.dock,
            app_entries: &[],
            icon_choices: &[],
            search_query: "",
            wallpapers: &[],
            wallpaper_hovered: None,
            wallpaper_scroll_x: 0.0,
            hovered: p.hovered,
            render_scale: scale,
            available_fonts: &[],
            open_dropdown: menu::OpenDropdown::None,
            font_query: "",
            custom_hex: &EMPTY_CUSTOM_HEX,
            custom_light: true,
            custom_focus: None,
            custom_name: "",
            custom_name_focused: false,
            custom_panel_blend: None,
            tray_items: &p.tray_items,
            volume_rows: &p.volume_rows,
            volume_devices: &p.volume_devices,
            overlay_tabs: None,
            slide_offset: 0.0,
            body_opacity: 1.0,
        };
        // ----- el contenido se renderiza UNA vez por apertura/cambio de estado:
        // rehacerlo en cada frame de la animación saturaba el hilo principal y
        // hacía que el menú tardara en aparecer y que los clicks se encolaran -----
        let need_w = box_w.round().max(1.0) as u32;
        let need_h = box_h.round().max(1.0) as u32;
        let reuse = self
            .popup_mode
            .as_ref()
            .and_then(|p| {
                p.content
                    .as_ref()
                    .map(|c| (c.width(), c.height(), p.content_dirty))
            })
            .is_some_and(|(w, h, dirty)| w == need_w && h == need_h && !dirty);
        if !reuse {
            let mut fresh = tiny_skia::Pixmap::new(need_w, need_h).unwrap();
            menu_render::draw_content(
                &mut fresh,
                &mut self.icon_cache,
                &mut self.text_cache,
                &self.thumbnail_cache,
                &args,
            );
            if let Some(p) = self.popup_mode.as_mut() {
                p.content = Some(fresh);
                p.content_dirty = false;
            }
        }

        let Some(p) = self.popup_mode.as_ref() else {
            return;
        };
        let Some(box_pixmap) = p.content.as_ref() else {
            return;
        };
        // ----- offsets enteros: con un desplazamiento fraccionario tiny-skia vuelve
        // a muestrear el contenido y el texto y los bordes salen borrosos -----
        let bx = (p.box_x * scale).round();
        let by = (p.box_y * scale).round();

        let mut pixmap = tiny_skia::Pixmap::new(sw as u32, sh as u32).unwrap();
        // ----- sin máscara de recorte: el panel se dibuja completo de una sola pasada
        // (se elimina el desplegado a hachazos y la allocación de máscara por frame) -----
        let paint = tiny_skia::PixmapPaint {
            opacity: anim_opacity(transparency, eased),
            ..Default::default()
        };
        pixmap.draw_pixmap(
            0,
            0,
            box_pixmap.as_ref(),
            &paint,
            tiny_skia::Transform::from_translate(bx, by),
            None,
        );

        let Some(p) = self.popup_mode.as_mut() else {
            return;
        };
        let stride = sw * 4;
        let (buffer, canvas) = p
            .pool
            .create_buffer(sw, sh, stride, wl_shm::Format::Argb8888)
            .expect("failed to create popup shm buffer");
        bgra_from_rgba(pixmap.data(), canvas);

        let surface = p.layer.wl_surface();
        surface.set_buffer_scale(self.output_scale.max(1));
        buffer
            .attach_to(surface)
            .expect("failed to attach popup buffer");
        surface.damage_buffer(0, 0, sw, sh);
        surface.frame(qh, surface.clone());
        p.awaiting_frame = true;
        log::debug!(
            "popup: t={} anim={linear:.2} cache={} frame={}ms surface=({},{}) box=({},{}) center={:?}",
            crate::app::hdbg_ms(),
            if reuse { "reuse" } else { "render" },
            t0.elapsed().as_millis(),
            p.surface_w,
            p.surface_h,
            p.box_x,
            p.box_y,
            p.center
        );
        surface.commit();
    }

    pub(super) fn tick_popup_frame(&mut self, qh: &QueueHandle<Self>) {
        let Some(p) = self.popup_mode.as_mut() else {
            return;
        };
        p.awaiting_frame = false;
        let animating = if p.anim < p.target_anim {
            p.anim = (p.anim + menu::ANIM_STEP_OPEN).min(p.target_anim);
            true
        } else if p.anim > p.target_anim {
            p.anim = (p.anim - menu::ANIM_STEP_CLOSE).max(p.target_anim);
            true
        } else {
            false
        };
        let closing = p.closing;
        let anim = p.anim;

        if closing && anim <= 0.0 {
            p.layer.wl_surface().attach(None, 0, 0);
            p.layer.wl_surface().commit();
            self.popup_mode = None;
            trim_heap();
            return;
        }
        if !animating {
            return;
        }
        self.draw_popup_mode(qh);
    }

    pub(super) fn handle_popup_pointer_event(
        &mut self,
        event: &PointerEvent,
        qh: &QueueHandle<Self>,
    ) {
        let (box_x, box_y) = self
            .popup_mode
            .as_ref()
            .map(|p| (p.box_x, p.box_y))
            .unwrap_or((0.0, 0.0));
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                let (x, y) = event.position;
                // ----- mientras el puntero esté en el popup, salir del dock no lo
                // cierra: el salto del icono al menú pasa por Leave+Enter en el
                // mismo lote de eventos -----
                if let Some(p) = self.popup_mode.as_mut() {
                    p.popup_hovered = true;
                }
                // ----- arrastre de una barra del panel de volumen: manda el
                // arrastre, no el hover, pero el hover se recalcula igual -----
                let _ = self.volume_panel_drag(x as f32, qh);
                // ----- sólo se toca el contenido si cambia la fila bajo el puntero:
                // antes se re-renderizaba el panel entero en CADA movimiento -----
                let changed = if let Some(p) = self.popup_mode.as_mut() {
                    let new = menu::hit_test(
                        &p.controls,
                        &self.dock.config.settings,
                        menu::MENU_WIDTH,
                        x as f32 - box_x,
                        y as f32 - box_y,
                    );
                    if p.hovered != new {
                        p.hovered = new;
                        p.content_dirty = true;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };
                if changed {
                    self.request_popup_redraw(qh);
                }
            }
            PointerEventKind::Leave { .. } => {
                // ----- el cierre lo decide el tick del autohide (`autohide_timeout`),
                // que sí sabe si el puntero sigue en el dock: cerrarlo acá no
                // alcanzaba porque al irse lejos del dock el popup no recibe
                // ningún Leave y quedaba abierto esperando el próximo click. -----
                let had_hover = self
                    .popup_mode
                    .as_ref()
                    .is_some_and(|p| p.hovered.is_some());
                if let Some(p) = self.popup_mode.as_mut() {
                    p.popup_hovered = false;
                    p.hovered = None;
                    p.content_dirty = true;
                }
                if had_hover {
                    self.request_popup_redraw(qh);
                }
                // ----- decidir el cierre en el tick del autohide: acá todavía no
                // se sabe si el puntero fue al dock o se fue lejos -----
                self.arm_popup_tick();
            }
            PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                let (x, y) = event.position;
                if self.volume_panel_press(x as f32, y as f32, qh) {
                    return;
                }
                let hit = self.popup_mode.as_ref().and_then(|p| {
                    menu::hit_test(
                        &p.controls,
                        &self.dock.config.settings,
                        menu::MENU_WIDTH,
                        x as f32 - box_x,
                        y as f32 - box_y,
                    )
                });
                self.handle_popup_click(hit, qh);
            }
            PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                let (x, _) = event.position;
                self.volume_panel_release(x as f32, qh);
            }
            _ => {}
        }
    }

    /// Teclado en los menús del popup (tray, energía y panel de volumen): ↑↓
    /// mueven el resaltado, Enter activa, ←/→ ajustan la fila de volumen y
    /// Escape cierra. El resaltado es el MISMO `hovered` que dibuja el mouse, así
    /// que con las flechas se ve exactamente lo mismo que con el puntero.
    pub(super) fn handle_popup_key(&mut self, event: KeyEvent, qh: &QueueHandle<Self>) {
        use menu::HitTarget;
        let Some(p) = self.popup_mode.as_ref() else {
            return;
        };
        let screen = p.screen;
        let targets = menu::panel_targets(
            &p.controls,
            &self.dock.config.settings,
            menu::MENU_WIDTH,
            &p.tray_items,
        );
        let cur = p.hovered;
        match event.keysym {
            Keysym::Escape => {
                log::debug!("teclado: Escape -> cierra popup {screen:?}");
                self.close_popup_mode(qh);
                return;
            }
            Keysym::Up | Keysym::Down => {
                let dir = if event.keysym == Keysym::Down { 1 } else { -1 };
                if let Some(next) = menu::nav_step(&targets, cur, dir) {
                    log::debug!("teclado: popup {cur:?} -> {next:?}");
                    self.set_popup_hovered(next, qh);
                }
                return;
            }
            Keysym::Left | Keysym::Right => {
                let dir = if event.keysym == Keysym::Right { 1 } else { -1 };
                match cur {
                    Some(HitTarget::VolumeTrack(i)) => self.nudge_volume_row(i, dir, qh),
                    // ----- el calendario no tiene nada que resaltar (sus targets
                    // están vacíos), así que acá `cur` es siempre `None` -----
                    None if screen == menu::MenuScreen::Calendar => {
                        self.step_calendar_month(dir, qh)
                    }
                    _ => {}
                }
                return;
            }
            Keysym::Return | Keysym::KP_Enter => {}
            _ => return,
        }
        let Some(target) = cur else {
            return;
        };
        // Enter sobre una fila de volumen mutea, igual que el icono de la fila
        let target = match target {
            HitTarget::VolumeTrack(i) => HitTarget::VolumeMute(i),
            other => other,
        };
        if self.volume_panel_hit(Some(target), 0.0, qh) {
            return;
        }
        self.handle_popup_click(Some(target), qh);
    }

    fn set_popup_hovered(&mut self, target: menu::HitTarget, qh: &QueueHandle<Self>) {
        if let Some(p) = self.popup_mode.as_mut() {
            p.hovered = Some(target);
            p.content_dirty = true;
        }
        self.request_popup_redraw(qh);
    }

    pub(super) fn handle_popup_click(
        &mut self,
        hit: Option<menu::HitTarget>,
        qh: &QueueHandle<Self>,
    ) {
        match hit {
            Some(menu::HitTarget::Button(kind)) => match kind {
                menu::ButtonKind::Suspend => {
                    crate::power::suspend();
                    self.close_popup_mode(qh);
                }
                menu::ButtonKind::Logout => {
                    crate::power::logout();
                    self.close_popup_mode(qh);
                }
                menu::ButtonKind::Reboot => {
                    crate::power::reboot();
                    self.close_popup_mode(qh);
                }
                menu::ButtonKind::Shutdown => {
                    crate::power::poweroff();
                    self.close_popup_mode(qh);
                }
                menu::ButtonKind::Back => {
                    let Some(p) = self.popup_mode.as_mut() else {
                        return;
                    };
                    let Some(items) = p.tray_stack.pop() else {
                        return;
                    };
                    let has_back = !p.tray_stack.is_empty();
                    self.set_tray_items(items, has_back, qh);
                }
                _ => {}
            },
            Some(menu::HitTarget::TrayItem(index)) => {
                let Some(p) = self.popup_mode.as_ref() else {
                    return;
                };
                let Some(item) = p.tray_items.get(index).cloned() else {
                    return;
                };
                if !item.enabled {
                    return;
                }
                if item.has_submenu {
                    let (service, menu_path) = (p.tray_service.clone(), p.tray_menu_path.clone());
                    let submenu = crate::tray::fetch_menu(&service, &menu_path, item.id);
                    if submenu.is_empty() {
                        return;
                    }
                    let Some(p) = self.popup_mode.as_mut() else {
                        return;
                    };
                    p.tray_stack.push(p.tray_items.clone());
                    self.set_tray_items(submenu, true, qh);
                } else {
                    crate::tray::send_menu_event(
                        p.tray_service.clone(),
                        p.tray_menu_path.clone(),
                        item.id,
                    );
                    self.close_popup_mode(qh);
                }
            }
            _ => {}
        }
    }
}
