//! Panel de volumen: se abre con click izquierdo en el widget y vive en la
//! superficie del popup (la misma de los menús del tray). Datos en
//! `widgets::read_volume_rows`, geometría en `menu::volume_panel`.

use super::*;

/// Cada cuánto se manda un cambio de volumen mientras se arrastra la barra: un
/// subproceso por evento de movimiento satura el hilo principal (y el ajuste
/// llega desordenado). El valor final se aplica siempre al soltar.
const VOLUME_APPLY_MS: u128 = 50;

impl App {
    pub(super) fn open_volume_panel(&mut self, qh: &QueueHandle<Self>) {
        let rows = widgets::read_volume_rows();
        if rows.is_empty() {
            // ----- sin wpctl ni pactl no hay nada que mostrar: mejor el mute de
            // antes que un panel vacío -----
            widgets::volume_toggle_mute();
            self.refresh_sys(qh);
            return;
        }
        let devices = widgets::read_audio_devices();
        let (controls, content_height) = menu::build_volume_controls(&rows, &devices);
        let tray_count = self.tray.lock().unwrap().len();
        let center = render::widget_center(
            &self.dock,
            &self.widgets,
            tray_count,
            crate::config::WidgetKind::Volume,
        );
        self.create_popup_surface(
            menu::MenuScreen::VolumePanel,
            controls,
            content_height,
            Vec::new(),
            String::new(),
            String::new(),
            center,
            qh,
        );
        if let Some(p) = self.popup_mode.as_mut() {
            p.volume_rows = rows;
            p.volume_devices = devices;
        }
    }

    /// Relee mientras el panel está abierto: las apps aparecen y desaparecen de
    /// la lista de streams sin avisar. Se llama desde el tick de sistema.
    pub(super) fn refresh_volume_panel(&mut self, qh: &QueueHandle<Self>) {
        if !self.volume_panel_open() {
            return;
        }
        let rows = widgets::read_volume_rows();
        let devices = widgets::read_audio_devices();
        let (controls, content_height) = menu::build_volume_controls(&rows, &devices);
        let center = self.popup_mode.as_ref().and_then(|p| p.center);
        let (surface_w, surface_h, box_x, box_y) = self.popup_geometry(content_height, center);
        let region = dock_popup::popup_input_region(&self.compositor, box_x, box_y, content_height);
        let Some(p) = self.popup_mode.as_mut() else {
            return;
        };
        p.layer.set_size(surface_w as u32, surface_h as u32);
        if let Some(region) = &region {
            p.layer
                .wl_surface()
                .set_input_region(Some(region.wl_region()));
        }
        // ----- con el dedo en la barra manda el arrastre: pisar el valor con la
        // lectura deja el thumb saltando atrás -----
        if p.volume_drag.is_none() {
            p.volume_rows = rows;
        }
        p.volume_devices = devices;
        p.controls = controls;
        p.content_height = content_height;
        p.surface_w = surface_w;
        p.surface_h = surface_h;
        p.box_x = box_x;
        p.box_y = box_y;
        p.content_dirty = true;
        self.request_popup_redraw(qh);
    }

    fn volume_panel_open(&self) -> bool {
        self.popup_mode
            .as_ref()
            .is_some_and(|p| p.screen == menu::MenuScreen::VolumePanel)
    }

