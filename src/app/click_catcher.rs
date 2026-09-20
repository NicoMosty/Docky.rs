//! Catcher de clicks afuera.
//!
//! El launcher y los paneles de ajustes viven en la superficie del dock, que con el
//! dock anclado arriba mide **[dock][8px][panel]** (`panel_layout`): un click fuera de
//! ese rectángulo lo entrega el compositor a la ventana de abajo y la app no se
//! entera. Por eso sólo se cerraban con Escape (el teclado sí lo tiene la
//! superficie), con click derecho o clickeando otro widget del dock.
//!
//! `wlr-layer-shell` **no tiene pointer grab** —sus requests son `set_size`,
//! `set_anchor`, `set_exclusive_zone`, `set_margin`, `set_keyboard_interactivity`
//! y `get_popup`— así que la única forma de enterarse es tener una superficie
//! propia debajo del click. Es el mismo truco que el selector de región de
//! screenshots, que ya es una superficie full-screen con input region.
//!
//! Acá la región es la pantalla **menos** el rectángulo del dock (`wl_region` es
//! una unión, así que el agujero se arma con los cuatro rectángulos que lo
//! rodean): los clicks del panel le siguen llegando al panel y cualquier otro
//! cierra el modo, igual que Escape.

use super::*;

pub(crate) struct ClickCatcher {
    layer: LayerSurface,
    /// Buffer transparente y su pool: se crean en el configure, que es cuando se
    /// sabe cuánto mide la salida. El pool de shm no se escribe: los ceros del
    /// memfd ya son transparentes, sólo tiene que existir para que la superficie
    /// esté mapeada y reciba input.
    buffer: Option<(SlotPool, smithay_client_toolkit::shm::slot::Buffer)>,
    /// Tamaño de la salida, del configure (= el de la superficie entera).
    screen: (i32, i32),
    /// Rectángulo del dock que queda afuera de la input region (el agujero).
    hole: (i32, i32, i32, i32),
}

impl ClickCatcher {
    /// La superficie del catcher, para el despacho de eventos por superficie.
    pub(super) fn surface(&self) -> &wl_surface::WlSurface {
        self.layer.wl_surface()
    }
}

/// Rectángulos que cubren `screen` menos `hole`. Es la cuenta que hace que los
/// clicks del panel sigan llegando al panel; pura para poder testearla.
pub(crate) fn catcher_region_rects(
    hole: (i32, i32, i32, i32),
    screen: (i32, i32),
) -> Vec<(i32, i32, i32, i32)> {
    let (hx, hy, hw, hh) = hole;
    let (sw, sh) = screen;
    // ----- recortado contra la pantalla: el dock puede quedar pegado a un borde,
    // y una salida más chica que el dock daría rectángulos negativos -----
    let x0 = hx.clamp(0, sw);
    let y0 = hy.clamp(0, sh);
    let x1 = (hx + hw).clamp(x0, sw);
    let y1 = (hy + hh).clamp(y0, sh);
    let mut rects = Vec::with_capacity(4);
    if y0 > 0 {
        rects.push((0, 0, sw, y0));
    }
    if y1 < sh {
        rects.push((0, y1, sw, sh - y1));
    }
    if x0 > 0 {
        rects.push((0, y0, x0, y1 - y0));
    }
    if x1 < sw {
        rects.push((x1, y0, sw - x1, y1 - y0));
    }
    rects
}

fn catcher_region(
    compositor: &CompositorState,
    hole: (i32, i32, i32, i32),
    screen: (i32, i32),
) -> Option<Region> {
    let region = Region::new(compositor).ok()?;
    for (x, y, w, h) in catcher_region_rects(hole, screen) {
        region.add(x, y, w, h);
    }
    Some(region)
}

