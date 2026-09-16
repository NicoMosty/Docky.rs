use super::*;

use crate::menu::{OVERLAY_TABS, overlay_tabs_h};

/// La banda de pestañas de los modos del overlay. Va arriba del contenido en los
/// tres paneles (el portapapeles incluido) y es la única señal de en qué mini-app
/// estás. `index` es la posición en `OVERLAY_TABS`; `stacked` es el panel vertical
/// angosto, donde las cuatro etiquetas no entran en una fila y van apiladas.
pub fn draw_overlay_tabs(
    pixmap: &mut Pixmap,
    text_cache: &mut TextCache,
    settings: &crate::config::DockSettings,
    panel_w: f32,
    scale: f32,
    index: usize,
    stacked: bool,
) {
    let w = panel_w * scale;
    let band = overlay_tabs_h(stacked) * scale;
    let acc = accent(settings);
    let on_accent = on_accent_hex(settings);
    let dim = text_dim_hex(settings);
    let (slot_w, slot_h) = if stacked {
        (w, band / OVERLAY_TABS.len() as f32)
    } else {
        (w / OVERLAY_TABS.len() as f32, band)
    };

    for (i, label) in OVERLAY_TABS.iter().enumerate() {
        let selected = i == index;
        let slot_y = if stacked { i as f32 * slot_h } else { 0.0 };
        let color = if selected { &on_accent } else { &dim };
        let Some(glyphs) =
            text_cache.get(label, 9.5 * scale, color, if selected { 700 } else { 500 })
        else {
            continue;
        };
        let slot_x = if stacked { 0.0 } else { i as f32 * slot_w };
        let text_w = glyphs.width() as f32;
        let pill_w = (text_w + 20.0 * scale).min(slot_w);
        if selected {
            fill_rrect(
                pixmap,
                slot_x + (slot_w - pill_w) / 2.0,
                slot_y + 3.0 * scale,
                pill_w,
                (slot_h - 6.0 * scale).max(1.0),
                crate::menu::OVERLAY_RADIUS * scale,
                acc,
            );
        }
        let paint = tiny_skia::PixmapPaint::default();
        pixmap.draw_pixmap(
            0,
            0,
            glyphs.as_ref().as_ref(),
            &paint,
            Transform::from_translate(
                slot_x + (slot_w - text_w) / 2.0,
                slot_y + centered_text_y(slot_h / scale, 9.5) * scale,
            ),
            None,
        );
    }

    let mut divider = Paint::default();
    divider.set_color_rgba8(HAIRLINE.0, HAIRLINE.1, HAIRLINE.2, HAIRLINE.3);
    if let Some(r) = Rect::from_xywh(0.0, (band - scale).max(0.0), w, scale) {
        pixmap.fill_rect(r, &divider, Transform::identity(), None);
    }
}

#[cfg(test)]
mod tabs_tests {
    use crate::menu::{OVERLAY_TABS, overlay_tabs_h};

    /// La banda tiene que correr el contenido exactamente lo que ocupa: si no,
    /// tapa la primera fila (o deja un hueco).
    #[test]
    fn la_banda_ocupa_lo_que_corre_al_contenido() {
        assert_eq!(overlay_tabs_h(false), crate::menu::OVERLAY_TABS_H);
        assert_eq!(
            overlay_tabs_h(true),
            crate::menu::OVERLAY_TABS_H * OVERLAY_TABS.len() as f32
        );
    }
}
