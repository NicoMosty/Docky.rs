use crate::config::DockEdge;
use crate::dock::Dock;
use crate::widgets::{BatteryState, WidgetSnapshot};
use dockyrs_canvas::{IconCache, TextCache};
use std::time::Instant;
use tiny_skia::{Color, Paint, Pixmap, Rect, Transform};

mod layout;
pub use layout::*;
mod tray;
pub use tray::*;
mod clock_battery;
mod media;
pub use media::*;
mod cpu_ram;
mod power_bluetooth;
mod workspaces;
pub use workspaces::*;
mod syswidgets;

// ----- Lo que `crate::widget` (la tabla de widgets) necesita de aca adentro.
//
// La tabla vive en la raiz del crate porque `menu/` tiene que leer las etiquetas
// y el orden, y `menu/` no importa `render/` (esa es la convencion: menu = logica
// y geometria, sin dibujo). En vez de ensanchar 23 items repartidos en 8 archivos,
// el ensanchado queda aca y se ve de un saque cual es la superficie que el resto
// del crate puede tocar. Los que ya eran `pub` (MarqueeState, workspaces_geometry)
// no figuran. -----
pub(crate) use clock_battery::{draw_battery_widget, draw_clock_widget};
pub(crate) use cpu_ram::{draw_cpu_widget, draw_ram_widget, ram_label};
pub(crate) use layout::{WidgetRect, percentage_widget_len, text_widget_len};
pub(crate) use media::{draw_media_widget, media_ideal_len};
pub(crate) use power_bluetooth::{draw_bluetooth_icon, draw_power_widget};
pub(crate) use syswidgets::{
    draw_kblayout_widget, draw_mic_widget, draw_network_widget, draw_recording_widget,
    draw_text_widget, draw_volume_widget,
};
pub(crate) use tray::{draw_tray_widget, tray_geometry};
pub(crate) use workspaces::draw_workspaces_widget;

const DATE_FONT_FAMILY: &str = "JetBrains Mono";

const MARQUEE_SPEED: f32 = 12.0;
const MARQUEE_HOLD_SECS: f32 = 2.0;
const MARQUEE_GAP_FRAC: f32 = 0.4;
pub const MARQUEE_TICK_MS: u64 = 33;

struct MarqueeLane {
    offset: f32,
    hold_secs: f32,
    last_tick: Option<Instant>,
}

impl Default for MarqueeLane {
    fn default() -> Self {
        Self {
            offset: 0.0,
            hold_secs: MARQUEE_HOLD_SECS,
            last_tick: None,
        }
    }
}

#[derive(Default)]
pub struct MarqueeState {
    key: String,
    title: MarqueeLane,
    ws_initialized: bool,
    ws_current: f32,
    ws_target: f32,
    ws_from: f32,
    ws_t: f32,
    ws_last_tick: Option<Instant>,
}

impl MarqueeState {
    pub fn workspace_animating(&self) -> bool {
        self.ws_initialized && self.ws_t < 1.0
    }

    // ----- el tracking vive fuera del draw: si el redraw se salta
    // (frame pendiente), el update no se pierde y el próximo frame anima -----
    pub fn track_ws_target(&mut self, target: f32) {
        if !self.ws_initialized {
            self.ws_initialized = true;
            self.ws_current = target;
            self.ws_target = target;
            self.ws_t = 1.0;
        } else if (self.ws_target - target).abs() > 0.01 {
            self.ws_from = self.ws_current;
            self.ws_target = target;
            self.ws_t = 0.0;
            self.ws_last_tick = None;
        }
    }
}

const WS_ANIM_DURATION_MS: f32 = 220.0;

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn marquee_step(
    lane: &mut MarqueeLane,
    text_w: f32,
    avail_w: f32,
    advance: bool,
    render_scale: f32,
    smooth_scroll: bool,
) -> (f32, bool) {
    if !smooth_scroll || text_w <= avail_w {
        *lane = MarqueeLane::default();
        return (0.0, false);
    }
    if advance {
        let now = Instant::now();
        // ----- long gap clamp -----
        let dt = lane
            .last_tick
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(0.0)
            .min(0.5);
        lane.last_tick = Some(now);
        if lane.hold_secs > 0.0 {
            lane.hold_secs -= dt;
        } else {
            let period = text_w + avail_w * MARQUEE_GAP_FRAC;
            lane.offset += MARQUEE_SPEED * render_scale * dt;
            if lane.offset >= period {
                lane.offset -= period;
            }
        }
    }
    (lane.offset, true)
}

fn tile_text(clip: &mut Pixmap, glyphs: &Pixmap, avail_w: f32, offset: f32) {
    let text_w = glyphs.width() as f32;
    let paint = tiny_skia::PixmapPaint::default();
    let period = text_w + avail_w * MARQUEE_GAP_FRAC;
    for i in -1..=1 {
        let tx = offset - period + period * i as f32;
        if tx + text_w < 0.0 || tx > avail_w {
            continue;
        }
        clip.draw_pixmap(
            0,
            0,
            glyphs.as_ref(),
            &paint,
            Transform::from_translate(tx, 0.0),
            None,
        );
    }
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let r = r.min(w / 2.0).min(h / 2.0);
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}

// ----- Fondo de botón de la barra: la pastilla redondeada que ya usaba el WiFi
// (accent al 30%, al 80% con el puntero encima). La comparten WiFi, volumen y
// bluetooth para que los tres se lean igual. -----
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_widget_button_bg(
    pixmap: &mut Pixmap,
    zx: f32,
    zy: f32,
    zw: f32,
    zh: f32,
    render_scale: f32,
    colors: &WidgetColors,
    hovered: bool,
) {
    let path = rounded_rect_path(zx + 1.0, zy + 1.0, zw - 2.0, zh - 2.0, 6.0 * render_scale);
    let mut paint = Paint::default();
    paint.set_color_rgba8(
        colors.accent.0,
        colors.accent.1,
        colors.accent.2,
        if hovered { 80 } else { 30 },
    );
    paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
}

// ----- geometría del botón de volumen, en un solo lugar a propósito: la usan el
// reparto de la barra (`widget_natural_len`, que fija su ancho) y el dibujo
// (`draw_volume_widget`). Si divergen, el número queda descentrado o pisando el
// borde de la pastilla — es el mismo tipo de bug que el de `hit_scale`. -----
const VOLUME_ICON_R: f32 = 3.0;
const VOLUME_ICON_GAP: f32 = 3.0;
const VOLUME_PAD: f32 = 2.0;

