use super::*;

impl App {
    pub(crate) fn toggle_wallpaper_picker(&mut self, qh: &QueueHandle<Self>) {
        if self.wallpaper_mode.is_some() {
            self.close_wallpaper_mode(qh);
        } else {
            self.open_wallpaper_picker(qh);
        }
    }

    pub(super) fn open_wallpaper_picker(&mut self, qh: &QueueHandle<Self>) {
        self.dock_menu_mode = None;
        self.app_search_mode = None;
        self.clipboard_mode = None;
        self.osd_mode = None;
        self.popup_mode = None;
        self.held_key = None;
        self.layer.set_layer(Layer::Top);
        self.menu = None;
        let wallpapers = wallpaper::scan_wallpapers(&self.dock.config.settings.wallpaper_dir);
        let is_vertical = self.dock.is_vertical();
        // ----- el frame es la caja del CONTENIDO: la banda de pestañas vive fuera
        // (fila arriba en el panel ancho, columna al costado del dock en el
        // vertical), así que el filmstrip ya no la cuenta -----
        let (content_w, content_h) = if is_vertical {
            // ----- en el panel vertical el "ancho" es el cross: queda el de
            // siempre (una columna angosta junto al dock) y el largo es el ALTO
            // COMÚN del overlay, para que el panel no cambie de tamaño al ciclar
            // con Shift+←/→ (el filmstrip scrollea dentro) -----
            (
                menu::overlay_vertical_cross(self.dock.thickness() as f32),
                menu::OVERLAY_PANEL_H,
            )
        } else {
            // ----- mismo ancho que el launcher y el portapapeles -----
            (menu::OVERLAY_PANEL_W, WALLPAPER_PANEL_H)
        };
        let band_left = self.dock.band_left();
        let frame = menu::frame_for(content_w, content_h, is_vertical, band_left);
        let (panel_w, panel_h) = menu::panel_size(frame, is_vertical);
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        // ----- el panel va DEBAJO del dock, como el panel de ajustes -----
        self.apply_panel_size(panel_w, panel_h);

        let (tw_logical, th_logical) = menu::wallpaper_thumb_size(frame, is_vertical);
        let scale = self.output_scale.max(1) as f32;
        let thumb_w = (tw_logical * scale).round().max(1.0) as u32;
        let thumb_h = (th_logical * scale).round().max(1.0) as u32;
        // ----- el radio va horneado en el pixmap y lo re-dibuja el anillo del
        // hover, así que sale de la misma constante (si no, la esquina de la
        // imagen y el anillo no coinciden) -----
        let thumb_radius = menu::OVERLAY_RADIUS * scale;

        let (thumb_request_tx, request_rx) =
            std::sync::mpsc::channel::<(std::path::PathBuf, u32, u32)>();
        let (result_tx, thumb_result_rx) = std::sync::mpsc::channel();
        // ----- el caché en disco vive en `~/.cache/dockyrs/thumbs`: decodificar el
        // original (un 4K son ~85 ms y ~33 MB de pico) se paga una vez por imagen y
        // tamaño, no en cada visita al selector. -----
        let cache_dir = crate::usage::cache_dir().map(|d| d.join("thumbs"));
        std::thread::spawn(move || {
            while let Ok((path, w, h)) = request_rx.recv() {
                let pixmap = dockyrs_canvas::load_thumbnail_cached(
                    &path,
                    w,
                    h,
                    thumb_radius,
                    cache_dir.as_deref(),
                );
                if result_tx.send((path, w, h, pixmap)).is_err() {
                    return;
                }
            }
        });

        self.wallpaper_mode = Some(WallpaperMode {
            wallpapers,
            hovered: None,
            scroll_x: 0.0,
            scroll_target: 0.0,
            anim: self.initial_panel_anim(),
            target_anim: 1.0,
            closing: false,
            frame,
            is_vertical,
            slide_dir: 0.0,
            thumb_w,
            thumb_h,
            thumb_requested: std::collections::HashSet::new(),
            thumbs_pending: 0,
            thumb_request_tx,
            thumb_result_rx,
        });
        self.request_visible_thumbnails();
        self.request_redraw(qh);
    }

