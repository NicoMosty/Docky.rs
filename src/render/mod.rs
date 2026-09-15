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
pub(crate) use cpu_ram::{draw_cpu_widget, draw_ram_widget};
pub(crate) use layout::{WidgetRect, percentage_widget_len, text_widget_len};
pub(crate) use media::{draw_media_widget, media_ideal_len};
pub(crate) use power_bluetooth::{draw_bluetooth_icon, draw_power_widget};
pub(crate) use syswidgets::{
    draw_kblayout_widget, draw_network_widget, draw_text_widget, draw_volume_widget,
};
pub(crate) use tray::{draw_tray_widget, tray_geometry};
pub(crate) use workspaces::draw_workspaces_widget;

const ICON_OVERSAMPLE: f32 = 1.0;
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
) -> bool {
    pixmap.fill(Color::TRANSPARENT);

    let s = &dock.config.settings;
    let (base_w, base_h) = dock.base_size();
    let w = base_w as f32 * render_scale;
    let h = base_h as f32 * render_scale;

    // ----- blur via layerrule -----
    let bg_margin = 0.0;
    let bg_path = edge_rounded_rect_path(
        bg_margin,
        bg_margin,
        w - bg_margin * 2.0,
        h - bg_margin * 2.0,
        s.corner_radius * render_scale,
        s.dock_edge,
    );
    let (bg_r, bg_g, bg_b) = (s.panel_r, s.panel_g, s.panel_b);
    let mut bg_paint = Paint::default();
    bg_paint.set_color_rgba8(bg_r, bg_g, bg_b, (255.0 * s.transparency) as u8);
    bg_paint.anti_alias = true;
    pixmap.fill_path(
        &bg_path,
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
        pixmap.stroke_path(
            &bg_path,
            &border_paint,
            &stroke,
            Transform::identity(),
            None,
        );
    }

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

    let cache_size = (s.icon_size * s.dock_scale * s.magnify_scale * render_scale * ICON_OVERSAMPLE)
        .ceil() as u32;
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
/// con un único contenido: el indicador de workspaces.
#[allow(clippy::too_many_arguments)]
pub fn draw_ws_flash(
    pixmap: &mut Pixmap,
    dock: &Dock,
    widgets: &WidgetSnapshot,
    marquee: &mut MarqueeState,
    advance: bool,
    render_scale: f32,
    panel_w: f32,
    panel_h: f32,
) -> bool {
    let s = &dock.config.settings;
    let w = panel_w * render_scale;
    let h = panel_h * render_scale;
    // ----- fondo: el mismo panel del dock, como pastilla redondeada -----
    let path = rounded_rect_path(0.0, 0.0, w, h, s.corner_radius * render_scale);
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
    // ----- sólo el indicador de workspaces, centrado -----
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
        0.0,
        0.0,
        w,
        h,
        render_scale,
        &colors,
        dock.is_vertical(),
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

fn draw_placeholder(pixmap: &mut Pixmap, name: &str, cx: f32, cy: f32, size: f32) {
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
