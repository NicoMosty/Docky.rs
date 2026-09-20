use super::*;

/// Separación entre el dock y el panel de ajustes cuando el panel va debajo.
const PANEL_GAP: f32 = 8.0;

/// Reparto de la superficie compartida entre el dock y el panel, en unidades
/// lógicas.
#[derive(Clone, Copy, Debug)]
pub(super) struct PanelLayout {
    /// Tamaño de la superficie.
    pub surf: (f32, f32),
    /// Posición del dock, o `None` si el panel ocupa todo (bordes no superiores).
    pub dock_at: Option<(f32, f32)>,
    /// Posición del panel.
    pub panel_at: (f32, f32),
}

fn align_offset(outer: f32, inner: f32, align: crate::config::DockAlign) -> f32 {
    use crate::config::DockAlign;
    match align {
        DockAlign::Middle => ((outer - inner) / 2.0).max(0.0),
        DockAlign::Left => 0.0,
        DockAlign::Right => (outer - inner).max(0.0),
    }
}

/// Reparto de la superficie compartida que dibuja el dock y el panel: el dock queda
/// a la vista y el panel va AL LADO que le toca —debajo con el dock arriba, encima
/// con el dock abajo, a la derecha con el dock a la izquierda y a la izquierda con el
/// dock a la derecha—. Antes sólo el borde superior lo hacía: en los otros tres el
/// panel ocupaba la superficie entera y TAPABA el dock (por eso en el launcher con el
/// dock al costado no se veía la barra).
///
/// Los offsets salen de `align_offset` sobre el eje que NO comparten, así que el dock
/// y el panel quedan alineados entre sí con el mismo `dock_align` de siempre.
pub(super) fn panel_layout(
    s: &crate::config::DockSettings,
    dock_size: (u32, u32),
    panel_w: f32,
    panel_h: f32,
) -> PanelLayout {
    use crate::config::DockEdge;
    let (dock_w, dock_h) = (dock_size.0 as f32, dock_size.1 as f32);
    match s.dock_edge {
        DockEdge::Top => {
            let surf_w = dock_w.max(panel_w);
            PanelLayout {
                surf: (surf_w, dock_h + PANEL_GAP + panel_h),
                dock_at: Some((align_offset(surf_w, dock_w, s.dock_align), 0.0)),
                panel_at: (
                    align_offset(surf_w, panel_w, s.dock_align),
                    dock_h + PANEL_GAP,
                ),
            }
        }
        DockEdge::Bottom => {
            let surf_w = dock_w.max(panel_w);
            PanelLayout {
                surf: (surf_w, panel_h + PANEL_GAP + dock_h),
                dock_at: Some((
                    align_offset(surf_w, dock_w, s.dock_align),
                    panel_h + PANEL_GAP,
                )),
                panel_at: (align_offset(surf_w, panel_w, s.dock_align), 0.0),
            }
        }
        DockEdge::Left => {
            let surf_h = dock_h.max(panel_h);
            PanelLayout {
                surf: (dock_w + PANEL_GAP + panel_w, surf_h),
                dock_at: Some((0.0, align_offset(surf_h, dock_h, s.dock_align))),
                panel_at: (
                    dock_w + PANEL_GAP,
                    align_offset(surf_h, panel_h, s.dock_align),
                ),
            }
        }
        DockEdge::Right => {
            let surf_h = dock_h.max(panel_h);
            PanelLayout {
                surf: (panel_w + PANEL_GAP + dock_w, surf_h),
                dock_at: Some((
                    panel_w + PANEL_GAP,
                    align_offset(surf_h, dock_h, s.dock_align),
                )),
                panel_at: (0.0, align_offset(surf_h, panel_h, s.dock_align)),
            }
        }
    }
}