    pub(super) fn request_visible_thumbnails(&mut self) {
        let Some(wp) = self.wallpaper_mode.as_mut() else {
            return;
        };
        let (tw_logical, th_logical) = menu::wallpaper_thumb_size(wp.frame, wp.is_vertical);
        let along_size = if wp.is_vertical {
            th_logical
        } else {
            tw_logical
        };
        let viewport_along = if wp.is_vertical {
            wp.frame.h
        } else {
            wp.frame.w
        };
        let margin = along_size * 2.0;
        let visible_0 = wp.scroll_x - margin;
        let visible_1 = wp.scroll_x + viewport_along + margin;
        for (idx, entry) in wp.wallpapers.iter().enumerate() {
            let local = idx as f32 * (along_size + menu::WALLPAPER_GAP);
            if local + along_size < visible_0 || local > visible_1 {
                continue;
            }
            if wp.thumb_requested.contains(&entry.path) {
                continue;
            }
            wp.thumb_requested.insert(entry.path.clone());
            if wp
                .thumb_request_tx
                .send((entry.path.clone(), wp.thumb_w, wp.thumb_h))
                .is_ok()
            {
                wp.thumbs_pending += 1;
            }
        }
    }

    pub(super) fn choose_wallpaper(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let path = self
            .wallpaper_mode
            .as_ref()
            .and_then(|w| w.wallpapers.get(index))
            .map(|e| e.path.clone());
        if let Some(path) = &path {
            wallpaper::apply_wallpaper(path.clone(), self.apps_matugen());
            self.dock.config.settings.last_wallpaper = path.to_string_lossy().to_string();
            if self.dock.config.settings.accent_from_wallpaper {
                self.sync_accent_from_last_wallpaper();
                let dir = repo_dir();
                let _ = std::process::Command::new(dir.join("sync-dolphin-theme.sh")).spawn();
                let _ = std::process::Command::new(dir.join("sync-kitty-theme.sh")).spawn();
                let _ = std::process::Command::new(dir.join("sync-p10k-theme.sh")).spawn();
            } else {
                let _ = self.dock.config.save();
            }
        }
        if let Some(wp) = self.wallpaper_mode.as_mut() {
            wp.hovered = Some(menu::WallpaperHit::Thumbnail(index));
        }
        self.request_redraw(qh);
    }

    pub(super) fn sync_accent_from_last_wallpaper(&mut self) {
        let mut path = self.dock.config.settings.last_wallpaper.clone();
        if path.is_empty() {
            path = wallpaper::current_wallpaper_path()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            self.dock.config.settings.last_wallpaper = path.clone();
        }
        if path.is_empty() {
            return;
        }
        let (matugen_scheme, matugen_mode) = (
            self.dock.config.settings.matugen_scheme.clone(),
            self.dock.config.settings.matugen_mode(),
        );
        if let Some(scheme) = wallpaper::extract_color_scheme(
            std::path::Path::new(&path),
            &matugen_scheme,
            matugen_mode,
        ) {
            let s = &mut self.dock.config.settings;
            (s.accent_r, s.accent_g, s.accent_b) = scheme.primary;
            (s.accent2_r, s.accent2_g, s.accent2_b) = scheme.secondary;
            (s.on_accent_r, s.on_accent_g, s.on_accent_b) = scheme.on_primary;
            (s.panel_r, s.panel_g, s.panel_b) = scheme.surface;
            (s.text_r, s.text_g, s.text_b) = scheme.on_surface;
            (s.text_dim_r, s.text_dim_g, s.text_dim_b) = scheme.outline;
        }
        let _ = self.dock.config.save();
    }

