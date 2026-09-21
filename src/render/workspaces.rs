use super::*;

pub(super) const WS_DOT: f32 = 13.0;
pub(super) const WS_ACTIVE: f32 = 28.0;
pub(super) const WS_SLOT: f32 = 24.0;

fn slot_count(workspaces: &[crate::widgets::WorkspaceInfo]) -> usize {
    workspaces.len().max(1)
}

fn active_slot(workspaces: &[crate::widgets::WorkspaceInfo]) -> f32 {
    workspaces
        .iter()
        .position(|w| w.active)
        .map(|i| i as f32 + 1.0)
        .unwrap_or(1.0)
}

pub fn ws_target_for(workspaces: &[crate::widgets::WorkspaceInfo]) -> f32 {
    active_slot(workspaces)
}

/// ¿El workspace activo no tiene ventanas? El autohide lo usa para dejar el dock
/// fijo en pantalla: sin ventanas no hay nada que tape ni motivo para ocultarlo.
pub fn active_is_empty(workspaces: &[crate::widgets::WorkspaceInfo]) -> bool {
    workspaces.iter().any(|w| w.active && w.empty)
}

pub fn workspaces_geometry(workspaces: &[crate::widgets::WorkspaceInfo], render_scale: f32) -> f32 {
    let n = slot_count(workspaces) as f32;
    ((n - 1.0) * WS_SLOT + WS_ACTIVE) * render_scale
}

