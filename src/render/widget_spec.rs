//! La tabla de widgets: un solo lugar que sabe que widgets existen y con que se
//! mide y se dibuja cada uno.
//!
//! Reemplaza, de a uno, los `match kind` que hoy estan repartidos entre
//! `widget_natural_len` y el despacho de `draw_widgets`. Agregar un widget
//! costaba 16 sitios en 7 archivos (medido con `KbdLayout`, commit c458821);
//! la meta es que sea UNA entrada aca.
//!
//! El dibujo recibe el MISMO `WidgetRect` que produjo `layout_widgets`, asi que
//! el reparto y el dibujo no pueden usar escalas distintas. Eso es lo que las
//! trampas 10 y 12 de AGENTS.md piden y hoy sostienen a mano los tests.

use super::*;

/// Lo que un widget necesita para medirse Y para dibujarse.
///
/// Los campos se fueron agregando con cada migracion, no antes: `settings` lo
/// pide Media y `cross_len`/`tray_count` los pide Tray. Con un solo widget
/// migrado el struct nacia con 3 campos muertos y lo dijo el compilador
/// (`dead_code`), que es la senal de que el campo todavia no hacia falta.
/// `bar_len`, `colors` y `hovered` NO van aca porque no existen cuando se mide
/// (el reparto no tiene paleta ni largo de barra): viven en `Canvas`.
pub(super) struct Ctx<'a> {
    pub(super) widgets: &'a WidgetSnapshot,
    pub(super) settings: &'a crate::config::DockSettings,
    /// Ya viene multiplicada por `widget_scale`, igual que en el dibujo.
    pub(super) render_scale: f32,
    pub(super) is_vertical: bool,
    /// Largo del eje corto: `w` en horizontal, `h` en vertical.
    pub(super) cross_len: f32,
    pub(super) tray_count: usize,
}

/// Donde se pinta un frame. Se arma una vez por frame y lo reciben todos los
/// widgets que se dibujan.
pub(super) struct Canvas<'a, 'b> {
    pub(super) pixmap: &'a mut Pixmap,
    pub(super) text_cache: &'a mut TextCache,
    pub(super) icon_cache: &'a mut IconCache,
    pub(super) marquee: &'a mut MarqueeState,
    pub(super) tray: &'a [crate::tray::TrayIcon],
    pub(super) colors: &'b WidgetColors<'b>,
    /// Largo del eje principal: `w` en horizontal, `h` en vertical. Es de
    /// pintado y no de medicion: lo usa el marquee de Media.
    pub(super) bar_len: f32,
    /// Cual widget tiene el puntero encima, si alguno.
    pub(super) hovered: Option<crate::config::WidgetKind>,
    /// `true` si este frame toca avanzar el marquee.
    pub(super) advance: bool,
}

impl Canvas<'_, '_> {
    /// `true` si el puntero esta sobre `rect`. Antes cada rama del `match`
    /// calculaba este booleano a mano comparando su propio `kind`.
    pub(super) fn is_hovered(&self, rect: &WidgetRect) -> bool {
        self.hovered == Some(rect.kind)
    }
}

pub(super) struct WidgetSpec {
    pub(super) kind: crate::config::WidgetKind,
    /// Ancho que reserva el reparto. La llama `widget_natural_len`.
    pub(super) natural_len: fn(&Ctx) -> f32,
    /// Dibuja dentro del `rect` que le dio el reparto. Devuelve `true` si el
    /// widget animo este frame (hoy solo el marquee de Media).
    pub(super) draw: fn(&mut Canvas, &WidgetRect, &Ctx) -> bool,
}

/// ponytail: 6 de 12 migrados. El resto sigue en los dos `match` de
/// `layout.rs`, que tienen una rama `unreachable!()` para los que ya estan aca:
/// si un widget de esta tabla cayera hasta el `match`, es que el despacho de
/// arriba se rompio y conviene que paniquee con el nombre. Cuando esten los 12,
/// los dos `match` desaparecen y esto pasa a ser la unica lista -- y reemplaza
/// tambien `WIDGET_KIND_ORDER` y `widget_label`, que repiten los 12 variantes a
/// mano por tercera y cuarta vez.
pub(super) const WIDGETS: &[WidgetSpec] = &[
    WidgetSpec {
        kind: crate::config::WidgetKind::Clock,
        natural_len: len_clock,
        draw: draw_clock,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::Media,
        natural_len: len_media,
        draw: draw_media,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::Tray,
        natural_len: len_tray,
        draw: draw_tray,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::Network,
        natural_len: len_network,
        draw: draw_network,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::Volume,
        natural_len: len_volume,
        draw: draw_volume,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::KbdLayout,
        natural_len: len_kblayout,
        draw: draw_kblayout,
    },
];

