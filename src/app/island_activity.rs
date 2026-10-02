use super::*;
use crate::config::WidgetKind;
use crate::widgets::BatteryState;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Cuánto dura una actividad viva en la isla antes de volver al reposo (la hora y la
/// batería). Un solo valor: la actividad es una novedad, no un panel.
pub(crate) const ISLAND_ACTIVITY_MS: u64 = 3200;

/// Cola de actividades vivas de la isla (batería cargando, y las que vengan: bluetooth,
/// portapapeles, descargas). Estado **puro** (sin Wayland) a propósito: la decisión de
/// qué se muestra, cuándo espera y cuándo rota se testea sin construir el `App` entero,
/// mismo criterio que `popup_dismiss_due` y `accion_del_configure`.
///
/// Reglas (espejo del `showFeedback` de island):
/// - una actividad viva **ocupa la isla**: reemplaza el reparto de reposo (hora +
///   batería) mientras dura;
/// - si ya hay una activa, la nueva **espera** en la cola (no la pisa ni la corta);
/// - al vencer la activa se toma la siguiente (FIFO) y si la cola está vacía se vuelve
///   al reposo;
/// - un repetido (la que ya está, o dos iguales seguidas en la cola) se ignora.
#[derive(Default)]
pub(crate) struct IslandActivities {
    current: Option<(WidgetKind, Instant)>,
    queue: VecDeque<WidgetKind>,
}

impl IslandActivities {
    /// La actividad que la isla muestra AHORA (`None` = reposo).
    pub(crate) fn current(&self) -> Option<WidgetKind> {
        self.current.map(|(kind, _)| kind)
    }

    /// Encola una actividad. Devuelve `true` si además quedó **a la vista** (o sea: hay
    /// que repintar); `false` si espera en la cola o si era un repetido.
    pub(crate) fn announce(&mut self, kind: WidgetKind, now: Instant) -> bool {
        self.expire(now);
        if self.current() == Some(kind) || self.queue.back() == Some(&kind) {
            return false;
        }
        let until = now + Duration::from_millis(ISLAND_ACTIVITY_MS);
        match self.current {
            Some(_) => {
                self.queue.push_back(kind);
                false
            }
            None => {
                self.current = Some((kind, until));
                true
            }
        }
    }

    /// Rota si la activa ya venció. Devuelve `true` si cambió lo que se ve (hay que
    /// repintar). Es la ÚNICA cuenta del vencimiento.
    pub(crate) fn expire(&mut self, now: Instant) -> bool {
        let Some((_, until)) = self.current else {
            return false;
        };
        if now < until {
            return false;
        }
        let until = now + Duration::from_millis(ISLAND_ACTIVITY_MS);
        self.current = self.queue.pop_front().map(|kind| (kind, until));
        true
    }
}

/// Umbral de batería ya avisado (101 = ninguno): 20 % y 10 %, una vez por descarga,
/// como el `batteryWarned` de island.
fn battery_threshold(pct: u8) -> u8 {
    if pct <= 10 {
        10
    } else if pct <= 20 {
        20
    } else {
        101
    }
}

impl App {
    /// Anuncia una actividad viva en la isla. Se descarta si no hay isla que la muestre
    /// (dock a la vista, o un panel/menú/popup ocupando la superficie): la actividad es
    /// una novedad, no un panel, y mostrarla tarde confunde.
    pub(crate) fn announce_island_activity(&mut self, kind: WidgetKind, qh: &QueueHandle<Self>) {
        if self.dock_visible
            || self.menu.is_some()
            || self.popup_mode.is_some()
            || self.layer_is_borrowed()
        {
            return;
        }
        if !self.island_activity.announce(kind, Instant::now()) {
            return;
        }
        log::debug!("island: actividad {kind:?}");
        self.arm_island_activity_tick();
        self.needs_repaint = true;
        self.request_redraw(qh);
    }

    /// La actividad que la isla tiene que mostrar ahora (la viva, o `None` = reposo).
    pub(crate) fn active_island_activity(&self) -> Option<WidgetKind> {
        self.island_activity.current()
    }

    /// Vence la actividad viva y toma la siguiente de la cola. La llama
    /// `autohide_timeout`, el único tick que corre con el dock oculto.
    pub(crate) fn tick_island_activity(&mut self, qh: &QueueHandle<Self>) {
        if !self.island_activity.expire(Instant::now()) {
            return;
        }
        log::debug!(
            "island: fin de actividad -> {:?}",
            self.active_island_activity()
        );
        if self.active_island_activity().is_some() {
            self.arm_island_activity_tick();
        }
        self.needs_repaint = true;
        self.request_redraw(qh);
    }

    /// El vencimiento va por el canal del autohide (`arm_calendar_tick` usa la misma
    /// jugada): el envío es directo, así que también sirve con el autohide apagado.
    fn arm_island_activity_tick(&mut self) {
        let _ = self.autohide_hide_tx.send(ISLAND_ACTIVITY_MS);
    }

    /// Anuncia la actividad de batería al **enchufar** o al cruzar **20 % / 10 %**
    /// descargando. El primer dato sólo se registra (warm-up): el estado con el que
    /// arranca el dock no es una novedad, igual que los 3 s de island.
    pub(crate) fn note_battery_activity(
        &mut self,
        battery: Option<(u8, BatteryState)>,
        qh: &QueueHandle<Self>,
    ) {
        let Some((pct, state)) = battery else {
            return;
        };
        let on_power = !matches!(state, BatteryState::Discharging);
        if !self.battery_activity_ready {
            self.battery_activity_ready = true;
            self.battery_on_power = on_power;
            self.battery_warned = battery_threshold(pct);
            return;
        }
        if on_power && !self.battery_on_power {
            // ----- enchufada: una vez por episodio, y rearma los umbrales de descarga -----
            self.battery_warned = 101;
            self.announce_island_activity(WidgetKind::Battery, qh);
        } else if !on_power {
            // ----- descargando: 20 % y 10 %, cada umbral una sola vez -----
            let threshold = battery_threshold(pct);
            if threshold < self.battery_warned {
                self.battery_warned = threshold;
                self.announce_island_activity(WidgetKind::Battery, qh);
            }
        }
        self.battery_on_power = on_power;
    }
}

#[cfg(test)]
mod island_activity_tests {
    use super::*;

    #[test]
    fn la_actividad_viva_ocupa_la_isla_y_vence() {
        let t0 = Instant::now();
        let mut a = IslandActivities::default();
        assert_eq!(a.current(), None, "sin nada anunciado no hay isla");
        assert!(
            a.announce(WidgetKind::Battery, t0),
            "la primera se muestra ya"
        );
        assert_eq!(a.current(), Some(WidgetKind::Battery));
        // ----- antes del plazo sigue -----
        let casi = t0 + Duration::from_millis(ISLAND_ACTIVITY_MS - 1);
        assert!(!a.expire(casi));
        assert_eq!(a.current(), Some(WidgetKind::Battery));
        // ----- al vencer, al reposo -----
        let tarde = t0 + Duration::from_millis(ISLAND_ACTIVITY_MS);
        assert!(a.expire(tarde));
        assert_eq!(a.current(), None);
        assert!(
            !a.expire(tarde + Duration::from_millis(500)),
            "ya en reposo no rota"
        );
    }

    #[test]
    fn la_segunda_actividad_espera_en_la_cola_y_rota() {
        let t0 = Instant::now();
        let mut a = IslandActivities::default();
        assert!(a.announce(WidgetKind::Recording, t0));
        // ----- la segunda NO pisa a la que está: espera -----
        assert!(!a.announce(WidgetKind::Battery, t0));
        assert_eq!(a.current(), Some(WidgetKind::Recording));
        // ----- al vencer la primera entra la segunda -----
        assert!(a.expire(t0 + Duration::from_millis(ISLAND_ACTIVITY_MS)));
        assert_eq!(a.current(), Some(WidgetKind::Battery));
        // ----- y al vencer la segunda, reposo -----
        assert!(a.expire(t0 + Duration::from_millis(2 * ISLAND_ACTIVITY_MS)));
        assert_eq!(a.current(), None);
    }

    #[test]
    fn no_se_repite_la_misma_actividad_seguida() {
        let t0 = Instant::now();
        let dur = Duration::from_millis(ISLAND_ACTIVITY_MS);
        let mut a = IslandActivities::default();
        assert!(a.announce(WidgetKind::Battery, t0));
        assert!(
            !a.announce(WidgetKind::Battery, t0),
            "la que ya está no se re-anuncia"
        );
        // ----- en la cola: B espera, y un segundo B seguido se ignora -----
        assert!(!a.announce(WidgetKind::Recording, t0));
        assert!(
            !a.announce(WidgetKind::Recording, t0),
            "dos iguales seguidas en la cola no"
        );
        assert!(a.expire(t0 + dur), "entra B");
        assert_eq!(a.current(), Some(WidgetKind::Recording));
        // ----- pero A -> B -> A sí (no es "seguida") -----
        assert!(!a.announce(WidgetKind::Battery, t0 + dur));
        assert!(a.expire(t0 + 2 * dur), "vuelve A");
        assert_eq!(a.current(), Some(WidgetKind::Battery));
    }

    #[test]
    fn los_umbrales_de_bateria_son_20_y_10() {
        assert_eq!(battery_threshold(100), 101);
        assert_eq!(battery_threshold(21), 101);
        assert_eq!(battery_threshold(20), 20);
        assert_eq!(battery_threshold(11), 20);
        assert_eq!(battery_threshold(10), 10);
        assert_eq!(battery_threshold(0), 10);
    }
}
