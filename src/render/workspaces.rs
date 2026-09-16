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

/// Relleno del panel del HUD alrededor del indicador.
const WS_FLASH_PAD: f32 = 26.0;

/// Largo del panel del HUD de workspaces. La escala sale de las settings
/// (`widget_scale`, vía `hit_scale`) y NO del call site: el contenido del HUD se
/// dibuja con el MISMO factor que el widget del dock (el `output_scale` se cancela,
/// está en el panel y en el buffer), así que el panel tiene que medirse con él. Una
/// sola definición del panel: la usan el tamaño de la superficie y el hit test, que
/// si no se despegan dejan puntos inclickeables (trampa 1). El relleno no se escala:
/// es padding, no parte del indicador.
pub fn ws_flash_panel_len(
    workspaces: &[crate::widgets::WorkspaceInfo],
    settings: &crate::config::DockSettings,
) -> f32 {
    workspaces_geometry(workspaces, hit_scale(settings)) + WS_FLASH_PAD
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
    for i in 0..count {
        let c = first + i as f32 * slot;
        let (cx, cy) = if is_vertical {
            (cross_center, c)
        } else {
            (c, cross_center)
        };
        draw_ws_shape(pixmap, cx, cy, dot, dot, (ar, ag, ab), 70);
    }

    let c = first + (marquee.ws_current - 1.0).max(0.0) * slot;
    let (cx, cy, w, h) = if is_vertical {
        (cross_center, c, dot, active)
    } else {
        (c, cross_center, active, dot)
    };
    draw_ws_shape(pixmap, cx, cy, w, h, (ar, ag, ab), 255);
    animating
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

/// ----- hit test del HUD de workspaces -----
/// Mismo cálculo que `workspace_dot_hit`, pero sobre el panel del HUD: ahí los
/// puntos van centrados en el panel entero (zx=0, zw=panel_w), no en la
/// posición que ocupa el widget dentro del layout del dock. Coordenadas
/// lógicas, igual que el dibujo.
///
/// `settings` es de donde sale la escala, igual que en `workspace_slot_at`: si el
/// call site pudiera pasar un `1.0`, al crecer el panel con `widget_scale` el click
/// caería corrido y no habría nada que lo avise.
pub fn ws_flash_dot_hit(
    workspaces: &[crate::widgets::WorkspaceInfo],
    is_vertical: bool,
    panel_w: f32,
    panel_h: f32,
    settings: &crate::config::DockSettings,
    x: f64,
    y: f64,
) -> Option<i32> {
    if workspaces.is_empty() {
        return None;
    }
    let (bar_len, bar_start, main) = if is_vertical {
        (panel_h, 0.0, y as f32)
    } else {
        (panel_w, 0.0, x as f32)
    };
    let count = slot_count(workspaces);
    let i = slot_at(count, bar_len, bar_start, main, hit_scale(settings))?;
    workspaces.get(i).map(|ws| ws.id)
}

#[cfg(test)]
mod workspace_hit_tests {
    use super::*;

    fn ws(id: i32, active: bool, empty: bool) -> crate::widgets::WorkspaceInfo {
        crate::widgets::WorkspaceInfo {
            id,
            active,
            empty,
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

    /// El HUD se dibuja con el MISMO factor que el widget del dock, así que su panel
    /// crece con `widget_scale` y el hit test tiene que usar esa escala. Con el `1.0`
    /// de antes el panel quedaba chico y el punto cambiaba de tamaño al ocultarse el
    /// dock (el pedido era justamente que coincidan).
    #[test]
    fn el_panel_del_hud_crece_con_widget_scale_y_el_click_lo_sigue() {
        let count = 6;
        let con_escala = crate::config::DockSettings {
            widget_scale: SCALE,
            ..Default::default()
        };
        let sin_escala = crate::config::DockSettings::default();
        let ws: Vec<_> = (0..count)
            .map(|i| ws(i as i32 + 1, i == count - 1, false))
            .collect();
        let chico = ws_flash_panel_len(&ws, &sin_escala);
        let grande = ws_flash_panel_len(&ws, &con_escala);
        assert!(grande > chico, "el panel tiene que crecer con widget_scale");
        // ----- y crecer exactamente lo que crece el indicador: el relleno no se escala
        assert_eq!(
            grande - chico,
            workspaces_geometry(&ws, SCALE) - workspaces_geometry(&ws, 1.0)
        );
        for (s, scale) in [(&sin_escala, 1.0), (&con_escala, SCALE)] {
            let panel = ws_flash_panel_len(&ws, s);
            assert!(
                workspaces_geometry(&ws, scale) <= panel,
                "el contenido entra"
            );
            // ----- el centro DIBUJADO del último slot tiene que dar el último
            let x = first_center(count, panel, 0.0, scale) + (count - 1) as f32 * WS_SLOT * scale;
            assert_eq!(
                ws_flash_dot_hit(&ws, false, panel, 26.0, s, x as f64, 13.0),
                Some(count as i32),
                "click en el centro dibujado ({x:.1}) con widget_scale {scale}"
            );
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
}