impl App {
    /// Tamaño de la superficie compartida del modo que se cierra con un click
    /// afuera, si hay uno. Es el tamaño de la SUPERFICIE (el dock + el panel cuando el
    /// panel va debajo), no el del panel: el agujero tiene que cubrir todo lo que
    /// dibuja la app, si no el catcher se come los clicks de la franja del dock y del
    /// borde de abajo del panel.
    fn catcher_panel_size(&self) -> Option<(u32, u32)> {
        let l = self.overlay_layout()?;
        Some((
            l.surf.0.round().max(1.0) as u32,
            l.surf.1.round().max(1.0) as u32,
        ))
    }

    /// Dónde queda la superficie del dock en pantalla. Es la cuenta inversa de
    /// `edge_anchor_margin`: ahí se le pide al compositor un anclaje y un margen,
    /// acá se deduce dónde la dejó (hace falta para recortar el agujero).
    fn dock_screen_rect(&self, size: (u32, u32), screen: (i32, i32)) -> (i32, i32, i32, i32) {
        use crate::config::{DockAlign, DockEdge};
        let s = &self.dock.config.settings;
        let (w, h) = (size.0 as i32, size.1 as i32);
        let (sw, sh) = screen;
        let centro = |largo: i32, salida: i32| (salida - largo) / 2;
        let (x, y) = match s.dock_edge {
            DockEdge::Top | DockEdge::Bottom => {
                let x = match s.dock_align {
                    DockAlign::Left => 0,
                    DockAlign::Middle => centro(w, sw),
                    DockAlign::Right => sw - w,
                };
                let y = if matches!(s.dock_edge, DockEdge::Top) {
                    s.pos_y
                } else {
                    sh - h - s.pos_y
                };
                (x, y)
            }
            DockEdge::Left | DockEdge::Right => {
                let y = match s.dock_align {
                    DockAlign::Left => 0,
                    DockAlign::Middle => centro(h, sh),
                    DockAlign::Right => sh - h,
                };
                let x = if matches!(s.dock_edge, DockEdge::Left) {
                    s.pos_y
                } else {
                    sw - w - s.pos_y
                };
                (x, y)
            }
        };
        (x, y, w, h)
    }

    /// Crea, actualiza o desmapea el catcher según haya (o no) un panel abierto y
    /// cuánto mida. Corre desde `draw_ex`, así que un cambio de pestaña (que cambia
    /// el alto) recalcula el agujero y el cierre lo desmapea.
    pub(super) fn sync_click_catcher(&mut self, qh: &QueueHandle<Self>) {
        let Some(size) = self.catcher_panel_size() else {
            // ----- sin panel: se desmapea de verdad (drop), no una región vacía:
            // el buffer es del tamaño de la pantalla y no tiene por qué quedar -----
            self.click_catcher = None;
            return;
        };
        if self.click_catcher.is_none() {
            self.create_click_catcher(qh);
        }
        let screen = self
            .click_catcher
            .as_ref()
            .map(|c| c.screen)
            .unwrap_or_default();
        // ----- sin configure todavía no se sabe cuánto mide la pantalla: el agujero
        // se arma en `catcher_configure` -----
        if screen.0 == 0 || screen.1 == 0 {
            return;
        }
        let hole = self.dock_screen_rect(size, screen);
        let region = catcher_region(&self.compositor, hole, screen);
        let Some(c) = self.click_catcher.as_mut() else {
            return;
        };
        if hole == c.hole {
            return;
        }
        c.hole = hole;
        c.layer
            .wl_surface()
            .set_input_region(region.as_ref().map(|r| r.wl_region()));
        c.layer.commit();
    }

