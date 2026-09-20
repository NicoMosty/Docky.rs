use super::*;

impl App {
    pub(crate) fn toggle_app_search(&mut self, qh: &QueueHandle<Self>) {
        if self.app_search_mode.is_some() {
            self.close_app_search_mode(qh);
        } else {
            self.open_app_search(qh);
        }
    }

    pub(super) fn open_app_search(&mut self, qh: &QueueHandle<Self>) {
        self.open_search(SearchList::Apps, qh);
    }

    /// Cambiador de ventanas: es el mismo panel del launcher, pero la lista son las
    /// ventanas abiertas de niri y Enter las enfoca en vez de lanzar algo.
    pub(super) fn open_windows_mode(&mut self, qh: &QueueHandle<Self>) {
        self.open_search(SearchList::Windows, qh);
    }

    fn open_search(&mut self, list: SearchList, qh: &QueueHandle<Self>) {
        self.wallpaper_mode = None;
        self.clipboard_mode = None;
        self.dock_menu_mode = None;
        self.osd_mode = None;
        self.popup_mode = None;
        self.held_key = None;
        self.layer.set_layer(Layer::Top);
        self.menu = None;

        let all_entries = match list {
            SearchList::Apps => desktop::list_all_desktop_entries(),
            SearchList::Windows => desktop::list_niri_windows(),
        };
        let is_vertical = self.dock.is_vertical();
        let band_left = self.dock.band_left();
        // ----- el frame es la caja del CONTENIDO: en el panel ancho mide lo de
        // siempre (640) y en el vertical dos columnas de tarjeta, y la banda de
        // pestañas vive fuera de la caja (fila arriba o columna al costado) -----
        let content_w = if is_vertical {
            menu::app_search_vertical_content_w()
        } else {
            // ----- mismo ancho que el portapapeles y el selector de fondos
            // (`OVERLAY_PANEL_W`): los tres van centrados con el mismo anclaje, así
            // que al ciclar con Shift+←/→ los bordes no se mueven. -----
            menu::OVERLAY_PANEL_W
        };
        let base = menu::frame_for(content_w, 0.0, is_vertical, band_left);
        let (controls, content_h) = menu::build_app_search_controls(base, is_vertical);
        let frame = menu::PanelFrame {
            // ----- en el vertical el alto es el COMÚN del overlay (640) y la grilla
            // scrollea dentro: el borde no se mueve al ciclar con Shift←/→. En el
            // horizontal el alto sigue saliendo de las dos filas de tarjetas. -----
            h: if is_vertical {
                menu::OVERLAY_PANEL_H
            } else {
                content_h
            },
            ..base
        };
        let (panel_w, panel_h) = menu::panel_size(frame, is_vertical);
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        // ----- el panel va DEBAJO del dock, misma composición que el panel de
        // ajustes (`panel_layout`): la superficie pasa a ser [dock][8px][panel] y el
        // dock se queda a la vista en vez de quedar tapado. -----
        self.apply_panel_size(panel_w, panel_h);
        let content_anim = self.initial_content_anim();
        self.app_search_mode = Some(AppSearchMode {
            list,
            query: String::new(),
            all_entries,
            filtered: Vec::new(),
            controls,
            selected: 0,
            scroll_x: 0.0,
            scroll_target: 0.0,
            highlight_x: 0.0,
            highlight_target: 0.0,
            highlight_y: 0.0,
            highlight_target_y: 0.0,
            content_anim,
            anim: self.initial_panel_anim(),
            target_anim: 1.0,
            closing: false,
            frame,
            is_vertical,
            slide_dir: 0.0,
        });
        self.refresh_app_search(qh);
    }

