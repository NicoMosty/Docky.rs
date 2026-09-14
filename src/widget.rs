//! La tabla de widgets: un solo lugar que sabe que widgets existen, con que se
//! miden, con que se dibujan y que hacen cuando los tocan.
//!
//! Reemplaza los `match kind` que estaban repartidos entre
//! `widget_natural_len`, `draw_widgets` y el despacho de clicks de
//! `app/pointer.rs`. Agregar un widget costaba 16 sitios en 7 archivos (medido
//! con `KbdLayout`, commit c458821); la meta es que sea UNA entrada aca.
//!
//! El dibujo recibe el MISMO `WidgetRect` que produjo `layout_widgets`, asi que
//! el reparto y el dibujo no pueden usar escalas distintas. Eso es lo que las
//! trampas 10 y 12 de AGENTS.md piden y hoy sostienen a mano los tests.

use crate::config::WidgetKind;
// ----- los tipos y las funciones de dibujo. `render/mod.rs` los re-exporta en un
// solo bloque: la tabla vive en la raiz, pero el dibujo sigue siendo de `render/` -----
use crate::render::{
    MarqueeState, WidgetColors, WidgetRect, draw_battery_widget, draw_bluetooth_icon,
    draw_clock_widget, draw_cpu_widget, draw_kblayout_widget, draw_media_widget,
    draw_network_widget, draw_power_widget, draw_ram_widget, draw_tray_widget,
    draw_volume_widget, draw_widget_button_bg, draw_workspaces_widget, media_ideal_len,
    percentage_widget_len, text_widget_len, text_width_estimate_render, tray_geometry,
    volume_content_len, workspaces_geometry,
};
use crate::widgets::WidgetSnapshot;
use dockyrs_canvas::{IconCache, TextCache};
use tiny_skia::Pixmap;

/// Que se toco y donde. La rueda lleva la direccion porque el volumen la usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WidgetClick {
    Left,
    Right,
    Wheel { up: bool },
}

/// Lo que un widget necesita para decidir que hacer con un click.
///
/// NO lleva `dock` a proposito: las sub-zonas de un widget (que icono del tray,
/// que workspace, el boton de play de Media) las resuelve el llamador con los hit
/// tests que ya existen y entran aca ya masticadas. Gracias a eso las decisiones
/// de la tabla son puras y se pueden testear sin armar un `Dock`.
pub(crate) struct ClickCtx {
    pub(crate) click: WidgetClick,
    /// Icono del tray bajo el puntero, si el puntero esta sobre el tray.
    pub(crate) tray_index: Option<usize>,
    /// Workspace bajo el puntero, si esta sobre el widget de workspaces.
    pub(crate) workspace_id: Option<i32>,
    /// `true` si el puntero esta sobre el boton de play de Media.
    pub(crate) media_toggle: bool,
}

/// Que hacer despues del click.
///
/// El widget DECIDE la accion; EJECUTARLA es de la app, porque varias necesitan
/// `&mut App` (abrir el menu de apagado, el panel de volumen, el menu del tray)
/// o el estado del tray, que no vive en el widget. Antes esto era un `match
/// kind` de 10 ramas dentro de `app/pointer.rs`, y era el unico contrato de un
/// widget que no custodiaba nadie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WidgetAction {
    MediaToggle,
    OpenPowerMenu,
    OpenBluetoothManager,
    ActivateTray {
        index: usize,
    },
    OpenTrayMenu {
        index: usize,
    },
    SwitchWorkspace {
        id: i32,
    },
    OpenNetworkSettings,
    OpenSystemMonitor,
    OpenVolumePanel,
    OpenVolumeControl,
    VolumeStep {
        up: bool,
    },
    NextKbdLayout,
    OpenDockMenu,
    /// Menu del SNI del widget (nm-applet / blueman). El ejecutor sabe el
    /// patron y el fallback de cada uno.
    OpenWidgetTrayMenu(crate::config::WidgetKind),
}