    fn create_click_catcher(&mut self, qh: &QueueHandle<Self>) {
        let surface = self.compositor.create_surface(qh);
        let layer = self.layer_shell.create_layer_surface(
            qh,
            surface,
            Layer::Top,
            Some("dockyrs-catcher"),
            self.pinned_output.as_ref(),
        );
        // ----- anclada a los cuatro bordes con tamaño 0 = la salida entera. La
        // `Layer::Top` es la misma del panel a propósito: si una ventana
        // full-screen tapa el panel, tiene que tapar el catcher también. -----
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_size(0, 0);
        layer.set_exclusive_zone(-1);
        // ----- el teclado lo tiene el panel: si el catcher lo pidiera, Escape
        // dejaría de llegarle -----
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        // ----- arranca con región VACÍA: sin el configure no se sabe dónde está el
        // dock, y una región completa se comería los clicks del panel -----
        if let Ok(region) = Region::new(&self.compositor) {
            layer
                .wl_surface()
                .set_input_region(Some(region.wl_region()));
        }
        layer.commit();
        log::debug!("catcher: crear superficie full-screen");
        self.click_catcher = Some(ClickCatcher {
            layer,
            buffer: None,
            screen: (0, 0),
            hole: (0, 0, 0, 0),
        });
    }

    /// Configure del catcher: llega con el tamaño de la salida (la superficie está
    /// anclada a los cuatro bordes). Recién acá se puede calcular el agujero.
    pub(super) fn catcher_configure(&mut self, size: (u32, u32)) {
        let (w, h) = (size.0.max(1), size.1.max(1));
        let screen = (w as i32, h as i32);
        let stride = w * 4;
        let pool_size = (stride as usize * h as usize + 4096).max(65536);
        let mut pool = SlotPool::new(pool_size, &self.shm).ok();
        let buffer = pool.as_mut().and_then(|p| {
            p.create_buffer(w as i32, h as i32, stride as i32, wl_shm::Format::Argb8888)
                .ok()
                .map(|(buffer, _canvas)| buffer)
        });
        let Some(c) = self.click_catcher.as_mut() else {
            return;
        };
        if c.screen == screen && c.buffer.is_some() {
            return;
        }
        c.screen = screen;
        match (pool, buffer) {
            (Some(pool), Some(buffer)) => {
                if let Err(err) = buffer.attach_to(c.layer.wl_surface()) {
                    log::error!("catcher: no pude mapear el buffer transparente: {err}");
                    return;
                }
                c.layer.commit();
                c.buffer = Some((pool, buffer));
            }
            _ => {
                log::error!("catcher: no pude crear el buffer transparente ({w}x{h})");
                return;
            }
        }
        log::debug!("catcher: configure screen={screen:?}");
        // ----- el agujero: cambió el tamaño de la pantalla, así que el que tenía
        // anotado (0,0,0,0) ya no sirve y hay que re-aplicar la región -----
        c.hole = (0, 0, 0, 0);
    }

    /// Un click en el catcher es un click afuera: cierra el panel, igual que Escape.
    pub(super) fn handle_catcher_pointer_event(
        &mut self,
        event: &PointerEvent,
        qh: &QueueHandle<Self>,
    ) {
        if let PointerEventKind::Press { .. } = event.kind {
            log::debug!("catcher: click afuera -> cerrar");
            self.dismiss_overlay(qh);
        }
    }

    /// Cierra lo que esté abierto encima del dock (lo mismo que hace Escape en cada
    /// modo). Lo llama el catcher cuando clickeás fuera.
    pub(super) fn dismiss_overlay(&mut self, qh: &QueueHandle<Self>) {
        if self.dock_menu_mode.is_some() {
            self.close_dock_menu(qh);
        } else if self.app_search_mode.is_some() {
            self.close_app_search_mode(qh);
        } else if self.notifications_mode.is_some() {
            self.close_notifications_mode(qh);
        } else if self.clipboard_mode.is_some() {
            self.close_clipboard_mode(qh);
        } else if self.wallpaper_mode.is_some() {
            self.close_wallpaper_mode(qh);
        }
    }
}

#[cfg(test)]
mod catcher_tests {
    use super::*;