/// Dibuja el dock en `dst`, en la posición `at` (unidades lógicas). Va en su propio
/// pixmap porque `render::draw` pinta siempre desde (0,0) y la superficie compartida
/// puede ser más ancha y más alta que él. Es lo que deja el dock a la vista arriba
/// mientras el panel se abre debajo.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_dock_into(
    dst: &mut tiny_skia::Pixmap,
    dock: &crate::dock::Dock,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    widgets: &crate::widgets::WidgetSnapshot,
    tray: &crate::tray::TrayState,
    marquee: &mut crate::render::MarqueeState,
    at: (f32, f32),
    scale: f32,
) {
    let (dock_w, dock_h) = dock.base_size();
    let dw = (dock_w as f32 * scale).round() as i32;
    let dh = (dock_h as f32 * scale).round() as i32;
    if dw <= 0 || dh <= 0 {
        return;
    }
    let Some(mut dock_pixmap) = tiny_skia::Pixmap::new(dw as u32, dh as u32) else {
        return;
    };
    {
        let tray_icons = tray.lock().unwrap();
        let _ = crate::render::draw(
            &mut dock_pixmap,
            dock,
            icon_cache,
            text_cache,
            widgets,
            &tray_icons,
            marquee,
            false,
            false,
            scale,
            // ----- los paneles se dibujan con el dock entero: la animación de
            // aparición es de la superficie del dock, no de la composición del panel.
            // El split de la isla tampoco aplica: con el panel abierto el dock está
            // visible y la isla no existe. -----
            1.0,
            0.0,
        );
    }
    dst.draw_pixmap(
        (at.0 * scale).round() as i32,
        (at.1 * scale).round() as i32,
        dock_pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
}

impl App {
    pub(crate) fn toggle_dock_menu(&mut self, qh: &QueueHandle<Self>) {
        if self.dock_menu_mode.is_some() {
            self.close_dock_menu(qh);
        } else {
            self.open_dock_menu(qh);
        }
    }

    /// Tamaño lógico del panel del modo que comparte la superficie con el dock
    /// (el panel de ajustes y los tres del overlay). Los tres del overlay guardan el
    /// **frame** (la caja del contenido, ver `AppSearchMode::frame`) y el tamaño del
    /// panel sale de ahí con `menu::panel_size`: un solo número, no dos.
    pub(super) fn overlay_panel_size(&self) -> Option<(f32, f32)> {
        if let Some(m) = self.app_search_mode.as_ref() {
            Some(menu::panel_size(m.frame, m.is_vertical))
        } else if let Some(m) = self.clipboard_mode.as_ref() {
            Some(menu::panel_size(m.frame, m.is_vertical))
        } else if let Some(m) = self.notifications_mode.as_ref() {
            Some(menu::panel_size(m.frame, m.is_vertical))
        } else if let Some(m) = self.wallpaper_mode.as_ref() {
            Some(menu::panel_size(m.frame, m.is_vertical))
        } else {
            self.dock_menu_mode.as_ref().map(|m| (m.panel_w, m.panel_h))
        }
    }

    /// Reparto de la superficie compartida del modo abierto: con el dock arriba el
    /// panel va debajo (ver `panel_layout`), así el dock queda a la vista. `None` si
    /// no hay ningún panel abierto.
    pub(super) fn overlay_layout(&self) -> Option<PanelLayout> {
        let (w, h) = self.overlay_panel_size()?;
        Some(panel_layout(
            &self.dock.config.settings,
            self.dock.base_size(),
            w,
            h,
        ))
    }

    /// Coordenadas de la superficie -> coordenadas del panel. El panel ya no está en
    /// el origen cuando el dock se dibuja arriba, y sin esta resta los controles de
    /// TODOS los paneles que comparten la superficie quedan corridos justo esa
    /// distancia (y por lo tanto muertos).
    pub(super) fn panel_local(&self, x: f64, y: f64) -> (f64, f64) {
        match self.overlay_layout() {
            Some(l) => (x - l.panel_at.0 as f64, y - l.panel_at.1 as f64),
            None => (x, y),
        }
    }

    /// Muestra un panel ya dibujado en la superficie compartida: el dock arriba (si
    /// el panel va debajo) y el panel en su offset. Un solo camino para el panel de
    /// ajustes y los tres del overlay, así el tamaño de la superficie, la posición
    /// del panel y el `attach` no se pueden desincronizar entre sí.
    pub(super) fn show_panel_surface(
        &mut self,
        qh: &QueueHandle<Self>,
        panel: &tiny_skia::Pixmap,
        layout: PanelLayout,
        opacity: f32,
    ) {
        let scale = self.output_scale.max(1) as f32;
        let width = (layout.surf.0 * scale).round() as i32;
        let height = (layout.surf.1 * scale).round() as i32;
        if width <= 0 || height <= 0 {
            return;
        }
        let Some(mut pixmap) = tiny_skia::Pixmap::new(width as u32, height as u32) else {
            return;
        };
        if let Some((dock_x, dock_y)) = layout.dock_at {
            draw_dock_into(
                &mut pixmap,
                &self.dock,
                &mut self.icon_cache,
                &mut self.text_cache,
                &self.widgets,
                &self.tray,
                &mut self.marquee,
                (dock_x, dock_y),
                scale,
            );
        }
        pixmap.draw_pixmap(
            (layout.panel_at.0 * scale).round() as i32,
            (layout.panel_at.1 * scale).round() as i32,
            panel.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity,
                ..Default::default()
            },
            tiny_skia::Transform::identity(),
            None,
        );
        let stride = width * 4;
        let Ok((buffer, canvas)) =
            self.pool
                .create_buffer(width, height, stride, wl_shm::Format::Argb8888)
        else {
            log::error!("no pude crear el buffer del panel ({width}x{height})");
            return;
        };
        bgra_from_rgba(pixmap.data(), canvas);
        let surface = self.layer.wl_surface();
        surface.set_buffer_scale(self.output_scale.max(1));
        if buffer.attach_to(surface).is_err() {
            return;
        }
        surface.damage_buffer(0, 0, width, height);
        surface.frame(qh, surface.clone());
        self.awaiting_frame = true;
        surface.commit();
    }

    /// Devuelve la superficie compartida al tamaño del dock. Anota `applied_size`
    /// porque `sync_autohide_surfaces` nunca lo restaura (ver `apply_panel_size`):
    /// sin la anotación el dock queda dibujado dentro del rectángulo del panel.
    pub(super) fn restore_dock_size(&mut self) {
        let (w, h) = self.dock.base_size();
        self.layer.set_size(w, h);
        self.applied_size = Some((w, h));
    }

    /// Aplicar a la superficie el tamaño que necesita el panel de ajustes,
    /// ANOTÁNDOLO en `applied_size`/`applied_geom`. Sin esa anotación
    /// `relayout_dock` cree que el tamaño no cambió y al cerrar el menú la
    /// superficie queda con el del panel: el dock se dibuja dentro de ese
    /// rectángulo y se ve descentrado.
    ///
    /// El reparto depende del ancho del dock, así que se vuelve a llamar desde
    /// `relayout_dock` cada vez que un ajuste de layout cambia con el panel
    /// abierto (ver ahí). Idempotente: no re-setea lo que ya está puesto, porque
    /// cada `set_*` dispara un configure que puede quitarle el foco al puntero.
    pub(super) fn apply_panel_size(&mut self, panel_w: f32, panel_h: f32) {
        let s = &self.dock.config.settings;
        let (anchor, margin) = edge_anchor_margin(s.dock_edge, s.dock_align, s.pos_y, 0);
        if self.applied_geom != Some((anchor, margin)) {
            self.layer.set_anchor(anchor);
            self.layer
                .set_margin(margin.0, margin.1, margin.2, margin.3);
            self.applied_geom = Some((anchor, margin));
        }
        let (surf_w, surf_h) = panel_layout(s, self.dock.base_size(), panel_w, panel_h).surf;
        let (surf_w, surf_h) = (surf_w.round() as u32, surf_h.round() as u32);
        if self.applied_size != Some((surf_w, surf_h)) {
            self.layer.set_size(surf_w, surf_h);
            self.applied_size = Some((surf_w, surf_h));
        }
    }

    /// El editor del tab "Widgets" le escribe al widget elegido, así que sin
    /// ninguno puesto el tab no tendría a quién: arranca por el primero de la
    /// barra y, si el dock no tiene widgets, por el primero disponible. Los
    /// otros tabs no lo miran, así que se puede llamar siempre.
    fn ensure_widget_selection(&mut self) {
        let settings = &mut self.dock.config.settings;
        if settings.selected_widget.is_some() {
            return;
        }
        settings.selected_widget = settings
            .widgets
            .first()
            .map(|p| p.kind)
            .or_else(|| menu::widget_kind_order().first().copied());
    }

    pub(super) fn open_dock_menu(&mut self, qh: &QueueHandle<Self>) {
        self.wallpaper_mode = None;
        self.clipboard_mode = None;
        self.app_search_mode = None;
        self.osd_mode = None;
        self.notification_mode = None;
        self.popup_mode = None;
        self.held_key = None;
        self.layer.set_layer(Layer::Top);
        self.menu = None;
        self.ensure_widget_selection();
        let category = menu::MenuCategory::Layout;
        let controls = menu::build_category_controls(category, &self.dock.config.settings);
        let panel_w =
            menu::DOCK_MENU_LEFT_COL_W + menu::DOCK_MENU_DIVIDER_W + menu::DOCK_MENU_RIGHT_COL_W;
        let panel_h = menu::dock_menu_content_height(category, &self.dock.config.settings);
        self.apply_panel_size(panel_w, panel_h);
        // ----- Exclusive y no OnDemand: con OnDemand el compositor no le da el
        // foco de teclado a la superficie, así que ningún KeyEvent llega y
        // Escape no puede cerrar el menú por más que el handler lo contemple.
        // Es el mismo modo que usan app_search, clipboard y el resto. -----
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        // ----- el fundido de apertura: con las transiciones apagadas (o el panel
        // opaco) arranca terminado, así se dibuja UNA vez -----
        let anim = self.initial_panel_anim();
        self.dock_menu_mode = Some(DockMenuMode {
            category,
            controls,
            hovered: None,
            dragging_slider: None,
            held_stepper: None,
            dragging_widget: None,
            anim,
            target_anim: 1.0,
            closing: false,
            panel_w,
            panel_h,
            slide_anim: 1.0,
            slide_dir: 0.0,
            open_dropdown: menu::OpenDropdown::None,
            dropdown_anim: 0.0,
            dropdown_scroll: 0,
            font_query: String::new(),
            dropdown_selected: 0,
            custom_hex: Default::default(),
            custom_light: true,
            custom_focus: None,
            custom_name: String::new(),
            custom_name_focused: false,
            custom_panel_blend: None,
        });
        self.request_redraw(qh);
    }

    /// Invariante del teclado de la superficie compartida: la tiene SOLO mientras
    /// hay un modo que la necesita (es lo que hace funcionar Escape en el panel) y
    /// en OnDemand mientras hay un menú de iconos o del tray abierto. Se re-aplica
    /// en cada frame: si un camino cierra un modo sin soltar el teclado, queda preso
    /// y entonces NINGUNA tecla llega a ninguna aplicación. Con esto, cualquier
    /// camino que se olvide se auto-corrige al siguiente frame.
    ///
    /// El menú de íconos y los del popup NO pasan por acá: tienen superficie
    /// propia y se llevan su `Exclusive` al crearse (`open_menu`,
    /// `create_popup_surface`); el compositor lo suelta cuando la superficie se
    /// desmapea, así que no hay estado que se pueda quedar preso.
    pub(super) fn enforce_keyboard(&mut self) {
        let exclusive = self.dock_menu_mode.is_some()
            || self.app_search_mode.is_some()
            || self.notifications_mode.is_some()
            || self.clipboard_mode.is_some()
            || self.wallpaper_mode.is_some();
        let on_demand = self.menu.is_some() || self.popup_mode.is_some();
        // ----- idempotente (trampa 1): re-setear la interactividad en cada frame
        // puede re-disparar el foco del teclado en el compositor y perder teclas. -----
        let want: u8 = if exclusive {
            2
        } else if on_demand {
            1
        } else {
            0
        };
        if want == self.keyboard_state {
            return;
        }
        self.keyboard_state = want;
        self.layer.set_keyboard_interactivity(if exclusive {
            KeyboardInteractivity::Exclusive
        } else if on_demand {
            KeyboardInteractivity::OnDemand
        } else {
            KeyboardInteractivity::None
        });
    }

    pub(super) fn close_dock_menu(&mut self, qh: &QueueHandle<Self>) {
        // ----- cierre inmediato: la animación era de 20 frames (ANIM_STEP_CLOSE
        // = 0.05) y ese era el ~1s que tardaba en reaccionar después de Escape.
        // El teclado se suelta en el mismo paso: así no queda secuestrado -----
        if let Some(dm) = self.dock_menu_mode.as_mut() {
            dm.dragging_slider = None;
            dm.held_stepper = None;
        }
        self.dock_menu_mode = None;
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::None);
        self.relayout_dock(qh);
        // ----- el menú se quedó con los eventos del puntero, así que el estado del
        // autohide quedó viejo: `pointer_pos` y `ptr_left_at` son de antes de
        // abrirlo, y con `pointer_pos` viejo en `Some` el dock no se oculta nunca.
        // Se refresca como una salida: si el cursor sigue sobre la franja, el
        // compositor manda Enter/Motion y el dock se mantiene. -----
        self.dock.set_pointer(None);
        self.ptr_left_at = None;
        self.last_ptr_event = Some(std::time::Instant::now());
        self.autohide_armed = false;
        self.arm_autohide();
        trim_heap();
    }

    pub(super) fn switch_dock_menu_category(
        &mut self,
        category: menu::MenuCategory,
        qh: &QueueHandle<Self>,
    ) {
        // ----- antes de armar el tab: `Widgets` necesita su selección, y el alto
        // del panel sale de las filas que ese tab arme -----
        self.ensure_widget_selection();
        let panel_w = self.dock_menu_mode.as_ref().map(|dm| dm.panel_w);
        if let Some(dm) = self.dock_menu_mode.as_mut() {
            if category != dm.category {
                let dir = (menu::category_order_index(category)
                    - menu::category_order_index(dm.category))
                .signum() as f32;
                // ----- con las transiciones apagadas no se desliza: el panel se
                // redibuja ya terminado (offset 0) -----
                let smooth = self.dock.config.settings.smooth_transitions;
                dm.slide_dir = if smooth { dir } else { 0.0 };
                dm.slide_anim = if smooth { 0.0 } else { 1.0 };
                dm.panel_h = menu::dock_menu_content_height(category, &self.dock.config.settings);
            }
            dm.category = category;
            dm.controls = menu::build_category_controls(category, &self.dock.config.settings);
            dm.hovered = None;
            dm.open_dropdown = menu::OpenDropdown::None;
            dm.dropdown_scroll = 0;
            dm.font_query.clear();
            dm.dropdown_selected = 0;
        }
        if let (Some(panel_w), Some(panel_h)) =
            (panel_w, self.dock_menu_mode.as_ref().map(|dm| dm.panel_h))
        {
            self.apply_panel_size(panel_w, panel_h);
        }
        self.request_redraw(qh);
    }

    pub(super) fn draw_dock_menu_mode(&mut self, qh: &QueueHandle<Self>) {
        let scale = self.output_scale.max(1) as f32;
        let transparency = self.dock.config.settings.transparency;
        let Some(dm) = self.dock_menu_mode.as_mut() else {
            return;
        };
        let linear = dm.anim.clamp(0.0, 1.0);
        let eased = 0.5 - 0.5 * (std::f32::consts::PI * linear).cos();

        let layout = panel_layout(
            &self.dock.config.settings,
            self.dock.base_size(),
            dm.panel_w,
            dm.panel_h,
        );
        let panel_w_px = (dm.panel_w * scale).round() as i32;
        let panel_h_px = (dm.panel_h * scale).round() as i32;
        if panel_w_px <= 0 || panel_h_px <= 0 {
            return;
        }

        let slide_t = dm.slide_anim.clamp(0.0, 1.0);
        let slide_eased = 0.5 - 0.5 * (std::f32::consts::PI * slide_t).cos();
        let slide_offset = dm.slide_dir * (1.0 - slide_eased) * menu::TAB_SLIDE_DISTANCE * scale;

        let args = menu_render::DockMenuArgs {
            category: dm.category,
            controls: &dm.controls,
            panel_w: dm.panel_w,
            panel_h: dm.panel_h,
            slide_offset,
            dock: &self.dock,
            hovered: dm.hovered,
            render_scale: scale,
            dragging_widget: dm.dragging_widget,
            open_dropdown: dm.open_dropdown,
            dropdown_anim: dm.dropdown_anim,
            dropdown_scroll: dm.dropdown_scroll,
            available_fonts: &self.available_fonts,
            font_query: &dm.font_query,
            dropdown_selected: dm.dropdown_selected,
            custom_hex: &dm.custom_hex,
            custom_light: dm.custom_light,
            custom_focus: dm.custom_focus,
            custom_name: &dm.custom_name,
            custom_name_focused: dm.custom_name_focused,
            custom_panel_blend: dm.custom_panel_blend,
        };
        // ----- el panel se dibuja aparte y `show_panel_surface` lo pega en su
        // offset, con el dock nítido arriba: el fade de apertura no toca al dock -----
        let mut content = tiny_skia::Pixmap::new(panel_w_px as u32, panel_h_px as u32).unwrap();
        menu_render::draw_dock_menu(
            &mut content,
            &mut self.icon_cache,
            &mut self.text_cache,
            &args,
        );
        let opacity = if eased >= 0.999 {
            1.0
        } else {
            anim_opacity(transparency, eased)
        };
        self.show_panel_surface(qh, &content, layout, opacity);
    }

    pub(super) fn tick_dock_menu_frame(&mut self, qh: &QueueHandle<Self>) {
        let Some(dm) = self.dock_menu_mode.as_mut() else {
            return;
        };
        let fade_animating = if dm.anim < dm.target_anim {
            dm.anim = (dm.anim + menu::ANIM_STEP_OPEN).min(dm.target_anim);
            true
        } else if dm.anim > dm.target_anim {
            dm.anim = (dm.anim - menu::ANIM_STEP_CLOSE).max(dm.target_anim);
            true
        } else {
            false
        };
        let dropdown_target = if dm.open_dropdown != menu::OpenDropdown::None {
            1.0
        } else {
            0.0
        };
        let dropdown_animating = if dm.dropdown_anim < dropdown_target {
            dm.dropdown_anim = (dm.dropdown_anim + menu::ANIM_STEP_OPEN).min(dropdown_target);
            true
        } else if dm.dropdown_anim > dropdown_target {
            dm.dropdown_anim = (dm.dropdown_anim - menu::ANIM_STEP_CLOSE).max(dropdown_target);
            true
        } else {
            false
        };
        let slide_animating = if dm.slide_anim < 1.0 {
            dm.slide_anim = (dm.slide_anim + menu::ANIM_STEP_OPEN).min(1.0);
            true
        } else {
            false
        };
        let closing = dm.closing;
        let anim = dm.anim;
        let has_stepper = dm.held_stepper.is_some();

        if closing && anim <= 0.0 {
            // ----- una sola ruta de cierre: si el cierre se duplica acá, este
            // camino se saltea la restauración del tamaño y el reset del
            // autohide que sí hace `close_dock_menu`. -----
            self.close_dock_menu(qh);
            return;
        }
        if has_stepper {
            self.tick_dock_menu_held_stepper(qh);
            return;
        }

        let key_repeating = self.held_key.is_some();
        if let Some((keysym, steps)) = self.poll_held_key() {
            for _ in 0..steps {
                self.handle_dock_menu_key(
                    KeyEvent {
                        time: 0,
                        raw_code: 0,
                        keysym,
                        utf8: None,
                    },
                    qh,
                );
            }
        }

        if !fade_animating && !dropdown_animating && !slide_animating && !key_repeating {
            return;
        }
        self.draw_dock_menu_mode(qh);
    }

    pub(super) fn tick_dock_menu_held_stepper(&mut self, qh: &QueueHandle<Self>) {
        let Some((id, dir, frames)) = self.dock_menu_mode.as_ref().and_then(|dm| dm.held_stepper)
        else {
            return;
        };
        let (_, _, step) = id.range();
        let speed = menu::stepper_speed(frames);
        let cur = id.get(&self.dock.config.settings);
        id.set(
            &mut self.dock.config.settings,
            cur + dir * step * speed * 0.05,
        );
        self.on_setting_changed(id, qh, false);
        if let Some(dm) = self.dock_menu_mode.as_mut() {
            dm.held_stepper = Some((id, dir, frames + 1));
        }
        self.request_redraw(qh);
    }

    pub(super) fn start_dock_menu_stepper(
        &mut self,
        id: menu::SettingId,
        dir: f32,
        qh: &QueueHandle<Self>,
    ) {
        let (_, _, step) = id.range();
        let cur = id.get(&self.dock.config.settings);
        id.set(&mut self.dock.config.settings, cur + dir * step);
        self.on_setting_changed(id, qh, true);
        if let Some(dm) = self.dock_menu_mode.as_mut() {
            dm.held_stepper = Some((id, dir, 0));
        }
        self.request_redraw(qh);
    }

    pub(super) fn handle_dock_menu_button(
        &mut self,
        kind: menu::ButtonKind,
        qh: &QueueHandle<Self>,
    ) {
        match kind {
            menu::ButtonKind::AddApp => {
                // ----- cerrar por el camino único: suelta el teclado y refresca el
                // autohide. Antes el modo se limpiaba a mano y quedaba
                // KeyboardInteractivity::Exclusive con el estado del puntero viejo. -----
                self.close_dock_menu(qh);
                self.open_menu(menu::MenuScreen::AddApp, qh);
            }
            menu::ButtonKind::QuitDock => self.exit = true,
            // ----- cicla la carpeta de fondos: el selector la usa al abrirse -----
            menu::ButtonKind::WallpaperDir => {
                let opts = crate::wallpaper::WALLPAPER_DIRS;
                let cur = self.dock.config.settings.wallpaper_dir.clone();
                let i = opts.iter().position(|d| *d == cur).unwrap_or(0);
                self.dock.config.settings.wallpaper_dir = opts[(i + 1) % opts.len()].to_string();
                let _ = self.dock.config.save();
                log::debug!(
                    "wallpaper: carpeta -> {}",
                    self.dock.config.settings.wallpaper_dir
                );
                // ----- el label del botón sale de las settings: hay que rearmar -----
                if let Some(dm) = self.dock_menu_mode.as_mut() {
                    dm.controls =
                        menu::build_category_controls(dm.category, &self.dock.config.settings);
                }
                self.request_redraw(qh);
            }
            menu::ButtonKind::CreatePalette => self.open_custom_palette(qh),
            menu::ButtonKind::SavePalette => self.save_custom_palette(qh),
            menu::ButtonKind::Back => {
                if matches!(
                    self.dock_menu_mode.as_ref().map(|dm| dm.category),
                    Some(menu::MenuCategory::CustomPalette)
                ) {
                    self.switch_dock_menu_category(menu::MenuCategory::Colors, qh);
                }
            }
            _ => {}
        }
    }

    pub(super) fn open_custom_palette(&mut self, qh: &QueueHandle<Self>) {
        let s = &self.dock.config.settings;
        let hex = [
            rgb_to_hex(s.accent_r, s.accent_g, s.accent_b),
            rgb_to_hex(s.accent2_r, s.accent2_g, s.accent2_b),
            rgb_to_hex(s.panel_r, s.panel_g, s.panel_b),
            rgb_to_hex(s.text_r, s.text_g, s.text_b),
            rgb_to_hex(s.text_dim_r, s.text_dim_g, s.text_dim_b),
        ];
        let panel_luma =
            0.299 * s.panel_r as f32 + 0.587 * s.panel_g as f32 + 0.114 * s.panel_b as f32;
        let is_light = panel_luma > 140.0;
        self.switch_dock_menu_category(menu::MenuCategory::CustomPalette, qh);
        if let Some(dm) = self.dock_menu_mode.as_mut() {
            dm.custom_hex = hex;
            dm.custom_light = is_light;
            dm.custom_focus = None;
            dm.custom_name.clear();
            dm.custom_name_focused = false;
            dm.custom_panel_blend = None;
        }
        self.request_redraw(qh);
    }

    pub(super) fn reseed_custom_palette_base(&mut self, is_light: bool) {
        let (panel, text, text_dim) = if is_light {
            ((245, 245, 248), (30, 30, 34), (100, 100, 108))
        } else {
            ((18, 18, 20), (235, 235, 240), (150, 150, 158))
        };
        if let Some(dm) = self.dock_menu_mode.as_mut() {
            dm.custom_hex[2] = rgb_to_hex(panel.0, panel.1, panel.2);
            dm.custom_hex[3] = rgb_to_hex(text.0, text.1, text.2);
            dm.custom_hex[4] = rgb_to_hex(text_dim.0, text_dim.1, text_dim.2);
        }
    }

    // ----- preset blend ratio -----
    pub(super) fn apply_panel_blend(&mut self, accent_pct: u8, qh: &QueueHandle<Self>) {
        let Some(dm) = self.dock_menu_mode.as_mut() else {
            return;
        };
        let base = if dm.custom_light {
            (245u8, 245u8, 248u8)
        } else {
            (18u8, 18u8, 20u8)
        };
        let accent = menu::parse_hex(&dm.custom_hex[0]).unwrap_or((
            self.dock.config.settings.accent_r,
            self.dock.config.settings.accent_g,
            self.dock.config.settings.accent_b,
        ));
        let t = accent_pct as f32 / 100.0;
        let mix = |b: u8, a: u8| (b as f32 * (1.0 - t) + a as f32 * t).round() as u8;
        let panel = (
            mix(base.0, accent.0),
            mix(base.1, accent.1),
            mix(base.2, accent.2),
        );
        dm.custom_hex[2] = rgb_to_hex(panel.0, panel.1, panel.2);
        dm.custom_panel_blend = Some(accent_pct);
        self.request_redraw(qh);
    }

    pub(super) fn delete_custom_palette(&mut self, index: usize, qh: &QueueHandle<Self>) {
        if index >= self.dock.config.settings.custom_palettes.len() {
            return;
        }
        self.dock.config.settings.custom_palettes.remove(index);
        let _ = self.dock.config.save();

        let category = self.dock_menu_mode.as_ref().map(|dm| dm.category);
        let Some(category) = category else {
            self.request_redraw(qh);
            return;
        };
        let panel_h = menu::dock_menu_content_height(category, &self.dock.config.settings);
        let panel_w = self.dock_menu_mode.as_ref().map(|dm| dm.panel_w);
        if let Some(dm) = self.dock_menu_mode.as_mut() {
            dm.controls = menu::build_category_controls(category, &self.dock.config.settings);
            dm.panel_h = panel_h;
            dm.hovered = None;
        }
        if let Some(panel_w) = panel_w {
            self.apply_panel_size(panel_w, panel_h);
        }
        self.request_redraw(qh);
    }

    pub(super) fn save_custom_palette(&mut self, qh: &QueueHandle<Self>) {
        let Some(dm) = self.dock_menu_mode.as_ref() else {
            return;
        };
        let hex = dm.custom_hex.clone();
        let is_light = dm.custom_light;
        let name = if dm.custom_name.trim().is_empty() {
            "My Palette".to_string()
        } else {
            dm.custom_name.trim().to_string()
        };
        let cur = &self.dock.config.settings;
        let fallback = [
            (cur.accent_r, cur.accent_g, cur.accent_b),
            (cur.accent2_r, cur.accent2_g, cur.accent2_b),
            (cur.panel_r, cur.panel_g, cur.panel_b),
            (cur.text_r, cur.text_g, cur.text_b),
            (cur.text_dim_r, cur.text_dim_g, cur.text_dim_b),
        ];
        let color_at = |i: usize| menu::parse_hex(&hex[i]).unwrap_or(fallback[i]);
        let custom = crate::config::CustomPalette {
            name,
            accent: color_at(0),
            accent2: color_at(1),
            panel: color_at(2),
            text: color_at(3),
            text_dim: color_at(4),
            is_light,
        };
        self.dock.config.settings.custom_palettes.push(custom);
        let _ = self.dock.config.save();
        self.switch_dock_menu_category(menu::MenuCategory::Colors, qh);
    }
}

