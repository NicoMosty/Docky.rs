use super::*;

use crate::menu::{OVERLAY_TABS, band_rect};

/// La banda de pestañas de los modos del overlay. Es la única señal de en qué
/// mini-app estás. En el panel ancho es una fila de 26 arriba; en el vertical (el
/// panel es una columna) es una columna de 26 pegada al lado del dock, con las
/// etiquetas rotadas 90° como los widgets de la barra vertical: apilada arriba le
/// comía 104 de alto y el panel quedaba desproporcionado.
///
/// El rect de la banda y el origen del contenido salen los dos de
/// `menu::band_rect`/`menu::PanelFrame`, así que el dibujo no puede quedar corrido
/// respecto del reparto (trampa 10).
#[allow(clippy::too_many_arguments)]
pub fn draw_overlay_tabs(
    pixmap: &mut Pixmap,
    text_cache: &mut TextCache,
    settings: &crate::config::DockSettings,
    panel_w: f32,
    panel_h: f32,
    scale: f32,
    index: usize,
    is_vertical: bool,
    band_left: bool,
) {
    let (bx, by, bw, bh) = band_rect(panel_w, panel_h, is_vertical, band_left);
    // ----- los slots se reparten a lo LARGO del panel, no a lo ancho: la fila se
    // divide a lo ancho (4 columnas de 26 de alto) y la columna a lo alto (4 filas
    // de 26 de ancho). Confundir el eje los apila en 26px -----
    let slots = OVERLAY_TABS.len() as f32;
    let acc = accent(settings);
    let on_accent = on_accent_hex(settings);
    let dim = text_dim_hex(settings);
    // ----- la pastilla es del MISMO tamaño en las cuatro pestañas: con el largo del
    // texto cada una medía distinto (Apps corta, Wallpapers larga) y la banda se veía
    // despareja. Se mide con el peso de la elegida (700) para que la activa entre
    // siempre. -----
    let pill_text = OVERLAY_TABS
        .iter()
        .filter_map(|label| {
            text_cache
                .get(label, 9.5 * scale, &dim, 700)
                .map(|g| g.width() as f32)
        })
        .fold(0.0f32, f32::max);
    let pill_len = (pill_text + 20.0 * scale).max(6.0 * scale);
    // ----- y la pestaña mide lo mismo SIEMPRE: en la columna el alto sale del texto
    // más largo, no del alto del panel. Con `bh / 4` cada panel abría con pestañas de
    // otro alto (139 en el launcher, 108 en el portapapeles, 160 en los fondos) y al
    // cambiar de pestaña la banda saltaba. El `.min(bh / slots)` es sólo el piso para
    // un panel más bajo que las cuatro pestañas juntas. -----
    let slot_len = if is_vertical {
        (pill_len + 10.0 * scale).min(bh / slots)
    } else {
        bw / slots
    };

    for (i, label) in OVERLAY_TABS.iter().enumerate() {
        let selected = i == index;
        let color = if selected { &on_accent } else { &dim };
        let weight = if selected { 700 } else { 500 };
        let Some(glyphs) = text_cache.get(label, 9.5 * scale, color, weight) else {
            continue;
        };
        let slot_center = if is_vertical {
            by + (i as f32 + 0.5) * slot_len
        } else {
            bx + (i as f32 + 0.5) * slot_len
        };
        if selected {
            if is_vertical {
                fill_rrect(
                    pixmap,
                    (bx + 3.0) * scale,
                    slot_center * scale - pill_len / 2.0,
                    (bw - 6.0) * scale,
                    pill_len,
                    crate::menu::OVERLAY_RADIUS * scale,
                    acc,
                );
            } else {
                fill_rrect(
                    pixmap,
                    slot_center * scale - pill_len / 2.0,
                    (by + 3.0) * scale,
                    pill_len,
                    (bh - 6.0) * scale,
                    crate::menu::OVERLAY_RADIUS * scale,
                    acc,
                );
            }
        }
        if is_vertical {
            // ----- rotada: la usan el OSD y los widgets de la barra vertical, así
            // que se lee igual (de abajo hacia arriba) -----
            super::osd::draw_text_rotated(
                pixmap,
                text_cache,
                label,
                (bx + bw * 0.5) * scale,
                slot_center * scale,
                9.5 * scale,
                color,
                weight,
            );
        } else {
            let paint = tiny_skia::PixmapPaint::default();
            pixmap.draw_pixmap(
                0,
                0,
                glyphs.as_ref().as_ref(),
                &paint,
                Transform::from_translate(
                    slot_center * scale - glyphs.width() as f32 / 2.0,
                    (by + bh * 0.5) * scale - glyphs.height() as f32 / 2.0,
                ),
                None,
            );
        }
    }

    // ----- hairline del lado del contenido -----
    let mut divider = Paint::default();
    divider.set_color_rgba8(HAIRLINE.0, HAIRLINE.1, HAIRLINE.2, HAIRLINE.3);
    let rect = if is_vertical {
        let x = if band_left { bx + bw } else { bx };
        Rect::from_xywh((x - scale / 2.0) * scale, 0.0, scale, panel_h * scale)
    } else {
        Rect::from_xywh(
            0.0,
            ((by + bh) * scale - scale).max(0.0),
            panel_w * scale,
            scale,
        )
    };
    if let Some(r) = rect {
        pixmap.fill_rect(r, &divider, Transform::identity(), None);
    }
}

