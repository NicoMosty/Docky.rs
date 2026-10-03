//! El panel de ATAJOS de niri: una pestaña del overlay que lista los binds con las teclas a
//! la izquierda y qué hacen a la derecha, y los filtra por cualquiera de las dos.
//!
//! Niri no expone los binds por IPC, así que la lista sale de leer los `.kdl` del usuario
//! (`desktop::list_niri_keybinds`). Se relee cada vez que se abre: no hay watcher.

use super::*;

use crate::menu_render::{
    KEYBIND_ROW_H, KeybindArgs, keybind_content_h, keybind_content_y, keybind_visible_rows,
};

impl App {
    pub(crate) fn toggle_keybinds(&mut self, qh: &QueueHandle<Self>) {
        if self.keybinds_mode.is_some() {
            self.close_keybinds_mode(qh);
        } else {
            self.open_keybinds(qh);
        }
    }

    pub(super) fn open_keybinds(&mut self, qh: &QueueHandle<Self>) {
        self.wallpaper_mode = None;
        self.clipboard_mode = None;
        self.dock_menu_mode = None;
        self.app_search_mode = None;
        self.notifications_mode = None;
        self.osd_mode = None;
        self.popup_mode = None;
        self.menu = None;
        self.held_key = None;

        self.layer.set_layer(Layer::Top);
        let is_vertical = self.dock.is_vertical();
        // ----- mismo cross que el portapapeles: sus etiquetas también son largas -----
        let content_w = if is_vertical {
            menu::OVERLAY_PANEL_VERTICAL_WIDE.max(self.dock.thickness() as f32)
        } else {
            menu::OVERLAY_PANEL_W
        };
        let frame = menu::frame_for(
            content_w,
            if is_vertical {
                menu::OVERLAY_PANEL_H
            } else {
                keybind_content_h()
            },
            is_vertical,
            self.dock.band_left(),
        );
        let (panel_w, panel_h) = menu::panel_size(frame, is_vertical);
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        self.apply_panel_size(panel_w, panel_h);

        self.keybinds_mode = Some(KeybindsMode {
            query: String::new(),
            all: crate::desktop::list_niri_keybinds(),
            filtered: Vec::new(),
            selected: 0,
            scroll_y: 0.0,
            scroll_target: 0.0,
            hovered: None,
            anim: self.initial_panel_anim(),
            target_anim: 1.0,
            frame,
            is_vertical,
            slide_dir: 0.0,
        });
        self.refresh_keybinds_filter(qh);
    }

    pub(super) fn close_keybinds_mode(&mut self, qh: &QueueHandle<Self>) {
        if self.keybinds_mode.is_none() {
            return;
        }
        self.keybinds_mode = None;
        self.held_key = None;
        self.layer
            .set_keyboard_interactivity(KeyboardInteractivity::None);
        self.restore_dock_size();
        self.draw(qh);
        trim_heap();
    }