/// Ancho del texto del botón de volumen.
///
/// `text_width_estimate_render` usa 0.56 em/char, que anda bien para los dígitos
/// pero subestima ~20% las mayúsculas anchas (M, U, W): "MUTE" medía ~23px contra
/// los 19 que estimaba y se salía de la pastilla. Por eso acá las etiquetas con
/// letras pagan un factor más grande, y sólo las de puros dígitos se quedan con el
/// estimador general (así el botón normal no se ensancha de gusto).
fn volume_label_len(label: &str, size: f32) -> f32 {
    let per_char = if label.chars().all(|c| c.is_ascii_digit()) {
        0.56
    } else {
        0.70
    };
    label.chars().count() as f32 * size * per_char
}

/// Radio del símbolo del botón de volumen, en px lógicos ya escalados. Es la
/// ÚNICA fuente del tamaño de la bocina: la usan el reparto (que reserva
/// `2 * radio`) y el dibujo, así que un ajuste de "Widget Icon" no puede
/// desincronizar el contenido de su pastilla (trampa 12).
pub(crate) fn volume_icon_r(settings: &crate::config::DockSettings, render_scale: f32) -> f32 {
    VOLUME_ICON_R * settings.widget_icon_scale(crate::config::WidgetKind::Volume) * render_scale
}

/// Ancho que necesita el contenido del botón de volumen con esa etiqueta.
/// `text_px` es el de `widget_text_px` e `icon_r` el de `volume_icon_r`: el
/// reparto y el dibujo comparten los dos.
pub(crate) fn volume_content_len(label: &str, render_scale: f32, text_px: f32, icon_r: f32) -> f32 {
    icon_r * 2.0
        + VOLUME_ICON_GAP * render_scale
        + volume_label_len(label, text_px)
        + VOLUME_PAD * 2.0 * render_scale
}

// ----- esquinas redondeadas solo en el lado opuesto al borde anclado -----
fn edge_rounded_rect_path(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    edge: DockEdge,
) -> tiny_skia::Path {
    let r = r.min(w / 2.0).min(h / 2.0);
    // (tl, tr, br, bl)
    let (tl, tr, br, bl) = match edge {
        DockEdge::Top => (0.0, 0.0, r, r),
        DockEdge::Bottom => (r, r, 0.0, 0.0),
        DockEdge::Left => (0.0, r, r, 0.0),
        DockEdge::Right => (r, 0.0, 0.0, r),
    };
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(x + tl, y);
    pb.line_to(x + w - tr, y);
    pb.quad_to(x + w, y, x + w, y + tr);
    pb.line_to(x + w, y + h - br);
    pb.quad_to(x + w, y + h, x + w - br, y + h);
    pb.line_to(x + bl, y + h);
    pb.quad_to(x, y + h, x, y + h - bl);
    pb.line_to(x, y + tl);
    pb.quad_to(x, y, x + tl, y);
    pb.close();
    pb.finish().unwrap()
}

/// Factor del blob compacto: la isla mide esto por el GROSOR del dock (con 26 px
/// de grosor son ~68 px de largo). Es el piso de `reveal_len`: el dock oculto no
/// colapsa a la nada, colapsa a la isla.
pub(crate) const ISLAND_COMPACT: f32 = 2.6;

/// Rect de la cápsula del dock durante la animación de aparición, en píxeles
/// físicos. El eje largo va de `compact` (la isla: ver `island_slot`) al dock entero
/// **desde el centro** y el otro queda completo, así que la cápsula sigue pegada al
/// borde de la pantalla: eso es lo que la hace parecer un notch y no un panel
/// flotando.
///
/// Es la ÚNICA cuenta de la forma —la usan `reveal_mask` y `draw_island`—, así que
/// el recorte, el fondo y el estado compacto no se pueden despegar (trampa 10).
///
/// `compact` es un parámetro y no una constante porque la isla crece para que entre
/// su actividad (Media pide ~104 px, el volumen ~60): con un largo fijo, al título
/// de Media le cortaría las letras a la mitad. Sin actividad va 0 y el dock colapsa
/// a nada (transparente), como antes de la isla.
pub(crate) fn reveal_capsule(
    w: f32,
    h: f32,
    vertical: bool,
    compact: f32,
    reveal: f32,
    shift: f32,
) -> (f32, f32, f32, f32) {
    // ----- `shift` corre la cápsula a lo largo del eje: es lo que lleva la isla a la
    // altura del indicador de workspaces (y con eso el dock no salta al revelarse) -----
    if vertical {
        let len = reveal_len(h, compact, reveal);
        (0.0, (h - len) / 2.0 + shift, w, len)
    } else {
        let len = reveal_len(w, compact, reveal);
        ((w - len) / 2.0 + shift, 0.0, len, h)
    }
}

/// Largo del eje largo de la cápsula: en 0 es la isla (`compact`) y en 1 el dock
/// entero. La curva es la que ya usan las otras animaciones.
pub(crate) fn reveal_len(max_len: f32, compact: f32, reveal: f32) -> f32 {
    let compact = compact.clamp(0.0, max_len);
    compact + (max_len - compact) * ease_out(reveal.clamp(0.0, 1.0))
}

/// Máscara de la cápsula en el estado `reveal`. Misma forma que el fondo del dock
/// (`edge_rounded_rect_path`: en el borde las dos esquinas van rectas, del otro lado
/// redondeadas).
fn reveal_mask(
    dock: &Dock,
    render_scale: f32,
    compact: f32,
    reveal: f32,
    w: f32,
    h: f32,
    shift: f32,
) -> Option<tiny_skia::Mask> {
    let s = &dock.config.settings;
    let (x, y, cw, ch) = reveal_capsule(w, h, dock.is_vertical(), compact, reveal, shift);
    let path = edge_rounded_rect_path(
        x,
        y,
        cw.max(0.0),
        ch.max(0.0),
        s.corner_radius * render_scale,
        s.dock_edge,
    );
    let mut mask = tiny_skia::Mask::new(w as u32, h as u32)?;
    mask.fill_path(
        &path,
        tiny_skia::FillRule::Winding,
        true,
        Transform::identity(),
    );
    Some(mask)
}

/// Fondo (y borde) de una cápsula: el MISMO dibujo para el dock entero y para la
/// isla compacta, así el color, el redondeado y el borde no se pueden despegar.
fn draw_capsule(pixmap: &mut Pixmap, dock: &Dock, render_scale: f32, rect: (f32, f32, f32, f32)) {
    let s = &dock.config.settings;
    let (x, y, w, h) = rect;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let path = edge_rounded_rect_path(x, y, w, h, s.corner_radius * render_scale, s.dock_edge);
    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(
        s.panel_r,
        s.panel_g,
        s.panel_b,
        (255.0 * s.transparency) as u8,
    );
    bg_paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &bg_paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    if s.border_width > 0.0 {
        let mut border_paint = Paint::default();
        border_paint.set_color_rgba8(s.accent_r, s.accent_g, s.accent_b, 200);
        border_paint.anti_alias = true;
        let stroke = tiny_skia::Stroke {
            width: s.border_width * render_scale,
            ..Default::default()
        };
        pixmap.stroke_path(&path, &border_paint, &stroke, Transform::identity(), None);
    }
}

