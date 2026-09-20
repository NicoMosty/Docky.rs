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
    draw_clock_widget, draw_cpu_widget, draw_kblayout_widget, draw_media_widget, draw_mic_widget,
    draw_network_widget, draw_power_widget, draw_ram_widget, draw_recording_widget,
    draw_text_widget, draw_tray_widget, draw_volume_widget, draw_widget_button_bg,
    draw_workspaces_widget, media_ideal_len, percentage_widget_len, text_widget_len,
    text_width_estimate_render, tray_geometry, volume_content_len, volume_icon_r, widget_text_px,
    workspaces_geometry,
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
    /// Toggle del mute del micrófono (click del widget `Mic`).
    ToggleMic,
    /// Arranca/para la grabación corriendo `record-toggle.sh` (click del widget).
    ToggleRecording,
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
    pub(super) kind: WidgetKind,
    pub(super) widgets: &'a WidgetSnapshot,
    pub(super) settings: &'a crate::config::DockSettings,
    /// Ya viene multiplicada por `widget_scale`, igual que en el dibujo.
    pub(super) render_scale: f32,
    pub(super) is_vertical: bool,
    /// Largo del eje corto: `w` en horizontal, `h` en vertical.
    pub(super) cross_len: f32,
    pub(super) tray_count: usize,
    /// Modo **compacto** de la isla: 26 px de grosor y una sola actividad por vez, así
    /// que el widget muestra lo mínimo. Hoy sólo lo mira el reloj (hora sin AM/PM y sin
    /// fecha); el resto de los widgets lo ignoran. La MEDIDA y el DIBUJO leen el mismo
    /// flag, así que no se pueden despegar (trampa 12).
    pub(super) compact: bool,
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
    /// `true` si el widget dibuja texto propio. Lo mira el editor del panel para
    /// no ofrecer la fila de letra donde no hay nada que escalar (Tray,
    /// Workspaces, Bluetooth y Power son sólo símbolo).
    pub(crate) text: bool,
}

/// Los 12 widgets fijos del enum más la entrada genérica para `Custom`. Esa
/// entrada cubre todos los payloads: el índice vive en el `WidgetRect` que ya
/// recibe el dibujo y en el `kind` que ahora lleva `Ctx` al medir. Es la unica
/// lista: reemplaza el `match` de `widget_natural_len`, el `match` de
/// `draw_widgets`, `WIDGET_KIND_ORDER` y el `match` de `widget_label`.
///
/// ponytail: los adaptadores de abajo todavia desarman el `WidgetRect` para
/// llamar a las `draw_*_widget`, que siguen recibiendo `(zx, zy, zw, zh)`. Por
/// eso este archivo es mas largo que los dos `match` que reemplaza: el
/// boilerplate se movio, no desaparecio. Cobrarlo del todo pide cambiar esas 12
/// firmas a `&WidgetRect`.
pub(crate) const WIDGETS: &[WidgetSpec] = &[
    WidgetSpec {
        kind: WidgetKind::Clock,

        text: true,
        label: "Clock",
        natural_len: len_clock,
        draw: draw_clock,
        click: None,
    },
    WidgetSpec {
        kind: WidgetKind::Battery,

        text: true,
        label: "Battery",
        natural_len: len_battery,
        draw: draw_battery,
        click: None,
    },
    WidgetSpec {
        kind: WidgetKind::Media,

        text: true,
        label: "Media Player",
        natural_len: len_media,
        draw: draw_media,
        click: Some(click_media),
    },
    WidgetSpec {
        kind: WidgetKind::PowerMenu,

        text: false,
        label: "Power Menu",
        natural_len: len_power,
        draw: draw_power,
        click: Some(click_power),
    },
    WidgetSpec {
        kind: WidgetKind::Bluetooth,

        text: false,
        label: "Bluetooth",
        natural_len: len_bluetooth,
        draw: draw_bluetooth,
        click: Some(click_bluetooth),
    },
    WidgetSpec {
        kind: WidgetKind::Tray,

        text: false,
        label: "System Tray",
        natural_len: len_tray,
        draw: draw_tray,
        click: Some(click_tray),
    },
    WidgetSpec {
        kind: WidgetKind::Workspaces,

        text: false,
        label: "Workspaces",
        natural_len: len_workspaces,
        draw: draw_workspaces,
        click: Some(click_workspaces),
    },
    WidgetSpec {
        kind: WidgetKind::Cpu,

        text: true,
        label: "CPU",
        natural_len: len_cpu,
        draw: draw_cpu,
        click: None,
    },
    WidgetSpec {
        kind: WidgetKind::Ram,

        text: true,
        label: "RAM",
        natural_len: len_ram,
        draw: draw_ram,
        click: Some(click_ram),
    },
    WidgetSpec {
        kind: WidgetKind::Network,

        text: true,
        label: "Network",
        natural_len: len_network,
        draw: draw_network,
        click: Some(click_network),
    },
    WidgetSpec {
        kind: WidgetKind::Volume,

        text: true,
        label: "Volume",
        natural_len: len_volume,
        draw: draw_volume,
        click: Some(click_volume),
    },
    WidgetSpec {
        kind: WidgetKind::Mic,

        text: true,
        label: "Mic",
        natural_len: len_mic,
        draw: draw_mic,
        click: Some(click_mic),
    },
    WidgetSpec {
        kind: WidgetKind::Recording,

        text: true,
        label: "Recording",
        natural_len: len_recording,
        draw: draw_recording,
        // ----- el click corre `record-toggle.sh` (arranca/para la grabación) -----
        click: Some(click_recording),
    },
    WidgetSpec {
        kind: WidgetKind::KbdLayout,

        text: true,
        label: "Keyboard Layout",
        natural_len: len_kblayout,
        draw: draw_kblayout,
        click: Some(click_kblayout),
    },
    WidgetSpec {
        kind: WidgetKind::Custom(0),
        label: "Custom widget",
        natural_len: len_custom,
        draw: draw_custom,
        click: None,
        text: true,
    },
];