    pub(super) fn close_app_search_mode(&mut self, qh: &QueueHandle<Self>) {
        // ----- cierre inmediato: con la animación dependía del tick de frames, y en
        // un cambio de modo (Shift+←/→) se cruzaba con el que entra. Mismo criterio
        // que el panel de ajustes y el selector de fondos. -----
        if self.app_search_mode.is_none() {
            return;
        }
        self.app_search_mode = None;
        self.held_key = None;
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::None);
        self.restore_dock_size();
        self.draw(qh);
        trim_heap();
    }

    pub(super) fn refresh_app_search(&mut self, qh: &QueueHandle<Self>) {
        let content_anim = self.initial_content_anim();
        let Some(m) = self.app_search_mode.as_mut() else {
            return;
        };
        let query = m.query.to_lowercase();
        // ----- como rofi: primero la calidad del match (prefijo > inicio de palabra
        // > contiene > difuso) y después lo que más usás. Con la caja vacía queda
        // ordenado sólo por uso, que es lo que hace rofi. -----
        let mut scored: Vec<(usize, u8, f64)> = m
            .all_entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let tier = desktop::match_tier(e, &query)?;
                Some((i, tier, crate::usage::score(&e.id)))
            })
            .collect();
        scored.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| {
                    m.all_entries[a.0]
                        .name
                        .to_lowercase()
                        .cmp(&m.all_entries[b.0].name.to_lowercase())
                })
        });
        m.filtered = scored
            .into_iter()
            .take(menu::SEARCH_MAX_RESULTS)
            .map(|(i, _, _)| m.all_entries[i].clone())
            .collect();
        m.selected = 0;
        m.scroll_x = 0.0;
        m.scroll_target = 0.0;
        m.highlight_x = 0.0;
        m.highlight_target = 0.0;
        m.highlight_y = 0.0;
        m.highlight_target_y = 0.0;
        m.content_anim = content_anim;
        self.request_redraw(qh);
    }

    pub(super) fn scroll_search_into_view(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let Some(m) = self.app_search_mode.as_mut() else {
            return;
        };
        // ----- el scroll se mide en tramos (la página entera en el horizontal, la
        // tarjeta en el vertical), pero el resaltado va a la tarjeta exacta: si no,
        // la pastilla quedaría en el borde de la página en vez de sobre la app. -----
        let along = menu::app_search_card_along(index, m.frame, m.is_vertical);
        let along_len = menu::app_search_along_len(m.frame, m.is_vertical);
        let viewport_along = menu::app_search_viewport_along(m.frame, m.is_vertical);
        let mut target = m.scroll_target;
        if along < target {
            target = along;
        } else if along + along_len > target + viewport_along {
            target = along + along_len - viewport_along;
        }
        let max_scroll = menu::app_search_max_scroll(m.filtered.len(), m.frame, m.is_vertical);
        m.scroll_target = target.clamp(0.0, max_scroll);
        let (ox, oy) = menu::app_search_card_offset(index, m.frame, m.is_vertical);
        m.highlight_target = ox;
        m.highlight_target_y = oy;
        self.request_redraw(qh);
    }

    pub(super) fn launch_search_result(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let list = self.app_search_mode.as_ref().map(|m| m.list);
        if let Some(entry) = self
            .app_search_mode
            .as_ref()
            .and_then(|m| m.filtered.get(index))
            .cloned()
        {
            if list == Some(SearchList::Windows) {
                // ----- en el cambiador de ventanas, Enter enfoca: el `id` de la
                // entrada es el id de ventana de niri -----
                if let Ok(id) = entry.id.parse::<u64>() {
                    crate::widgets::niri_focus_window(id);
                }
            } else {
                // ----- historial: es lo que después ordena la lista como rofi -----
                crate::usage::record(&entry.id);
                if entry.terminal {
                    let term = std::env::var("TERMINAL").unwrap_or_else(|_| "kitty".to_string());
                    let _ = std::process::Command::new(term)
                        .args(["-e", "sh", "-c", &entry.exec])
                        .spawn();
                } else {
                    launch_app(&entry.exec);
                }
            }
        }
        self.close_app_search_mode(qh);
    }

    pub(super) fn draw_app_search_mode(&mut self, qh: &QueueHandle<Self>) {
        let scale = self.output_scale.max(1) as f32;
        let transparency = self.dock.config.settings.transparency;
        // ----- con las transiciones apagadas el strip no funde: va directo a 1.0 -----
        let smooth = self.dock.config.settings.smooth_transitions;
        // ----- la banda marca el modo: launcher y ventanas son la misma lista y se
        // distinguen sólo por la pestaña encendida -----
        let tabs = self.current_overlay().map(OverlayMode::tab_index);
        // ----- el reparto (dock arriba, panel abajo) se calcula ANTES de tomar
        // prestado el modo: `overlay_layout` necesita `&self` entero -----
        let Some(layout) = self.overlay_layout() else {
            return;
        };
        let Some(m) = self.app_search_mode.as_mut() else {
            return;
        };
        let linear = m.anim.clamp(0.0, 1.0);
        let eased = 0.5 - 0.5 * (std::f32::consts::PI * linear).cos();

        // ----- el tamaño del panel sale del frame: un solo numero -----
        let (panel_w, panel_h) = menu::panel_size(m.frame, m.is_vertical);
        let width = (panel_w * scale).round() as i32;
        let height = (panel_h * scale).round() as i32;
        if width <= 0 || height <= 0 {
            return;
        }

        let args = menu_render::DrawArgs {
            screen: menu::MenuScreen::AddApp,
            controls: &m.controls,
            content_height: panel_h,
            panel_width: panel_w,
            dock: &self.dock,
            app_entries: &m.filtered,
            icon_choices: &[],
            search_query: &m.query,
            wallpapers: &[],
            wallpaper_hovered: None,
            wallpaper_scroll_x: m.scroll_x,
            // ----- las tarjetas no usan el hover del args: el resaltado y el color
            // de la etiqueta los decide la pastilla (ver `app_search_hot_card`). -----
            hovered: None,
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
            tray_items: &[],
            overlay_tabs: tabs,
            volume_rows: &[],
            volume_devices: &[],
            slide_offset: menu::overlay_slide_offset(m.slide_dir, m.anim),
            body_opacity: anim_opacity(transparency, eased),
        };
        let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32).unwrap();
        menu_render::draw_app_search(
            &mut pixmap,
            &mut self.icon_cache,
            &mut self.text_cache,
            &args,
            (m.highlight_x, m.highlight_y),
            if smooth { m.content_anim } else { 1.0 },
        );
        // ----- el panel ya trae su propio fundido en el cuerpo; la superficie se
        // compone opaca, con el dock nítido arriba -----
        self.show_panel_surface(qh, &pixmap, layout, 1.0);
    }

    pub(super) fn tick_app_search_frame(&mut self, qh: &QueueHandle<Self>) {
        // ----- con las transiciones apagadas nada de acá adentro se anima: el
        // scroll, el resaltado y el fundido del strip saltan directo al valor
        // final, así cada interacción cuesta UN frame en vez de ~8 (el lerp del
        // resaltado es el que más se paga, porque sigue al puntero) -----
        let smooth = self.dock.config.settings.smooth_transitions;
        let Some(m) = self.app_search_mode.as_mut() else {
            return;
        };
        let fade_animating = if m.anim < m.target_anim {
            m.anim = (m.anim + menu::ANIM_STEP_OPEN).min(m.target_anim);
            true
        } else if m.anim > m.target_anim {
            m.anim = (m.anim - menu::ANIM_STEP_CLOSE).max(m.target_anim);
            true
        } else {
            false
        };
        let scroll_delta = m.scroll_target - m.scroll_x;
        let scroll_animating = if smooth && scroll_delta.abs() > 0.5 {
            m.scroll_x += scroll_delta * 0.28;
            true
        } else {
            m.scroll_x = m.scroll_target;
            false
        };
        let highlight_delta = m.highlight_target - m.highlight_x;
        let highlight_delta_y = m.highlight_target_y - m.highlight_y;
        let highlight_animating =
            if smooth && (highlight_delta.abs() > 0.5 || highlight_delta_y.abs() > 0.5) {
                m.highlight_x += highlight_delta * 0.35;
                m.highlight_y += highlight_delta_y * 0.35;
                true
            } else {
                m.highlight_x = m.highlight_target;
                m.highlight_y = m.highlight_target_y;
                false
            };
        let content_animating = if smooth && m.content_anim < 1.0 {
            m.content_anim = (m.content_anim + 0.16).min(1.0);
            true
        } else {
            m.content_anim = 1.0;
            false
        };
        let closing = m.closing;
        let anim = m.anim;

        if closing && anim <= 0.0 {
            self.app_search_mode = None;
            self.layer
                .set_keyboard_interactivity(KeyboardInteractivity::None);
            self.restore_dock_size();
            self.draw(qh);
            trim_heap();
            return;
        }

        let key_repeating = self.held_key.is_some();
        if let Some((keysym, steps)) = self.poll_held_key() {
            for _ in 0..steps {
                self.handle_app_search_key(
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

        if !fade_animating
            && !scroll_animating
            && !highlight_animating
            && !content_animating
            && !key_repeating
        {
            return;
        }
        self.draw_app_search_mode(qh);
    }

    pub(super) fn handle_app_search_pointer_event(
        &mut self,
        event: &PointerEvent,
        qh: &QueueHandle<Self>,
    ) {
        // ----- las coordenadas del puntero son de la SUPERFICIE, y el panel ya no
        // arranca en el origen cuando el dock se dibuja arriba -----
        let (px, py) = self.panel_local(event.position.0, event.position.1);
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                let (x, y) = (px, py);
                if let Some(m) = self.app_search_mode.as_mut() {
                    let strip_hit = menu::app_search_strip_hit_test(
                        m.filtered.len(),
                        m.frame,
                        m.is_vertical,
                        m.scroll_x,
                        x as f32,
                        y as f32,
                    );
                    if let Some(i) = strip_hit {
                        // ----- pasar el mouse por una tarjeta también la elige: si no,
                        // Enter lanzaba la 1ª (la seleccionada por teclado) mientras la
                        // pastilla marcaba la que estabas mirando. Es lo que hace rofi, y
                        // es lo que deja a la pastilla como única fuente de verdad.
                        // Ojo: al SALIR de las tarjetas no se toca nada, así que el
                        // resaltado se queda donde estaba en vez de saltar a la 1ª. -----
                        m.selected = i;
                        let (ox, oy) = menu::app_search_card_offset(i, m.frame, m.is_vertical);
                        m.highlight_target = ox;
                        m.highlight_target_y = oy;
                    }
                }
                self.request_redraw(qh);
            }
            PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                let (x, y) = (px, py);
                let hit = self.app_search_mode.as_ref().and_then(|m| {
                    menu::app_search_strip_hit_test(
                        m.filtered.len(),
                        m.frame,
                        m.is_vertical,
                        m.scroll_x,
                        x as f32,
                        y as f32,
                    )
                });
                if let Some(i) = hit {
                    self.launch_search_result(i, qh);
                }
            }
            PointerEventKind::Press { button, .. } if button == BTN_RIGHT => {
                self.close_app_search_mode(qh);
            }
            PointerEventKind::Axis {
                horizontal,
                vertical,
                ..
            } => {
                let delta = if horizontal.absolute != 0.0 {
                    horizontal.absolute
                } else {
                    vertical.absolute
                };
                if let Some(m) = self.app_search_mode.as_mut() {
                    let max_scroll =
                        menu::app_search_max_scroll(m.filtered.len(), m.frame, m.is_vertical);
                    m.scroll_target = (m.scroll_target + delta as f32).clamp(0.0, max_scroll);
                }
                self.request_redraw(qh);
            }
            _ => {}
        }
    }

    pub(super) fn handle_app_search_key(&mut self, event: KeyEvent, qh: &QueueHandle<Self>) {
        if event.keysym == Keysym::Escape {
            self.close_app_search_mode(qh);
            return;
        }
        match event.keysym {
            Keysym::Return => {
                let selected = self
                    .app_search_mode
                    .as_ref()
                    .map(|m| m.selected)
                    .unwrap_or(0);
                self.launch_search_result(selected, qh);
                return;
            }
            // ----- ←/→ siguen el orden de lectura (la fila) y ↑/↓ saltan a la
            // otra fila de la grilla: con dos filas, ±1 arriba/abajo repetiría el
            // vecino de al lado. En el panel vertical hay una sola columna. -----
            Keysym::Right | Keysym::Left => {
                let delta = if event.keysym == Keysym::Right { 1 } else { -1 };
                self.step_app_search(delta, qh);
                return;
            }
            Keysym::Down | Keysym::Up => {
                let cols = self
                    .app_search_mode
                    .as_ref()
                    .map(|m| menu::app_search_row_step(m.frame, m.is_vertical))
                    .unwrap_or(1);
                let delta = if event.keysym == Keysym::Down {
                    cols
                } else {
                    -cols
                };
                self.step_app_search(delta, qh);
                return;
            }
            _ => {}
        }
        if let Some(m) = self.app_search_mode.as_mut() {
            if event.keysym == Keysym::BackSpace {
                m.query.pop();
            } else if let Some(text) = &event.utf8 {
                for ch in text.chars() {
                    if !ch.is_control() {
                        m.query.push(ch);
                    }
                }
            }
        }
        self.refresh_app_search(qh);
    }

    /// Mueve el resaltado `delta` tarjetas, saturado en los extremos (no da la
    /// vuelta) y scrolleando para que quede visible.
    fn step_app_search(&mut self, delta: isize, qh: &QueueHandle<Self>) {
        let next = self.app_search_mode.as_ref().and_then(|m| {
            if m.filtered.is_empty() {
                return None;
            }
            let last = m.filtered.len() as isize - 1;
            Some((m.selected as isize + delta).clamp(0, last) as usize)
        });
        if let Some(next) = next {
            if let Some(m) = self.app_search_mode.as_mut() {
                m.selected = next;
            }
            self.scroll_search_into_view(next, qh);
        }
    }
}