/// Rect (coordenadas LÓGICAS de la superficie) del blob de la isla: es la región que
/// el dock **oculto** deja activa, así que el reveal sólo lo dispara el puntero cuando
/// cae en la isla. `None` = no hay isla.
///
/// Sale de `island_plan` + `reveal_capsule`, las MISMAS que dibujan, y con
/// `render_scale = 1.0` a propósito: la `wl_region` va en coordenadas de superficie, no
/// en píxeles físicos (trampa 10 otra vez).
pub(crate) fn island_blob_region(
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    ws_split: f32,
) -> Option<(i32, i32, i32, i32)> {
    let plan = island_plan(dock, widgets, tray_count, 1.0, ws_split);
    if plan.items.is_empty() {
        return None;
    }
    let (w, h) = dock.base_size();
    // ----- el desplazamiento al indicador viaja con el split (en 0 la isla está
    // centrada, en 1 está sobre el widget): así el movimiento es parte de la misma
    // animación de apertura -----
    let shift = island_ws_shift(dock, widgets, tray_count) * ws_split;
    let blob = reveal_capsule(
        w as f32,
        h as f32,
        dock.is_vertical(),
        plan.compact,
        0.0,
        shift,
    );
    Some((
        blob.0.floor() as i32,
        blob.1.floor() as i32,
        blob.2.ceil() as i32,
        blob.3.ceil() as i32,
    ))
}

/// Qué muestra la isla compacta, en orden: la **hora** (con la fecha, es el widget de
/// reloj tal cual) y la **batería**. Las dos mitades del compacto del iPhone.
///
/// Va una LISTA y no una sola actividad porque la isla las muestra pegadas a lo largo
/// de la cápsula. Un `Vec` vacío = no hay nada que contar y el dock oculto queda
/// transparente.
///
/// El **volumen queda en el dock** a propósito (pedido: ahí tiene su pastilla y su
/// panel) y **Media no se muestra**: necesitaría el widget `Media` colocado para
/// tener dato (los `refresh_*` están gateados por el widget) y hoy no lo está. Si
/// vuelve, es una línea acá: `if widgets.media…is_some_and(|m| m.playing)`.
///
/// El dato lo leen los `refresh_*`, que ya están gateados por el widget colocado: la
/// isla no prende ninguna lectura nueva.
pub(crate) fn island_activities(
    widgets: &WidgetSnapshot,
    ws_split: f32,
) -> Vec<crate::config::WidgetKind> {
    use crate::config::WidgetKind as K;
    let mut out = Vec::with_capacity(4);
    // ----- la grabación es la actividad MÁS viva que hay: va primera, aunque su widget
    // no esté colocado en la barra (el dato es un `stat`, no un spawn) -----
    if widgets.recording.is_some() {
        out.push(K::Recording);
    }
    if !widgets.time.is_empty() {
        out.push(K::Clock);
    }
    // ----- al cambiar de workspace la isla se ABRE EN DOS y el indicador entra en el
    // medio (`ws_split` = cuánto se abrió, 0 = cerrada). Es una actividad más del
    // plan, así que sale gratis del mismo reparto y el mismo motor de morph. -----
    if ws_split > 0.0 {
        out.push(K::Workspaces);
    }
    if widgets.battery.is_some() {
        out.push(K::Battery);
    }
    out
}

/// La isla compacta: el estado que se ve MIENTRAS el dock está oculto. Es la misma
/// cápsula del dock en `reveal = 0` (el blob que arma `island_plan`) pero con las
/// actividades pegadas a lo largo en vez del reparto entero de widgets.
///
/// El contenido se dibuja en un pixmap aparte y se pega recortado por el blob, así
/// un título largo (Media) no se sale de la cápsula. Devuelve true si algún widget
/// animó este frame (hoy sólo Media, si el marquee está avanzando).
#[allow(clippy::too_many_arguments)]
pub fn draw_island(
    pixmap: &mut Pixmap,
    dock: &Dock,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    tray: &[crate::tray::TrayIcon],
    marquee: &mut MarqueeState,
    render_scale: f32,
    ws_split: f32,
) -> bool {
    pixmap.fill(Color::TRANSPARENT);
    let (w, h) = (pixmap.width() as f32, pixmap.height() as f32);
    let plan = island_plan(dock, widgets, tray.len(), render_scale, ws_split);
    if plan.items.is_empty() {
        return false;
    }
    let vertical = dock.is_vertical();
    // ----- la isla entera se corre a la altura del indicador de workspaces mientras
    // el split está abierto (físico: el blob se dibuja en píxeles del buffer) -----
    let shift = island_ws_shift(dock, widgets, tray.len()) * ws_split * render_scale;
    let blob = reveal_capsule(w, h, vertical, plan.compact, 0.0, shift);
    draw_capsule(pixmap, dock, render_scale, blob);
    let Some(mut content) = Pixmap::new(pixmap.width(), pixmap.height()) else {
        return false;
    };
    // ----- si el tema cambió, la pista del marquee arranca de cero (misma regla que
    // la barra: `sync_marquee_key`) -----
    layout::sync_marquee_key(marquee, widgets);
    let (blob_len, blob_start) = if vertical {
        (blob.3, blob.1)
    } else {
        (blob.2, blob.0)
    };
    let mut animating = false;
    for (kind, start, len) in island_spans(&plan, blob_start, blob_len) {
        let area = if vertical {
            (blob.0, start, blob.2, len)
        } else {
            (start, blob.1, len, blob.3)
        };
        animating |= layout::draw_one_widget(
            &mut content,
            dock,
            icon_cache,
            text_cache,
            widgets,
            tray,
            marquee,
            kind,
            area,
            render_scale,
        );
    }
    if let Some(mask) = reveal_mask(dock, render_scale, plan.compact, 0.0, w, h, shift) {
        pixmap.draw_pixmap(
            0,
            0,
            content.as_ref(),
            &tiny_skia::PixmapPaint::default(),
            Transform::identity(),
            Some(&mask),
        );
    }
    animating
}