#[cfg(test)]
mod tabs_tests {
    use crate::menu::{OVERLAY_TABS, OVERLAY_TABS_H, OVERLAY_TABS_W, band_rect, frame_for};

    /// La banda tiene que estar exactamente fuera del frame del contenido: si no,
    /// tapa la primera fila o deja un hueco. Las dos cuentas salen de `menu::`.
    #[test]
    fn la_banda_esta_fuera_del_frame() {
        // ----- panel ancho: fila de 26 arriba, contenido debajo -----
        let (x, y, w, h) = band_rect(640.0, 236.0, false, true);
        assert_eq!((x, y, w, h), (0.0, 0.0, 640.0, OVERLAY_TABS_H));
        let f = frame_for(640.0, 236.0 - OVERLAY_TABS_H, false, true);
        assert_eq!(f.y, y + h, "el contenido arranca donde termina la banda");

        // ----- panel vertical con el dock a la izquierda: columna de 26 a la
        // izquierda, contenido a la derecha -----
        let (x, y, w, h) = band_rect(196.0, 640.0, true, true);
        assert_eq!((x, y, w, h), (0.0, 0.0, OVERLAY_TABS_W, 640.0));
        let f = frame_for(196.0 - OVERLAY_TABS_W, 640.0, true, true);
        assert_eq!(f.x, x + w, "el contenido arranca donde termina la banda");
        assert_eq!(f.y, 0.0, "en el vertical la banda no come alto");

        // ----- y con el dock a la derecha la columna va del otro lado -----
        let (x, _, w, _) = band_rect(196.0, 640.0, true, false);
        assert_eq!((x, w), (196.0 - OVERLAY_TABS_W, OVERLAY_TABS_W));
        assert_eq!(frame_for(196.0 - OVERLAY_TABS_W, 640.0, true, false).x, 0.0);
    }

    /// Cada pestaña tiene que caer en su slot: sin eso las cuatro se apilan en los
    /// 26px del grosor de la banda (fue el bug de la primera versión de la columna).
    #[test]
    fn las_pestanas_se_reparten_a_lo_largo_del_panel() {
        // ----- columna (panel vertical de 196x640): 4 slots de 160 de alto -----
        let (_, _, bw, bh) = band_rect(196.0, 640.0, true, true);
        assert_eq!(bw, OVERLAY_TABS_W);
        let slot = bh / OVERLAY_TABS.len() as f32;
        assert_eq!(slot, 160.0);
        assert!(
            slot > OVERLAY_TABS_W,
            "el slot tiene que ser mas largo que ancho"
        );
        // ----- fila (panel ancho): 4 slots a lo ancho -----
        let (_, _, bw, bh) = band_rect(640.0, 236.0, false, true);
        assert_eq!(bh, OVERLAY_TABS_H);
        assert_eq!(bw / OVERLAY_TABS.len() as f32, 160.0);
    }

    /// La pastilla activa mide igual en las cuatro pestañas: sale del texto MÁS
    /// LARGO, no del de la que está seleccionada (si sale del texto de cada una, la
    /// banda cambia de tamaño al ciclar, que es lo que se veía desparejo).
    #[test]
    fn la_pastilla_mide_igual_en_todas_las_pestanas() {
        let mas_larga = OVERLAY_TABS
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0);
        assert!(
            mas_larga >= 10,
            "la etiqueta mas larga tiene {mas_larga} chars"
        );
        // ----- el slot de la fila tiene que dar lugar al texto más largo + padding:
        // a 9.5px por char el ancho real de la fuente es menor que este techo -----
        let slot_fila = 640.0 / OVERLAY_TABS.len() as f32;
        assert!(
            mas_larga as f32 * 9.5 + 20.0 < slot_fila,
            "no entra en el slot"
        );
        // ----- y en la columna el slot es el alto del panel entre 4, siempre más
        // largo que el texto (640/4 = 160 contra ~95) -----
        let slot_columna = 640.0 / OVERLAY_TABS.len() as f32;
        assert!(mas_larga as f32 * 9.5 + 20.0 < slot_columna);
    }
}
