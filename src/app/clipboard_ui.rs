use super::*;

use crate::menu_render::{CLIP_ROW_H, CLIP_VISIBLE_ROWS, ClipArgs, clip_content_h, clip_content_y};

impl App {
    pub(crate) fn toggle_clipboard(&mut self, qh: &QueueHandle<Self>) {
        if self.clipboard_mode.is_some() {
            self.close_clipboard_mode(qh);
        } else {
            self.open_clipboard(qh);
        }
    }

    pub(super) fn open_clipboard(&mut self, qh: &QueueHandle<Self>) {
        self.wallpaper_mode = None;
        self.dock_menu_mode = None;
        self.app_search_mode = None;
        self.osd_mode = None;
        self.notification_mode = None;
        self.popup_mode = None;
        self.menu = None;
        self.held_key = None;
        self.clipboard_history.ensure_loaded();

        self.layer.set_layer(Layer::Top);
        // ----- el frame es la caja del CONTENIDO: en el vertical es la columna de
        // siempre (dos filas de la banda de pestañas quedan afuera, al costado del
        // dock) y el alto sale de las filas visibles -----
        let is_vertical = self.dock.is_vertical();
        // ----- el portapapeles usa el cross ANCHO en vertical (el del launcher): sus
        // títulos son oraciones largas y en 170 se elidían a ~16 caracteres -----
        let content_w = if is_vertical {
            menu::OVERLAY_PANEL_VERTICAL_WIDE.max(self.dock.thickness() as f32)
        } else {
            menu::OVERLAY_PANEL_W
        };
        let frame = menu::frame_for(
            content_w,
            clip_content_h(),
            is_vertical,
            self.dock.band_left(),
        );
        let (panel_w, panel_h) = menu::panel_size(frame, is_vertical);
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        // ----- el panel va DEBAJO del dock, como el panel de ajustes -----
        self.apply_panel_size(panel_w, panel_h);

        self.clipboard_mode = Some(ClipboardMode {
            query: String::new(),
            filtered: Vec::new(),
            selected: 0,
            scroll_y: 0.0,
            scroll_target: 0.0,
            hovered: None,
            anim: self.initial_panel_anim(),
            target_anim: 1.0,
            closing: false,
            frame,
            is_vertical,
            slide_dir: 0.0,
            previews: std::collections::HashMap::new(),
        });
        self.refresh_clipboard_filter(qh);
    }