/// Dónde va cada actividad dentro del blob, en píxeles físicos y **sobre el eje
/// largo**: `(kind, inicio, largo)`. Arrancan después del relleno (adentro de esa
/// franja el blob se angosta por el redondeo y la máscara le cortaba el borde: la
/// batería, que es el icono más ancho, se veía mordida) y quedan centradas.
///
/// Es la ÚNICA cuenta de la posición de cada actividad: la usan el dibujo de la isla
/// y su hit test, así que no se pueden despegar (trampa 10).
fn island_spans(
    plan: &IslandPlan,
    blob_start: f32,
    blob_len: f32,
) -> Vec<(crate::config::WidgetKind, f32, f32)> {
    let items_w: f32 = plan.items.iter().map(|(_, len)| len).sum::<f32>()
        + plan.gap * plan.items.len().saturating_sub(1) as f32;
    let band = (blob_len - 2.0 * plan.pad).max(0.0);
    let mut cursor = blob_start + plan.pad + (band - items_w).max(0.0) / 2.0;
    plan.items
        .iter()
        .map(|(kind, len)| {
            let span = (*kind, cursor, *len);
            cursor += len + plan.gap;
            span
        })
        .collect()
}

/// Dibuja el dock, con `reveal` = animación de aparición (1.0 = dock entero).
///
/// En 1.0 —el estado en reposo, o sea el 99% de los frames— va derecho al camino de
/// siempre: ni máscara ni pixmap de más. Animando, el dock completo se dibuja en un
/// pixmap propio y se pega **recortado por la cápsula que crece**, así el contenido
/// aparece a medida que la cápsula se abre (que es el efecto de la isla).
#[allow(clippy::too_many_arguments)]
pub fn draw(
    pixmap: &mut Pixmap,
    dock: &Dock,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    tray: &[crate::tray::TrayIcon],
    marquee: &mut MarqueeState,
    advance_marquee: bool,
    advance_ws: bool,
    render_scale: f32,
    reveal: f32,
    ws_split: f32,
) -> bool {
    let reveal = reveal.clamp(0.0, 1.0);
    if reveal >= 0.999 {
        return draw_full(
            pixmap,
            dock,
            icon_cache,
            text_cache,
            widgets,
            tray,
            marquee,
            advance_marquee,
            advance_ws,
            render_scale,
        );
    }
    let Some(mut full) = Pixmap::new(pixmap.width(), pixmap.height()) else {
        // ----- sin memoria para el pixmap de la animación se dibuja el dock entero:
        // es preferible un frame sin animación a una superficie vacía -----
        return draw_full(
            pixmap,
            dock,
            icon_cache,
            text_cache,
            widgets,
            tray,
            marquee,
            advance_marquee,
            advance_ws,
            render_scale,
        );
    };
    let needs_marquee = draw_full(
        &mut full,
        dock,
        icon_cache,
        text_cache,
        widgets,
        tray,
        marquee,
        advance_marquee,
        advance_ws,
        render_scale,
    );
    pixmap.fill(Color::TRANSPARENT);
    // ----- el piso de la animación es la isla de la actividad de turno (mismo
    // `island_slot` que dibuja `draw_island`): sin actividad va 0 y el dock colapsa a
    // nada, como antes de la isla. -----
    let compact = island_plan(dock, widgets, tray.len(), render_scale, ws_split).compact;
    // ----- el piso del colapso también usa el desplazamiento: si el split está
    // abierto, el dock se encoge HACIA el indicador, no al centro -----
    let shift = island_ws_shift(dock, widgets, tray.len()) * ws_split * render_scale;
    if let Some(mask) = reveal_mask(
        dock,
        render_scale,
        compact,
        reveal,
        pixmap.width() as f32,
        pixmap.height() as f32,
        shift,
    ) {
        // ----- la opacidad sube con el largo: el contenido entra desvanecido en vez
        // de aparecer cortado de golpe -----
        let paint = tiny_skia::PixmapPaint {
            opacity: reveal,
            ..Default::default()
        };
        pixmap.draw_pixmap(
            0,
            0,
            full.as_ref(),
            &paint,
            Transform::identity(),
            Some(&mask),
        );
    }
    needs_marquee
}

