use super::*;

// ----- la tabla de widgets vive en la raiz del crate (ver `src/widget.rs`) -----
use crate::config::WidgetKind;
use crate::widget::{Canvas, Ctx, spec_for};

pub(crate) struct WidgetRect {
    pub(crate) kind: crate::config::WidgetKind,
    pub(crate) slot: crate::config::WidgetSlot,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) w: f32,
    pub(crate) h: f32,
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
        kind,
        widgets,
        settings,
        render_scale,
        is_vertical,
        cross_len,
        tray_count,
        // ----- mide para la barra: la isla arma su propio `Ctx` con `compact` -----
        compact: false,
    };
    let Some(spec) = spec_for(kind) else {
        // ----- si el enum crece sin entrada en la tabla, `WIDGETS` deja de estar
        // completo. Antes lo garantizaba la exhaustividad del `match`; ahora lo
        // garantiza el test `la_tabla_cubre_todas_las_variantes_del_enum` -----
        unreachable!("{kind:?} no esta en WIDGETS (src/widget.rs)")
    };
    (spec.natural_len)(&cx)
}

pub(crate) fn text_widget_len(
    label: &str,
    is_vertical: bool,
    render_scale: f32,
    text_px: f32,
) -> f32 {
    let icon_side = 12.0 * render_scale;
    if is_vertical {
        icon_side + 5.0 * render_scale + text_width_estimate_render(label, text_px)
    } else {
        icon_side + 6.0 * render_scale + text_width_estimate_render(label, text_px)
    }
}
pub(crate) fn percentage_widget_len(
    pct: Option<u8>,
    is_vertical: bool,
    render_scale: f32,
    text_px: f32,
) -> f32 {
    let Some(pct) = pct else { return 0.0 };
    text_widget_len(&format!("{pct}%"), is_vertical, render_scale, text_px)
}
/// Sincroniza la clave del marquee con el título de Media: cuando el tema cambia, la
/// pista arranca de cero. Lo usan el dibujo de la barra y el de la isla, así que la
/// regla de "cuándo se resetea" es una sola.
pub(super) fn sync_marquee_key(marquee: &mut MarqueeState, widgets: &WidgetSnapshot) {
    let key = widgets
        .media
        .as_ref()
        .map(|m| m.title.clone())
        .unwrap_or_else(|| "Nothing is playing".to_string());
    if marquee.key != key {
        marquee.key = key;
        marquee.title = MarqueeLane::default();
    }
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

    sync_marquee_key(marquee, widgets);
    let is_vertical = dock.is_vertical();
    let tray_count = tray.len();
    let cross_len = if is_vertical { w } else { h };
    let mut animating = false;
    for r in layout_widgets(s, widgets, tray_count, is_vertical, w, h, render_scale) {
        // ----- el `kind` viaja en el contexto para que la entrada genérica
        // `Custom` sepa qué texto en caché tiene que medir y dibujar -----
        let cx = Ctx {
            kind: r.kind,
            widgets,
            settings: s,
            render_scale,
            is_vertical,
            cross_len,
            tray_count,
            // ----- la barra va completa: el modo compacto es de la isla -----
            compact: false,
        };
        // ----- la tabla es la unica lista: si un widget del enum no esta ahi,
        // `WIDGETS` quedo incompleto y conviene que paniquee con el nombre -----
        let Some(spec) = spec_for(r.kind) else {
            unreachable!("{:?} no esta en WIDGETS (src/widget.rs)", r.kind)
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

/// Dibuja UN widget centrado en `area` (píxeles físicos) y recortado a lo que entre.
///
/// Lo usa la isla compacta: así el estado mínimo muestra el MISMO dibujo del widget
/// —mismo icono, mismo número, misma paleta— y no una segunda versión que se pueda
/// desincronizar (trampa 12). El `WidgetRect` se arma acá con la medida que pide el
/// propio widget (`natural_len`), centrada en la cápsula.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_one_widget(
    pixmap: &mut Pixmap,
    dock: &Dock,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    tray: &[crate::tray::TrayIcon],
    marquee: &mut MarqueeState,
    kind: crate::config::WidgetKind,
    area: (f32, f32, f32, f32),
    render_scale: f32,
) -> bool {
    let s = &dock.config.settings;
    let render_scale = render_scale * s.widget_scale;
    let palette = widget_palette(s);
    let colors = WidgetColors {
        accent: palette.accent,
        text_rgb: palette.text_rgb,
        text_color: &palette.text_color,
    };
    let is_vertical = dock.is_vertical();
    let tray_count = tray.len();
    let (ax, ay, aw, ah) = area;
    let ctx = Ctx {
        kind,
        widgets,
        settings: s,
        render_scale,
        is_vertical,
        cross_len: if is_vertical { aw } else { ah },
        tray_count,
        // ----- el dibujo de una sola actividad en la isla es SIEMPRE compacto -----
        compact: true,
    };
    let Some(spec) = spec_for(kind) else {
        return false;
    };
    // ----- largo que pide el widget, recortado a la cápsula -----
    let natural = (spec.natural_len)(&ctx);
    let rect = if is_vertical {
        let len = natural.min(ah);
        WidgetRect {
            kind,
            slot: crate::config::WidgetSlot::Middle,
            x: ax,
            y: ay + (ah - len) / 2.0,
            w: aw,
            h: len,
        }
    } else {
        let len = natural.min(aw);
        WidgetRect {
            kind,
            slot: crate::config::WidgetSlot::Middle,
            x: ax + (aw - len) / 2.0,
            y: ay,
            w: len,
            h: ah,
        }
    };
    let mut canvas = Canvas {
        pixmap,
        text_cache,
        icon_cache,
        marquee,
        tray,
        colors: &colors,
        // ----- la isla NO se resalta al hover como el dock: el puntero sobre ella
        // dispara el reveal, no un hover de widget -----
        hovered: None,
        // ----- el marquee de Media (el único que anima acá) es por tiempo: `marquee_step`
        // mide con su propio reloj, así que avanzar en cada redibujado es correcto y no
        // adelanta de más (el tick de 33 ms lo agenda el que dibuja) -----
        advance: true,
        // ----- la animación de Workspaces SÍ avanza en la isla (el split la muestra):
        // con `false` los puntos pedían frames para siempre y el tick no terminaba
        // nunca — medido: 74% de un core en reposo, un bucle de 30 fps -----
        advance_ws: true,
        // ----- el largo de la BARRA, no el del recorte: el widget decide con él si
        // le entra el texto largo (Media pide 65 px en una barra ancha y 22 en una
        // angosta). Pasándole el largo de la isla se creería en una barra angosta y
        // dejaría el hueco del texto vacío al lado de la carátula -----
        bar_len: if dock.is_vertical() {
            dock.base_size().1 as f32 * render_scale / s.widget_scale
        } else {
            dock.base_size().0 as f32 * render_scale / s.widget_scale
        },
    };
    (spec.draw)(&mut canvas, &rect, &ctx)
}

/// Qué va a mostrar la isla compacta y cuánto mide: las actividades con su largo (en
/// orden, pegadas a lo largo de la cápsula) y el largo total del blob.
///
/// Es la ÚNICA cuenta del estado compacto —la usan `draw_island` para dibujar y el
/// reveal para saber hasta dónde encogerse—, así que el fondo, el recorte y el
/// contenido no se pueden despegar (trampa 10).
#[derive(Default)]
pub(super) struct IslandPlan {
    pub(super) items: Vec<(crate::config::WidgetKind, f32)>,
    /// Largo del blob (el piso de la animación). 0 = no hay isla.
    pub(super) compact: f32,
    /// Relleno en cada extremo del blob y separación entre actividades. Es el radio
    /// con el que el borde se curva: adentro de esa franja el blob se angosta, así
    /// que el contenido tiene que arrancar después (si no, la máscara le corta el
    /// borde — se veía en la batería, que es el icono más ancho).
    pub(super) pad: f32,
    pub(super) gap: f32,
}

/// Separación entre dos actividades pegadas en la isla.
const ISLAND_GAP: f32 = 5.0;

/// Arma el plan: la medida natural de cada actividad de `island_activities`, el
/// relleno de los extremos (el radio) y el techo del dock (lo que no entre se cae).
///
/// El piso no es por actividad: la isla crece para que entren como se dibujan en la
/// barra (Media ~104 px con su carátula y su texto, el volumen ~28, la batería ~29),
/// porque con un largo fijo al título de Media le cortaba las letras a la mitad.
/// Cuánto hay que correr el blob de la isla (eje largo, píxeles LÓGICOS) para que
/// quede centrado donde el dock tiene el **indicador de workspaces**. Es lo que hace
/// que al revelarse el dock el indicador no salte: la isla ya se movió a su lugar.
///
/// Sale del MISMO reparto que el dock (`layout_widgets`), así que no hay una segunda
/// cuenta de dónde vive el widget (trampa 10). 0 = ya está centrado, o el widget no
/// está colocado.
pub(super) fn island_ws_shift(dock: &Dock, widgets: &WidgetSnapshot, tray_count: usize) -> f32 {
    let s = &dock.config.settings;
    let (bw, bh) = dock.base_size();
    let (w, h) = (bw as f32, bh as f32);
    let vertical = dock.is_vertical();
    let Some(r) = layout_widgets(s, widgets, tray_count, vertical, w, h, 1.0)
        .into_iter()
        .find(|r| r.kind == crate::config::WidgetKind::Workspaces)
    else {
        return 0.0;
    };
    let (centro_widget, centro_barra) = if vertical {
        (r.y + r.h / 2.0, h / 2.0)
    } else {
        (r.x + r.w / 2.0, w / 2.0)
    };
    centro_widget - centro_barra
}

pub(super) fn island_plan(
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    render_scale: f32,
    ws_split: f32,
) -> IslandPlan {
    let s = &dock.config.settings;
    let (base_w, base_h) = dock.base_size();
    let (w, h) = (base_w as f32 * render_scale, base_h as f32 * render_scale);
    let is_vertical = dock.is_vertical();
    let (cross, max_len) = if is_vertical { (w, h) } else { (h, w) };
    // ----- el mismo radio que va a usar la máscara (`edge_rounded_rect_path` lo
    // clampa a la mitad del eje corto): adentro de esa franja el blob se angosta -----
    let mut plan = IslandPlan {
        pad: (s.corner_radius * render_scale).min(cross / 2.0),
        gap: ISLAND_GAP * render_scale,
        ..Default::default()
    };
    // ----- el indicador de workspaces (si está) es el que lleva el split -----
    let es_ws = |k: crate::config::WidgetKind| k == crate::config::WidgetKind::Workspaces;
    let mut prev_ws = false;
    for kind in super::island_activities(widgets, ws_split) {
        let ctx = Ctx {
            kind,
            widgets,
            settings: s,
            render_scale: render_scale * s.widget_scale,
            is_vertical,
            cross_len: cross,
            tray_count,
            // ----- la MEDIDA tiene que salir del mismo modo que el dibujo: si el reloj
            // se dibuja sin la fecha, el ancho reservado tampoco la puede contar
            // (trampa 12) -----
            compact: true,
        };
        let len = spec_for(kind)
            .map(|spec| (spec.natural_len)(&ctx))
            .unwrap_or(0.0);
        // ----- el techo es el dock entero y se va sumando (con el relleno y el gap):
        // si una actividad no entra, se cae ella y las que seguían (van por prioridad) -----
        let extra = if plan.items.is_empty() {
            2.0 * plan.pad
        } else if es_ws(kind) || prev_ws {
            // ----- los dos gaps que abren alrededor del indicador también crecen con
            // el split: en 0 no aportan nada y la isla no da un salto al abrirse -----
            plan.gap * ws_split
        } else {
            plan.gap
        };
        let len = if es_ws(kind) { len * ws_split } else { len };
        if plan.compact + extra + len > max_len {
            break;
        }
        plan.items.push((kind, len));
        plan.compact += extra + len;
        prev_ws = es_ws(kind);
    }
    // ----- el piso: un blob, aunque las actividades midan menos. Sin actividades NO
    // se aplica: `compact = 0` es "no hay isla" y el dock colapsa a nada (si no, el
    // reveal terminaría en un blob que `draw_island` no dibuja) -----
    if !plan.items.is_empty() {
        plan.compact = plan.compact.max(cross * super::ISLAND_COMPACT).min(max_len);
    }
    plan
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
    // ----- en una barra vertical el ancho es el grosor (~25px) y la pastilla
    // con el SSID se dibuja horizontal: no entra, se veía recortada. La pastilla
    // es una comodidad del modo horizontal; en vertical el ícono queda solo.
    // ponytail: una pastilla rotada pediría 1.5*font (~22) + padding de alto, que
    // tampoco entra; haría falta encoger la letra sólo para el hover.
    if dock.is_vertical() {
        return;
    }
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
    let fs = super::widget_text_px(
        &dock.config.settings,
        crate::config::WidgetKind::Network,
        render_scale,
    );
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
    // ----- solo lo usa el test de aca abajo: a nivel de modulo el build sin
    // tests lo marcaria `unused` (y el auto-fix lo borraria, rompiendo el test) -----
    use crate::widget::WIDGETS;
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
            mic: None,
            recording: None,
            network: NetworkInfo {
                label: "wifi".into(),
                online: true,
            },
            kblayout: KbLayout { short: "EN".into() },
            custom_texts: Vec::new(),
            custom_last_polls: Vec::new(),
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

    /// El ajuste de letra mueve la medida: con `font_scale = 2.0` el ancho del
    /// texto es el doble que con el default, porque `len_*` usa `widget_text_px`
    /// (la misma base 8.5 que el dibujo). Si alguien vuelve a hardcodear un
    /// tamaño en un `len_*`, este test se corre.
    #[test]
    fn el_factor_de_letra_mueve_la_medida() {
        let mut s = settings();
        s.font_scale = 2.0;
        let w = snapshot();
        let tp = crate::render::widget_text_px(&s, WidgetKind::KbdLayout, SCALE_DEL_BUG);
        assert!((tp - 2.0 * 8.5 * SCALE_DEL_BUG).abs() < 0.001);
        let esperado = text_width_estimate_render(&w.kblayout.short, tp) + 10.0 * SCALE_DEL_BUG;
        let r = rect(&s, WidgetKind::KbdLayout, SCALE_DEL_BUG);
        assert!(
            (r.w - esperado).abs() < 0.01,
            "KbdLayout con letra x2: el reparto reservo {} y tocaba {}",
            r.w,
            esperado
        );
    }

    /// La escala de letra por widget mueve SÓLO la medida de ese widget: es el
    /// caso de uso del editor del panel (el reloj más grande sin tocar el resto).
    #[test]
    fn la_escala_del_widget_mueve_solo_su_medida() {
        let base = settings();
        let mut grande = base.clone();
        grande.set_widget_options(
            WidgetKind::Clock,
            crate::config::WidgetOptions {
                font_scale: Some(1.5),
                ..Default::default()
            },
        );
        let antes = rect(&base, WidgetKind::Clock, SCALE_DEL_BUG);
        let despues = rect(&grande, WidgetKind::Clock, SCALE_DEL_BUG);
        assert!(
            despues.w > antes.w + 1.0,
            "el reloj no creció: {} contra {}",
            despues.w,
            antes.w
        );
        // ----- el vecino mide exactamente lo mismo -----
        let vecino_antes = rect(&base, WidgetKind::Volume, SCALE_DEL_BUG);
        let vecino_despues = rect(&grande, WidgetKind::Volume, SCALE_DEL_BUG);
        assert!(
            (vecino_despues.w - vecino_antes.w).abs() < 0.01,
            "subir la letra del reloj movió el volumen: {} contra {}",
            vecino_despues.w,
            vecino_antes.w
        );
    }

    /// `WIDGETS` tiene que cubrir TODAS las variantes fijas del enum y la entrada
    /// genérica tiene que cubrir CUALQUIER payload `Custom`. La lista de aca
    /// abajo esta escrita A MANO a proposito: es el unico testigo independiente
    /// que queda. Antes lo garantizaba la exhaustividad del `match` de
    /// `widget_label`, y ese `match` desaparecio cuando la etiqueta paso a ser un
    /// campo de la tabla (paso 2). Si el enum crece y no se agrega ni aca ni en
    /// `WIDGETS`, el widget queda muerto: no rompe nada, pero no se puede poner
    /// en la barra.
    #[test]
    fn la_tabla_cubre_todas_las_variantes_del_enum() {
        let s = settings();
        let w = snapshot();
        for kind in [
            WidgetKind::Clock,
            WidgetKind::Battery,
            WidgetKind::Media,
            WidgetKind::PowerMenu,
            WidgetKind::Bluetooth,
            WidgetKind::Tray,
            WidgetKind::Workspaces,
            WidgetKind::Cpu,
            WidgetKind::Ram,
            WidgetKind::Network,
            WidgetKind::Volume,
            WidgetKind::KbdLayout,
        ] {
            assert!(spec_for(kind).is_some(), "{kind:?} falta en WIDGETS");
            // Y medirlo no puede paniquear.
            let _ = widget_natural_len(kind, &s, &w, 1, false, CROSS, SCALE_DEL_BUG);
        }
        // ----- todos los payloads usan la misma entrada genérica -----
        let generica = spec_for(WidgetKind::Custom(0)).expect("falta la entrada Custom");
        for index in [0, 1, 42, u16::MAX] {
            let kind = WidgetKind::Custom(index);
            let spec = spec_for(kind).expect("falta el payload Custom");
            assert!(
                std::ptr::eq(spec, generica),
                "{kind:?} no usa la entrada Custom"
            );
            let _ = widget_natural_len(kind, &s, &w, 1, false, CROSS, SCALE_DEL_BUG);
        }
        // ----- 14 fijos + la entrada `Custom`. Cambiar estos números es la señal de
        // que se agregó un widget: hay que tocar la tabla (y nada más: el orden y las
        // etiquetas de Ajustes salen de ella) -----
        assert_eq!(WIDGETS.len(), 15, "la tabla cambio de tamano");
        // ----- Ajustes ofrece los 14 fijos: el índice de un `Custom` se escribe a
        // mano en el JSON -----
        let orden = crate::widget::widget_kind_order();
        assert_eq!(orden.len(), 14, "Ajustes cambio de tamano");
        assert!(
            !orden
                .iter()
                .any(|kind| matches!(kind, WidgetKind::Custom(_))),
            "Ajustes no ofrece el widget genérico"
        );
        assert_eq!(
            crate::widget::widget_label(WidgetKind::Custom(7)),
            "Custom widget"
        );
    }
}