    /// `(esquema, modo)` a aplicar a las apps, o `None` si el ajuste "Matugen
    /// Apps" (Colors) está apagado.
    pub(super) fn apps_matugen(&self) -> Option<(String, String)> {
        let s = &self.dock.config.settings;
        s.matugen_apps
            .then(|| (s.matugen_scheme.clone(), s.matugen_mode().to_string()))
    }

    /// Aplica el matugen del usuario a las apps **ahora**, para los cambios que no
    /// pasan por elegir fondo (Matugen Style, Color Theme). Fire-and-forget: matugen
    /// tarda 1-2 s y no puede frenar el loop.
    pub(super) fn apply_matugen_to_apps(&self) {
        let Some((scheme, mode)) = self.apps_matugen() else {
            return;
        };
        let path = std::path::PathBuf::from(&self.dock.config.settings.last_wallpaper);
        if !path.is_file() {
            return;
        }
        log::debug!("matugen apps: {scheme}/{mode} sobre {}", path.display());
        std::thread::spawn(move || {
            wallpaper::run_matugen(&path, &scheme, &mode);
        });
    }

    pub(super) fn close_wallpaper_mode(&mut self, qh: &QueueHandle<Self>) {
        // ----- cierre inmediato, igual que el panel de ajustes: con la animación
        // dependía del tick de frames, y si ese tick no corría Escape no cerraba
        // nunca. -----
        if self.wallpaper_mode.is_none() {
            return;
        }
        log::debug!("wallpaper: close");
        self.wallpaper_mode = None;
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::None);
        self.restore_dock_size();
        self.draw(qh);
        trim_heap();
    }

    pub(super) fn scroll_wallpaper_into_view(&mut self, index: usize, qh: &QueueHandle<Self>) {
        let Some(wp) = self.wallpaper_mode.as_mut() else {
            return;
        };
        let (tw, th) = menu::wallpaper_thumb_size(wp.frame, wp.is_vertical);
        let along = if wp.is_vertical { th } else { tw };
        let cx = index as f32 * (along + menu::WALLPAPER_GAP);
        let viewport_along = (menu::wallpaper_along(wp.frame, wp.is_vertical)
            - menu::WALLPAPER_BACK_ZONE_W)
            .max(1.0);
        let mut target = wp.scroll_target;
        if cx < target {
            target = cx;
        } else if cx + along > target + viewport_along {
            target = cx + along - viewport_along;
        }
        let max_scroll = menu::wallpaper_max_scroll(wp.wallpapers.len(), wp.frame, wp.is_vertical);
        wp.scroll_target = target.clamp(0.0, max_scroll);
        self.request_redraw(qh);
    }
    pub(super) fn handle_wallpaper_key(&mut self, keysym: Keysym, qh: &QueueHandle<Self>) {
        let Some(wp) = self.wallpaper_mode.as_ref() else {
            return;
        };
        if keysym == Keysym::Escape {
            log::debug!("wallpaper: Escape");
            self.close_wallpaper_mode(qh);
            return;
        }
        if wp.wallpapers.is_empty() {
            return;
        }
        let current = match wp.hovered {
            Some(menu::WallpaperHit::Thumbnail(i)) => i,
            _ => 0,
        };
        match keysym {
            Keysym::Left | Keysym::Up => {
                let next = current.saturating_sub(1);
                if let Some(wp) = self.wallpaper_mode.as_mut() {
                    wp.hovered = Some(menu::WallpaperHit::Thumbnail(next));
                }
                self.scroll_wallpaper_into_view(next, qh);
            }
            Keysym::Right | Keysym::Down => {
                let next = (current + 1).min(wp.wallpapers.len() - 1);
                if let Some(wp) = self.wallpaper_mode.as_mut() {
                    wp.hovered = Some(menu::WallpaperHit::Thumbnail(next));
                }
                self.scroll_wallpaper_into_view(next, qh);
            }
            Keysym::Return => {
                self.choose_wallpaper(current, qh);
            }
            _ => {}
        }
    }
    pub(super) fn draw_wallpaper_mode(&mut self, qh: &QueueHandle<Self>) {
        let scale = self.output_scale.max(1) as f32;
        let mut received = 0usize;
        if let Some(wp) = self.wallpaper_mode.as_ref() {
            while let Ok((path, w, h, pixmap)) = wp.thumb_result_rx.try_recv() {
                self.thumbnail_cache.insert(&path, w, h, pixmap);
                received += 1;
            }
        }
        if let Some(wp) = self.wallpaper_mode.as_mut() {
            wp.thumbs_pending = wp.thumbs_pending.saturating_sub(received);
        }
        self.request_visible_thumbnails();
        let transparency = self.dock.config.settings.transparency;
        // ----- el reparto (dock arriba, panel abajo) se calcula ANTES de tomar
        // prestado el modo: `overlay_layout` necesita `&self` entero -----
        let Some(layout) = self.overlay_layout() else {
            return;
        };
        let Some(wp) = self.wallpaper_mode.as_mut() else {
            return;
        };
        let linear = wp.anim.clamp(0.0, 1.0);
        let eased = 0.5 - 0.5 * (std::f32::consts::PI * linear).cos();

        // ----- el tamaño del panel sale del frame: un solo número -----
        let (panel_w, panel_h) = menu::panel_size(wp.frame, wp.is_vertical);
        let width = (panel_w * scale).round() as i32;
        let height = (panel_h * scale).round() as i32;
        if width <= 0 || height <= 0 {
            return;
        }

        let args = menu_render::DrawArgs {
            screen: menu::MenuScreen::WallpaperPicker,
            controls: &[],
            content_height: panel_h,
            panel_width: panel_w,
            dock: &self.dock,
            app_entries: &[],
            icon_choices: &[],
            search_query: "",
            wallpapers: &wp.wallpapers,
            wallpaper_hovered: wp.hovered,
            wallpaper_scroll_x: wp.scroll_x,
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
            overlay_tabs: Some(OverlayMode::Wallpaper.tab_index()),
            volume_rows: &[],
            volume_devices: &[],
            slide_offset: menu::overlay_slide_offset(wp.slide_dir, wp.anim),
            body_opacity: anim_opacity(transparency, eased),
        };
        let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32).unwrap();
        menu_render::draw_content(
            &mut pixmap,
            &mut self.icon_cache,
            &mut self.text_cache,
            &self.thumbnail_cache,
            &args,
        );
        self.show_panel_surface(qh, &pixmap, layout, 1.0);
    }
    pub(super) fn tick_wallpaper_frame(&mut self, qh: &QueueHandle<Self>) {
        // ----- transiciones apagadas: el filmstrip salta, no se desliza -----
        let smooth = self.dock.config.settings.smooth_transitions;
        let Some(wp) = self.wallpaper_mode.as_mut() else {
            return;
        };
        let fade_animating = if wp.anim < wp.target_anim {
            wp.anim = (wp.anim + menu::ANIM_STEP_OPEN).min(wp.target_anim);
            true
        } else if wp.anim > wp.target_anim {
            wp.anim = (wp.anim - menu::ANIM_STEP_CLOSE).max(wp.target_anim);
            true
        } else {
            false
        };
        let scroll_delta = wp.scroll_target - wp.scroll_x;
        let scroll_animating = if smooth && scroll_delta.abs() > 0.5 {
            wp.scroll_x += scroll_delta * 0.28;
            true
        } else {
            wp.scroll_x = wp.scroll_target;
            false
        };
        let closing = wp.closing;
        let anim = wp.anim;
        let thumbs_pending = wp.thumbs_pending;

        if closing && anim <= 0.0 {
            self.wallpaper_mode = None;
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
                self.handle_wallpaper_key(keysym, qh);
            }
        }

        if !wallpaper_needs_frames(
            fade_animating,
            scroll_animating,
            key_repeating,
            thumbs_pending,
        ) {
            return;
        }
        self.draw_wallpaper_mode(qh);
    }
    pub(super) fn handle_wallpaper_pointer_event(
        &mut self,
        event: &PointerEvent,
        qh: &QueueHandle<Self>,
    ) {
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                let (x, y) = self.panel_local(event.position.0, event.position.1);
                if let Some(wp) = self.wallpaper_mode.as_mut() {
                    wp.hovered = menu::wallpaper_hit_test(
                        wp.wallpapers.len(),
                        wp.frame,
                        wp.is_vertical,
                        wp.scroll_x,
                        x as f32,
                        y as f32,
                    );
                }
                self.request_redraw(qh);
            }
            PointerEventKind::Leave { .. } => {
                if let Some(wp) = self.wallpaper_mode.as_mut() {
                    wp.hovered = None;
                }
                self.request_redraw(qh);
            }
            PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                let (x, y) = self.panel_local(event.position.0, event.position.1);
                let hit = self.wallpaper_mode.as_ref().and_then(|wp| {
                    menu::wallpaper_hit_test(
                        wp.wallpapers.len(),
                        wp.frame,
                        wp.is_vertical,
                        wp.scroll_x,
                        x as f32,
                        y as f32,
                    )
                });
                match hit {
                    Some(menu::WallpaperHit::Back) => self.close_wallpaper_mode(qh),
                    Some(menu::WallpaperHit::Thumbnail(i)) => self.choose_wallpaper(i, qh),
                    None => {}
                }
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
                if let Some(wp) = self.wallpaper_mode.as_mut() {
                    let max_scroll =
                        menu::wallpaper_max_scroll(wp.wallpapers.len(), wp.frame, wp.is_vertical);
                    wp.scroll_target = (wp.scroll_target + delta as f32).clamp(0.0, max_scroll);
                    wp.scroll_x = wp.scroll_target;
                }
                self.request_redraw(qh);
            }
            _ => {}
        }
    }
}