/// Lo que un widget necesita para medirse Y para dibujarse.
///
/// Los campos se fueron agregando con cada migracion, no antes: `settings` lo
/// pide Media y `cross_len`/`tray_count` los pide Tray. Con un solo widget
/// migrado el struct nacia con 3 campos muertos y lo dijo el compilador
/// (`dead_code`), que es la senal de que el campo todavia no hacia falta.
/// `bar_len`, `colors` y `hovered` NO van aca porque no existen cuando se mide
/// (el reparto no tiene paleta ni largo de barra): viven en `Canvas`.
pub(crate) struct Ctx<'a> {
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
pub(crate) struct Canvas<'a, 'b> {
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

pub(crate) struct WidgetSpec {
    pub(crate) kind: WidgetKind,
    /// Nombre que muestra el panel de Ajustes. Vive aca y no en un `match`
    /// aparte: era la ultima lista duplicada de los 12 variantes.
    pub(crate) label: &'static str,
    /// Ancho que reserva el reparto. La llama `widget_natural_len`.
    pub(crate) natural_len: fn(&Ctx) -> f32,
    /// Dibuja dentro del `rect` que le dio el reparto. Devuelve `true` si el
    /// widget animo este frame (hoy solo el marquee de Media).
    pub(crate) draw: fn(&mut Canvas, &WidgetRect, &Ctx) -> bool,
    /// Que hace con un click. `None` = el widget no reacciona (hoy Clock,
    /// Battery y Cpu).
    pub(crate) click: Option<fn(&ClickCtx) -> Option<WidgetAction>>,
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
pub(crate) const WIDGETS: &[WidgetSpec] = &[
    WidgetSpec {
        kind: WidgetKind::Clock,
        label: "Clock",
        natural_len: len_clock,
        draw: draw_clock,
        click: None,
    },
    WidgetSpec {
        kind: WidgetKind::Battery,
        label: "Battery",
        natural_len: len_battery,
        draw: draw_battery,
        click: None,
    },
    WidgetSpec {
        kind: WidgetKind::Media,
        label: "Media Player",
        natural_len: len_media,
        draw: draw_media,
        click: Some(click_media),
    },
    WidgetSpec {
        kind: WidgetKind::PowerMenu,
        label: "Power Menu",
        natural_len: len_power,
        draw: draw_power,
        click: Some(click_power),
    },
    WidgetSpec {
        kind: WidgetKind::Bluetooth,
        label: "Bluetooth",
        natural_len: len_bluetooth,
        draw: draw_bluetooth,
        click: Some(click_bluetooth),
    },
    WidgetSpec {
        kind: WidgetKind::Tray,
        label: "System Tray",
        natural_len: len_tray,
        draw: draw_tray,
        click: Some(click_tray),
    },
    WidgetSpec {
        kind: WidgetKind::Workspaces,
        label: "Workspaces",
        natural_len: len_workspaces,
        draw: draw_workspaces,
        click: Some(click_workspaces),
    },
    WidgetSpec {
        kind: WidgetKind::Cpu,
        label: "CPU",
        natural_len: len_cpu,
        draw: draw_cpu,
        click: None,
    },
    WidgetSpec {
        kind: WidgetKind::Ram,
        label: "RAM",
        natural_len: len_ram,
        draw: draw_ram,
        click: Some(click_ram),
    },
    WidgetSpec {
        kind: WidgetKind::Network,
        label: "Network",
        natural_len: len_network,
        draw: draw_network,
        click: Some(click_network),
    },
    WidgetSpec {
        kind: WidgetKind::Volume,
        label: "Volume",
        natural_len: len_volume,
        draw: draw_volume,
        click: Some(click_volume),
    },
    WidgetSpec {
        kind: WidgetKind::KbdLayout,
        label: "Keyboard Layout",
        natural_len: len_kblayout,
        draw: draw_kblayout,
        click: Some(click_kblayout),
    },
];

pub(crate) fn spec_for(kind: WidgetKind) -> Option<&'static WidgetSpec> {
    WIDGETS.iter().find(|s| s.kind == kind)
}

/// El orden en que los widgets aparecen en el panel de Ajustes: el de `WIDGETS`.
/// Ya no hay una segunda lista que mantener sincronizada a mano.
pub(crate) fn widget_kind_order() -> Vec<WidgetKind> {
    WIDGETS.iter().map(|s| s.kind).collect()
}

/// Nombre que muestra Ajustes. Sale de la tabla, no de un `match` aparte.
pub(crate) fn widget_label(kind: WidgetKind) -> &'static str {
    spec_for(kind).map(|s| s.label).unwrap_or("?")
}

// ----- clicks -----
//
// Cada uno traduce un click (o una rueda) a una `WidgetAction`, y son PUROS: la
// unica entrada es `ClickCtx`, sin `Dock` ni estado de la app. Los que tienen
// sub-zonas (Media, Tray, Workspaces) las reciben ya resueltas. Los
// `WidgetClick::Wheel` de la mayoria son `None` a proposito: la rueda hoy es
// solo del volumen.