pub(crate) fn spec_for(kind: WidgetKind) -> Option<&'static WidgetSpec> {
    WIDGETS
        .iter()
        .find(|s| s.kind == kind)
        .or_else(|| matches!(kind, WidgetKind::Custom(_)).then(custom_spec))
}

/// La única entrada con payload: todos los `Custom(i)` comparten medida,
/// dibujo y etiqueta, y el índice real viaja en el `kind` que ya reparte el
/// layout.
fn custom_spec() -> &'static WidgetSpec {
    WIDGETS
        .iter()
        .find(|s| matches!(s.kind, WidgetKind::Custom(_)))
        .expect("WIDGETS necesita una entrada Custom")
}

/// El orden en que los widgets aparecen en el panel de Ajustes: el de `WIDGETS`, sin la entrada
/// genérica. Un `Custom(i)` se configura a mano en el JSON: su índice no se puede elegir en un chip.
/// Ya no hay una segunda lista que mantener sincronizada a mano.
pub(crate) fn widget_kind_order() -> Vec<WidgetKind> {
    WIDGETS
        .iter()
        .filter(|s| !matches!(s.kind, WidgetKind::Custom(_)))
        .map(|s| s.kind)
        .collect()
}

/// Nombre que muestra Ajustes. Sale de la tabla, no de un `match` aparte.
pub(crate) fn widget_label(kind: WidgetKind) -> &'static str {
    spec_for(kind).map(|s| s.label).unwrap_or("?")
}

/// `true` si el widget dibuja texto propio. Lo mira el editor del panel: la fila
/// de letra no tiene sentido en un widget que es sólo símbolo (Tray, Workspaces,
/// Bluetooth, Power), y ofrecerla igual hace que el usuario mueva un slider que
/// no cambia nada.
pub(crate) fn widget_has_text(kind: WidgetKind) -> bool {
    spec_for(kind).is_some_and(|s| s.text)
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

/// El micrófono es un indicador de dos estados: el click togglea el mute (rueda y
/// click derecho no hacen nada).
fn click_mic(cx: &ClickCtx) -> Option<WidgetAction> {
    (cx.click == WidgetClick::Left).then_some(WidgetAction::ToggleMic)
}

fn click_recording(cx: &ClickCtx) -> Option<WidgetAction> {
    (cx.click == WidgetClick::Left).then_some(WidgetAction::ToggleRecording)
}

fn click_kblayout(cx: &ClickCtx) -> Option<WidgetAction> {
    (cx.click == WidgetClick::Left).then_some(WidgetAction::NextKbdLayout)
}

// ----- reloj -----

fn len_clock(cx: &Ctx) -> f32 {
    let s = cx.render_scale;
    let tp = widget_text_px(cx.settings, cx.kind, s);
    // ----- en la isla va SÓLO la hora (sin AM/PM ni fecha): el ancho sale de la misma
    // cadena que dibuja `draw_clock_widget` con `compact` (trampa 12) -----
    if cx.compact {
        return text_width_estimate_render(cx.widgets.time_short(), tp);
    }
    // ----- hora y fecha en UNA línea (horizontal) o en UNA columna rotada
    // (vertical): el vertical medía `max` porque las dibujaba en dos columnas
    // lado a lado, y esas dos columnas no entran en el grosor de la barra -----
    let time_w = text_width_estimate_render(&cx.widgets.time, tp);
    let date_w = text_width_estimate_render(&cx.widgets.date, tp);
    time_w + 3.5 * s + date_w
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
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
        cx.compact,
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
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
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
    volume_content_len(
        &label,
        cx.render_scale,
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
        volume_icon_r(cx.settings, cx.render_scale),
    )
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
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
        volume_icon_r(cx.settings, cx.render_scale),
    );
    false
}