/// ----- ¿hay que seguir dando frames? -----
/// Sí mientras haya algo animándose (la apertura, el scroll, una tecla
/// repetida) o **miniaturas pedidas sin respuesta**. Ese último término es el
/// que faltaba: las miniaturas llegan por un canal desde el hilo que decodifica,
/// y un canal no genera ningún evento de Wayland, así que si el loop se apagaba
/// antes de que llegaran, el selector quedaba en negro hasta que cualquier otra
/// cosa (mover el scroll, un click) forzara un redraw. El loop se apaga solo
/// cuando `thumbs_pending` llega a 0.
fn wallpaper_needs_frames(
    fade_animating: bool,
    scroll_animating: bool,
    key_repeating: bool,
    thumbs_pending: usize,
) -> bool {
    fade_animating || scroll_animating || key_repeating || thumbs_pending > 0
}

#[cfg(test)]
mod frame_tests {
    use super::wallpaper_needs_frames;

    /// El término de las miniaturas es el que hacía que el selector se abriera
    /// en negro: sin él, con todo quieto el loop se apagaba y las miniaturas que
    /// ya estaban en el canal no se dibujaban nunca.
    #[test]
    fn con_miniaturas_en_vuelo_sigue_dando_frames() {
        assert!(wallpaper_needs_frames(false, false, false, 1));
        assert!(wallpaper_needs_frames(false, false, false, 7));
        // ----- y se apaga cuando no queda nada: si no, sería un loop eterno -----
        assert!(!wallpaper_needs_frames(false, false, false, 0));
        // ----- lo de siempre sigue mandando -----
        assert!(wallpaper_needs_frames(true, false, false, 0));
        assert!(wallpaper_needs_frames(false, true, false, 0));
        assert!(wallpaper_needs_frames(false, false, true, 0));
    }
}
