use std::collections::HashMap;

use tiny_skia::{Pixmap, PixmapPaint, Transform};

use crate::clipboard::{ClipboardEntry, ScaledPreview};
use crate::config::DockSettings;
use dockyrs_canvas::TextCache;

use super::{
    draw_body, draw_search_field, draw_text, fill_rrect, menu_radius, panel_bg, rounded_rect_path,
    stroke_menu_border, text_dim_hex, text_hex,
};

pub const CLIP_ROW_H: f32 = 54.0;
/// Inset del contenido dentro de la pastilla de una fila. La pastilla arranca en
/// `CLIP_PAD` (= el inset de la caja de búsqueda) y el contenido va 6px adentro.
const ROW_INSET: f32 = 6.0;
/// Encabezado = lo que corre al contenido: el padding de arriba, la caja de
/// búsqueda (la misma altura que en el launcher) y el de abajo.
pub const CLIP_HEADER_H: f32 = crate::menu::MENU_PADDING * 2.0 + crate::menu::SEARCH_BOX_HEIGHT;
pub const CLIP_PAD: f32 = 10.0;
pub const CLIP_VISIBLE_ROWS: usize = 7;

/// Alto del CONTENIDO del portapapeles: el encabezado (caja de búsqueda) más las
/// filas visibles. La banda de pestañas no entra: vive fuera del frame.
pub fn clip_content_h() -> f32 {
    CLIP_HEADER_H + CLIP_ROW_H * CLIP_VISIBLE_ROWS as f32 + CLIP_PAD
}

/// Dónde arranca la primera fila, en coordenadas del PANEL. La usan el dibujo y
/// el hit test, así que se mueven juntos.
pub fn clip_content_y(frame: crate::menu::PanelFrame) -> f32 {
    frame.y + CLIP_HEADER_H
}

pub struct ClipArgs<'a> {
    pub settings: &'a DockSettings,
    pub entries: &'a [ClipboardEntry],
    pub filtered: &'a [usize],
    pub previews: &'a mut HashMap<usize, ScaledPreview>,
    pub query: &'a str,
    pub selected: usize,
    pub hovered: Option<usize>,
    pub scroll_y: f32,
    pub render_scale: f32,
    /// Caja del CONTENIDO del panel: el panel menos la banda de pestañas. El
    /// tamaño del panel sale de acá (`menu::panel_size`).
    pub frame: crate::menu::PanelFrame,
    pub overlay_tabs: Option<usize>,
    /// Panel vertical del overlay (`Left`/`Right`): la banda es una columna al
    /// costado del dock.
    pub is_vertical: bool,
    /// Ver `DrawArgs::slide_offset` / `body_opacity`.
    pub slide_offset: f32,
    pub body_opacity: f32,
}