    /// Click izquierdo dentro del panel. `true` si lo consumió (el despacho del
    /// popup tiene ramas propias para los menús del tray).
    pub(super) fn volume_panel_press(&mut self, x: f32, y: f32, qh: &QueueHandle<Self>) -> bool {
        if !self.volume_panel_open() {
            return false;
        }
        let Some((box_x, box_y)) = self.popup_mode.as_ref().map(|p| (p.box_x, p.box_y)) else {
            return false;
        };
        let hit = self.popup_mode.as_ref().and_then(|p| {
            menu::hit_test(
                &p.controls,
                &self.dock.config.settings,
                menu::MENU_WIDTH,
                x - box_x,
                y - box_y,
            )
        });
        log::debug!(
            "volumen: click panel ({:.0},{:.0}) -> {hit:?}",
            x - box_x,
            y - box_y
        );
        match hit {
            Some(menu::HitTarget::VolumeTrack(i)) => {
                if let Some(p) = self.popup_mode.as_mut() {
                    p.volume_drag = Some(i);
                }
                self.drag_volume_row(i, x - box_x, true, qh);
                true
            }
            Some(menu::HitTarget::VolumeMute(i)) => {
                let target = self
                    .popup_mode
                    .as_ref()
                    .and_then(|p| p.volume_rows.get(i))
                    .map(|r| r.target);
                let Some(target) = target else { return true };
                match target {
                    widgets::VolumeTarget::Output => widgets::volume_toggle_mute(),
                    widgets::VolumeTarget::Stream(id) => widgets::toggle_stream_mute(id),
                }
                if let Some(p) = self.popup_mode.as_mut() {
                    if let Some(row) = p.volume_rows.get_mut(i) {
                        row.muted = !row.muted;
                    }
                    p.content_dirty = true;
                }
                self.request_popup_redraw(qh);
                true
            }
            Some(menu::HitTarget::VolumeDevice(i)) => {
                let name = self
                    .popup_mode
                    .as_ref()
                    .and_then(|p| p.volume_devices.get(i))
                    .map(|d| d.name.clone());
                let Some(name) = name else { return true };
                widgets::set_default_sink(&name);
                if let Some(p) = self.popup_mode.as_mut() {
                    for d in p.volume_devices.iter_mut() {
                        d.default = d.name == name;
                    }
                    p.content_dirty = true;
                }
                self.request_popup_redraw(qh);
                // ----- cambiar de salida mueve el stream y la lectura tarda un
                // instante: el tick de sistema rehace la lista enseguida -----
                true
            }
            _ => false,
        }
    }

    /// Arrastre con el botón apretado. Sólo consume el evento si hay una barra
    /// agarrada; si no, el movimiento sigue siendo hover del panel.
    pub(super) fn volume_panel_drag(&mut self, x: f32, qh: &QueueHandle<Self>) -> bool {
        let Some(i) = self.popup_mode.as_ref().and_then(|p| p.volume_drag) else {
            return false;
        };
        let box_x = self.popup_mode.as_ref().map(|p| p.box_x).unwrap_or(0.0);
        self.drag_volume_row(i, x - box_x, false, qh);
        true
    }

    pub(super) fn volume_panel_release(&mut self, x: f32, qh: &QueueHandle<Self>) -> bool {
        let Some(i) = self.popup_mode.as_mut().and_then(|p| p.volume_drag.take()) else {
            return false;
        };
        let box_x = self.popup_mode.as_ref().map(|p| p.box_x).unwrap_or(0.0);
        // el paso final se aplica siempre: el throttle pudo dejarlo sin mandar
        self.drag_volume_row(i, x - box_x, true, qh);
        true
    }

    /// Valor que corresponde a `x` (coordenada del panel) en la fila `i`. Con
    /// `force` se manda sí o sí; los pasos intermedios van limitados por
    /// `VOLUME_APPLY_MS` (el número en pantalla se actualiza igual).
    fn drag_volume_row(&mut self, i: usize, panel_x: f32, force: bool, qh: &QueueHandle<Self>) {
        let pct = menu::volume_pct_from_x(menu::MENU_WIDTH, panel_x);
        let now = std::time::Instant::now();
        let apply = force
            || self.popup_mode.as_ref().is_some_and(|p| {
                p.volume_apply_at
                    .is_none_or(|t| now.duration_since(t).as_millis() >= VOLUME_APPLY_MS)
            });
        let Some(p) = self.popup_mode.as_mut() else {
            return;
        };
        let Some(row) = p.volume_rows.get_mut(i) else {
            return;
        };
        if row.pct == pct && !force {
            return;
        }
        let target = row.target;
        row.pct = pct;
        if apply {
            p.volume_apply_at = Some(now);
            match target {
                widgets::VolumeTarget::Output => widgets::set_output_volume(pct),
                widgets::VolumeTarget::Stream(id) => widgets::set_stream_volume(id, pct),
            }
        }
        p.content_dirty = true;
        self.request_popup_redraw(qh);
    }
}
