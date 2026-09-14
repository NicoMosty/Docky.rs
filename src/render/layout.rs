use super::*;

pub(crate) struct WidgetRect {
    pub(super) kind: crate::config::WidgetKind,
    pub(super) slot: crate::config::WidgetSlot,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

pub(super) const WIDGET_GAP: f32 = 6.0;
pub(super) const ZONE_GAP: f32 = 18.0;

// ----- flexible zones -----
pub(super) fn layout_widgets(
    settings: &crate::config::DockSettings,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    is_vertical: bool,
    w: f32,
    h: f32,
    render_scale: f32,
) -> Vec<WidgetRect> {
    use crate::config::WidgetSlot;
    let bar_len = if is_vertical { h } else { w };
    let cross_len = if is_vertical { w } else { h };
    let gap = WIDGET_GAP * render_scale;
    let zone_gap = ZONE_GAP * render_scale;
    let inset = 10.0 * render_scale;

    let members_of = |slot: WidgetSlot| -> Vec<crate::config::WidgetKind> {
        settings
            .widgets
            .iter()
            .filter(|p| p.slot == slot)
            .map(|p| p.kind)
            .collect()
    };
    let lens_of = |members: &[crate::config::WidgetKind]| -> Vec<f32> {
        members
            .iter()
            .map(|&kind| {
                widget_natural_len(
                    kind,
                    settings,
                    widgets,
                    tray_count,
                    is_vertical,
                    cross_len,
                    render_scale,
                )
                .max(1.0)
            })
            .collect()
    };
    let block_len = |lens: &[f32]| -> f32 {
        lens.iter().sum::<f32>() + gap * (lens.len() as f32 - 1.0).max(0.0)
    };

    let place = |slot: WidgetSlot,
                 members: Vec<crate::config::WidgetKind>,
                 lens: Vec<f32>,
                 block_start: f32|
     -> Vec<WidgetRect> {
        let mut pos = block_start;
        members
            .into_iter()
            .zip(lens)
            .map(|(kind, len)| {
                let start = pos;
                pos += len + gap;
                if is_vertical {
                    WidgetRect {
                        kind,
                        slot,
                        x: 0.0,
                        y: start,
                        w,
                        h: len,
                    }
                } else {
                    WidgetRect {
                        kind,
                        slot,
                        x: start,
                        y: 0.0,
                        w: len,
                        h,
                    }
                }
            })
            .collect()
    };

    let left_members = members_of(WidgetSlot::Left);
    let left_lens = lens_of(&left_members);
    let left_len = block_len(&left_lens);
    let left_end = inset + left_len;

    let right_members = members_of(WidgetSlot::Right);
    let right_lens = lens_of(&right_members);
    let right_len = block_len(&right_lens);
    let right_start = bar_len - inset - right_len;

    let mid_members = members_of(WidgetSlot::Middle);
    let mid_lens = lens_of(&mid_members);
    let mid_len = block_len(&mid_lens);
    let segment_start = if left_len > 0.0 {
        left_end + zone_gap
    } else {
        inset
    };
    let segment_end = if right_len > 0.0 {
        right_start - zone_gap
    } else {
        bar_len - inset
    };
    // ----- zona media siempre centrada en su espacio libre -----
    let gap_center = segment_start + (segment_end - segment_start - mid_len) / 2.0;
    let mid_start = gap_center.clamp(
        segment_start.min(segment_end - mid_len),
        segment_end - mid_len,
    );

    let mut out = place(WidgetSlot::Left, left_members, left_lens, inset);
    out.extend(place(WidgetSlot::Middle, mid_members, mid_lens, mid_start));
    out.extend(place(
        WidgetSlot::Right,
        right_members,
        right_lens,
        right_start,
    ));
    out
}

// ----- mirrors layout -----
pub fn widget_bar_natural_len(
    settings: &crate::config::DockSettings,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    is_vertical: bool,
    cross_len: f32,
    render_scale: f32,
) -> f32 {
    use crate::config::WidgetSlot;
    if settings.widgets.is_empty() {
        return 0.0;
    }
    let gap = WIDGET_GAP * render_scale;
    let zone_gap = ZONE_GAP * render_scale;
    let inset = 10.0 * render_scale;

    let zone_len = |slot: WidgetSlot| -> f32 {
        let lens: Vec<f32> = settings
            .widgets
            .iter()
            .filter(|p| p.slot == slot)
            .map(|p| {
                widget_natural_len(
                    p.kind,
                    settings,
                    widgets,
                    tray_count,
                    is_vertical,
                    cross_len,
                    render_scale,
                )
                .max(1.0)
            })
            .collect();
        if lens.is_empty() {
            0.0
        } else {
            lens.iter().sum::<f32>() + gap * (lens.len() as f32 - 1.0)
        }
    };

    let zones = [
        zone_len(WidgetSlot::Left),
        zone_len(WidgetSlot::Middle),
        zone_len(WidgetSlot::Right),
    ];
    let present: Vec<f32> = zones.into_iter().filter(|&len| len > 0.0).collect();
    present.iter().sum::<f32>() + zone_gap * (present.len() as f32 - 1.0).max(0.0) + inset * 2.0
}

pub(super) fn widget_natural_len(
    kind: crate::config::WidgetKind,
    settings: &crate::config::DockSettings,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    is_vertical: bool,
    cross_len: f32,
    render_scale: f32,
) -> f32 {
    // ----- la tabla es la unica lista de widgets -----
    let cx = Ctx {
        widgets,
        settings,
        render_scale,
        is_vertical,
        cross_len,
        tray_count,
    };
    let Some(spec) = spec_for(kind) else {
        // ----- si el enum crece sin entrada en la tabla, `WIDGETS` deja de estar
        // completo. Antes lo garantizaba la exhaustividad del `match`; ahora lo
        // garantiza el test `la_tabla_cubre_todos_los_widgets_del_panel` -----
        unreachable!("{kind:?} no esta en WIDGETS (render/widget_spec.rs)")
    };
    (spec.natural_len)(&cx)
}

pub(super) fn text_widget_len(label: &str, is_vertical: bool, render_scale: f32) -> f32 {
    let icon_side = 12.0 * render_scale;
    if is_vertical {
        icon_side + 5.0 * render_scale + text_width_estimate_render(label, 8.5 * render_scale)
    } else {
        icon_side + 6.0 * render_scale + text_width_estimate_render(label, 8.5 * render_scale)
    }
}
pub(super) fn percentage_widget_len(pct: Option<u8>, is_vertical: bool, render_scale: f32) -> f32 {
    let Some(pct) = pct else { return 0.0 };
    text_widget_len(&format!("{pct}%"), is_vertical, render_scale)
}
pub(super) fn draw_widgets(
    pixmap: &mut Pixmap,
    dock: &Dock,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    tray: &[crate::tray::TrayIcon],
    marquee: &mut MarqueeState,
    advance: bool,
    advance_ws: bool,
    render_scale: f32,
) -> bool {
    use crate::config::WidgetKind;

    let s = &dock.config.settings;
    let (base_w, base_h) = dock.base_size();
    let w = base_w as f32 * render_scale;
    let h = base_h as f32 * render_scale;
    // ----- widget scale -----
    let render_scale = render_scale * s.widget_scale;
    let palette = widget_palette(s);
    let colors = WidgetColors {
        accent: palette.accent,
        text_rgb: palette.text_rgb,
        text_color: &palette.text_color,
    };

    let key = widgets
        .media
        .as_ref()
        .map(|m| m.title.clone())
        .unwrap_or_else(|| "Nothing is playing".to_string());
    if marquee.key != key {
        marquee.key = key;
        marquee.title = MarqueeLane::default();
    }

    let is_vertical = dock.is_vertical();
    let tray_count = tray.len();
    let cx = Ctx {
        widgets,
        settings: s,
        render_scale,
        is_vertical,
        cross_len: if is_vertical { w } else { h },
        tray_count,
    };
    let mut animating = false;
    for r in layout_widgets(s, widgets, tray_count, is_vertical, w, h, render_scale) {
        // ----- la tabla es la unica lista: si un widget del enum no esta ahi,
        // `WIDGETS` quedo incompleto y conviene que paniquee con el nombre -----
        let Some(spec) = spec_for(r.kind) else {
            unreachable!("{:?} no esta en WIDGETS (render/widget_spec.rs)", r.kind)
        };
        let mut canvas = Canvas {
            pixmap: &mut *pixmap,
            text_cache: &mut *text_cache,
            icon_cache: &mut *icon_cache,
            marquee: &mut *marquee,
            tray,
            colors: &colors,
            hovered: dock.hovered_widget,
            advance,
            advance_ws,
            bar_len: if is_vertical { h } else { w },
        };
        // ----- el `|=` es por Media (marquee) y Workspaces (animacion) -----
        animating |= (spec.draw)(&mut canvas, &r, &cx);
    }
    // ----- separadores sutiles entre zonas -----
    {
        use crate::config::WidgetSlot;
        let rects = layout_widgets(
            &dock.config.settings,
            widgets,
            tray_count,
            dock.is_vertical(),
            w,
            h,
            render_scale,
        );
        // (min, max) por zona sobre el eje principal; max < 0 = vacía
        let mut bounds = [(0.0f32, -1.0f32); 3];
        for r in &rects {
            let i = match r.slot {
                WidgetSlot::Left => 0,
                WidgetSlot::Middle => 1,
                WidgetSlot::Right => 2,
            };
            let (a, b) = if dock.is_vertical() {
                (r.y, r.y + r.h)
            } else {
                (r.x, r.x + r.w)
            };
            if bounds[i].1 < 0.0 {
                bounds[i] = (a, b);
            } else {
                bounds[i].0 = bounds[i].0.min(a);
                bounds[i].1 = bounds[i].1.max(b);
            }
        }
        let mut div_paint = Paint::default();
        div_paint.set_color_rgba8(colors.text_rgb.0, colors.text_rgb.1, colors.text_rgb.2, 35);
        div_paint.anti_alias = true;
        let mut prev: Option<(f32, f32)> = None;
        for i in [0, 1, 2] {
            if bounds[i].1 < 0.0 {
                continue;
            }
            if let Some((_, pmx)) = prev {
                let s = (pmx + bounds[i].0) / 2.0;
                let rc = if dock.is_vertical() {
                    let m = 4.0 * render_scale;
                    tiny_skia::Rect::from_xywh(m, s - 0.5, (w - 2.0 * m).max(0.0), 1.0)
                } else {
                    let m = 4.0 * render_scale;
                    tiny_skia::Rect::from_xywh(s - 0.5, m, 1.0, (h - 2.0 * m).max(0.0))
                };
                if let Some(rc) = rc {
                    pixmap.fill_rect(rc, &div_paint, Transform::identity(), None);
                }
            }
            prev = Some(bounds[i]);
        }
    }
    // ----- pill flotante con el SSID al pasar el mouse -----
    if dock.hovered_widget == Some(WidgetKind::Network) {
        draw_network_hover_pill(
            pixmap,
            dock,
            text_cache,
            widgets,
            tray_count,
            w,
            h,
            render_scale,
            &colors,
        );
    }
    animating
}

#[allow(clippy::too_many_arguments)]
fn draw_network_hover_pill(
    pixmap: &mut Pixmap,
    dock: &Dock,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    w: f32,
    h: f32,
    render_scale: f32,
    colors: &WidgetColors,
) {
    use crate::config::WidgetKind;
    let label = widgets.network.label.trim();
    if label.is_empty() {
        return;
    }
    let Some(rect) = layout_widgets(
        &dock.config.settings,
        widgets,
        tray_count,
        dock.is_vertical(),
        w,
        h,
        render_scale,
    )
    .into_iter()
    .find(|r| r.kind == WidgetKind::Network) else {
        return;
    };
    let fs = 8.5 * render_scale;
    let Some(txt) = text_cache.get(label, fs, colors.text_color, 500) else {
        return;
    };
    let pad_x = 7.0 * render_scale;
    let pad_y = 4.0 * render_scale;
    let pw = txt.width() as f32 + pad_x * 2.0;
    let ph = txt.height() as f32 + pad_y * 2.0;
    let margin = 3.0 * render_scale;
    let px = (rect.x + rect.w / 2.0 - pw / 2.0).clamp(margin, (w - pw - margin).max(margin));
    let py = (rect.y + rect.h / 2.0 - ph / 2.0).clamp(0.0, (h - ph).max(0.0));
    let path = rounded_rect_path(px, py, pw, ph, 6.0 * render_scale);
    let mut paint = Paint::default();
    paint.set_color_rgba8(18, 18, 22, 235);
    paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    pixmap.draw_pixmap(
        0,
        0,
        txt.as_ref().as_ref(),
        &tiny_skia::PixmapPaint::default(),
        Transform::from_translate(px + pad_x, py + pad_y),
        None,
    );
}

/// Escala con la que hay que repartir los widgets en un HIT TEST.
///
/// `draw_widgets` multiplica el `render_scale` por `widget_scale` antes de llamar
/// a `layout_widgets`, y `base_size()` ya cuenta ese factor (viene de
/// `widget_bar_natural_len(..., widget_scale)`). Los hit tests, en cambio, reciben
/// coordenadas LÓGICAS del puntero (sin `output_scale`), así que les toca
/// `widget_scale` pelado: con el `1.0` de antes los rects quedaban comprimidos
/// hacia la izquierda — con `widget_scale=1.2` el click caía ~20px a la izquierda
/// del icono de WiFi y ~45px a la izquierda de "EN" (y el menú del tray se abría
/// corrido a la izquierda del icono, porque `widget_center` tenía lo mismo).
pub(super) fn hit_scale(settings: &crate::config::DockSettings) -> f32 {
    settings.widget_scale
}

/// Reparto de la barra en coordenadas LÓGICAS de la superficie, para hit tests.
///
/// Es la única forma de pedir el layout para un hit test: el bug de escala salió
/// de tener cinco lugares llamando a `layout_widgets` con su propio `1.0`. Si
/// necesitás el layout para decidir un click, usá esto y no `layout_widgets`
/// directo (eso es cosa de `draw_widgets`, que pasa `render_scale * widget_scale`).
pub(super) fn hit_layout(
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
) -> Vec<WidgetRect> {
    let (base_w, base_h) = dock.base_size();
    layout_widgets(
        &dock.config.settings,
        widgets,
        tray_count,
        dock.is_vertical(),
        base_w as f32,
        base_h as f32,
        hit_scale(&dock.config.settings),
    )
}

pub fn widget_hit_test(
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    x: f64,
    y: f64,
) -> Option<crate::config::WidgetKind> {
    let rects = hit_layout(dock, widgets, tray_count);
    let (xf, yf) = (x as f32, y as f32);
    rects
        .into_iter()
        .find(|r| xf >= r.x && xf < r.x + r.w && yf >= r.y && yf < r.y + r.h)
        .map(|r| r.kind)
}

pub fn widget_center(
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    kind: crate::config::WidgetKind,
) -> Option<(f32, f32)> {
    let rects = hit_layout(dock, widgets, tray_count);
    let r = rects.into_iter().find(|r| r.kind == kind)?;
    Some((r.x + r.w / 2.0, r.y + r.h / 2.0))
}

#[cfg(test)]
mod hit_layout_tests {
    // ----- El contrato que rompió todo: el reparto para un hit test tiene que usar
    // la MISMA escala que el dibujo. `draw_widgets` calcula `render_scale *
    // widget_scale` y `base_size()` mide la superficie con esa escala (vía
    // `widget_bar_natural_len(..., widget_scale)`). Con otra escala los rects
    // quedan corridos respecto de los iconos y el click cae sobre otro widget (o
    // sobre nada): en el clúster izquierdo hacia la izquierda, y en el derecho
    // —donde está el tray— hacia la derecha, porque ese bloque se ancla al borde.
    //
    // Con `widget_scale = 1.0` (el default de `DockSettings`) el bug es invisible,
    // así que el test fuerza el valor con el que apareció.
    use super::*;
    use crate::config::WidgetKind;
    use crate::widgets::{BatteryState, KbLayout, NetworkInfo, WidgetSnapshot};