#[cfg(test)]
mod panel_layout_tests {
    use super::*;
    use crate::config::{DockAlign, DockEdge, DockSettings};

    fn settings() -> DockSettings {
        DockSettings {
            dock_edge: DockEdge::Top,
            dock_align: DockAlign::Middle,
            ..Default::default()
        }
    }

    /// Contrato que rompería el bug de "Dock Width": con el panel abierto el dock
    /// se dibuja en `dock_at` DENTRO de la superficie, así que la superficie tiene
    /// que crecer con el dock. Si `surf.0` quedara fijo en el ancho del panel,
    /// `align_offset` caería a 0 y el dock aparecería pegado al borde izquierdo
    /// (descentrado y con la punta derecha recortada).
    #[test]
    fn la_superficie_crece_con_el_dock_y_lo_deja_centrado() {
        let s = settings();
        let (panel_w, panel_h) = (720.0, 400.0);
        for dock_w in [200u32, 400, 720, 900, 2400] {
            let l = panel_layout(&s, (dock_w, 26), panel_w, panel_h);
            assert!(
                l.surf.0 >= dock_w as f32,
                "superficie más angosta que el dock ({dock_w})"
            );
            let (dx, _) = l
                .dock_at
                .expect("borde superior: el dock va arriba del panel");
            assert!(
                (dx - (l.surf.0 - dock_w as f32) / 2.0).abs() < 0.01,
                "dock descentrado con ancho {dock_w}: x={dx} en surf={}",
                l.surf.0
            );
            assert!(
                dx + dock_w as f32 <= l.surf.0 + 0.01,
                "el dock se sale de la superficie"
            );
        }
    }

