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

/// Estado de la actividad de batería: warm-up del primer dato, umbral ya avisado (101 =
/// ninguno) y si estaba enchufada en la lectura previa. Está acá y no en `App` para poder
/// testear la transición sin construir el dock (el cargador no siempre está a mano).
#[derive(Default)]
pub(crate) struct BatteryActivity {
    ready: bool,
    warned: u8,
    on_power: bool,
}

impl BatteryActivity {
    /// Registra una lectura y dice si hay que anunciar. `None` (sin dato) no toca nada.
    pub(crate) fn note(&mut self, battery: Option<(u8, BatteryState)>) -> bool {
        let Some((pct, state)) = battery else {
            return false;
        };
        let on_power = !matches!(state, BatteryState::Discharging);
        if !self.ready {
            // ----- warm-up: el estado con el que arranca el dock no es novedad -----
            self.ready = true;
            self.on_power = on_power;
            self.warned = battery_threshold(pct);
            return false;
        }
        let avisar = if on_power && !self.on_power {
            // ----- enchufada: una vez por episodio, y rearma los umbrales de descarga -----
            self.warned = 101;
            true
        } else if !on_power {
            // ----- descargando: 20 % y 10 %, cada umbral una sola vez -----
            let threshold = battery_threshold(pct);
            if threshold < self.warned {
                self.warned = threshold;
                true
            } else {
                false
            }
        } else {
            false
        };
        self.on_power = on_power;
        avisar
    }
}

/// Estado de la actividad de Bluetooth: warm-up del primer dato y último dispositivo
/// conectado visto. Está acá por la misma razón que `BatteryActivity`.
#[derive(Default)]
pub(crate) struct BluetoothActivity {
    ready: bool,
    connected: Option<String>,
}