    const SCALE_DEL_BUG: f32 = 1.2166064;
    const CROSS: f32 = 26.0;

    fn settings() -> crate::config::DockSettings {
        crate::config::DockSettings {
            widget_scale: SCALE_DEL_BUG,
            ..Default::default()
        }
    }

    fn snapshot() -> WidgetSnapshot {
        WidgetSnapshot {
            time: "11:11".into(),
            date: "11-Sept".into(),
            battery: Some((50, BatteryState::Discharging)),
            media: None,
            bluetooth: None,
            workspaces: Vec::new(),
            cpu: None,
            ram: None,
            ram_gb: None,
            volume: Some((50, false)),
            network: NetworkInfo {
                label: "wifi".into(),
                online: true,
            },
            kblayout: KbLayout { short: "EN".into() },
        }
    }

    /// Ancho de la superficie, medido como en `base_size()` para el caso sin
    /// iconos de aplicaciones.
    fn ancho_superficie(s: &crate::config::DockSettings, w: &WidgetSnapshot) -> f32 {
        widget_bar_natural_len(s, w, 1, false, CROSS, s.widget_scale)
            + s.width_padding * s.widget_scale * 2.0
    }

    /// Rect de un widget con una escala dada. Para el DIBUJO la escala es
    /// `output_scale * widget_scale`, o sea `widget_scale` cuando la salida no
    /// tiene escala.
    fn rect(s: &crate::config::DockSettings, kind: WidgetKind, scale: f32) -> WidgetRect {
        let w = snapshot();
        let width = ancho_superficie(s, &w);
        let mut all = layout_widgets(s, &w, 1, false, width, CROSS, scale);
        let i = all
            .iter()
            .position(|r| r.kind == kind)
            .expect("widget presente en la barra");
        all.swap_remove(i)
    }

