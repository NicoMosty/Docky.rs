//! Calendario del reloj: se abre al dejar el puntero encima del widget `Clock` y
//! vive en la superficie del popup (la misma de los menús del tray). Geometría y
//! aritmética del mes en `menu::calendar`; el dibujo en `menu_render::calendar`.

use super::*;

/// Cuánto tiene que quedarse el puntero sobre el reloj para abrirlo. Sin este
/// plazo el panel se abriría con sólo cruzar el widget de camino a otro lado.
pub(super) const CALENDAR_HOVER_MS: u64 = 450;

impl App {
    /// Abre el mes actual sobre el widget del reloj.
    pub(super) fn open_calendar(&mut self, qh: &QueueHandle<Self>) {
        let (controls, content_height) = menu::calendar_controls(menu::today().month);
        let tray_count = self.tray.lock().unwrap().len();
        let center = render::widget_center(
            &self.dock,
            &self.widgets,
            tray_count,
            crate::config::WidgetKind::Clock,
        );
        log::debug!("calendario: abre center={center:?}");
        self.create_popup_surface(
            menu::MenuScreen::Calendar,
            controls,
            content_height,
            Vec::new(),
            String::new(),
            String::new(),
            center,
            qh,
        );
    }

    /// ←/→ cambian de mes. El mes sale del propio `Control` del panel (única copia
    /// del estado) y se reemplaza entero: el alto no cambia (siempre 6 semanas),
    /// así que la superficie no se redimensiona y sólo hay que re-renderizar.
    pub(super) fn step_calendar_month(&mut self, dir: i32, qh: &QueueHandle<Self>) {
        let Some(p) = self.popup_mode.as_mut() else {
            return;
        };
        let Some(month) = menu::calendar_month(&p.controls) else {
            return;
        };
        let mut next = month;
        next.step(dir);
        let (controls, content_height) = menu::calendar_controls(next);
        p.controls = controls;
        p.content_height = content_height;
        p.content_dirty = true;
        p.hovered = None;
        log::debug!("calendario: {} ({dir:+})", next.label());
        self.request_popup_redraw(qh);
    }

    /// El puntero está sobre el widget del reloj.
    pub(super) fn pointer_over_clock(&self) -> bool {
        let Some((x, y)) = self.dock.pointer_pos else {
            return false;
        };
        let tray_count = self.tray.lock().unwrap().len();
        render::widget_hit_test(&self.dock, &self.widgets, tray_count, x, y)
            == Some(crate::config::WidgetKind::Clock)
    }

    /// El hover sobre el reloj ya cumplió su plazo y no hay nada más abierto: abre
    /// el calendario. Se evalúa en `autohide_timeout` (el único tick que corre con
    /// el popup abierto y también sin autohide).
    pub(super) fn calendar_hover_due(&self) -> bool {
        self.dock_visible
            && self.popup_mode.is_none()
            && self.menu.is_none()
            && !self.layer_is_borrowed()
            && self
                .calendar_hover_at
                .is_some_and(|t| t.elapsed().as_millis() >= CALENDAR_HOVER_MS as u128)
            && self.pointer_over_clock()
    }

    /// Lo llama el despacho del puntero del dock en cada Enter/Motion. Registrar el
    /// hover una sola vez (y no en cada movimiento) es lo que hace que el plazo
    /// venza: resetearlo con cada Motion lo dejaría sin abrir nunca.
    pub(super) fn note_calendar_hover(&mut self, over_clock: bool) {
        let now = std::time::Instant::now();
        let (next, recien_llego) = hover_step(self.calendar_hover_at, over_clock, now);
        self.calendar_hover_at = next;
        if recien_llego && self.popup_mode.is_none() {
            self.arm_calendar_tick();
        }
        if over_clock {
            return;
        }
        // ----- el calendario se cierra al salir del RELOJ, no del dock: quedarse
        // sobre otro widget de la barra no lo mantiene abierto. `ptr_left_at` es el
        // reloj de salida que ya usa el autohide y `popup_should_dismiss` lo lee con
        // el widget del reloj como objetivo cuando el panel es el calendario. -----
        if self
            .popup_mode
            .as_ref()
            .is_some_and(|p| p.screen == menu::MenuScreen::Calendar)
        {
            self.ptr_left_at = Some(now);
            self.arm_popup_tick();
        }
    }
}

/// Estado del hover sobre el reloj: `(instante, hay que agendar el tick)`. Pura y
/// con test porque lo que rompe la feature es el reset silencioso: si cada `Motion`
/// sobre el reloj volviera a empezar el plazo, el panel no abriría nunca.
fn hover_step(
    cur: Option<std::time::Instant>,
    over_clock: bool,
    now: std::time::Instant,
) -> (Option<std::time::Instant>, bool) {
    match (over_clock, cur) {
        // ----- primer Motion sobre el reloj: arranca el plazo -----
        (true, None) => (Some(now), true),
        // ----- sigue encima: el plazo NO se corre -----
        (true, Some(t)) => (Some(t), false),
        // ----- fuera del reloj: limpio -----
        (false, _) => (None, false),
    }
}

#[cfg(test)]
mod calendar_hover_tests {
    use super::hover_step;
    use std::time::{Duration, Instant};

    #[test]
    fn el_plazo_no_se_resetea_con_cada_movimiento() {
        let t0 = Instant::now();
        let (cur, agendar) = hover_step(None, true, t0);
        assert_eq!(cur, Some(t0));
        assert!(agendar);
        // 200ms después, otro Motion encima: el instante no se corre y no hace
        // falta agendar de nuevo (el tick ya está en camino)
        let (cur, agendar) = hover_step(cur, true, t0 + Duration::from_millis(200));
        assert_eq!(cur, Some(t0));
        assert!(!agendar);
    }

    #[test]
    fn salir_del_reloj_limpia_el_hover() {
        let t0 = Instant::now();
        let (cur, _) = hover_step(None, true, t0);
        let (cur, agendar) = hover_step(cur, false, t0 + Duration::from_millis(100));
        assert_eq!(cur, None);
        assert!(!agendar);
        // volver a entrar arranca el plazo de cero (con el instante nuevo)
        let t1 = t0 + Duration::from_millis(500);
        let (cur, agendar) = hover_step(cur, true, t1);
        assert_eq!(cur, Some(t1));
        assert!(agendar);
    }
}