    pub(super) fn close_clipboard_mode(&mut self, qh: &QueueHandle<Self>) {
        // ----- cierre inmediato, como el panel de ajustes y el selector de fondos:
        // con la animación el cambio de modo (Shift+←/→) se cruzaba con el que
        // entra, y el cierre dependía del tick de frames. -----
        if self.clipboard_mode.is_none() {
            return;
        }
        self.clipboard_mode = None;
        self.held_key = None;
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::None);
        self.restore_dock_size();
        self.draw(qh);
        trim_heap();
    }

    pub(crate) fn refresh_clipboard_filter(&mut self, qh: &QueueHandle<Self>) {
        let entries = self.clipboard_history.entries();
        let Some(cm) = self.clipboard_mode.as_mut() else {
            return;
        };
        let query = cm.query.to_lowercase();
        cm.filtered = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| query.is_empty() || e.matches(&query))
            .map(|(i, _)| i)
            .collect();
        cm.selected = cm.selected.min(cm.filtered.len().saturating_sub(1));
        cm.previews.retain(|k, _| cm.filtered.contains(k));
        self.scroll_clipboard_into_view(qh);
        self.request_redraw(qh);
    }

    fn scroll_clipboard_into_view(&mut self, qh: &QueueHandle<Self>) {
        let Some(cm) = self.clipboard_mode.as_mut() else {
            return;
        };
        let row_top = cm.selected as f32 * CLIP_ROW_H;
        let viewport = CLIP_ROW_H * CLIP_VISIBLE_ROWS as f32;
        let mut target = cm.scroll_target;
        if row_top < target {
            target = row_top;
        } else if row_top + CLIP_ROW_H > target + viewport {
            target = row_top + CLIP_ROW_H - viewport;
        }
        let max_scroll = (cm.filtered.len() as f32 * CLIP_ROW_H - viewport).max(0.0);
        cm.scroll_target = target.clamp(0.0, max_scroll);
        self.request_redraw(qh);
    }

    pub(super) fn handle_clipboard_key(&mut self, event: KeyEvent, qh: &QueueHandle<Self>) {
        match event.keysym {
            Keysym::Escape => {
                self.close_clipboard_mode(qh);
                return;
            }
            Keysym::Return | Keysym::KP_Enter => {
                self.paste_clipboard_selected(qh);
                return;
            }
            Keysym::Down => {
                if let Some(cm) = self.clipboard_mode.as_mut()
                    && !cm.filtered.is_empty()
                {
                    cm.selected = (cm.selected + 1).min(cm.filtered.len() - 1);
                    cm.hovered = None;
                }
                self.scroll_clipboard_into_view(qh);
                return;
            }
            Keysym::Up => {
                if let Some(cm) = self.clipboard_mode.as_mut() {
                    cm.selected = cm.selected.saturating_sub(1);
                    cm.hovered = None;
                }
                self.scroll_clipboard_into_view(qh);
                return;
            }
            Keysym::Delete => {
                self.delete_clipboard_selected(qh);
                return;
            }
            Keysym::BackSpace => {
                if let Some(cm) = self.clipboard_mode.as_mut() {
                    cm.query.pop();
                }
                self.refresh_clipboard_filter(qh);
                return;
            }
            _ => {}
        }
        if let Some(text) = &event.utf8 {
            let mut changed = false;
            if let Some(cm) = self.clipboard_mode.as_mut() {
                for ch in text.chars() {
                    if !ch.is_control() {
                        cm.query.push(ch);
                        changed = true;
                    }
                }
            }
            if changed {
                self.refresh_clipboard_filter(qh);
            }
        }
    }

    fn paste_clipboard_selected(&mut self, qh: &QueueHandle<Self>) {
        let entry = self
            .clipboard_mode
            .as_ref()
            .and_then(|cm| cm.filtered.get(cm.selected).copied())
            .and_then(|i| self.clipboard_history.entries().get(i).cloned());
        let Some(entry) = entry else {
            return;
        };
        self.set_clipboard_entry(&entry, qh);
        self.close_clipboard_mode(qh);
        crate::clipboard::paste_active();
    }

    fn delete_clipboard_selected(&mut self, qh: &QueueHandle<Self>) {
        let index = self
            .clipboard_mode
            .as_ref()
            .and_then(|cm| cm.filtered.get(cm.selected).copied());
        if let Some(index) = index
            && self.clipboard_history.remove(index)
        {
            self.clipboard_history.save();
        }
        self.refresh_clipboard_filter(qh);
    }

    pub(super) fn handle_clipboard_pointer_event(
        &mut self,
        event: &PointerEvent,
        qh: &QueueHandle<Self>,
    ) {
        // ----- coordenadas de la superficie -> del panel (el panel va debajo del
        // dock cuando el dock está arriba) -----
        let (px, py) = self.panel_local(event.position.0, event.position.1);
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                let hit = self.clipboard_row_at(px as f32, py as f32);
                if let Some(cm) = self.clipboard_mode.as_mut()
                    && cm.hovered != hit
                {
                    cm.hovered = hit;
                    self.request_redraw(qh);
                }
            }
            PointerEventKind::Leave { .. } => {
                if let Some(cm) = self.clipboard_mode.as_mut() {
                    cm.hovered = None;
                }
                self.request_redraw(qh);
            }
            PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                if let Some(pos) = self.clipboard_row_at(px as f32, py as f32) {
                    if let Some(cm) = self.clipboard_mode.as_mut() {
                        cm.selected = pos;
                    }
                    self.paste_clipboard_selected(qh);
                }
            }
            PointerEventKind::Axis {
                horizontal,
                vertical,
                ..
            } => {
                let delta = if vertical.absolute != 0.0 {
                    vertical.absolute
                } else {
                    horizontal.absolute
                };
                if let Some(cm) = self.clipboard_mode.as_mut() {
                    let viewport = CLIP_ROW_H * CLIP_VISIBLE_ROWS as f32;
                    let max_scroll = (cm.filtered.len() as f32 * CLIP_ROW_H - viewport).max(0.0);
                    cm.scroll_target =
                        (cm.scroll_target + delta as f32 * 0.6).clamp(0.0, max_scroll);
                }
                self.request_redraw(qh);
            }
            _ => {}
        }
    }

    fn clipboard_row_at(&self, _x: f32, y: f32) -> Option<usize> {
        let cm = self.clipboard_mode.as_ref()?;
        let top = clip_content_y(cm.frame);
        if y < top {
            return None;
        }
        let pos = ((y - top + cm.scroll_y) / CLIP_ROW_H).floor() as usize;
        (pos < cm.filtered.len()).then_some(pos)
    }

    pub(super) fn tick_clipboard_frame(&mut self, qh: &QueueHandle<Self>) {
        // ----- transiciones apagadas: el scroll salta, no se anima -----
        let smooth = self.dock.config.settings.smooth_transitions;
        let Some(cm) = self.clipboard_mode.as_mut() else {
            return;
        };
        let fade_animating = if cm.anim < cm.target_anim {
            cm.anim = (cm.anim + menu::ANIM_STEP_OPEN).min(cm.target_anim);
            true
        } else if cm.anim > cm.target_anim {
            cm.anim = (cm.anim - menu::ANIM_STEP_CLOSE).max(cm.target_anim);
            true
        } else {
            false
        };
        let scroll_delta = cm.scroll_target - cm.scroll_y;
        let scroll_animating = if smooth && scroll_delta.abs() > 0.5 {
            cm.scroll_y += scroll_delta * 0.3;
            true
        } else {
            cm.scroll_y = cm.scroll_target;
            false
        };
        let closing = cm.closing;
        let anim = cm.anim;

        if closing && anim <= 0.0 {
            self.clipboard_mode = None;
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
                self.handle_clipboard_key(
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

        if !fade_animating && !scroll_animating && !key_repeating {
            return;
        }
        self.draw_clipboard_mode(qh);
    }

    pub(super) fn draw_clipboard_mode(&mut self, qh: &QueueHandle<Self>) {
        let scale = self.output_scale.max(1) as f32;
        let transparency = self.dock.config.settings.transparency;
        let tabs = self.current_overlay().map(OverlayMode::tab_index);
        let is_vertical = self.dock.is_vertical();
        // ----- el reparto (dock arriba, panel abajo) se calcula ANTES de tomar
        // prestado el modo: `overlay_layout` necesita `&self` entero -----
        let Some(layout) = self.overlay_layout() else {
            return;
        };
        let Some(cm) = self.clipboard_mode.as_mut() else {
            return;
        };
        let linear = cm.anim.clamp(0.0, 1.0);
        let eased = 0.5 - 0.5 * (std::f32::consts::PI * linear).cos();
        // ----- el tamaño del panel sale del frame: un solo número -----
        let (panel_w, panel_h) = menu::panel_size(cm.frame, cm.is_vertical);
        let width = (panel_w * scale).round() as i32;
        let height = (panel_h * scale).round() as i32;
        if width <= 0 || height <= 0 {
            return;
        }

        let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32).unwrap();
        let args = ClipArgs {
            settings: &self.dock.config.settings,
            entries: self.clipboard_history.entries(),
            filtered: &cm.filtered,
            previews: &mut cm.previews,
            query: &cm.query,
            selected: cm.selected,
            hovered: cm.hovered,
            scroll_y: cm.scroll_y,
            render_scale: scale,
            frame: cm.frame,
            overlay_tabs: tabs,
            is_vertical,
            // ----- el fondo y la banda los deja fijos el render; el cuerpo se
            // corre (cambio de pestaña) y/o se funde (transparency < 1) -----
            slide_offset: menu::overlay_slide_offset(cm.slide_dir, cm.anim),
            body_opacity: anim_opacity(transparency, eased),
        };
        crate::menu_render::draw_clipboard(&mut pixmap, &mut self.text_cache, args);
        // ----- el panel ya trae su propio fundido; la superficie va opaca con el
        // dock nítido arriba -----
        self.show_panel_surface(qh, &pixmap, layout, 1.0);
    }
}