fn click_media(cx: &ClickCtx) -> Option<WidgetAction> {
    // ----- el widget de media tiene zonas: solo el boton de play dispara -----
    (cx.click == WidgetClick::Left && cx.media_toggle).then_some(WidgetAction::MediaToggle)
}

fn click_power(cx: &ClickCtx) -> Option<WidgetAction> {
    (cx.click == WidgetClick::Left).then_some(WidgetAction::OpenPowerMenu)
}

fn click_bluetooth(cx: &ClickCtx) -> Option<WidgetAction> {
    match cx.click {
        WidgetClick::Left => Some(WidgetAction::OpenBluetoothManager),
        WidgetClick::Right => Some(WidgetAction::OpenWidgetTrayMenu(
            crate::config::WidgetKind::Bluetooth,
        )),
        WidgetClick::Wheel { .. } => None,
    }
}

fn click_tray(cx: &ClickCtx) -> Option<WidgetAction> {
    let index = cx.tray_index?;
    match cx.click {
        WidgetClick::Left => Some(WidgetAction::ActivateTray { index }),
        WidgetClick::Right => Some(WidgetAction::OpenTrayMenu { index }),
        WidgetClick::Wheel { .. } => None,
    }
}

fn click_workspaces(cx: &ClickCtx) -> Option<WidgetAction> {
    match cx.click {
        WidgetClick::Left => {
            let id = cx.workspace_id?;
            Some(WidgetAction::SwitchWorkspace { id })
        }
        // ----- ajustes: SÓLO con click derecho sobre el indicador -----
        WidgetClick::Right => Some(WidgetAction::OpenDockMenu),
        WidgetClick::Wheel { .. } => None,
    }
}

fn click_ram(cx: &ClickCtx) -> Option<WidgetAction> {
    (cx.click == WidgetClick::Left).then_some(WidgetAction::OpenSystemMonitor)
}

fn click_network(cx: &ClickCtx) -> Option<WidgetAction> {
    match cx.click {
        WidgetClick::Left => Some(WidgetAction::OpenNetworkSettings),
        // ----- wifi sale del tray visible, asi que su menu se abre desde su
        // propio widget de la izquierda -----
        WidgetClick::Right => Some(WidgetAction::OpenWidgetTrayMenu(
            crate::config::WidgetKind::Network,
        )),
        WidgetClick::Wheel { .. } => None,
    }
}

fn click_volume(cx: &ClickCtx) -> Option<WidgetAction> {
    match cx.click {
        // ----- click = panel de volumen (salida, un stream por app y selector
        // de salida) -----
        WidgetClick::Left => Some(WidgetAction::OpenVolumePanel),
        // ----- click derecho = el control externo (pavucontrol) -----
        WidgetClick::Right => Some(WidgetAction::OpenVolumeControl),
        // ----- rueda = +/- 5% -----
        WidgetClick::Wheel { up } => Some(WidgetAction::VolumeStep { up }),
    }
}