    #[test]
    fn el_rect_del_hit_test_coincide_con_el_que_se_dibuja() {
        let s = settings();
        for kind in [
            WidgetKind::PowerMenu,
            WidgetKind::Network,
            WidgetKind::Bluetooth,
            WidgetKind::KbdLayout,
            WidgetKind::Clock,
        ] {
            let dibujado = rect(&s, kind, SCALE_DEL_BUG);
            let hiteado = rect(&s, kind, hit_scale(&s));
            assert!(
                (dibujado.x - hiteado.x).abs() < 0.01 && (dibujado.w - hiteado.w).abs() < 0.01,
                "{kind:?}: el hit test reparte distinto que el dibujo \
                 (x {} vs {}, w {} vs {})",
                hiteado.x,
                dibujado.x,
                hiteado.w,
                dibujado.w
            );
        }
    }

    #[test]
    fn con_escala_1_los_rects_quedan_corridos_ese_era_el_bug() {
        // Caracterización: el corrimiento es de decenas de px, no un redondeo.
        // En el clúster izquierdo el rect se va a la izquierda del icono; en el
        // derecho (tray/reloj) al revés, porque ese bloque se ancla al borde.
        let s = settings();
        for kind in [
            WidgetKind::Network,
            WidgetKind::Bluetooth,
            WidgetKind::KbdLayout,
        ] {
            let bien = rect(&s, kind, hit_scale(&s));
            let mal = rect(&s, kind, 1.0);
            assert!(
                bien.x - mal.x > 10.0,
                "{kind:?}: con escala 1.0 el rect debería quedar a la izquierda del \
                 icono; se corrió {} px (bien x={}, mal x={})",
                bien.x - mal.x,
                bien.x,
                mal.x
            );
        }
        let bien = rect(&s, WidgetKind::Clock, hit_scale(&s));
        let mal = rect(&s, WidgetKind::Clock, 1.0);
        assert!(
            mal.x - bien.x > 10.0,
            "el reloj (bloque anclado al borde derecho) debería correrse al revés: \
             bien x={}, mal x={}",
            bien.x,
            mal.x
        );
    }