impl BluetoothActivity {
    /// Registra el dispositivo conectado (`None` = ninguno) y dice si hay que anunciar. El
    /// primer dato nunca lo es: es el estado con el que arranca el dock.
    pub(crate) fn note(&mut self, ahora: Option<String>) -> bool {
        let avisar = self.ready && ahora != self.connected;
        self.ready = true;
        self.connected = ahora;
        avisar
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
    /// descargando. La transición la decide `BatteryActivity` (con test).
    pub(crate) fn note_battery_activity(
        &mut self,
        battery: Option<(u8, BatteryState)>,
        qh: &QueueHandle<Self>,
    ) {
        if !self.battery_activity.note(battery) {
            return;
        }
        log::debug!("island: bateria -> anuncio");
        self.announce_island_activity(WidgetKind::Battery, qh);
    }

    /// Anuncia el cambio de dispositivo Bluetooth (conectar, desconectar, cambiar). La
    /// transición la decide `BluetoothActivity` (con test).
    pub(crate) fn note_bluetooth_activity(
        &mut self,
        ahora: Option<String>,
        qh: &QueueHandle<Self>,
    ) {
        if !self.bluetooth_activity.note(ahora) {
            return;
        }
        log::debug!("island: bluetooth -> anuncio");
        self.announce_island_activity(WidgetKind::Bluetooth, qh);
    }

    /// Anuncia la captura guardada. `path = None` significa que fue SÓLO al portapapeles
    /// (así lo manda niri). La etiqueta es el nombre del archivo.
    pub(crate) fn note_screenshot(&mut self, path: Option<String>, qh: &QueueHandle<Self>) {
        let label = screenshot_label(path.as_deref());
        log::debug!("island: captura -> {label}");
        self.widgets.screenshot = Some(label);
        self.announce_island_activity(WidgetKind::Screenshot, qh);
    }

    /// Aplica el conteo de pantallas compartidas (`CastsChanged` de niri). Empezar a
    /// compartir y dejar de hacerlo son novedades; además es un ESTADO, así que la isla lo
    /// muestra en el reposo mientras haya cast (ver `island_activities`).
    pub(crate) fn note_casts(&mut self, count: usize, qh: &QueueHandle<Self>) {
        let antes = self.widgets.cast.unwrap_or(0);
        self.widgets.cast = (count > 0).then_some(count);
        if antes != count {
            log::debug!("island: casts {antes} -> {count}");
            self.announce_island_activity(WidgetKind::Cast, qh);
        }
        // ----- el reposo de la isla cambió (aparece o se va el indicador): repintar -----
        self.needs_repaint = true;
        self.request_redraw(qh);
    }
}

/// Etiqueta de la captura: el nombre del archivo (sin la carpeta), o "Portapapeles" si
/// niri no dio ruta. Pura, para que un path raro no la rompa.
fn screenshot_label(path: Option<&str>) -> String {
    let Some(path) = path.filter(|p| !p.is_empty()) else {
        return "Portapapeles".to_string();
    };
    match path.rsplit('/').next() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => "Captura".to_string(),
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

    /// La etiqueta de la captura es el nombre del archivo; sin ruta es "Portapapeles"
    /// (niri manda `path: null` cuando la captura fue sólo al portapapeles).
    #[test]
    fn la_etiqueta_de_la_captura_es_el_nombre_del_archivo() {
        assert_eq!(
            screenshot_label(Some("/home/u/Pics/Shot_1.png")),
            "Shot_1.png"
        );
        assert_eq!(screenshot_label(Some("Shot.png")), "Shot.png");
        // ----- sin ruta = sólo portapapeles -----
        assert_eq!(screenshot_label(None), "Portapapeles");
        assert_eq!(screenshot_label(Some("")), "Portapapeles");
        // ----- un path que termina en barra no deja la etiqueta vacía -----
        assert_eq!(screenshot_label(Some("/tmp/")), "Captura");
    }

    /// La transición de la actividad de batería: warm-up, el enchufe, los dos umbrales una
    /// sola vez, y el rearme al enchufar. Es lo que no se puede probar sin cargador.
    #[test]
    fn la_bateria_anuncia_al_enchufar_y_en_los_umbrales() {
        use crate::widgets::BatteryState::{Charging, Discharging};
        let mut a = BatteryActivity::default();
        // ----- warm-up: el primer dato no anuncia -----
        assert!(!a.note(Some((50, Discharging))));
        // ----- descargando: 50 y 21 no; 20 sí; repetir el 20 no; 10 sí; 5 no -----
        assert!(!a.note(Some((50, Discharging))));
        assert!(!a.note(Some((21, Discharging))));
        assert!(a.note(Some((20, Discharging))), "cruza 20");
        assert!(!a.note(Some((19, Discharging))), "el 20 ya se aviso");
        assert!(!a.note(Some((11, Discharging))));
        assert!(a.note(Some((10, Discharging))), "cruza 10");
        assert!(!a.note(Some((5, Discharging))), "el 10 ya se aviso");
        // ----- enchufar: anuncia y rearma los umbrales -----
        assert!(a.note(Some((15, Charging))), "enchufa");
        assert!(!a.note(Some((20, Charging))), "ya enchufada no repite");
        // ----- desenchufar en 20: el umbral se rearmo, asi que avisa -----
        assert!(a.note(Some((20, Discharging))), "desenchufa en 20");
        assert!(!a.note(Some((20, Discharging))), "y no repite");
        assert!(a.note(Some((10, Discharging))), "baja a 10");
        // ----- sin dato no toca nada -----
        assert!(!a.note(None));
        assert!(
            !a.note(Some((10, Discharging))),
            "el None no cambio el estado"
        );
    }

    /// La transición de la actividad de Bluetooth: el primer dato nunca anuncia; conectar,
    /// desconectar y cambiar de dispositivo sí.
    #[test]
    fn el_bluetooth_no_anuncia_el_primer_dato_y_si_los_cambios() {
        let mut b = BluetoothActivity::default();
        assert!(!b.note(None), "warm-up sin dispositivo");
        assert!(!b.note(None), "mismo estado");
        assert!(b.note(Some("headset".into())), "conecta");
        assert!(!b.note(Some("headset".into())), "mismo dispositivo");
        assert!(b.note(None), "desconecta");
        assert!(b.note(Some("otro".into())), "otro dispositivo");
    }
}