/// Escala de los puntos cuando los dibuja la ISLA. En el split, el largo disponible
/// es el natural por el split, así que en vez de dejar que la cápsula recorte el
/// indicador (el wipe) el widget se dibuja ESCALADO: los puntos y la separación
/// entran exactos en el rectángulo. En la barra (`compact = false`) siempre es 1.0.
///
/// El clamp es el que garantiza que el contenido entre: `natural * escala <= avail`.
pub(crate) fn ws_compact_scale(compact: bool, natural: f32, avail: f32) -> f32 {
    if compact && natural > 0.0 {
        (avail / natural).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// Relleno de la pastilla del HUD alrededor del indicador.
const WS_FLASH_PAD: f32 = 26.0;

/// Caja LÓGICA de la pastilla del HUD de workspaces: el rectángulo que el reparto
/// del dock le da al widget (`hit_layout`) más el relleno, simétrico.
///
/// El HUD no tiene reparto propio: su superficie mide y se ancla igual que la del
/// dock, así que el indicador cae donde estaba y sólo cambia la pastilla que lo
/// enmarca. El relleno no se escala aparte: es padding, el call site lo multiplica
/// por el `render_scale` como al rectángulo.
pub fn ws_flash_pill(r: &WidgetRect, is_vertical: bool) -> (f32, f32, f32, f32) {
    let h = WS_FLASH_PAD / 2.0;
    if is_vertical {
        (r.x, r.y - h, r.w, r.h + WS_FLASH_PAD)
    } else {
        (r.x - h, r.y, r.w + WS_FLASH_PAD, r.h)
    }
}

fn first_center(count: usize, bar_len: f32, bar_start: f32, render_scale: f32) -> f32 {
    let slot = WS_SLOT * render_scale;
    let content = (count.max(1) as f32 - 1.0) * slot + WS_ACTIVE * render_scale;
    bar_start + (bar_len - content) / 2.0 + WS_ACTIVE * render_scale / 2.0
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_workspaces_widget(
    pixmap: &mut Pixmap,
    workspaces: &[crate::widgets::WorkspaceInfo],
    marquee: &mut MarqueeState,
    advance: bool,
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    colors: &WidgetColors,
    is_vertical: bool,
) -> bool {
    if workspaces.is_empty() {
        return false;
    }
    let dot = WS_DOT * render_scale;
    let active = WS_ACTIVE * render_scale;
    let slot = WS_SLOT * render_scale;
    let count = slot_count(workspaces);
    let target = active_slot(workspaces);
    marquee.track_ws_target(target);
    if advance && marquee.ws_t < 1.0 {
        let now = std::time::Instant::now();
        let dt_ms = marquee
            .ws_last_tick
            .map(|prev| now.duration_since(prev).as_secs_f32() * 1000.0)
            .unwrap_or(1000.0 / 60.0);
        marquee.ws_last_tick = Some(now);
        marquee.ws_t = (marquee.ws_t + dt_ms / WS_ANIM_DURATION_MS).min(1.0);
        marquee.ws_current =
            marquee.ws_from + (marquee.ws_target - marquee.ws_from) * ease_out(marquee.ws_t);
        if marquee.ws_t >= 1.0 {
            marquee.ws_current = marquee.ws_target;
        }
    }
    let animating = marquee.ws_t < 1.0;

    let bar_len = if is_vertical { zh } else { zw };
    let bar_start = if is_vertical { zy } else { zx };
    let cross_center = if is_vertical {
        zx + zw / 2.0
    } else {
        zy + zh / 2.0
    };
    let first = first_center(count, bar_len, bar_start, render_scale);

    let (ar, ag, ab, _) = colors.accent;
    let accent = (ar, ag, ab);
    for (i, ws) in workspaces.iter().enumerate() {
        if i >= count {
            break;
        }
        let c = first + i as f32 * slot;
        let (cx, cy) = if is_vertical {
            (cross_center, c)
        } else {
            (c, cross_center)
        };
        let (rgb, alpha) = ws_tone(ws.urgent, 70, accent);
        draw_ws_shape(pixmap, cx, cy, dot, dot, rgb, alpha);
    }

    let c = first + (marquee.ws_current - 1.0).max(0.0) * slot;
    let (cx, cy, w, h) = if is_vertical {
        (cross_center, c, dot, active)
    } else {
        (c, cross_center, active, dot)
    };
    // ----- la pastilla del activo también se pinta si su workspace es urgente -----
    let activo_urgente = workspaces
        .iter()
        .position(|w| w.active)
        .and_then(|i| workspaces.get(i))
        .is_some_and(|w| w.urgent);
    let (rgb, alpha) = ws_tone(activo_urgente, 255, accent);
    draw_ws_shape(pixmap, cx, cy, w, h, rgb, alpha);
    animating
}

/// Rojo de la urgencia: el mismo que usa el OSD para el mute.
const WS_URGENT_RGB: (u8, u8, u8) = (255, 69, 58);

/// Tono de un punto de workspace: el acento del tema, y **rojo pleno** si el
/// workspace es urgente (algo pide atención: la señal que hoy no existía en el dock).
/// Es una función aparte para poder fijarla en un test sin renderizar.
fn ws_tone(urgent: bool, alpha: u8, accent: (u8, u8, u8)) -> ((u8, u8, u8), u8) {
    if urgent {
        (WS_URGENT_RGB, 255)
    } else {
        (accent, alpha)
    }
}

fn draw_ws_shape(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    rgb: (u8, u8, u8),
    alpha: u8,
) {
    let mut paint = Paint::default();
    paint.set_color_rgba8(rgb.0, rgb.1, rgb.2, alpha);
    paint.anti_alias = true;
    let path = rounded_rect_path(cx - w / 2.0, cy - h / 2.0, w, h, h.min(w) / 2.0);
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
}

// ----- slot hit test -----
/// Slot cuyo centro cae a `main` sobre la barra `[bar_start, bar_start + bar_len]`.
///
/// `scale` TIENE que ser la misma que usa el dibujo en esta superficie: el
/// layout del hit test (`hit_layout`) ya reparte con `hit_scale`, así que pasarle
/// `1.0` corría los slots decenas de px y con `widget_scale != 1` y ≥6 workspaces
/// el click en los extremos no hacía nada (o caía en el vecino). Es el mismo
/// contrato que la trampa 10, un nivel más adentro.
fn slot_at(count: usize, bar_len: f32, bar_start: f32, main: f32, scale: f32) -> Option<usize> {
    let first = first_center(count, bar_len, bar_start, scale);
    let half = WS_SLOT * scale / 2.0;
    (0..count).find(|i| (main - (first + *i as f32 * WS_SLOT * scale)).abs() <= half)
}

/// Reparto del hit test del dock. La escala sale de las settings y NO del call
/// site: es lo único que hace que el guard
/// `el_hit_test_toma_la_escala_de_las_settings` falle si alguien vuelve a clavar
/// un `1.0` acá adentro (con la escala pelada en una constante, el test pasa
/// igual y el bug vuelve sin que nadie se entere).
fn workspace_slot_at(
    settings: &crate::config::DockSettings,
    bar_len: f32,
    bar_start: f32,
    main: f32,
    count: usize,
) -> Option<usize> {
    slot_at(count, bar_len, bar_start, main, hit_scale(settings))
}

pub fn workspace_dot_hit(
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    x: f64,
    y: f64,
) -> Option<i32> {
    let workspaces = &widgets.workspaces;
    if workspaces.is_empty() {
        return None;
    }
    let is_vertical = dock.is_vertical();
    let rects = hit_layout(dock, widgets, tray_count);
    let r = rects
        .into_iter()
        .find(|r| r.kind == crate::config::WidgetKind::Workspaces)?;
    let (bar_len, bar_start, main) = if is_vertical {
        (r.h, r.y, y as f32)
    } else {
        (r.w, r.x, x as f32)
    };
    let count = slot_count(workspaces);
    let i = workspace_slot_at(&dock.config.settings, bar_len, bar_start, main, count)?;
    workspaces.get(i).map(|ws| ws.id)
}

#[cfg(test)]
mod ws_tone_tests {
    use super::*;

    /// Rojo pleno si el workspace es urgente (la señal de "algo te reclama"), y el
    /// acento del tema con su alfa si no. Es lo que ve el usuario en el punto, así que
    /// el contrato es el color, no el trazo.
    #[test]
    fn el_urgente_va_en_rojo_y_el_resto_en_el_acento() {
        let acento = (200, 180, 170);
        assert_eq!(ws_tone(true, 70, acento), (WS_URGENT_RGB, 255));
        assert_eq!(ws_tone(false, 70, acento), (acento, 70));
        assert_eq!(ws_tone(false, 255, acento), (acento, 255));
        assert_ne!(WS_URGENT_RGB, acento);
    }
}

#[cfg(test)]
mod workspace_hit_tests {
    use super::*;

    fn ws(id: i32, active: bool, empty: bool) -> crate::widgets::WorkspaceInfo {
        crate::widgets::WorkspaceInfo {
            id,
            active,
            empty,
            urgent: false,
            output: String::new(),
        }
    }

    #[test]
    fn solo_el_activo_vacio_cuenta() {
        // activo vacío -> fijo
        assert!(active_is_empty(&[ws(1, false, true), ws(2, true, true)]));
        // activo con ventanas -> autohide normal (aunque otro esté vacío)
        assert!(!active_is_empty(&[ws(1, false, true), ws(2, true, false)]));
        assert!(!active_is_empty(&[ws(1, true, false), ws(2, false, true)]));
        // sin estado (lista vacía) -> no fuerza nada
        assert!(!active_is_empty(&[]));
    }

    // ----- escala real reportada en AGENTS.md trampa 10; con 1.0 el bug es
    // invisible, así que el test tiene que correr con ésta. -----
    const SCALE: f32 = 1.2166;
    const BAR_LEN: f32 = 260.0;
    const BAR_START: f32 = 100.0;

    /// Centro del slot `i` según el dibujo (`draw_workspaces_widget`).
    fn centro_dibujado(count: usize, i: usize, scale: f32) -> f32 {
        first_center(count, BAR_LEN, BAR_START, scale) + i as f32 * WS_SLOT * scale
    }

    #[test]
    fn el_click_cae_en_el_slot_que_el_dibujo_ubica_ahi() {
        let count = 8;
        for i in 0..count {
            let x = centro_dibujado(count, i, SCALE);
            assert_eq!(
                slot_at(count, BAR_LEN, BAR_START, x, SCALE),
                Some(i),
                "click en el centro dibujado del slot {i} ({x:.1})"
            );
        }
    }

    #[test]
    fn con_escala_1_los_extremos_erran_ese_era_el_bug() {
        let count = 8;
        let ultimo = count - 1;
        let bien = centro_dibujado(count, ultimo, SCALE);
        let mal = centro_dibujado(count, ultimo, 1.0);
        assert!(
            bien - mal > 10.0,
            "el slot {ultimo} debería correrse decenas de px: bien {bien:.1}, mal {mal:.1}"
        );
        // ----- y con 1.0 el click en el extremo dibujado no da ningún slot -----
        assert_eq!(slot_at(count, BAR_LEN, BAR_START, bien, 1.0), None);
    }

    #[test]
    fn la_tolerancia_no_solapa_slots_vecinos() {
        let count = 6;
        for i in 0..count {
            let x = centro_dibujado(count, i, SCALE);
            let hits: Vec<_> = (0..count)
                .filter(|j| {
                    let c = centro_dibujado(count, *j, SCALE);
                    (x - c).abs() <= WS_SLOT * SCALE / 2.0
                })
                .collect();
            assert_eq!(
                hits,
                vec![i],
                "el slot {i} no debe robarle el click al vecino"
            );
        }
    }

    /// El HUD no tiene reparto propio: su pastilla sale del rectángulo que el
    /// reparto del dock le da al widget de Workspaces. El bug era un panel del
    /// largo del indicador centrado con `dock_align`: con zonas desparejas
    /// (izquierda cargada, tray a la derecha) el indicador NO está centrado en el
    /// dock, así que el punto saltaba de lugar al ocultarse el dock.
    #[test]
    fn la_pastilla_del_hud_sale_del_rectangulo_del_dock_no_de_un_panel_centrado() {
        use crate::config::{WidgetKind, WidgetPlacement, WidgetSlot};

        fn snapshot(workspaces: Vec<crate::widgets::WorkspaceInfo>) -> WidgetSnapshot {
            WidgetSnapshot {
                time: "11:11".into(),
                date: "11-Sept".into(),
                battery: None,
                media: None,
                bluetooth: None,
                workspaces,
                cpu: None,
                ram: None,
                ram_gb: None,
                volume: None,
                mic: None,
                recording: None,
                network: crate::widgets::NetworkInfo {
                    label: "wifi".into(),
                    online: true,
                },
                kblayout: crate::widgets::KbLayout { short: "EN".into() },
                custom_texts: Vec::new(),
                custom_last_polls: Vec::new(),
            }
        }

        let p = |kind, slot| WidgetPlacement { kind, slot };
        let settings = crate::config::DockSettings {
            widget_scale: SCALE,
            widgets: vec![
                p(WidgetKind::Clock, WidgetSlot::Left),
                p(WidgetKind::Battery, WidgetSlot::Left),
                p(WidgetKind::Volume, WidgetSlot::Left),
                p(WidgetKind::Workspaces, WidgetSlot::Middle),
                p(WidgetKind::Tray, WidgetSlot::Right),
            ],
            ..Default::default()
        };
        let widgets = snapshot((0..6).map(|i| ws(i + 1, i == 1, false)).collect());
        const BAR: f32 = 700.0;

        for is_vertical in [false, true] {
            let (w, h) = if is_vertical {
                (26.0, BAR)
            } else {
                (BAR, 26.0)
            };
            let rects =
                super::layout::layout_widgets(&settings, &widgets, 2, is_vertical, w, h, SCALE);
            let r = rects
                .iter()
                .find(|r| r.kind == WidgetKind::Workspaces)
                .expect("el widget esta colocado");
            let (main0, main_len) = if is_vertical { (r.y, r.h) } else { (r.x, r.w) };
            let bar_len = if is_vertical { h } else { w };
            // ----- el caso del test tiene que ser desparejo, si no no prueba nada -----
            assert!(
                (main0 + main_len / 2.0 - bar_len / 2.0).abs() > 10.0,
                "el reparto tiene que dejar el indicador lejos del centro: {:.1} vs {:.1}",
                main0 + main_len / 2.0,
                bar_len / 2.0
            );
            // ----- la pastilla lo enmarca simétricamente y cruza todo el grosor -----
            let (px, py, pw, ph) = ws_flash_pill(r, is_vertical);
            let (p0, p_len) = if is_vertical { (py, ph) } else { (px, pw) };
            assert!((p0 - (main0 - WS_FLASH_PAD / 2.0)).abs() < 0.01);
            assert!((p0 + p_len - (main0 + main_len + WS_FLASH_PAD / 2.0)).abs() < 0.01);
            let cross = if is_vertical { (px, pw) } else { (py, ph) };
            let rect_cross = if is_vertical { (r.x, r.w) } else { (r.y, r.h) };
            assert_eq!(cross, rect_cross);
        }
    }

    /// El guard de verdad: mira el CALL SITE, no el helper. Si
    /// `workspace_slot_at` vuelve a clavar `1.0` en vez de leer `hit_scale`, esto
    /// falla. Los otros tres tests pasan igual en ese caso, porque `slot_at`
    /// recibe la escala como parámetro.
    #[test]
    fn el_hit_test_toma_la_escala_de_las_settings() {
        let count = 8;
        let ultimo = count - 1;
        let x = centro_dibujado(count, ultimo, SCALE);

        let con_escala = crate::config::DockSettings {
            widget_scale: SCALE,
            ..Default::default()
        };
        assert_eq!(
            workspace_slot_at(&con_escala, BAR_LEN, BAR_START, x, count),
            Some(ultimo),
            "con widget_scale = {SCALE} el extremo dibujado tiene que responder"
        );

        // ----- el mismo x, con la escala del default, tiene que errar: si esto
        // da `Some(ultimo)` es que la escala no está entrando al reparto. -----
        let sin_escala = crate::config::DockSettings::default();
        assert_eq!(
            workspace_slot_at(&sin_escala, BAR_LEN, BAR_START, x, count),
            None,
            "con widget_scale = 1.0 el mismo x NO debería dar el extremo"
        );
        assert_ne!(hit_scale(&con_escala), hit_scale(&sin_escala));
    }

    /// El indicador de la isla se ESCALA con el split en vez de dejarse recortar por
    /// la cápsula (el morph). El clamp es el contrato: el contenido tiene que entrar
    /// (`natural * escala <= disponible`), y en la barra nunca se escala.
    #[test]
    fn el_indicador_de_la_isla_se_escala_y_entra() {
        // ----- en la barra (no compacto) siempre 1.0, aunque sobre lugar -----
        assert_eq!(ws_compact_scale(false, 100.0, 50.0), 1.0);
        assert_eq!(ws_compact_scale(false, 100.0, 250.0), 1.0);
        // ----- en la isla, el cociente disponible/natural -----
        assert_eq!(ws_compact_scale(true, 100.0, 100.0), 1.0);
        assert!((ws_compact_scale(true, 100.0, 50.0) - 0.5).abs() < 1e-6);
        assert!((ws_compact_scale(true, 100.0, 7.0) - 0.07).abs() < 1e-6);
        // ----- nunca agranda y el 0 disponible da 0 (el indicador ya no está) -----
        assert_eq!(ws_compact_scale(true, 100.0, 250.0), 1.0);
        assert_eq!(ws_compact_scale(true, 100.0, 0.0), 0.0);
        // ----- sin workspaces (`natural = 0`) no se divide por cero -----
        assert_eq!(ws_compact_scale(true, 0.0, 50.0), 1.0);
        // ----- y con la escala el contenido ENTRA en el rectángulo -----
        for avail in [0.0, 3.0, 40.0, 99.0, 100.0, 250.0] {
            let sc = ws_compact_scale(true, 100.0, avail);
            assert!(
                100.0 * sc <= avail + 1e-3,
                "avail={avail} escala={sc}: el contenido se sale"
            );
        }
    }
}
