//! El panel de atajos de niri: una fila por bind, con las TECLAS a la izquierda y qué
//! hace a la derecha. Es un panel del overlay más (como el portapapeles), así que comparte
//! el encabezado con la caja de búsqueda y la banda de pestañas.
//!
//! La geometría vive acá y la usan el dibujo y los hit tests (trampa 10): una sola cuenta
//! para la fila, las filas visibles y dónde arranca el contenido.

use tiny_skia::{Pixmap, Transform};

use crate::config::DockSettings;
use crate::desktop::Keybind;
use dockyrs_canvas::TextCache;

use super::{
    accent, draw_body, draw_search_field, draw_text, fill_rrect, menu_radius, panel_bg,
    rounded_rect_path, stroke_menu_border, text_dim_hex, text_hex,
};

pub const KEYBIND_ROW_H: f32 = 28.0;
/// Ancho LÓGICO de la columna de las teclas. Las más largas de una config real llegan a
/// ~30 caracteres (`Mod+Ctrl+Shift+Page_Down`), que a 9 px entran en 210.
pub const KEYBIND_KEYS_W: f32 = 210.0;
/// Filas del panel ANCHO. En el vertical salen del alto del frame (que es el común del
/// overlay), como en el portapapeles.
pub const KEYBIND_VISIBLE_ROWS: usize = 12;
pub const KEYBIND_PAD: f32 = 10.0;

/// El encabezado es el MISMO que el del portapapeles: padding + caja de búsqueda + padding.
use super::CLIP_HEADER_H as KEYBIND_HEADER_H;

/// Filas que entran en el panel. Única cuenta: la usan el rango que dibuja, el scrollbar y
/// el clamp del scroll.
pub fn keybind_visible_rows(frame: crate::menu::PanelFrame) -> usize {
    (((frame.h - KEYBIND_HEADER_H - KEYBIND_PAD) / KEYBIND_ROW_H).floor()).max(1.0) as usize
}

/// Alto del CONTENIDO en el panel ancho: encabezado + filas.
pub fn keybind_content_h() -> f32 {
    KEYBIND_HEADER_H + KEYBIND_ROW_H * KEYBIND_VISIBLE_ROWS as f32 + KEYBIND_PAD
}

/// Dónde arranca la primera fila, en coordenadas del PANEL. La usan el dibujo y el hit test.
pub fn keybind_content_y(frame: crate::menu::PanelFrame) -> f32 {
    frame.y + KEYBIND_HEADER_H
}

/// Rect de la fila `pos`, en coordenadas del panel y con el scroll aplicado.
pub fn keybind_row_rect(
    frame: crate::menu::PanelFrame,
    pos: usize,
    scroll_y: f32,
) -> (f32, f32, f32, f32) {
    let top = keybind_content_y(frame) - scroll_y;
    (
        frame.x + KEYBIND_PAD,
        top + pos as f32 * KEYBIND_ROW_H,
        frame.w - KEYBIND_PAD * 2.0,
        KEYBIND_ROW_H,
    )
}