#[allow(clippy::too_many_arguments)]
pub fn draw_clipboard(pixmap: &mut Pixmap, text_cache: &mut TextCache, args: ClipArgs) {
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

    // ----- banda de pestañas: fila arriba en el panel ancho, columna al costado
    // del dock (etiquetas rotadas) en el vertical -----
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

    // ----- search box: vive en el encabezado, o sea arriba de la primera fila.
    // Ojo, `clip_content_y()` es la PRIMERA fila, no el encabezado -----
    let top = clip_content_y(frame);
    let placeholder = args.query.is_empty();
    // ----- el cuerpo (caja de búsqueda + filas) se corre en un cambio de
    // pestaña; la banda y la barra de scroll quedan fijas. El parámetro de la
    // closure se llama `pixmap` a propósito: adentro se dibuja en el destino. -----
    let accent = super::accent(settings);
    draw_body(pixmap, args.slide_offset * s, args.body_opacity, |pixmap| {
        draw_search_field(
            pixmap,
            text_cache,
            settings,
            (frame.x + CLIP_PAD) * s,
            (frame.y + MENU_PADDING) * s,
            (frame.w - CLIP_PAD * 2.0) * s,
            SEARCH_BOX_HEIGHT * s,
            if placeholder {
                "Search clipboard\u{2026}"
            } else {
                args.query
            },
            placeholder,
            false,
            s,
        );

        if args.filtered.is_empty() {
            let msg = if args.query.is_empty() {
                "Clipboard is empty"
            } else {
                "No matching clipboard items"
            };
            draw_text(
                pixmap,
                text_cache,
                msg,
                (frame.x + CLIP_PAD) * s,
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

        let start = (args.scroll_y / CLIP_ROW_H).floor().max(0.0) as usize;
        let end = (start + CLIP_VISIBLE_ROWS + 1).min(args.filtered.len());
        for pos in start..end {
            let entry_index = args.filtered[pos];
            let Some(entry) = args.entries.get(entry_index) else {
                continue;
            };
            let row_y = (top + pos as f32 * CLIP_ROW_H - args.scroll_y) * s;
            if row_y + CLIP_ROW_H * s < top * s || row_y > h {
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
                    (frame.x + CLIP_PAD) * s,
                    row_y + 3.0 * s,
                    (frame.w - CLIP_PAD * 2.0) * s,
                    (CLIP_ROW_H - 6.0) * s,
                    OVERLAY_RADIUS * s,
                    fill,
                );
            }

            if let Some(thumbnail) = entry.thumbnail.clone() {
                let px = (frame.x + CLIP_PAD + ROW_INSET) * s;
                let py = row_y + 6.0 * s;
                let pw = (frame.w - (CLIP_PAD + ROW_INSET) * 2.0) * s;
                let ph = (CLIP_ROW_H - 12.0) * s;
                let want_w = pw.round().max(1.0) as u32;
                let want_h = ph.round().max(1.0) as u32;
                let needs = args
                    .previews
                    .get(&entry_index)
                    .map(|p| p.width != want_w || p.height != want_h)
                    .unwrap_or(true);
                if needs && let Some(scaled) = thumbnail.scale_to(want_w, want_h) {
                    args.previews.insert(entry_index, scaled);
                }
                fill_rrect(pixmap, px, py, pw, ph, 4.0 * s, (0, 0, 0, 90));
                if let Some(preview) = args.previews.get(&entry_index) {
                    blit_preview(pixmap, preview, px, py);
                }
            } else {
                let icon_x = (frame.x + CLIP_PAD + ROW_INSET) * s;
                fill_rrect(
                    pixmap,
                    icon_x,
                    row_y + 10.0 * s,
                    34.0 * s,
                    34.0 * s,
                    OVERLAY_RADIUS * s,
                    super::track_bg(settings),
                );
                draw_text(
                    pixmap,
                    text_cache,
                    "T",
                    icon_x + 11.0 * s,
                    row_y + 15.0 * s,
                    15.0 * s,
                    if selected { &on_accent } else { &dim_c },
                    700,
                );

                // ----- la misma relación que la miniatura: caja en CLIP_PAD+ROW_INSET,
                // texto 12px a la derecha de la caja (los 34 del icono) -----
                let text_x = frame.x + CLIP_PAD + ROW_INSET + 46.0;
                let text_w = frame.w - (CLIP_PAD + ROW_INSET + 46.0) - (CLIP_PAD + ROW_INSET);
                let title = fit(entry.title.trim(), &entry.title, 11.0, text_w);
                let desc = fit(&entry.description, &entry.description, 8.0, text_w);
                let tcol = if selected { &on_accent } else { &title_c };
                let dcol = if selected { &on_accent } else { &dim_c };
                draw_text(
                    pixmap,
                    text_cache,
                    &title,
                    text_x * s,
                    row_y + 12.0 * s,
                    11.0 * s,
                    tcol,
                    600,
                );
                draw_text(
                    pixmap,
                    text_cache,
                    &desc,
                    text_x * s,
                    row_y + 30.0 * s,
                    8.0 * s,
                    dcol,
                    400,
                );
            }
        }
    });

    // ----- scrollbar -----
    let total = args.filtered.len() as f32 * CLIP_ROW_H;
    let viewport = CLIP_ROW_H * CLIP_VISIBLE_ROWS as f32;
    if total > viewport {
        let track_x = (frame.x + frame.w - 4.0) * s;
        let track_h = h - top * s - CLIP_PAD * s;
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

fn blit_preview(pixmap: &mut Pixmap, preview: &ScaledPreview, x: f32, y: f32) {
    let mut src = match Pixmap::new(preview.width, preview.height) {
        Some(p) => p,
        None => return,
    };
    let data = src.data_mut();
    for (dst, chunk) in data.chunks_exact_mut(4).zip(preview.pixels.chunks_exact(4)) {
        let a = chunk[3] as u16;
        dst[0] = (chunk[0] as u16 * a / 255) as u8;
        dst[1] = (chunk[1] as u16 * a / 255) as u8;
        dst[2] = (chunk[2] as u16 * a / 255) as u8;
        dst[3] = chunk[3];
    }
    pixmap.draw_pixmap(
        0,
        0,
        src.as_ref(),
        &PixmapPaint::default(),
        Transform::from_translate(x, y),
        None,
    );
}

fn fit(display: &str, _full: &str, size: f32, max_w: f32) -> String {
    let approx = (max_w / (size * 0.55)) as usize;
    if display.chars().count() <= approx {
        return display.to_string();
    }
    let mut out: String = display.chars().take(approx.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod clip_vertical_tests {
    use super::*;

    /// En el dock vertical (`Left`/`Right`) la banda de pestañas va apilada y el
    /// El alto del contenido no depende de la orientación (son las mismas filas y
    /// el mismo encabezado) y la banda de pestañas queda FUERA del frame: en el
    /// panel ancho es una fila arriba y en el vertical una columna al costado del
    /// dock, así que ya no le come alto al panel.
    #[test]
    fn el_contenido_mide_lo_mismo_en_las_dos_orientaciones() {
        assert_eq!(
            clip_content_h(),
            CLIP_HEADER_H + CLIP_ROW_H * CLIP_VISIBLE_ROWS as f32 + CLIP_PAD
        );
        // ----- la primera fila arranca en el frame, no en la banda -----
        for (band_left, esperado) in [(true, crate::menu::OVERLAY_TABS_W), (false, 0.0)] {
            let frame = crate::menu::frame_for(
                crate::menu::OVERLAY_PANEL_VERTICAL_W,
                clip_content_h(),
                true,
                band_left,
            );
            assert_eq!(frame.x, esperado);
            assert_eq!(clip_content_y(frame), frame.y + CLIP_HEADER_H);
        }
        let ancho =
            crate::menu::frame_for(crate::menu::OVERLAY_PANEL_W, clip_content_h(), false, true);
        assert_eq!(
            clip_content_y(ancho),
            crate::menu::OVERLAY_TABS_H + CLIP_HEADER_H
        );
    }
}
