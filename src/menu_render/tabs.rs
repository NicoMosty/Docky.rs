use super::*;

use crate::menu::{OVERLAY_TABS, band_rect};

/// Geometría de la banda de pestañas: su rect, el largo de cada pestaña y el tamaño
/// de la pastilla. La usan el dibujo y el hit test del click —los dos con los MISMOS
/// números—, porque son los que el reparto de la banda tiene: si se escribieran dos
/// veces, el click caería en otra pestaña que la que se ve resaltada (trampa 10).
pub struct OverlayTabLayout {
    /// `(x, y, w, h)` de la banda dentro del panel.
    pub band: (f32, f32, f32, f32),
    /// Largo de cada pestaña a lo largo de la banda.
    pub slot_len: f32,
    /// Alto/ancho de la pastilla de la pestaña activa.
    pub pill_len: f32,
}

pub fn overlay_tab_layout(
    text_cache: &mut TextCache,
    settings: &crate::config::DockSettings,
    panel_w: f32,
    panel_h: f32,
    scale: f32,
    is_vertical: bool,
    band_left: bool,
) -> OverlayTabLayout {
    let band = band_rect(panel_w, panel_h, is_vertical, band_left);
    let slots = OVERLAY_TABS.len() as f32;
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
        (pill_len + 10.0 * scale).min(band.3 / slots)
    } else {
        band.2 / slots
    };
    OverlayTabLayout {
        band,
        slot_len,
        pill_len,
    }
}

/// Índice de la pestaña bajo un punto en coordenadas del PANEL. `None` fuera de la
/// banda y, en la columna, en el tramo libre que queda debajo de la última pestaña
/// (ahí `slot_len * 4` no llega al alto del panel: ese aire no cambia de modo).
pub fn overlay_tab_at(
    layout: &OverlayTabLayout,
    is_vertical: bool,
    x: f32,
    y: f32,
) -> Option<usize> {
    let (bx, by, bw, bh) = layout.band;
    if x < bx || x > bx + bw || y < by || y > by + bh || layout.slot_len <= 0.0 {
        return None;
    }
    let rel = if is_vertical { y - by } else { x - bx };
    let i = (rel / layout.slot_len).floor();
    if i < 0.0 || i >= OVERLAY_TABS.len() as f32 {
        return None;
    }
    Some(i as usize)
}

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
    let layout = overlay_tab_layout(
        text_cache,
        settings,
        panel_w,
        panel_h,
        scale,
        is_vertical,
        band_left,
    );
    let (bx, by, bw, bh) = layout.band;
    // ----- los slots se reparten a lo LARGO del panel, no a lo ancho: la fila se
    // divide a lo ancho (5 columnas de 26 de alto) y la columna a lo alto (5 filas
    // de 26 de ancho). Confundir el eje los apila en 26px -----
    let acc = accent(settings);
    let on_accent = on_accent_hex(settings);
    let dim = text_dim_hex(settings);
    let pill_len = layout.pill_len;
    let slot_len = layout.slot_len;

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
    use super::{OverlayTabLayout, overlay_tab_at};
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
        assert!(
            slot > OVERLAY_TABS_W,
            "el slot ({slot}) tiene que ser mas largo que el grosor ({OVERLAY_TABS_W})"
        );
        // ----- fila (panel ancho): 4 slots a lo ancho -----
        let (_, _, bw, bh) = band_rect(640.0, 236.0, false, true);
        assert_eq!(bh, OVERLAY_TABS_H);
        // ----- y en la fila tiene que alcanzar para la etiqueta más larga
        // ("Wallpapers", ~74) más su padding: con 5 pestañas en 640 son 128 -----
        let slot_w = bw / OVERLAY_TABS.len() as f32;
        assert!(
            slot_w > 84.0,
            "el slot ({slot_w}) no alcanza para la etiqueta más larga"
        );
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

    /// El click en la banda tiene que caer en la pestaña que el dibujo pinta en ese
    /// punto: las dos cuentas salen de `overlay_tab_layout` (el dibujo la usa para el
    /// `slot_center` y el hit test para el índice), así que acá se fija el reparto de
    /// `overlay_tab_at` que es lo único que se puede despegar.
    #[test]
    fn el_click_de_la_banda_cae_en_la_pestana_dibujada() {
        let n = OVERLAY_TABS.len();
        // ----- fila (panel ancho): 640 entre 5 = 128 por pestaña -----
        let fila = OverlayTabLayout {
            band: (0.0, 0.0, 640.0, OVERLAY_TABS_H),
            slot_len: 128.0,
            pill_len: 90.0,
        };
        for i in 0..n {
            let centro = (i as f32 + 0.5) * 128.0;
            assert_eq!(
                overlay_tab_at(&fila, false, centro, 13.0),
                Some(i),
                "el centro del slot {i} tiene que dar su índice"
            );
        }
        // ----- los bordes: el primero cae en 0 y el último en el 4º, no en el 5º ----
        assert_eq!(overlay_tab_at(&fila, false, 0.0, 13.0), Some(0));
        assert_eq!(overlay_tab_at(&fila, false, 639.0, 13.0), Some(n - 1));
        assert_eq!(overlay_tab_at(&fila, false, 128.0, 13.0), Some(1));
        // ----- fuera de la banda: abajo (el contenido) y a los costados -----
        assert_eq!(overlay_tab_at(&fila, false, 300.0, 30.0), None);
        assert_eq!(overlay_tab_at(&fila, false, -1.0, 13.0), None);
        // ----- el borde derecho exacto ya no da pestaña (cae en el slot 5, que no
        // existe): la banda llega hasta 640 exclusive -----
        assert_eq!(overlay_tab_at(&fila, false, 640.0, 13.0), None);
        assert_eq!(overlay_tab_at(&fila, false, 641.0, 13.0), None);

        // ----- columna (panel vertical con el dock a la izquierda): slots de 84 a lo
        // alto, y el aire que sobra abajo del último NO cambia de modo -----
        let col = OverlayTabLayout {
            band: (0.0, 0.0, OVERLAY_TABS_W, 640.0),
            slot_len: 84.0,
            pill_len: 74.0,
        };
        for i in 0..n {
            let centro = (i as f32 + 0.5) * 84.0;
            assert_eq!(
                overlay_tab_at(&col, true, 13.0, centro),
                Some(i),
                "el centro del slot {i} de la columna tiene que dar su índice"
            );
        }
        assert_eq!(overlay_tab_at(&col, true, 13.0, 84.0), Some(1));
        assert_eq!(
            overlay_tab_at(&col, true, 13.0, n as f32 * 84.0),
            None,
            "debajo de la última pestaña la columna no cambia de modo"
        );
        assert_eq!(overlay_tab_at(&col, true, 13.0, 639.0), None);
        // ----- y a la derecha de la columna ya es contenido -----
        assert_eq!(overlay_tab_at(&col, true, OVERLAY_TABS_W + 1.0, 42.0), None);

        // ----- con el dock a la derecha la banda arranca corrida: el hit test resta
        // el origen de la banda, no asume 0 -----
        let derecha = OverlayTabLayout {
            band: (196.0 - OVERLAY_TABS_W, 0.0, OVERLAY_TABS_W, 640.0),
            slot_len: 84.0,
            pill_len: 74.0,
        };
        assert_eq!(overlay_tab_at(&derecha, true, 183.0, 42.0), Some(0));
        assert_eq!(overlay_tab_at(&derecha, true, 100.0, 42.0), None);
    }
}