    /// Filtra por el ranking del launcher (`desktop::tier_of`): prefijo > inicio de palabra >
    /// contiene > difuso, contra las TECLAS y contra el texto de búsqueda (comando + título).
    pub(super) fn refresh_keybinds_filter(&mut self, qh: &QueueHandle<Self>) {
        let Some(m) = self.keybinds_mode.as_mut() else {
            return;
        };
        let query = m.query.to_lowercase();
        let mut scored: Vec<(usize, u8)> = m
            .all
            .iter()
            .enumerate()
            .filter_map(|(i, kb)| {
                let keywords = [kb.search.clone()];
                let tier = crate::desktop::tier_of(&kb.keys.to_lowercase(), &keywords, &query)?;
                Some((i, tier))
            })
            .collect();
        scored.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| m.all[a.0].keys.cmp(&m.all[b.0].keys))
        });
        m.filtered = scored.into_iter().map(|(i, _)| i).collect();
        m.selected = m.selected.min(m.filtered.len().saturating_sub(1));
        m.scroll_y = 0.0;
        m.scroll_target = 0.0;
        self.request_redraw(qh);
    }

    fn scroll_keybinds_into_view(&mut self, qh: &QueueHandle<Self>) {
        let Some(m) = self.keybinds_mode.as_mut() else {
            return;
        };
        let row_top = m.selected as f32 * KEYBIND_ROW_H;
        let viewport = KEYBIND_ROW_H * keybind_visible_rows(m.frame) as f32;
        let mut target = m.scroll_target;
        if row_top < target {
            target = row_top;
        } else if row_top + KEYBIND_ROW_H > target + viewport {
            target = row_top + KEYBIND_ROW_H - viewport;
        }
        let max_scroll = (m.filtered.len() as f32 * KEYBIND_ROW_H - viewport).max(0.0);
        m.scroll_target = target.clamp(0.0, max_scroll);
        self.request_redraw(qh);
    }

    pub(super) fn handle_keybinds_key(&mut self, event: KeyEvent, qh: &QueueHandle<Self>) {
        match event.keysym {
            Keysym::Escape => {
                self.close_keybinds_mode(qh);
                return;
            }
            // ----- Enter copia las TECLAS (es lo que se viene a buscar): así se pegan
            // donde haga falta -----
            Keysym::Return | Keysym::KP_Enter => {
                self.copy_keybind_selected(qh);
                return;
            }
            Keysym::Down | Keysym::Up => {
                // ----- el salto se loguea con la MISMA etiqueta `teclado:` que los
                // otros menús: es el sensor de `scripts/pointer.py --key`, que si no
                // no ve que la flecha llegó -----
                let mut salto = None;
                if let Some(m) = self.keybinds_mode.as_mut()
                    && !m.filtered.is_empty()
                {
                    let arriba = event.keysym == Keysym::Up;
                    let antes = m.selected;
                    m.selected = if arriba {
                        m.selected.saturating_sub(1)
                    } else {
                        (m.selected + 1).min(m.filtered.len() - 1)
                    };
                    m.hovered = None;
                    if m.selected != antes {
                        salto = Some((antes, m.selected));
                    }
                }
                if let Some((antes, ahora)) = salto {
                    log::debug!("teclado: keybinds {antes} -> {ahora}");
                }
                self.scroll_keybinds_into_view(qh);
                return;
            }
            Keysym::BackSpace => {
                if let Some(m) = self.keybinds_mode.as_mut() {
                    m.query.pop();
                }
                self.refresh_keybinds_filter(qh);
                return;
            }
            _ => {}
        }
        if let Some(text) = &event.utf8 {
            let mut changed = false;
            if let Some(m) = self.keybinds_mode.as_mut() {
                for ch in text.chars() {
                    if !ch.is_control() {
                        m.query.push(ch);
                        changed = true;
                    }
                }
            }
            if changed {
                self.refresh_keybinds_filter(qh);
            }
        }
    }

    /// Copia las TECLAS de la fila seleccionada al portapapeles y cierra el panel. Mismo
    /// camino que copiar una entrada del historial (`set_clipboard_entry`).
    fn copy_keybind_selected(&mut self, qh: &QueueHandle<Self>) {
        // ----- las teclas se copian ANTES de tocar el modo: el préstamo de
        // `keybinds_mode` no puede vivir mientras `set_clipboard_entry` pide `&mut self` -----
        let keys = self
            .keybinds_mode
            .as_ref()
            .and_then(|m| m.filtered.get(m.selected).copied())
            .and_then(|i| self.keybinds_mode.as_ref().and_then(|m| m.all.get(i)))
            .map(|kb| kb.keys.clone());
        let Some(keys) = keys else {
            return;
        };
        log::debug!("keybinds: copio {keys}");
        let Some(entry) =
            crate::clipboard::ClipboardEntry::from_data("text/plain".into(), keys.into_bytes())
        else {
            return;
        };
        self.set_clipboard_entry(&entry, qh);
        self.clipboard_history
            .add(entry, self.dock.config.settings.clipboard_items);
        self.clipboard_history.save();
        self.close_keybinds_mode(qh);
    }

    pub(super) fn handle_keybinds_pointer_event(
        &mut self,
        event: &PointerEvent,
        qh: &QueueHandle<Self>,
    ) {
        let (px, py) = self.panel_local(event.position.0, event.position.1);
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                let hit = self.keybind_row_at(px as f32, py as f32);
                if let Some(m) = self.keybinds_mode.as_mut()
                    && m.hovered != hit
                {
                    m.hovered = hit;
                    self.request_redraw(qh);
                }
            }
            PointerEventKind::Leave { .. } => {
                if let Some(m) = self.keybinds_mode.as_mut() {
                    m.hovered = None;
                }
                self.request_redraw(qh);
            }
            PointerEventKind::Press { button, .. } if button == BTN_LEFT => {
                if let Some(pos) = self.keybind_row_at(px as f32, py as f32) {
                    if let Some(m) = self.keybinds_mode.as_mut() {
                        m.selected = pos;
                    }
                    self.copy_keybind_selected(qh);
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
                if let Some(m) = self.keybinds_mode.as_mut() {
                    let viewport = KEYBIND_ROW_H * keybind_visible_rows(m.frame) as f32;
                    let max_scroll = (m.filtered.len() as f32 * KEYBIND_ROW_H - viewport).max(0.0);
                    m.scroll_target = (m.scroll_target + delta as f32 * 0.6).clamp(0.0, max_scroll);
                }
                self.request_redraw(qh);
            }
            _ => {}
        }
    }

    fn keybind_row_at(&self, _x: f32, y: f32) -> Option<usize> {
        let m = self.keybinds_mode.as_ref()?;
        let top = keybind_content_y(m.frame);
        if y < top {
            return None;
        }
        let pos = ((y - top + m.scroll_y) / KEYBIND_ROW_H).floor() as usize;
        (pos < m.filtered.len()).then_some(pos)
    }

    pub(super) fn tick_keybinds_frame(&mut self, qh: &QueueHandle<Self>) {
        let smooth = self.dock.config.settings.smooth_transitions;
        let dt = self.frame_dt_ms;
        let Some(m) = self.keybinds_mode.as_mut() else {
            return;
        };
        let fade_animating = menu::anim_towards(
            &mut m.anim,
            m.target_anim,
            dt,
            menu::Pace::Open,
            menu::Pace::Close,
        );
        let scroll_delta = m.scroll_target - m.scroll_y;
        let scroll_animating = if smooth && scroll_delta.abs() > 0.5 {
            m.scroll_y += scroll_delta * menu::lerp_factor(0.3, dt);
            true
        } else {
            m.scroll_y = m.scroll_target;
            false
        };
        let key_repeating = self.held_key.is_some();
        if let Some((keysym, steps)) = self.poll_held_key() {
            for _ in 0..steps {
                self.handle_keybinds_key(
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
        self.draw_keybinds_mode(qh);
    }

    pub(super) fn draw_keybinds_mode(&mut self, qh: &QueueHandle<Self>) {
        let scale = self.output_scale.max(1) as f32;
        let transparency = self.dock.config.settings.transparency;
        let tabs = self.current_overlay().map(OverlayMode::tab_index);
        let is_vertical = self.dock.is_vertical();
        let Some(layout) = self.overlay_layout() else {
            return;
        };
        let Some(m) = self.keybinds_mode.as_mut() else {
            return;
        };
        let linear = m.anim.clamp(0.0, 1.0);
        let eased = 0.5 - 0.5 * (std::f32::consts::PI * linear).cos();
        let (panel_w, panel_h) = menu::panel_size(m.frame, m.is_vertical);
        let width = (panel_w * scale).round() as i32;
        let height = (panel_h * scale).round() as i32;
        if width <= 0 || height <= 0 {
            return;
        }

        let mut pixmap = tiny_skia::Pixmap::new(width as u32, height as u32).unwrap();
        let args = KeybindArgs {
            settings: &self.dock.config.settings,
            keybinds: &m.all,
            filtered: &m.filtered,
            query: &m.query,
            selected: m.selected,
            hovered: m.hovered,
            scroll_y: m.scroll_y,
            render_scale: scale,
            frame: m.frame,
            overlay_tabs: tabs,
            is_vertical,
            slide_offset: menu::overlay_slide_offset(m.slide_dir, m.anim),
            body_opacity: anim_opacity(transparency, eased),
        };
        crate::menu_render::draw_keybinds(&mut pixmap, &mut self.text_cache, args);
        self.show_panel_surface(qh, &pixmap, layout, 1.0);
    }
}
