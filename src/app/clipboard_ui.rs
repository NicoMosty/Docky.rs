use super::*;

use crate::menu_render::{CLIP_ROW_H, ClipArgs, clip_content_h, clip_content_y, clip_visible_rows};

/// Caja máxima (px) a la que se decodifica la imagen de la vista previa. Más grande que
/// cualquier panel; el dibujo la centra 1:1 (no se re-escala por frame).
const PREVIEW_MAX_W: u32 = 1400;
const PREVIEW_MAX_H: u32 = 800;

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
            // ----- en el vertical el alto es el COMÚN del overlay (640) y el panel
            // muestra las filas que entren: así el borde no se mueve al ciclar. En
            // el horizontal siguen siendo las siete filas de siempre. -----
            if is_vertical {
                menu::OVERLAY_PANEL_H
            } else {
                clip_content_h()
            },
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
            picked: Default::default(),
            anchor: None,
            preview: None,
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
        // ----- con la vista previa abierta la lista no se mueve: sólo repintar -----
        if cm.preview.is_some() {
            self.request_redraw(qh);
            return;
        }
        let row_top = cm.selected as f32 * CLIP_ROW_H;
        let viewport = CLIP_ROW_H * clip_visible_rows(cm.frame) as f32;
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
        // ----- la vista previa (Tab) intercepta: Tab y Esc vuelven a la lista, y las
        // flechas/PageUp/PageDown/Home/End scrollean el texto -----
        if self
            .clipboard_mode
            .as_ref()
            .is_some_and(|cm| cm.preview.is_some())
        {
            match event.keysym {
                Keysym::Tab | Keysym::Escape => {
                    self.close_clipboard_preview(qh);
                    return;
                }
                Keysym::Down | Keysym::Up | Keysym::Next | Keysym::Prior | Keysym::Home
                | Keysym::End => {
                    self.clipboard_scroll_preview_key(event.keysym, qh);
                    return;
                }
                _ => {}
            }
        }
        let shift = self.modifiers.shift;
        match event.keysym {
            Keysym::Escape => {
                self.close_clipboard_mode(qh);
                return;
            }
            // ----- Tab: vista previa grande de la fila seleccionada -----
            Keysym::Tab => {
                self.toggle_clipboard_preview(qh);
                return;
            }
            Keysym::Return | Keysym::KP_Enter => {
                self.paste_clipboard_selected(qh);
                return;
            }
            // ----- Shift+Space marca/desmarca la fila. El space PELADO sigue siendo un
            // carácter de la búsqueda: cae al `utf8` de abajo -----
            Keysym::space if shift => {
                self.clipboard_toggle_pick(qh);
                return;
            }
            // ----- el rango de Shift va con ↑/↓: la banda de pestañas les cede esas dos
            // mientras el portapapeles está abierto (ver `press_key`) y sigue ciclando con
            // ←/→, así que el gesto estándar de extender selección queda libre -----
            Keysym::Down | Keysym::Up if shift => {
                self.clipboard_move(event.keysym == Keysym::Up, true, qh);
                return;
            }
            Keysym::Down | Keysym::Up => {
                self.clipboard_move(event.keysym == Keysym::Up, false, qh);
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

    /// Mueve la fila seleccionada. Con `shift` extiende el rango marcado desde el ancla,
    /// como un gestor de archivos (el ancla queda fija hasta que se suelta el Shift).
    fn clipboard_move(&mut self, arriba: bool, shift: bool, qh: &QueueHandle<Self>) {
        if let Some(cm) = self.clipboard_mode.as_mut()
            && !cm.filtered.is_empty()
        {
            let n = cm.filtered.len() - 1;
            if shift {
                let ancla = cm.anchor.unwrap_or(cm.selected);
                cm.anchor = Some(ancla);
                cm.selected = if arriba {
                    cm.selected.saturating_sub(1)
                } else {
                    (cm.selected + 1).min(n)
                };
                let (a, b) = (ancla.min(cm.selected), ancla.max(cm.selected));
                cm.picked = (a..=b).collect();
            } else {
                cm.anchor = None;
                cm.selected = if arriba {
                    cm.selected.saturating_sub(1)
                } else {
                    (cm.selected + 1).min(n)
                };
            }
            cm.hovered = None;
        }
        self.scroll_clipboard_into_view(qh);
    }

    /// `Shift+Space`: marca o desmarca la fila seleccionada y deja el ancla ahí.
    fn clipboard_toggle_pick(&mut self, qh: &QueueHandle<Self>) {
        if let Some(cm) = self.clipboard_mode.as_mut()
            && !cm.filtered.is_empty()
        {
            if !cm.picked.remove(&cm.selected) {
                cm.picked.insert(cm.selected);
            }
            cm.anchor = Some(cm.selected);
        }
        self.request_redraw(qh);
    }

    /// `Tab`: abre o cierra la vista previa grande de la fila seleccionada.
    fn toggle_clipboard_preview(&mut self, qh: &QueueHandle<Self>) {
        if self
            .clipboard_mode
            .as_ref()
            .is_some_and(|cm| cm.preview.is_some())
        {
            self.close_clipboard_preview(qh);
            return;
        }
        let Some((pos, entry_index)) = self.clipboard_mode.as_ref().and_then(|cm| {
            cm.filtered
                .get(cm.selected)
                .copied()
                .map(|e| (cm.selected, e))
        }) else {
            return;
        };
        let Some(entry) = self.clipboard_history.entries().get(entry_index).cloned() else {
            return;
        };
        // ----- el texto se envuelve acá (una vez); la imagen se decodifica en un HILO y
        // vuelve por IPC, porque un 4K decodifica a ~33 MB antes de escalar y eso no puede
        // correr en el hilo que dibuja (AUDIT A4) -----
        let ancho = self.clipboard_preview_width();
        let mut preview = crate::menu_render::ClipPreview {
            pos,
            entry: entry_index,
            lines: Vec::new(),
            image: None,
            pending: false,
            scroll: 0.0,
            scroll_target: 0.0,
        };
        if entry.is_text() {
            preview.lines = texto_envuelto(&entry, ancho);
        } else {
            preview.pending = true;
            let tx = self.ipc_tx.clone();
            let conn = self.conn.clone();
            let qh_hilo = qh.clone();
            std::thread::spawn(move || {
                let image =
                    crate::clipboard::decode_preview(&entry, PREVIEW_MAX_W, PREVIEW_MAX_H);
                log::debug!(
                    "clip preview: decodificada la entrada {entry_index} -> {:?}",
                    image.as_ref().map(|i| (i.width, i.height))
                );
                let msg = crate::ipc::IpcMessage::ClipboardPreviewReady(
                    pos,
                    entry_index,
                    image.map(Box::new),
                );
                if tx.send(msg).is_ok() {
                    conn.display().sync(&qh_hilo, ());
                    let _ = conn.flush();
                }
            });
        }
        if let Some(cm) = self.clipboard_mode.as_mut() {
            cm.preview = Some(preview);
        }
        self.request_redraw(qh);
    }

    fn close_clipboard_preview(&mut self, qh: &QueueHandle<Self>) {
        if let Some(cm) = self.clipboard_mode.as_mut() {
            cm.preview = None;
        }
        self.request_redraw(qh);
    }

    /// Ancho LÓGICO que tiene el texto en la vista previa (el del frame menos el inset).
    /// Es el MISMO número que usa el dibujo: si se despegan, el texto se sale del panel.
    fn clipboard_preview_width(&self) -> f32 {
        self.clipboard_mode
            .as_ref()
            .map(|cm| cm.frame.w - crate::menu_render::CLIP_PAD * 2.0)
            .unwrap_or(400.0)
    }

    /// Scroll de la vista previa por TECLADO. Las flechas van de a una línea; PageUp/Down
    /// de a una pantalla; Home/End a los extremos.
    fn clipboard_scroll_preview_key(&mut self, keysym: Keysym, qh: &QueueHandle<Self>) {
        let salto = match keysym {
            Keysym::Up => -1.0,
            Keysym::Down => 1.0,
            Keysym::Prior => -8.0,
            Keysym::Next => 8.0,
            Keysym::Home => -1.0e6,
            _ => 1.0e6,
        };
        self.clipboard_scroll_preview(salto, qh);
    }

    /// Avanza el scroll de la vista previa (en líneas). El tope sale del largo ya envuelto.
    fn clipboard_scroll_preview(&mut self, lineas: f32, qh: &QueueHandle<Self>) {
        {
            let Some(cm) = self.clipboard_mode.as_mut() else {
                return;
            };
            let frame_h = cm.frame.h;
            let Some(p) = cm.preview.as_mut() else {
                return;
            };
            let line_h = crate::menu_render::CLIP_PREVIEW_LINE_H;
            let max = (p.lines.len() as f32 * line_h - frame_h * 0.5).max(0.0);
            p.scroll_target = if lineas.abs() > 1.0e5 {
                if lineas < 0.0 { 0.0 } else { max }
            } else {
                (p.scroll_target + lineas * line_h).clamp(0.0, max)
            };
        }
        self.request_redraw(qh);
    }

    /// El resultado del hilo que decodifica la imagen de la vista previa. Se DESCARTA si ya
    /// no aplica (el panel se cerró, o se está previsualizando otra entrada): el mismo guard
    /// que `tray_menu_still_wanted`.
    pub(crate) fn apply_clipboard_preview(
        &mut self,
        pos: usize,
        entry: usize,
        image: Option<Box<crate::clipboard::ScaledPreview>>,
        qh: &QueueHandle<Self>,
    ) {
        let mut aplica = false;
        if let Some(cm) = self.clipboard_mode.as_mut()
            && let Some(p) = cm.preview.as_mut()
            && p.pos == pos
            && p.entry == entry
        {
            p.pending = false;
            p.image = image.map(|b| *b);
            aplica = true;
        }
        if !aplica {
            log::debug!("clip preview: descarto el resultado {pos}/{entry}");
            return;
        }
        log::debug!("clip preview: aplico {pos}/{entry}");
        self.needs_repaint = true;
        self.request_redraw(qh);
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
        let indices = match self.clipboard_mode.as_ref() {
            Some(cm) => indices_a_borrar(&cm.filtered, &cm.picked, cm.selected),
            None => Vec::new(),
        };
        if indices.is_empty() {
            return;
        }
        // ----- vienen de MAYOR a menor: así los índices que quedan no se corren -----
        for index in indices {
            self.clipboard_history.remove(index);
        }
        self.clipboard_history.save();
        if let Some(cm) = self.clipboard_mode.as_mut() {
            cm.picked.clear();
            cm.anchor = None;
            // ----- la entrada previsualizada puede ser una de las borradas -----
            cm.preview = None;
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
        // ----- con la vista previa abierta: cualquier click la cierra y la rueda
        // scrollea el texto (la lista no se toca) -----
        if self
            .clipboard_mode
            .as_ref()
            .is_some_and(|cm| cm.preview.is_some())
        {
            match event.kind {
                PointerEventKind::Press { .. } => self.close_clipboard_preview(qh),
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
                    self.clipboard_scroll_preview(-(delta as f32) * 0.5, qh);
                }
                _ => {}
            }
            return;
        }
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
                    let viewport = CLIP_ROW_H * clip_visible_rows(cm.frame) as f32;
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
        let dt = self.frame_dt_ms;
        let Some(cm) = self.clipboard_mode.as_mut() else {
            return;
        };
        let fade_animating = menu::anim_towards(
            &mut cm.anim,
            cm.target_anim,
            dt,
            menu::Pace::Open,
            menu::Pace::Close,
        );
        let scroll_delta = cm.scroll_target - cm.scroll_y;
        let scroll_animating = if smooth && scroll_delta.abs() > 0.5 {
            cm.scroll_y += scroll_delta * menu::lerp_factor(0.3, dt);
            true
        } else {
            cm.scroll_y = cm.scroll_target;
            false
        };
        let closing = cm.closing;
        let anim = cm.anim;
        let preview_animating = {
            match cm.preview.as_mut() {
                Some(p) if !smooth => {
                    p.scroll = p.scroll_target;
                    false
                }
                Some(p) => {
                    let delta = p.scroll_target - p.scroll;
                    if delta.abs() <= 0.5 {
                        p.scroll = p.scroll_target;
                        false
                    } else {
                        p.scroll += delta * menu::lerp_factor(0.3, dt);
                        true
                    }
                }
                None => false,
            }
        };

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

        if !fade_animating && !scroll_animating && !preview_animating && !key_repeating {
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
            picked: &cm.picked,
            preview: cm.preview.as_ref(),
        };
        crate::menu_render::draw_clipboard(&mut pixmap, &mut self.text_cache, args);
        // ----- el panel ya trae su propio fundido; la superficie va opaca con el
        // dock nítido arriba -----
        self.show_panel_surface(qh, &pixmap, layout, 1.0);
    }
}

/// Texto COMPLETO de una entrada, envuelto al ancho del panel y con topes: el portapapeles
/// admite 256 KB, y envolver miles de líneas para leer veinte no tiene sentido.
fn texto_envuelto(entry: &crate::clipboard::ClipboardEntry, ancho: f32) -> Vec<String> {
    const MAX_CHARS: usize = 20_000;
    const MAX_LINES: usize = 400;
    let Ok(texto) = std::str::from_utf8(&entry.data) else {
        return Vec::new();
    };
    let recortado: String = texto.chars().take(MAX_CHARS).collect();
    crate::menu_render::wrap_to_width(&recortado, 10.0, ancho.max(80.0), MAX_LINES)
}

/// Los índices REALES del historial a borrar: las posiciones marcadas con `Shift+Space` (o
/// la seleccionada si no hay marcadas), mapeadas por `filtered` y ordenadas de **MAYOR a
/// menor** — que es como hay que borrarlas para que los índices que quedan no se corran —.
fn indices_a_borrar(
    filtered: &[usize],
    picked: &std::collections::BTreeSet<usize>,
    selected: usize,
) -> Vec<usize> {
    let mut out: Vec<usize> = if picked.is_empty() {
        filtered.get(selected).copied().into_iter().collect()
    } else {
        picked
            .iter()
            .filter_map(|p| filtered.get(*p).copied())
            .collect()
    };
    out.sort_unstable_by(|a, b| b.cmp(a));
    out
}

#[cfg(test)]
mod clipboard_multi_tests {
    use super::*;

    /// El borrado múltiple sale de MAYOR a menor: con los índices del historial en orden,
    /// borrar el primero corre a todos los demás y se borra lo que no era.
    #[test]
    fn los_indices_a_borrar_van_de_mayor_a_menor() {
        let filtered = [10usize, 3, 7, 1];
        let picked: std::collections::BTreeSet<usize> = [0, 2].into_iter().collect();
        assert_eq!(indices_a_borrar(&filtered, &picked, 0), vec![10, 7]);
        // ----- sin marcadas: la seleccionada -----
        let vacio = std::collections::BTreeSet::new();
        assert_eq!(indices_a_borrar(&filtered, &vacio, 1), vec![3]);
        // ----- fuera de rango: nada -----
        assert!(indices_a_borrar(&filtered, &vacio, 9).is_empty());
    }

    /// El texto de la vista previa se envuelve al ancho y se recorta: una entrada enorme no
    /// puede llenar la lista de líneas.
    #[test]
    fn el_texto_de_la_vista_previa_se_envuelve() {
        let mut entry = crate::clipboard::ClipboardEntry::from_data(
            "text/plain".into(),
            b"hola mundo como estas hoy".to_vec(),
        )
        .expect("entrada de texto");
        // ----- envuelve en varias líneas cuando el ancho es chico (el ancho tiene un
        // piso de 80, así que no baja de ahí) -----
        let lineas = texto_envuelto(&entry, 60.0);
        assert!(lineas.len() >= 2, "lineas: {lineas:?}");
        assert!(lineas.iter().all(|l| l.chars().count() < 30), "lineas: {lineas:?}");
        assert_eq!(lineas[0].split_whitespace().next(), Some("hola"));
        // ----- y una entrada gigante se recorta en vez de explotar -----
        entry.data = "palabra ".repeat(50_000).into_bytes().into();
        let recortadas = texto_envuelto(&entry, 60.0);
        assert!(recortadas.len() <= 400, "tope de lineas: {}", recortadas.len());
        // ----- bytes que no son UTF-8 no rompen nada -----
        entry.data = vec![0xff, 0xfe].into();
        assert!(texto_envuelto(&entry, 60.0).is_empty());
    }
}