#[allow(clippy::too_many_arguments)]
fn draw_full(
    pixmap: &mut Pixmap,
    dock: &Dock,
    icon_cache: &mut IconCache,
    text_cache: &mut TextCache,
    widgets: &WidgetSnapshot,
    tray: &[crate::tray::TrayIcon],
    marquee: &mut MarqueeState,
    advance_marquee: bool,
    advance_ws: bool,
    render_scale: f32,
) -> bool {
    pixmap.fill(Color::TRANSPARENT);

    let s = &dock.config.settings;
    let (base_w, base_h) = dock.base_size();
    let w = base_w as f32 * render_scale;
    let h = base_h as f32 * render_scale;

    // ----- blur via layerrule -----
    draw_capsule(pixmap, dock, render_scale, (0.0, 0.0, w, h));

    if dock.icons.is_empty() {
        return draw_widgets(
            pixmap,
            dock,
            icon_cache,
            text_cache,
            widgets,
            tray,
            marquee,
            advance_marquee,
            advance_ws,
            render_scale,
        );
    }

    let cache_size = (s.icon_size * s.dock_scale * s.magnify_scale * render_scale).ceil() as u32;
    let dragging = dock.dragging_index;
    let layout = dock.layout();
    let (edx, edy) = dock.elevate_dir();

    let order = (0..dock.icons.len())
        .filter(|i| Some(*i) != dragging)
        .chain(dragging);

    for i in order {
        let icon = &dock.icons[i];
        let (cx, cy, drawn_w) = layout[i];
        let elevated = Some(i) == dragging;
        let cx = if elevated { cx + edx * 10.0 } else { cx };
        let cy = if elevated { cy + edy * 10.0 } else { cy };

        let Some(icon_pixmap) = icon_cache.get(&icon.app.icon, cache_size.max(1)) else {
            draw_placeholder(
                pixmap,
                &icon.app.name,
                cx * render_scale,
                cy * render_scale,
                drawn_w * render_scale,
            );
            continue;
        };

        let drawn_w = drawn_w * render_scale;
        let cx = cx * render_scale;
        let cy = cy * render_scale;

        if elevated {
            let mut shadow_paint = Paint::default();
            shadow_paint.set_color_rgba8(0, 0, 0, 90);
            shadow_paint.anti_alias = true;
            let shadow_r = drawn_w * 0.5;
            let back_x = cx - edx * drawn_w * 0.32;
            let back_y = cy - edy * drawn_w * 0.32;
            let (sw, sh) = if edx.abs() > 0.5 {
                (shadow_r, shadow_r * 2.0)
            } else {
                (shadow_r * 2.0, shadow_r * 0.5)
            };
            if let Some(rect) = Rect::from_xywh(back_x - sw / 2.0, back_y - sh / 2.0, sw, sh) {
                let path = rounded_rect_path(
                    rect.x(),
                    rect.y(),
                    rect.width(),
                    rect.height(),
                    sw.min(sh) * 0.4,
                );
                pixmap.fill_path(
                    &path,
                    &shadow_paint,
                    tiny_skia::FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }

        let scale = drawn_w / icon_pixmap.width() as f32;
        let transform = Transform::from_translate(cx - drawn_w / 2.0, cy - drawn_w / 2.0)
            .pre_scale(scale, scale);
        let icon_paint = tiny_skia::PixmapPaint::default();
        pixmap.draw_pixmap(
            0,
            0,
            icon_pixmap.as_ref().as_ref(),
            &icon_paint,
            transform,
            None,
        );
    }
    false
}

pub(crate) struct WidgetColors<'a> {
    accent: (u8, u8, u8, u8),
    text_rgb: (u8, u8, u8),
    text_color: &'a str,
}

/// Paleta derivada de los ajustes. La comparten el dock y los HUDs para que el
/// color sea idéntico sin duplicar la lógica de mezcla con el acento.
pub(super) struct WidgetPalette {
    pub(super) accent: (u8, u8, u8, u8),
    pub(super) text_rgb: (u8, u8, u8),
    pub(super) text_color: String,
}

pub(super) fn widget_palette(s: &crate::config::DockSettings) -> WidgetPalette {
    let blend =
        |base: u8, tint: u8, frac: f32| (base as f32 * (1.0 - frac) + tint as f32 * frac) as u8;
    let text_rgb = if s.custom_theme {
        (s.text_r, s.text_g, s.text_b)
    } else {
        (
            blend(s.text_r, s.accent_r, 0.45),
            blend(s.text_g, s.accent_g, 0.45),
            blend(s.text_b, s.accent_b, 0.45),
        )
    };
    WidgetPalette {
        accent: (s.accent_r, s.accent_g, s.accent_b, 255),
        text_rgb,
        text_color: format!("#{:02x}{:02x}{:02x}", text_rgb.0, text_rgb.1, text_rgb.2),
    }
}

/// HUD al cambiar de workspace: mismo panel y misma posición que el dock, pero
/// con un único contenido: el indicador de workspaces. El contenido se dibuja con
/// el mismo factor que el widget del dock (`render_scale * widget_scale`), así que
/// los puntos no cambian de tamaño cuando el dock se oculta y aparece el HUD.
#[allow(clippy::too_many_arguments)]
pub fn draw_ws_flash(
    pixmap: &mut Pixmap,
    dock: &Dock,
    widgets: &WidgetSnapshot,
    tray_count: usize,
    marquee: &mut MarqueeState,
    advance: bool,
    render_scale: f32,
) -> bool {
    let s = &dock.config.settings;
    let is_vertical = dock.is_vertical();
    // ----- el indicador va en el rectángulo que el reparto del dock le da al widget
    // de Workspaces (`hit_layout`, en coordenadas LÓGICAS; acá se pasan a físicas).
    // La superficie del HUD mide y se ancla IGUAL que la del dock, así que el punto
    // queda donde estaba y sólo se dibuja la pastilla que lo enmarca. Centrarlo en un
    // panel propio con `dock_align` era el bug: con zonas desparejas (p. ej. 6
    // widgets a la izquierda y el tray a la derecha) el punto saltaba al ocultarse el
    // dock (trampa 1: una sola definición del reparto). -----
    let Some(r) = layout::hit_layout(dock, widgets, tray_count)
        .into_iter()
        .find(|r| r.kind == crate::config::WidgetKind::Workspaces)
    else {
        return false;
    };
    let (rx, ry, rw, rh) = (
        r.x * render_scale,
        r.y * render_scale,
        r.w * render_scale,
        r.h * render_scale,
    );
    // ----- fondo: la pastilla del dock, pero sólo alrededor del indicador -----
    let (px, py, pw, ph) = ws_flash_pill(&r, is_vertical);
    let path = rounded_rect_path(
        px * render_scale,
        py * render_scale,
        pw * render_scale,
        ph * render_scale,
        s.corner_radius * render_scale,
    );
    let mut bg = Paint::default();
    bg.set_color_rgba8(
        s.panel_r,
        s.panel_g,
        s.panel_b,
        (255.0 * s.transparency) as u8,
    );
    bg.anti_alias = true;
    pixmap.fill_path(
        &path,
        &bg,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    // ----- sólo el indicador de workspaces, en el rectángulo del dock -----
    let palette = widget_palette(s);
    let colors = WidgetColors {
        accent: palette.accent,
        text_rgb: palette.text_rgb,
        text_color: &palette.text_color,
    };
    draw_workspaces_widget(
        pixmap,
        &widgets.workspaces,
        marquee,
        advance,
        rx,
        ry,
        rw,
        rh,
        // ----- mismo factor que el dock (`draw_widgets` también multiplica por
        // `widget_scale`): el rectángulo ya viene en píxeles físicos. -----
        render_scale * s.widget_scale,
        &colors,
        is_vertical,
    )
}

fn draw_text_rotated(
    pixmap: &mut Pixmap,
    text_cache: &mut TextCache,
    text: &str,
    cx: f32,
    cy: f32,
    size: f32,
    color: &str,
    weight: u16,
) {
    let Some(glyphs) = text_cache.get(text, size, color, weight) else {
        return;
    };
    let iw = glyphs.width() as f32;
    let ih = glyphs.height() as f32;
    let paint = tiny_skia::PixmapPaint::default();
    let transform = Transform::from_translate(-iw / 2.0, -ih / 2.0)
        .post_rotate(-90.0)
        .post_translate(cx, cy);
    pixmap.draw_pixmap(0, 0, glyphs.as_ref().as_ref(), &paint, transform, None);
}

fn draw_text_rotated_family(
    pixmap: &mut Pixmap,
    text_cache: &mut TextCache,
    text: &str,
    cx: f32,
    cy: f32,
    size: f32,
    color: &str,
    weight: u16,
    family: &'static str,
) {
    let Some(glyphs) = text_cache.get_with_family(text, size, color, weight, family) else {
        return;
    };
    let iw = glyphs.width() as f32;
    let ih = glyphs.height() as f32;
    let paint = tiny_skia::PixmapPaint::default();
    let transform = Transform::from_translate(-iw / 2.0, -ih / 2.0)
        .post_rotate(-90.0)
        .post_translate(cx, cy);
    pixmap.draw_pixmap(0, 0, glyphs.as_ref().as_ref(), &paint, transform, None);
}

pub(crate) fn text_width_estimate_render(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.64
}

/// Base única de letra de la barra, en px lógicos (antes de escalas). Todos
/// los widgets miden y dibujan su texto con este mismo tamaño: el reloj iba a
/// 11/12 y la batería a 9/9.5, y por eso se veía desigual. El ajuste
/// "Font Size" multiplica esta base sin tocar iconos ni paddings.
pub(crate) const WIDGET_TEXT_PX: f32 = 8.5;

/// Tamaño de letra ya escalado para medir Y dibujar. Medida y dibujo tienen
/// que usar este mismo valor (trampa 12): la pastilla reserva lo que el texto
/// ocupa. El `kind` importa porque el editor del panel puede escalar un widget
/// solo, encima del `font_scale` general.
pub(crate) fn widget_text_px(
    settings: &crate::config::DockSettings,
    kind: crate::config::WidgetKind,
    render_scale: f32,
) -> f32 {
    WIDGET_TEXT_PX * settings.widget_font_scale(kind) * render_scale
}

/// Cuadrado de color derivado del nombre, para un ícono que no está en el tema.
/// Lo usan las apps fijas del dock (una app sin ícono dejaba un hueco) y las
/// tarjetas del launcher (cuatro tiles vacíos en la grilla del panel vertical).
pub(crate) fn draw_placeholder(pixmap: &mut Pixmap, name: &str, cx: f32, cy: f32, size: f32) {
    let hash = name
        .bytes()
        .fold(37u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    let hue_r = 90 + (hash % 120) as u8;
    let hue_g = 90 + ((hash >> 8) % 120) as u8;
    let hue_b = 90 + ((hash >> 16) % 120) as u8;

    let mut paint = Paint::default();
    paint.set_color_rgba8(hue_r, hue_g, hue_b, 230);
    paint.anti_alias = true;
    let path = rounded_rect_path(cx - size / 2.0, cy - size / 2.0, size, size, size * 0.22);
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
}

#[cfg(test)]
mod reveal_tests {
    use super::*;

    /// El piso de la animación es la isla compacta, no la nada: el dock oculto se
    /// encoge a un blob del alto del grosor y de ahí crece al dock entero.
    #[test]
    fn la_capsula_va_del_blob_compacto_al_dock_entero() {
        let (_, y0, w0, len0) = reveal_capsule(26.0, 610.0, true, 26.0 * ISLAND_COMPACT, 0.0, 0.0);
        assert_eq!(
            (w0, len0),
            (26.0, 26.0 * ISLAND_COMPACT),
            "en 0 tiene que quedar la isla compacta: el ancho completo y el blob"
        );
        assert!(
            len0 < 610.0 * 0.2,
            "el blob compacto no puede pasar de un quinto del dock: {len0}"
        );
        assert_eq!(y0, (610.0 - len0) / 2.0, "el blob va centrado");
        let (x1, y1, w1, len1) = reveal_capsule(26.0, 610.0, true, 26.0 * ISLAND_COMPACT, 1.0, 0.0);
        assert_eq!(
            (x1, y1, w1, len1),
            (0.0, 0.0, 26.0, 610.0),
            "en 1 tiene que ser la superficie entera (el camino sin animación)"
        );
    }

    #[test]
    fn la_capsula_se_pega_al_borde_y_se_centra() {
        // ----- vertical (dock a la izquierda): el eje corto va completo —pegado al
        // borde de la pantalla, que es lo que la hace parecer un notch— y el largo
        // se centra -----
        let (x, y, w, h) = reveal_capsule(26.0, 610.0, true, 26.0 * ISLAND_COMPACT, 0.5, 0.0);
        assert_eq!(
            (x, w),
            (0.0, 26.0),
            "el eje corto no se encoge: si se encogiera, la cápsula se despegaría del borde"
        );
        assert!(h > 0.0 && h < 610.0, "largo a medias: {h}");
        assert!(
            (y - (610.0 - h) / 2.0).abs() < 0.001,
            "no está centrada: y={y}"
        );
        // ----- horizontal: los ejes al revés -----
        let (x, y, w, h) = reveal_capsule(610.0, 26.0, false, 26.0 * ISLAND_COMPACT, 0.5, 0.0);
        assert_eq!((y, h), (0.0, 26.0));
        assert!(
            (x - (610.0 - w) / 2.0).abs() < 0.001,
            "no está centrada: x={x}"
        );
    }

    #[test]
    fn el_largo_crece_todos_los_pasos() {
        let mut antes = -1.0;
        for i in 0..=10 {
            let len = reveal_len(610.0, 26.0 * ISLAND_COMPACT, i as f32 / 10.0);
            assert!(
                len > antes,
                "el largo retrocedió en el paso {i}: {len} < {antes}"
            );
            antes = len;
        }
    }

    /// Snapshot vacío: `island_content` sólo mira media/volumen/batería, pero el
    /// struct no tiene `Default` (el repo arma sus fixtures campo por campo).
    fn vacio() -> WidgetSnapshot {
        use crate::widgets::{KbLayout, NetworkInfo};
        WidgetSnapshot {
            time: String::new(),
            date: String::new(),
            battery: None,
            media: None,
            bluetooth: None,
            workspaces: Vec::new(),
            cpu: None,
            ram: None,
            ram_gb: None,
            volume: None,
            mic: None,
            recording: None,
            network: NetworkInfo {
                label: String::new(),
                online: false,
            },
            kblayout: KbLayout {
                short: String::new(),
            },
            custom_texts: Vec::new(),
            custom_last_polls: Vec::new(),
        }
    }

    /// La isla muestra la **hora** (con la fecha) y la **batería**. El volumen queda en
    /// el dock a propósito y Media no se muestra (necesitaría el widget `Media`
    /// colocado: los `refresh_*` están gateados por el widget).
    #[test]
    fn la_isla_muestra_la_hora_y_la_bateria() {
        use crate::config::WidgetKind;
        use crate::widgets::{BatteryState, MediaInfo};
        let base = vacio();
        assert!(
            island_activities(&base, 0.0).is_empty(),
            "sin hora ni batería no hay isla"
        );

        let mut con_hora = vacio();
        con_hora.time = "10:37".into();
        con_hora.date = "17-Sept".into();
        assert_eq!(island_activities(&con_hora, 0.0), vec![WidgetKind::Clock]);

        let mut con_bat = vacio();
        con_bat.battery = Some((39, BatteryState::Discharging));
        assert_eq!(island_activities(&con_bat, 0.0), vec![WidgetKind::Battery]);

        // ----- las dos, la hora primero -----
        let mut las_dos = vacio();
        las_dos.time = "10:37".into();
        las_dos.date = "17-Sept".into();
        las_dos.battery = Some((39, BatteryState::Discharging));
        assert_eq!(
            island_activities(&las_dos, 0.0),
            vec![WidgetKind::Clock, WidgetKind::Battery]
        );

        // ----- el volumen no va a la isla: tiene su pastilla y su panel en el dock -----
        let mut con_vol = vacio();
        con_vol.volume = Some((100, false));
        assert!(island_activities(&con_vol, 0.0).is_empty());

        // ----- y media tampoco, ni siquiera sonando -----
        let mut con_media = vacio();
        con_media.media = Some(MediaInfo {
            title: "Tema".into(),
            playing: true,
            art_path: None,
        });
        assert!(island_activities(&con_media, 0.0).is_empty());
    }

    /// La isla crece para que entren sus actividades (hora + batería), pegadas y **con
    /// el relleno de los extremos**: adentro de la franja del radio el blob se angosta y
    /// la máscara le cortaba el borde (se veía en la batería, que es el icono más
    /// ancho).
    #[test]
    fn la_isla_crece_para_que_entren_las_actividades() {
        use crate::widgets::BatteryState;
        let dock = dock_de(crate::config::DockEdge::Left);

        // ----- la franja útil (blob menos el relleno de los dos extremos) tiene que
        // alcanzar para todo lo que el plan quiere dibujar: es el contrato que evita
        // el borde mordido -----
        let franja_util = |plan: &layout::IslandPlan| {
            let items: f32 = plan.items.iter().map(|(_, len)| len).sum();
            let gaps = plan.gap * plan.items.len().saturating_sub(1) as f32;
            (plan.compact - 2.0 * plan.pad) - (items + gaps)
        };

        let mut con_bat = vacio();
        con_bat.battery = Some((39, BatteryState::Discharging));
        let plan = island_plan(&dock, &con_bat, 0, 1.0, 0.0);
        assert_eq!(plan.items.len(), 1);
        assert!(plan.pad > 0.0, "el relleno sale del radio");
        assert!(
            franja_util(&plan) >= -0.5,
            "con una actividad la franja útil queda corta: {}",
            franja_util(&plan)
        );

        // ----- con la hora van DOS -----
        let mut las_dos = vacio();
        las_dos.time = "10:37".into();
        las_dos.date = "17-Sept".into();
        las_dos.battery = Some((39, BatteryState::Discharging));
        let plan_dos = island_plan(&dock, &las_dos, 0, 1.0, 0.0);
        assert_eq!(plan_dos.items.len(), 2);
        assert_eq!(plan_dos.items[0].0, crate::config::WidgetKind::Clock);
        assert!(
            franja_util(&plan_dos) >= -0.5,
            "con dos actividades la franja útil queda corta: {}",
            franja_util(&plan_dos)
        );
        assert!(
            plan_dos.compact > plan.compact,
            "y el blob es más largo que con la batería sola: {} contra {}",
            plan_dos.compact,
            plan.compact
        );
        assert!(
            plan_dos.compact <= dock.base_size().1 as f32,
            "y nunca más largo que el dock: {}",
            plan_dos.compact
        );
        // ----- sin actividad no hay isla (y no se aplica el piso) -----
        let vacia = island_plan(&dock, &vacio(), 0, 1.0, 0.0);
        assert!(vacia.items.is_empty());
        assert_eq!(vacia.compact, 0.0);
    }

    /// Al cambiar de workspace la isla se **parte**: el indicador entra AL MEDIO y sus
    /// dos gaps abren con él, así que en 0 no hay salto (la isla cerrada mide lo mismo
    /// que antes de la feature) y en 1 mide la suma de las tres actividades.
    #[test]
    fn la_isla_se_parte_en_dos_con_el_indicador_al_medio() {
        use crate::config::WidgetKind;
        use crate::widgets::{BatteryState, WorkspaceInfo};
        let mut dock = dock_de(crate::config::DockEdge::Left);
        // ----- el largo real de la barra: en el test nadie llamó a
        // `sync_widget_bar_len`, así que sin esto la superficie queda en el mínimo y
        // el plan se corta antes de la batería -----
        dock.widget_bar_content_len = 700.0;
        let mut w = vacio();
        w.time = "10:37".into();
        w.date = "17-Sept".into();
        w.battery = Some((39, BatteryState::Discharging));
        w.workspaces = (1..=3)
            .map(|id| WorkspaceInfo {
                id,
                active: id == 2,
                empty: false,
                urgent: false,
                output: String::new(),
            })
            .collect();

        let cerrada = island_plan(&dock, &w, 0, 1.0, 0.0);
        assert_eq!(cerrada.items.len(), 2, "cerrada: hora + batería");
        assert_eq!(cerrada.items[0].0, WidgetKind::Clock);
        assert_eq!(cerrada.items[1].0, WidgetKind::Battery);

        let abierta = island_plan(&dock, &w, 0, 1.0, 1.0);
        assert_eq!(abierta.items.len(), 3);
        assert_eq!(
            abierta.items[1].0,
            WidgetKind::Workspaces,
            "el indicador va en el medio"
        );
        assert!(
            abierta.compact > cerrada.compact,
            "abrirla suma largo: {} contra {}",
            abierta.compact,
            cerrada.compact
        );

        // ----- a medias: el indicador mide la mitad y la isla queda entre las dos -----
        let media = island_plan(&dock, &w, 0, 1.0, 0.5);
        assert!(
            (media.items[1].1 - abierta.items[1].1 / 2.0).abs() < 0.01,
            "el indicador a medias mide la mitad: {} contra {}",
            media.items[1].1,
            abierta.items[1].1
        );
        assert!(
            cerrada.compact < media.compact && media.compact < abierta.compact,
            "y el largo queda entre las dos: {} / {} / {}",
            cerrada.compact,
            media.compact,
            abierta.compact
        );
    }

    /// La isla entera se corre a la altura del **indicador de workspaces**: sin eso,
    /// al revelarse el dock el indicador salta de lugar (era el bug del HUD centrado).
    /// El caso es desparejo a propósito: con el indicador en el centro el shift sería 0
    /// y el test no probaría nada.
    #[test]
    fn la_isla_se_corre_al_lugar_del_indicador() {
        use crate::config::{WidgetKind, WidgetPlacement, WidgetSlot};
        let mut config = crate::config::Config::default();
        config.settings.dock_edge = crate::config::DockEdge::Left;
        config.settings.widgets = vec![
            WidgetPlacement {
                kind: WidgetKind::Clock,
                slot: WidgetSlot::Left,
            },
            WidgetPlacement {
                kind: WidgetKind::Workspaces,
                slot: WidgetSlot::Middle,
            },
            WidgetPlacement {
                kind: WidgetKind::Battery,
                slot: WidgetSlot::Right,
            },
        ];
        let dock = Dock::new(config);
        let mut w = vacio();
        w.time = "10:37".into();
        w.battery = Some((39, crate::widgets::BatteryState::Discharging));
        w.workspaces = (1..=3)
            .map(|id| crate::widgets::WorkspaceInfo {
                id,
                active: id == 1,
                empty: false,
                urgent: false,
                output: String::new(),
            })
            .collect();
        let shift = layout::island_ws_shift(&dock, &w, 0);
        assert!(
            shift.abs() > 1.0,
            "con el indicador fuera del centro el shift no puede ser 0: {shift}"
        );
        // ----- el contrato: centro de la barra + shift == centro del widget -----
        let (bw, bh) = dock.base_size();
        let r = layout::layout_widgets(
            &dock.config.settings,
            &w,
            0,
            true,
            bw as f32,
            bh as f32,
            1.0,
        )
        .into_iter()
        .find(|r| r.kind == WidgetKind::Workspaces)
        .expect("workspaces colocado");
        assert!(
            (bh as f32 / 2.0 + shift - (r.y + r.h / 2.0)).abs() < 0.01,
            "el shift tiene que caer en el centro del widget: {shift} contra {}",
            r.y + r.h / 2.0
        );
    }

    fn fila(mask: &tiny_skia::Mask, y: usize, w: usize) -> usize {
        mask.data()[y * w..(y + 1) * w]
            .iter()
            .filter(|b| **b > 0)
            .count()
    }

    fn columna(mask: &tiny_skia::Mask, x: usize, w: usize, h: usize) -> usize {
        (0..h).filter(|y| mask.data()[y * w + x] > 0).count()
    }

    fn dock_de(edge: crate::config::DockEdge) -> Dock {
        let mut config = crate::config::Config::default();
        config.settings.dock_edge = edge;
        Dock::new(config)
    }

    fn cobertura(mask: &tiny_skia::Mask) -> f32 {
        let d = mask.data();
        d.iter().filter(|b| **b > 0).count() as f32 / d.len() as f32
    }

    /// La máscara es lo que recorta el reveal y la isla: si queda vacía el dock
    /// aparece en blanco (o directamente no aparece) y si cubre todo, no hay
    /// animación. Además el eje corto tiene que ir COMPLETO: si se encogiera junto
    /// con el largo, la cápsula se despegaría del borde y dejaría de parecer un
    /// notch.
    #[test]
    fn la_mascara_recorta_la_capsula() {
        use crate::config::DockEdge;
        let dock = dock_de(DockEdge::Left);
        let (w, h) = (26.0, 610.0);

        // ----- en 0 es el blob compacto: chico, centrado y con el ancho completo -----
        let blob = reveal_mask(&dock, 1.0, 26.0 * ISLAND_COMPACT, 0.0, w, h, 0.0).expect("máscara");
        let cb = cobertura(&blob);
        assert!(
            (0.05..0.15).contains(&cb),
            "en 0 la cápsula es el blob compacto (~7% de la superficie) y quedó en {cb}"
        );
        assert_eq!(
            fila(&blob, 0, w as usize),
            0,
            "el blob no puede llegar al extremo de la superficie"
        );
        assert_eq!(
            fila(&blob, 305, w as usize),
            w as usize,
            "el blob tiene que cubrir el ancho completo (pegado al borde)"
        );

        let llena =
            reveal_mask(&dock, 1.0, 26.0 * ISLAND_COMPACT, 1.0, w, h, 0.0).expect("máscara");
        assert!(
            cobertura(&llena) > 0.95,
            "en 1 la cápsula es el dock entero, y quedó en {}",
            cobertura(&llena)
        );

        let media =
            reveal_mask(&dock, 1.0, 26.0 * ISLAND_COMPACT, 0.5, w, h, 0.0).expect("máscara");
        let c = cobertura(&media);
        // ----- ease_out(0.5) sobre lo que falta del blob al dock entero -----
        assert!(
            (0.75..0.95).contains(&c),
            "la cobertura a medias se fue de rango: {c}"
        );
        assert_eq!(
            fila(&media, 0, w as usize),
            0,
            "el extremo de la superficie tiene que quedar afuera"
        );
        assert_eq!(
            fila(&media, 305, w as usize),
            w as usize,
            "el medio tiene que cubrir el ancho completo (pegado al borde)"
        );
        // ----- horizontal: los ejes al revés -----
        let dock = dock_de(DockEdge::Top);
        let (w, h) = (610.0, 26.0);
        let media =
            reveal_mask(&dock, 1.0, 26.0 * ISLAND_COMPACT, 0.5, w, h, 0.0).expect("máscara");
        assert_eq!(
            columna(&media, 0, w as usize, h as usize),
            0,
            "el extremo izquierdo tiene que quedar afuera"
        );
        assert_eq!(
            columna(&media, 305, w as usize, h as usize),
            h as usize,
            "el centro tiene que cubrir el alto completo (pegado al borde)"
        );
    }
}

#[cfg(test)]
mod volume_pill_tests {
    use super::*;
    use crate::config::{DockSettings, WidgetKind, WidgetOptions};

    fn con_bocina(icon_scale: f32) -> DockSettings {
        let mut s = DockSettings::default();
        s.set_widget_options(
            WidgetKind::Volume,
            WidgetOptions {
                icon_scale: Some(icon_scale),
                ..Default::default()
            },
        );
        s
    }

    /// El default no cambia nada: escala 1.0 es el radio de fábrica.
    #[test]
    fn la_escala_neutra_es_el_radio_de_siempre() {
        assert_eq!(volume_icon_r(&con_bocina(1.0), 1.0), VOLUME_ICON_R);
    }

    /// La bocina y el ancho que le reserva la pastilla salen de la MISMA función,
    /// así que agrandar el símbolo agranda la pastilla en la misma medida. Si
    /// alguien escala sólo el dibujo, el número se sale de la pastilla (es el
    /// mismo tipo de bug que el de `hit_scale`, trampa 12).
    #[test]
    fn la_bocina_y_su_lugar_crecen_juntos() {
        let chico = volume_icon_r(&con_bocina(1.0), 1.0);
        let grande = volume_icon_r(&con_bocina(1.6), 1.0);
        assert!(
            (grande - chico * 1.6).abs() < 0.001,
            "el radio no siguió la escala: {grande} contra {chico}"
        );
        let w_chico = volume_content_len("100", 1.0, 8.5, chico);
        let w_grande = volume_content_len("100", 1.0, 8.5, grande);
        assert!(
            (w_grande - w_chico - 2.0 * (grande - chico)).abs() < 0.001,
            "la pastilla no reservó el doble de radio de más: {w_grande} contra {w_chico}"
        );
    }
}