pub(super) fn spec_for(kind: crate::config::WidgetKind) -> Option<&'static WidgetSpec> {
    WIDGETS.iter().find(|s| s.kind == kind)
}

// ----- reloj -----

fn len_clock(cx: &Ctx) -> f32 {
    let s = cx.render_scale;
    if cx.is_vertical {
        let time_w = text_width_estimate_render(&cx.widgets.time, 11.0 * s);
        let date_w = text_width_estimate_render(&cx.widgets.date, 11.0 * s);
        time_w.max(date_w)
    } else {
        let time_w = text_width_estimate_render(&cx.widgets.time, 12.0 * s);
        let date_w = text_width_estimate_render(&cx.widgets.date, 12.0 * s);
        time_w + 3.5 * s + date_w
    }
}

fn draw_clock(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_clock_widget(
        canvas.pixmap,
        canvas.text_cache,
        cx.widgets,
        r.x,
        r.y,
        r.w,
        r.h,
        cx.render_scale,
        canvas.colors,
        cx.is_vertical,
    );
    false
}

// ----- media -----

fn len_media(cx: &Ctx) -> f32 {
    media_ideal_len(
        cx.is_vertical,
        cx.render_scale,
        cx.settings.media_width_scale,
    )
}

fn draw_media(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    // ----- el bloque derecho espeja el contenido -----
    let mirrored = r.slot == crate::config::WidgetSlot::Right;
    draw_media_widget(
        canvas.pixmap,
        canvas.icon_cache,
        canvas.text_cache,
        cx.widgets,
        canvas.marquee,
        canvas.advance,
        r.x,
        r.y,
        r.w,
        r.h,
        cx.render_scale,
        canvas.colors,
        cx.is_vertical,
        mirrored,
        canvas.bar_len,
        cx.settings.media_smooth_scroll,
        cx.settings.media_width_scale,
    )
}

// ----- tray -----

fn len_tray(cx: &Ctx) -> f32 {
    if cx.tray_count == 0 {
        0.0
    } else {
        tray_geometry(
            cx.tray_count,
            cx.is_vertical,
            cx.cross_len,
            cx.cross_len,
            cx.render_scale,
        )
        .2
    }
}

fn draw_tray(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_tray_widget(
        canvas.pixmap,
        canvas.icon_cache,
        canvas.tray,
        r.x,
        r.y,
        r.w,
        r.h,
        cx.render_scale,
        cx.is_vertical,
    );
    false
}

// ----- red -----

fn len_network(cx: &Ctx) -> f32 {
    24.0 * cx.render_scale
}

fn draw_network(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_network_widget(
        canvas.pixmap,
        canvas.text_cache,
        cx.widgets,
        r.x,
        r.y,
        r.w,
        r.h,
        cx.render_scale,
        canvas.colors,
        cx.is_vertical,
        canvas.is_hovered(r),
    );
    false
}

// ----- volumen -----

fn len_volume(cx: &Ctx) -> f32 {
    // ----- boton compacto: simbolo chico + el numero pelado (sin "%") -----
    let label = match cx.widgets.volume {
        Some((_, true)) => "MUTE".to_string(),
        Some((pct, false)) => pct.to_string(),
        None => return 0.0,
    };
    volume_content_len(&label, cx.render_scale)
}

fn draw_volume(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_volume_widget(
        canvas.pixmap,
        canvas.text_cache,
        cx.widgets,
        r.x,
        r.y,
        r.w,
        r.h,
        cx.render_scale,
        canvas.colors,
        cx.is_vertical,
        canvas.is_hovered(r),
    );
    false
}

// ----- disposicion de teclado -----

fn len_kblayout(cx: &Ctx) -> f32 {
    let w = text_width_estimate_render(&cx.widgets.kblayout.short, 8.5 * cx.render_scale);
    w + 10.0 * cx.render_scale
}

fn draw_kblayout(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_kblayout_widget(
        canvas.pixmap,
        canvas.text_cache,
        cx.widgets,
        r.x,
        r.y,
        r.w,
        r.h,
        cx.render_scale,
        canvas.colors,
        cx.is_vertical,
    );
    false
}