pub struct KeybindArgs<'a> {
    pub settings: &'a DockSettings,
    pub keybinds: &'a [Keybind],
    pub filtered: &'a [usize],
    pub query: &'a str,
    pub selected: usize,
    pub hovered: Option<usize>,
    pub scroll_y: f32,
    pub render_scale: f32,
    pub frame: crate::menu::PanelFrame,
    pub overlay_tabs: Option<usize>,
    pub is_vertical: bool,
    pub slide_offset: f32,
    pub body_opacity: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_keybinds(pixmap: &mut Pixmap, text_cache: &mut TextCache, args: KeybindArgs) {
    use crate::menu::{MENU_PADDING, OVERLAY_RADIUS, SEARCH_BOX_HEIGHT};
    let s = args.render_scale;
    let settings = args.settings;
    let frame = args.frame;
    let (panel_w, panel_h) = crate::menu::panel_size(frame, args.is_vertical);
    let w = panel_w * s;
    let h = panel_h * s;

    let bg = panel_bg(settings);
    let path = rounded_rect_path(0.0, 0.0, w, h, menu_radius(settings, s));
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(bg.0, bg.1, bg.2, bg.3);
    paint.anti_alias = true;
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        Transform::identity(),
        None,
    );
    stroke_menu_border(pixmap, &path, settings, s);

    if let Some(index) = args.overlay_tabs {
        super::draw_overlay_tabs(
            pixmap,
            text_cache,
            settings,
            panel_w,
            panel_h,
            s,
            index,
            args.is_vertical,
            frame.x > 0.0,
        );
    }

    let top = keybind_content_y(frame);
    let placeholder = args.query.is_empty();
    let accent = accent(settings);
    draw_body(pixmap, args.slide_offset * s, args.body_opacity, |pixmap| {
        draw_search_field(
            pixmap,
            text_cache,
            settings,
            (frame.x + KEYBIND_PAD) * s,
            (frame.y + MENU_PADDING) * s,
            (frame.w - KEYBIND_PAD * 2.0) * s,
            SEARCH_BOX_HEIGHT * s,
            if placeholder {
                "Search keybinds\u{2026}"
            } else {
                args.query
            },
            placeholder,
            false,
            s,
        );

        if args.filtered.is_empty() {
            let msg = if args.query.is_empty() {
                "No keybinds found"
            } else {
                "No matching keybinds"
            };
            draw_text(
                pixmap,
                text_cache,
                msg,
                (frame.x + KEYBIND_PAD) * s,
                (top + 24.0) * s,
                10.0 * s,
                &text_dim_hex(settings),
                400,
            );
            return;
        }

        let title_c = text_hex(settings);
        let dim_c = text_dim_hex(settings);
        let on_accent = super::on_accent_hex(settings);
        let start = (args.scroll_y / KEYBIND_ROW_H).floor().max(0.0) as usize;
        let rows = keybind_visible_rows(frame);
        // ----- +1: la fila que no entra queda asomando como pista de que hay más -----
        let end = (start + rows + 1).min(args.filtered.len());
        for pos in start..end {
            let Some(kb) = args.filtered.get(pos).and_then(|i| args.keybinds.get(*i)) else {
                continue;
            };
            let (rx, ry, rw, rh) = keybind_row_rect(frame, pos, args.scroll_y);
            let ry = ry * s;
            if ry + rh * s < top * s || ry > h {
                continue;
            }
            let selected = pos == args.selected;
            let hovered = args.hovered == Some(pos);
            if selected || hovered {
                let fill = if selected {
                    accent
                } else {
                    (255, 255, 255, 22)
                };
                fill_rrect(
                    pixmap,
                    rx * s,
                    ry + 2.0 * s,
                    rw * s,
                    (KEYBIND_ROW_H - 4.0) * s,
                    OVERLAY_RADIUS * s,
                    fill,
                );
            }
            // ----- las TECLAS, en su columna: pastilla de acento suave, texto centrado -----
            let keys_bg = if selected {
                (0, 0, 0, 46)
            } else {
                (accent.0, accent.1, accent.2, 40)
            };
            fill_rrect(
                pixmap,
                (rx + 4.0) * s,
                ry + 4.0 * s,
                (KEYBIND_KEYS_W - 8.0) * s,
                (KEYBIND_ROW_H - 8.0) * s,
                OVERLAY_RADIUS * s,
                keys_bg,
            );
            let keys_c = if selected { &on_accent } else { &title_c };
            draw_text(
                pixmap,
                text_cache,
                &kb.keys,
                (rx + 12.0) * s,
                ry + 8.0 * s,
                10.0 * s,
                keys_c,
                700,
            );
            // ----- qué hace: el resto del ancho, elidido -----
            let label_x = rx + KEYBIND_KEYS_W + 8.0;
            let label_w = (rx + rw) - label_x - 6.0;
            let label = super::clipboard::fit(&kb.label, &kb.label, 9.0, label_w);
            draw_text(
                pixmap,
                text_cache,
                &label,
                label_x * s,
                ry + 9.0 * s,
                9.0 * s,
                if selected { &on_accent } else { &dim_c },
                400,
            );
        }
    });

    // ----- scrollbar -----
    let total = args.filtered.len() as f32 * KEYBIND_ROW_H;
    let viewport = KEYBIND_ROW_H * keybind_visible_rows(args.frame) as f32;
    if total > viewport {
        let track_x = (frame.x + frame.w - 4.0) * s;
        let track_h = h - top * s - KEYBIND_PAD * s;
        let thumb_h = (track_h * (viewport / total)).max(20.0 * s);
        let max_scroll = total - viewport;
        let thumb_y = top * s + (track_h - thumb_h) * (args.scroll_y / max_scroll).clamp(0.0, 1.0);
        fill_rrect(
            pixmap,
            track_x,
            thumb_y,
            3.0 * s,
            thumb_h,
            1.5 * s,
            (accent.0, accent.1, accent.2, 150),
        );
    }
}