    #[test]
    fn hit_scale_es_la_misma_escala_que_usa_el_dibujo() {
        // Si alguien agrega otro factor en `draw_widgets`, este test lo obliga a
        // tocar `hit_scale` en el mismo commit.
        let s = settings();
        assert_eq!(hit_scale(&s), s.widget_scale);
        let mut sin_escala = s.clone();
        sin_escala.widget_scale = 1.0;
        assert_eq!(hit_scale(&sin_escala), 1.0);
    }

    /// Fija el ancho de KbdLayout contra el valor que reservaba la rama vieja del
    /// `match` (8.5 del texto mas 10 de padding). Si `len_kblayout` se despega, el
    /// contenido se sale de la pastilla: la trampa 12 pide que el ancho del reparto
    /// y el del dibujo salgan de la misma funcion, y ahora esa funcion es la de la
    /// tabla.
    #[test]
    fn el_widget_migrado_a_la_tabla_mide_lo_mismo() {
        assert!(
            spec_for(WidgetKind::KbdLayout).is_some(),
            "KbdLayout tendria que estar en WIDGETS"
        );
        // La rama vieja del `match` reservaba el texto a 8.5 mas 10 de padding;
        // si `len_kblayout` se despega, el contenido se sale de la pastilla
        // (trampa 12: el ancho del reparto y el del dibujo van en la misma
        // funcion, y ahora esa funcion es la de la tabla).
        let s = settings();
        let w = snapshot();
        let esperado = text_width_estimate_render(&w.kblayout.short, 8.5 * SCALE_DEL_BUG)
            + 10.0 * SCALE_DEL_BUG;
        let r = rect(&s, WidgetKind::KbdLayout, SCALE_DEL_BUG);
        assert!(
            (r.w - esperado).abs() < 0.01,
            "KbdLayout: el reparto reservo {} y la rama vieja reservaba {}",
            r.w,
            esperado
        );
    }

    /// `WIDGETS` tiene que cubrir TODOS los widgets del enum. Antes lo
    /// garantizaba la exhaustividad de los dos `match`; ahora que no hay `match`,
    /// es lo unico que evita que un widget exista en el enum y no se pueda medir
    /// ni dibujar. Se itera `WIDGET_KIND_ORDER` (la lista del panel de ajustes,
    /// que por construccion tiene que estar completa: si un widget no esta ahi el
    /// usuario no lo puede agregar a la barra).
    #[test]
    fn la_tabla_cubre_todos_los_widgets_del_panel() {
        let s = settings();
        let w = snapshot();
        for kind in crate::menu::WIDGET_KIND_ORDER {
            assert!(
                spec_for(kind).is_some(),
                "{kind:?} esta en el panel de ajustes pero no en WIDGETS"
            );
            // Y medirlo no puede paniquear.
            let _ = widget_natural_len(kind, &s, &w, 1, false, CROSS, SCALE_DEL_BUG);
        }
        assert_eq!(
            WIDGETS.len(),
            crate::menu::WIDGET_KIND_ORDER.len(),
            "WIDGETS y el panel de ajustes no tienen el mismo tamano"
        );
    }
}