fn click_kblayout(cx: &ClickCtx) -> Option<WidgetAction> {
    (cx.click == WidgetClick::Left).then_some(WidgetAction::NextKbdLayout)
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

#[cfg(test)]
mod click_tests {
    // ----- Las decisiones de click son puras (no piden `Dock`), asi que se
    // pueden fijar aca. Antes eran un `match kind` de 10 ramas dentro de
    // `app/pointer.rs` que no custodiaba ningun test: era el unico contrato de un
    // widget sin guard. -----
    use super::*;
    use WidgetAction as A;
    use WidgetClick::{Left, Right, Wheel};
    use crate::config::WidgetKind;

    fn sin_zonas(click: WidgetClick) -> ClickCtx {
        ClickCtx {
            click,
            tray_index: None,
            workspace_id: None,
            media_toggle: false,
        }
    }

    fn decidir(kind: WidgetKind, cx: &ClickCtx) -> Option<WidgetAction> {
        let click_fn = spec_for(kind).expect("widget en la tabla").click;
        click_fn.and_then(|f| f(cx))
    }

    /// Clock, Battery y Cpu no reaccionan a nada: en el `match` viejo caian al
    /// `_ => {}`.
    #[test]
    fn los_que_no_reaccionan_no_reaccionan() {
        for kind in [WidgetKind::Clock, WidgetKind::Battery, WidgetKind::Cpu] {
            assert!(
                spec_for(kind).is_some_and(|s| s.click.is_none()),
                "{kind:?} tendria que no tener click"
            );
            assert_eq!(decidir(kind, &sin_zonas(Left)), None, "{kind:?}");
            assert_eq!(decidir(kind, &sin_zonas(Right)), None, "{kind:?}");
            assert_eq!(decidir(kind, &sin_zonas(Wheel { up: true })), None, "{kind:?}");
        }
    }

    #[test]
    fn el_click_izquierdo_es_el_de_siempre() {
        assert_eq!(
            decidir(WidgetKind::PowerMenu, &sin_zonas(Left)),
            Some(A::OpenPowerMenu)
        );
        assert_eq!(
            decidir(WidgetKind::Bluetooth, &sin_zonas(Left)),
            Some(A::OpenBluetoothManager)
        );
        assert_eq!(
            decidir(WidgetKind::Ram, &sin_zonas(Left)),
            Some(A::OpenSystemMonitor)
        );
        assert_eq!(
            decidir(WidgetKind::Network, &sin_zonas(Left)),
            Some(A::OpenNetworkSettings)
        );
        assert_eq!(
            decidir(WidgetKind::Volume, &sin_zonas(Left)),
            Some(A::OpenVolumePanel)
        );
        assert_eq!(
            decidir(WidgetKind::KbdLayout, &sin_zonas(Left)),
            Some(A::NextKbdLayout)
        );

        // ----- los tres con sub-zona: sin sub-zona no hay accion -----
        assert_eq!(decidir(WidgetKind::Media, &sin_zonas(Left)), None);
        assert_eq!(decidir(WidgetKind::Tray, &sin_zonas(Left)), None);
        assert_eq!(decidir(WidgetKind::Workspaces, &sin_zonas(Left)), None);

        let con_zona = |tray, ws, media| ClickCtx {
            click: Left,
            tray_index: tray,
            workspace_id: ws,
            media_toggle: media,
        };
        assert_eq!(
            decidir(WidgetKind::Tray, &con_zona(Some(3), None, false)),
            Some(A::ActivateTray { index: 3 })
        );
        assert_eq!(
            decidir(WidgetKind::Workspaces, &con_zona(None, Some(7), false)),
            Some(A::SwitchWorkspace { id: 7 })
        );
        assert_eq!(
            decidir(WidgetKind::Media, &con_zona(None, None, true)),
            Some(A::MediaToggle)
        );
        // ----- Media tiene zonas anchas: estar sobre el widget no alcanza -----
        assert_eq!(decidir(WidgetKind::Media, &con_zona(None, None, false)), None);
    }

    #[test]
    fn el_derecho_y_la_rueda_son_los_de_siempre() {
        assert_eq!(
            decidir(WidgetKind::Network, &sin_zonas(Right)),
            Some(A::OpenWidgetTrayMenu(WidgetKind::Network))
        );
        assert_eq!(
            decidir(WidgetKind::Bluetooth, &sin_zonas(Right)),
            Some(A::OpenWidgetTrayMenu(WidgetKind::Bluetooth))
        );
        assert_eq!(
            decidir(WidgetKind::Volume, &sin_zonas(Right)),
            Some(A::OpenVolumeControl)
        );
        assert_eq!(
            decidir(WidgetKind::Workspaces, &sin_zonas(Right)),
            Some(A::OpenDockMenu)
        );
        let mut con_tray = sin_zonas(Right);
        con_tray.tray_index = Some(1);
        assert_eq!(
            decidir(WidgetKind::Tray, &con_tray),
            Some(A::OpenTrayMenu { index: 1 })
        );

        // ----- la rueda es SOLO del volumen; el resto devuelve None a proposito -----
        assert_eq!(
            decidir(WidgetKind::Volume, &sin_zonas(Wheel { up: true })),
            Some(A::VolumeStep { up: true })
        );
        assert_eq!(
            decidir(WidgetKind::Volume, &sin_zonas(Wheel { up: false })),
            Some(A::VolumeStep { up: false })
        );
        for kind in [
            WidgetKind::PowerMenu,
            WidgetKind::KbdLayout,
            WidgetKind::Network,
            WidgetKind::Tray,
        ] {
            assert_eq!(decidir(kind, &sin_zonas(Wheel { up: true })), None, "{kind:?}");
        }
    }
}