    fn area(rects: &[(i32, i32, i32, i32)]) -> i32 {
        rects.iter().map(|(_, _, w, h)| w * h).sum()
    }

    fn cubre(rects: &[(i32, i32, i32, i32)], x: i32, y: i32) -> usize {
        rects
            .iter()
            .filter(|(rx, ry, rw, rh)| x >= *rx && x < rx + rw && y >= *ry && y < ry + rh)
            .count()
    }

    /// El contrato: los rectángulos cubren la pantalla MENOS el agujero y sin
    /// pisarse. Si se solaparan o dejaran una franja sin cubrir, o el panel no
    /// recibiría los clicks o una zona fuera del panel no cerraría nada.
    #[test]
    fn la_region_tapa_la_pantalla_menos_el_agujero() {
        let screen = (1920, 1080);
        let rects = catcher_region_rects((640, 0, 640, 236), screen);
        assert_eq!(area(&rects), 1920 * 1080 - 640 * 236);
        // ----- adentro del agujero no hay ninguno: el click es del panel -----
        for (x, y) in [(640, 0), (1000, 120), (1279, 235)] {
            assert_eq!(cubre(&rects, x, y), 0, "el punto {x},{y} es del panel");
        }
        // ----- afuera, exactamente uno -----
        for (x, y) in [(0, 0), (639, 235), (1280, 0), (960, 236), (1919, 1079)] {
            assert_eq!(
                cubre(&rects, x, y),
                1,
                "el punto {x},{y} tendría que cerrar"
            );
        }
        // ----- y nada se sale de la pantalla -----
        for (x, y, w, h) in &rects {
            assert!(x >= &0 && y >= &0 && x + w <= 1920 && y + h <= 1080);
        }
    }

    /// Con el dock abajo, o pegado a los bordes, no tienen que aparecer
    /// rectángulos de tamaño cero ni negativo.
    #[test]
    fn los_bordes_se_recortan() {
        let rects = catcher_region_rects((0, 900, 1920, 180), (1920, 1080));
        assert_eq!(
            area(&rects),
            1920 * (1080 - 180),
            "cubre la pantalla menos el dock"
        );
        assert!(rects.iter().all(|(_, _, w, h)| *w > 0 && *h > 0));
        // ----- agujero más grande que la pantalla: no queda región, pero no puede
        // paniquear ni devolver rectángulos raros -----
        assert!(catcher_region_rects((0, 0, 4000, 4000), (1920, 1080)).is_empty());
        // ----- dock vertical, agujero a la izquierda -----
        let rects = catcher_region_rects((0, 400, 66, 300), (1920, 1080));
        assert_eq!(area(&rects), 1920 * 1080 - 66 * 300);
        assert_eq!(cubre(&rects, 10, 500), 0, "el punto del dock no cierra");
        assert_eq!(cubre(&rects, 100, 500), 1, "al lado del dock sí");
    }

    /// Con el panel debajo del dock la superficie es [dock][8px][panel] (el launcher
    /// mide 26+8+236 = 270), y el agujero tiene que ser TODO eso: la app dibuja el dock
    /// en la franja de arriba. Si el agujero se calculara con el alto del panel, la
    /// franja de abajo quedaría tapada por el catcher y un click en el borde inferior
    /// del panel lo cerraría en vez de tocar el control (era el caso antes de que el
    /// agujero saliera de `overlay_layout`).
    #[test]
    fn el_agujero_cubre_el_dock_y_el_panel() {
        let screen = (1920, 1080);
        let rects = catcher_region_rects((640, 0, 640, 270), screen);
        assert_eq!(area(&rects), 1920 * 1080 - 640 * 270);
        assert_eq!(cubre(&rects, 960, 10), 0, "la franja del dock es de la app");
        assert_eq!(cubre(&rects, 960, 269), 0, "y el borde de abajo del panel");
        assert_eq!(cubre(&rects, 960, 270), 1, "justo debajo ya cierra");
    }
}