    /// El contrato vertical del que dependen los CUATRO paneles que comparten la
    /// superficie (ajustes, launcher/ventanas, portapapeles y fondos): el dock queda
    /// a la vista y el panel va al lado que le toca según el borde. El dibujo y los
    /// hit tests salen del MISMO reparto (`overlay_layout` / `panel_local`), así que
    /// si esto se rompe el panel se ve en un lado y los clicks responden en otro.
    #[test]
    fn el_panel_va_al_lado_del_dock_en_los_cuatro_bordes() {
        let s = settings();
        let l = panel_layout(&s, (603, 26), 640.0, 460.0);
        assert_eq!(
            l.surf,
            (640.0, 26.0 + PANEL_GAP + 460.0),
            "la superficie es [dock][gap][panel]"
        );
        assert_eq!(l.dock_at.map(|(_, y)| y), Some(0.0), "el dock va arriba");
        assert_eq!(
            l.panel_at.1,
            26.0 + PANEL_GAP,
            "el panel va debajo del dock"
        );

        // ----- dock abajo: el panel va ENCIMA del dock -----
        let mut s2 = settings();
        s2.dock_edge = DockEdge::Bottom;
        let l2 = panel_layout(&s2, (603, 26), 640.0, 460.0);
        assert_eq!(l2.surf, (640.0, 460.0 + PANEL_GAP + 26.0));
        assert_eq!(l2.panel_at.1, 0.0, "el panel va arriba");
        assert_eq!(
            l2.dock_at.map(|(_, y)| y),
            Some(460.0 + PANEL_GAP),
            "el dock va abajo"
        );

        // ----- dock a la izquierda: el panel a la derecha, misma altura de barra -----
        let mut s3 = settings();
        s3.dock_edge = DockEdge::Left;
        let l3 = panel_layout(&s3, (26, 610), 356.0, 556.0);
        assert_eq!(l3.surf, (26.0 + PANEL_GAP + 356.0, 610.0));
        assert_eq!(
            l3.dock_at,
            Some((0.0, 0.0)),
            "el dock mide lo mismo que la superficie: pegado arriba"
        );
        assert_eq!(
            l3.panel_at,
            (26.0 + PANEL_GAP, 27.0),
            "el panel a la derecha del dock y centrado (556 en 610)"
        );

        // ----- y con el dock a la derecha el panel va a la izquierda -----
        let mut s4 = settings();
        s4.dock_edge = DockEdge::Right;
        let l4 = panel_layout(&s4, (26, 610), 356.0, 556.0);
        assert_eq!(l4.surf, (356.0 + PANEL_GAP + 26.0, 610.0));
        assert_eq!(l4.panel_at, (0.0, 27.0), "el panel primero");
        assert_eq!(
            l4.dock_at,
            Some((356.0 + PANEL_GAP, 0.0)),
            "y el dock pegado al borde derecho"
        );

        // ----- en los cuatro casos el panel entra en la superficie -----
        for l in [l, l2, l3, l4] {
            assert!(
                l.panel_at.0 >= 0.0
                    && l.panel_at.1 >= 0.0
                    && l.panel_at.0 + 1.0 <= l.surf.0
                    && l.panel_at.1 + 1.0 <= l.surf.1,
                "el panel se sale de la superficie: {l:?}"
            );
        }
    }
}
