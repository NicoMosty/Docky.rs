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
    /// `true` si este frame toca avanzar el marquee de Media.
    pub(super) advance: bool,
    /// `true` si este frame toca avanzar la animacion de Workspaces. Es un flag
    /// distinto del de Media: el tick que los mueve no es el mismo.
    pub(super) advance_ws: bool,
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

/// Los 12 widgets del enum, en el orden del enum. Es la unica lista: reemplazo
/// el `match` de `widget_natural_len`, el `match` de `draw_widgets`,
/// `WIDGET_KIND_ORDER` y el `match` de `widget_label`.
///
/// ponytail: los adaptadores de abajo todavia desarman el `WidgetRect` para
/// llamar a las `draw_*_widget`, que siguen recibiendo `(zx, zy, zw, zh)`. Por
/// eso este archivo es mas largo que los dos `match` que reemplaza: el
/// boilerplate se movio, no desaparecio. Cobrarlo del todo pide cambiar esas 12
/// firmas a `&WidgetRect`.
pub(super) const WIDGETS: &[WidgetSpec] = &[
    WidgetSpec {
        kind: crate::config::WidgetKind::Clock,
        natural_len: len_clock,
        draw: draw_clock,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::Battery,
        natural_len: len_battery,
        draw: draw_battery,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::Media,
        natural_len: len_media,
        draw: draw_media,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::PowerMenu,
        natural_len: len_power,
        draw: draw_power,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::Bluetooth,
        natural_len: len_bluetooth,
        draw: draw_bluetooth,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::Tray,
        natural_len: len_tray,
        draw: draw_tray,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::Workspaces,
        natural_len: len_workspaces,
        draw: draw_workspaces,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::Cpu,
        natural_len: len_cpu,
        draw: draw_cpu,
    },
    WidgetSpec {
        kind: crate::config::WidgetKind::Ram,
        natural_len: len_ram,
        draw: draw_ram,
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

// ----- bateria -----

fn len_battery(cx: &Ctx) -> f32 {
    let Some((pct, _)) = cx.widgets.battery else {
        return 0.0;
    };
    let label = format!("{pct}%");
    if cx.is_vertical {
        let label_len = text_width_estimate_render(&label, 9.5 * cx.render_scale);
        9.0 * cx.render_scale + 5.0 * cx.render_scale + label_len
    } else {
        // ----- el porcentaje va dentro del icono: solo el ancho de este -----
        32.0 * cx.render_scale
    }
}

fn draw_battery(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_battery_widget(
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

// ----- apagado -----

fn len_power(cx: &Ctx) -> f32 {
    14.0 * cx.render_scale
}

fn draw_power(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_power_widget(
        canvas.pixmap,
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

// ----- bluetooth -----

fn len_bluetooth(cx: &Ctx) -> f32 {
    // ----- solo el icono: el estado se expresa con color, sin texto -----
    16.0 * cx.render_scale
}

fn draw_bluetooth(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    let (powered, in_use) = match &cx.widgets.bluetooth {
        Some(bt) => (bt.powered, bt.powered && bt.connected.is_some()),
        None => (false, false),
    };
    draw_widget_button_bg(
        canvas.pixmap,
        r.x,
        r.y,
        r.w,
        r.h,
        cx.render_scale,
        canvas.colors,
        canvas.is_hovered(r),
    );
    draw_bluetooth_icon(
        canvas.pixmap,
        cx.render_scale,
        r.x + r.w / 2.0,
        r.y + r.h / 2.0,
        powered,
        in_use,
        canvas.colors,
    );
    false
}

// ----- workspaces -----

fn len_workspaces(cx: &Ctx) -> f32 {
    workspaces_geometry(&cx.widgets.workspaces, cx.render_scale)
}

fn draw_workspaces(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_workspaces_widget(
        canvas.pixmap,
        &cx.widgets.workspaces,
        canvas.marquee,
        canvas.advance_ws,
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

// ----- cpu y ram -----

fn len_cpu(cx: &Ctx) -> f32 {
    percentage_widget_len(cx.widgets.cpu, cx.is_vertical, cx.render_scale)
}

fn draw_cpu(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_cpu_widget(
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

fn len_ram(cx: &Ctx) -> f32 {
    let label = match cx.widgets.ram_gb {
        Some((used, total)) => format!("{:.1} / {:.0} GB", used, total),
        None => match cx.widgets.ram {
            Some(pct) => format!("{pct}%"),
            None => return 0.0,
        },
    };
    text_widget_len(&label, cx.is_vertical, cx.render_scale)
}

fn draw_ram(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_ram_widget(
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