// ----- micrófono -----

/// Etiqueta del micrófono: el estado (el nivel de una entrada no dice nada útil). Con
/// "" el widget no se dibuja.
fn mic_label(mic: Option<(u8, bool)>) -> &'static str {
    match mic {
        Some((_, true)) => "MUTE",
        Some(_) => "ON",
        None => "",
    }
}

fn len_mic(cx: &Ctx) -> f32 {
    let label = mic_label(cx.widgets.mic);
    if label.is_empty() {
        return 0.0;
    }
    // ----- misma geometría de pastilla que el volumen (icono + etiqueta) -----
    volume_content_len(
        label,
        cx.render_scale,
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
        volume_icon_r(cx.settings, cx.render_scale),
    )
}

// ----- grabación -----

fn len_recording(cx: &Ctx) -> f32 {
    let Some(secs) = cx.widgets.recording else {
        return 0.0;
    };
    let label = crate::widgets::fmt_elapsed(secs);
    // ----- misma pastilla que el volumen/mic (punto + etiqueta) -----
    volume_content_len(
        &label,
        cx.render_scale,
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
        volume_icon_r(cx.settings, cx.render_scale),
    )
}

fn draw_recording(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_recording_widget(
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
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
        volume_icon_r(cx.settings, cx.render_scale),
    );
    false
}

fn draw_mic(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    draw_mic_widget(
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
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
        volume_icon_r(cx.settings, cx.render_scale),
    );
    false
}

// ----- disposicion de teclado -----

fn len_kblayout(cx: &Ctx) -> f32 {
    let tp = widget_text_px(cx.settings, cx.kind, cx.render_scale);
    let w = text_width_estimate_render(&cx.widgets.kblayout.short, tp);
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
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
    );
    false
}

// ----- widget con script -----

fn len_custom(cx: &Ctx) -> f32 {
    let Some(text) = crate::widgets::custom_text_for(cx.settings, cx.widgets, cx.kind) else {
        return 0.0;
    };
    text_widget_len(
        text,
        cx.is_vertical,
        cx.render_scale,
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
    )
}

fn draw_custom(canvas: &mut Canvas, r: &WidgetRect, cx: &Ctx) -> bool {
    let Some(label) = crate::widgets::custom_text_for(cx.settings, cx.widgets, cx.kind) else {
        return false;
    };
    draw_text_widget(
        canvas.pixmap,
        canvas.text_cache,
        label,
        r.x,
        r.y,
        r.w,
        r.h,
        cx.render_scale,
        canvas.colors,
        cx.is_vertical,
        canvas.is_hovered(r),
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
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
        let tp = widget_text_px(cx.settings, cx.kind, cx.render_scale);
        let label_len = text_width_estimate_render(&label, tp);
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
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
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
    percentage_widget_len(
        cx.widgets.cpu,
        cx.is_vertical,
        cx.render_scale,
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
    )
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
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
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
    text_widget_len(
        &label,
        cx.is_vertical,
        cx.render_scale,
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
    )
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
        widget_text_px(cx.settings, cx.kind, cx.render_scale),
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
    use crate::config::WidgetKind;
    use WidgetAction as A;
    use WidgetClick::{Left, Right, Wheel};

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
            assert_eq!(
                decidir(kind, &sin_zonas(Wheel { up: true })),
                None,
                "{kind:?}"
            );
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
        assert_eq!(
            decidir(WidgetKind::Media, &con_zona(None, None, false)),
            None
        );
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
            assert_eq!(
                decidir(kind, &sin_zonas(Wheel { up: true })),
                None,
                "{kind:?}"
            );
        }
    }
}

#[cfg(test)]
mod clock_vertical_tests {
    // ----- En barra vertical (dock Edge Left/Right) el reloj dibujaba la hora y
    // la fecha como DOS columnas rotadas lado a lado. Cada columna de texto mide
    // `1.5 * text_px` en el eje corto (el alto del pixmap rasterizado) y con
    // `font_scale` > 1 eso son ~22px contra los ~25 que mide el grosor de la
    // barra: no entraban. El reloj se veía cortado, la hora perdía la mitad de
    // las letras contra el borde de la pantalla y la fecha se metía en la esquina
    // redondeada. Ahora es UNA columna y el largo que reserva `len_clock` es la
    // misma suma que en horizontal (trampa 12: medir y dibujar con la misma
    // cuenta).
    use super::*;
    use crate::config::WidgetKind;
    use crate::render::text_width_estimate_render;
    use crate::widgets::{BatteryState, KbLayout, NetworkInfo};

    /// Grosor de la barra con el `dock_scale` de la config del usuario
    /// (`icon_size * magnify_scale + 2 * V_EDGE_PADDING`, 44*0.3*1.35 + 7.2).
    const CROSS: f32 = 25.02;

    fn snapshot() -> WidgetSnapshot {
        WidgetSnapshot {
            time: "10:29 PM".into(),
            date: "17-Sept".into(),
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

    /// Los valores con los que se reportó el corte.
    fn settings() -> crate::config::DockSettings {
        crate::config::DockSettings {
            font_scale: 1.513213,
            widget_scale: 1.1065265,
            ..Default::default()
        }
    }

    fn largo_vertical(s: &crate::config::DockSettings, w: &WidgetSnapshot) -> f32 {
        let cx = Ctx {
            kind: WidgetKind::Clock,
            widgets: w,
            settings: s,
            render_scale: s.widget_scale,
            is_vertical: true,
            cross_len: CROSS,
            tray_count: 0,
            // ----- mide como la barra; el compacto tiene su propio test -----
            compact: false,
        };
        (spec_for(WidgetKind::Clock)
            .expect("reloj en la tabla")
            .natural_len)(&cx)
    }

    fn largo_compacto(s: &crate::config::DockSettings, w: &WidgetSnapshot) -> f32 {
        let cx = Ctx {
            kind: WidgetKind::Clock,
            widgets: w,
            settings: s,
            render_scale: s.widget_scale,
            is_vertical: true,
            cross_len: CROSS,
            tray_count: 0,
            compact: true,
        };
        (spec_for(WidgetKind::Clock)
            .expect("reloj en la tabla")
            .natural_len)(&cx)
    }

    #[test]
    fn el_reloj_vertical_mide_la_suma_y_entra_en_el_grosor() {
        let s = settings();
        let w = snapshot();
        let tp = widget_text_px(&s, WidgetKind::Clock, s.widget_scale);
        let esperado = text_width_estimate_render(&w.time, tp)
            + 3.5 * s.widget_scale
            + text_width_estimate_render(&w.date, tp);
        let largo = largo_vertical(&s, &w);
        assert!(
            (largo - esperado).abs() < 0.01,
            "el reloj vertical tiene que medir la suma hora+fecha (una columna), \
             no el `max` de las dos columnas: {largo} vs {esperado}"
        );
        // ----- la columna rotada ocupa 1.5*text_px en el eje corto -----
        assert!(
            tp * 1.5 <= CROSS,
            "una columna de texto ({}) no entra en el grosor de la barra ({CROSS})",
            tp * 1.5
        );
    }

    /// La isla muestra SÓLO la hora (sin AM/PM ni fecha): el ancho reservado tiene que
    /// salir de esa misma cadena, o el blob queda largo de más y el texto no queda
    /// centrado (el caso real: `11:30 PM 19-Sept` contra `11:30`).
    #[test]
    fn el_reloj_compacto_mide_solo_la_hora_sin_ampm() {
        let s = settings();
        let w = snapshot();
        let tp = widget_text_px(&s, WidgetKind::Clock, s.widget_scale);
        let esperado = text_width_estimate_render(w.time_short(), tp);
        let compacto = largo_compacto(&s, &w);
        assert!(
            (compacto - esperado).abs() < 0.01,
            "el reloj de la isla mide la hora sin AM/PM: {compacto} vs {esperado}"
        );
        assert!(
            compacto < largo_vertical(&s, &w),
            "y menos que el de la barra (que lleva la fecha): {compacto} vs {}",
            largo_vertical(&s, &w)
        );
    }
}
